// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The Vessel panel: a side panel docked on the viewport's left edge while the `VESSEL` tab is
//! active, drawn by the Triya app shells after `SolveApp::ui`. It reads the vessel document from
//! the design and acts only through `app.run("vessel.*", …)` ([`edits::Action`]); every change to
//! the document is a `vessel.set {patch}` of JSON pointers, the same JSON a script or MCP client
//! sends. Pages: Stack (the element editor, T5.2), Attachments (nozzles, supports and details,
//! T5.3), Plan (the orientation widget, T5.4), Drawing (the GA sheet and its export, T6.7, which
//! runs `drawing.*`) and JSON (the whole document).

mod attach;
pub mod drawing;
pub mod edits;
pub mod plan;
pub mod regen;
mod stack;

pub use attach::owner_of;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use egui::RichText;
use serde_json::{Value, json};
use solidcraft_ui_egui::SolveApp;
use solidcraft_ui_egui::theme::Tokens;
use triya_vessel::eval::Resolved;
use triya_vessel::model::VesselDoc;

use edits::Action;

/// The panel's pages.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Stack,
    Attachments,
    Plan,
    Drawing,
    Json,
}

type SavePick = Pin<Box<dyn Future<Output = Option<String>>>>;

/// The Vessel panel's own state.
#[derive(Default)]
pub struct VesselPanel {
    pub page: Page,
    /// The document and its stations as of `synced_revision`.
    doc: Option<VesselDoc>,
    resolved: Option<Result<Resolved, String>>,
    synced_revision: Option<u64>,
    /// The element the stack editor shows in detail.
    pub selected_element: Option<String>,
    /// The attachment the attachment page shows in detail.
    pub selected_attachment: Option<String>,
    /// The body the 3D selection named last frame (the panel follows changes).
    last_selection: Option<String>,
    /// Don't follow the 3D selection / don't snap angles (both on by default).
    pub no_follow: bool,
    pub no_snap: bool,
    /// NPS sizes and their schedules, from the pipe tables.
    pipe_sizes: Vec<(String, Vec<String>)>,
    /// Text being typed into a field, by field key (committed on Enter or when focus leaves).
    editing: HashMap<String, String>,
    /// The JSON page's text.
    text: String,
    error: Option<String>,
    note: Option<String>,
    regen: regen::Regen,
    /// A save dialog in flight: the command to run with its `path`, and its other parameters.
    save_pick: Option<(SavePick, &'static str, Value)>,
    /// The Drawing page.
    pub drawing: drawing::DrawingPage,
    /// Commands queued by the widgets this frame, run after drawing.
    queued: Vec<Action>,
    show_about: bool,
}

impl VesselPanel {
    /// The vessel document the design holds now, if any.
    pub fn doc(&self) -> Option<&VesselDoc> {
        self.doc.as_ref()
    }

    /// Re-read the design's vessel when the design changed under the panel.
    pub fn sync(&mut self, app: &SolveApp) {
        if self.synced_revision == Some(app.session.revision) {
            return;
        }
        self.synced_revision = Some(app.session.revision);
        let has = app.session.doc.metadata.get(triya_vessel::commands::METADATA_KEY).is_some();
        self.doc = if has { triya_vessel::commands::stored_doc(&app.session).ok() } else { None };
        self.resolved = self.doc.as_ref().map(|d| triya_vessel::eval::resolve(d).map_err(|e| e.to_string()));
        self.text = match &self.doc {
            Some(d) => d.to_value().ok().and_then(|v| serde_json::to_string_pretty(&v).ok()).unwrap_or_default(),
            None => "(no vessel: New Vessel makes a 1000 × 10 × 2000 cylinder)".into(),
        };
        if let (Some(d), Some(sel)) = (&self.doc, &self.selected_element)
            && d.element(sel).is_none()
        {
            self.selected_element = None;
        }
        if let (Some(d), Some(sel)) = (&self.doc, &self.selected_attachment)
            && !d.attachments.iter().any(|a| &a.id == sel)
        {
            self.selected_attachment = None;
        }
        self.editing.clear();
    }

    /// The stations, when the document resolves.
    fn resolved(&self) -> Option<&Resolved> {
        self.resolved.as_ref().and_then(|r| r.as_ref().ok())
    }

    /// Queue a command for the end of this frame.
    pub fn queue(&mut self, a: Option<Action>) {
        if let Some(a) = a {
            self.queued.push(a);
        }
    }

    /// Run one command now and read the design back on the next frame.
    pub fn run(&mut self, app: &mut SolveApp, a: Action) -> Option<Value> {
        let r = app.run(a.id, a.params);
        self.synced_revision = None;
        match r {
            Ok(v) => {
                self.error = None;
                Some(v)
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        }
    }

    /// The commands the widgets queued this frame (for tests); running them is [`Self::flush`].
    pub fn take_queued(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.queued)
    }

    fn flush(&mut self, app: &mut SolveApp) {
        for a in self.take_queued() {
            let id = a.id;
            let Some(v) = self.run(app, a) else { break };
            let added = v.get("id").and_then(Value::as_str).map(str::to_string);
            match id {
                "vessel.element.add" => self.selected_element = added,
                "vessel.attachment.add" | "vessel.support.add" => {
                    self.selected_attachment = added;
                    self.page = Page::Attachments;
                }
                _ => {}
            }
            if id == "vessel.inspect" {
                app.session.echo(serde_json::to_string(&v).unwrap_or_default());
            }
        }
    }

    /// A numeric field committed on Enter or when focus leaves: `Some(None)` clears it (empty
    /// text), `Some(Some(x))` sets it.
    fn num_field(&mut self, ui: &mut egui::Ui, key: &str, current: Option<f64>, hint: &str, width: f32) -> Option<Option<f64>> {
        let shown = current.map(edits::fmt_num).unwrap_or_default();
        let mut text = self.editing.get(key).cloned().unwrap_or_else(|| shown.clone());
        let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint).desired_width(width));
        if r.changed() {
            self.editing.insert(key.to_string(), text.clone());
        }
        if r.lost_focus() {
            let typed = self.editing.remove(key)?;
            if typed.trim() == shown {
                return None;
            }
            if typed.trim().is_empty() {
                return Some(None);
            }
            match edits::parse_mm(&typed) {
                Some(x) => return Some(Some(x)),
                None => self.error = Some(format!("`{}` is not a number", typed.trim())),
            }
        }
        None
    }

    /// A text field committed on Enter or when focus leaves.
    fn text_field(&mut self, ui: &mut egui::Ui, key: &str, current: &str, hint: &str, width: f32) -> Option<String> {
        let mut text = self.editing.get(key).cloned().unwrap_or_else(|| current.to_string());
        let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint).desired_width(width));
        if r.changed() {
            self.editing.insert(key.to_string(), text.clone());
        }
        if r.lost_focus() {
            let typed = self.editing.remove(key)?;
            if typed != current {
                return Some(typed);
            }
        }
        None
    }

    /// Ask where to save `name`, then run `command {path, ...params}`.
    fn save_as(&mut self, app: &mut SolveApp, name: &str, ext: &str, command: &'static str, params: Value) {
        match app.services.pick_save.as_ref() {
            Some(pick) => self.save_pick = Some((pick(name, &[ext]), command, params)),
            None => self.error = Some(format!("this app has no save dialog: run {command} with a path")),
        }
    }

    /// Ask where to save the BOM, then run `vessel.bom.export {path}`.
    fn export_bom(&mut self, app: &mut SolveApp) {
        let name = self.doc.as_ref().map_or("vessel".into(), |d| d.id.clone());
        self.save_as(app, &format!("{name}-bom.csv"), "csv", "vessel.bom.export", json!({}));
    }

    fn poll_save(&mut self, app: &mut SolveApp, ctx: &egui::Context) {
        let Some((f, _, _)) = self.save_pick.as_mut() else { return };
        let mut cx = Context::from_waker(Waker::noop());
        match f.as_mut().poll(&mut cx) {
            Poll::Pending => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
            Poll::Ready(path) => {
                let Some((_, id, mut params)) = self.save_pick.take() else { return };
                if let Some(path) = path {
                    params["path"] = json!(path);
                    if self.run(app, Action { id, params }).is_some() {
                        self.note = Some(format!("Written to {path}"));
                    }
                }
            }
        }
    }

    fn take_regen(&mut self, app: &mut SolveApp, ctx: &egui::Context) {
        if let Some(done) = self.regen.poll(app, ctx) {
            self.synced_revision = None;
            match done {
                regen::Finished::Done(v) => {
                    let w = v.get("warnings").and_then(Value::as_array).map_or(0, Vec::len);
                    let msg = format!("Vessel regenerated in {:.0} s ({w} warnings)", self.regen.elapsed_s());
                    app.set_status(msg.clone(), false);
                    self.note = Some(msg);
                    self.error = None;
                }
                regen::Finished::Failed(e) => self.error = Some(e),
            }
        }
    }

    /// Draw the panel (only while the `VESSEL` tab is active) and run what it queued.
    pub fn show(&mut self, app: &mut SolveApp, ctx: &egui::Context) {
        self.take_regen(app, ctx);
        self.poll_save(app, ctx);
        if app.ui.tab != triya_vessel::commands::TAB || app.home.open {
            return;
        }
        self.sync(app);
        let vp = app.viewport.rect.unwrap_or_else(|| ctx.content_rect());
        let side = solidcraft_ui_egui::frame::Side::Left;
        let anchor = egui::pos2(vp.left() + 8.0, vp.top() + 8.0);
        solidcraft_ui_egui::frame::docked_on(
            RichText::new("Vessel").strong().size(13.0),
            egui::Id::new("triya_vessel_panel"),
            anchor,
            vp.height() - 16.0,
            solidcraft_ui_egui::frame::Width::Wide,
            side,
        )
        .frame(solidcraft_ui_egui::frame::dock_frame(ctx, side))
        .show(ctx, |ui| self.contents(app, ui));
        self.flush(app);
    }

    /// The panel's contents (also drawn by the tests into a bare `Ui`).
    pub fn contents(&mut self, app: &mut SolveApp, ui: &mut egui::Ui) {
        let t = Tokens::get();
        attach::follow_selection(self, app);
        ui.horizontal(|ui| {
            ui.label(RichText::new("VESSEL").strong());
            if let Some(d) = &self.doc {
                ui.label(RichText::new(format!("{} {}", d.id, d.title.clone().unwrap_or_default())).color(t.text_dim));
            }
        });
        let busy = self.regen.busy();
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy, egui::Button::new("New")).on_hover_text("vessel.new: a one-cylinder vessel (replaces the current one)").clicked()
            {
                self.queue(Some(Action { id: "vessel.new", params: json!({"replace": true}) }));
            }
            let has = self.doc.is_some();
            if ui
                .add_enabled(has && !busy, egui::Button::new("Regenerate"))
                .on_hover_text("vessel.regenerate: build the bodies (one undo step), off the window thread")
                .clicked()
            {
                self.regen.start(app);
            }
            if ui.add_enabled(has, egui::Button::new("Export BOM…")).on_hover_text("vessel.bom.export: the BOM as CSV").clicked() {
                self.export_bom(app);
            }
            ui.separator();
            ui.selectable_value(&mut self.page, Page::Stack, "Stack");
            ui.selectable_value(&mut self.page, Page::Attachments, "Attachments");
            ui.selectable_value(&mut self.page, Page::Plan, "Plan");
            ui.selectable_value(&mut self.page, Page::Drawing, "Drawing");
            ui.selectable_value(&mut self.page, Page::Json, "JSON");
        });
        self.regen.show(ui);
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(t.error));
        } else if let Some(n) = &self.note {
            ui.label(RichText::new(n).color(t.text_dim));
        }
        if let Some(Err(e)) = &self.resolved {
            ui.label(RichText::new(e).color(t.warning));
        }
        ui.separator();
        ui.add_enabled_ui(!busy, |ui| match self.page {
            Page::Stack => stack::show(self, ui),
            Page::Attachments => attach::show(self, app, ui),
            Page::Plan => plan::show(self, ui),
            Page::Drawing => drawing::show(self, app, ui),
            Page::Json => self.json_page(ui),
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new(triya_brand::APP_NAME).strong());
            if ui.small_button("About").clicked() {
                self.show_about = !self.show_about;
            }
        });
        if self.show_about {
            ui.label(RichText::new(crate::about_text()).color(t.text_dim).size(11.0));
        }
    }

    fn json_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Apply").on_hover_text("vessel.set: store the JSON below as the vessel").clicked() {
                match serde_json::from_str::<Value>(&self.text) {
                    Ok(doc) => self.queue(Some(Action { id: "vessel.set", params: json!({"doc": doc}) })),
                    Err(e) => self.error = Some(format!("JSON: {e}")),
                }
            }
            if ui.button("Inspect").on_hover_text("vessel.inspect: stations and the command plan, to the log").clicked() {
                self.queue(Some(Action { id: "vessel.inspect", params: json!({}) }));
            }
        });
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut self.text).code_editor().desired_width(f32::INFINITY).desired_rows(18));
        });
    }
}
