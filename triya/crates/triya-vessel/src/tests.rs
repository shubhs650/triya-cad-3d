// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Tests: the plan for a plain cylinder, the vessel commands under hostile parameters (no
//! panic, as upstream's `hostile_params_never_panic`), and a real regeneration measured.

use serde_json::{Value, json};
use solidcraft_engine::{EngineError, Session};
use triya_stdlib::HeadPreset;

use crate::model::{Element, ElementKind, VesselDoc};
use crate::{commands, emit, eval};

/// The commands are registered once per test process (the registry is global and idempotent).
fn registered() {
    solidcraft_engine::cmd::triya_extra::register(commands::COMMANDS).unwrap();
}

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

#[test]
fn plan_for_a_cylinder_is_sketch_rectangle_finish_revolve() {
    let doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 2000.0);
    let r = eval::resolve(&doc).unwrap();
    assert_eq!(r.stations.len(), 1);
    let st = &r.stations[0];
    assert_eq!((st.z_from, st.z_to, st.r_from, st.thickness), (0.0, 2000.0, 500.0, 10.0));
    assert_eq!(r.radius_at(1000.0), Some(500.0));
    assert_eq!(r.radius_at(2500.0), None);

    let plan = emit::plan(&r).unwrap();
    let ids: Vec<&str> = plan.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["sketch.create", "sketch.rectangle.two_point", "sketch.finish", "solid.revolve"]);
    assert_eq!(
        plan[0].params,
        json!({"name": "Vessel S1 profile", "plane": {"origin": [0.0, 0.0, 0.0], "x_dir": [1.0, 0.0, 0.0], "y_dir": [0.0, 0.0, 1.0]}})
    );
    assert_eq!(plan[1].params, json!({"p0": [500.0, 0.0], "p1": [510.0, 2000.0], "sketch": "$ref:0.sketch"}));
    assert_eq!(plan[2].params, json!({}));
    assert_eq!(
        plan[3].params,
        json!({"sketch": "$ref:0.sketch", "axis": "Z", "angle": "360 deg", "operation": "new", "name": "Vessel S1", "body_name": "Vessel S1"})
    );
    // References resolve against earlier replies.
    let sub = emit::substitute(&plan[3].params, &[json!({"sketch": 7})]).unwrap();
    assert_eq!(sub["sketch"], json!(7));
    assert!(emit::substitute(&plan[3].params, &[]).is_err());
}

#[test]
fn heads_cones_and_attachments_are_not_supported_yet_but_resolve() {
    let mut doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 2000.0);
    doc.elements.insert(
        0,
        Element {
            id: "H1".into(),
            kind: ElementKind::Head { preset: HeadPreset::ELLIPSOIDAL_2_1 },
            id_or_od_mm: 1000.0,
            thickness_mm: 10.0,
            length_mm: 0.0,
            straight_flange_mm: 40.0,
            corrosion_allowance_mm: 0.0,
            material: None,
            note: None,
        },
    );
    doc.global.datum = crate::model::Datum::FirstTangentLine;
    let r = eval::resolve(&doc).unwrap();
    let h = r.station("H1").unwrap();
    assert!((h.z_from + 290.0).abs() < 1e-9 && h.z_to.abs() < 1e-9, "{h:?}");
    assert_eq!(r.station("S1").map(|s| (s.z_from, s.z_to)), Some((0.0, 2000.0)));
    // The head's radius: full in the straight flange, the semi-ellipse's at its crown.
    assert!((r.radius_at(-20.0).unwrap() - 500.0).abs() < 1e-9);
    assert!(r.radius_at(-289.9).unwrap() < 20.0);
    assert!((r.radius_at(-40.0 - 125.0).unwrap() - 500.0 * (0.75f64).sqrt()).abs() < 1e-6);
    let e = emit::plan(&r).unwrap_err();
    assert!(matches!(e, crate::VesselError::NotSupportedYet(_)), "{e}");

    // A torispherical head's profile: continuous, monotone, zero at the crown.
    let mut tori = VesselDoc::cylinder("S1", 1000.0, 10.0, 100.0);
    tori.elements.push(Element {
        id: "H2".into(),
        kind: ElementKind::Head { preset: HeadPreset::KLOPPER },
        id_or_od_mm: 1000.0,
        thickness_mm: 10.0,
        length_mm: 0.0,
        straight_flange_mm: 0.0,
        corrosion_allowance_mm: 0.0,
        material: None,
        note: None,
    });
    let r = eval::resolve(&tori).unwrap();
    let h = r.station("H2").unwrap();
    let mut last = f64::INFINITY;
    let n = 200;
    for i in 0..=n {
        let z = h.z_from + (h.z_to - h.z_from) * i as f64 / n as f64;
        let x = h.radius_at(z).unwrap();
        assert!(x <= last + 1e-6, "radius must not grow toward the crown: z = {z}, {x} > {last}");
        last = x;
    }
    assert!((h.radius_at(h.z_from).unwrap() - 500.0).abs() < 1e-9);
    assert!(h.radius_at(h.z_to).unwrap() < 1.0);
}

#[test]
fn invalid_documents_are_errors() {
    assert!(VesselDoc::from_json("nope").is_err());
    assert!(VesselDoc::from_json(r#"{"schema":"other/1"}"#).is_err());
    let mut doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 2000.0);
    doc.elements.push(doc.elements[0].clone());
    assert!(doc.validate().is_err(), "duplicate ids");
    let mut doc = VesselDoc::cylinder("S1", f64::NAN, 10.0, 2000.0);
    assert!(doc.validate().is_err());
    doc.elements[0].id_or_od_mm = 1000.0;
    doc.elements[0].length_mm = 0.0;
    assert!(doc.validate().is_err());
    let empty = VesselDoc::new(None);
    assert!(eval::resolve(&empty).is_err());
    let mut outside = VesselDoc::cylinder("S1", 20.0, 10.0, 100.0);
    outside.global.diameter_basis = crate::model::DiameterBasis::Outside;
    assert!(eval::resolve(&outside).is_err(), "no inside diameter left");
}

/// Every vessel command, every documented key, every hostile value: no panic (an
/// `EngineError::Internal` is a panic the engine caught).
#[test]
fn vessel_commands_never_panic_on_hostile_params() {
    registered();
    let keys =
        ["doc", "diameter_mm", "thickness_mm", "length_mm", "name", "replace", "element", "attachment", "global", "remove", "schema", "elements"];
    let hostile = [
        Value::Null,
        json!(true),
        json!(-1),
        json!(0),
        json!(1e308),
        json!(-1e308),
        json!(""),
        json!("nope"),
        json!("$ref:0.sketch"),
        json!([]),
        json!([1e308, -1e308]),
        json!({}),
        json!({"x": 1}),
        json!({"schema": "triya.vessel/0.1"}),
        json!({"schema": "triya.vessel/0.1", "elements": [{"id": "", "type": "cylinder"}]}),
        json!({"schema": "triya.vessel/0.1", "elements": [{"id": "a", "kind": {"type": "cylinder"}, "id_or_od_mm": 1e308, "thickness_mm": 1, "length_mm": 1}]}),
        json!({"schema": "triya.vessel/0.1", "elements": [{"id": "a", "kind": {"type": "head", "preset": {"type": "torispherical", "crown_ratio": 0.1, "knuckle_ratio": 0.49}}, "id_or_od_mm": 10, "thickness_mm": 1}]}),
        json!({"id": "S1", "kind": {"type": "cone", "small_diameter_mm": -5}, "id_or_od_mm": 100, "thickness_mm": 5, "length_mm": 50}),
        json!({"id": "N1", "host": "S1", "axial": {"type": "fraction", "t": 2}, "angle_deg": 0, "frame": {"type": "radial"}, "kind": {"type": "nozzle", "nps": "2", "class": "150", "flange": "wn", "projection_mm": 150}}),
    ];
    let mut internal = Vec::new();
    for spec in commands::COMMANDS {
        for v in &hostile {
            for k in keys {
                for (label, p) in [("keyed", json!({ k: v.clone() })), ("whole", v.clone())] {
                    // With and without a vessel already in the design.
                    for with_vessel in [false, true] {
                        let mut s = Session::default();
                        if with_vessel {
                            let _ = s.execute("vessel.new", &json!({}));
                        }
                        if let Err(EngineError::Internal(id, msg)) = s.execute(spec.id, &p) {
                            internal.push(format!("{id} {label} {p}: {msg}"));
                        }
                    }
                }
            }
        }
    }
    assert!(internal.is_empty(), "commands panicked:\n{}", internal.join("\n"));
}

#[test]
fn registry_sees_the_vessel_commands_once() {
    registered();
    // Registering again is a no-op; a conflicting id is refused.
    assert!(solidcraft_engine::cmd::triya_extra::register(commands::COMMANDS).is_ok());
    static CLASH: &[solidcraft_engine::CommandSpec] = &[solidcraft_engine::CommandSpec::new("solid.extrude", "Clash", |_, _| Ok(Value::Null))];
    assert!(solidcraft_engine::cmd::triya_extra::register(CLASH).is_err());
    let all = solidcraft_engine::command_specs();
    for spec in commands::COMMANDS {
        assert_eq!(all.iter().filter(|c| c.id == spec.id).count(), 1, "{}", spec.id);
        assert_eq!(all.iter().find(|c| c.id == spec.id).map(|c| c.tab), Some(commands::TAB));
    }
    assert_eq!(solidcraft_engine::find_command("vessel.new").map(|c| c.label), Some("New Vessel"));
}

/// A 1000 ID × 10 t × 2000 L cylinder regenerated through the engine: one body whose volume is
/// π (510² − 500²) × 2000, built in one undo step, kept with the design's metadata.
#[test]
fn regenerate_builds_the_cylinder_in_one_undo_step() {
    registered();
    let mut s = Session::default();
    let before_undo = s.undo.len();
    run(&mut s, "vessel.new", json!({"diameter_mm": 1000, "thickness_mm": 10, "length_mm": 2000, "name": "Drum"}));
    let stored = s.doc.metadata.get(commands::METADATA_KEY).expect("vessel kept in metadata");
    assert_eq!(stored["doc"]["schema"], json!("triya.vessel/0.1"));
    assert_eq!(stored["doc"]["name"], json!("Drum"));
    assert!(s.execute("vessel.new", &json!({})).is_err(), "a second vessel is refused without replace");

    let inspect = run(&mut s, "vessel.inspect", json!({}));
    assert_eq!(inspect["plan"].as_array().map(Vec::len), Some(4));
    assert_eq!(inspect["stations"][0]["z_to"], json!(2000.0));

    let r = run(&mut s, "vessel.regenerate", json!({}));
    assert_eq!(r["steps"].as_array().map(Vec::len), Some(4));
    assert_eq!(s.undo.len(), before_undo + 2, "vessel.new and vessel.regenerate: one undo step each");
    assert!(s.active_sketch.is_none());
    let st = s.model.state();
    assert_eq!(st.bodies.len(), 1, "{:?}", st.bodies.iter().map(|b| &b.name).collect::<Vec<_>>());
    let body = &st.bodies[0];
    assert_eq!(body.name, "Vessel S1");
    let m = solidcraft_engine::kernel::measure(&body.body).unwrap();
    let expected = std::f64::consts::PI * (510.0f64.powi(2) - 500.0f64.powi(2)) * 2000.0;
    assert!((m.volume - expected).abs() / expected < 1e-3, "volume {} vs {expected}", m.volume);
    assert!((m.bbox.min.z - 0.0).abs() < 1e-6 && (m.bbox.max.z - 2000.0).abs() < 1e-6, "{:?}", m.bbox);
    assert!((m.bbox.max.x - 510.0).abs() < 1e-3, "{:?}", m.bbox);

    // The generated features are remembered, and replaced by the next regeneration.
    let features = r["features"].as_array().cloned().unwrap_or_default();
    assert_eq!(features.len(), 2, "sketch and revolve: {features:?}");
    run(
        &mut s,
        "vessel.set",
        json!({"element": {"id": "S1", "kind": {"type": "cylinder"}, "id_or_od_mm": 800, "thickness_mm": 8, "length_mm": 1000}}),
    );
    let r2 = run(&mut s, "vessel.regenerate", json!({}));
    assert_ne!(r2["features"], r["features"]);
    let st = s.model.state();
    assert_eq!(st.bodies.len(), 1, "the old body went with its features");
    let m = solidcraft_engine::kernel::measure(&st.bodies[0].body).unwrap();
    let expected = std::f64::consts::PI * (408.0f64.powi(2) - 400.0f64.powi(2)) * 1000.0;
    assert!((m.volume - expected).abs() / expected < 1e-3, "volume {} vs {expected}", m.volume);

    // Undo takes the whole regeneration back at once.
    run(&mut s, "edit.undo", json!({}));
    let st = s.model.state();
    assert_eq!(st.bodies.len(), 1, "back to the first cylinder");
    let m = solidcraft_engine::kernel::measure(&st.bodies[0].body).unwrap();
    assert!((m.bbox.max.z - 2000.0).abs() < 1e-6, "{:?}", m.bbox);

    // A design with the vessel round-trips through JSON with the vessel intact.
    let text = s.doc.to_json();
    let back = solidcraft_engine::doc::Document::from_json(&text).unwrap();
    assert_eq!(back.metadata.get(commands::METADATA_KEY).map(|v| v["doc"]["elements"][0]["id"].clone()), Some(json!("S1")));
}

#[test]
fn regenerate_refuses_attachments_and_leaves_the_design_as_it_was() {
    registered();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({}));
    run(
        &mut s,
        "vessel.set",
        json!({"attachment": {"id": "N1", "host": "S1", "axial": {"type": "from_host_start", "dz_mm": 500}, "angle_deg": 90, "frame": {"type": "radial"},
            "kind": {"type": "nozzle", "nps": "2", "class": "150", "flange": "wn", "projection_mm": 150}}}),
    );
    let before = s.doc.clone();
    let e = s.execute("vessel.regenerate", &json!({})).unwrap_err().to_string();
    assert!(e.contains("not supported yet"), "{e}");
    assert!(std::sync::Arc::ptr_eq(&before, &s.doc) || *before == *s.doc);
    assert!(s.model.state().bodies.is_empty());
}

/// The template for every element's analytic-volume test (PLAN T0.5): the example script that
/// `triya-cli run` exports, replayed here and measured against the closed-form volume.
#[test]
fn cylinder_volume_matches_analytic() {
    registered();
    let script: Value = serde_json::from_str(include_str!("../examples/cylinder.json")).unwrap();
    let mut s = Session::default();
    s.run_script(&script).unwrap();
    let st = s.model.state();
    assert_eq!(st.bodies.len(), 1);
    let m = solidcraft_engine::kernel::measure(&st.bodies[0].body).unwrap();
    // 1000 ID × 10 t × 2000 L shell: π (510² − 500²) · 2000.
    let expected = std::f64::consts::PI * (510.0f64.powi(2) - 500.0f64.powi(2)) * 2000.0;
    // 0.1 %: the revolve is exact geometry; the kernel's measure integrates a tessellation.
    assert!((m.volume - expected).abs() / expected < 1e-3, "volume {} vs {expected}", m.volume);
}
