// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! One row of a pipe schedule table. Lengths in millimetres.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PipeScheduleRow {
    /// Nominal pipe size, canonical (`"2"`, `"1/2"`, `"1-1/4"`).
    pub nps: String,
    /// Nominal diameter (DN) when the table gives one.
    #[serde(default)]
    pub dn: Option<u32>,
    /// Outside diameter.
    pub od: f64,
    /// Schedule designation as written (`"STD"`, `"40"`, `"XS"`, `"160"`).
    pub schedule: String,
    /// Nominal wall thickness.
    pub wall: f64,
    #[serde(default)]
    pub source: Vec<String>,
    #[serde(default)]
    pub verified: crate::Verified,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl PipeScheduleRow {
    pub fn validate(&self) -> crate::Result<()> {
        crate::positive("od", self.od)?;
        crate::positive("wall", self.wall)?;
        if self.wall * 2.0 >= self.od {
            return Err(crate::StdlibError::Invalid(format!("wall {} leaves no bore in od {}", self.wall, self.od)));
        }
        Ok(())
    }

    /// Inside diameter.
    pub fn id(&self) -> f64 {
        self.od - 2.0 * self.wall
    }

    /// The key a vessel attachment uses to pick this row: `<nps>/<schedule>`.
    pub fn size_key(&self) -> String {
        format!("{}/{}", self.nps, self.schedule)
    }
}
