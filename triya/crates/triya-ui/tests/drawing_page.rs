// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! PLAN T6.1 and T6.7: the Drawing page generates the GA sheet through `drawing.generate_ga`
//! and paints it with epaint (views, tables, title block), says when the design changed under
//! it, and its exports run `drawing.export` with the picked path.

use serde_json::{Value, json};
use solidcraft_engine::Session;
use solidcraft_ui_egui::{Services, SolveApp};
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

/// Paint the panel into a 1600 × 1400 screen; the texts painted and the number of shapes.
fn painted(panel: &mut VesselPanel, app: &mut SolveApp) -> (String, usize) {
    let ctx = egui::Context::default();
    let mut text = String::new();
    let mut count = 0;
    for _ in 0..3 {
        let input =
            egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1400.0))), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                panel.sync(app);
                panel.contents(app, ui);
            });
        });
        out.textures_delta.clear();
        text.clear();
        count = 0;
        for c in &out.shapes {
            collect(&c.shape, &mut text, &mut count);
        }
    }
    (text, count)
}

fn collect(shape: &egui::Shape, text: &mut String, count: &mut usize) {
    *count += 1;
    match shape {
        egui::Shape::Text(t) => {
            text.push_str(t.galley.text());
            text.push('\n');
        }
        egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, text, count)),
        _ => {}
    }
}

#[test]
fn the_drawing_page_paints_the_ga_sheet_of_vessel_a() {
    let mut app = app_with(VESSEL_A);
    let mut panel = VesselPanel::default();
    panel.page = Page::Drawing;
    let (text, _) = painted(&mut panel, &mut app);
    assert!(text.contains("Generate") && text.contains("Generate builds the GA sheet"), "{text}");

    triya_ui::vessel::drawing::DrawingPage::generate(&mut panel, &mut app);
    let sheet = panel.drawing.sheet.clone().expect("the command returned the sheet");
    assert_eq!(sheet.views.len(), 3);
    assert!(panel.drawing.summary.starts_with("A1 at 1:30"), "{}", panel.drawing.summary);
    let (text, shapes) = painted(&mut panel, &mut app);
    for want in ["ELEVATION", "SECTION A-A", "PLAN (NOZZLE ORIENTATION)", "NOZZLE SCHEDULE", "DESIGN DATA", "REVISIONS", "V-101-GA-001", "Update"] {
        assert!(text.contains(want), "`{want}` not painted:\n{text}");
    }
    assert!(shapes > 2000, "only {shapes} shapes");
    assert!(!text.contains("the design changed"));

    // The design changes under the sheet: the page says so; Update rebuilds it.
    app.run("vessel.set", json!({"patch": [{"op": "replace", "path": "/title", "value": "Column C-7"}]})).unwrap();
    let (text, _) = painted(&mut panel, &mut app);
    assert!(text.contains("the design changed: Update"), "{text}");
    triya_ui::vessel::drawing::DrawingPage::generate(&mut panel, &mut app);
    let (text, _) = painted(&mut panel, &mut app);
    assert!(text.contains("COLUMN C-7") && !text.contains("the design changed"), "{text}");
}

#[test]
fn the_export_buttons_write_through_drawing_export() {
    let mut app = app_with(VESSEL_A);
    let path = std::env::temp_dir().join(format!("triya-ui-ga-{}.dxf", std::process::id()));
    let p = path.to_string_lossy().to_string();
    // What the DXF button runs once the save dialog answers.
    let mut panel = VesselPanel::default();
    let v = panel.run(&mut app, triya_ui::vessel::edits::Action { id: "drawing.export", params: json!({"format": "dxf", "path": p}) }).unwrap();
    assert_eq!(v["format"], json!("dxf"));
    let text = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    for layer in ["OUTLINE", "HIDDEN", "CENTER", "DIM", "HATCH", "TEXT", "TITLE"] {
        assert!(text.contains(&format!("\n{layer}\n")), "{layer}");
    }
}
