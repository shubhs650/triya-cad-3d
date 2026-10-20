// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Orthographic projection of 3D wireframes into a view: circles stay exact (a circle, an
//! ellipse or a line, as the view sees them), cylinders and cones get their silhouette lines,
//! and an optional occluder (the vessel's shell, seen from the side) splits edges into visible
//! and hidden runs.

use triya_vessel::nozzle::{V3, add, cross, dot, mul, norm, sub};

use crate::model::{Edge, Layer, P2, StyledEdge, View};

/// A view's frame in model space: `right` and `up` span the sheet, `toward` points from the
/// viewer into the scene (`right × up = −toward`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Basis {
    pub right: V3,
    pub up: V3,
    pub toward: V3,
}

pub fn unit(a: V3) -> Option<V3> {
    let n = norm(a);
    (n > 1e-12 && n.is_finite()).then(|| mul(a, 1.0 / n))
}

impl Basis {
    pub fn new(right: V3, up: V3) -> Basis {
        Basis { right, up, toward: mul(cross(right, up), -1.0) }
    }
    pub fn p(&self, v: V3) -> P2 {
        P2::new(dot(v, self.right), dot(v, self.up))
    }
    /// A direction in the sheet's plane (not normalised).
    pub fn d(&self, v: V3) -> P2 {
        self.p(v)
    }
    /// How far `v` lies into the scene.
    pub fn depth(&self, v: V3) -> f64 {
        dot(v, self.toward)
    }
}

/// Says whether a model point is hidden behind something.
pub trait Occluder {
    fn hidden(&self, p: V3) -> bool;
}

/// Nothing hides anything.
pub struct NoOcclusion;

impl Occluder for NoOcclusion {
    fn hidden(&self, _: V3) -> bool {
        false
    }
}

/// Points on a circle: centre `c`, unit normal `n`, radius `r`, from angle `a0` over `sweep`
/// (radians, about `n` from the reference direction `e1`).
fn circle_pts(c: V3, n: V3, r: f64, e1: V3, a0: f64, sweep: f64, segs: u32) -> Vec<V3> {
    let e2 = cross(n, e1);
    (0..=segs)
        .map(|i| {
            let t = a0 + sweep * f64::from(i) / f64::from(segs);
            add(c, add(mul(e1, r * t.cos()), mul(e2, r * t.sin())))
        })
        .collect()
}

/// Any unit vector perpendicular to the unit `n`.
pub fn any_perp(n: V3) -> V3 {
    let k = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    unit(cross(n, k)).unwrap_or([0.0, 0.0, 1.0])
}

/// Collects edges for one view.
pub struct Wire<'a> {
    pub basis: Basis,
    pub occluder: &'a dyn Occluder,
    /// Draw hidden runs (dashed, on `HIDDEN`), or leave them out.
    pub show_hidden: bool,
    pub edges: Vec<StyledEdge>,
}

impl<'a> Wire<'a> {
    pub fn new(basis: Basis, occluder: &'a dyn Occluder, show_hidden: bool) -> Wire<'a> {
        Wire { basis, occluder, show_hidden, edges: Vec::new() }
    }

    fn push(&mut self, edge: Edge, hidden: bool, layer: Layer) {
        if hidden {
            if self.show_hidden {
                self.edges.push(StyledEdge::new(edge, Layer::Hidden));
            }
        } else {
            self.edges.push(StyledEdge::new(edge, layer));
        }
    }

    /// A 3D polyline, split into visible and hidden runs.
    pub fn polyline(&mut self, pts: &[V3], layer: Layer) {
        if pts.len() < 2 {
            return;
        }
        let mut run: Vec<P2> = Vec::new();
        let mut run_hidden = false;
        for (i, p) in pts.iter().enumerate() {
            let h = self.occluder.hidden(*p);
            let q = self.basis.p(*p);
            if i == 0 {
                run.push(q);
                run_hidden = h;
                continue;
            }
            if h != run_hidden {
                // Split at the midpoint of this step.
                let Some(prev) = pts.get(i - 1) else { continue };
                let mid = self.basis.p(mul(add(*prev, *p), 0.5));
                run.push(mid);
                self.flush(&mut run, run_hidden, layer);
                run.push(mid);
                run_hidden = h;
            }
            run.push(q);
        }
        self.flush(&mut run, run_hidden, layer);
    }

    fn flush(&mut self, run: &mut Vec<P2>, hidden: bool, layer: Layer) {
        let pts = std::mem::take(run);
        match pts.as_slice() {
            [] | [_] => {}
            [a, b] => self.push(Edge::Line { a: *a, b: *b }, hidden, layer),
            _ => self.push(Edge::Polyline { points: pts, closed: false }, hidden, layer),
        }
    }

    /// A straight 3D segment (sampled so the occluder can split it).
    pub fn line(&mut self, a: V3, b: V3, layer: Layer) {
        let n = 24u32;
        let pts: Vec<V3> = (0..=n).map(|i| add(a, mul(sub(b, a), f64::from(i) / f64::from(n)))).collect();
        if pts.iter().all(|p| !self.occluder.hidden(*p)) {
            self.push(Edge::Line { a: self.basis.p(a), b: self.basis.p(b) }, false, layer);
        } else if pts.iter().all(|p| self.occluder.hidden(*p)) {
            self.push(Edge::Line { a: self.basis.p(a), b: self.basis.p(b) }, true, layer);
        } else {
            self.polyline(&pts, layer);
        }
    }

    /// A whole circle: exact (circle, ellipse or line) unless the occluder splits it.
    pub fn circle(&mut self, c: V3, n: V3, r: f64, layer: Layer) {
        let Some(n) = unit(n) else { return };
        if !(r.is_finite() && r > 0.0) {
            return;
        }
        let pts = circle_pts(c, n, r, any_perp(n), 0.0, std::f64::consts::TAU, 48);
        let hidden: Vec<bool> = pts.iter().map(|p| self.occluder.hidden(*p)).collect();
        if hidden.iter().any(|h| *h) && !hidden.iter().all(|h| *h) {
            return self.polyline(&pts, layer);
        }
        let all_hidden = hidden.first().copied().unwrap_or(false);
        let k = dot(n, self.basis.toward);
        let c2 = self.basis.p(c);
        let edge = if k.abs() > 1.0 - 1e-9 {
            Edge::Circle { center: c2, radius: r }
        } else {
            let Some(m) = unit(cross(n, self.basis.toward)) else { return };
            let m2 = self.basis.d(m);
            if k.abs() < 1e-9 {
                Edge::Line { a: c2.sub(m2.mul(r)), b: c2.add(m2.mul(r)) }
            } else {
                Edge::Ellipse { center: c2, rx: r, ry: r * k.abs(), rotation: m2.y.atan2(m2.x), start: 0.0, end: std::f64::consts::TAU }
            }
        };
        self.push(edge, all_hidden, layer);
    }

    /// A partial circle about `n` through the unit `e1` (sampled).
    pub fn arc(&mut self, c: V3, n: V3, r: f64, e1: V3, a0: f64, sweep: f64, layer: Layer) {
        if !(r.is_finite() && r > 0.0 && a0.is_finite() && sweep.is_finite()) {
            return;
        }
        let segs = ((sweep.abs() / std::f64::consts::TAU * 64.0).ceil() as u32).clamp(4, 64);
        let pts = circle_pts(c, n, r, e1, a0, sweep, segs);
        self.polyline(&pts, layer);
    }

    /// A frustum (a cylinder when `r0 == r1`) from `base` along the unit `axis` over `len`: its
    /// end circles and the two silhouette lines.
    pub fn frustum(&mut self, base: V3, axis: V3, len: f64, r0: f64, r1: f64, layer: Layer) {
        let Some(axis) = unit(axis) else { return };
        if !(len.is_finite() && len.abs() > 1e-9) {
            return;
        }
        let top = add(base, mul(axis, len));
        self.circle(base, axis, r0, layer);
        self.circle(top, axis, r1, layer);
        if let Some(w) = unit(cross(axis, self.basis.toward)) {
            for s in [1.0, -1.0] {
                self.line(add(base, mul(w, s * r0)), add(top, mul(w, s * r1)), layer);
            }
        }
    }

    pub fn into_view(self, view: &mut View) {
        view.edges.extend(self.edges);
    }
}

/// Edges that coincide (the same line either way round, within 0.01 mm) are drawn once; a
/// visible edge wins over a hidden one.
pub fn dedupe(edges: &mut Vec<StyledEdge>) {
    let key = |e: &StyledEdge| -> Option<[i64; 4]> {
        if let Edge::Line { a, b } = e.edge {
            let q = |x: f64| (x * 100.0).round() as i64;
            let (p, r) = if (a.x, a.y) <= (b.x, b.y) { (a, b) } else { (b, a) };
            Some([q(p.x), q(p.y), q(r.x), q(r.y)])
        } else {
            None
        }
    };
    let mut seen: std::collections::BTreeMap<[i64; 4], usize> = std::collections::BTreeMap::new();
    let mut out: Vec<StyledEdge> = Vec::with_capacity(edges.len());
    for e in edges.drain(..) {
        match key(&e) {
            Some(k) => match seen.get(&k) {
                Some(&i) => {
                    if let Some(prev) = out.get_mut(i)
                        && prev.layer == Layer::Hidden
                        && e.layer != Layer::Hidden
                    {
                        *prev = e;
                    }
                }
                None => {
                    seen.insert(k, out.len());
                    out.push(e);
                }
            },
            None => out.push(e),
        }
    }
    *edges = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front() -> Basis {
        Basis::new([1.0, 0.0, 0.0], [0.0, 0.0, 1.0])
    }

    #[test]
    fn the_front_basis_looks_north_and_circles_project_exactly() {
        let b = front();
        assert_eq!(b.toward, [0.0, 1.0, 0.0]);
        let mut w = Wire::new(b, &NoOcclusion, true);
        // Facing the viewer: a circle. Edge-on: a line. At 60°: an ellipse with ry = r cos 60°.
        w.circle([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 10.0, Layer::Visible);
        w.circle([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 10.0, Layer::Visible);
        w.circle([0.0, 0.0, 0.0], [0.0, 0.5, 3f64.sqrt() / 2.0], 10.0, Layer::Visible);
        assert!(matches!(w.edges[0].edge, Edge::Circle { radius, .. } if radius == 10.0));
        assert!(matches!(w.edges[1].edge, Edge::Line { a, b } if (a.x + b.x).abs() < 1e-9 && (a.x.abs() - 10.0).abs() < 1e-9 && a.y.abs() < 1e-9));
        assert!(matches!(w.edges[2].edge, Edge::Ellipse { rx, ry, .. } if rx == 10.0 && (ry - 5.0).abs() < 1e-9));
    }

    struct Behind;
    impl Occluder for Behind {
        fn hidden(&self, p: V3) -> bool {
            p[1] > 0.0
        }
    }

    #[test]
    fn the_occluder_splits_runs_and_hidden_runs_can_be_left_out() {
        let mut w = Wire::new(front(), &Behind, true);
        w.line([0.0, -10.0, 0.0], [0.0, 10.0, 5.0], Layer::Visible);
        assert_eq!(w.edges.iter().filter(|e| e.layer == Layer::Hidden).count(), 1);
        assert_eq!(w.edges.iter().filter(|e| e.layer == Layer::Visible).count(), 1);
        let mut w = Wire::new(front(), &Behind, false);
        w.frustum([0.0, 5.0, 0.0], [0.0, 0.0, 1.0], 10.0, 2.0, 2.0, Layer::Visible);
        assert!(w.edges.is_empty(), "all hidden: {:?}", w.edges);
        let mut e = vec![
            StyledEdge::line(P2::new(0.0, 0.0), P2::new(1.0, 1.0), Layer::Hidden),
            StyledEdge::line(P2::new(1.0, 1.0), P2::new(0.0, 0.0), Layer::Visible),
        ];
        dedupe(&mut e);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].layer, Layer::Visible);
    }
}
