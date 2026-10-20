// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The plan-view orientation widget (PLAN T5.4): the shell as a circle seen down the vessel axis
//! (from above for a vertical vessel, from the left end for a horizontal one), azimuth ticks and
//! labels, the 0° reference and the sense of rotation, every attachment at its angle. Directions
//! come from `triya_vessel::nozzle::azimuth`, the function the generator places nozzles with, so
//! the widget and the model cannot disagree. Clicking the ring turns the selected attachment to
//! that angle (snapped); the 0° reference and the sense are vessel settings.

use egui::{Color32, Pos2, Sense, Stroke, Vec2, vec2};
use serde_json::json;
use solidcraft_ui_egui::theme::Tokens;
use triya_vessel::model::{AngleSense, AngleZero, AttachmentKind, Frame, Global, Orientation};
use triya_vessel::nozzle::azimuth;

use super::VesselPanel;
use super::edits::{self, fmt_num};

/// Where world direction `d` (perpendicular to the vessel axis) points on the screen: +X right
/// and +Y up looking down a vertical vessel; −Y right and +Z up looking along +X from the left
/// end of a horizontal one.
pub fn screen_dir(g: &Global, d: [f64; 3]) -> Vec2 {
    match g.orientation {
        Orientation::Vertical => vec2(d[0] as f32, -d[1] as f32),
        Orientation::Horizontal => vec2(-d[1] as f32, -d[2] as f32),
    }
}

/// The azimuth (degrees, [0, 360)) whose direction is nearest the screen direction `v`.
pub fn angle_at(g: &Global, v: Vec2) -> f64 {
    let want = v.y.atan2(v.x);
    let mut best = (f32::INFINITY, 0.0);
    for i in 0..720 {
        let a = i as f64 * 0.5;
        let s = screen_dir(g, azimuth(g, a));
        let mut diff = (s.y.atan2(s.x) - want).abs();
        if diff > std::f32::consts::PI {
            diff = 2.0 * std::f32::consts::PI - diff;
        }
        if diff < best.0 {
            best = (diff, a);
        }
    }
    best.1
}

fn zero_label(g: &Global) -> &'static str {
    match (g.orientation, g.angle_zero) {
        (Orientation::Vertical, AngleZero::PlusX) => "0° = +X",
        (Orientation::Vertical, _) => "0° = N (+Y)",
        (Orientation::Horizontal, AngleZero::PlusY) => "0° = +Y",
        (Orientation::Horizontal, _) => "0° = top",
    }
}

/// The outside radius the circle stands for: the largest cylinder's (else the largest station's).
fn shell_radius(p: &VesselPanel) -> f64 {
    let Some(r) = p.resolved() else { return 500.0 };
    let cyl = r.stations.iter().filter(|s| s.kind == "cylinder").map(|s| s.r_from + s.thickness).fold(0.0, f64::max);
    if cyl > 0.0 { cyl } else { r.stations.iter().map(|s| s.r_from.max(s.r_to) + s.thickness).fold(500.0, f64::max) }
}

pub fn show(p: &mut VesselPanel, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let Some(doc) = p.doc.clone() else {
        ui.label("The design has no vessel.");
        return;
    };
    let g = doc.global.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label("0° at");
        let mut z = g.angle_zero;
        let names: &[(AngleZero, &str)] = match g.orientation {
            Orientation::Vertical => &[(AngleZero::PlantNorth, "Plant north"), (AngleZero::PlusX, "+X")],
            Orientation::Horizontal => &[(AngleZero::PlantNorth, "Top"), (AngleZero::PlusY, "+Y")],
        };
        let shown = names.iter().find(|(v, _)| *v == z).map_or("Plant north", |(_, l)| *l);
        egui::ComboBox::from_id_salt("triya_plan_zero").selected_text(shown).show_ui(ui, |ui| {
            for (v, l) in names {
                ui.selectable_value(&mut z, *v, *l);
            }
        });
        if z != g.angle_zero {
            p.queue(Some(edits::global_field("angle_zero", json!(z))));
        }
        let mut s = g.angle_sense;
        let view = if g.orientation == Orientation::Vertical { "from above" } else { "from the left end" };
        let label = |s: AngleSense| match s {
            AngleSense::ClockwiseFromAbove => format!("clockwise {view}"),
            AngleSense::CounterclockwiseFromAbove => format!("counter-clockwise {view}"),
        };
        egui::ComboBox::from_id_salt("triya_plan_sense").selected_text(label(s)).show_ui(ui, |ui| {
            ui.selectable_value(&mut s, AngleSense::ClockwiseFromAbove, label(AngleSense::ClockwiseFromAbove));
            ui.selectable_value(&mut s, AngleSense::CounterclockwiseFromAbove, label(AngleSense::CounterclockwiseFromAbove));
        });
        if s != g.angle_sense {
            p.queue(Some(edits::global_field("angle_sense", json!(s))));
        }
    });
    let size = ui.available_width().clamp(220.0, 360.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let painter = ui.painter_at(rect);
    let c = rect.center();
    let ring = size * 0.32;
    let r_shell = shell_radius(p);
    let scale = ring / r_shell.max(1.0) as f32;
    let at = |dir: Vec2, r: f32| -> Pos2 { c + dir * r };
    let dim = t.text_dim;
    painter.circle_stroke(c, ring, Stroke::new(2.0, t.text));
    // Ticks inside the ring: every 22.5°, long every 90°, labelled every 45° (the outside is
    // left to the attachments and their ids).
    for i in 0..16 {
        let a = i as f64 * 22.5;
        let d = screen_dir(&g, azimuth(&g, a));
        let long = i % 4 == 0;
        painter.line_segment([at(d, ring), at(d, ring - if long { 10.0 } else { 5.0 })], Stroke::new(1.0, dim));
        if i % 2 == 0 {
            painter.text(at(d, ring - 20.0), egui::Align2::CENTER_CENTER, format!("{}°", fmt_num(a)), egui::FontId::proportional(9.0), dim);
        }
    }
    // The 0° reference arrow and the sense of rotation.
    let zero = screen_dir(&g, azimuth(&g, 0.0));
    painter.arrow(c, zero * (ring * 0.55), Stroke::new(1.5, t.accent));
    painter.text(rect.left_top() + vec2(4.0, 4.0), egui::Align2::LEFT_TOP, zero_label(&g), egui::FontId::proportional(11.0), t.accent);
    let arc: Vec<Pos2> = (0..=12).map(|i| at(screen_dir(&g, azimuth(&g, 20.0 + i as f64 * 5.0)), ring * 0.4)).collect();
    if let [.., a, b] = arc.as_slice() {
        painter.arrow(*a, *b - *a, Stroke::new(1.0, dim));
    }
    painter.add(egui::Shape::line(arc, Stroke::new(1.0, dim)));
    // Attachments at their angles.
    for a in &doc.attachments {
        let selected = p.selected_attachment.as_deref() == Some(a.id.as_str());
        let col = if selected { t.accent } else { t.text };
        let d = screen_dir(&g, azimuth(&g, a.angle_deg));
        match &a.kind {
            AttachmentKind::Nozzle { projection, .. } => {
                let on_head = matches!(a.frame, Frame::ParallelToAxis { .. } | Frame::HeadRadialAtR { .. });
                if on_head {
                    let radius = match a.frame {
                        Frame::ParallelToAxis { radius_mm } | Frame::HeadRadialAtR { radius_mm } => radius_mm,
                        _ => 0.0,
                    };
                    let pos = at(d, (radius as f32 * scale).min(ring));
                    painter.circle_stroke(pos, 5.0, Stroke::new(1.5, col));
                    painter.text(pos + vec2(7.0, -7.0), egui::Align2::LEFT_BOTTOM, &a.id, egui::FontId::proportional(10.0), col);
                } else {
                    let out = match projection.from {
                        triya_vessel::model::ProjectionFrom::Cl => projection.mm as f32 * scale,
                        _ => ring + projection.mm as f32 * scale,
                    }
                    .clamp(ring + 18.0, size * 0.44);
                    painter.line_segment([at(d, ring), at(d, out)], Stroke::new(if selected { 3.0 } else { 2.0 }, col));
                    painter.line_segment([at(d, out) + d.rot90() * 5.0, at(d, out) - d.rot90() * 5.0], Stroke::new(2.0, col));
                    painter.text(at(d, out + 11.0), egui::Align2::CENTER_CENTER, &a.id, egui::FontId::proportional(10.0), col);
                }
            }
            AttachmentKind::StiffeningRing { .. } | AttachmentKind::InsulationRing { .. } | AttachmentKind::LimpetCoil { .. } => {}
            kind => {
                // Supports and details: a small block on the shell; patterns at each copy.
                let (count, start) = match kind {
                    AttachmentKind::Leg { count, start_angle_deg, .. } => (*count, *start_angle_deg),
                    AttachmentKind::Bracket { count, .. } | AttachmentKind::AnchorChair { count, .. } => (*count, a.angle_deg),
                    _ => (1, a.angle_deg),
                };
                let n = count.clamp(1, 64);
                for k in 0..n {
                    let ang = start + 360.0 * f64::from(k) / f64::from(n);
                    let d = screen_dir(&g, azimuth(&g, ang));
                    let pos = at(d, ring);
                    painter.rect_filled(
                        egui::Rect::from_center_size(pos, vec2(8.0, 8.0)),
                        1.0,
                        if selected { t.accent } else { Color32::from_gray(140) },
                    );
                    if k == 0 {
                        painter.text(at(d, ring + 14.0), egui::Align2::CENTER_CENTER, &a.id, egui::FontId::proportional(9.0), col);
                    }
                }
            }
        }
    }
    // A click on the ring turns the selected attachment to that angle.
    if resp.clicked()
        && let (Some(pos), Some(id)) = (resp.interact_pointer_pos(), p.selected_attachment.clone())
        && let Some(a) = doc.attachments.iter().find(|a| a.id == id)
    {
        let step = if p.no_snap { 0.0 } else { edits::snap_step(&doc, &a.host) };
        let angle = edits::snap(angle_at(&g, pos - c), step);
        p.queue(edits::attachment_field(&doc, &id, &["angle_deg"], Some(json!(angle))));
    }
    let hint = match &p.selected_attachment {
        Some(id) => format!("Click the ring to turn {id} to that angle."),
        None => "Select an attachment (Attachments page or 3D view), then click the ring to turn it.".into(),
    };
    ui.label(egui::RichText::new(hint).color(dim).size(11.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_directions_follow_the_convention() {
        let mut g = Global::default();
        // Vertical, plant north = +Y up, clockwise from above: 90° points right.
        let up = screen_dir(&g, azimuth(&g, 0.0));
        let right = screen_dir(&g, azimuth(&g, 90.0));
        assert!((up - vec2(0.0, -1.0)).length() < 1e-6, "{up:?}");
        assert!((right - vec2(1.0, 0.0)).length() < 1e-6, "{right:?}");
        assert_eq!(angle_at(&g, vec2(1.0, 0.0)), 90.0);
        g.angle_sense = AngleSense::CounterclockwiseFromAbove;
        assert_eq!(angle_at(&g, vec2(1.0, 0.0)), 270.0);
        // Horizontal: 0° on top, clockwise looking from the left end.
        let g = Global { orientation: Orientation::Horizontal, datum: triya_vessel::model::Datum::LeftTl, ..Global::default() };
        assert!((screen_dir(&g, azimuth(&g, 0.0)) - vec2(0.0, -1.0)).length() < 1e-6);
        assert!((screen_dir(&g, azimuth(&g, 90.0)) - vec2(1.0, 0.0)).length() < 1e-6);
        assert_eq!(angle_at(&g, vec2(0.0, 1.0)), 180.0);
    }
}
