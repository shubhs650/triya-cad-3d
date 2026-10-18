// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Plotters: a pen-style sink a sheet is drawn into. [`SvgPlotter`] writes SVG text; a DXF or
//! PDF plotter, or an egui painter in `triya-ui`, implements the same trait.

use crate::model::{Dimension, Edge, Layer, LineStyle, P2, Sheet, StyledEdge, View};
use crate::{DrawingError, Result};

pub trait Plotter {
    fn set_layer(&mut self, layer: Layer);
    fn set_line_style(&mut self, style: LineStyle);
    /// Line width in sheet mm.
    fn set_width(&mut self, width: f64);
    fn move_to(&mut self, p: P2);
    fn line_to(&mut self, p: P2);
    /// Counter-clockwise from `start` to `end` (radians).
    fn arc(&mut self, center: P2, radius: f64, start: f64, end: f64);
    fn circle(&mut self, center: P2, radius: f64);
    /// `height` is the text height in mm; `angle` in radians.
    fn text(&mut self, at: P2, height: f64, angle: f64, text: &str);
}

fn finite(p: P2) -> Result<P2> {
    if p.is_finite() { Ok(p) } else { Err(DrawingError::Invalid(format!("non-finite point ({}, {})", p.x, p.y))) }
}

fn finite_len(name: &str, x: f64) -> Result<f64> {
    if x.is_finite() && x >= 0.0 { Ok(x) } else { Err(DrawingError::Invalid(format!("`{name}` must be a finite length ≥ 0 (got {x})"))) }
}

/// Draw one edge, in sheet coordinates.
pub fn plot_edge(plotter: &mut dyn Plotter, edge: &StyledEdge, map: &dyn Fn(P2) -> P2, scale: f64) -> Result<()> {
    plotter.set_layer(edge.layer);
    plotter.set_line_style(edge.style);
    match &edge.edge {
        Edge::Line { a, b } => {
            plotter.move_to(finite(map(*a))?);
            plotter.line_to(finite(map(*b))?);
        }
        Edge::Arc { center, radius, start, end } => {
            if !(start.is_finite() && end.is_finite()) {
                return Err(DrawingError::Invalid("non-finite arc angle".into()));
            }
            plotter.arc(finite(map(*center))?, finite_len("radius", *radius)? * scale, *start, *end);
        }
        Edge::Circle { center, radius } => plotter.circle(finite(map(*center))?, finite_len("radius", *radius)? * scale),
        Edge::Ellipse { center, rx, ry, rotation } => {
            // Approximated by a 64-segment polyline (plotters have no ellipse primitive).
            let c = finite(map(*center))?;
            let (rx, ry) = (finite_len("rx", *rx)? * scale, finite_len("ry", *ry)? * scale);
            if !rotation.is_finite() {
                return Err(DrawingError::Invalid("non-finite ellipse rotation".into()));
            }
            let (sr, cr) = rotation.sin_cos();
            for i in 0..=64u32 {
                let t = f64::from(i) / 64.0 * std::f64::consts::TAU;
                let (x, y) = (rx * t.cos(), ry * t.sin());
                let p = P2::new(c.x + x * cr - y * sr, c.y + x * sr + y * cr);
                if i == 0 { plotter.move_to(p) } else { plotter.line_to(p) }
            }
        }
        Edge::Polyline { points, closed } => {
            if points.len() > 1_000_000 {
                return Err(DrawingError::Invalid("polyline too long".into()));
            }
            for (i, p) in points.iter().enumerate() {
                let p = finite(map(*p))?;
                if i == 0 { plotter.move_to(p) } else { plotter.line_to(p) }
            }
            if *closed && let Some(first) = points.first() {
                plotter.line_to(finite(map(*first))?);
            }
        }
    }
    Ok(())
}

/// Draw a dimension (its lines and text) in sheet coordinates.
pub fn plot_dimension(plotter: &mut dyn Plotter, dim: &Dimension, map: &dyn Fn(P2) -> P2, scale: f64) -> Result<()> {
    plotter.set_layer(Layer::Dimension);
    plotter.set_line_style(LineStyle::Continuous);
    const TEXT_H: f64 = 3.5;
    match dim {
        Dimension::Linear { a, b, line_at, text } => {
            let (a, b, l) = (finite(map(*a))?, finite(map(*b))?, finite(map(*line_at))?);
            // The dimension line is parallel to a→b, through `line_at`.
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let len = (dx * dx + dy * dy).sqrt();
            if !(len > 1e-9) {
                return Err(DrawingError::Invalid("linear dimension between coincident points".into()));
            }
            let (ux, uy) = (dx / len, dy / len);
            let (nx, ny) = (-uy, ux);
            let off = (l.x - a.x) * nx + (l.y - a.y) * ny;
            let a2 = P2::new(a.x + nx * off, a.y + ny * off);
            let b2 = P2::new(b.x + nx * off, b.y + ny * off);
            for (from, to) in [(a, a2), (b, b2), (a2, b2)] {
                plotter.move_to(from);
                plotter.line_to(to);
            }
            let mid = P2::new((a2.x + b2.x) / 2.0 + nx * TEXT_H * 0.4, (a2.y + b2.y) / 2.0 + ny * TEXT_H * 0.4);
            plotter.text(mid, TEXT_H, uy.atan2(ux), text);
        }
        Dimension::Diameter { center, radius, text_at, text } | Dimension::Radius { center, radius, text_at, text } => {
            let (c, t) = (finite(map(*center))?, finite(map(*text_at))?);
            let r = finite_len("radius", *radius)? * scale;
            let (dx, dy) = (t.x - c.x, t.y - c.y);
            let len = (dx * dx + dy * dy).sqrt();
            let on = if len > 1e-9 { P2::new(c.x + dx / len * r, c.y + dy / len * r) } else { P2::new(c.x + r, c.y) };
            let from = if matches!(dim, Dimension::Diameter { .. }) { P2::new(2.0 * c.x - on.x, 2.0 * c.y - on.y) } else { c };
            plotter.move_to(from);
            plotter.line_to(t);
            plotter.text(t, TEXT_H, 0.0, text);
        }
        Dimension::Angular { vertex, a, b, arc_radius, text } => {
            let v = finite(map(*vertex))?;
            if !(a.is_finite() && b.is_finite()) {
                return Err(DrawingError::Invalid("non-finite angle".into()));
            }
            let r = finite_len("arc_radius", *arc_radius)? * scale;
            plotter.arc(v, r, *a, *b);
            let m = (a + b) / 2.0;
            plotter.text(P2::new(v.x + (r + TEXT_H) * m.cos(), v.y + (r + TEXT_H) * m.sin()), TEXT_H, 0.0, text);
        }
    }
    Ok(())
}

pub fn plot_view(plotter: &mut dyn Plotter, view: &View) -> Result<()> {
    if !(view.scale.is_finite() && view.scale > 0.0) || !view.origin.is_finite() {
        return Err(DrawingError::Invalid(format!("view `{}`: bad scale or origin", view.name)));
    }
    let map = |p: P2| view.to_sheet(p);
    plotter.set_width(0.35);
    for e in &view.edges {
        plot_edge(plotter, e, &map, view.scale)?;
    }
    plotter.set_width(0.25);
    for d in &view.dimensions {
        plot_dimension(plotter, d, &map, view.scale)?;
    }
    Ok(())
}

/// The whole sheet: frame, views, tables and the title block.
pub fn plot_sheet(plotter: &mut dyn Plotter, sheet: &Sheet) -> Result<()> {
    let (w, h) = sheet.size.mm();
    plotter.set_layer(Layer::Frame);
    plotter.set_line_style(LineStyle::Continuous);
    plotter.set_width(0.7);
    let frame = [P2::new(10.0, 10.0), P2::new(w - 10.0, 10.0), P2::new(w - 10.0, h - 10.0), P2::new(10.0, h - 10.0)];
    plotter.move_to(frame[0]);
    for p in frame.iter().skip(1).chain(frame.first()) {
        plotter.line_to(*p);
    }
    let id = |p: P2| p;
    plotter.set_width(0.35);
    for e in &sheet.edges {
        plot_edge(plotter, e, &id, 1.0)?;
    }
    for v in &sheet.views {
        plot_view(plotter, v)?;
    }
    plotter.set_layer(Layer::Text);
    plotter.set_width(0.25);
    for t in &sheet.tables {
        if t.rows.len() > 10_000 || t.columns.len() > 256 {
            return Err(DrawingError::Invalid(format!("table `{}` is too large", t.title)));
        }
        let rh = if t.row_height.is_finite() && t.row_height > 0.0 { t.row_height } else { 8.0 };
        let mut y = t.at.y;
        plotter.text(P2::new(t.at.x, y + 1.0), 3.5, 0.0, &t.title);
        for row in std::iter::once(&t.columns).chain(t.rows.iter()) {
            y -= rh;
            let mut x = t.at.x;
            for (i, cell) in row.iter().enumerate() {
                let cw = t.column_widths.get(i).copied().filter(|w| w.is_finite() && *w > 0.0).unwrap_or(30.0);
                plotter.text(P2::new(x + 1.0, y + 2.0), 2.5, 0.0, cell);
                x += cw;
            }
        }
    }
    let tb = &sheet.title_block;
    let (x0, y0) = (w - 10.0 - 180.0, 10.0);
    plotter.set_layer(Layer::Frame);
    for (i, line) in [
        format!("{} {}", tb.title, tb.drawing_number),
        format!("{} · {}", tb.project, tb.organization),
        format!("Drawn {} · Checked {} · {}", tb.drawn_by, tb.checked_by, tb.date),
        format!("Scale {} · Sheet {}", tb.scale, tb.sheet_of),
    ]
    .iter()
    .enumerate()
    {
        plotter.text(P2::new(x0 + 2.0, y0 + 30.0 - 7.0 * i as f64), 2.5, 0.0, line);
    }
    Ok(())
}

/// An SVG document; `finish()` gives the text. The sheet's y axis points up, SVG's down: the
/// plotter flips y about the sheet height.
pub struct SvgPlotter {
    width: f64,
    height: f64,
    body: String,
    layer: Layer,
    style: LineStyle,
    stroke_w: f64,
    pen: Option<P2>,
}

impl SvgPlotter {
    pub fn new(width_mm: f64, height_mm: f64) -> SvgPlotter {
        SvgPlotter {
            width: width_mm.max(1.0),
            height: height_mm.max(1.0),
            body: String::new(),
            layer: Layer::Visible,
            style: LineStyle::Continuous,
            stroke_w: 0.35,
            pen: None,
        }
    }

    pub fn for_sheet(sheet: &Sheet) -> SvgPlotter {
        let (w, h) = sheet.size.mm();
        SvgPlotter::new(w, h)
    }

    fn y(&self, y: f64) -> f64 {
        self.height - y
    }

    fn colour(&self) -> &'static str {
        match self.layer {
            Layer::Visible | Layer::Frame => "#000000",
            Layer::Hidden => "#606060",
            Layer::Center => "#b00000",
            Layer::Dimension => "#0040c0",
            Layer::Hatch => "#008000",
            Layer::Text => "#000000",
        }
    }

    fn dash(&self) -> &'static str {
        match self.style {
            LineStyle::Continuous => "",
            LineStyle::Dashed => " stroke-dasharray=\"3 1.5\"",
            LineStyle::Center => " stroke-dasharray=\"12 2 2 2\"",
            LineStyle::Hidden => " stroke-dasharray=\"6 2\"",
            LineStyle::Phantom => " stroke-dasharray=\"12 2 2 2 2 2\"",
        }
    }

    fn stroke(&self) -> String {
        format!("stroke=\"{}\" stroke-width=\"{:.3}\" fill=\"none\"{}", self.colour(), self.stroke_w, self.dash())
    }

    /// The SVG text.
    pub fn finish(self) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n{body}</svg>\n",
            w = self.width,
            h = self.height,
            body = self.body
        )
    }
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

impl Plotter for SvgPlotter {
    fn set_layer(&mut self, layer: Layer) {
        self.layer = layer;
    }
    fn set_line_style(&mut self, style: LineStyle) {
        self.style = style;
    }
    fn set_width(&mut self, width: f64) {
        if width.is_finite() && width > 0.0 {
            self.stroke_w = width;
        }
    }
    fn move_to(&mut self, p: P2) {
        self.pen = Some(p);
    }
    fn line_to(&mut self, p: P2) {
        if let Some(from) = self.pen {
            let s = self.stroke();
            self.body.push_str(&format!(
                "<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" {s}/>\n",
                from.x,
                self.y(from.y),
                p.x,
                self.y(p.y)
            ));
        }
        self.pen = Some(p);
    }
    fn arc(&mut self, center: P2, radius: f64, start: f64, end: f64) {
        let mut sweep = end - start;
        while sweep < 0.0 {
            sweep += std::f64::consts::TAU;
        }
        if sweep >= std::f64::consts::TAU - 1e-9 {
            return self.circle(center, radius);
        }
        let a = P2::new(center.x + radius * start.cos(), center.y + radius * start.sin());
        let b = P2::new(center.x + radius * end.cos(), center.y + radius * end.sin());
        let large = u8::from(sweep > std::f64::consts::PI);
        // Counter-clockwise on the sheet is clockwise in SVG's flipped y: sweep flag 0.
        let s = self.stroke();
        self.body.push_str(&format!(
            "<path d=\"M {:.3} {:.3} A {r:.3} {r:.3} 0 {large} 0 {:.3} {:.3}\" {s}/>\n",
            a.x,
            self.y(a.y),
            b.x,
            self.y(b.y),
            r = radius
        ));
        self.pen = Some(b);
    }
    fn circle(&mut self, center: P2, radius: f64) {
        let s = self.stroke();
        self.body.push_str(&format!("<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" {s}/>\n", center.x, self.y(center.y), radius));
    }
    fn text(&mut self, at: P2, height: f64, angle: f64, text: &str) {
        let deg = -angle.to_degrees();
        self.body.push_str(&format!(
            "<text x=\"{x:.3}\" y=\"{y:.3}\" font-size=\"{height:.3}\" font-family=\"sans-serif\" fill=\"{c}\" transform=\"rotate({deg:.3} {x:.3} {y:.3})\">{t}</text>\n",
            x = at.x,
            y = self.y(at.y),
            c = self.colour(),
            t = esc(text)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Revision, SheetSize};

    #[test]
    fn a_rectangle_plots_as_four_lines() {
        let mut view = View::new("front", P2::new(50.0, 50.0), 0.1);
        let rect = [P2::new(0.0, 0.0), P2::new(1000.0, 0.0), P2::new(1000.0, 500.0), P2::new(0.0, 500.0)];
        for i in 0..4 {
            view.edges.push(StyledEdge {
                edge: Edge::Line { a: rect[i], b: rect[(i + 1) % 4] },
                layer: Layer::Visible,
                style: LineStyle::Continuous,
            });
        }
        let mut svg = SvgPlotter::new(297.0, 210.0);
        plot_view(&mut svg, &view).unwrap();
        let text = svg.finish();
        assert_eq!(text.matches("<line ").count(), 4, "{text}");
        // Scaled and placed: the far corner (1000, 500) × 0.1 + (50, 50) = (150, 100), y flipped to 110.
        assert!(text.contains("x2=\"150.000\" y2=\"110.000\""), "{text}");
        assert!(text.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"297mm\""));
    }

    #[test]
    fn a_polyline_rectangle_also_gives_four_lines_and_arcs_circles_and_text_are_written() {
        let mut view = View::new("v", P2::new(0.0, 0.0), 1.0);
        view.edges.push(StyledEdge {
            edge: Edge::Polyline { points: vec![P2::new(0.0, 0.0), P2::new(10.0, 0.0), P2::new(10.0, 5.0), P2::new(0.0, 5.0)], closed: true },
            layer: Layer::Hidden,
            style: LineStyle::Hidden,
        });
        view.edges.push(StyledEdge { edge: Edge::Circle { center: P2::new(5.0, 2.5), radius: 1.0 }, layer: Layer::Center, style: LineStyle::Center });
        view.edges.push(StyledEdge {
            edge: Edge::Arc { center: P2::new(5.0, 2.5), radius: 2.0, start: 0.0, end: 1.0 },
            layer: Layer::Visible,
            style: LineStyle::Continuous,
        });
        view.dimensions.push(Dimension::Linear { a: P2::new(0.0, 0.0), b: P2::new(10.0, 0.0), line_at: P2::new(0.0, -5.0), text: "10 <mm>".into() });
        let mut svg = SvgPlotter::new(100.0, 100.0);
        plot_view(&mut svg, &view).unwrap();
        let text = svg.finish();
        assert_eq!(text.matches("<line ").count(), 4 + 3, "{text}");
        assert_eq!(text.matches("<circle ").count(), 1);
        assert_eq!(text.matches("<path ").count(), 1);
        assert!(text.contains("stroke-dasharray=\"6 2\""));
        assert!(text.contains("10 &lt;mm&gt;"));
    }

    #[test]
    fn a_sheet_plots_its_frame_and_title_block_and_refuses_bad_views() {
        let mut sheet = Sheet::new(SheetSize::A3);
        sheet.title_block.title = "Drum".into();
        let mut svg = SvgPlotter::for_sheet(&sheet);
        plot_sheet(&mut svg, &sheet).unwrap();
        let text = svg.finish();
        assert_eq!(text.matches("<line ").count(), 4, "frame only");
        assert!(text.contains("Drum"));
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
}
