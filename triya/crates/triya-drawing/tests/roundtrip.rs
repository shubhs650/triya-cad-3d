// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T7.3: a regenerated vessel with a GA sheet survives save and open. The saved `.sc` holds
//! native features only (sketches, revolves) plus two metadata entries, so opening it needs no
//! vessel command: the geometry, the vessel JSON and the sheet options come back, and the sheet
//! exports to the same SVG. The browser saves the same bytes to OPFS through upstream's VFS. The
//! check against an unmodified upstream v0.22.0 build is `triya/ci/upstream-roundtrip.sh`.

use serde_json::{Value, json};
use solidcraft_engine::Session;
use triya_drawing::commands::SHEET_KEY;
use triya_vessel::commands::METADATA_KEY;

fn registered() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    solidcraft_engine::cmd::triya_extra::register(triya_drawing::COMMANDS).unwrap();
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn shell_of_vessel_a() -> Value {
    let script: Value = serde_json::from_str(include_str!("../../triya-vessel/examples/vessel-a-shell.json")).unwrap();
    script["commands"][0]["params"]["doc"].clone()
}

fn volumes(s: &mut Session) -> Vec<(String, f64)> {
    let m = run(s, "inspect.measure", json!({}));
    let mut v: Vec<(String, f64)> = m["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["name"].as_str().unwrap_or("").to_string(), b["volume_mm3"].as_f64().unwrap_or(f64::NAN)))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn saved_vessel_opens_with_geometry_vessel_and_sheet() {
    registered();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": shell_of_vessel_a()}));
    run(&mut s, "vessel.regenerate", json!({}));
    run(&mut s, "drawing.generate_ga", json!({"size": "A3", "title_block": {"checked_by": "RT"}}));
    let before = volumes(&mut s);
    assert!(!before.is_empty());
    let svg = run(&mut s, "drawing.export", json!({}))["text"].as_str().unwrap().to_string();

    let dir = std::env::temp_dir().join(format!("triya-roundtrip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, format) in [("vessel-a-shell.sc", "container"), ("vessel-a-shell.solidcraft", "container"), ("vessel-a-shell.json", "json")] {
        let path = dir.join(name).to_string_lossy().to_string();
        run(&mut s, "file.save_as", json!({"path": path, "format": format}));

        let mut o = Session::default();
        run(&mut o, "doc.open", json!({"path": path}));
        let after = volumes(&mut o);
        assert_eq!(after.len(), before.len(), "{format}: bodies");
        for ((n0, v0), (n1, v1)) in before.iter().zip(&after) {
            assert_eq!(n0, n1, "{format}");
            assert!((v0 - v1).abs() <= 1e-6 * v0.abs().max(1.0), "{format} {n0}: {v0} vs {v1}");
        }
        for key in [METADATA_KEY, SHEET_KEY] {
            assert_eq!(o.doc.metadata.get(key), s.doc.metadata.get(key), "{format}: {key}");
        }
        let again = run(&mut o, "drawing.export", json!({}))["text"].as_str().unwrap().to_string();
        assert_eq!(again, svg, "{format}: the sheet exports the same SVG after opening");
        // The vessel is still editable after opening (stored features are found again).
        let inspect = run(&mut o, "vessel.inspect", json!({}));
        assert!(!inspect["generated"].as_array().map(Vec::is_empty).unwrap_or(true), "{format}: {inspect}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
