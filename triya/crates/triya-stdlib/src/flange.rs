// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! One row of a flange dimension table. Lengths in millimetres.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The flange type a table row describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FlangeKind {
    /// Weld neck.
    Wn,
    /// Slip-on.
    So,
    Blind,
    /// Lap joint.
    Lj,
    /// Socket weld.
    Sw,
    /// Long weld neck.
    Lwn,
}

impl FlangeKind {
    /// The short code tables and size keys use (`wn`, `so`, `blind`, `lj`, `sw`, `lwn`).
    pub fn code(self) -> &'static str {
        match self {
            FlangeKind::Wn => "wn",
            FlangeKind::So => "so",
            FlangeKind::Blind => "blind",
            FlangeKind::Lj => "lj",
            FlangeKind::Sw => "sw",
            FlangeKind::Lwn => "lwn",
        }
    }

    /// The kind named by its code (case-insensitive).
    pub fn parse(code: &str) -> Option<FlangeKind> {
        [FlangeKind::Wn, FlangeKind::So, FlangeKind::Blind, FlangeKind::Lj, FlangeKind::Sw, FlangeKind::Lwn]
            .into_iter()
            .find(|k| k.code().eq_ignore_ascii_case(code.trim()))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlangeRow {
    /// Nominal pipe size as written in the table (`"2"`, `"1/2"`, `"24"`).
    pub nps: String,
    /// Pressure class or PN rating as written (`"150"`, `"PN16"`).
    pub class: String,
    pub kind: FlangeKind,
    /// Outside diameter.
    pub od: f64,
    pub bolt_circle: f64,
    pub bolt_count: u32,
    /// Bolt designation as written (`"M16"`, `"5/8"`).
    pub bolt_size: String,
    /// Flange (ring) thickness.
    pub thickness: f64,
    /// Hub diameter at the base (0 for a blind).
    pub hub_od: f64,
    pub raised_face_d: f64,
    pub length_through_hub: f64,
    /// Bore schedule the table row is for, when the standard distinguishes them.
    #[serde(default)]
    pub bore_schedule: Option<String>,
}

impl FlangeRow {
    /// Every length finite and not negative, at least one bolt: a row a modeller can trust not
    /// to produce NaN geometry.
    pub fn validate(&self) -> crate::Result<()> {
        for (name, v) in [
            ("od", self.od),
            ("bolt_circle", self.bolt_circle),
            ("thickness", self.thickness),
            ("hub_od", self.hub_od),
            ("raised_face_d", self.raised_face_d),
            ("length_through_hub", self.length_through_hub),
        ] {
            crate::non_negative(name, v)?;
        }
        if self.bolt_count == 0 || self.bolt_count > 1000 {
            return Err(crate::StdlibError::Invalid(format!("`bolt_count` must be 1…1000 (got {})", self.bolt_count)));
        }
        Ok(())
    }

    /// The bolt's nominal diameter in mm, from `bolt_size`: `"M16"` (metric) or inches as
    /// `"5/8"`, `"1-1/8"`, `"1 1/8"` or `"0.625"`.
    pub fn bolt_diameter_mm(&self) -> Option<f64> {
        let s = self.bolt_size.trim();
        if let Some(m) = s.strip_prefix(['M', 'm']) {
            return m.trim().parse::<f64>().ok().filter(|d| d.is_finite() && *d > 0.0 && *d < 500.0);
        }
        let inches = parse_inches(s.trim_end_matches(['"', ' ']).trim_end_matches("in").trim())?;
        Some(inches * 25.4).filter(|d| d.is_finite() && *d > 0.0 && *d < 500.0)
    }

    /// The bolt-hole diameter: metric bolts + 2 mm up to M24 and + 3 mm above (the clearance
    /// EN 1092-1 tables show), inch bolts + 1/8 in (the B16.5 table note). An approximation
    /// until the tables carry the hole (T3.1); callers flag it.
    pub fn bolt_hole_mm(&self) -> Option<f64> {
        let d = self.bolt_diameter_mm()?;
        Some(if self.bolt_size.trim().starts_with(['M', 'm']) { d + if d <= 24.0 { 2.0 } else { 3.0 } } else { d + 3.175 })
    }

    /// The key a vessel attachment uses to pick this row: `<nps>/<class>/<kind>`.
    pub fn size_key(&self) -> String {
        format!("{}/{}/{}", self.nps, self.class, self.kind.code())
    }
}

/// `"5/8"`, `"1-1/8"`, `"1 1/8"`, `"2"`, `"0.625"` as inches.
fn parse_inches(s: &str) -> Option<f64> {
    let frac = |f: &str| -> Option<f64> {
        match f.split_once('/') {
            Some((n, d)) => {
                let (n, d) = (n.trim().parse::<f64>().ok()?, d.trim().parse::<f64>().ok()?);
                (d > 0.0).then(|| n / d)
            }
            None => f.trim().parse::<f64>().ok(),
        }
    };
    match s.split_once(['-', ' ']) {
        Some((whole, f)) if f.contains('/') => Some(whole.trim().parse::<f64>().ok()? + frac(f)?),
        _ => frac(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(bolt: &str) -> FlangeRow {
        FlangeRow {
            nps: "2".into(),
            class: "150".into(),
            kind: FlangeKind::Wn,
            od: 150.0,
            bolt_circle: 120.0,
            bolt_count: 4,
            bolt_size: bolt.into(),
            thickness: 18.0,
            hub_od: 84.0,
            raised_face_d: 92.0,
            length_through_hub: 62.0,
            bore_schedule: None,
        }
    }

    #[test]
    fn bolt_sizes_parse_and_holes_have_clearance() {
        assert_eq!(row("M16").bolt_hole_mm(), Some(18.0));
        assert_eq!(row("M30").bolt_hole_mm(), Some(33.0));
        assert!((row("5/8").bolt_diameter_mm().unwrap() - 15.875).abs() < 1e-9);
        assert!((row("1-1/8").bolt_diameter_mm().unwrap() - 28.575).abs() < 1e-9);
        assert!((row("1 1/8\"").bolt_diameter_mm().unwrap() - 28.575).abs() < 1e-9);
        assert!((row("0.625").bolt_hole_mm().unwrap() - (15.875 + 3.175)).abs() < 1e-9);
        for bad in ["", "M", "Mx", "1/0", "nope", "M-5", "M1e9"] {
            assert_eq!(row(bad).bolt_hole_mm(), None, "{bad}");
        }
    }
}
