// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D's hooks on the SolidCraft egui front end (L5, beside `solidcraft-ui-egui`).
//!
//! Upstream's toolbar lists its tabs in `solidcraft_ui_egui::toolbar::TABS` and fills each tab's
//! panels from `CommandSpec.tab`/`.panel` automatically (`toolbar_custom::panel_lists`), so the
//! `vessel.*` commands appear on the toolbar as soon as they are registered and `TABS` holds
//! `VESSEL` (the overlay's one-line seam in `crates/ui-egui/src/toolbar.rs`). What this crate adds:
//!
//! - [`register`]: puts `triya_vessel::COMMANDS` into the engine's registry (call it once, before
//!   the first `Session`).
//! - [`VesselPanel`]: a thin window shown while the `VESSEL` tab is active, with the vessel JSON in
//!   a text box and New / Apply / Regenerate buttons. It reads engine state and acts only through
//!   `app.run(id, params)`, like every upstream panel.
//! - [`about_text`]: the brand and attribution sentence for the app shell's About page.
//! - [`about::About`]: the About window, which takes over upstream's Help ▸ About.
//! - [`icons`]: the code-drawn Triya mark as egui images and the window icon.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod about;
pub mod icons;

use egui::RichText;
use serde_json::{Value, json};
use solidcraft_ui_egui::SolveApp;
use solidcraft_ui_egui::theme::Tokens;

/// Register the overlay's commands with the engine. Idempotent; `Err` names a clashing id.
pub fn register() -> Result<(), String> {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS)
}

/// The brand and attribution line (`triya_brand::about_text`).
pub fn about_text() -> String {
    triya_brand::about_text()
}

/// The Vessel panel's own state: the JSON being edited and which design revision it was read at.
#[derive(Default)]
pub struct VesselPanel {
    text: String,
    synced_revision: Option<u64>,
    error: Option<String>,
    show_about: bool,
}

impl VesselPanel {
    /// The vessel JSON the design holds right now (pretty-printed), if any.
    fn stored(app: &SolveApp) -> Option<String> {
        let v = app.session.doc.metadata.get(triya_vessel::commands::METADATA_KEY)?;
        serde_json::to_string_pretty(v.get("doc")?).ok()
    }

    /// Re-read the design's vessel when the design changed under the panel.
    fn sync(&mut self, app: &SolveApp) {
        if self.synced_revision == Some(app.session.revision) {
            return;
        }
        self.synced_revision = Some(app.session.revision);
        if let Some(text) = Self::stored(app) {
            self.text = text;
        } else if self.text.is_empty() {
            self.text = "(no vessel: New makes a 1000 × 10 × 2000 cylinder)".into();
        }
    }

    fn run(&mut self, app: &mut SolveApp, id: &str, params: Value) {
        match app.run(id, params) {
            Ok(_) => self.error = None,
            Err(e) => self.error = Some(e),
        }
        // The next frame reads the design back (the command may have changed it).
        self.synced_revision = None;
    }

    /// Draw the panel (only while the `VESSEL` tab is active).
    pub fn show(&mut self, app: &mut SolveApp, ctx: &egui::Context) {
        if app.ui.tab != triya_vessel::commands::TAB {
            return;
        }
        self.sync(app);
        let t = Tokens::get();
        let mut action: Option<(&'static str, Value)> = None;
        solidcraft_ui_egui::frame::window(ctx, "Vessel", solidcraft_ui_egui::frame::Width::Normal).id(egui::Id::new("triya_vessel_panel")).show(
            ctx,
            |ui| {
                ui.horizontal(|ui| {
                    if ui.button("New").on_hover_text("vessel.new: a one-cylinder vessel (replaces the current one)").clicked() {
                        action = Some(("vessel.new", json!({"replace": true})));
                    }
                    if ui.button("Apply").on_hover_text("vessel.set: store the JSON below as the vessel").clicked() {
                        match serde_json::from_str::<Value>(&self.text) {
                            Ok(doc) => action = Some(("vessel.set", json!({"doc": doc}))),
                            Err(e) => self.error = Some(format!("JSON: {e}")),
                        }
                    }
                    if ui.button("Regenerate").on_hover_text("vessel.regenerate: build the vessel's bodies (one undo step)").clicked() {
                        action = Some(("vessel.regenerate", json!({})));
                    }
                    if ui.button("Inspect").on_hover_text("vessel.inspect: stations and the command plan, to the log").clicked() {
                        action = Some(("vessel.inspect", json!({})));
                    }
                });
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).color(t.error));
                }
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.text).code_editor().desired_width(f32::INFINITY).desired_rows(16));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(triya_brand::APP_NAME).strong());
                    if ui.small_button("About").clicked() {
                        self.show_about = !self.show_about;
                    }
                });
                if self.show_about {
                    ui.label(RichText::new(about_text()).color(t.text_dim).size(11.0));
                }
            },
        );
        if let Some((id, params)) = action {
            if id == "vessel.inspect" {
                match app.run(id, params) {
                    Ok(v) => {
                        let line = serde_json::to_string(&v).unwrap_or_default();
                        app.session.echo(line);
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                }
            } else {
                self.run(app, id, params);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn register_is_idempotent_and_about_names_upstream() {
        assert!(super::register().is_ok());
        assert!(super::register().is_ok());
        assert!(solidcraft_engine::find_command("vessel.regenerate").is_some());
        assert!(super::about_text().contains("SolidCraft"));
        assert!(solidcraft_ui_egui::toolbar::TABS.contains(&triya_vessel::commands::TAB), "the VESSEL tab seam is in place");
        assert_eq!(solidcraft_ui_egui::toolbar::panels(triya_vessel::commands::TAB), triya_vessel::commands::PANELS, "the panels seam is in place");
        // Every vessel command is on one of those panels, so the ribbon shows each of them.
        let lists = solidcraft_ui_egui::toolbar_custom::panel_lists(triya_vessel::commands::TAB);
        for spec in triya_vessel::COMMANDS {
            assert_eq!(lists.iter().filter(|(_, cmds, _)| cmds.iter().any(|c| c.id == spec.id)).count(), 1, "{}", spec.id);
        }
    }
}
