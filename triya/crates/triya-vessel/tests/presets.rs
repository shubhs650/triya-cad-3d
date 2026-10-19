// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T4.4 acceptance: an attachment's `standard_ref` + `size_key` take the dimensions of a preset
//! row. Importing a fictional private table (`vessel.import_tables`) changes the generated
//! saddle; the shipped proportional presets (`OD<mm>`) fill a bare saddle; the repository's
//! tables carry no EIL (or other company) values.

use serde_json::{Value, json};
use solidcraft_engine::Session;

const PRIVATE: &str = include_str!("fixtures/fictional-private-supports.json");

fn vol(s: &Session, name: &str) -> f64 {
    let st = s.model.state();
    let b = st.bodies.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("no body `{name}`"));
    solidcraft_engine::kernel::measure(&b.body).unwrap().volume
}

/// Vessel B with only its first saddle, keyed to the fictional standard.
fn vessel_b_one_saddle(size_key: &str) -> Session {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let mut doc: Value = serde_json::from_str(include_str!("reference/vessel-b.json")).unwrap();
    let mut s1 = doc["attachments"].as_array().unwrap().iter().find(|a| a["id"] == "S1").cloned().unwrap();
    s1["standard_ref"] = json!({"org": "ACME", "number": "FICT-001", "rev": "0"});
    s1["size_key"] = json!(size_key);
    doc["attachments"] = json!([s1]);
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    s
}

#[test]
fn a_private_table_changes_the_generated_saddle() {
    let mut s = vessel_b_one_saddle("FS-1");
    // Before the import: no row, so the saddle's own dimensions, and a warning.
    let r = s.execute("vessel.regenerate", &json!({})).unwrap();
    let w = r["warnings"].as_array().unwrap();
    assert!(w.iter().any(|w| w.as_str().unwrap_or("").contains("no preset `FS-1` for ACME FICT-001:0")), "{w:?}");
    let before = vol(&s, "Vessel S1 base plate");
    let hole = |d: f64, t: f64| 2.0 * std::f64::consts::PI * (d / 2.0).powi(2) * t;
    assert!((before / (1100.0 * 200.0 * 16.0 - hole(24.0 + 6.0, 16.0)) - 1.0).abs() < 1e-4, "{before}");
    // Import the fictional table: the same regenerate builds the table's saddle.
    let table: Value = serde_json::from_str(PRIVATE).unwrap();
    let imp = s.execute("vessel.import_tables", &json!({"tables": {"supports": table}})).unwrap();
    assert_eq!(imp["private_tables"]["supports"]["rows"], 1, "{imp}");
    let r = s.execute("vessel.regenerate", &json!({})).unwrap();
    assert_eq!(r["presets"][0]["row"], "saddle/FS-1", "{r:#}");
    assert_eq!(r["presets"][0]["private"], true);
    let after = vol(&s, "Vessel S1 base plate");
    assert!((after / (1200.0 * 250.0 * 20.0 - hole(30.0 + 6.0, 20.0)) - 1.0).abs() < 1e-4, "{after}");
    // The row says nothing about verification: flagged, and the stored attachment carries it.
    let inspect = s.execute("vessel.inspect", &json!({})).unwrap();
    let flags = inspect["flags"].as_array().unwrap();
    assert!(flags.iter().any(|f| f["id"] == "S1" && f["unverified_table"] == true), "{flags:?}");
    let saddle = inspect["doc"]["attachments"].as_array().unwrap().iter().find(|a| a["id"] == "S1").cloned().unwrap();
    assert_eq!(saddle["height_mm"], 900.0);
    // The import is one undo step of its own (after the regenerate's); `clear` drops it too.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(!s.execute("vessel.inspect", &json!({})).unwrap()["private_tables"].is_null());
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.execute("vessel.inspect", &json!({})).unwrap()["private_tables"].is_null());
    s.execute("edit.redo", &json!({})).unwrap();
    s.execute("vessel.import_tables", &json!({"clear": true})).unwrap();
    assert!(s.execute("vessel.inspect", &json!({})).unwrap()["private_tables"].is_null());
}

#[test]
fn shipped_presets_by_outside_diameter() {
    let mut s = vessel_b_one_saddle("OD1600");
    let r = s.execute("vessel.inspect", &json!({})).unwrap();
    assert_eq!(r["presets"][0]["row"], "saddle/OD1600", "{r:#}");
    assert_eq!(r["presets"][0]["private"], false);
    // A key between rows takes the nearest smaller OD.
    let mut s = vessel_b_one_saddle("OD1650");
    let r = s.execute("vessel.inspect", &json!({})).unwrap();
    assert_eq!(r["presets"][0]["row"], "saddle/OD1600", "{r:#}");
    let r = s.execute("vessel.regenerate", &json!({})).unwrap();
    assert_eq!(r["supports"][0]["built"], "built", "{r:#}");
}

#[test]
fn import_refuses_bad_tables() {
    let mut s = vessel_b_one_saddle("FS-1");
    for bad in [
        json!({}),
        json!({"tables": {}}),
        json!({"tables": {"pipes": {}}}),
        json!({"tables": {"supports": {"source": "x", "rows": [{"kind": "saddle", "size_key": "A", "fields": {"id": "Z"}}]}}}),
        json!({"tables": {"supports": {"source": "x", "rows": [{"kind": "saddle", "size_key": "A", "fields": {"pad": "x".repeat(70_000)}}]}}}),
        json!({"tables": {"supports": {}}, "path": "/nonexistent"}),
        json!({"path": "/nonexistent/tables.json"}),
    ] {
        assert!(s.execute("vessel.import_tables", &bad).is_err(), "{bad}");
    }
    // A row that does not fit the attachment is an error at regenerate, and nothing changes.
    let table = json!({"source": "fictional", "rows": [{"kind": "saddle", "size_key": "FS-1", "fields": {"height_mm": "tall"}}]});
    s.execute("vessel.import_tables", &json!({"tables": {"supports": table}})).unwrap();
    let e = s.execute("vessel.regenerate", &json!({})).unwrap_err().to_string();
    assert!(e.contains("preset") && e.contains("does not fit"), "{e}");
}

#[test]
fn the_repository_ships_no_company_standard_values() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../triya-stdlib/data");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        for word in ["EIL", "Engineers India", "7-12-00"] {
            assert!(!text.contains(word), "{} mentions `{word}`", path.display());
        }
    }
    // The only private table in the tree is the fictional one under tests/.
    assert!(PRIVATE.contains("FICTIONAL") && !PRIVATE.contains("EIL"));
}
