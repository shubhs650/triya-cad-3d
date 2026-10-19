// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Bill of materials and weights (T4.5, `vessel.bom`).
//!
//! Every item's weight comes from the volume `inspect.measure` reports for its body, times the
//! density of its material; a handbook value is computed beside it from the dimensions alone as
//! a cross-check. Shells and heads: area × thickness on the mean diameter `D` (cylinder π·D·L;
//! skirt also its base ring; cone π·(D₁ + D₂)/2 · slant; 2:1 ellipsoidal 1.084·D²; hemisphere π/2·D²; ASME F&D 0.9286·D²;
//! Klöpper 0.99·D²; other torispherical heads from their crown and knuckle; a head's straight
//! flange adds π·D·h₁). Nozzle necks and flanges come from their table rows; supports from their
//! closed forms. A nozzle joined into its shell is measured as part of the shell, so the shell's
//! item is the shell's body less the nozzle's computed volume and the nozzle's item is that
//! computed volume. Totals: empty = every `Vessel …` body; test = empty + the vessel full of
//! water (placeholder until hydrotest data is modelled); operating = `design_data` when given,
//! else empty (the document has no contents).

use std::f64::consts::PI;

use serde_json::{Value, json};

use crate::eval::{HeadShape, Profile, Resolved, Station};
use crate::model::{Element, ElementKind, HeadPresetId};
use crate::nozzle::NozzleFlangeGeom;
use triya_stdlib::FlangeKind;

/// Density (kg/m³) of a material named in the vessel document: stainless and nickel-alloy
/// grades 8000, aluminium 2700, everything else carbon steel 7850.
pub fn density(material: &str) -> f64 {
    let m = material.to_ascii_lowercase();
    let has = |k: &[&str]| k.iter().any(|w| m.contains(w));
    if has(&["stainless", "ss3", "ss 3", "304", "316", "321", "347", "310", "a240", "a312", "a182 f3", "duplex", "2205", "inconel", "alloy 6"]) {
        8000.0
    } else if has(&["alumin", "b209", "b221", "5083", "6061"]) {
        2700.0
    } else {
        7850.0
    }
}

/// Weight (kg) of `volume_mm3` at `density` (kg/m³).
pub fn kg(volume_mm3: f64, density: f64) -> f64 {
    volume_mm3 * 1e-9 * density
}

/// The handbook metal volume of a shell element (mm³) and the rule used, from its resolved
/// station; `None` for elements without a simple rule (body flanges).
pub fn handbook_volume(e: &Element, st: &Station) -> Option<(f64, &'static str)> {
    let t = st.thickness;
    match (&e.kind, st.profile) {
        (ElementKind::Skirt { base_ring: b, .. }, _) => {
            let d = 2.0 * st.r_from + t;
            let ring = PI / 4.0 * (b.od_mm * b.od_mm - b.id_mm * b.id_mm) * b.thickness_mm;
            Some((PI * d * (st.length() - b.thickness_mm) * t + ring, "π·D·L·t + base ring π/4·(OD²−ID²)·t"))
        }
        (ElementKind::Cylinder { .. }, _) => {
            let d = 2.0 * st.r_from + t;
            Some((PI * d * st.length() * t, "π·D·L·t"))
        }
        (ElementKind::Cone { .. }, _) => {
            let (d1, d2) = (2.0 * st.r_from + t, 2.0 * st.r_to + t);
            let slant = st.length().hypot((d1 - d2) / 2.0);
            Some((PI * (d1 + d2) / 2.0 * slant * t, "π·(D₁+D₂)/2·s·t"))
        }
        (ElementKind::FlatCover { .. }, _) => {
            let d = 2.0 * st.r_from.max(st.r_to);
            Some((PI / 4.0 * d * d * t, "π/4·D²·t"))
        }
        (ElementKind::Head { preset, .. }, Profile::Head { sf, dish, shape, .. }) => {
            let r_in = st.r_from.max(st.r_to);
            let d = 2.0 * r_in + t;
            let flange = PI * d * sf;
            let (area, rule) = match (preset, shape) {
                (HeadPresetId::Ellipsoidal21, _) => (1.084 * d * d, "1.084·D² + π·D·h₁"),
                (HeadPresetId::Hemispherical, _) => (PI / 2.0 * d * d, "π/2·D² + π·D·h₁"),
                (HeadPresetId::AsmeFd, _) => (0.9286 * d * d, "0.9286·D² + π·D·h₁"),
                (HeadPresetId::KloepperDin28011, _) => (0.99 * d * d, "0.99·D² + π·D·h₁"),
                (_, HeadShape::Torispherical { crown_r, knuckle_r }) => {
                    (torispherical_area(r_in + t / 2.0, crown_r + t / 2.0, knuckle_r + t / 2.0)?, "crown and knuckle zones + π·D·h₁")
                }
                (_, HeadShape::Ellipsoidal) => (ellipsoid_cap_area(r_in + t / 2.0, dish + t / 2.0)?, "half ellipsoid + π·D·h₁"),
            };
            Some(((area + flange) * t, rule))
        }
        _ => None,
    }
}

/// Surface of a torispherical dish of base radius `r`, crown radius `l`, knuckle radius `k`.
fn torispherical_area(r: f64, l: f64, k: f64) -> Option<f64> {
    let s = (r - k) / (l - k);
    if !(s.is_finite() && (0.0..=1.0).contains(&s)) {
        return None;
    }
    let phi = s.asin();
    let cap = 2.0 * PI * l * l * (1.0 - phi.cos());
    let knuckle = 2.0 * PI * k * ((r - k) * (PI / 2.0 - phi) + k * phi.cos());
    Some(cap + knuckle)
}

/// Surface of half an ellipsoid of revolution, equatorial radius `a`, depth `b`.
fn ellipsoid_cap_area(a: f64, b: f64) -> Option<f64> {
    if !(a > 0.0 && b > 0.0) {
        return None;
    }
    if (a - b).abs() < 1e-9 * a {
        return Some(2.0 * PI * a * a);
    }
    if b < a {
        let e = (1.0 - b * b / (a * a)).sqrt();
        Some(PI * a * a + PI * b * b / e * ((1.0 + e) / (1.0 - e)).ln() / 2.0)
    } else {
        let e = (1.0 - a * a / (b * b)).sqrt();
        Some(PI * a * a + PI * a * b / e * e.asin())
    }
}

/// The volume (mm³) inside the pressure envelope, by Simpson's rule over each station.
pub fn inside_volume(r: &Resolved) -> f64 {
    const N: usize = 400;
    r.stations
        .iter()
        .filter(|s| s.envelope && s.length() > 0.0)
        .map(|s| {
            let h = s.length() / N as f64;
            let f = |i: usize| {
                let z = s.z_from + h * i as f64;
                let rr = s.radius_at(z).unwrap_or(0.0);
                PI * rr * rr
            };
            let mut sum = f(0) + f(N);
            for i in 1..N {
                sum += if i % 2 == 1 { 4.0 } else { 2.0 } * f(i);
            }
            sum * h / 3.0
        })
        .sum()
}

/// A nozzle flange's metal from its table row (ring less bolt holes, raised face, hub), bored
/// to the neck's bore (weld neck), its own bore (slip-on, lap joint, socket weld) or not at all
/// (blind).
pub fn flange_volume(f: &NozzleFlangeGeom, neck_id: f64) -> f64 {
    let bore = match f.kind {
        FlangeKind::Blind => 0.0,
        FlangeKind::Wn | FlangeKind::Lwn => neck_id,
        _ => f.bore,
    };
    let disc = |d: f64, h: f64| PI / 4.0 * d * d * h;
    let ring = disc(f.od, f.thickness) - disc(bore, f.thickness) - f64::from(f.bolt_count) * disc(f.bolt_hole, f.thickness);
    let rf = if f.rf_d > bore { disc(f.rf_d, f.rf_h) - disc(bore, f.rf_h) } else { 0.0 };
    let hub_len = (f.length_through_hub - f.thickness).max(0.0);
    let hub = if f.hub_od > 0.0 && hub_len > 0.0 {
        let (r1, r2) = (f.hub_od / 2.0, f.hub_weld_d.max(bore) / 2.0);
        PI * hub_len / 3.0 * (r1 * r1 + r1 * r2 + r2 * r2) - disc(bore, hub_len)
    } else {
        0.0
    };
    (ring + rf + hub).max(0.0)
}

/// One BOM line.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The element or attachment.
    pub id: String,
    /// The body it is weighed from (`None` when nothing was built).
    pub body: Option<String>,
    pub description: String,
    pub material: String,
    pub qty: u32,
    /// `measured` (the body), `measured less nozzles`, `computed` (a joined part's share) or
    /// `not built`.
    pub basis: &'static str,
    pub volume_mm3: Option<f64>,
    pub handbook_mm3: Option<f64>,
    pub handbook_rule: Option<String>,
    pub density: f64,
}

impl Item {
    pub fn weight_kg(&self) -> Option<f64> {
        self.volume_mm3.map(|v| kg(v, self.density))
    }

    pub fn handbook_kg(&self) -> Option<f64> {
        self.handbook_mm3.map(|v| kg(v, self.density))
    }

    pub fn to_json(&self, n: usize) -> Value {
        let w = self.weight_kg();
        let h = self.handbook_kg();
        let delta = match (w, h) {
            (Some(w), Some(h)) if h > 0.0 => Some((w - h) / h * 100.0),
            _ => None,
        };
        json!({"item": n, "id": self.id, "body": self.body, "description": self.description, "material": self.material,
            "qty": self.qty, "basis": self.basis, "density_kg_m3": self.density, "volume_mm3": self.volume_mm3,
            "weight_kg": w.map(round2), "handbook_kg": h.map(round2), "handbook": self.handbook_rule,
            "delta_pct": delta.map(round2)})
    }
}

pub fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn densities() {
        assert_eq!(density("SA-516 Gr 70"), 7850.0);
        assert_eq!(density("SA-240 316L"), 8000.0);
        assert_eq!(density("Stainless steel"), 8000.0);
        assert_eq!(density("Aluminium 5083"), 2700.0);
        assert_eq!(density(""), 7850.0);
    }

    #[test]
    fn handbook_head_constants_match_the_shapes() {
        // ASME F&D (crown D, knuckle 0.06·D) and Klöpper (crown D, knuckle 0.1·D) on D = 1.
        let fd = torispherical_area(0.5, 1.0, 0.06).unwrap_or(0.0);
        let kl = torispherical_area(0.5, 1.0, 0.1).unwrap_or(0.0);
        assert!((fd / 0.9286 - 1.0).abs() < 0.005, "{fd}");
        assert!((kl / 0.99 - 1.0).abs() < 0.001, "{kl}");
        let e21 = ellipsoid_cap_area(0.5, 0.25).unwrap_or(0.0);
        assert!((e21 / 1.084 - 1.0).abs() < 0.001, "{e21}");
        assert!((ellipsoid_cap_area(0.5, 0.5).unwrap_or(0.0) - PI / 2.0).abs() < 1e-9);
        assert!(torispherical_area(0.5, 0.2, 0.1).is_none());
        assert!(ellipsoid_cap_area(0.0, 1.0).is_none());
    }

    #[test]
    fn flanges_from_their_table_rows() {
        // B16.5 4 in Cl 150 WN on 114.3 × 6.02: 7.27 kg by hand (ring 5.43, RF 0.18, hub 1.66).
        let wn = NozzleFlangeGeom {
            kind: FlangeKind::Wn,
            od: 230.0,
            bolt_circle: 190.5,
            bolt_count: 8,
            bolt_hole: 19.1,
            thickness: 22.3,
            hub_od: 135.0,
            hub_weld_d: 114.3,
            length_through_hub: 75.0,
            rf_d: 157.2,
            rf_h: 2.0,
            bore: 115.9,
        };
        assert!((kg(flange_volume(&wn, 114.3 - 2.0 * 6.02), 7850.0) - 7.27).abs() < 0.01);
        // 20 in Cl 150 blind: a solid disc less its 20 holes, plus the raised face.
        let blind = NozzleFlangeGeom {
            kind: FlangeKind::Blind,
            hub_od: 0.0,
            length_through_hub: 41.3,
            thickness: 41.3,
            od: 700.0,
            bolt_circle: 635.0,
            bolt_count: 20,
            bolt_hole: 31.8,
            rf_d: 584.2,
            ..wn
        };
        let v = PI / 4.0 * (700.0_f64.powi(2) - 20.0 * 31.8_f64.powi(2)) * 41.3 + PI / 4.0 * 584.2_f64.powi(2) * 2.0;
        assert!((flange_volume(&blind, 488.9) - v).abs() < 1.0);
    }

    #[test]
    fn kg_of_a_cubic_metre_of_steel() {
        assert!((kg(1e9, 7850.0) - 7850.0).abs() < 1e-9);
        assert_eq!(round2(1.23456), 1.23);
    }
}
