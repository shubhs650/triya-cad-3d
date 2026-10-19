// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T4.3 acceptance: a stiffening ring (bar outside, angle inside), a half-pipe limpet coil, three
//! lifting lugs (longitudinal on a pad, circumferential, trunnion), an insulation support ring
//! and four clips on one course. Every body's measured volume matches its closed form; the coil's
//! developed length is within 1 % of π·D·n / cos(helix).

use serde_json::{Value, json};
use solidcraft_engine::Session;
use std::f64::consts::PI;

fn vol(s: &Session, name: &str) -> f64 {
    let st = s.model.state();
    let b = st.bodies.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("no body `{name}`"));
    solidcraft_engine::kernel::measure(&b.body).unwrap().volume
}

fn close(what: &str, measured: f64, expected: f64, tol: f64) {
    let d = (measured - expected) / expected;
    eprintln!("{what}: measured {measured:.0} expected {expected:.0} ({:+.4} %)", d * 100.0);
    assert!(d.abs() < tol, "{what}: {measured} vs {expected}");
}

#[test]
fn rings_coil_lugs_and_clips_match_their_closed_forms() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let at = |mm: f64| json!({"ref": "host_from", "mm": mm});
    let doc = json!({"schema": "triya.vessel/0.1", "id": "T11",
        "elements": [{"id": "E1", "kind": "cylinder", "diameter_mm": 1600, "length_mm": 3000, "thickness_mm": 12}],
        "attachments": [
            {"id": "R1", "kind": "stiffening_ring", "host": "E1", "axial": at(2600.0), "section": {"bar": {"height_mm": 100, "thk_mm": 12}}},
            {"id": "R2", "kind": "stiffening_ring", "host": "E1", "axial": at(2800.0), "section": {"angle": {"size": "ISA 75x75x8"}}, "inside": true},
            {"id": "C1", "kind": "limpet_coil", "host": "E1", "axial": at(200.0), "pipe_nps": "2", "schedule": "40", "pitch_mm": 150, "turns": 12,
             "hand": "rh", "half_pipe": true},
            {"id": "L1", "kind": "lifting_lug", "host": "E1", "axial": at(2300.0), "angle_deg": 90, "style": "longitudinal",
             "plate": {"len_mm": 300, "height_mm": 200, "thk_mm": 25}, "hole_dia_mm": 60, "pad": {"width_mm": 400, "thickness_mm": 14}},
            {"id": "L2", "kind": "lifting_lug", "host": "E1", "axial": at(2300.0), "angle_deg": 270, "style": "circumferential",
             "plate": {"len_mm": 150, "height_mm": 100, "thk_mm": 12}, "hole_dia_mm": 30},
            {"id": "L3", "kind": "lifting_lug", "host": "E1", "axial": at(2300.0), "angle_deg": 0, "style": "trunnion",
             "plate": {"len_mm": 250, "height_mm": 150, "thk_mm": 20}, "hole_dia_mm": 50},
            {"id": "I1", "kind": "insulation_ring", "host": "E1", "axial": at(2700.0), "width_mm": 40, "thk_mm": 6, "segments": 4},
            {"id": "CL1", "kind": "clip", "host": "E1", "axial": at(2450.0), "angle_deg": 45, "plate": {"len_mm": 150, "wid_mm": 100, "thk_mm": 10},
             "count": 4, "purpose": "ladder"}]});
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let t = std::time::Instant::now();
    let r = s.execute("vessel.regenerate", &json!({})).unwrap();
    eprintln!("regenerate: {:?}\nsupports: {:#}\nwarnings: {:#}", t.elapsed(), r["supports"], r["warnings"]);
    for sp in r["supports"].as_array().unwrap() {
        assert_eq!(sp["built"], "built", "{sp}");
    }
    let (ri, ro) = (800.0_f64, 812.0_f64);
    let ann = |a: f64, b: f64| PI * (b * b - a * a);
    close("bar ring", vol(&s, "Vessel R1 ring"), ann(ro, ro + 100.0) * 12.0, 1e-3);
    close("angle ring", vol(&s, "Vessel R2 ring"), ann(ri - 8.0, ri) * 75.0 + ann(ri - 75.0, ri - 8.0) * 8.0, 1e-3);
    close("insulation ring", vol(&s, "Vessel I1 ring"), ann(ro, ro + 40.0) * 6.0, 1e-3);
    close("longitudinal lug", vol(&s, "Vessel L1 lug"), 300.0 * 200.0 * 25.0 - PI * 30.0 * 30.0 * 25.0, 1e-3);
    close("lug pad", vol(&s, "Vessel L1 pad"), 400.0 / ro / 2.0 * ((ro + 14.0).powi(2) - ro * ro) * 400.0, 1e-3);
    close("circumferential lug", vol(&s, "Vessel L2 lug"), 150.0 * 100.0 * 12.0 - PI * 15.0 * 15.0 * 12.0, 1e-3);
    close("trunnion", vol(&s, "Vessel L3 lug") + vol(&s, "Vessel L3 end plate"), PI * 125.0 * 125.0 * 130.0 + PI * 145.0 * 145.0 * 20.0, 1e-3);
    for n in 1..=4 {
        close(&format!("clip {n}"), vol(&s, &format!("Vessel CL1 clip {n}")), 150.0 * 100.0 * 10.0, 1e-3);
    }
    // The coil: 2 in sch 40 (60.3 OD), its centreline on the shell's outside, 12 turns at 150
    // pitch, modelled as the solid pipe. Its developed length is π·D·n / cos(atan(p / (π·D))).
    let (d, n, p) = (2.0 * ro, 12.0, 150.0);
    let length = PI * d * n / (p / (PI * d)).atan().cos();
    assert!(r["parameters"].as_array().unwrap().iter().any(|x| x == "VT11_C1_length"), "{:#}", r["parameters"]);
    let measured_length = vol(&s, "Vessel C1 coil") / (PI * 30.15 * 30.15);
    close("coil developed length", measured_length, length, 1e-2);
}

#[test]
fn vessel_a_builds_its_lugs_ring_coil_and_chairs() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let mut doc: Value = serde_json::from_str(include_str!("reference/vessel-a.json")).unwrap();
    // The nozzles are the Phase 3 exit's business (and slow); everything else must build.
    doc["attachments"] = json!(doc["attachments"].as_array().unwrap().iter().filter(|a| a["kind"] != "nozzle").cloned().collect::<Vec<_>>());
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let r = s.execute("vessel.regenerate", &json!({})).unwrap();
    eprintln!("warnings: {:#}", r["warnings"]);
    let ids: Vec<&str> = r["supports"].as_array().unwrap().iter().filter_map(|x| x["id"].as_str()).collect();
    assert_eq!(ids, ["L1", "L2", "R1", "C1", "AC1"]);
    for sp in r["supports"].as_array().unwrap() {
        assert_eq!(sp["built"], "built", "{sp}");
    }
    assert!(!r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap_or("").contains("not built yet")), "{:#}", r["warnings"]);
}
