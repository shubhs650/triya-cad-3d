// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! One row of a flange dimension table. Lengths in millimetres.

use serde::{Deserialize, Serialize};

/// The flange type a table row describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    /// The key a vessel attachment uses to pick this row: `<nps>/<class>/<kind>`.
    pub fn size_key(&self) -> String {
        format!("{}/{}/{}", self.nps, self.class, self.kind.code())
    }
}
