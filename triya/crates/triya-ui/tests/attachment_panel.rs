// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! PLAN T5.3 and T5.4: the attachment page and the plan-view widget. Picking a vessel body in the
//! 3D view selects its element or attachment; adding a nozzle from the panel stores exactly what
//! the same `vessel.attachment.add` from MCP stores; angles snap 22.5° on shells and 5° on heads;
//! the plan view paints every attachment at its angle with the 0° reference.

use serde_json::{Value, json};
use solidcraft_engine::{Sel, Session};
use solidcraft_ui_egui::{Services, SolveApp};
use triya_ui::vessel::edits;
use triya_ui::vessel::{Page, VesselPanel};

const VESSEL_A: &str = include_str!("../../triya-vessel/tests/reference/vessel-a.json");

fn app_with(doc: &str) -> SolveApp {
    triya_ui::register().unwrap();
    let mut app = SolveApp::new(Session::default(), Services::default());
    let doc: Value = serde_json::from_str(doc).unwrap();
    app.run("vessel.new", json!({"doc": doc})).unwrap();
    app.ui.tab = triya_vessel::commands::TAB.into();
    app
}

fn painted(panel: &mut VesselPanel, app: &mut SolveApp) -> String {
    let ctx = egui::Context::default();
    let mut text = String::new();
    for _ in 0..3 {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                panel.sync(app);
                panel.contents(app, ui);
            });
        });
        out.textures_delta.clear();
        text.clear();
        for c in &out.shapes {
            collect(&c.shape, &mut text);
        }
    }
    text
}

fn collect(shape: &egui::Shape, text: &mut String) {
    match shape {
        egui::Shape::Text(t) => {
            text.push_str(t.galley.text());
            text.push('\n');
        }
        egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, text)),
        _ => {}
    }
}

fn doc(s: &Session) -> triya_vessel::VesselDoc {
    triya_vessel::commands::stored_doc(s).unwrap()
}

#[test]
fn the_attachment_page_follows_the_3d_selection() {
    let mut app = app_with(VESSEL_A);
    let mut panel = VesselPanel::default();
    panel.page = Page::Attachments;
    // N1's join failed at the last regenerate: the list says so.
    app.run("vessel.set", json!({"patch": [{"op": "add", "path": "/attachments/0/flags", "value": {"boolean_failed": true}}]})).unwrap();
    // A face of N3's flange picked in the view selects N3; a shell body selects its element.
    app.session.selection = vec![Sel::Body { name: "Vessel N3 flange".into() }];
    let text = painted(&mut panel, &mut app);
    assert_eq!(panel.selected_attachment.as_deref(), Some("N3"));
    for want in ["N1", "N2", "N4", "Nozzle", "separate", "N3 · Nozzle", "Host", "Projection", "snaps to 22.5°"] {
        assert!(text.contains(want), "`{want}` not painted:\n{text}");
    }
    app.session.selection = vec![Sel::Body { name: "Vessel E3".into() }];
    let text = painted(&mut panel, &mut app);
    assert_eq!(panel.selected_element.as_deref(), Some("E3"));
    assert!(text.contains("Add on E3"), "{text}");
    // A vertex of N2's flange (a corner pick) selects N2.
    app.session.selection = vec![Sel::Vertex { body: "Vessel N2 flange".into(), point: solidcraft_engine::geom::Vec3 { x: 0.0, y: 0.0, z: 0.0 } }];
    painted(&mut panel, &mut app);
    assert_eq!(panel.selected_attachment.as_deref(), Some("N2"));
    // Following can be switched off.
    panel.no_follow = true;
    app.session.selection = vec![Sel::Body { name: "Vessel E2".into() }];
    painted(&mut panel, &mut app);
    assert_eq!(panel.selected_element.as_deref(), Some("E3"));
    assert!(panel.take_queued().is_empty(), "drawing alone changes nothing");
}

/// An attachment's elevation from the datum.
fn z_of(d: &triya_vessel::VesselDoc, id: &str) -> f64 {
    let a = d.attachments.iter().find(|a| a.id == id).unwrap();
    match a.axial.reference {
        triya_vessel::model::AxialRef::Datum => a.axial.mm,
        triya_vessel::model::AxialRef::HostFrom => a.axial.mm + triya_vessel::eval::resolve(d).unwrap().station(&a.host).unwrap().z_from,
    }
}

#[test]
fn a_nozzle_added_from_the_panel_equals_one_added_over_mcp() {
    let mut app = app_with(VESSEL_A);
    let mut panel = VesselPanel::default();
    let a = edits::add_attachment("nozzle", Some("E3"));
    assert_eq!((a.id, &a.params), ("vessel.attachment.add", &json!({"attachment": {"kind": "nozzle", "host": "E3"}})));
    let r = panel.run(&mut app, a).unwrap();
    // The same JSON from an MCP client's engine.execute.
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": serde_json::from_str::<Value>(VESSEL_A).unwrap()})).unwrap();
    let r2 = s.execute("vessel.attachment.add", &json!({"attachment": {"kind": "nozzle", "host": "E3"}})).unwrap();
    assert_eq!(r, r2);
    assert_eq!(doc(&app.session), doc(&s));
    // Then edit it from the panel: angle (snapped on a shell), axial reference, flange class.
    let id = r["id"].as_str().unwrap().to_string();
    let d = doc(&app.session);
    let step = edits::snap_step(&d, "E3");
    assert_eq!(step, 22.5);
    let a = edits::attachment_field(&d, &id, &["angle_deg"], Some(json!(edits::snap(50.0, step)))).unwrap();
    assert_eq!(a.id, "vessel.set");
    panel.run(&mut app, a).unwrap();
    let d = doc(&app.session);
    let r = triya_vessel::eval::resolve(&d).unwrap();
    let before = z_of(&d, &id);
    panel.run(&mut app, edits::axial_ref(&d, &r, &id, triya_vessel::model::AxialRef::Datum).unwrap()).unwrap();
    let d = doc(&app.session);
    let n = d.attachments.iter().find(|a| a.id == id).unwrap();
    assert_eq!(n.angle_deg, 45.0);
    assert_eq!(n.axial.reference, triya_vessel::model::AxialRef::Datum);
    assert!((z_of(&d, &id) - before).abs() < 1e-6, "the nozzle stays where it is");
    panel.run(&mut app, edits::attachment_field(&d, &id, &["flange", "class"], Some(json!(300))).unwrap()).unwrap();
    let after = doc(&app.session).to_value().unwrap();
    let n = after["attachments"].as_array().unwrap().iter().find(|a| a["id"] == id.as_str()).unwrap();
    assert_eq!(n["flange"]["class"], 300);
    // On a head the step is 5°.
    assert_eq!(edits::snap_step(&d, "E1"), 5.0);
    assert_eq!(edits::snap(47.4, 5.0), 45.0);
    assert_eq!(edits::snap(359.0, 22.5), 0.0);
}

#[test]
fn the_plan_view_paints_every_attachment_at_its_angle() {
    let mut app = app_with(VESSEL_A);
    let mut panel = VesselPanel::default();
    panel.page = Page::Plan;
    panel.selected_attachment = Some("N2".into());
    let text = painted(&mut panel, &mut app);
    for want in ["0°", "90°", "180°", "270°", "0° = N (+Y)", "N1", "N2", "N3", "N4", "N5", "M1", "L1", "AC1", "Click the ring to turn N2"] {
        assert!(text.contains(want), "`{want}` not painted:\n{text}");
    }
    // 0° at +X instead: a vessel setting, through vessel.set.
    let d = doc(&app.session);
    let a = edits::global_field("angle_zero", json!("plus_x"));
    assert_eq!(a.id, "vessel.set");
    panel.run(&mut app, a).unwrap();
    assert_ne!(doc(&app.session).global.angle_zero, d.global.angle_zero);
    assert!(painted(&mut panel, &mut app).contains("0° = +X"));
    assert!(panel.take_queued().is_empty());
}
