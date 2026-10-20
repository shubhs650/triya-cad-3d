// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The DXF plotter (T6.6): upstream's public writer `solidcraft_io::write_dxf_drawing` (AutoCAD
//! 2000, exact arcs, circles and ellipses, dashed layers, TEXT) has every entity a beta sheet
//! needs, so no DXF crate is added. Layers `OUTLINE`, `HIDDEN` (hidden linetype), `CENTER`
//! (dashed: the writer has no centre linetype), `DIM`, `HATCH`, `TEXT`, `TITLE`; dimensions are
//! exploded into lines, arrowheads (closed polylines) and TEXT; hatching is LINEs on `HATCH`.

use solidcraft_geom::Vec2;
use solidcraft_io::{DxfEntity, DxfLayer, DxfLinetype, DxfShape, write_dxf_drawing};

use crate::model::{Layer, LineStyle, P2};
use crate::plot::{Align, Plotter, text_width};

/// Most entities in one drawing (a hostile sheet cannot make the file unbounded).
pub const MAX_ENTITIES: usize = 2_000_000;

#[derive(Default)]
pub struct DxfPlotter {
    layer: Layer,
    entities: Vec<DxfEntity>,
}

fn v(p: P2) -> Vec2 {
    Vec2::new(p.x, p.y)
}

/// The layer table.
pub fn layers() -> Vec<DxfLayer> {
    Layer::ALL
        .iter()
        .map(|l| {
            let colour = match l {
                Layer::Visible | Layer::Text | Layer::Frame => 7,
                Layer::Hidden => 8,
                Layer::Center => 1,
                Layer::Dimension => 5,
                Layer::Hatch => 3,
            };
            let d = DxfLayer::new(l.dxf_name(), colour);
            match l.style() {
                LineStyle::Hidden => DxfLayer { linetype: DxfLinetype::Hidden, ..d },
                LineStyle::Center | LineStyle::Dashed | LineStyle::Phantom => d.dashed(),
                LineStyle::Continuous => d,
            }
        })
        .collect()
}

impl DxfPlotter {
    pub fn new() -> DxfPlotter {
        DxfPlotter::default()
    }

    fn push(&mut self, shape: DxfShape) {
        if self.entities.len() < MAX_ENTITIES {
            self.entities.push(DxfEntity { layer: self.layer.dxf_name().to_string(), shape });
        }
    }

    /// The DXF text.
    pub fn finish(self) -> String {
        write_dxf_drawing(&layers(), &self.entities)
    }
}

impl Plotter for DxfPlotter {
    fn set_layer(&mut self, layer: Layer) {
        self.layer = layer;
    }
    fn set_line_style(&mut self, _: LineStyle) {}
    fn set_width(&mut self, _: f64) {}
    fn line(&mut self, a: P2, b: P2) {
        self.push(DxfShape::Line(v(a), v(b)));
    }
    fn polyline(&mut self, pts: &[P2], closed: bool) {
        let mut points: Vec<Vec2> = pts.iter().map(|p| v(*p)).collect();
        if closed && let Some(f) = points.first().copied() {
            points.push(f);
        }
        if points.len() > 1 {
            self.push(DxfShape::Polyline { points });
        }
    }
    fn arc(&mut self, center: P2, radius: f64, start: f64, end: f64) {
        let sweep = (end - start).rem_euclid(std::f64::consts::TAU);
        if sweep < 1e-12 {
            return self.circle(center, radius);
        }
        self.push(DxfShape::Arc { c: v(center), r: radius, a0: start.to_degrees().rem_euclid(360.0), a1: end.to_degrees().rem_euclid(360.0) });
    }
    fn circle(&mut self, center: P2, radius: f64) {
        self.push(DxfShape::Circle { c: v(center), r: radius });
    }
    fn ellipse(&mut self, center: P2, rx: f64, ry: f64, rotation: f64, start: f64, end: f64) {
        if !(rx > 0.0 && ry > 0.0) {
            return;
        }
        let full = (end - start).rem_euclid(std::f64::consts::TAU) < 1e-12 || (end - start).abs() >= std::f64::consts::TAU - 1e-9;
        // DXF wants the major axis: when ry is the longer one, turn the frame by 90°.
        let (major, ratio, shift) = if rx >= ry {
            (Vec2::new(rx * rotation.cos(), rx * rotation.sin()), ry / rx, 0.0)
        } else {
            let r = rotation + std::f64::consts::FRAC_PI_2;
            (Vec2::new(ry * r.cos(), ry * r.sin()), rx / ry, -std::f64::consts::FRAC_PI_2)
        };
        let (t0, t1) = if full {
            (0.0, std::f64::consts::TAU)
        } else {
            ((start + shift).rem_euclid(std::f64::consts::TAU), (end + shift).rem_euclid(std::f64::consts::TAU))
        };
        self.push(DxfShape::Ellipse { c: v(center), major, ratio, t0, t1 });
    }
    fn fill(&mut self, pts: &[P2]) {
        self.polyline(pts, true);
    }
    fn text(&mut self, at: P2, height: f64, angle: f64, align: Align, text: &str) {
        // The writer centres a text on its anchor.
        let w = text_width(text, height);
        let shift = match align {
            Align::Left => w / 2.0,
            Align::Center => 0.0,
            Align::Right => -w / 2.0,
        };
        let c = P2::new(at.x + shift * angle.cos(), at.y + shift * angle.sin());
        let text = text.replace('°', "%%d");
        self.push(DxfShape::Text { at: v(c), height, angle_deg: angle.to_degrees(), text });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layers_and_entities_are_written() {
        let mut p = DxfPlotter::new();
        p.set_layer(Layer::Visible);
        p.line(P2::new(0.0, 0.0), P2::new(10.0, 0.0));
        p.arc(P2::new(0.0, 0.0), 5.0, 0.0, 1.0);
        p.ellipse(P2::new(0.0, 0.0), 2.0, 4.0, 0.0, 0.0, std::f64::consts::TAU);
        p.set_layer(Layer::Text);
        p.text(P2::new(0.0, 0.0), 3.5, 0.0, Align::Left, "90°");
        let s = p.finish();
        for name in ["OUTLINE", "HIDDEN", "CENTER", "DIM", "HATCH", "TEXT", "TITLE"] {
            assert!(s.contains(&format!("\n{name}\n")), "{name}");
        }
        assert!(s.contains("\nLINE\n") && s.contains("\nARC\n") && s.contains("\nELLIPSE\n") && s.contains("\nTEXT\n"));
        assert!(s.contains("90%%d"));
    }
}
