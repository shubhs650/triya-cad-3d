// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! PLAN T5.2: the stack editor. The panel shows the elements with their live stations, every
//! edit is a `vessel.set` JSON-pointer patch (or another `vessel.*` command) run through
//! `app.run`, a diameter change at the bottom propagates up the stack, and Regenerate runs off
//! the window thread.

use serde_json::{Value, json};
use solidcraft_engine::Session;
use solidcraft_ui_egui::{Services, SolveApp};
use triya_ui::vessel::edits;
use triya_ui::vessel::{Page, VesselPanel};

fn app_with(doc: &str) -> SolveApp {
    triya_ui::register().unwrap();
    let mut app = SolveApp::new(Session::default(), Services::default());
    let doc: Value = serde_json::from_str(doc).unwrap();
    app.run("vessel.new", json!({"doc": doc})).unwrap();
    app.ui.tab = triya_vessel::commands::TAB.into();
    app
}

/// Every piece of text one frame of the panel painted.
fn painted(panel: &mut VesselPanel, app: &mut SolveApp) -> String {
    let ctx = egui::Context::default();
    let mut text = String::new();
    // Grids lay themselves out on the first frame and paint on the next.
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

fn doc(app: &SolveApp) -> triya_vessel::VesselDoc {
    triya_vessel::commands::stored_doc(&app.session).unwrap()
}

#[test]
fn the_stack_shows_elements_and_live_stations() {
    let mut app = app_with(include_str!("../../triya-vessel/tests/reference/vessel-c.json"));
    let mut panel = VesselPanel::default();
    panel.page = Page::Stack;
    panel.selected_element = Some("E1".into());
    let text = painted(&mut panel, &mut app);
    for want in ["E1", "E2", "E3", "Head", "Hemispherical", "Straight flange", "Cylinder", "TL–TL 2450", "Bottom TL", "bottom → top", "2425"] {
        assert!(text.contains(want), "`{want}` not painted:\n{text}");
    }
    // Drawing alone changes nothing.
    assert!(panel.take_queued().is_empty());
}

#[test]
fn edits_are_vessel_patches_and_a_diameter_change_propagates() {
    let mut app = app_with(include_str!("../../triya-vessel/tests/reference/vessel-c.json"));
    let mut panel = VesselPanel::default();
    let d = doc(&app);
    let undo = app.session.undo.len();
    // The bottom head's diameter from 900 to 1000: E2 and E3 inherit it.
    let a = edits::element_field(&d, "E1", &["diameter_mm"], Some(json!(1000.0))).unwrap();
    assert!(a.id.starts_with("vessel."));
    assert_eq!(a.params, json!({"patch": [{"op": "add", "path": "/elements/0/diameter_mm", "value": 1000.0}]}));
    panel.run(&mut app, a).unwrap();
    assert_eq!(app.session.undo.len(), undo + 1, "one edit, one undo step");
    let i = app.run("vessel.inspect", json!({})).unwrap();
    let st = |id: &str| i["stations"].as_array().unwrap().iter().find(|s| s["id"] == id).cloned().unwrap();
    assert_eq!(st("E2")["diameter_mm"], 1000.0);
    assert_eq!(st("E3")["r_from"], 500.0);
    // Downstream: N1 on E2 now meets a 1000 shell (its neck starts on the bigger radius).
    let n1 = i["nozzles"].as_array().unwrap().iter().find(|n| n["id"] == "N1").cloned().unwrap();
    assert!((n1["s_inner"].as_f64().unwrap() - 500.0).abs() < 1.0, "{n1}");
    // The same JSON through MCP's engine.execute would do the same: the panel adds nothing.
    let mut s = Session::default();
    s.execute(
        "vessel.new",
        &json!({"doc": serde_json::from_str::<Value>(include_str!("../../triya-vessel/tests/reference/vessel-c.json")).unwrap()}),
    )
    .unwrap();
    s.execute("vessel.set", &json!({"patch": [{"op": "add", "path": "/elements/0/diameter_mm", "value": 1000.0}]})).unwrap();
    assert_eq!(triya_vessel::commands::stored_doc(&s).unwrap(), doc(&app));

    // Insert, move, change kind, delete: each a vessel command.
    let r = panel.run(&mut app, edits::insert_element("cylinder", Some("E2"))).unwrap();
    assert_eq!(r["id"], "E4");
    let d = doc(&app);
    let ids = |d: &triya_vessel::VesselDoc| d.elements.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&d), ["E1", "E2", "E4", "E3"]);
    panel.run(&mut app, edits::move_element(&d, None, "E4", -1).unwrap()).unwrap();
    assert_eq!(ids(&doc(&app)), ["E1", "E4", "E2", "E3"]);
    let d = doc(&app);
    let r = triya_vessel::eval::resolve(&d).unwrap();
    panel.run(&mut app, edits::change_kind(&d, Some(&r), "E4", "cone").unwrap()).unwrap();
    assert_eq!(doc(&app).element("E4").unwrap().kind.name(), "cone");
    let d = doc(&app);
    panel.run(&mut app, edits::delete_element(&d, None, "E4").unwrap()).unwrap();
    assert_eq!(ids(&doc(&app)), ["E1", "E2", "E3"]);
    // A bad value is refused by the command and shown; the design is unchanged.
    let before = doc(&app);
    let d = doc(&app);
    assert!(panel.run(&mut app, edits::element_field(&d, "E2", &["length_mm"], Some(json!(-1.0))).unwrap()).is_none());
    assert_eq!(doc(&app), before);
    // Every action the panel can make is a vessel command.
    for a in [
        edits::global_field("default_material", json!("SA-240 304")),
        edits::add_attachment("nozzle", Some("E2")),
        edits::insert_element("head", None),
    ] {
        assert!(a.id.starts_with("vessel."), "{}", a.id);
    }
}

#[test]
fn the_datum_selector_keeps_nozzles_in_place() {
    let mut app = app_with(include_str!("../../triya-vessel/tests/reference/vessel-c.json"));
    let mut panel = VesselPanel::default();
    let r = panel.run(&mut app, edits::Action { id: "vessel.datum", params: json!({"datum": "bottom_seam"}) }).unwrap();
    assert_eq!(r["shift_mm"], -25.0);
    let n1 = doc(&app).attachments.iter().find(|a| a.id == "N1").map(|a| a.axial.mm);
    assert_eq!(n1, Some(275.0));
}

#[test]
fn regenerate_runs_off_the_window_thread() {
    let mut app = app_with(&json!({"schema": "triya.vessel/0.1", "id": "D1", "elements": [{"id": "E1", "kind": "cylinder", "diameter_mm": 1000, "thickness_mm": 10, "length_mm": 2000}]}).to_string());
    let undo = app.session.undo.len();
    let ctx = egui::Context::default();
    let mut regen = triya_ui::vessel::regen::Regen::default();
    regen.start(&mut app);
    assert!(regen.busy());
    let t0 = std::time::Instant::now();
    let done = loop {
        if let Some(d) = regen.poll(&mut app, &ctx) {
            break d;
        }
        assert!(t0.elapsed().as_secs() < 300, "regeneration did not finish");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    match done {
        triya_ui::vessel::regen::Finished::Done(v) => assert_eq!(v["features"].as_array().map(Vec::len), Some(2), "{v}"),
        triya_ui::vessel::regen::Finished::Failed(e) => panic!("{e}"),
    }
    assert_eq!(app.session.model.state().bodies.len(), 1);
    assert_eq!(app.session.undo.len(), undo + 1, "one undo step, as from the ribbon");
    assert_eq!(triya_vessel::commands::progress::get().0, triya_vessel::commands::progress::get().1);
    // The design changing meanwhile drops the result.
    regen.start(&mut app);
    app.run("vessel.set", json!({"global": {"default_material": "SA-240 304"}})).unwrap();
    let done = loop {
        if let Some(d) = regen.poll(&mut app, &ctx) {
            break d;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(matches!(done, triya_ui::vessel::regen::Finished::Failed(e) if e.contains("changed")));
}
