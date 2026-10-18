// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D standards library (L0 of the overlay: serde only, no geometry kernel).
//!
//! Typed rows for the tables a pressure-vessel modeller looks things up in (flanges, pipe
//! schedules, head presets), a [`StandardRef`] naming the document a row comes from, and a
//! [`TableSet`] that loads them from JSON. The embedded `data/*.json` tables are dimensions of
//! public standards re-keyed from open sources (each row lists its sources and how it was
//! verified, `docs/standards-library.md`); they are facts of the standard, not a copy of it, and
//! must still be checked against the edition in force before fabrication.
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

/// How far a table row has been checked (`docs/standards-library.md` §1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Verified {
    /// Two independent sources agree (disagreements are in the row's `note`).
    TwoSource,
    /// One source only.
    SingleSource,
    /// Computed from other rows or a formula.
    Derived,
    /// Not checked (rows of a user's table that do not say).
    #[default]
    Unverified,
    /// A placeholder; never shipped outside tests.
    Illustrative,
}

impl Verified {
    /// Values a modeller may use without a warning.
    pub fn is_trusted(self) -> bool {
        matches!(self, Verified::TwoSource | Verified::Derived)
    }
}

/// A nominal pipe size in the form the tables use: whole and fraction joined by `-`
/// (`"1-1/4"`), from `"1 1/4"`, `"1.1/4"`, `"1-1/4"`, `"1¼"` or `"NPS 1 1/4"`. Whitespace-only or
/// over-long input gives an empty string, which matches no row.
pub fn canonical_nps(s: &str) -> String {
    if s.len() > 32 {
        return String::new();
    }
    let s = s.trim();
    let s = s.strip_prefix("NPS").or_else(|| s.strip_prefix("nps")).unwrap_or(s);
    let mut t = String::new();
    for c in s.chars() {
        match c {
            '½' => t.push_str(" 1/2"),
            '¼' => t.push_str(" 1/4"),
            '¾' => t.push_str(" 3/4"),
            '.' | '-' | '"' => t.push(' '),
            c => t.push(c),
        }
    }
    // A dot only ever separates whole and fraction in tables ("1.1/4"); decimals match nothing.
    t.split_whitespace().collect::<Vec<_>>().join("-")
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
