// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Plotters (`docs/drawing-model.md` §6): a pen-style sink a sheet is drawn into, in sheet
//! millimetres (y up). [`crate::svg::SvgPlotter`], [`crate::dxf::DxfPlotter`],
//! [`crate::pdf::PdfPlotter`] and the egui preview in `triya-ui` implement the same trait, and
//! [`plot_sheet`] is the one place that turns a [`Sheet`] into pen strokes: the frame, the views
//! (edges, hatching, exploded dimensions, labels, titles), the tables and the title block.

use crate::model::{
    DimKind, Dimension, Edge, FRAME_MARGIN, HatchRegion, Label, LabelStyle, Layer, LineStyle, P2, Sheet, StyledEdge, TABLE_TITLE_H, TEXT_H, TITLE_H,
    Table, View, angle_pt, bounds,
};
use crate::{DrawingError, Result, hatch, template};

/// Horizontal alignment of a text about its anchor (along the text's direction).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

pub trait Plotter {
    fn set_layer(&mut self, layer: Layer);
    fn set_line_style(&mut self, style: LineStyle);
    /// Line width in sheet mm.
    fn set_width(&mut self, width: f64);
    fn line(&mut self, a: P2, b: P2);
    fn polyline(&mut self, pts: &[P2], closed: bool) {
        for w in pts.windows(2) {
            if let [a, b] = w {
                self.line(*a, *b);
            }
        }
        if closed
            && pts.len() > 2
            && let (Some(a), Some(b)) = (pts.last(), pts.first())
        {
            self.line(*a, *b);
        }
    }
    /// Counter-clockwise from `start` to `end` (radians).
    fn arc(&mut self, center: P2, radius: f64, start: f64, end: f64);
    fn circle(&mut self, center: P2, radius: f64);
    /// An ellipse (or an arc of one, counter-clockwise from parameter `start` to `end`).
    fn ellipse(&mut self, center: P2, rx: f64, ry: f64, rotation: f64, start: f64, end: f64) {
        let e = Edge::Ellipse { center, rx, ry, rotation, start, end };
        self.polyline(&e.sample(64), false);
    }
    /// A filled polygon (arrowheads).
    fn fill(&mut self, pts: &[P2]) {
        self.polyline(pts, true);
    }
    /// `height` is the text height in mm, `angle` counter-clockwise in radians; `at` is on the
    /// baseline.
    fn text(&mut self, at: P2, height: f64, angle: f64, align: Align, text: &str);
}

/// An estimate of a text's advance width (mm) in a sans face: what the layout and the collision
/// nudge use, and what the plotters scale their fonts to.
pub fn text_width(text: &str, height: f64) -> f64 {
    0.6 * height * text.chars().count() as f64
}

fn finite(p: P2) -> Result<P2> {
    if p.is_finite() { Ok(p) } else { Err(DrawingError::Invalid(format!("non-finite point ({}, {})", p.x, p.y))) }
}

fn finite_len(name: &str, x: f64) -> Result<f64> {
    if x.is_finite() && x >= 0.0 { Ok(x) } else { Err(DrawingError::Invalid(format!("`{name}` must be a finite length ≥ 0 (got {x})"))) }
}

fn finite_angle(x: f64) -> Result<f64> {
    if x.is_finite() { Ok(x) } else { Err(DrawingError::Invalid("non-finite angle".into())) }
}

/// Most points in one polyline or hatch loop (a hostile sheet cannot make the plotters loop for long).
pub const MAX_POINTS: usize = 1_000_000;

/// Draw one edge, in sheet coordinates (`map` takes view points to the sheet, `scale` lengths).
pub fn plot_edge(plotter: &mut dyn Plotter, edge: &StyledEdge, map: &dyn Fn(P2) -> P2, scale: f64) -> Result<()> {
    plotter.set_layer(edge.layer);
    plotter.set_line_style(edge.style);
    plotter.set_width(edge.layer.width());
    match &edge.edge {
        Edge::Line { a, b } => plotter.line(finite(map(*a))?, finite(map(*b))?),
        Edge::Arc { center, radius, start, end } => {
            plotter.arc(finite(map(*center))?, finite_len("radius", *radius)? * scale, finite_angle(*start)?, finite_angle(*end)?)
        }
        Edge::Circle { center, radius } => plotter.circle(finite(map(*center))?, finite_len("radius", *radius)? * scale),
        Edge::Ellipse { center, rx, ry, rotation, start, end } => plotter.ellipse(
            finite(map(*center))?,
            finite_len("rx", *rx)? * scale,
            finite_len("ry", *ry)? * scale,
            finite_angle(*rotation)?,
            finite_angle(*start)?,
            finite_angle(*end)?,
        ),
        Edge::Polyline { points, closed } => {
            if points.len() > MAX_POINTS {
                return Err(DrawingError::Invalid("polyline too long".into()));
            }
            let pts = points.iter().map(|p| finite(map(*p))).collect::<Result<Vec<_>>>()?;
            plotter.polyline(&pts, *closed);
        }
    }
    Ok(())
}

/// Arrowhead length and half width (mm on the sheet).
pub const ARROW_L: f64 = 3.0;
pub const ARROW_W: f64 = 0.6;

/// A filled arrowhead with its tip at `tip`, pointing along `dir` (a unit vector).
fn arrow(plotter: &mut dyn Plotter, tip: P2, dir: P2) {
    let back = tip.sub(dir.mul(ARROW_L));
    let n = dir.perp().mul(ARROW_W);
    plotter.fill(&[tip, back.add(n), back.sub(n)]);
}

/// A text angle turned so the text reads left to right or bottom to top.
pub fn upright(angle: f64) -> f64 {
    let mut a = angle.rem_euclid(std::f64::consts::TAU);
    if a > std::f64::consts::FRAC_PI_2 + 1e-9 && a <= 3.0 * std::f64::consts::FRAC_PI_2 + 1e-9 {
        a -= std::f64::consts::PI;
    }
    if a > std::f64::consts::PI {
        a -= std::f64::consts::TAU;
    }
    a
}

/// Draw a dimension (exploded: extension lines, the dimension line, arrowheads and the text), in
/// sheet coordinates.
pub fn plot_dimension(plotter: &mut dyn Plotter, dim: &Dimension, map: &dyn Fn(P2) -> P2, scale: f64) -> Result<()> {
    plotter.set_layer(Layer::Dimension);
    plotter.set_line_style(LineStyle::Continuous);
    plotter.set_width(Layer::Dimension.width());
    let text_at = match dim.text_at {
        Some(p) => Some(finite(map(p))?),
        None => None,
    };
    match &dim.kind {
        DimKind::Linear { a, b, line_at, dir } => {
            let (a, b, l) = (finite(map(*a))?, finite(map(*b))?, finite(map(*line_at))?);
            let u = finite(*dir)?.unit().ok_or_else(|| DrawingError::Invalid("linear dimension without a direction".into()))?;
            let n = u.perp();
            let a2 = a.add(n.mul(l.sub(a).x * n.x + l.sub(a).y * n.y));
            let b2 = b.add(n.mul(l.sub(b).x * n.x + l.sub(b).y * n.y));
            // Extension lines: a 1 mm gap at the object, 2 mm past the dimension line.
            for (p, q) in [(a, a2), (b, b2)] {
                if let Some(e) = q.sub(p).unit() {
                    if p.dist(q) > 1.0 {
                        plotter.line(p.add(e.mul(1.0)), q.add(e.mul(2.0)));
                    }
                } else {
                    // The point is on the dimension line already.
                }
            }
            let span = a2.dist(b2);
            let Some(along) = b2.sub(a2).unit() else {
                return Err(DrawingError::Invalid("linear dimension between coincident points".into()));
            };
            if span >= 2.5 * ARROW_L {
                plotter.line(a2, b2);
                arrow(plotter, a2, along.mul(-1.0));
                arrow(plotter, b2, along);
            } else {
                // Too short for the arrows inside: they point in from outside.
                plotter.line(a2.sub(along.mul(ARROW_L * 2.0)), b2.add(along.mul(ARROW_L * 2.0)));
                arrow(plotter, a2, along);
                arrow(plotter, b2, along.mul(-1.0));
            }
            let ang = upright(along.y.atan2(along.x));
            let up = P2::new(-ang.sin(), ang.cos());
            let at = text_at.unwrap_or_else(|| a2.add(b2).mul(0.5).add(up.mul(1.0)));
            plotter.set_layer(Layer::Text);
            plotter.text(at, TEXT_H, ang, Align::Center, &dim.text);
        }
        DimKind::Diameter { center, radius, text_at: t } | DimKind::Radius { center, radius, text_at: t } => {
            let (c, t) = (finite(map(*center))?, text_at.unwrap_or(finite(map(*t))?));
            let r = finite_len("radius", *radius)? * scale;
            let d = t.sub(c).unit().unwrap_or(P2::new(1.0, 0.0));
            let on = c.add(d.mul(r));
            if matches!(dim.kind, DimKind::Diameter { .. }) {
                let opp = c.sub(d.mul(r));
                plotter.line(opp, if t.dist(c) > r { t } else { on });
                arrow(plotter, on, d);
                arrow(plotter, opp, d.mul(-1.0));
            } else {
                plotter.line(c, if t.dist(c) > r { t } else { on });
                arrow(plotter, on, d);
            }
            plotter.set_layer(Layer::Text);
            let align = if d.x >= 0.0 { Align::Left } else { Align::Right };
            plotter.text(t.add(P2::new(if d.x >= 0.0 { 1.0 } else { -1.0 }, 0.5)), TEXT_H, 0.0, align, &dim.text);
        }
        DimKind::Angular { vertex, a, b, arc_radius } => {
            let v = finite(map(*vertex))?;
            let (a, b) = (finite_angle(*a)?, finite_angle(*b)?);
            let r = finite_len("arc_radius", *arc_radius)? * scale;
            plotter.arc(v, r, a, b);
            let mut sweep = b - a;
            while sweep < 0.0 {
                sweep += std::f64::consts::TAU;
            }
            if r * sweep > 2.5 * ARROW_L {
                arrow(plotter, angle_pt(v, r, a), P2::new(a.sin(), -a.cos()));
                arrow(plotter, angle_pt(v, r, b), P2::new(-b.sin(), b.cos()));
            }
            let m = a + sweep / 2.0;
            let at = text_at.unwrap_or_else(|| angle_pt(v, r + TEXT_H * 0.8, m));
            plotter.set_layer(Layer::Text);
            let align = if m.cos() > 0.3 {
                Align::Left
            } else if m.cos() < -0.3 {
                Align::Right
            } else {
                Align::Center
            };
            plotter.text(at, TEXT_H, 0.0, align, &dim.text);
        }
    }
    Ok(())
}

/// Draw a label: its text (boxed or ringed for marks and balloons) and its leader.
pub fn plot_label(plotter: &mut dyn Plotter, label: &Label, map: &dyn Fn(P2) -> P2) -> Result<()> {
    let at = finite(map(label.at))?;
    let h = if label.height.is_finite() && label.height > 0.0 { label.height.min(50.0) } else { TEXT_H };
    let w = text_width(&label.text, h);
    let (hw, hh) = match label.style {
        LabelStyle::Note => (w / 2.0 + 0.5, h / 2.0 + 0.5),
        LabelStyle::Mark => (w / 2.0 + 1.0, h / 2.0 + 1.0),
        LabelStyle::Balloon => {
            let r = (w / 2.0).max(h / 2.0) + 1.2;
            (r, r)
        }
    };
    plotter.set_line_style(LineStyle::Continuous);
    if let Some(l) = label.leader {
        let l = finite(map(l))?;
        // The leader stops at the box (or circle) around the text.
        let d = at.sub(l);
        let k = if label.style == LabelStyle::Balloon {
            (1.0 - hw / d.len().max(1e-9)).max(0.0)
        } else {
            let tx = if d.x.abs() > 1e-9 { (d.x.abs() - hw) / d.x.abs() } else { 1.0 };
            let ty = if d.y.abs() > 1e-9 { (d.y.abs() - hh) / d.y.abs() } else { 1.0 };
            tx.min(ty).max(0.0)
        };
        let end = l.add(d.mul(k));
        plotter.set_layer(Layer::Dimension);
        plotter.set_width(Layer::Dimension.width());
        if l.dist(end) > 0.5 {
            plotter.line(l, end);
            if let Some(u) = l.sub(end).unit() {
                arrow(plotter, l, u);
            }
        }
    }
    plotter.set_layer(Layer::Text);
    plotter.set_width(Layer::Text.width());
    match label.style {
        LabelStyle::Note => {}
        LabelStyle::Mark => plotter.polyline(
            &[P2::new(at.x - hw, at.y - hh), P2::new(at.x + hw, at.y - hh), P2::new(at.x + hw, at.y + hh), P2::new(at.x - hw, at.y + hh)],
            true,
        ),
        LabelStyle::Balloon => plotter.circle(at, hw),
    }
    let ang = finite_angle(label.angle)?;
    let base = at.add(P2::new(ang.sin(), -ang.cos()).mul(h * 0.35));
    plotter.text(base, h, ang, Align::Center, &label.text);
    Ok(())
}

/// Hatch lines for `region` (whose loops are mapped to the sheet), drawn on `HATCH`.
pub fn plot_hatch(plotter: &mut dyn Plotter, region: &HatchRegion, map: &dyn Fn(P2) -> P2) -> Result<()> {
    let mut loops = Vec::with_capacity(region.loops.len());
    let mut total = 0usize;
    for l in &region.loops {
        total += l.len();
        if total > MAX_POINTS {
            return Err(DrawingError::Invalid("hatch region too large".into()));
        }
        loops.push(l.iter().map(|p| finite(map(*p))).collect::<Result<Vec<_>>>()?);
    }
    let spacing = if region.spacing.is_finite() && region.spacing > 0.1 { region.spacing } else { 2.5 };
    plotter.set_layer(Layer::Hatch);
    plotter.set_line_style(LineStyle::Continuous);
    plotter.set_width(Layer::Hatch.width());
    for (a, b) in hatch::lines(&loops, finite_angle(region.angle)?, spacing)? {
        plotter.line(a, b);
    }
    Ok(())
}

/// The sheet bounds of a view's edges, `None` when it has none.
pub fn view_sheet_bounds(view: &View) -> Option<(P2, P2)> {
    let (lo, hi) = view.edge_bounds()?;
    Some((view.to_sheet(lo), view.to_sheet(hi)))
}

pub fn plot_view(plotter: &mut dyn Plotter, view: &View) -> Result<()> {
    if !(view.scale.is_finite() && view.scale > 0.0) || !view.origin.is_finite() {
        return Err(DrawingError::Invalid(format!("view `{}`: bad scale or origin", view.name)));
    }
    let map = |p: P2| view.to_sheet(p);
    for h in &view.hatches {
        plot_hatch(plotter, h, &map)?;
    }
    for e in &view.edges {
        plot_edge(plotter, e, &map, view.scale)?;
    }
    for d in &view.dimensions {
        plot_dimension(plotter, d, &map, view.scale)?;
    }
    for l in &view.labels {
        plot_label(plotter, l, &map)?;
    }
    if !view.title.is_empty()
        && let Some((lo, hi)) = bounds(
            view.edges
                .iter()
                .flat_map(|e| e.edge.sample(16))
                .chain(view.labels.iter().map(|l| l.at))
                .chain(view.dimensions.iter().filter_map(|d| match &d.kind {
                    DimKind::Linear { line_at, .. } => Some(*line_at),
                    _ => None,
                }))
                .map(map),
        )
    {
        plotter.set_layer(Layer::Text);
        plotter.set_line_style(LineStyle::Continuous);
        plotter.set_width(Layer::Text.width());
        let cx = (lo.x + hi.x) / 2.0;
        plotter.text(P2::new(cx, lo.y - 9.0), TITLE_H, 0.0, Align::Center, &view.title);
        let w = text_width(&view.title, TITLE_H);
        plotter.line(P2::new(cx - w / 2.0, lo.y - 10.0), P2::new(cx + w / 2.0, lo.y - 10.0));
        plotter.text(P2::new(cx, lo.y - 15.0), TEXT_H, 0.0, Align::Center, &format!("SCALE {}", scale_text(view.scale)));
    }
    Ok(())
}

/// `1:25` for 0.04, `2:1` for 2.
pub fn scale_text(scale: f64) -> String {
    if !(scale.is_finite() && scale > 0.0) {
        return "-".into();
    }
    if scale >= 1.0 { format!("{}:1", crate::fmt::num(scale)) } else { format!("1:{}", crate::fmt::num(1.0 / scale)) }
}

/// The text of a table cell cut to fit `width` (mm) at `height`.
pub fn fit_text(text: &str, width: f64, height: f64) -> String {
    let max = ((width - 2.0) / (0.6 * height)).floor().max(1.0) as usize;
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max.saturating_sub(2)).collect();
        s.push_str("..");
        s
    }
}

/// Text height in tables (mm).
pub const TABLE_TEXT_H: f64 = 2.5;

pub fn plot_table(plotter: &mut dyn Plotter, t: &Table) -> Result<()> {
    if t.rows.len() > 10_000 || t.columns.len() > 256 || !t.at.is_finite() {
        return Err(DrawingError::Invalid(format!("table `{}` is too large or misplaced", t.title)));
    }
    let rh = t.row_h();
    let w = t.width();
    let top = t.at.y - TABLE_TITLE_H;
    let bottom = top - rh * (1 + t.rows.len()) as f64;
    plotter.set_line_style(LineStyle::Continuous);
    plotter.set_layer(Layer::Text);
    plotter.set_width(Layer::Text.width());
    plotter.text(P2::new(t.at.x, top + 2.0), TEXT_H, 0.0, Align::Left, &t.title);
    plotter.set_layer(Layer::Frame);
    plotter.set_width(Layer::Frame.width());
    plotter.polyline(&[P2::new(t.at.x, top), P2::new(t.at.x + w, top), P2::new(t.at.x + w, bottom), P2::new(t.at.x, bottom)], true);
    plotter.set_width(Layer::Hatch.width());
    for i in 1..=t.rows.len() {
        let y = top - rh * i as f64;
        plotter.line(P2::new(t.at.x, y), P2::new(t.at.x + w, y));
    }
    let mut x = t.at.x;
    for i in 0..t.columns.len().saturating_sub(1) {
        x += t.column_w(i);
        plotter.line(P2::new(x, top), P2::new(x, bottom));
    }
    plotter.set_layer(Layer::Text);
    plotter.set_width(Layer::Text.width());
    for (r, row) in std::iter::once(&t.columns).chain(t.rows.iter()).enumerate() {
        let y = top - rh * (r + 1) as f64 + (rh - TABLE_TEXT_H) / 2.0;
        let mut x = t.at.x;
        for (i, cell) in row.iter().enumerate().take(t.columns.len().max(1)) {
            let cw = t.column_w(i);
            if !cell.is_empty() {
                plotter.text(P2::new(x + 1.0, y), TABLE_TEXT_H, 0.0, Align::Left, &fit_text(cell, cw, TABLE_TEXT_H));
            }
            x += cw;
        }
    }
    Ok(())
}

/// Where the title block sits: its top-left corner on the sheet and its size.
pub fn title_block_rect(sheet: &Sheet) -> Result<(P2, f64, f64)> {
    let tpl = template::Template::parse_or_default(&sheet.template)?;
    let (w, _) = sheet.size.mm();
    Ok((P2::new(w - FRAME_MARGIN - tpl.width, FRAME_MARGIN + tpl.height), tpl.width, tpl.height))
}

/// The whole sheet: frame, views, tables and the title block.
pub fn plot_sheet(plotter: &mut dyn Plotter, sheet: &Sheet) -> Result<()> {
    let (w, h) = sheet.size.mm();
    plotter.set_layer(Layer::Frame);
    plotter.set_line_style(LineStyle::Continuous);
    plotter.set_width(0.7);
    let m = FRAME_MARGIN;
    plotter.polyline(&[P2::new(m, m), P2::new(w - m, m), P2::new(w - m, h - m), P2::new(m, h - m)], true);
    let id = |p: P2| p;
    for e in &sheet.edges {
        plot_edge(plotter, e, &id, 1.0)?;
    }
    for v in &sheet.views {
        plot_view(plotter, v)?;
    }
    for t in &sheet.tables {
        plot_table(plotter, t)?;
    }
    for l in &sheet.labels {
        plot_label(plotter, l, &id)?;
    }
    let tpl = template::Template::parse_or_default(&sheet.template)?;
    let (corner, _, _) = title_block_rect(sheet)?;
    tpl.plot(plotter, corner, &|key| if key == "size" { Some(sheet.size.name().to_string()) } else { sheet.title_block.field(key) })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Revision, SheetSize};
    use crate::svg::SvgPlotter;

    #[test]
    fn a_rectangle_plots_as_four_lines() {
        let mut view = View::new("front", P2::new(50.0, 50.0), 0.1);
        let rect = [P2::new(0.0, 0.0), P2::new(1000.0, 0.0), P2::new(1000.0, 500.0), P2::new(0.0, 500.0)];
        for i in 0..4 {
            view.line(rect[i], rect[(i + 1) % 4], Layer::Visible);
        }
        let mut svg = SvgPlotter::new(297.0, 210.0);
        plot_view(&mut svg, &view).unwrap();
        let text = svg.finish();
        assert_eq!(text.matches("<line ").count(), 4, "{text}");
        // Scaled and placed: the far corner (1000, 500) × 0.1 + (50, 50) = (150, 100), y flipped to 110.
        assert!(text.contains("x2=\"150.000\" y2=\"110.000\""), "{text}");
        assert!(text.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"297mm\""));
        assert!(text.contains("<g id=\"OUTLINE\""), "{text}");
    }

    #[test]
    fn dimensions_explode_into_lines_arrows_and_upright_text() {
        let mut view = View::new("v", P2::new(0.0, 0.0), 1.0);
        view.dimensions.push(Dimension::linear(
            P2::new(0.0, 0.0),
            P2::new(40.0, 0.0),
            P2::new(0.0, -10.0),
            P2::new(1.0, 0.0),
            "40 <mm>",
            "E1.length_mm",
        ));
        view.dimensions.push(Dimension::linear(P2::new(0.0, 0.0), P2::new(0.0, 40.0), P2::new(-10.0, 0.0), P2::new(0.0, -1.0), "40", ""));
        let mut svg = SvgPlotter::new(100.0, 100.0);
        plot_view(&mut svg, &view).unwrap();
        let text = svg.finish();
        // Two extension lines and a dimension line each; two filled arrowheads each.
        assert_eq!(text.matches("<line ").count(), 6, "{text}");
        assert_eq!(text.matches("<polygon ").count(), 4, "{text}");
        assert!(text.contains("40 &lt;mm&gt;"));
        // The vertical one reads bottom to top (+90°, SVG's −90).
        assert!(text.contains("rotate(-90.000"), "{text}");
        assert_eq!(upright(std::f64::consts::PI), 0.0);
        assert!((upright(-std::f64::consts::FRAC_PI_2) - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn a_sheet_plots_its_frame_and_title_block_and_refuses_bad_views() {
        let mut sheet = Sheet::new(SheetSize::A3);
        sheet.title_block.title = "Drum".into();
        let mut svg = SvgPlotter::for_sheet(&sheet);
        plot_sheet(&mut svg, &sheet).unwrap();
        let text = svg.finish();
        assert!(text.contains(">Drum</text>"), "{text}");
        assert!(text.contains(">A3</text>"), "the size placeholder: {text}");
        assert!(!text.contains("{{"), "every placeholder is filled: {text}");
        sheet.views.push(View::new("bad", P2::new(f64::NAN, 0.0), 1.0));
        let mut svg = SvgPlotter::for_sheet(&sheet);
        assert!(plot_sheet(&mut svg, &sheet).is_err());
    }

    #[test]
    fn model_round_trips_through_json() {
        let mut sheet = Sheet::new(SheetSize::A4);
        sheet.views.push(View::new("front", P2::new(1.0, 2.0), 0.5));
        sheet.title_block.revisions.push(Revision { rev: "A".into(), date: "2026-10-18".into(), description: "issued".into(), by: "SP".into() });
        let text = serde_json::to_string(&sheet).unwrap();
        let back: Sheet = serde_json::from_str(&text).unwrap();
        assert_eq!(back, sheet);
    }

    #[test]
    fn scales_and_cells_read_as_drawings_write_them() {
        assert_eq!(scale_text(0.04), "1:25");
        assert_eq!(scale_text(2.0), "2:1");
        assert_eq!(fit_text("SHELL COURSE 1600 ID x 12 THK", 20.0, 2.5), "SHELL COUR..");
        assert_eq!(fit_text("E1", 20.0, 2.5), "E1");
    }
}
