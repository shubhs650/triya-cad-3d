// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The SVG plotter: one `<g id="LAYER">` per layer (the DXF layer names), dashes as
//! `stroke-dasharray`, texts as `<text>` in a sans face. The sheet's y axis points up, SVG's
//! down: the plotter flips y about the sheet height. The output is deterministic (fixed number
//! formatting, layers in a fixed order), which is what the golden-file tests compare.

use std::collections::BTreeMap;

use crate::model::{Layer, LineStyle, P2, Sheet};
use crate::plot::{Align, Plotter};

pub struct SvgPlotter {
    width: f64,
    height: f64,
    groups: BTreeMap<Layer, String>,
    layer: Layer,
    style: LineStyle,
    stroke_w: f64,
}

impl SvgPlotter {
    pub fn new(width_mm: f64, height_mm: f64) -> SvgPlotter {
        SvgPlotter {
            width: if width_mm.is_finite() { width_mm.max(1.0) } else { 1.0 },
            height: if height_mm.is_finite() { height_mm.max(1.0) } else { 1.0 },
            groups: BTreeMap::new(),
            layer: Layer::Visible,
            style: LineStyle::Continuous,
            stroke_w: 0.35,
        }
    }

    pub fn for_sheet(sheet: &Sheet) -> SvgPlotter {
        let (w, h) = sheet.size.mm();
        SvgPlotter::new(w, h)
    }

    fn y(&self, y: f64) -> f64 {
        self.height - y
    }

    fn colour(layer: Layer) -> &'static str {
        match layer {
            Layer::Visible | Layer::Frame | Layer::Text => "#000000",
            Layer::Hidden => "#404040",
            Layer::Center => "#a00000",
            Layer::Dimension => "#0038a8",
            Layer::Hatch => "#406040",
        }
    }

    fn dash(&self) -> &'static str {
        match self.style {
            LineStyle::Continuous => "",
            LineStyle::Dashed => " stroke-dasharray=\"3 1.5\"",
            LineStyle::Center => " stroke-dasharray=\"12 2 2 2\"",
            LineStyle::Hidden => " stroke-dasharray=\"2 1\"",
            LineStyle::Phantom => " stroke-dasharray=\"12 2 2 2 2 2\"",
        }
    }

    fn stroke(&self) -> String {
        format!("stroke-width=\"{:.3}\" fill=\"none\"{}", self.stroke_w, self.dash())
    }

    fn push(&mut self, s: String) {
        self.groups.entry(self.layer).or_default().push_str(&s);
    }

    /// The SVG text.
    pub fn finish(self) -> String {
        let mut body = String::new();
        for layer in Layer::ALL {
            if let Some(g) = self.groups.get(&layer) {
                body.push_str(&format!(
                    "<g id=\"{}\" stroke=\"{c}\" fill=\"{c}\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n{g}</g>\n",
                    layer.dxf_name(),
                    c = SvgPlotter::colour(layer)
                ));
            }
        }
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n<rect width=\"{w}\" height=\"{h}\" fill=\"#ffffff\"/>\n{body}</svg>\n",
            w = self.width,
            h = self.height,
        )
    }
}

/// `s` with XML's special characters escaped and control characters dropped.
pub fn esc(s: &str) -> String {
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
    fn line(&mut self, a: P2, b: P2) {
        let s = self.stroke();
        let l = format!("<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" {s}/>\n", a.x, self.y(a.y), b.x, self.y(b.y));
        self.push(l);
    }
    fn polyline(&mut self, pts: &[P2], closed: bool) {
        if pts.len() < 2 {
            return;
        }
        let s = self.stroke();
        let p: Vec<String> = pts.iter().map(|p| format!("{:.3},{:.3}", p.x, self.y(p.y))).collect();
        let tag = if closed { "polygon" } else { "polyline" };
        self.push(format!("<{tag} points=\"{}\" {s}/>\n", p.join(" ")));
    }
    fn arc(&mut self, center: P2, radius: f64, start: f64, end: f64) {
        let mut sweep = end - start;
        while sweep < 0.0 {
            sweep += std::f64::consts::TAU;
        }
        if sweep >= std::f64::consts::TAU - 1e-9 || sweep <= 1e-12 {
            return self.circle(center, radius);
        }
        let a = P2::new(center.x + radius * start.cos(), center.y + radius * start.sin());
        let b = P2::new(center.x + radius * end.cos(), center.y + radius * end.sin());
        let large = u8::from(sweep > std::f64::consts::PI);
        // Counter-clockwise on the sheet is clockwise in SVG's flipped y: sweep flag 0.
        let s = self.stroke();
        let p = format!("<path d=\"M {:.3} {:.3} A {r:.3} {r:.3} 0 {large} 0 {:.3} {:.3}\" {s}/>\n", a.x, self.y(a.y), b.x, self.y(b.y), r = radius);
        self.push(p);
    }
    fn circle(&mut self, center: P2, radius: f64) {
        let s = self.stroke();
        let c = format!("<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" {s}/>\n", center.x, self.y(center.y), radius);
        self.push(c);
    }
    fn ellipse(&mut self, center: P2, rx: f64, ry: f64, rotation: f64, start: f64, end: f64) {
        let mut sweep = end - start;
        while sweep < 0.0 {
            sweep += std::f64::consts::TAU;
        }
        let s = self.stroke();
        let deg = -rotation.to_degrees();
        if sweep >= std::f64::consts::TAU - 1e-9 {
            let e = format!(
                "<ellipse cx=\"{x:.3}\" cy=\"{y:.3}\" rx=\"{rx:.3}\" ry=\"{ry:.3}\" transform=\"rotate({deg:.3} {x:.3} {y:.3})\" {s}/>\n",
                x = center.x,
                y = self.y(center.y)
            );
            return self.push(e);
        }
        let a = crate::model::ellipse_pt(center, rx, ry, rotation, start);
        let b = crate::model::ellipse_pt(center, rx, ry, rotation, end);
        let large = u8::from(sweep > std::f64::consts::PI);
        let p = format!("<path d=\"M {:.3} {:.3} A {rx:.3} {ry:.3} {deg:.3} {large} 0 {:.3} {:.3}\" {s}/>\n", a.x, self.y(a.y), b.x, self.y(b.y));
        self.push(p);
    }
    fn fill(&mut self, pts: &[P2]) {
        let p: Vec<String> = pts.iter().map(|p| format!("{:.3},{:.3}", p.x, self.y(p.y))).collect();
        self.push(format!("<polygon points=\"{}\" stroke-width=\"0.1\"/>\n", p.join(" ")));
    }
    fn text(&mut self, at: P2, height: f64, angle: f64, align: Align, text: &str) {
        let deg = -angle.to_degrees();
        let anchor = match align {
            Align::Left => "start",
            Align::Center => "middle",
            Align::Right => "end",
        };
        let t = format!(
            "<text x=\"{x:.3}\" y=\"{y:.3}\" font-size=\"{height:.3}\" font-family=\"Helvetica, Arial, sans-serif\" stroke=\"none\" text-anchor=\"{anchor}\" transform=\"rotate({deg:.3} {x:.3} {y:.3})\">{t}</text>\n",
            x = at.x,
            y = self.y(at.y),
            t = esc(text)
        );
        self.push(t);
    }
}
