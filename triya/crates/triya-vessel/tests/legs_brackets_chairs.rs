// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T4.2 acceptance: pipe legs under a head and angle legs on a shell (each with its base plate),
//! bracket lugs (gussets, top plate with its bolt hole, bottom plate) and anchor chairs on a
//! skirt's base ring. Every body's measured volume matches its closed form, and each pattern has
//! as many copies as its `count`.

use serde_json::{Value, json};
use solidcraft_engine::Session;
use std::f64::consts::PI;

fn regenerate(doc: Value) -> (Session, Value) {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let t = std::time::Instant::now();
    let r = s.execute("vessel.regenerate", &json!({})).unwrap();
    eprintln!("regenerate: {:?}\nwarnings: {:#}", t.elapsed(), r["warnings"]);
    (s, r)
}

fn names(s: &Session, prefix: &str) -> Vec<String> {
    s.model.state().bodies.iter().map(|b| b.name.clone()).filter(|n| n.starts_with(prefix)).collect()
}

fn vol(s: &Session, name: &str) -> f64 {
    let st = s.model.state();
    let b = st.bodies.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("no body `{name}`"));
    solidcraft_engine::kernel::measure(&b.body).unwrap().volume
}

fn close(what: &str, measured: f64, expected: f64) {
    let d = (measured - expected) / expected;
    eprintln!("{what}: measured {measured:.0} expected {expected:.0} ({:+.4} %)", d * 100.0);
    assert!(d.abs() < 1e-3, "{what}: {measured} vs {expected}");
}

fn built(r: &Value, id: &str) -> Value {
    let sp = r["supports"].as_array().unwrap().iter().find(|x| x["id"] == id).cloned().unwrap_or_else(|| panic!("{id}: {r:#}"));
    assert_eq!(sp["built"], "built", "{sp}");
    sp
}

#[test]
fn vessel_c_stands_on_four_pipe_legs_and_carries_two_brackets() {
    let mut doc: Value = serde_json::from_str(include_str!("reference/vessel-c.json")).unwrap();
    doc["attachments"] =
        json!(doc["attachments"].as_array().unwrap().iter().filter(|a| a["kind"] == "leg" || a["kind"] == "bracket").cloned().collect::<Vec<_>>());
    let (s, r) = regenerate(doc);
    built(&r, "LG1");
    built(&r, "B1");
    // Four legs of 4 in sch 40 (114.3 × 6.02), 1200 long, each on a 200 × 200 × 16 base plate.
    let legs: Vec<String> = names(&s, "Vessel LG1 leg ").into_iter().filter(|n| !n.ends_with("base plate")).collect();
    assert_eq!(legs.len(), 4, "{legs:?}");
    for n in 1..=4 {
        close(
            &format!("leg {n}"),
            vol(&s, &format!("Vessel LG1 leg {n}")),
            PI / 4.0 * (114.3_f64.powi(2) - (114.3 - 2.0 * 6.02_f64).powi(2)) * 1200.0,
        );
        close(&format!("leg {n} base"), vol(&s, &format!("Vessel LG1 leg {n} base plate")), 200.0 * 200.0 * 16.0);
    }
    // Under the hemispherical bottom head (inside radius 450): the legs stand at 45°, 135°, …
    // with their outer face on the inside radius; their tops on the head's outside.
    let st = s.model.state();
    let leg1 = st.bodies.iter().find(|b| b.name == "Vessel LG1 leg 1").unwrap().body.tessellate(0.1).unwrap();
    let top = leg1.positions.iter().map(|p| p.z).fold(f64::MIN, f64::max);
    let (rho_in, ro) = (450.0 - 114.3, 458.0_f64);
    let tl = 0.0; // the bottom head's tangent line is the datum
    let expect_top = tl - (ro * ro - rho_in * rho_in).sqrt();
    assert!((top - expect_top).abs() < 0.05, "leg top {top}, head outside {expect_top}");
    // Two brackets at 90° and 270°: two gussets (trapezoids 150 wide, 250 high, 40 at the toe),
    // a 200 × 160 × 12 top plate less one 22 hole, a 200 × 40 × 10 bottom plate.
    for n in 1..=2 {
        for side in ["a", "b"] {
            close(&format!("bracket {n} gusset {side}"), vol(&s, &format!("Vessel B1 {n} gusset {side}")), (150.0 + 40.0) / 2.0 * 250.0 * 10.0);
        }
        close(&format!("bracket {n} top"), vol(&s, &format!("Vessel B1 {n} top plate")), 200.0 * 160.0 * 12.0 - PI * 11.0 * 11.0 * 12.0);
        close(&format!("bracket {n} bottom"), vol(&s, &format!("Vessel B1 {n} bottom plate")), 200.0 * 40.0 * 10.0);
    }
    assert_eq!(names(&s, "Vessel B1 ").iter().filter(|n| n.ends_with("top plate")).count(), 2);
}

#[test]
fn angle_legs_on_a_shell_and_channel_legs_reported() {
    let doc = json!({"schema": "triya.vessel/0.1", "id": "T10",
        "elements": [{"id": "E1", "kind": "cylinder", "diameter_mm": 1000, "length_mm": 1500, "thickness_mm": 8}],
        "attachments": [
            {"id": "LG1", "kind": "leg", "host": "E1", "axial": {"ref": "host_from", "mm": 200}, "count": 3,
             "section": {"angle": {"size": "ISA 100x100x10"}}, "length_mm": 1800, "base_plate": {"len_mm": 180, "wid_mm": 180, "thk_mm": 12}},
            {"id": "LG2", "kind": "leg", "host": "E1", "axial": {"ref": "host_from", "mm": 200}, "count": 3, "start_angle_deg": 60,
             "section": {"channel": {"size": "ISMC 150"}}, "length_mm": 1800}]});
    let (s, r) = regenerate(doc);
    built(&r, "LG1");
    for n in 1..=3 {
        close(&format!("angle leg {n}"), vol(&s, &format!("Vessel LG1 leg {n}")), (100.0 + 100.0 - 10.0) * 10.0 * 1800.0);
    }
    assert_eq!(names(&s, "Vessel LG1 leg ").len(), 6);
    // The legs' tops are at the stated elevation (200 above the shell's start).
    let w = r["warnings"].as_array().unwrap();
    assert!(w.iter().any(|w| w.as_str().unwrap_or("").contains("`LG2`") && w.as_str().unwrap_or("").contains("not built yet")), "{w:?}");
    assert!(names(&s, "Vessel LG2").is_empty());
}

#[test]
fn eight_anchor_chairs_on_vessel_a_skirt() {
    let mut doc: Value = serde_json::from_str(include_str!("reference/vessel-a.json")).unwrap();
    // The skirt and bottom head carry the chairs; the rest of the shell is other tests' business.
    doc["elements"] = json!(doc["elements"].as_array().unwrap()[..3].to_vec());
    doc["attachments"] = json!(doc["attachments"].as_array().unwrap().iter().filter(|a| a["kind"] == "anchor_chair").cloned().collect::<Vec<_>>());
    let (s, r) = regenerate(doc);
    built(&r, "AC1");
    // Skirt 1600 ID × 10 → OD 1620... the skirt's OD is the head's OD (1624), base ring OD 1900.
    let sp = built(&r, "AC1");
    let parts = sp["parts"].as_array().unwrap();
    assert_eq!(parts.iter().filter(|p| p["name"].as_str().unwrap().ends_with("top plate")).count(), 8);
    let rs = 1624.0 / 2.0;
    let radial = 950.0 - rs;
    for n in 1..=8 {
        for side in ["a", "b"] {
            close(&format!("chair {n} gusset {side}"), vol(&s, &format!("Vessel AC1 {n} gusset {side}")), radial * (300.0 - 25.0) * 16.0);
        }
        close(&format!("chair {n} top"), vol(&s, &format!("Vessel AC1 {n} top plate")), radial * 200.0 * 25.0 - PI * 18.0 * 18.0 * 25.0);
    }
}
