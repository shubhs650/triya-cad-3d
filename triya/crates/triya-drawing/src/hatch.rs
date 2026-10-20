// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Hatching: a family of parallel lines clipped to closed loops by even-odd scanline clipping
//! (`docs/drawing-model.md` §2, section view). The loops may overlap or nest: a point is inside
//! when a ray from it crosses the loops an odd number of times, so a hollow profile (outside
//! and inside loop) hatches only its wall.

use crate::model::P2;
use crate::{DrawingError, Result};

/// Most hatch lines for one region.
pub const MAX_LINES: usize = 20_000;

/// The hatch segments for `loops` (closed, sheet mm), lines at `angle` (radians), `spacing` mm
/// apart, on a grid through the sheet origin (so neighbouring regions line up).
pub fn lines(loops: &[Vec<P2>], angle: f64, spacing: f64) -> Result<Vec<(P2, P2)>> {
    if !(spacing.is_finite() && spacing > 0.0 && angle.is_finite()) {
        return Err(DrawingError::Invalid("hatch spacing and angle must be finite".into()));
    }
    let (s, c) = angle.sin_cos();
    // Rotate by −angle: the hatch lines become horizontal (y = const).
    let rot = |p: &P2| P2::new(p.x * c + p.y * s, -p.x * s + p.y * c);
    let back = |p: P2| P2::new(p.x * c - p.y * s, p.x * s + p.y * c);
    let loops: Vec<Vec<P2>> = loops.iter().filter(|l| l.len() > 2).map(|l| l.iter().map(rot).collect()).collect();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in loops.iter().flatten() {
        lo = lo.min(p.y);
        hi = hi.max(p.y);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return Ok(Vec::new());
    }
    let first = (lo / spacing).ceil();
    let count = ((hi / spacing).floor() - first + 1.0).max(0.0);
    if count > MAX_LINES as f64 {
        return Err(DrawingError::Invalid("hatch region too large for its spacing".into()));
    }
    let mut out = Vec::new();
    let mut xs: Vec<f64> = Vec::new();
    for k in 0..count as usize {
        let y = (first + k as f64) * spacing;
        xs.clear();
        for l in &loops {
            for i in 0..l.len() {
                let (Some(p), Some(q)) = (l.get(i), l.get((i + 1) % l.len())) else { continue };
                // Half-open on y: a vertex on the line counts once.
                if (p.y <= y) != (q.y <= y) {
                    xs.push(p.x + (y - p.y) * (q.x - p.x) / (q.y - p.y));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks_exact(2) {
            if let [a, b] = pair
                && b - a > 1e-9
            {
                out.push((back(P2::new(*a, y)), back(P2::new(*b, y))));
                if out.len() > MAX_LINES {
                    return Err(DrawingError::Invalid("hatch region too large for its spacing".into()));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, s: f64) -> Vec<P2> {
        vec![P2::new(x0, y0), P2::new(x0 + s, y0), P2::new(x0 + s, y0 + s), P2::new(x0, y0 + s)]
    }

    #[test]
    fn horizontal_hatch_of_a_square_and_a_hollow_square() {
        let l = lines(&[square(0.0, 0.0, 10.0)], 0.0, 2.5).unwrap();
        // y = 0 (half-open: counts), 2.5, 5, 7.5; y = 10 is outside.
        assert_eq!(l.len(), 4, "{l:?}");
        assert!(l.iter().all(|(a, b)| (a.x - 0.0).abs() < 1e-9 && (b.x - 10.0).abs() < 1e-9));
        // A 4 × 4 hole in the middle: the line through the hole (y = 5) splits in two.
        let l = lines(&[square(0.0, 0.0, 10.0), square(3.0, 3.0, 4.0)], 0.0, 2.5).unwrap();
        assert_eq!(l.len(), 4 + 1, "{l:?}");
    }

    #[test]
    fn diagonal_lines_stay_inside_and_hostile_input_is_refused() {
        let l = lines(&[square(0.0, 0.0, 10.0)], std::f64::consts::FRAC_PI_4, 1.0).unwrap();
        assert!(!l.is_empty());
        for (a, b) in &l {
            for p in [a, b] {
                assert!(p.x > -1e-6 && p.x < 10.0 + 1e-6 && p.y > -1e-6 && p.y < 10.0 + 1e-6, "{p:?}");
            }
            assert!(((b.y - a.y) - (b.x - a.x)).abs() < 1e-6, "45°");
        }
        assert!(lines(&[square(0.0, 0.0, 1e9)], 0.0, 1.0).is_err());
        assert!(lines(&[square(0.0, 0.0, 1.0)], f64::NAN, 1.0).is_err());
        assert!(lines(&[], 0.0, 1.0).unwrap().is_empty());
    }
}
