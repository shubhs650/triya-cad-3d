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
fn heads_cones_and_attachments_are_not_supported_yet_but_resolve() {
    let mut doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 2000.0);
    doc.elements.insert(0, head("H1", HeadPresetId::Ellipsoidal21, Some(40.0)));
    doc.global.datum = crate::model::Datum::BottomTl;
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
        json!({"schema": "triya.vessel/0.1", "elements": [{"id": "", "kind": "cylinder"}]}),
        json!({"schema": "triya.vessel/0.1", "elements": [{"id": "a", "kind": "cylinder", "diameter_mm": 1e308, "thickness_mm": 1, "length_mm": 1}]}),
        json!({"schema": "triya.vessel/0.1", "elements": [{"id": "a", "kind": "head", "preset": "custom_torispherical", "crown_radius_mm": 1, "knuckle_radius_mm": 4.9, "diameter_mm": 10, "thickness_mm": 1}]}),
        json!({"id": "S1", "kind": "cone", "diameter_small_mm": -5, "diameter_large_mm": 100, "thickness_mm": 5, "length_mm": 50}),
        json!({"id": "N1", "kind": "nozzle", "host": "S1", "axial": {"ref": "host_from", "mm": -1e300}, "angle_deg": 0, "frame": {"type": "radial"}, "nps": "2", "projection": {"from": "cl", "mm": 150}}),
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
fn regenerate_refuses_attachments_and_leaves_the_design_as_it_was() {
    registered();
    let mut s = Session::default();
    run(&mut s, "vessel.new", json!({}));
    run(
        &mut s,
        "vessel.set",
        json!({"attachment": {"id": "N1", "kind": "nozzle", "host": "S1", "axial": {"ref": "host_from", "mm": 500}, "angle_deg": 90, "frame": {"type": "radial"},
            "nps": "2", "projection": {"from": "od", "mm": 150}}}),
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
