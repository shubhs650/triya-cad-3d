// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! PLAN T1.2: Help ▸ About shows Triya's window with the attribution sentence, not upstream's.

use solidcraft_engine::Session;
use solidcraft_ui_egui::{Services, SolveApp};
use triya_ui::about::{About, TITLE};

/// Every piece of text one frame painted.
fn painted_text(ctx: &egui::Context, mut frame: impl FnMut(&egui::Context)) -> String {
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| frame(ui.ctx()));
    // No painter here to upload the mark's texture to.
    out.textures_delta.clear();
    let mut text = String::new();
    for clipped in &out.shapes {
        collect(&clipped.shape, &mut text);
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

#[test]
fn help_about_opens_the_triya_window_with_the_attribution_sentence() {
    let mut app = SolveApp::new(Session::default(), Services::default());
    let mut about = About::default();
    let ctx = egui::Context::default();
    // Upstream's windows name their own font families (v0.22 `ui_kit`).
    solidcraft_ui_egui::fonts::setup_bundled_fonts(&ctx);
    // Help ▸ About (what upstream's menu item and the control channel's `ui.help about` do).
    app.help.about = true;
    about.intercept(&mut app);
    assert!(!app.help.about, "upstream's About must not open");
    assert!(about.is_open());
    // Windows lay themselves out on the first frame and paint on the next.
    let mut text = String::new();
    for _ in 0..3 {
        text = painted_text(&ctx, |ctx| about.show(&mut app, ctx));
    }
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains(TITLE), "{flat}");
    assert!(flat.contains(&format!("Version {}", triya_brand::VERSION)), "{flat}");
    assert!(flat.contains("Not affiliated with the SolidCraft project."), "{flat}");
    assert!(flat.contains(&triya_brand::about_text().split_whitespace().collect::<Vec<_>>().join(" ")), "{flat}");
    assert!(!flat.contains("About SolidCraft"), "{flat}");
}

#[test]
fn diagnostics_lead_with_triya() {
    let app = SolveApp::new(Session::default(), Services::default());
    let d = About::diagnostics(&app);
    assert!(d.starts_with(&format!("Triya CAD 3D {} (upstream SolidCraft v0.22.0", triya_brand::VERSION)), "{d}");
}
