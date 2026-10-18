// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D standards library (L0 of the overlay: serde only, no geometry kernel).
//!
//! Typed rows for the tables a pressure-vessel modeller looks things up in (flanges, pipe
//! schedules, head presets), a [`StandardRef`] naming the document a row comes from, and a
//! [`TableSet`] that loads them from JSON. The embedded `data/*.json` files are **illustrative
//! placeholders** (each says so in its `source` field): they show the table shape so the
//! loader and the callers can be tested. Real tables are the user's to supply and verify;
//! this crate reproduces no standard.
//!
//! Head depth formulas (`HeadPreset::dish_depth`) are plain geometry, so they are implemented
//! here and unit-tested against published approximations.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod flange;
pub mod head;
pub mod pipe;
pub mod tables;

pub use flange::{FlangeKind, FlangeRow};
pub use head::{HeadPreset, RatioBasis};
pub use pipe::PipeScheduleRow;
pub use tables::{HeadPresetRow, Table, TableSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The document a row comes from: organisation (`ASME`, `EN`, `DIN`, `IS`), number and edition.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StandardRef {
    pub org: String,
    pub number: String,
    #[serde(default)]
    pub rev: String,
}

impl std::fmt::Display for StandardRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.rev.is_empty() { write!(f, "{} {}", self.org, self.number) } else { write!(f, "{} {}:{}", self.org, self.number, self.rev) }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StdlibError {
    #[error("not supported yet: {0}")]
    NotSupportedYet(String),
    #[error("invalid value: {0}")]
    Invalid(String),
    #[error("table `{table}`: {msg}")]
    Table { table: String, msg: String },
}

pub type Result<T> = std::result::Result<T, StdlibError>;

/// A usable length or ratio: finite and not negative (input-derived numbers are hostile).
pub(crate) fn non_negative(name: &str, x: f64) -> Result<f64> {
    if x.is_finite() && x >= 0.0 { Ok(x) } else { Err(StdlibError::Invalid(format!("`{name}` must be a finite number ≥ 0 (got {x})"))) }
}

/// A usable length or ratio: finite and strictly positive.
pub(crate) fn positive(name: &str, x: f64) -> Result<f64> {
    if x.is_finite() && x > 0.0 { Ok(x) } else { Err(StdlibError::Invalid(format!("`{name}` must be a finite number > 0 (got {x})"))) }
}
