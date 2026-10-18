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

/// One flange size. `thickness` is the ring (`tf` in B16.5) without the raised face; the raised
/// face (`raised_face_h`, 2 mm for Class 150/300 and 7 mm above in B16.5's metric tables) stands
/// on top of it. `length_through_hub` runs from the back of the hub to the ring's face, also
/// without the raised face.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FlangeRow {
    /// Nominal pipe size, canonical (`"2"`, `"1/2"`, `"1-1/4"`, `"24"`): see [`crate::canonical_nps`].
    pub nps: String,
    /// Pressure class or PN rating as written (`"150"`, `"PN16"`).
    pub class: String,
    pub kind: FlangeKind,
    /// Outside diameter.
    pub od: f64,
    pub bolt_circle: f64,
    pub bolt_count: u32,
    /// Bolt designation as written (`"M16"`, `"5/8"`, `"1-1/8"`).
    pub bolt_size: String,
    /// Bolt-hole diameter as the table gives it; without it the hole is derived from the bolt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bolt_hole: Option<f64>,
    /// Flange (ring) thickness without the raised face.
    pub thickness: f64,
    /// Hub diameter at the base (0 for a blind).
    pub hub_od: f64,
    /// Hub diameter at the weld bevel (a weld neck's `A`, the pipe's OD).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hub_weld_d: Option<f64>,
    /// Raised-face diameter (0 when the flange has none, e.g. lap joint).
    #[serde(default)]
    pub raised_face_d: f64,
    #[serde(default)]
    pub raised_face_h: f64,
    pub length_through_hub: f64,
    /// Bore of a slip-on, socket-weld or lap-joint flange (fits over the pipe).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bore: Option<f64>,
    /// Socket depth of a socket-weld flange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_depth: Option<f64>,
    /// Where the values come from, one entry per independent source.
    #[serde(default)]
    pub source: Vec<String>,
    #[serde(default)]
    pub verified: crate::Verified,
    /// Disagreements between the sources and what was kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
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
            ("raised_face_h", self.raised_face_h),
            ("length_through_hub", self.length_through_hub),
        ] {
            crate::non_negative(name, v)?;
        }
        for (name, v) in [("bolt_hole", self.bolt_hole), ("hub_weld_d", self.hub_weld_d), ("bore", self.bore), ("socket_depth", self.socket_depth)] {
            if let Some(v) = v {
                crate::positive(name, v)?;
            }
        }
        crate::positive("od", self.od)?;
        crate::positive("thickness", self.thickness)?;
        if !(self.bolt_circle < self.od) {
            return Err(crate::StdlibError::Invalid(format!("bolt circle {} must be inside the OD {}", self.bolt_circle, self.od)));
        }
        if self.raised_face_d > 0.0 && !(self.raised_face_d < self.bolt_circle) {
            return Err(crate::StdlibError::Invalid(format!(
                "raised face {} must be inside the bolt circle {}",
                self.raised_face_d, self.bolt_circle
            )));
        }
        if self.length_through_hub + 1e-9 < self.thickness {
            return Err(crate::StdlibError::Invalid(format!(
                "length through hub {} is shorter than the ring {}",
                self.length_through_hub, self.thickness
            )));
        }
        if self.bolt_diameter_mm().is_none() {
            return Err(crate::StdlibError::Invalid(format!("cannot read the bolt size `{}`", self.bolt_size)));
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

    /// The bolt-hole diameter: the table's, or [`FlangeRow::derived_bolt_hole_mm`] when the row
    /// has none.
    pub fn bolt_hole_mm(&self) -> Option<f64> {
        self.bolt_hole.filter(|h| h.is_finite() && *h > 0.0).or_else(|| self.derived_bolt_hole_mm())
    }

    /// The hole derived from the bolt: metric bolts + 2 mm up to M24 and + 3 mm above (the
    /// clearance EN 1092-1 tables show), inch bolts + 1/8 in (the B16.5 table note). An
    /// approximation for rows without a hole; callers flag it.
    pub fn derived_bolt_hole_mm(&self) -> Option<f64> {
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
            hub_weld_d: None,
            raised_face_d: 92.0,
            raised_face_h: 2.0,
            length_through_hub: 62.0,
            bolt_hole: None,
            bore: None,
            socket_depth: None,
            source: vec!["test".into()],
            verified: crate::Verified::SingleSource,
            note: None,
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
            assert!(row(bad).validate().is_err(), "{bad}");
        }
        let mut r = row("5/8");
        r.bolt_hole = Some(19.1);
        assert_eq!(r.bolt_hole_mm(), Some(19.1));
        assert!(r.validate().is_ok());
    }

    #[test]
    fn rows_that_cannot_be_built_are_refused() {
        let mut r = row("M16");
        r.bolt_circle = 160.0;
        assert!(r.validate().is_err());
        let mut r = row("M16");
        r.raised_face_d = 130.0;
        assert!(r.validate().is_err());
        let mut r = row("M16");
        r.length_through_hub = 10.0;
        assert!(r.validate().is_err());
        let mut r = row("M16");
        r.bolt_hole = Some(f64::NAN);
        assert!(r.validate().is_err());
    }
}
