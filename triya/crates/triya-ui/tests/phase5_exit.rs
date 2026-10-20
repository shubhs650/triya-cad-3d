// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Phase 5 exit (M2) and the T5.5 synthetic usability run. Every field of every element and
//! attachment kind is editable from the panel, through the same `vessel.set` JSON an MCP client
//! sends; a shell diameter change at the bottom propagates up the stack and moves what sits on
//! it; a 10-nozzle vessel can be modelled from `vessel.new {}` with panel actions alone, and the
//! run counts them.

use serde_json::{Value, json};
use solidcraft_engine::Session;
use solidcraft_ui_egui::{Services, SolveApp};
use triya_ui::vessel::VesselPanel;
use triya_ui::vessel::edits::{self, Action};

fn app_with(doc: Option<&str>) -> SolveApp {
    triya_ui::register().unwrap();
    let mut app = SolveApp::new(Session::default(), Services::default());
    let params = match doc {
        Some(d) => json!({"doc": serde_json::from_str::<Value>(d).unwrap()}),
        None => json!({}),
    };
    app.run("vessel.new", params).unwrap();
    app
}

fn doc(app: &SolveApp) -> triya_vessel::VesselDoc {
    triya_vessel::commands::stored_doc(&app.session).unwrap()
}

/// The fields the panel can point at in one JSON object: its keys and, one level down, the keys
/// of object values (`flange.class`, `axial.mm`, …).
fn fields(v: &Value) -> Vec<(Vec<String>, Value)> {
    let mut out = Vec::new();
    for (k, x) in v.as_object().into_iter().flatten() {
        if k == "id" || k == "kind" {
            continue;
        }
        out.push((vec![k.clone()], x.clone()));
        for (k2, y) in x.as_object().into_iter().flatten() {
            out.push((vec![k.clone(), k2.clone()], y.clone()));
        }
    }
    out
}

#[test]
fn every_field_is_editable_from_the_panel_and_from_mcp() {
    let src = include_str!("../../triya-vessel/tests/reference/every-kind.json");
    let mut app = app_with(Some(src));
    let mut mcp = Session::default();
    mcp.execute("vessel.new", &json!({"doc": serde_json::from_str::<Value>(src).unwrap()})).unwrap();
    let mut panel = VesselPanel::default();
    let v = doc(&app).to_value().unwrap();
    let mut edited = 0;
    for list in ["elements", "attachments"] {
        for item in v[list].as_array().unwrap() {
            let id = item["id"].as_str().unwrap();
            for (path, value) in fields(item) {
                let segs: Vec<&str> = path.iter().map(String::as_str).collect();
                let d = doc(&app);
                let a: Action = if list == "elements" {
                    edits::element_field(&d, id, &segs, Some(value.clone()))
                } else {
                    edits::attachment_field(&d, id, &segs, Some(value.clone()))
                }
                .unwrap_or_else(|| panic!("{id} {path:?}: no action"));
                assert_eq!(a.id, "vessel.set");
                // The panel's action and the same JSON over MCP both succeed and agree.
                assert!(panel.run(&mut app, Action { id: a.id, params: a.params.clone() }).is_some(), "{id} {path:?}");
                mcp.execute(a.id, &a.params).unwrap_or_else(|e| panic!("{id} {path:?}: {e}"));
                edited += 1;
            }
        }
    }
    for (k, x) in v["global"].as_object().unwrap() {
        let a = edits::global_field(k, x.clone());
        assert!(panel.run(&mut app, Action { id: a.id, params: a.params.clone() }).is_some(), "global {k}");
        mcp.execute(a.id, &a.params).unwrap();
        edited += 1;
    }
    assert!(edited > 100, "{edited} fields");
    assert_eq!(doc(&app), triya_vessel::commands::stored_doc(&mcp).unwrap());
    assert_eq!(doc(&app).to_value().unwrap(), v, "setting every field to itself changes nothing");
}

#[test]
fn a_shell_diameter_change_propagates_downstream() {
    let mut app = app_with(Some(include_str!("../../triya-vessel/tests/reference/vessel-a.json")));
    let mut panel = VesselPanel::default();
    let before = app.run("vessel.inspect", json!({})).unwrap();
    // Vessel A's bottom head sets 1600 for the shells above it, up to the cone.
    let d = doc(&app);
    panel.run(&mut app, edits::element_field(&d, "E1", &["diameter_mm"], Some(json!(1700.0))).unwrap()).unwrap();
    let after = app.run("vessel.inspect", json!({})).unwrap();
    let st = |i: &Value, id: &str| i["stations"].as_array().unwrap().iter().find(|s| s["id"] == id).cloned().unwrap();
    for id in ["E2", "E3"] {
        assert_eq!(st(&after, id)["diameter_mm"], 1700.0, "{id}");
    }
    assert_eq!(st(&after, "E5")["diameter_mm"], st(&before, "E5")["diameter_mm"], "the cone stops inheritance");
    // The nozzles on E2/E3 now start on the bigger shell; the cone's small end no longer meets it.
    let s_inner = |i: &Value, id: &str| i["nozzles"].as_array().unwrap().iter().find(|n| n["id"] == id).and_then(|n| n["s_inner"].as_f64()).unwrap();
    // N1 is radial: its neck starts exactly 50 further out; the others (hillside, tangential,
    // tilted) start further out too.
    assert!((s_inner(&after, "N1") - s_inner(&before, "N1") - 50.0).abs() < 1e-6);
    for n in ["N2", "N3", "N4"] {
        assert!(s_inner(&after, n) > s_inner(&before, n) + 1.0, "{n}: {} → {}", s_inner(&before, n), s_inner(&after, n));
    }
    let warnings = after["warnings"].to_string();
    assert!(warnings.contains("E4") || warnings.contains("mismatch"), "{warnings}");
    // The skirt hangs from the head's OD: it followed too.
    assert!(st(&after, "E0")["r_from"].as_f64().unwrap() > st(&before, "E0")["r_from"].as_f64().unwrap());
}

/// T5.5, synthetic: a 10-nozzle vertical drum (2:1 heads, 1600 × 12 shell, legs) from an empty
/// design with the panel's actions only. A person types one field per action, so the action
/// count bounds the time to model (recorded in PROGRESS.md "Measurements").
#[test]
fn a_ten_nozzle_vessel_from_panel_actions() {
    let mut app = app_with(None);
    let mut panel = VesselPanel::default();
    let mut actions = 0;
    let mut run = |panel: &mut VesselPanel, app: &mut SolveApp, a: Option<Action>| -> Value {
        actions += 1;
        let a = a.unwrap_or_else(|| panic!("action {actions}: nothing to do"));
        assert!(a.id.starts_with("vessel."));
        panel.run(app, a).unwrap_or_else(|| panic!("action {actions} refused"))
    };
    let d = doc(&app);
    let shell = d.elements.first().map(|e| e.id.clone()).unwrap();
    run(&mut panel, &mut app, edits::element_field(&d, &shell, &["diameter_mm"], Some(json!(1600.0))));
    let d = doc(&app);
    run(&mut panel, &mut app, edits::element_field(&d, &shell, &["thickness_mm"], Some(json!(12.0))));
    let d = doc(&app);
    run(&mut panel, &mut app, edits::element_field(&d, &shell, &["length_mm"], Some(json!(3200.0))));
    let top = run(&mut panel, &mut app, Some(edits::insert_element("head", Some(&shell))));
    // With the top closed, the next head closes the bottom and takes the shell's diameter.
    let bottom = run(&mut panel, &mut app, Some(edits::insert_element("head", None)));
    let bottom = bottom["id"].as_str().unwrap().to_string();
    let d = doc(&app);
    assert_eq!(d.elements.first().map(|e| e.id.as_str()), Some(bottom.as_str()));
    assert_eq!(d.elements.last().map(|e| e.id.clone()), top["id"].as_str().map(str::to_string));
    let r = triya_vessel::eval::resolve(&d).unwrap();
    let tl = r.tl_to_tl().unwrap();
    assert!((3200.0..3400.0).contains(&tl), "TL–TL {tl} (the shell plus the heads' straight flanges)");
    // Ten nozzles on the shell: add, then elevation, angle and size.
    let sizes = ["2", "2", "3", "3", "4", "4", "6", "6", "8", "1"];
    for (k, nps) in sizes.iter().enumerate() {
        let n = run(&mut panel, &mut app, Some(edits::add_attachment("nozzle", Some(&shell))));
        let id = n["id"].as_str().unwrap().to_string();
        let d = doc(&app);
        run(&mut panel, &mut app, edits::attachment_field(&d, &id, &["axial", "mm"], Some(json!(400.0 + 250.0 * k as f64))));
        let d = doc(&app);
        let angle = edits::snap(k as f64 * 67.5, edits::snap_step(&d, &shell));
        run(&mut panel, &mut app, edits::attachment_field(&d, &id, &["angle_deg"], Some(json!(angle))));
        let d = doc(&app);
        run(&mut panel, &mut app, edits::attachment_field(&d, &id, &["nps"], Some(json!(nps))));
    }
    run(&mut panel, &mut app, Some(Action { id: "vessel.support.add", params: json!({"kind": "leg"}) }));
    let d = doc(&app);
    assert_eq!(d.attachments.iter().filter(|a| a.kind.name() == "nozzle").count(), 10);
    let i = app.run("vessel.inspect", json!({})).unwrap();
    assert_eq!(i["nozzles"].as_array().map(Vec::len), Some(10), "{i}");
    println!("T5.5 synthetic: {actions} panel actions for a 10-nozzle vessel");
    assert!(actions <= 50, "{actions} actions");
}
