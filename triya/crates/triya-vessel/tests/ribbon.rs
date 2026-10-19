// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! PLAN T5.1: the ribbon's commands. Each `vessel.*` command sits on one of the VESSEL tab's
//! panels; Add Element / Add Nozzle / Add Support work from the toolbar's `{}` with defaults
//! taken from the stack and the presets; Export BOM writes CSV.

use serde_json::{Value, json};
use solidcraft_engine::Session;
use triya_vessel::commands::{PANELS, TAB};

fn session_with(doc: &str) -> Session {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let doc: Value = serde_json::from_str(doc).unwrap();
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    s
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn find<'a>(doc: &'a Value, list: &str, id: &str) -> &'a Value {
    doc[list].as_array().unwrap().iter().find(|x| x["id"] == id).unwrap_or_else(|| panic!("no {id} in {list}"))
}

#[test]
fn every_vessel_command_is_on_a_vessel_panel() {
    for spec in triya_vessel::COMMANDS {
        assert_eq!(spec.tab, TAB, "{}", spec.id);
        assert!(PANELS.contains(&spec.panel), "{} is on panel {}", spec.id, spec.panel);
    }
    // The ribbon's six groups of the plan: new vessel, elements, attachments, supports,
    // regenerate, export BOM — each a command.
    for id in ["vessel.new", "vessel.element.add", "vessel.attachment.add", "vessel.support.add", "vessel.regenerate", "vessel.bom.export"] {
        assert!(triya_vessel::COMMANDS.iter().any(|c| c.id == id), "{id}");
    }
}

#[test]
fn add_element_goes_before_the_closing_head_and_inherits() {
    let mut s = session_with(include_str!("reference/vessel-c.json"));
    let undo = s.undo.len();
    let r = run(&mut s, "vessel.element.add", json!({}));
    assert_eq!(r["id"], "E4");
    assert_eq!(r["index"], 2);
    let ids: Vec<&str> = r["doc"]["elements"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["E1", "E2", "E4", "E3"]);
    let e4 = find(&r["doc"], "elements", "E4");
    assert_eq!(e4["kind"], "cylinder");
    assert_eq!(e4["thickness_mm"], 8.0, "the neighbour's wall");
    assert!(e4.get("diameter_mm").is_none(), "the diameter is inherited");
    assert_eq!(s.undo.len(), undo + 1, "one undo step");
    // A head at the end, and a cone after a given element with fields of its own.
    let r = run(&mut s, "vessel.element.add", json!({"kind": "cone", "after": "E2", "element": {"length_mm": 300}}));
    let cone = find(&r["doc"], "elements", r["id"].as_str().unwrap());
    assert_eq!(
        (cone["length_mm"].as_f64(), cone["diameter_large_mm"].as_f64(), cone["diameter_small_mm"].as_f64()),
        (Some(300.0), Some(900.0), Some(450.0))
    );
    // From one cylinder: the first head closes the top, the second the bottom (with the
    // cylinder's diameter, as nothing is below it to inherit from).
    let mut c = Session::default();
    c.execute("vessel.new", &json!({"diameter_mm": 1600})).unwrap();
    assert_eq!(run(&mut c, "vessel.element.add", json!({"kind": "head"}))["index"], 1);
    let r = run(&mut c, "vessel.element.add", json!({"kind": "head"}));
    assert_eq!(r["index"], 0);
    assert_eq!(find(&r["doc"], "elements", r["id"].as_str().unwrap())["diameter_mm"], 1600.0);
    let d = triya_vessel::commands::stored_doc(&c).unwrap();
    assert!(triya_vessel::eval::resolve(&d).is_ok());
    assert!(s.execute("vessel.element.add", &json!({"kind": "body_flange"})).is_err(), "a body flange needs its dimensions");
    assert!(s.execute("vessel.element.add", &json!({"kind": "teapot"})).is_err());
    assert!(s.execute("vessel.element.add", &json!({"index": 99})).is_err());
    assert!(s.execute("vessel.element.add", &json!({"element": {"id": "E1"}})).is_err(), "ids stay unique");
}

#[test]
fn add_nozzle_from_the_ribbon_is_a_plain_attachment() {
    let mut s = session_with(include_str!("reference/vessel-c.json"));
    let r = run(&mut s, "vessel.attachment.add", json!({}));
    assert_eq!(r["id"], "N4");
    let n4 = find(&r["doc"], "attachments", "N4");
    assert_eq!(
        *n4,
        json!({
            "id": "N4", "kind": "nozzle", "host": "E2", "axial": {"ref": "host_from", "mm": 1200.0}, "angle_deg": 0.0,
            "frame": {"type": "radial"}, "nps": "4", "schedule": "40", "neck_type": "pipe",
            "flange": {"standard": "B16.5", "class": 150, "type": "wn", "facing": "rf"},
            "projection": {"from": "od", "mm": 200.0}, "inside_projection_mm": 0.0, "attach": "set_in",
        })
    );
    // On a head: axial, from the centreline.
    let r = run(&mut s, "vessel.attachment.add", json!({"attachment": {"host": "E3", "nps": "2"}}));
    let n = find(&r["doc"], "attachments", r["id"].as_str().unwrap());
    assert_eq!(n["frame"], json!({"type": "parallel_to_axis", "radius_mm": 0.0}));
    assert_eq!(n["projection"]["from"], "cl");
    // The stored nozzle places: inspect resolves it.
    let i = run(&mut s, "vessel.inspect", json!({}));
    let placed = i["nozzles"].as_array().unwrap().iter().find(|n| n["id"] == "N4").unwrap();
    assert!(placed.get("error").is_none() || placed["error"].is_null(), "{placed}");
}

#[test]
fn add_support_sizes_from_the_presets() {
    // Vessel B without its saddles: two saddles at a fifth of the TL–TL from each TL.
    let mut doc: Value = serde_json::from_str(include_str!("reference/vessel-b.json")).unwrap();
    let keep: Vec<Value> = doc["attachments"].as_array().unwrap().iter().filter(|a| a["kind"] != "saddle").cloned().collect();
    doc["attachments"] = json!(keep);
    let mut s = session_with(&doc.to_string());
    let r = run(&mut s, "vessel.support.add", json!({}));
    assert_eq!(r["ids"], json!(["SD1", "SD2"]));
    let a = find(&r["doc"], "attachments", "SD1");
    let b = find(&r["doc"], "attachments", "SD2");
    assert_eq!((a["axial"]["mm"].as_f64(), b["axial"]["mm"].as_f64()), (Some(1000.0), Some(4000.0)));
    assert_eq!(a["angle_deg"], 180.0, "under the vessel (0° is the top)");
    // OD 1220 takes the OD1200 row.
    assert_eq!(a["size_key"], "OD1220");
    assert_eq!(a["host"], "E2");
    let i = run(&mut s, "vessel.inspect", json!({}));
    assert!(!i["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap_or("").contains("SD1")), "{:#}", i["warnings"]);
    // Vessel C (vertical, no skirt): legs; anchor chairs are refused without a skirt.
    let mut s = session_with(include_str!("reference/vessel-c.json"));
    let r = run(&mut s, "vessel.support.add", json!({}));
    let lg = find(&r["doc"], "attachments", r["id"].as_str().unwrap());
    assert_eq!(lg["kind"], "leg");
    assert_eq!(lg["size_key"], "OD916");
    assert!(s.execute("vessel.support.add", &json!({"kind": "anchor_chair"})).is_err());
    assert!(s.execute("vessel.support.add", &json!({"kind": "nozzle"})).is_err());
}

#[test]
fn export_bom_writes_csv() {
    let mut s = session_with(&json!({"schema": "triya.vessel/0.1", "id": "D1", "elements": [{"id": "E1", "kind": "cylinder", "diameter_mm": 1000, "thickness_mm": 10, "length_mm": 2000}]}).to_string());
    let r = run(&mut s, "vessel.bom.export", json!({}));
    assert_eq!(r["filename"], "D1-bom.csv");
    let text = r["text"].as_str().unwrap();
    assert!(text.lines().next().unwrap().starts_with("item,id,description,material,qty,basis"), "{text}");
    assert!(text.contains("Empty weight (kg)"), "{text}");
    let dir = std::env::temp_dir().join(format!("triya-bom-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bom.csv");
    run(&mut s, "vessel.regenerate", json!({}));
    let r = run(&mut s, "vessel.bom.export", json!({"path": path.to_string_lossy()}));
    assert!(r["bytes"].as_u64().unwrap() > 100);
    let csv = std::fs::read_to_string(&path).unwrap();
    // π·1010·2000·10 mm³ of steel ≈ 498 kg on the shell row, measured.
    let shell = csv.lines().find(|l| l.contains(",E1,")).unwrap();
    assert!(shell.contains("measured"), "{shell}");
    let json_out = run(&mut s, "vessel.bom.export", json!({"format": "json"}));
    assert!(json_out["text"].as_str().unwrap().contains("\"items\""));
    assert!(s.execute("vessel.bom.export", &json!({"format": "xls"})).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn patches_and_the_datum_keep_attachments_in_place() {
    let mut s = session_with(include_str!("reference/vessel-c.json"));
    // A JSON-pointer patch is one undo step and is validated as a whole.
    let undo = s.undo.len();
    let r = run(&mut s, "vessel.set", json!({"patch": [{"op": "replace", "path": "/elements/1/length_mm", "value": 2600}]}));
    assert_eq!(r["doc"]["elements"][1]["length_mm"], 2600.0);
    assert_eq!(s.undo.len(), undo + 1);
    assert!(s.execute("vessel.set", &json!({"patch": [{"op": "replace", "path": "/elements/1/length_mm", "value": -5}]})).is_err());
    assert!(s.execute("vessel.set", &json!({"patch": [{"op": "remove", "path": "/elements/0"}]})).is_err(), "the legs' host would go");
    // Moving the datum from the bottom TL to the bottom seam: N1 (300 from the datum) stays put.
    let before = run(&mut s, "vessel.inspect", json!({}));
    let z = |i: &Value, id: &str| i["nozzles"].as_array().unwrap().iter().find(|n| n["id"] == id).unwrap()["origin"][2].as_f64().unwrap();
    let r = run(&mut s, "vessel.datum", json!({"datum": "bottom_seam"}));
    assert_eq!(r["shift_mm"], -25.0, "the straight flange: the seam is 25 above the TL");
    assert_eq!(r["moved"], json!(["N1", "N2", "B1", "CL1"]));
    let after = run(&mut s, "vessel.inspect", json!({}));
    let shift = after["stations"][0]["z_from"].as_f64().unwrap() - before["stations"][0]["z_from"].as_f64().unwrap();
    assert!((z(&after, "N1") - shift - z(&before, "N1")).abs() < 1e-9, "N1 keeps its place on the shell");
    assert!(s.execute("vessel.datum", &json!({"datum": "skirt_base"})).is_err(), "no skirt");
    assert!(s.execute("vessel.datum", &json!({})).is_err());
}
