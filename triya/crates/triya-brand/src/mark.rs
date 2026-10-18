// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The Triya CAD 3D mark, drawn in code: a vertical pressure vessel (shell, two 2:1 ellipsoidal
//! heads with their tangent lines, one flanged nozzle) standing on a skirt, on a slate tile.
//!
//! Original artwork; nothing is traced from another product. The web app's PNG icons
//! (`triya/apps/triya-web/icons/`) are exports of [`rgba`], kept current by
//! `triya-ui`'s `icons_match_the_code_drawn_mark` test, and the desktop window icon is drawn
//! from it at startup. Shapes live in a unit square (x right, y down) and are rasterised with
//! supersampled coverage, so every size comes from the same geometry.

/// Tile colour (the web manifest's `theme_color`).
pub const SLATE: [u8; 4] = [0x34, 0x38, 0x40, 0xff];
/// Vessel colour.
pub const STEEL_BLUE: [u8; 4] = [0x4a, 0xa3, 0xdf, 0xff];
/// Support (skirt and base ring) colour.
pub const LIGHT: [u8; 4] = [0xc9, 0xce, 0xd6, 0xff];

/// Largest raster [`rgba`] draws (pixels per side).
pub const MAX_SIZE: u32 = 2048;

/// Which tile the mark sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// Rounded tile with transparent corners (favicons, desktop icons, `purpose: "any"`).
    Rounded,
    /// Full-bleed tile with the drawing inside the central 80 % circle (`purpose: "maskable"`).
    Maskable,
}

#[derive(Clone, Copy)]
enum Shape {
    /// Axis-aligned rectangle `x0..x1 × y0..y1` with corner radius `r`.
    Rect { x0: f64, y0: f64, x1: f64, y1: f64, r: f64 },
    /// Ellipse centred at `(cx, cy)` with semi-axes `rx`, `ry`.
    Ellipse { cx: f64, cy: f64, rx: f64, ry: f64 },
}

impl Shape {
    fn contains(self, x: f64, y: f64) -> bool {
        match self {
            Shape::Rect { x0, y0, x1, y1, r } => {
                if x < x0 || x > x1 || y < y0 || y > y1 {
                    return false;
                }
                // Inside the rectangle; outside a corner's quarter circle only in that corner's square.
                let cx = x.clamp(x0 + r, x1 - r);
                let cy = y.clamp(y0 + r, y1 - r);
                (x - cx).powi(2) + (y - cy).powi(2) <= r * r
            }
            Shape::Ellipse { cx, cy, rx, ry } => ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0,
        }
    }

    /// The same shape scaled by `k` about the tile centre.
    fn scaled(self, k: f64) -> Shape {
        let s = |v: f64| 0.5 + (v - 0.5) * k;
        match self {
            Shape::Rect { x0, y0, x1, y1, r } => Shape::Rect { x0: s(x0), y0: s(y0), x1: s(x1), y1: s(y1), r: r * k },
            Shape::Ellipse { cx, cy, rx, ry } => Shape::Ellipse { cx: s(cx), cy: s(cy), rx: rx * k, ry: ry * k },
        }
    }
}

// Vessel proportions: shell half-width W, 2:1 heads (depth W/2), tangent lines at TOP_TL/BOT_TL.
const CX: f64 = 0.47;
const W: f64 = 0.16;
const TOP_TL: f64 = 0.31;
const BOT_TL: f64 = 0.66;
const SEAM: f64 = 0.012;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Shape {
    Shape::Rect { x0, y0, x1, y1, r: 0.0 }
}

/// The drawing on top of the tile, back to front.
fn drawing() -> [(Shape, [u8; 4]); 9] {
    let head = |cy| Shape::Ellipse { cx: CX, cy, rx: W, ry: W / 2.0 };
    [
        // Skirt and base ring behind the bottom head.
        (rect(CX - 0.12, BOT_TL, CX + 0.12, 0.83), LIGHT),
        (rect(CX - 0.17, 0.815, CX + 0.17, 0.85), LIGHT),
        // Nozzle neck and flange on the right of the upper shell course.
        (rect(CX + W - 0.01, 0.395, CX + 0.30, 0.445), STEEL_BLUE),
        (Shape::Rect { x0: CX + 0.285, y0: 0.355, x1: CX + 0.33, y1: 0.485, r: 0.008 }, STEEL_BLUE),
        // Shell and heads.
        (rect(CX - W, TOP_TL, CX + W, BOT_TL), STEEL_BLUE),
        (head(TOP_TL), STEEL_BLUE),
        (head(BOT_TL), STEEL_BLUE),
        // Tangent lines.
        (rect(CX - W, TOP_TL - SEAM / 2.0, CX + W, TOP_TL + SEAM / 2.0), SLATE),
        (rect(CX - W, BOT_TL - SEAM / 2.0, CX + W, BOT_TL + SEAM / 2.0), SLATE),
    ]
}

fn over(dst: [f64; 4], src: [u8; 4], coverage: f64) -> [f64; 4] {
    let a = coverage * f64::from(src[3]) / 255.0;
    let out_a = a + dst[3] * (1.0 - a);
    if out_a <= 0.0 {
        return [0.0; 4];
    }
    let mut out = [0.0, 0.0, 0.0, out_a];
    for (i, o) in out.iter_mut().take(3).enumerate() {
        let s = src.get(i).copied().map_or(0.0, f64::from);
        let d = dst.get(i).copied().unwrap_or(0.0);
        *o = (s * a + d * dst[3] * (1.0 - a)) / out_a;
    }
    out
}

/// The mark as straight-alpha RGBA8, `size × size` pixels, rows top to bottom; `None` when
/// `size` is 0 or above [`MAX_SIZE`].
pub fn rgba(size: u32, variant: Variant) -> Option<Vec<u8>> {
    if size == 0 || size > MAX_SIZE {
        return None;
    }
    let tile = match variant {
        Variant::Rounded => Shape::Rect { x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0, r: 0.2 },
        Variant::Maskable => rect(0.0, 0.0, 1.0, 1.0),
    };
    let k = match variant {
        Variant::Rounded => 1.0,
        Variant::Maskable => 0.8,
    };
    let mut layers = vec![(tile, SLATE)];
    layers.extend(drawing().iter().map(|&(s, c)| (s.scaled(k), c)));
    // Small icons need more samples per pixel for clean edges.
    let n: u32 = if size <= 64 { 8 } else { 4 };
    let samples = f64::from(n * n);
    let side = f64::from(size);
    let mut out = Vec::with_capacity((size as usize).saturating_mul(size as usize).saturating_mul(4));
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f64; 4];
            for (shape, colour) in &layers {
                let mut hits = 0u32;
                for sy in 0..n {
                    for sx in 0..n {
                        let x = (f64::from(px) + (f64::from(sx) + 0.5) / f64::from(n)) / side;
                        let y = (f64::from(py) + (f64::from(sy) + 0.5) / f64::from(n)) / side;
                        if shape.contains(x, y) {
                            hits += 1;
                        }
                    }
                }
                if hits > 0 {
                    acc = over(acc, *colour, f64::from(hits) / samples);
                }
            }
            for v in [acc[0], acc[1], acc[2]] {
                out.push(v.round().clamp(0.0, 255.0) as u8);
            }
            out.push((acc[3] * 255.0).round().clamp(0.0, 255.0) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn sizes_outside_the_range_draw_nothing() {
        assert!(rgba(0, Variant::Rounded).is_none());
        assert!(rgba(MAX_SIZE + 1, Variant::Maskable).is_none());
        assert_eq!(rgba(16, Variant::Rounded).map(|v| v.len()), Some(16 * 16 * 4));
    }

    #[test]
    fn rounded_tile_has_transparent_corners_and_maskable_is_full_bleed() {
        let r = rgba(64, Variant::Rounded).unwrap_or_default();
        let m = rgba(64, Variant::Maskable).unwrap_or_default();
        assert_eq!(px(&r, 64, 0, 0)[3], 0);
        assert_eq!(px(&m, 64, 0, 0), SLATE);
        assert_eq!(px(&r, 64, 32, 2), SLATE);
    }

    #[test]
    fn the_vessel_is_drawn_at_the_shell_centre() {
        let img = rgba(500, Variant::Rounded).unwrap_or_default();
        assert_eq!(px(&img, 500, 235, 250), STEEL_BLUE);
        // The lower tangent line (y = 0.66) crosses the shell in the tile colour.
        assert_eq!(px(&img, 500, 235, 329), SLATE);
    }
}
