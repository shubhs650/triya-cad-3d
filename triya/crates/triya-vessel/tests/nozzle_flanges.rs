// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T3.4 acceptance: nozzle flanges (WN, SO, LWN, blind) from the B16.5 table, their volume
//! against the revolved section less the bolt holes, the bolt count from the row; a manway
//! carries its davit as a reference.

use serde_json::{Value, json};
use solidcraft_engine::Session;
use std::f64::consts::PI;
use triya_stdlib::{FlangeKind, TableSet};

fn vessel(nozzle: Value) -> Value {
    json!({
        "schema": "triya.vessel/0.1", "id": "T8",
        "elements": [{"id": "S1", "kind": "cylinder", "diameter_mm": 1000, "length_mm": 1200, "thickness_mm": 10}],
        "attachments": [nozzle],
    })
}

fn nozzle(kind: &str, class: u32, facing: &str) -> Value {
    json!({"id": "N1", "kind": "nozzle", "host": "S1", "axial": {"ref": "host_from", "mm": 600}, "angle_deg": 0, "frame": {"type": "radial"},
        "nps": "4", "schedule": "STD", "projection": {"from": "cl", "mm": 800},
        "flange": {"standard": "B16.5", "class": class, "type": kind, "facing": facing}})
}

fn regenerate(doc: Value) -> (Session, Value) {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let r = s.execute("vessel.regenerate", &json!({})).unwrap();
    (s, r)
}

fn volume(s: &Session, body: &str) -> f64 {
    let st = s.model.state();
    let b = st.bodies.iter().find(|b| b.name == body).unwrap_or_else(|| panic!("no body `{body}`"));
    solidcraft_engine::kernel::measure(&b.body).unwrap().volume
}

/// Volume of a closed (r, s) polygon revolved about the s axis (Pappus).
fn revolved(pts: &[[f64; 2]]) -> f64 {
    let mut m = 0.0;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        m += (a[0] + b[0]) * (a[0] * b[1] - b[0] * a[1]);
    }
    (2.0 * PI * m / 6.0).abs()
}

/// The section by hand from the row: weld end, hub, ring, raised face (s from the gasket face).
fn section(kind: FlangeKind, class: &str, rf: bool) -> (Vec<[f64; 2]>, triya_stdlib::FlangeRow) {
    let t = TableSet::embedded().unwrap();
    let row = t.flange_in("B16.5", "4", class, kind).unwrap().clone();
    let (rb, od) = (102.26 / 2.0, 114.3);
    let rf_h = if rf { row.raised_face_h } else { 0.0 };
    let face = 800.0 - rf_h;
    let back = face - row.thickness;
    let hub_back = face - row.length_through_hub;
    let mut p = match kind {
        FlangeKind::Blind => vec![[0.0, back], [row.od / 2.0, back], [row.od / 2.0, face]],
        FlangeKind::Wn => {
            vec![[rb, hub_back], [row.hub_weld_d.unwrap_or(od) / 2.0, hub_back], [row.hub_od / 2.0, back], [row.od / 2.0, back], [row.od / 2.0, face]]
        }
        _ => {
            let b = row.bore.unwrap() / 2.0;
            vec![[b, hub_back], [row.hub_od / 2.0, hub_back], [row.hub_od / 2.0, back], [row.od / 2.0, back], [row.od / 2.0, face]]
        }
    };
    if rf {
        p.push([row.raised_face_d / 2.0, face]);
        p.push([row.raised_face_d / 2.0, 800.0]);
    }
    let inner = match kind {
        FlangeKind::Blind => 0.0,
        FlangeKind::Wn => rb,
        _ => row.bore.unwrap() / 2.0,
    };
    p.push([inner, 800.0]);
    (p, row)
}

fn check(kind: &str, fk: FlangeKind, class: u32, facing: &str) {
    let (s, r) = regenerate(vessel(nozzle(kind, class, facing)));
    let n = &r["nozzles"][0];
    assert_eq!(n["built"], json!("joined"), "{n} {}", r["warnings"]);
    let (pts, row) = section(fk, &class.to_string(), facing != "ff");
    assert_eq!(n["bolt_count"].as_u64(), Some(u64::from(row.bolt_count)), "bolt count from the row");
    let holes = f64::from(row.bolt_count) * PI * (row.bolt_hole.unwrap() / 2.0).powi(2) * row.thickness;
    let expected = revolved(&pts) - holes;
    let v = volume(&s, "Vessel N1 flange");
    assert!(((v - expected) / expected).abs() < 1e-3, "{kind} {class} {facing}: {v:.0} vs {expected:.0}");
    // One hole patterned `bolt_count` times.
    let pattern = r["steps"].as_array().unwrap().iter().find(|st| st["command"] == "solid.pattern.circular").unwrap();
    assert_eq!(pattern["params"]["count"].as_u64(), Some(u64::from(row.bolt_count)));
}

#[test]
fn weld_neck_class_150_rf() {
    check("wn", FlangeKind::Wn, 150, "rf");
}

#[test]
fn slip_on_class_300_ff() {
    check("so", FlangeKind::So, 300, "ff");
}

#[test]
fn blind_class_600_rf() {
    check("blind", FlangeKind::Blind, 600, "rf");
}

#[test]
fn long_weld_neck_uses_the_weld_neck_row_and_says_so() {
    let (s, r) = regenerate(vessel(nozzle("lwn", 150, "rf")));
    assert_eq!(r["nozzles"][0]["built"], json!("joined"));
    assert!(r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap_or("").contains("LWN")), "{}", r["warnings"]);
    let (pts, row) = section(FlangeKind::Wn, "150", true);
    let expected = revolved(&pts) - f64::from(row.bolt_count) * PI * (row.bolt_hole.unwrap() / 2.0).powi(2) * row.thickness;
    assert!(((volume(&s, "Vessel N1 flange") - expected) / expected).abs() < 1e-3);
}

#[test]
fn a_manway_keeps_its_davit_as_a_reference() {
    let mut m = nozzle("wn", 150, "rf");
    m["id"] = json!("M1");
    m["nps"] = json!("20");
    m["projection"] = json!({"from": "cl", "mm": 1100});
    m["manway"] = json!({"davit": true});
    let mut doc = vessel(m);
    doc["elements"][0]["diameter_mm"] = json!(1600);
    let (mut s, r) = regenerate(doc);
    let n = &r["nozzles"][0];
    assert_eq!(n["built"], json!("joined"), "{n} {}", r["warnings"]);
    assert_eq!((n["manway"].clone(), n["davit"].clone()), (json!(true), json!(true)));
    let inspect = s.execute("vessel.inspect", &json!({})).unwrap();
    assert_eq!(inspect["nozzles"][0]["davit"], json!(true));
    // No davit geometry: the manway is its neck and flange only.
    let names: Vec<String> = s.model.state().bodies.iter().map(|b| b.name.clone()).collect();
    assert_eq!(names, vec!["Vessel S1".to_string(), "Vessel M1 flange".to_string()]);
}
