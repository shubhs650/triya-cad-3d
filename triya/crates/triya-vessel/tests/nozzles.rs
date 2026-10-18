// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T3.3 acceptance: six nozzle configurations, on a cylinder and on both heads, cut and joined
//! by the kernel, each within tolerance of its reference volume (`nozzle::expected_change`).

use serde_json::{Value, json};
use solidcraft_engine::Session;
use std::f64::consts::PI;

/// A 1000 ID × 10 shell, 1500 long, between a 2:1 bottom head and an F&D top head.
fn vessel(nozzle: Value) -> Value {
    json!({
        "schema": "triya.vessel/0.1", "id": "T7",
        "elements": [
            {"id": "H0", "kind": "head", "preset": "ellipsoidal_2_1", "diameter_mm": 1000, "thickness_mm": 10, "straight_flange_mm": 40},
            {"id": "S1", "kind": "cylinder", "length_mm": 1500, "thickness_mm": 10},
            {"id": "H1", "kind": "head", "preset": "asme_fd", "thickness_mm": 10, "straight_flange_mm": 40}
        ],
        "attachments": [nozzle],
    })
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

/// The nozzle was joined, within tolerance, and its flags say so.
fn joined(s: &mut Session, r: &Value) -> f64 {
    let n = &r["nozzles"][0];
    assert_eq!(n["built"], json!("joined"), "{n}\nwarnings: {}", r["warnings"]);
    // `vessel.regenerate` kept it only within `nozzle::tolerance` of the reference.
    let e = n["expected_mm3"].as_f64().unwrap();
    assert!(e > 0.0 && n["measured_mm3"].is_number(), "{n}");
    let inspect = s.execute("vessel.inspect", &json!({})).unwrap();
    assert!(inspect["flags"].as_array().unwrap().iter().all(|f| f["boolean_failed"] == json!(false)), "{}", inspect["flags"]);
    e
}

fn on_shell(id: &str, frame: Value, nps: &str, sch: &str, extra: Value) -> Value {
    let mut n = json!({"id": id, "kind": "nozzle", "host": "S1", "axial": {"ref": "host_from", "mm": 750}, "angle_deg": 90, "frame": frame,
        "nps": nps, "schedule": sch, "projection": {"from": "od", "mm": 150}});
    if let (Some(o), Some(x)) = (n.as_object_mut(), extra.as_object()) {
        o.extend(x.clone());
    }
    n
}

const SHELL: f64 = PI * (510.0 * 510.0 - 500.0 * 500.0) * 1500.0;

#[test]
fn radial_set_in_with_pad() {
    let (mut s, r) = regenerate(vessel(on_shell(
        "N1",
        json!({"type": "radial"}),
        "4",
        "STD",
        json!({"pad": {"width_mm": 40, "thickness_mm": 10}, "projection": {"from": "cl", "mm": 750}}),
    )));
    let e = joined(&mut s, &r);
    // Independently: the shell course's analytic volume plus the nozzle's.
    let v = volume(&s, "Vessel S1");
    assert!((v - (SHELL + e)).abs() < 3e-4 * SHELL + 0.02 * e, "{v:.0} vs {:.0}", SHELL + e);
    let params: Vec<&str> = r["parameters"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(params.contains(&"VT7_N1_neck_od") && params.contains(&"VT7_N1_pad_od"), "{params:?}");
}

#[test]
fn hillside_set_on() {
    let (mut s, r) = regenerate(vessel(on_shell("N2", json!({"type": "hillside", "offset_mm": 200}), "3", "XS", json!({"attach": "set_on"}))));
    let e = joined(&mut s, &r);
    let v = volume(&s, "Vessel S1");
    assert!((v - (SHELL + e)).abs() < 3e-4 * SHELL + 0.02 * e, "{v:.0} vs {:.0}", SHELL + e);
}

#[test]
fn tangential() {
    let (mut s, r) = regenerate(vessel(on_shell("N3", json!({"type": "tangential"}), "2", "80", json!({}))));
    joined(&mut s, &r);
}

#[test]
fn tilted_with_inside_projection() {
    let (mut s, r) = regenerate(vessel(on_shell(
        "N4",
        json!({"type": "tilted", "tilt_deg": 15}),
        "2",
        "XS",
        json!({"inside_projection_mm": 25, "neck_type": "forged"}),
    )));
    joined(&mut s, &r);
}

#[test]
fn normal_to_the_bottom_head() {
    let (mut s, r) = regenerate(vessel(json!({"id": "N5", "kind": "nozzle", "host": "H0", "axial": {"ref": "host_from", "mm": 0}, "angle_deg": 270,
        "frame": {"type": "head_radial_at_r", "radius_mm": 250}, "nps": "2", "schedule": "XS", "projection": {"from": "od", "mm": 150}})));
    joined(&mut s, &r);
}

#[test]
fn parallel_to_the_axis_on_the_top_head() {
    let (mut s, r) = regenerate(vessel(json!({"id": "M1", "kind": "nozzle", "host": "H1", "axial": {"ref": "host_from", "mm": 0}, "angle_deg": 180,
        "frame": {"type": "parallel_to_axis", "radius_mm": 250}, "nps": "8", "schedule": "STD", "projection": {"from": "cl", "mm": 500},
        "manway": {"davit": true}})));
    joined(&mut s, &r);
}
