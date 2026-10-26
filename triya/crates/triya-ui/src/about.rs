// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D's About window, drawn by the app shells around `SolveApp::ui`.
//!
//! Upstream's Help menu (the ? button) sets `app.help.about`; [`About::show`] takes that request
//! over (clears the flag and opens this window instead), so the product people see is Triya CAD 3D
//! with the attribution sentence from [`triya_brand::about_text`]. The shell calls [`About::intercept`]
//! before `app.ui`, so a request from the last frame (control channel `ui.help`) never reaches
//! upstream's window, and [`About::show`] after it, so a menu click handled inside `app.ui` opens
//! this one. Upstream draws its own window in the frame of that click, so it can flash for a frame.

use egui::{RichText, vec2};
use solidcraft_ui_egui::SolveApp;
use solidcraft_ui_egui::theme::Tokens;

/// Window title (also its egui id source).
pub const TITLE: &str = "About Triya CAD 3D";
/// Rows of the root `ATTRIBUTION.md`, shown in the window.
const ATTRIBUTION: &str = include_str!("../../../../ATTRIBUTION.md");

/// The About window's state.
#[derive(Default)]
pub struct About {
    open: bool,
    mark: Option<egui::TextureHandle>,
}

impl About {
    /// Open the window (e.g. from a Triya panel's button).
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Whether the window is open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Take over upstream's About request, if any.
    pub fn intercept(&mut self, app: &mut SolveApp) {
        if app.help.about {
            app.help.about = false;
            self.open = true;
        }
    }

    /// The diagnostics line for bug reports: Triya's version first, then upstream's lines.
    pub fn diagnostics(app: &SolveApp) -> String {
        format!(
            "{} {} (upstream {} v{} {}, build {})\n{}",
            triya_brand::APP_NAME,
            triya_brand::VERSION,
            triya_brand::UPSTREAM_NAME,
            triya_brand::UPSTREAM_VERSION,
            triya_brand::UPSTREAM_SHA,
            solidcraft_ui_egui::help::COMMIT,
            solidcraft_ui_egui::help::diagnostics(app)
        )
    }

    /// Intercept upstream's request and draw the window while it is open.
    pub fn show(&mut self, app: &mut SolveApp, ctx: &egui::Context) {
        self.intercept(app);
        if !self.open {
            return;
        }
        let t = Tokens::get();
        let mark = self
            .mark
            .get_or_insert_with(|| {
                let image = crate::icons::mark_image(128).unwrap_or_else(|| egui::ColorImage::filled([1, 1], egui::Color32::TRANSPARENT));
                ctx.load_texture("triya_mark", image, egui::TextureOptions::LINEAR)
            })
            .clone();
        let mut copy = false;
        let closed = solidcraft_ui_egui::ui_kit::Window::new(egui::Id::new("triya_about"), "help", TITLE).show_body(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(&mark).fit_to_exact_size(vec2(56.0, 56.0)));
                ui.vertical(|ui| {
                    ui.label(RichText::new(triya_brand::APP_NAME).size(22.0).strong());
                    ui.label(format!("Version {} · build {}", triya_brand::VERSION, solidcraft_ui_egui::help::COMMIT));
                    ui.label(RichText::new(format!("{} · {}", triya_brand::ORG, triya_brand::LICENSE)).color(t.text_dim));
                });
            });
            ui.add_space(6.0);
            ui.label(triya_brand::about_text());
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to("Home page", triya_brand::HOMEPAGE);
                ui.hyperlink_to("Source", triya_brand::REPO);
                ui.hyperlink_to("Upstream", triya_brand::UPSTREAM_REPO);
                if ui.button("Copy diagnostic information").clicked() {
                    copy = true;
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Attributions (ATTRIBUTION.md)").show(ui, |ui| {
                    ui.label(RichText::new(ATTRIBUTION).monospace().size(11.0));
                });
                egui::CollapsingHeader::new("Fonts").show(ui, |ui| {
                    ui.label(solidcraft_ui_egui::i18n::tr("help.font_details"));
                });
                egui::CollapsingHeader::new("Licences").show(ui, |ui| {
                    use solidcraft_engine::licences::{LICENSE_APACHE, LICENSE_MIT, NOTICE, THIRD_PARTY};
                    for (title, text) in [("NOTICE", NOTICE), ("MIT licence", LICENSE_MIT), ("Apache licence 2.0", LICENSE_APACHE)] {
                        egui::CollapsingHeader::new(title).show(ui, |ui| {
                            ui.label(RichText::new(text).monospace().size(11.0));
                        });
                    }
                    egui::CollapsingHeader::new("Third-party licences").show(ui, |ui| {
                        if ui.button("Copy all licences").clicked() {
                            ui.ctx().copy_text(solidcraft_engine::licences::all());
                        }
                        ui.label(RichText::new(THIRD_PARTY).monospace().size(11.0));
                    });
                });
            });
        });
        if copy {
            ctx.copy_text(Self::diagnostics(app));
        }
        if closed {
            self.open = false;
        }
    }
}
