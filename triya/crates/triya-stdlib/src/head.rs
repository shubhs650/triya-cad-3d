// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Formed head presets and their depth formulas (plain geometry, millimetres).
//!
//! A torispherical head is a spherical crown of radius `L` blended into the shell by a knuckle
//! (torus) of radius `r`. With both radii measured on the inside and `D` the inside diameter,
//! the depth of the dished part (from the tangent line to the inside of the crown) is
//!
//! ```text
//! h = L − sqrt((L − r)² − (D/2 − r)²)
//! ```
//!
//! The presets give `L` and `r` as ratios of a reference diameter: the inside diameter (ASME
//! F&D: `L = D`, `r = 0.06 D`) or the outside diameter (DIN 28011 Klöpper: `R = Da`,
//! `r = 0.1 Da`; DIN 28013 Korbbogen: `R = 0.8 Da`, `r = 0.154 Da`), see [`RatioBasis`].

use serde::{Deserialize, Serialize};

use crate::{Result, StdlibError, positive};

/// Which diameter a preset's ratios multiply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RatioBasis {
    /// The inside diameter (ASME practice).
    #[default]
    Inside,
    /// The outside diameter `Da = Di + 2 t` (DIN practice).
    Outside,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HeadPreset {
    /// Crown radius `crown_ratio × D_ref`, knuckle radius `knuckle_ratio × D_ref`.
    Torispherical {
        crown_ratio: f64,
        knuckle_ratio: f64,
        #[serde(default)]
        basis: RatioBasis,
    },
    /// Semi-ellipsoidal: inside depth `D / (2 × ratio)` (`ratio = 2` is the common 2:1 head).
    Ellipsoidal {
        ratio: f64,
    },
    Hemispherical,
    /// Cone with a knuckle at the large end (depth needs the cone length, which the element
    /// carries, so the preset alone has no depth).
    Toriconical {
        half_apex_angle_deg: f64,
        knuckle_ratio: f64,
    },
}

impl HeadPreset {
    /// ASME flanged and dished: `L = D`, `r = 0.06 D` (inside basis).
    pub const ASME_FD: HeadPreset = HeadPreset::Torispherical { crown_ratio: 1.0, knuckle_ratio: 0.06, basis: RatioBasis::Inside };
    /// DIN 28011 Klöpper: `R = Da`, `r = 0.1 Da`.
    pub const KLOPPER: HeadPreset = HeadPreset::Torispherical { crown_ratio: 1.0, knuckle_ratio: 0.1, basis: RatioBasis::Outside };
    /// DIN 28013 Korbbogen: `R = 0.8 Da`, `r = 0.154 Da`.
    pub const KORBBOGEN: HeadPreset = HeadPreset::Torispherical { crown_ratio: 0.8, knuckle_ratio: 0.154, basis: RatioBasis::Outside };
    /// 2:1 semi-ellipsoidal.
    pub const ELLIPSOIDAL_2_1: HeadPreset = HeadPreset::Ellipsoidal { ratio: 2.0 };

    /// Inside depth of the dished part (tangent line to the inside of the crown), without the
    /// straight flange. `d_inside` and `t` in millimetres.
    pub fn dish_depth(&self, d_inside: f64, t: f64) -> Result<f64> {
        let d = positive("d_inside", d_inside)?;
        let t = crate::non_negative("t", t)?;
        match *self {
            HeadPreset::Torispherical { crown_ratio, knuckle_ratio, basis } => {
                let d_ref = match basis {
                    RatioBasis::Inside => d,
                    RatioBasis::Outside => d + 2.0 * t,
                };
                let l = positive("crown radius", crown_ratio * d_ref)?;
                let r = positive("knuckle radius", knuckle_ratio * d_ref)?;
                torispherical_depth(d, l, r)
            }
            HeadPreset::Ellipsoidal { ratio } => {
                let ratio = positive("ratio", ratio)?;
                Ok(d / (2.0 * ratio))
            }
            HeadPreset::Hemispherical => Ok(d / 2.0),
            HeadPreset::Toriconical { .. } => {
                Err(StdlibError::NotSupportedYet("toriconical head depth (needs the cone length from the element)".into()))
            }
        }
    }

    /// Overall inside depth: the straight flange `sf` plus the dished depth.
    pub fn depth(&self, d_inside: f64, sf: f64, t: f64) -> Result<f64> {
        let sf = crate::non_negative("sf", sf)?;
        Ok(sf + self.dish_depth(d_inside, t)?)
    }
}

/// `h = L − sqrt((L − r)² − (D/2 − r)²)`, refused when the knuckle cannot meet the crown.
pub fn torispherical_depth(d: f64, l: f64, r: f64) -> Result<f64> {
    let d = positive("D", d)?;
    let l = positive("L", l)?;
    let r = positive("r", r)?;
    if r >= d / 2.0 {
        return Err(StdlibError::Invalid(format!("knuckle radius {r} must be less than D/2 = {}", d / 2.0)));
    }
    if l <= r {
        return Err(StdlibError::Invalid(format!("crown radius {l} must exceed the knuckle radius {r}")));
    }
    let a = l - r;
    let b = d / 2.0 - r;
    let under = a * a - b * b;
    if !(under >= 0.0) {
        return Err(StdlibError::Invalid(format!("crown radius {l} is too small to reach the knuckle of D = {d}, r = {r}")));
    }
    let h = l - under.sqrt();
    if h.is_finite() && h > 0.0 { Ok(h) } else { Err(StdlibError::Invalid(format!("degenerate head depth {h}"))) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// DIN 28011 (Klöpper) publishes `h2 ≈ 0.1935 Da − 0.455 s`. Da = 1000, s = 10: 188.95.
    /// The exact geometry (R = Da, r = 0.1 Da on the inside, Di = Da − 2 s) gives 188.89.
    #[test]
    fn klopper_din_28011_depth_matches_the_published_approximation() {
        let (da, s) = (1000.0, 10.0);
        let di = da - 2.0 * s;
        let h2 = HeadPreset::KLOPPER.dish_depth(di, s).unwrap();
        let published = 0.1935 * da - 0.455 * s;
        assert!(close(published, 188.95, 1e-9));
        assert!(close(h2, published, 0.5), "h2 = {h2}, published ≈ {published}");
        assert!(close(h2, 188.889, 0.01), "exact h2 = {h2}");
    }

    /// DIN 28013 (Korbbogen) publishes `h2 ≈ 0.255 Da − 0.635 s`. Da = 1000, s = 10: 248.65.
    /// The exact geometry (R = 0.8 Da, r = 0.154 Da on the inside) gives 248.26.
    #[test]
    fn korbbogen_din_28013_depth_matches_the_published_approximation() {
        let (da, s) = (1000.0, 10.0);
        let di = da - 2.0 * s;
        let h2 = HeadPreset::KORBBOGEN.dish_depth(di, s).unwrap();
        let published = 0.255 * da - 0.635 * s;
        assert!(close(published, 248.65, 1e-9));
        assert!(close(h2, published, 0.5), "h2 = {h2}, published ≈ {published}");
        assert!(close(h2, 248.26, 0.01), "exact h2 = {h2}");
    }

    /// ASME F&D with L = D and r = 0.06 D: h = D (1 − sqrt(0.94² − 0.44²)) = 0.169338 D.
    #[test]
    fn asme_fd_depth_is_0_1693_d() {
        for d in [100.0, 1000.0, 2438.0] {
            let h = HeadPreset::ASME_FD.dish_depth(d, 12.0).unwrap();
            assert!(close(h / d, 0.1693, 5e-5), "D = {d}: h/D = {}", h / d);
            assert!(close(h / d, 0.169338, 1e-6), "D = {d}: h/D = {}", h / d);
        }
    }

    #[test]
    fn ellipsoidal_2_1_is_a_quarter_of_the_diameter_and_hemispherical_a_half() {
        assert!(close(HeadPreset::ELLIPSOIDAL_2_1.dish_depth(1000.0, 10.0).unwrap(), 250.0, 1e-9));
        assert!(close(HeadPreset::Hemispherical.dish_depth(1000.0, 10.0).unwrap(), 500.0, 1e-9));
        assert!(close(HeadPreset::ELLIPSOIDAL_2_1.depth(1000.0, 40.0, 10.0).unwrap(), 290.0, 1e-9));
    }

    #[test]
    fn hostile_inputs_are_errors_not_panics() {
        for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
            assert!(HeadPreset::ASME_FD.dish_depth(bad, 10.0).is_err());
            assert!(HeadPreset::KLOPPER.depth(1000.0, bad.min(-1.0), 10.0).is_err());
        }
        // A crown too flat to reach the knuckle.
        assert!(torispherical_depth(1000.0, 300.0, 60.0).is_err());
        // A knuckle wider than the head.
        assert!(torispherical_depth(100.0, 100.0, 60.0).is_err());
        assert!(HeadPreset::Toriconical { half_apex_angle_deg: 30.0, knuckle_ratio: 0.06 }.dish_depth(1000.0, 10.0).is_err());
    }

    #[test]
    fn presets_round_trip_through_json() {
        let json = serde_json::to_string(&HeadPreset::KLOPPER).unwrap();
        assert!(json.contains("\"type\":\"torispherical\"") && json.contains("\"basis\":\"outside\""));
        let back: HeadPreset = serde_json::from_str(&json).unwrap();
        assert_eq!(back, HeadPreset::KLOPPER);
        let e: HeadPreset = serde_json::from_str(r#"{"type":"ellipsoidal","ratio":2}"#).unwrap();
        assert_eq!(e, HeadPreset::ELLIPSOIDAL_2_1);
    }
}
