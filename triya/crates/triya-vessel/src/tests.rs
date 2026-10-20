// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Tests: the plan for a plain cylinder, the vessel commands under hostile parameters (no
//! panic, as upstream's `hostile_params_never_panic`), and a real regeneration measured.

use crate::model::{Element, ElementKind, HeadPresetId, MAX_EXTRA_BYTES, VesselDoc};
use crate::{commands, emit, eval};
use serde_json::{Value, json};
use solidcraft_engine::{EngineError, Session};

/// The commands are registered once per test process (the registry is global and idempotent).
fn registered() {
    solidcraft_engine::cmd::triya_extra::register(commands::COMMANDS).unwrap();
}

fn run(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn head(id: &str, preset: HeadPresetId, sf: Option<f64>) -> Element {
    Element {
        id: id.into(),
        kind: ElementKind::Head { preset, straight_flange_mm: sf, crown_radius_mm: None, knuckle_radius_mm: None, orientation: None },
        thickness_mm: 10.0,
        diameter_mm: Some(1000.0),
        material: None,
        description: None,
    }
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
fn heads_resolve_and_plan() {
    let mut doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 2000.0);
    doc.elements.insert(0, head("H1", HeadPresetId::Ellipsoidal21, Some(40.0)));
    doc.global.datum = crate::model::Datum::BottomTl;
    let r = eval::resolve(&doc).unwrap();
    let h = r.station("H1").unwrap();
    // The datum is the tangent line: the dish below it, the straight flange above.
    assert!((h.z_from + 250.0).abs() < 1e-9 && (h.z_to - 40.0).abs() < 1e-9 && h.tl == Some(0.0), "{h:?}");
    assert_eq!(r.station("S1").map(|s| (s.z_from, s.z_to)), Some((40.0, 2040.0)));
    // The head's radius: full in the straight flange, the semi-ellipse's at its crown.
    assert!((r.radius_at(20.0).unwrap() - 500.0).abs() < 1e-9);
    assert!(r.radius_at(-249.9).unwrap() < 20.0);
    assert!((r.radius_at(-125.0).unwrap() - 500.0 * (0.75f64).sqrt()).abs() < 1e-6);
    // The 2:1 head is two quarter-ellipse conics (inside and outside) in its profile.
    let plan = emit::plan(&r).unwrap();
    assert_eq!(plan.iter().filter(|c| c.id == "sketch.conic").count(), 2);

    // A torispherical head's profile: continuous, monotone, zero at the crown.
    let mut tori = VesselDoc::cylinder("S1", 1000.0, 10.0, 100.0);
    tori.elements.push(head("H2", HeadPresetId::KloepperDin28011, Some(0.0)));
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
    doc.elements[0].diameter_mm = Some(1000.0);
    doc.elements[0].kind = ElementKind::Cylinder { length_mm: 0.0 };
    assert!(doc.validate().is_err());
    let empty = VesselDoc::new(None);
    assert!(eval::resolve(&empty).is_err());
    let mut outside = VesselDoc::cylinder("S1", 20.0, 10.0, 100.0);
    outside.global.diameter_basis = crate::model::DiameterBasis::Outside;
    assert!(eval::resolve(&outside).is_err(), "no inside diameter left");
}

/// A value nested `n` arrays deep.
fn deep(n: usize) -> Value {
    (0..n).fold(json!(1), |v, _| json!([v]))
}

/// Every vessel command, every documented key, every hostile value: no panic (an
/// `EngineError::Internal` is a panic the engine caught).
#[test]
fn vessel_commands_never_panic_on_hostile_params() {
    registered();
    // Every `vessel.*` command the engine knows is one of ours, and each is exercised below.
    let known: Vec<&str> = solidcraft_engine::command_specs().iter().map(|c| c.id).filter(|id| id.starts_with("vessel.")).collect();
    let ours: Vec<&str> = commands::COMMANDS.iter().map(|c| c.id).collect();
    assert_eq!(known, ours, "a vessel command outside COMMANDS would escape this test");
    let keys = [
        "doc",
        "diameter_mm",
        "thickness_mm",
        "length_mm",
        "name",
        "replace",
        "element",
        "attachment",
        "global",
        "remove",
        "schema",
        "elements",
        "attachments",
        "chunks",
        "generated",
        "features",
        "group",
        "kind",
        "index",
        "before",
        "after",
        "pair",
        "format",
        "patch",
        "datum",
        "plant_elevation_mm",
        "keep_positions",
        "join_nozzles",
    ];
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
        json!({"schema": "triya.vessel/0.1", "elements": [{"id": "", "kind": "cylinder"}]}),
        json!({"schema": "triya.vessel/0.1", "elements": [{"id": "a", "kind": "cylinder", "diameter_mm": 1e308, "thickness_mm": 1, "length_mm": 1}]}),
        json!({"schema": "triya.vessel/0.1", "elements": [{"id": "a", "kind": "head", "preset": "custom_torispherical", "crown_radius_mm": 1, "knuckle_radius_mm": 4.9, "diameter_mm": 10, "thickness_mm": 1}]}),
        json!({"id": "S1", "kind": "cone", "diameter_small_mm": -5, "diameter_large_mm": 100, "thickness_mm": 5, "length_mm": 50}),
        json!({"id": "N1", "kind": "nozzle", "host": "S1", "axial": {"ref": "host_from", "mm": -1e300}, "angle_deg": 0, "frame": {"type": "radial"}, "nps": "2", "projection": {"from": "cl", "mm": 150}}),
        json!({"id": "S1", "kind": "cylinder", "diameter_mm": 1000, "thickness_mm": 10, "length_mm": 1e7}),
        json!({"id": "F1", "kind": "body_flange", "thickness_mm": 10, "explicit": {"od_mm": 1, "bolt_circle_mm": 1e9, "bolt_count": 4_000_000_000u64, "bolt_hole_mm": -1, "thickness_mm": 0, "hub_od_mm": 0, "hub_len_mm": 0}}),
        json!({"schema": "triya.vessel/0.1", "elements": vec![json!({"id": "x"}); 5000]}),
        json!({"schema": "triya.vessel/0.1", "attachments": vec![json!(1); 5000]}),
        json!("x".repeat(70 * 1024)),
        deep(300),
    ];
    let broken = |head: Value, chunk: Option<Value>| {
        move |s: &mut Session| {
            let _ = s.execute("vessel.new", &json!({}));
            let mut m = s.doc.metadata.clone();
            let _ = m.set(commands::METADATA_KEY, head.clone());
            if let Some(c) = &chunk {
                let _ = m.set(&commands::chunk_key(1), c.clone());
            }
            s.doc_mut().metadata = m;
        }
    };
    let states: Vec<Box<dyn Fn(&mut Session)>> = vec![
        Box::new(|_: &mut Session| {}),
        Box::new(|s: &mut Session| {
            let _ = s.execute("vessel.new", &json!({}));
        }),
        Box::new(broken(json!({"chunks": 99}), None)),
        Box::new(broken(json!({"chunks": 2}), Some(json!({"text": "{\"schema\""})))),
        Box::new(broken(json!({"chunks": 1}), Some(json!({"text": 7})))),
        Box::new(broken(json!({"doc": {"schema": "triya.vessel/0.1", "elements": "no"}}), None)),
        Box::new(broken(
            json!({"doc": VesselDoc::cylinder("S1", 1000.0, 10.0, 2000.0).to_value().unwrap(), "generated": {"features": [1, 2, u64::MAX, -1, "x"], "group": u64::MAX}}),
            None,
        )),
    ];
    let mut internal = Vec::new();
    for spec in commands::COMMANDS {
        for v in &hostile {
            for k in keys {
                for (label, p) in [("keyed", json!({ k: v.clone() })), ("whole", v.clone())] {
                    // With no vessel, a good one, and stored data someone broke.
                    for (state, prepare) in states.iter().enumerate() {
                        let mut s = Session::default();
                        prepare(&mut s);
                        if let Err(EngineError::Internal(id, msg)) = s.execute(spec.id, &p) {
                            internal.push(format!("{id} {label} state {state} {p}: {msg}"));
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
    assert_eq!(stored["doc"]["title"], json!("Drum"));
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
    run(&mut s, "vessel.set", json!({"element": {"id": "S1", "kind": "cylinder", "diameter_mm": 800, "thickness_mm": 8, "length_mm": 1000}}));
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
fn regenerate_refuses_a_misplaced_nozzle_and_warns_about_attachments_it_does_not_build() {
    registered();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({}));
    // A hillside offset that puts the neck outside the shell: an error, nothing built.
    run(
        &mut s,
        "vessel.set",
        json!({"attachment": {"id": "N1", "kind": "nozzle", "host": "S1", "axial": {"ref": "host_from", "mm": 500}, "angle_deg": 90,
            "frame": {"type": "hillside", "offset_mm": 495}, "nps": "2", "schedule": "XS", "projection": {"from": "od", "mm": 150}}}),
    );
    let before = s.doc.clone();
    let e = s.execute("vessel.regenerate", &json!({})).unwrap_err().to_string();
    assert!(e.contains("nozzle `N1`") && e.contains("outside the shell"), "{e}");
    assert!(std::sync::Arc::ptr_eq(&before, &s.doc) || *before == *s.doc);
    assert!(s.model.state().bodies.is_empty());
    // A support that cannot be built yet (channel legs) is reported, not refused.
    run(&mut s, "vessel.set", json!({"remove": "N1"}));
    run(
        &mut s,
        "vessel.set",
        json!({"attachment": {"id": "L1", "kind": "leg", "host": "S1", "axial": {"ref": "host_from", "mm": 200}, "count": 3,
            "section": {"channel": {"size": "ISMC 150"}}, "length_mm": 1800}}),
    );
    let r = run(&mut s, "vessel.regenerate", json!({}));
    assert!(r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap_or("").contains("`L1` (leg): not built yet")), "{r}");
    assert_eq!(body_names(&s), vec!["Vessel S1".to_string()]);
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

/// The reference vessels of `docs/beta-dod.md` §B, and a fixture holding every kind they leave out.
const REFERENCE: [(&str, &str); 4] = [
    ("vessel-a", include_str!("../tests/reference/vessel-a.json")),
    ("vessel-b", include_str!("../tests/reference/vessel-b.json")),
    ("vessel-c", include_str!("../tests/reference/vessel-c.json")),
    ("every-kind", include_str!("../tests/reference/every-kind.json")),
];

/// Every field `input` names is in `output`, with the same value (numbers compared as f64).
fn assert_kept(input: &Value, output: &Value, path: &str) {
    match (input, output) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in a {
                let o = b.get(k).unwrap_or_else(|| panic!("{path}.{k} was dropped"));
                assert_kept(v, o, &format!("{path}.{k}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: length");
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                assert_kept(x, y, &format!("{path}[{i}]"));
            }
        }
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{path}"),
        _ => assert_eq!(input, output, "{path}"),
    }
}

/// PLAN T2.1: the three reference vessels (and the every-kind fixture) parse, validate and
/// serialise back with every field they had, and a second trip changes nothing.
#[test]
fn reference_vessels_round_trip() {
    let mut element_kinds = std::collections::BTreeSet::new();
    let mut attachment_kinds = std::collections::BTreeSet::new();
    let mut frames = std::collections::BTreeSet::new();
    for (name, text) in REFERENCE {
        let doc = VesselDoc::from_json(text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let input: Value = serde_json::from_str(text).unwrap();
        let out = doc.to_value().unwrap();
        assert_kept(&input, &out, name);
        let back = VesselDoc::from_value(&out).unwrap();
        assert_eq!(back, doc, "{name}");
        assert_eq!(back.to_value().unwrap(), out, "{name}: a second trip is stable");
        element_kinds.extend(doc.elements.iter().map(|e| e.kind.name()));
        attachment_kinds.extend(doc.attachments.iter().map(|a| a.kind.name()));
        frames.extend(doc.attachments.iter().map(|a| serde_json::to_value(a.frame).unwrap()["type"].as_str().unwrap_or_default().to_string()));
    }
    let all_elements = ["body_flange", "cone", "cylinder", "flat_cover", "head", "skirt"];
    assert_eq!(element_kinds.into_iter().collect::<Vec<_>>(), all_elements);
    let all_attachments =
        ["anchor_chair", "bracket", "clip", "insulation_ring", "leg", "lifting_lug", "limpet_coil", "nozzle", "saddle", "stiffening_ring"];
    assert_eq!(attachment_kinds.into_iter().collect::<Vec<_>>(), all_attachments);
    let all_frames = ["head_radial_at_r", "hillside", "parallel_to_axis", "radial", "tangential", "tilted"];
    assert_eq!(frames.iter().map(String::as_str).collect::<Vec<_>>(), all_frames);
    // Unknown top-level fields survive.
    let fixture = VesselDoc::from_json(REFERENCE[3].1).unwrap();
    assert_eq!(fixture.unknown.get("a_future_field"), Some(&json!({"kept": true})));
}

#[test]
fn head_presets_use_the_schema_names() {
    let names: Vec<Value> = HeadPresetId::ALL.iter().map(|p| serde_json::to_value(p).unwrap()).collect();
    assert_eq!(
        names,
        [
            "asme_fd",
            "ellipsoidal_2_1",
            "ellipsoidal_2_1_ug32_approx",
            "kloepper_din28011",
            "korbbogen_din28013",
            "torispherical_80_10",
            "hemispherical",
            "custom_torispherical"
        ]
        .map(|s| json!(s))
    );
    for p in HeadPresetId::ALL {
        let back: HeadPresetId = serde_json::from_value(serde_json::to_value(p).unwrap()).unwrap();
        assert_eq!(back, p);
        assert_eq!(p.geometry().is_none(), p == HeadPresetId::CustomTorispherical);
    }
}

/// A reference vessel goes into the design (`vessel.new {doc}`) and comes back out of the
/// saved design unchanged.
#[test]
fn reference_vessel_survives_the_design_file() {
    registered();
    let doc: Value = serde_json::from_str(REFERENCE[0].1).unwrap();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": doc}));
    let text = s.doc.to_json();
    let back = solidcraft_engine::doc::Document::from_json(&text).unwrap();
    let stored = back.metadata.get(commands::METADATA_KEY).map(|v| v["doc"].clone()).unwrap_or_default();
    assert_eq!(VesselDoc::from_value(&stored).unwrap(), VesselDoc::from_json(REFERENCE[0].1).unwrap());
}

#[test]
fn documents_that_break_the_rules_are_refused() {
    let a: Value = serde_json::from_str(REFERENCE[0].1).unwrap();
    let refused = |patch: &dyn Fn(&mut Value)| {
        let mut v = a.clone();
        patch(&mut v);
        VesselDoc::from_value(&v).is_err()
    };
    assert!(refused(&|v| v["elements"][4]["diameter_mm"] = json!(1600)), "a cone with diameter_mm");
    assert!(refused(&|v| v["elements"][4]["diameter_small_mm"] = json!(3000)), "small end larger than large end");
    assert!(refused(&|v| v["elements"][1]["crown_radius_mm"] = json!(1600)), "radii on an ellipsoidal head");
    assert!(refused(&|v| v["elements"][1]["preset"] = json!("klopper")), "unknown preset");
    assert!(refused(&|v| v["attachments"][0]["host"] = json!("E99")), "unknown host");
    assert!(refused(&|v| v["attachments"][0]["id"] = json!("E2")), "id shared with an element");
    assert!(refused(&|v| v["attachments"][9]["end_axial_mm"] = json!(3000)), "coil with turns and end_axial_mm");
    assert!(refused(&|v| v["attachments"][3]["frame"]["tilt_deg"] = json!(90)), "tilt of 90°");
    assert!(refused(&|v| v["global"]["datum"] = json!("left_tl")), "left_tl on a vertical vessel");
    assert!(refused(&|v| v["design_data"]["joint_eff"] = json!(1.5)), "joint efficiency above 1");
    assert!(refused(&|v| v["schema"] = json!("triya.vessel/0.2")), "another schema");
    assert!(refused(&|v| v["extra"] = json!({"blob": "x".repeat(MAX_EXTRA_BYTES)})), "oversized extra");
    assert!(!refused(&|_| {}), "the reference vessel itself is fine");
}

/// `vessel-schema.json` is generated from the model (`TRIYA_BLESS=1` rewrites it).
#[test]
fn schema_file_is_current() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/vessel-schema.json");
    let text = serde_json::to_string_pretty(&VesselDoc::json_schema()).unwrap() + "\n";
    if std::env::var_os("TRIYA_BLESS").is_some() {
        std::fs::write(path, &text).unwrap();
    }
    let on_disk = std::fs::read_to_string(path).unwrap_or_default();
    assert!(on_disk == text, "vessel-schema.json is stale: TRIYA_BLESS=1 cargo test -p triya-vessel schema_file_is_current");
    let schema: Value = serde_json::from_str(&on_disk).unwrap();
    assert_eq!(schema["title"], json!("VesselDoc"));
    assert!(on_disk.contains("\"ellipsoidal_2_1_ug32_approx\"") && on_disk.contains("\"head_radial_at_r\""));
}

// ---------------------------------------------------------------------------------------------
// T2.2: the evaluator

fn reference(i: usize) -> VesselDoc {
    VesselDoc::from_json(REFERENCE[i].1).unwrap()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// PLAN T2.2: vessel A's seven elements against a hand calculation (datum: bottom TL).
///
/// E1 2:1 head ID 1600: dish 400, SF 40 → −400 … 40, TL 0. E2 40 … 3040, E3 3040 … 5540,
/// E4 cone 5540 … 6340 (r 800 → 1100), E5 6340 … 8340. E6 ASME F&D on ID 2200 (L 2200,
/// r = max(0.06·2200, 3·14) = 132): h = 2200 − √(2068² − 968²) = 372.543; SF 50 → TL 8390,
/// crown 8762.543. E0 skirt: top at the TL, 2500 down, OD = the head's OD 1624 → r_i 802.
#[test]
fn stations_of_a_seven_element_vertical_vessel_match_hand_calc() {
    let r = eval::resolve(&reference(0)).unwrap();
    let got: Vec<(&str, f64, f64, f64, f64)> = r.stations.iter().map(|s| (s.id.as_str(), s.z_from, s.z_to, s.r_from, s.r_to)).collect();
    let h6 = 2200.0 - (2068.0f64.powi(2) - 968.0f64.powi(2)).sqrt();
    assert!(close(h6, 372.543, 1e-3), "{h6}");
    let want = [
        ("E0", -2500.0, 0.0, 802.0, 802.0),
        ("E1", -400.0, 40.0, 800.0, 800.0),
        ("E2", 40.0, 3040.0, 800.0, 800.0),
        ("E3", 3040.0, 5540.0, 800.0, 800.0),
        ("E4", 5540.0, 6340.0, 800.0, 1100.0),
        ("E5", 6340.0, 8340.0, 1100.0, 1100.0),
        ("E6", 8340.0, 8390.0 + h6, 1100.0, 1100.0),
    ];
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g.0, w.0);
        for (a, b) in [(g.1, w.1), (g.2, w.2), (g.3, w.3), (g.4, w.4)] {
            assert!(close(a, b, 1e-6), "{}: got {g:?}, want {w:?}", w.0);
        }
    }
    assert_eq!(r.tl_bottom, Some(0.0));
    assert!(close(r.tl_top.unwrap(), 8390.0, 1e-9));
    assert!(close(r.tl_to_tl().unwrap(), 8390.0, 1e-9));
    assert!(close(r.z_min, -2500.0, 1e-9) && close(r.z_max, 8390.0 + h6, 1e-6));
    // The skirt is outside the pressure envelope: at −100 the radius is the bottom head's.
    assert!(r.radius_at(-100.0).unwrap() < 800.0);
    // The toriconical cone: half-apex from the knuckle-tangent line, 21.00° (20.56° without the
    // knuckle), no warning below 30°.
    let cone = r.station("E4").unwrap();
    assert!(close(cone.half_apex_deg.unwrap(), 21.002, 1e-3), "{:?}", cone.half_apex_deg);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    // radius_at along the cone: the cylinder's at the large end, tangent knuckle, the line, the small end.
    assert!(close(r.radius_at(6340.0).unwrap(), 1100.0, 1e-6));
    assert!(close(cone.radius_at(5540.0).unwrap(), 800.0, 1e-6));
    let mut last = 0.0;
    for i in 0..=100 {
        let x = cone.radius_at(5540.0 + 8.0 * f64::from(i)).unwrap();
        assert!(x >= last - 1e-9, "the cone widens monotonically");
        last = x;
    }
}

#[test]
fn datums_move_the_zero() {
    let mut doc = reference(0);
    let tl = |doc: &VesselDoc| eval::resolve(doc).unwrap().tl_bottom.unwrap();
    doc.global.datum = crate::model::Datum::BottomSeam;
    assert!(close(tl(&doc), -40.0, 1e-9));
    doc.global.datum = crate::model::Datum::SkirtBase;
    assert!(close(tl(&doc), 2500.0, 1e-9));
    doc.global.datum = crate::model::Datum::PlantElevation;
    assert!(close(tl(&doc), 300.0, 1e-9), "the bottom TL reads plant_elevation_mm");
    // Without a skirt, skirt_base is an error.
    let mut c = reference(2);
    c.global.datum = crate::model::Datum::SkirtBase;
    assert!(eval::resolve(&c).is_err());
    // A horizontal vessel: left TL at 0, TL–TL 5000 (vessel B).
    let b = eval::resolve(&reference(1)).unwrap();
    assert_eq!(b.orientation, crate::model::Orientation::Horizontal);
    assert_eq!(b.tl_bottom, Some(0.0));
    assert!(close(b.tl_to_tl().unwrap(), 5000.0, 1e-9), "{:?}", b.tl_to_tl());
}

/// A change of E2's diameter flows to E3 and to the cone's near end, and stops there.
#[test]
fn diameter_inheritance_stops_at_the_cone() {
    let mut doc = reference(0);
    doc.elements[2].diameter_mm = Some(1500.0);
    let r = eval::resolve(&doc).unwrap();
    assert_eq!(r.station("E3").map(|s| s.r_from), Some(750.0));
    assert_eq!(r.station("E5").map(|s| s.r_from), Some(1100.0));
    assert_eq!(r.station("E6").map(|s| s.r_from), Some(1100.0));
    assert!(r.warnings.iter().any(|w| w.contains("E4") && w.contains("does not meet")), "{:?}", r.warnings);
    // E5 without a diameter inherits the cone's far end.
    let mut doc = reference(0);
    doc.elements[5].diameter_mm = None;
    assert_eq!(eval::resolve(&doc).unwrap().station("E5").map(|s| s.r_from), Some(1100.0));
    // The first shell element must state its diameter.
    let mut doc = reference(2);
    doc.elements[0].diameter_mm = None;
    assert!(eval::resolve(&doc).is_err());
}

#[test]
fn outside_basis_and_steep_cones() {
    let mut doc = VesselDoc::cylinder("S1", 1020.0, 10.0, 1000.0);
    doc.global.diameter_basis = crate::model::DiameterBasis::Outside;
    doc.elements.push(Element {
        id: "K1".into(),
        kind: ElementKind::Cone {
            length_mm: 200.0,
            diameter_small_mm: 420.0,
            diameter_large_mm: 1020.0,
            knuckle_large_mm: None,
            knuckle_small_mm: None,
        },
        thickness_mm: 10.0,
        diameter_mm: None,
        material: None,
        description: None,
    });
    let r = eval::resolve(&doc).unwrap();
    assert_eq!(r.station("S1").map(|s| s.r_from), Some(500.0));
    assert_eq!(r.station("K1").map(|s| (s.r_from, s.r_to)), Some((500.0, 200.0)));
    // atan(300 / 200) = 56.3°: warned.
    assert!(r.warnings.iter().any(|w| w.contains("half-apex")), "{:?}", r.warnings);
    assert!(r.tl_bottom.is_none() && r.tl_top.is_none());
}

/// DIN worked examples (Da 1000, s 10): the exact geometry Triya uses is within 0.5 mm of the
/// published fits h2 = 0.1935 Da − 0.455 s = 188.95 (Klöpper) and 0.255 Da − 0.635 s = 248.65
/// (Korbbogen); exact: 188.89 and 248.26.
#[test]
fn head_depths_match_the_din_worked_examples() {
    for (preset, fit, exact) in [(HeadPresetId::KloepperDin28011, 188.95, 188.889), (HeadPresetId::KorbbogenDin28013, 248.65, 248.26)] {
        let mut doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 500.0);
        doc.global.diameter_basis = crate::model::DiameterBasis::Outside;
        let mut h = head("H1", preset, Some(35.0));
        h.diameter_mm = None;
        doc.elements.push(h);
        let r = eval::resolve(&doc).unwrap();
        let st = r.station("H1").unwrap();
        let crate::eval::Profile::Head { dish, sf, .. } = st.profile else { panic!("{:?}", st.profile) };
        assert!(close(dish, fit, 0.5), "{preset:?}: {dish} vs DIN {fit}");
        assert!(close(dish, exact, 0.01), "{preset:?}: {dish} vs exact {exact}");
        assert_eq!(sf, 35.0);
        assert!(close(st.length(), 35.0 + dish, 1e-9));
    }
    // The default straight flange: 3.5 s for Klöpper, 3 t otherwise.
    let mut doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 500.0);
    doc.elements.push(head("H1", HeadPresetId::KloepperDin28011, None));
    doc.elements.push(head("H0", HeadPresetId::AsmeFd, None));
    assert!(eval::resolve(&doc).is_err(), "two heads at the top");
    doc.elements.pop();
    doc.elements.insert(0, head("H0", HeadPresetId::AsmeFd, None));
    let r = eval::resolve(&doc).unwrap();
    let sf = |id: &str| match r.station(id).map(|s| s.profile) {
        Some(crate::eval::Profile::Head { sf, .. }) => sf,
        other => panic!("{other:?}"),
    };
    assert_eq!((sf("H0"), sf("H1")), (30.0, 35.0));
    // ASME F&D's knuckle is at least 3 t: ID 200, t 10 → r = 30, not 12.
    let (shape, _) = eval::head_dish(HeadPresetId::AsmeFd, 200.0, 10.0, None, None).unwrap();
    assert_eq!(shape, crate::eval::HeadShape::Torispherical { crown_r: 200.0, knuckle_r: 30.0 });
}

#[test]
fn heads_in_the_wrong_place_are_errors() {
    let mut doc = reference(2);
    doc.elements.swap(1, 2);
    assert!(eval::resolve(&doc).is_err(), "a head in the middle");
    let mut doc = reference(2);
    if let ElementKind::Head { orientation, .. } = &mut doc.elements[0].kind {
        *orientation = Some(crate::model::HeadOrientation::Up);
    }
    assert!(eval::resolve(&doc).is_err(), "a bottom head pointing up");
    let mut doc = reference(0);
    doc.elements.push(doc.elements[0].clone());
    doc.elements.last_mut().unwrap().id = "E9".into();
    assert!(eval::resolve(&doc).is_err(), "a second skirt");
    // Every reference vessel resolves (the every-kind fixture names table rows the embedded
    // tables do not have).
    for (name, text) in REFERENCE.iter().take(3) {
        eval::resolve(&VesselDoc::from_json(text).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

// ---------------------------------------------------------------------------------------------
// T2.3: heads, cones and flat covers built through the engine, measured against closed forms

use std::f64::consts::PI;

/// Inside volume of a torispherical dish (tangent line to crown) of radius `r_s`, crown `l`,
/// knuckle `r`: the knuckle's torus segment plus the spherical zone, integrated in closed form.
fn tori_dish_volume(r_s: f64, l: f64, r: f64) -> f64 {
    let (a, b) = (l - r, r_s - r);
    let c = (a * a - b * b).sqrt();
    let s_t = r * c / a;
    let h = l - c;
    let knuckle = (b * b + r * r) * s_t - s_t.powi(3) / 3.0 + b * (s_t * (r * r - s_t * s_t).sqrt() + r * r * (s_t / r).asin());
    let crown = l * l * (h - s_t) - ((h + c).powi(3) - (s_t + c).powi(3)) / 3.0;
    PI * (knuckle + crown)
}

/// A 1000 ID × 10 shell with `top` as its top element, regenerated; the top's body measured.
fn build_and_measure(top: Element, global: Option<Value>) -> (f64, eval::Resolved) {
    registered();
    let mut doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 500.0);
    doc.elements.push(top);
    if let Some(g) = global {
        doc.global = serde_json::from_value(g).unwrap();
    }
    let id = doc.elements[1].id.clone();
    let resolved = eval::resolve(&doc).unwrap();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": doc.to_value().unwrap()}));
    run(&mut s, "vessel.regenerate", json!({}));
    let st = s.model.state();
    let body = st.bodies.iter().find(|b| b.name == format!("Vessel {id}")).unwrap_or_else(|| panic!("no body for {id}"));
    (solidcraft_engine::kernel::measure(&body.body).unwrap().volume, resolved)
}

fn within(measured: f64, analytic: f64, what: &str) {
    let rel = (measured - analytic) / analytic;
    assert!(rel.abs() < 1e-3, "{what}: measured {measured:.1} vs analytic {analytic:.1} ({:+.4} %)", rel * 100.0);
}

fn head_el(preset: HeadPresetId, sf: f64, crown: Option<f64>, knuckle: Option<f64>) -> Element {
    Element {
        id: "H1".into(),
        kind: ElementKind::Head { preset, straight_flange_mm: Some(sf), crown_radius_mm: crown, knuckle_radius_mm: knuckle, orientation: None },
        thickness_mm: 10.0,
        diameter_mm: None,
        material: None,
        description: None,
    }
}

#[test]
fn the_torispherical_closed_form_agrees_with_the_handbook() {
    // ASME F&D inside volume ≈ 0.0809 D³ (common handbook coefficient), D = 1000, r = 0.06 D.
    let v = tori_dish_volume(500.0, 1000.0, 60.0);
    assert!((v / 1e9 - 0.0809).abs() < 0.0809 * 0.01, "{}", v / 1e9);
    // A hemisphere is the limit r = L = R.
    let hemi = tori_dish_volume(500.0, 500.0 + 1e-9, 500.0 - 1e-9);
    assert!((hemi - 2.0 / 3.0 * PI * 500f64.powi(3)).abs() / hemi < 1e-6);
}

#[test]
fn ellipsoidal_and_hemispherical_heads_match_analytic_volume() {
    let sf = 40.0;
    let ring = PI * (510f64.powi(2) - 500f64.powi(2)) * sf;
    let (v, _) = build_and_measure(head_el(HeadPresetId::Ellipsoidal21, sf, None, None), None);
    within(v, 2.0 / 3.0 * PI * (510f64.powi(2) * 260.0 - 500f64.powi(2) * 250.0) + ring, "2:1 ellipsoidal");
    let (v, _) = build_and_measure(head_el(HeadPresetId::Hemispherical, sf, None, None), None);
    within(v, 2.0 / 3.0 * PI * (510f64.powi(3) - 500f64.powi(3)) + ring, "hemispherical");
}

#[test]
fn torispherical_heads_match_analytic_volume() {
    let sf = 30.0;
    let ring = PI * (510f64.powi(2) - 500f64.powi(2)) * sf;
    let t = 10.0;
    for (preset, crown, knuckle) in [
        (HeadPresetId::AsmeFd, 1000.0, 60.0),
        (HeadPresetId::Ellipsoidal21Ug32Approx, 900.0, 170.0),
        (HeadPresetId::KloepperDin28011, 1020.0, 102.0),
        (HeadPresetId::KorbbogenDin28013, 816.0, 157.08),
        (HeadPresetId::Torispherical8010, 800.0, 100.0),
        (HeadPresetId::CustomTorispherical, 950.0, 120.0),
    ] {
        let custom = preset == HeadPresetId::CustomTorispherical;
        let el = head_el(preset, sf, custom.then_some(crown), custom.then_some(knuckle));
        let (v, r) = build_and_measure(el, None);
        let crate::eval::Profile::Head { shape, .. } = r.station("H1").unwrap().profile else { panic!() };
        let crate::eval::HeadShape::Torispherical { crown_r, knuckle_r } = shape else { panic!("{preset:?}: {shape:?}") };
        assert!(close(crown_r, crown, 1e-9) && close(knuckle_r, knuckle, 1e-9), "{preset:?}: {shape:?}");
        let metal = tori_dish_volume(500.0 + t, crown + t, knuckle + t) - tori_dish_volume(500.0, crown, knuckle);
        within(v, metal + ring, &format!("{preset:?}"));
    }
}

fn cone_el(knuckle_large: Option<f64>, knuckle_small: Option<f64>) -> Element {
    Element {
        id: "K1".into(),
        kind: ElementKind::Cone {
            length_mm: 600.0,
            diameter_small_mm: 600.0,
            diameter_large_mm: 1000.0,
            knuckle_large_mm: knuckle_large,
            knuckle_small_mm: knuckle_small,
        },
        thickness_mm: 10.0,
        diameter_mm: None,
        material: None,
        description: None,
    }
}

/// π ∫ (x_o² − x_i²) du over the cone, the outside being the concentric offset of the inside
/// (large knuckle + t, small knuckle − t, line + t), by Simpson's rule on 20 000 intervals.
fn cone_volume(r_l: f64, r_s: f64, k_l: f64, k_s: f64, len: f64, t: f64, alpha: f64) -> f64 {
    let (sa, ca) = alpha.sin_cos();
    let c = (r_l - k_l) * ca + k_l;
    let x = |u: f64, off: f64| {
        let (kl, ks) = (k_l + off, k_s - off);
        if k_l > 0.0 && u <= kl * sa {
            (r_l - k_l) + (kl * kl - u * u).sqrt()
        } else if k_s > 0.0 && u >= len - ks * sa {
            (r_s + k_s) - (ks * ks - (len - u).powi(2)).sqrt()
        } else {
            (c + off - u * sa) / ca
        }
    };
    let n = 20_000;
    let hstep = len / f64::from(n);
    let f = |u: f64| x(u, t).powi(2) - x(u, 0.0).powi(2);
    let mut sum = f(0.0) + f(len);
    for i in 1..n {
        sum += f(f64::from(i) * hstep) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    PI * sum * hstep / 3.0
}

#[test]
fn cones_match_analytic_volume() {
    // A bare cone: the outside is the offset line, so both surfaces are frustums.
    let (v, r) = build_and_measure(cone_el(None, None), None);
    let st = r.station("K1").unwrap();
    let alpha = st.half_apex_deg.unwrap().to_radians();
    assert!((alpha - (200.0f64 / 600.0).atan()).abs() < 1e-12);
    let frustum = |a: f64, b: f64| PI * 600.0 / 3.0 * (a * a + a * b + b * b);
    let off = 10.0 / alpha.cos();
    within(v, frustum(500.0 + off, 300.0 + off) - frustum(500.0, 300.0), "bare cone");
    assert!((cone_volume(500.0, 300.0, 0.0, 0.0, 600.0, 10.0, alpha) - (frustum(500.0 + off, 300.0 + off) - frustum(500.0, 300.0))).abs() < 1.0);
    // Toriconical: knuckles at both ends, then at the large end only.
    for (kl, ks) in [(Some(80.0), Some(50.0)), (Some(100.0), None)] {
        let (v, r) = build_and_measure(cone_el(kl, ks), None);
        let alpha = r.station("K1").unwrap().half_apex_deg.unwrap().to_radians();
        within(v, cone_volume(500.0, 300.0, kl.unwrap_or(0.0), ks.unwrap_or(0.0), 600.0, 10.0, alpha), &format!("toriconical {kl:?} {ks:?}"));
    }
}

#[test]
fn flat_cover_matches_analytic_volume() {
    let cover = Element {
        id: "F1".into(),
        kind: ElementKind::FlatCover { bolted: false },
        thickness_mm: 40.0,
        diameter_mm: None,
        material: None,
        description: None,
    };
    // A disc out to the shell's outside radius 510, 40 thick.
    let (v, r) = build_and_measure(cover, None);
    assert_eq!(r.station("F1").map(|s| s.r_from), Some(510.0));
    within(v, PI * 510f64.powi(2) * 40.0, "flat cover");
}

/// Phase 2 exit (without attachments): vessel A's shell regenerates into seven bodies, its
/// dimensions land in the parameter table, and a second regeneration replaces them.
#[test]
fn vessel_a_shell_regenerates() {
    registered();
    let mut doc = reference(0);
    doc.attachments.clear();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": doc.to_value().unwrap()}));
    let r = run(&mut s, "vessel.regenerate", json!({}));
    let names: Vec<String> = s.model.state().bodies.iter().map(|b| b.name.clone()).collect();
    assert_eq!(names.len(), 7, "{names:?}");
    for e in &doc.elements {
        assert!(names.contains(&format!("Vessel {}", e.id)), "{names:?}");
    }
    let params: Vec<&str> = r["parameters"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(params.contains(&"V101_E2_length") && params.contains(&"V101_E6_depth") && params.contains(&"V101_E4_half_apex"), "{params:?}");
    let list = run(&mut s, "parameters.list", json!({"filter": "user"}));
    let length = list["parameters"].as_array().unwrap().iter().find(|p| p["name"] == "V101_E2_length").cloned().unwrap();
    assert_eq!(length["value"].as_f64(), Some(3000.0), "{length}");
    run(&mut s, "vessel.regenerate", json!({}));
    assert_eq!(s.model.state().bodies.len(), 7);
    // The F&D top head's body: TL 8390, inside crown 8762.543, outside 14 further.
    let st = s.model.state();
    let top = st.bodies.iter().find(|b| b.name == "Vessel E6").unwrap();
    let m = solidcraft_engine::kernel::measure(&top.body).unwrap();
    assert!((m.bbox.max.z - (8762.543 + 14.0)).abs() < 0.01, "{:?}", m.bbox);
}

/// The captured kernel case (AGENTS.md §5): with plain arcs the revolve's measured volume is
/// far off; the emitter's conic arcs avoid it (the test above). Kept so a kernel fix shows up.
#[test]
fn kernel_case_torispherical_arc_revolve_is_still_wrong_with_plain_arcs() {
    registered();
    let script: Value = serde_json::from_str(include_str!("../tests/kernel-cases/torispherical-arc-revolve.json")).unwrap();
    let mut s = Session::default();
    s.run_script(&script).unwrap();
    let st = s.model.state();
    let v = solidcraft_engine::kernel::measure(&st.bodies[0].body).unwrap().volume;
    let analytic = 12_224_761.0;
    // When this fails the kernel got fixed: the conic workaround in `emit::draw` can go.
    assert!((v - analytic).abs() / analytic > 0.01, "the kernel now revolves this case right ({v})");
}

// ---------------------------------------------------------------------------------------------
// T2.4: body flange and skirt

/// Volume of a polygon (radius, axial) revolved 360°: π Σ (x₀² + x₀x₁ + x₁²)(y₁ − y₀) / 3.
fn revolved_polygon(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    let sum: f64 = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            (a[0] * a[0] + a[0] * b[0] + b[0] * b[0]) * (b[1] - a[1])
        })
        .sum();
    (PI * sum / 3.0).abs()
}

/// Regenerate `doc`, return the session and the named body's volume.
fn regenerate_doc(doc: &VesselDoc, body: &str) -> (Session, f64, Value) {
    registered();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": doc.to_value().unwrap()}));
    let reply = run(&mut s, "vessel.regenerate", json!({}));
    let st = s.model.state();
    let b =
        st.bodies.iter().find(|b| b.name == body).unwrap_or_else(|| panic!("no {body}: {:?}", st.bodies.iter().map(|b| &b.name).collect::<Vec<_>>()));
    let v = solidcraft_engine::kernel::measure(&b.body).unwrap().volume;
    drop(st);
    (s, v, reply)
}

fn flange(id: &str, explicit: Option<Value>, flange_ref: Option<Value>, t: f64) -> Element {
    let mut v = json!({"id": id, "kind": "body_flange", "thickness_mm": t});
    if let Some(e) = explicit {
        v["explicit"] = e;
    }
    if let Some(r) = flange_ref {
        v["flange_ref"] = r;
    }
    serde_json::from_value(v).unwrap()
}

#[test]
fn skirt_with_base_ring_matches_analytic_volume() {
    let doc = reference(0);
    let mut doc2 = doc.clone();
    doc2.attachments.clear();
    doc2.elements.truncate(3);
    let (_, v, _) = regenerate_doc(&doc2, "Vessel E0");
    // Ring 1500/1900 × 20, shell r 802 … 812 from 20 up to 2500.
    let analytic = PI * (950f64.powi(2) - 750f64.powi(2)) * 20.0 + PI * (812f64.powi(2) - 802f64.powi(2)) * 2480.0;
    within(v, analytic, "skirt");
    // A base ring that does not reach the skirt wall is refused.
    let mut bad = doc2.clone();
    if let ElementKind::Skirt { base_ring, .. } = &mut bad.elements[0].kind {
        base_ring.od_mm = 1610.0;
    }
    assert!(eval::resolve(&bad).is_err());
}

#[test]
fn explicit_body_flange_matches_analytic_volume_and_bolt_count() {
    let mut doc = VesselDoc::cylinder("S1", 1000.0, 8.0, 800.0);
    let dims = json!({"od_mm": 1255, "bolt_circle_mm": 1170, "bolt_count": 28, "bolt_hole_mm": 33, "thickness_mm": 60, "hub_od_mm": 1060, "hub_len_mm": 120});
    doc.elements.push(flange("F1", Some(dims.clone()), None, 8.0));
    doc.elements.push(flange("F2", Some(dims), None, 8.0));
    doc.elements.push(Element {
        id: "S2".into(),
        kind: ElementKind::Cylinder { length_mm: 500.0 },
        thickness_mm: 8.0,
        diameter_mm: None,
        material: None,
        description: None,
    });
    let r = eval::resolve(&doc).unwrap();
    let (f1, f2) = (r.station("F1").unwrap(), r.station("F2").unwrap());
    assert_eq!((f1.z_from, f1.z_to, f2.z_to), (800.0, 980.0, 1160.0));
    let hub = |s: &eval::Station| match s.detail {
        eval::Detail::Flange(g) => g.hub_at_start,
        d => panic!("{d:?}"),
    };
    assert!(hub(f1) && !hub(f2), "the pair faces each other");
    let section = [[500.0, 0.0], [508.0, 0.0], [530.0, 120.0], [627.5, 120.0], [627.5, 180.0], [500.0, 180.0]];
    let holes = 28.0 * PI * 16.5f64.powi(2) * 60.0;
    let (s, v, reply) = regenerate_doc(&doc, "Vessel F1");
    within(v, revolved_polygon(&section) - holes, "explicit body flange F1");
    let st = s.model.state();
    let f2_body = st.bodies.iter().find(|b| b.name == "Vessel F2").unwrap();
    within(solidcraft_engine::kernel::measure(&f2_body.body).unwrap().volume, revolved_polygon(&section) - holes, "mating flange F2");
    let patterns: Vec<&Value> = reply["steps"].as_array().unwrap().iter().filter(|x| x["command"] == "solid.pattern.circular").collect();
    assert_eq!(patterns.len(), 2);
    assert!(patterns.iter().all(|p| p["params"]["count"] == json!(28)));
    assert!(reply["parameters"].as_array().unwrap().contains(&json!("V1_F1_bolt_count")));
}

/// A flange from the B16.5 table: NPS 2 Cl 150 WN, 4 × 5/8 in on 120.7 in 19.1 holes, OD 150,
/// ring tf 17.5, hub 78 × 44.5 (length through hub 62 − 17.5). On a 52.5 ID × 3.91 shell.
#[test]
fn table_body_flange_takes_the_rows_bolt_count() {
    let mut doc = VesselDoc::cylinder("S1", 52.5, 3.91, 200.0);
    doc.elements.push(flange("F1", None, Some(json!({"standard": "B16.5", "nps_or_dn": "2", "class": 150, "type": "wn"})), 3.91));
    let tables = triya_stdlib::TableSet::embedded().unwrap();
    let row = tables.flange("2", "150", triya_stdlib::FlangeKind::Wn).unwrap();
    let r = eval::resolve(&doc).unwrap();
    let eval::Detail::Flange(g) = r.station("F1").unwrap().detail else { panic!() };
    assert_eq!((g.bolt_count, g.bolt_hole, g.hub_len), (row.bolt_count, 19.1, 44.5));
    assert!(!r.warnings.iter().any(|w| w.contains("derived") || w.contains("F1")), "{:?}", r.warnings);
    let (_, v, reply) = regenerate_doc(&doc, "Vessel F1");
    let section = [[26.25, 200.0], [30.16, 200.0], [39.0, 244.5], [75.0, 244.5], [75.0, 262.0], [26.25, 262.0]];
    within(v, revolved_polygon(&section) - 4.0 * PI * 9.55 * 9.55 * 17.5, "table body flange");
    let pattern = reply["steps"].as_array().unwrap().iter().find(|x| x["command"] == "solid.pattern.circular").cloned().unwrap();
    assert_eq!(pattern["params"]["count"], json!(row.bolt_count));
    // A row that is not in the tables, and a flange too small for its shell, are errors.
    let mut missing = doc.clone();
    missing.elements[1] = flange("F1", None, Some(json!({"standard": "B16.5", "nps_or_dn": "2", "class": 125, "type": "wn"})), 3.91);
    assert!(eval::resolve(&missing).is_err());
    missing.elements[1] = flange("F1", None, Some(json!({"standard": "EN 1092-1", "nps_or_dn": "2", "class": 150, "type": "wn"})), 3.91);
    assert!(eval::resolve(&missing).is_err());
    let mut big = doc.clone();
    big.elements[0].diameter_mm = Some(100.0);
    assert!(eval::resolve(&big).is_err());
}

// ---- T2.5: regenerate hardening ----------------------------------------------------------

/// A two-element vessel (2:1 head under a course): cheap to build, more than one body.
fn small_vessel() -> VesselDoc {
    let mut doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 1500.0);
    doc.id = "V-7".into();
    doc.elements.insert(0, head("H1", HeadPresetId::Ellipsoidal21, Some(40.0)));
    doc
}

/// A box of the user's own, away from the vessel.
fn user_box(s: &mut Session) -> u64 {
    let sk = run(s, "sketch.create", json!({"plane": "XY", "name": "Mine"}))["sketch"].as_u64().unwrap();
    run(s, "sketch.rectangle.two_point", json!({"p0": [3000, 0], "p1": [3100, 100], "sketch": sk}));
    run(s, "sketch.finish", json!({}));
    run(s, "solid.extrude", json!({"sketch": sk, "distance": 50, "name": "My box", "body_name": "My box"}))["feature"].as_u64().unwrap()
}

fn body_names(s: &Session) -> Vec<String> {
    s.model.state().bodies.iter().map(|b| b.name.clone()).collect()
}

fn vessel_group(s: &Session) -> Option<solidcraft_engine::doc::TimelineGroup> {
    s.doc.timeline_groups.iter().find(|g| g.name == "Vessel V-7").cloned()
}

#[test]
fn regenerate_replaces_only_its_own_features() {
    registered();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": small_vessel().to_value().unwrap()}));
    let mine = user_box(&mut s);

    let r1 = run(&mut s, "vessel.regenerate", json!({}));
    let count = s.doc.features.len();
    let group = vessel_group(&s).expect("the vessel's features sit in their folder");
    let built: Vec<u64> = r1["features"].as_array().unwrap().iter().filter_map(Value::as_u64).collect();
    assert_eq!(group.features, built);
    assert_eq!(r1["group"], json!(group.id));
    assert_eq!(body_names(&s).len(), 3, "{:?}", body_names(&s));

    // Regenerating again replaces the vessel's features and nothing else.
    let r2 = run(&mut s, "vessel.regenerate", json!({}));
    assert_eq!(s.doc.features.len(), count, "regenerate twice: feature count unchanged");
    assert_ne!(r2["features"], r1["features"]);
    assert_eq!(r2["group"], json!(group.id), "the same folder is reused");
    assert_eq!(s.doc.timeline_groups.len(), 1);
    assert!(s.doc.feature(mine).is_some(), "the user's box stays");
    assert!(body_names(&s).contains(&"My box".to_string()));

    // A feature of the user's own moved into the vessel's folder is not the vessel's.
    run(&mut s, "timeline.move_to_group", json!({"features": [mine], "group": group.id}));
    run(&mut s, "vessel.regenerate", json!({}));
    assert!(s.doc.feature(mine).is_some(), "moved into the folder, still the user's");
    assert_eq!(s.doc.features.len(), count);
}

#[test]
fn one_undo_takes_the_vessel_away() {
    registered();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": small_vessel().to_value().unwrap()}));
    user_box(&mut s);
    let before = (s.doc.features.len(), s.doc.timeline_groups.len());
    run(&mut s, "vessel.regenerate", json!({}));
    assert_eq!(body_names(&s).len(), 3);
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(body_names(&s), ["My box"], "one undo: the vessel's bodies are gone");
    assert_eq!((s.doc.features.len(), s.doc.timeline_groups.len()), before);
    let parameters = run(&mut s, "parameters.list", json!({"filter": "user"}));
    assert!(parameters["parameters"].as_array().is_none_or(|p| p.iter().all(|p| !p["name"].as_str().unwrap_or("").starts_with("V7_"))));
    // The vessel document itself stays (it came from vessel.new) and regenerates again.
    run(&mut s, "vessel.regenerate", json!({}));
    assert_eq!(body_names(&s).len(), 3);
}

#[test]
fn a_saved_design_regenerates_in_place_after_opening() {
    registered();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": small_vessel().to_value().unwrap()}));
    run(&mut s, "vessel.regenerate", json!({}));
    let count = s.doc.features.len();
    let text = s.doc.to_json();
    let back = solidcraft_engine::doc::Document::from_json(&text).unwrap();
    let mut s2 = Session::new(back);
    let inspect = run(&mut s2, "vessel.inspect", json!({}));
    assert_eq!(VesselDoc::from_value(&inspect["doc"]).unwrap(), small_vessel(), "save/open keeps the JSON");
    run(&mut s2, "vessel.regenerate", json!({}));
    assert_eq!(s2.doc.features.len(), count, "the reopened design knows which features are the vessel's");
    assert_eq!(s2.doc.timeline_groups.len(), 1);
}

/// A document of about `kib` KiB (long notes with quotes and backslashes, which grow when
/// escaped).
fn big_vessel(kib: usize) -> VesselDoc {
    let mut doc = small_vessel();
    let note = "Note \"quoted\" and a back\\slash, ünïcode; ".repeat(90);
    while serde_json::to_string(&doc).unwrap().len() < kib * 1024 {
        doc.notes.push(note.clone());
    }
    doc
}

fn chunk_keys(s: &Session) -> Vec<String> {
    (1..=commands::MAX_CHUNKS + 1).map(commands::chunk_key).filter(|k| s.doc.metadata.get(k).is_some()).collect()
}

#[test]
fn a_70_kib_document_is_chunked_and_round_trips() {
    registered();
    let doc = big_vessel(70);
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": doc.to_value().unwrap()}));
    let head = s.doc.metadata.get(commands::METADATA_KEY).cloned().unwrap();
    assert!(head.get("doc").is_none() && head["chunks"] == json!(2), "{}", head);
    assert_eq!(chunk_keys(&s).len(), 2);
    for k in chunk_keys(&s) {
        let n = serde_json::to_string(s.doc.metadata.get(&k).unwrap()).unwrap().len();
        assert!(n <= commands::CHUNK_BYTES, "{k}: {n} bytes");
    }
    // Through the design file and back.
    let back = solidcraft_engine::doc::Document::from_json(&s.doc.to_json()).unwrap();
    let mut s2 = Session::new(back);
    let inspect = run(&mut s2, "vessel.inspect", json!({}));
    assert_eq!(VesselDoc::from_value(&inspect["doc"]).unwrap(), doc);
    run(&mut s2, "vessel.regenerate", json!({}));
    assert_eq!(chunk_keys(&s2).len(), 2, "regenerate keeps the chunks");
    assert_eq!(VesselDoc::from_value(&run(&mut s2, "vessel.inspect", json!({}))["doc"]).unwrap(), doc);

    // Shrinking it back drops the chunks; growing past the cap is refused cleanly.
    run(&mut s2, "vessel.set", json!({"doc": small_vessel().to_value().unwrap()}));
    assert!(chunk_keys(&s2).is_empty());
    assert!(s2.doc.metadata.get(commands::METADATA_KEY).unwrap().get("doc").is_some());
    let before = s2.doc.clone();
    let e = s2.execute("vessel.set", &json!({"doc": big_vessel(300).to_value().unwrap()})).unwrap_err().to_string();
    assert!(e.contains("KiB"), "{e}");
    assert_eq!(*before, *s2.doc);
}

#[test]
fn inspect_reports_resolved_stations_warnings_and_flags() {
    registered();
    let mut s = Session::default();
    let mut doc = reference(0);
    if let Some(a) = doc.attachments.first_mut() {
        a.flags.boolean_failed = true;
    }
    let first = doc.attachments.first().map(|a| a.id.clone()).unwrap();
    run(&mut s, "vessel.new", json!({"doc": doc.to_value().unwrap()}));
    let r = run(&mut s, "vessel.inspect", json!({}));
    let stations = r["stations"].as_array().unwrap();
    assert_eq!(stations.len(), 7);
    let e1 = &stations[1];
    assert_eq!(e1["profile"]["type"], json!("head"));
    assert_eq!(e1["profile"]["shape"]["type"], json!("ellipsoidal"));
    assert!(e1["tl"].is_number());
    let skirt = &stations[0];
    assert_eq!(skirt["detail"]["type"], json!("base_ring"));
    assert_eq!(skirt["envelope"], json!(false));
    let cone = stations.iter().find(|st| st["kind"] == "cone").unwrap();
    assert!((cone["half_apex_deg"].as_f64().unwrap() - 21.002).abs() < 1e-3, "{cone}");
    assert!(r["extent"]["tl_to_tl"].is_number());
    assert!(r["warnings"].is_array());
    assert_eq!(r["flags"], json!([{"id": first, "boolean_failed": true, "unverified_table": false}]));
    assert!(r["parameters"].as_array().unwrap().iter().any(|p| p["name"] == "V101_E2_length"));
    assert!(r["generated"].as_array().unwrap().is_empty() && r["group"].is_null());
}

#[test]
fn inspect_warns_when_a_parameter_drifted() {
    registered();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": small_vessel().to_value().unwrap()}));
    run(&mut s, "vessel.regenerate", json!({}));
    let drift = |s: &mut Session| {
        let r = run(s, "vessel.inspect", json!({}));
        r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap_or("").contains("V7_S1_length"))
    };
    assert!(!drift(&mut s));
    run(&mut s, "parameters.change", json!({"name": "V7_S1_length", "expression": "1600 mm"}));
    assert!(drift(&mut s), "a changed parameter is reported");
    run(&mut s, "vessel.regenerate", json!({}));
    assert!(!drift(&mut s), "regenerate rewrites it");
}

// ---- T2.6: caps ---------------------------------------------------------------------------

#[test]
fn element_and_attachment_counts_are_capped() {
    use crate::model::{MAX_ATTACHMENTS, MAX_ELEMENTS};
    registered();
    let mut doc = VesselDoc::cylinder("S0", 1000.0, 10.0, 10.0);
    for i in 1..MAX_ELEMENTS {
        let mut e = doc.elements[0].clone();
        e.id = format!("S{i}");
        doc.elements.push(e);
    }
    assert!(doc.validate().is_ok(), "{MAX_ELEMENTS} elements are allowed");
    let host = doc.elements[0].id.clone();
    let nozzle = |i: usize| -> crate::model::Attachment {
        serde_json::from_value(json!({"id": format!("N{i}"), "kind": "nozzle", "host": host, "axial": {"ref": "host_from", "mm": 5},
            "angle_deg": 0, "frame": {"type": "radial"}, "nps": "2", "projection": {"from": "od", "mm": 150}}))
        .unwrap()
    };
    doc.attachments = (0..MAX_ATTACHMENTS).map(nozzle).collect();
    assert!(doc.validate().is_ok(), "{MAX_ATTACHMENTS} attachments are allowed");
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({"doc": doc.to_value().unwrap()}));

    // One more of either is refused, through every way in, and the design is left alone.
    let mut more = doc.clone();
    more.attachments.push(nozzle(MAX_ATTACHMENTS));
    assert!(more.validate().is_err());
    let before = s.doc.clone();
    let e = s.execute("vessel.set", &json!({"attachment": serde_json::to_value(nozzle(MAX_ATTACHMENTS)).unwrap()})).unwrap_err().to_string();
    assert!(e.contains("at most 500 attachments"), "{e}");
    let mut e201 = doc.elements[0].clone();
    e201.id = "S_extra".into();
    let e = s.execute("vessel.set", &json!({"element": serde_json::to_value(&e201).unwrap()})).unwrap_err().to_string();
    assert!(e.contains("at most 200 elements"), "{e}");
    assert_eq!(*before, *s.doc);
    let mut big = doc.to_value().unwrap();
    big["elements"] = json!(vec![json!({"id": "x"}); 100_000]);
    let e = s.execute("vessel.new", &json!({"doc": big, "replace": true})).unwrap_err().to_string();
    assert!(e.contains("at most 200 elements (got 100000)"), "refused before it is read: {e}");
}
