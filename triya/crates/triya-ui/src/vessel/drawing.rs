// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The Drawing page (PLAN T6.1, T6.7): the vessel's GA sheet painted with epaint, Generate
//! (`drawing.generate_ga {size, sheet: true}`) and Export SVG / DXF / PDF (`drawing.export
//! {path}`; in the browser the save picker answers `/downloads/<name>`, which becomes a
//! download). The page only runs those commands; the sheet it paints is the one the command
//! returned, through [`EguiPlotter`], the same `Plotter` calls the SVG, DXF and PDF writers get.
//! The sheet is paper: black ink on white whatever the theme, like the exported files.

use egui::{Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Vec2, pos2, vec2};
use serde_json::{Value, json};
use solidcraft_ui_egui::SolveApp;
use solidcraft_ui_egui::theme::Tokens;
use triya_drawing::Sheet;
use triya_drawing::model::{Layer, LineStyle, P2, SheetSize};
use triya_drawing::plot::{Align, Plotter, plot_sheet};

use super::VesselPanel;
use super::edits::Action;

/// Ink colours, as the SVG writer's.
fn ink(layer: Layer) -> Color32 {
    match layer {
        Layer::Visible | Layer::Frame | Layer::Text => Color32::BLACK,
        Layer::Hidden => Color32::from_gray(0x40),
        Layer::Center => Color32::from_rgb(0xa0, 0, 0),
        Layer::Dimension => Color32::from_rgb(0, 0x38, 0xa8),
        Layer::Hatch => Color32::from_rgb(0x40, 0x60, 0x40),
    }
}

/// A `Plotter` that paints a sheet into an egui painter: sheet millimetres (y up) map to the
/// screen at `px_per_mm` with the sheet's top-left corner at `top_left`.
pub struct EguiPlotter<'a> {
    painter: &'a egui::Painter,
    top_left: Pos2,
    px_per_mm: f32,
    sheet_h: f64,
    layer: Layer,
    style: LineStyle,
    width: f64,
    /// Shapes painted (for tests and for a cap on hostile sheets).
    pub shapes: usize,
}

/// Most shapes one frame paints.
const MAX_SHAPES: usize = 400_000;

impl<'a> EguiPlotter<'a> {
    pub fn new(painter: &'a egui::Painter, top_left: Pos2, px_per_mm: f32, sheet_h: f64) -> EguiPlotter<'a> {
        EguiPlotter { painter, top_left, px_per_mm, sheet_h, layer: Layer::Visible, style: LineStyle::Continuous, width: 0.35, shapes: 0 }
    }

    fn pos(&self, p: P2) -> Pos2 {
        pos2(self.top_left.x + p.x as f32 * self.px_per_mm, self.top_left.y + (self.sheet_h - p.y) as f32 * self.px_per_mm)
    }

    fn stroke(&self) -> Stroke {
        Stroke::new((self.width as f32 * self.px_per_mm).clamp(0.5, 4.0), ink(self.layer))
    }

    fn visible(&self, pts: &[Pos2]) -> bool {
        let clip = self.painter.clip_rect().expand(4.0);
        let r = Rect::from_points(pts);
        pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()) && clip.intersects(r)
    }

    fn add(&mut self, shape: Shape) {
        if self.shapes < MAX_SHAPES {
            self.shapes += 1;
            self.painter.add(shape);
        }
    }

    /// Dash and gap (screen px) of the current line style; `None` draws it solid.
    fn dashes(&self) -> Option<(f32, f32)> {
        let (d, g) = match self.style {
            LineStyle::Continuous => return None,
            LineStyle::Dashed => (3.0, 1.5),
            LineStyle::Hidden => (2.0, 1.0),
            LineStyle::Center | LineStyle::Phantom => (12.0, 2.0),
        };
        let (d, g) = (d * self.px_per_mm, g * self.px_per_mm);
        (g >= 1.5).then_some((d, g))
    }

    fn path(&mut self, pts: Vec<Pos2>, closed: bool) {
        if pts.len() < 2 || !self.visible(&pts) {
            return;
        }
        let stroke = self.stroke();
        match self.dashes() {
            Some((d, g)) => {
                let mut pts = pts;
                if closed && let Some(f) = pts.first().copied() {
                    pts.push(f);
                }
                for s in Shape::dashed_line(&pts, stroke, d, g) {
                    self.add(s);
                }
            }
            None if closed => self.add(Shape::closed_line(pts, stroke)),
            None => self.add(Shape::line(pts, stroke)),
        }
    }

    /// Points along a curve of screen length about `len_px`.
    fn segments(len_px: f32) -> usize {
        if len_px.is_finite() { ((len_px / 6.0).ceil() as usize).clamp(8, 160) } else { 8 }
    }
}

impl Plotter for EguiPlotter<'_> {
    fn set_layer(&mut self, layer: Layer) {
        self.layer = layer;
    }
    fn set_line_style(&mut self, style: LineStyle) {
        self.style = style;
    }
    fn set_width(&mut self, width: f64) {
        if width.is_finite() && width > 0.0 {
            self.width = width;
        }
    }
    fn line(&mut self, a: P2, b: P2) {
        let pts = vec![self.pos(a), self.pos(b)];
        self.path(pts, false);
    }
    fn polyline(&mut self, pts: &[P2], closed: bool) {
        let pts = pts.iter().map(|p| self.pos(*p)).collect();
        self.path(pts, closed);
    }
    fn arc(&mut self, center: P2, radius: f64, start: f64, end: f64) {
        let sweep = (end - start).rem_euclid(std::f64::consts::TAU);
        let sweep = if sweep < 1e-12 { std::f64::consts::TAU } else { sweep };
        let n = Self::segments(radius as f32 * sweep as f32 * self.px_per_mm);
        let pts = (0..=n)
            .map(|i| {
                let a = start + sweep * i as f64 / n as f64;
                self.pos(P2::new(center.x + radius * a.cos(), center.y + radius * a.sin()))
            })
            .collect();
        self.path(pts, false);
    }
    fn circle(&mut self, center: P2, radius: f64) {
        if !(radius > 0.0 && radius.is_finite()) {
            return;
        }
        let c = self.pos(center);
        let r = radius as f32 * self.px_per_mm;
        if self.dashes().is_none() {
            if self.visible(&[c - vec2(r, r), c + vec2(r, r)]) {
                let stroke = self.stroke();
                self.add(Shape::circle_stroke(c, r, stroke));
            }
            return;
        }
        let n = Self::segments(r * std::f32::consts::TAU);
        let pts = (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                self.pos(P2::new(center.x + radius * a.cos(), center.y + radius * a.sin()))
            })
            .collect();
        self.path(pts, true);
    }
    fn ellipse(&mut self, center: P2, rx: f64, ry: f64, rotation: f64, start: f64, end: f64) {
        if !(rx > 0.0 && ry > 0.0 && rx.is_finite() && ry.is_finite()) {
            return;
        }
        let full = (end - start).abs() >= std::f64::consts::TAU - 1e-9 || (end - start).rem_euclid(std::f64::consts::TAU) < 1e-12;
        let sweep = if full { std::f64::consts::TAU } else { (end - start).rem_euclid(std::f64::consts::TAU) };
        let n = Self::segments(rx.max(ry) as f32 * sweep as f32 * self.px_per_mm);
        let pts =
            (0..=n).map(|i| self.pos(triya_drawing::model::ellipse_pt(center, rx, ry, rotation, start + sweep * i as f64 / n as f64))).collect();
        self.path(pts, false);
    }
    fn fill(&mut self, pts: &[P2]) {
        let pts: Vec<Pos2> = pts.iter().map(|p| self.pos(*p)).collect();
        if pts.len() < 3 || !self.visible(&pts) {
            return;
        }
        self.add(Shape::convex_polygon(pts, ink(self.layer), Stroke::NONE));
    }
    fn text(&mut self, at: P2, height: f64, angle: f64, align: Align, text: &str) {
        let px = height as f32 * self.px_per_mm;
        if !(px >= 2.5 && px.is_finite() && angle.is_finite()) || text.is_empty() {
            return;
        }
        let anchor = self.pos(at);
        if !self.painter.clip_rect().expand(px * text.chars().count() as f32).contains(anchor) {
            return;
        }
        let galley = self.painter.layout_no_wrap(text.to_string(), FontId::proportional(px), ink(self.layer));
        let w = galley.size().x;
        let dx = match align {
            Align::Left => 0.0,
            Align::Center => -w / 2.0,
            Align::Right => -w,
        };
        // The sheet's text sits on its baseline at `at`; a galley hangs from its top-left corner.
        let (dx, dy) = (dx, -px * 0.8);
        let t = -(angle as f32);
        let pos = anchor + vec2(dx * t.cos() - dy * t.sin(), dx * t.sin() + dy * t.cos());
        self.add(Shape::Text(egui::epaint::TextShape::new(pos, galley, ink(self.layer)).with_angle(t)));
    }
}

/// The sheet preview: fitted to its rectangle, scroll to zoom at the pointer, drag to pan,
/// double-click to fit again.
#[derive(Clone, Debug)]
pub struct SheetPreview {
    pub zoom: f32,
    pub pan: Vec2,
}

impl Default for SheetPreview {
    fn default() -> Self {
        SheetPreview { zoom: 1.0, pan: Vec2::ZERO }
    }
}

impl SheetPreview {
    /// Paint `sheet` into a `size` rectangle; returns the shapes painted.
    pub fn show(&mut self, ui: &mut egui::Ui, sheet: &Sheet, size: Vec2) -> usize {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let (w, h) = sheet.size.mm();
        let fit = (rect.width() / w as f32).min(rect.height() / h as f32);
        if resp.dragged() {
            self.pan += resp.drag_delta();
        }
        if resp.double_clicked() {
            *self = SheetPreview::default();
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let old = self.zoom;
                self.zoom = (self.zoom * (scroll / 200.0).exp()).clamp(0.5, 40.0);
                if let Some(p) = resp.hover_pos() {
                    // Keep the sheet point under the pointer where it is.
                    let c = rect.center() + self.pan;
                    self.pan += (p - c) * (1.0 - self.zoom / old);
                }
            }
        }
        let k = fit * self.zoom;
        let paper = Rect::from_center_size(rect.center() + self.pan, vec2(w as f32 * k, h as f32 * k));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Tokens::get().panel_header);
        painter.rect_filled(paper, 0.0, Color32::WHITE);
        let mut plotter = EguiPlotter::new(&painter, paper.min, k, h);
        // A sheet the command built always plots; a failure leaves the paper blank.
        let _ = plot_sheet(&mut plotter, sheet);
        plotter.shapes
    }
}

/// The Drawing page's state.
#[derive(Default)]
pub struct DrawingPage {
    pub size: Option<SheetSize>,
    /// The sheet `drawing.generate_ga` returned, and the design revision it was built from.
    pub sheet: Option<Sheet>,
    built_at: Option<u64>,
    pub summary: String,
    pub warnings: Vec<String>,
    pub preview: SheetPreview,
    pub window: bool,
    window_preview: SheetPreview,
}

impl DrawingPage {
    /// Generate the sheet now: `drawing.generate_ga {size, sheet: true}`.
    pub fn generate(panel: &mut VesselPanel, app: &mut SolveApp) {
        let size = panel.drawing.size.unwrap_or_else(|| triya_drawing::commands::stored_options(&app.session).size);
        let a = Action { id: "drawing.generate_ga", params: json!({"size": size.name(), "sheet": true}) };
        let Some(v) = panel.run(app, a) else { return };
        let d = &mut panel.drawing;
        d.sheet = v.get("sheet").and_then(|s| serde_json::from_value(s.clone()).ok());
        d.built_at = Some(app.session.revision);
        d.warnings = v.get("warnings").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
        d.summary = format!(
            "{} at {}, {} views, {} tables",
            v["size"].as_str().unwrap_or("?"),
            v["scale"].as_str().unwrap_or("?"),
            v["views"].as_array().map_or(0, Vec::len),
            v["tables"].as_array().map_or(0, Vec::len)
        );
    }
}

pub fn show(p: &mut VesselPanel, app: &mut SolveApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let stored = triya_drawing::commands::stored_options(&app.session).size;
    let mut size = p.drawing.size.unwrap_or(stored);
    ui.horizontal(|ui| {
        ui.label("Sheet");
        egui::ComboBox::from_id_salt("triya_drawing_size").selected_text(size.name()).show_ui(ui, |ui| {
            for s in [SheetSize::A0, SheetSize::A1, SheetSize::A2, SheetSize::A3, SheetSize::A4] {
                ui.selectable_value(&mut size, s, s.name());
            }
        });
        p.drawing.size = Some(size);
        let label = if p.drawing.sheet.is_some() { "Update" } else { "Generate" };
        if ui.button(label).on_hover_text("drawing.generate_ga: the GA sheet from the vessel (no regenerate needed)").clicked() {
            DrawingPage::generate(p, app);
        }
        if ui.add_enabled(p.drawing.sheet.is_some(), egui::Button::new("Open large")).clicked() {
            p.drawing.window = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label("Export");
        for (fmt, hint) in [
            ("svg", "drawing.export: SVG"),
            ("dxf", "drawing.export: DXF (layers OUTLINE, HIDDEN, CENTER, DIM, HATCH, TEXT, TITLE)"),
            ("pdf", "drawing.export: one-page PDF"),
        ] {
            if ui.button(fmt.to_ascii_uppercase()).on_hover_text(hint).clicked() {
                let stem = p.drawing.sheet.as_ref().map_or_else(|| "drawing".to_string(), |s| s.title_block.drawing_number.clone());
                p.save_as(app, &format!("{stem}.{fmt}"), fmt, "drawing.export", json!({ "format": fmt }));
            }
        }
    });
    let Some(sheet) = p.drawing.sheet.clone() else {
        ui.label(RichText::new("Generate builds the GA sheet: elevation, section A-A, plan, design data, nozzle schedule, BOM, notes, revisions and the title block.").color(t.text_dim));
        return;
    };
    let stale = p.drawing.built_at != Some(app.session.revision);
    ui.label(
        RichText::new(if stale { format!("{} · the design changed: Update", p.drawing.summary) } else { p.drawing.summary.clone() })
            .color(if stale { t.warning } else { t.text_dim }),
    );
    let w = ui.available_width().max(200.0);
    let (sw, sh) = sheet.size.mm();
    p.drawing.preview.show(ui, &sheet, vec2(w, w * (sh / sw) as f32));
    ui.label(RichText::new("Scroll to zoom, drag to pan, double-click to fit.").color(t.text_dim).size(11.0));
    if !p.drawing.warnings.is_empty() {
        egui::CollapsingHeader::new(format!("{} warnings", p.drawing.warnings.len())).id_salt("triya_drawing_warnings").show(ui, |ui| {
            for w in &p.drawing.warnings {
                ui.label(RichText::new(w).color(t.warning).size(11.0));
            }
        });
    }
    if p.drawing.window {
        let mut open = true;
        let preview = &mut p.drawing.window_preview;
        egui::Window::new(format!("GA drawing {}", sheet.title_block.drawing_number))
            .id(egui::Id::new("triya_drawing_window"))
            .open(&mut open)
            .default_size([1100.0, 800.0])
            .resizable(true)
            .show(ui.ctx(), |ui| {
                let avail = ui.available_size();
                let size = vec2(avail.x.max(200.0), avail.y.max(150.0));
                preview.show(ui, &sheet, size);
            });
        p.drawing.window = open;
    }
}
