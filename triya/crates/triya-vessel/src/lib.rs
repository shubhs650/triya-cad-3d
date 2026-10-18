// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D pressure-vessel add-on (L4 of the overlay, beside the engine).
//!
//! A vessel is described by a [`model::VesselDoc`] (shell elements stacked along the axis, with
//! attachments placed on them), resolved into axial stations by [`eval::resolve`], turned into a
//! plan of ordinary engine commands by [`emit::plan`], and built by the `vessel.*` commands in
//! [`commands`], which run that plan through the engine's own command path in one undo step.
//! Nozzles are placed by [`nozzle`] and joined into their hosts after the shell, each checked
//! against a reference volume computed without the kernel.
//!
//! # Where the vessel JSON lives
//! The document is kept in the design's own metadata, under the reverse-DNS key
//! [`commands::METADATA_KEY`] (`in.openengtools.triya.vessel`), as `{"doc": VesselDoc, "generated":
//! {"features": [ids]}}`. Upstream's `Document::metadata` is made for exactly this ("another
//! tool's data goes under its reverse-DNS key", see `crates/doc/src/metadata.rs`): it is saved
//! with the `.solidcraft` file, round-trips through older and newer SolidCraft versions untouched,
//! takes part in undo (a metadata change is a document change) and needs **no design-format
//! change** (`crates/doc/src/format/pending/`). The limits are upstream's: one key of at most
//! 64 KiB; a larger vessel is refused with an error, never truncated.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod commands;
pub mod emit;
pub mod eval;
pub mod model;
pub mod nozzle;
pub mod support;

pub use commands::COMMANDS;
pub use emit::{CommandCall, plan};
pub use eval::{Resolved, Station, resolve};
pub use model::VesselDoc;

#[derive(Debug, thiserror::Error)]
pub enum VesselError {
    #[error("not supported yet: {0}")]
    NotSupportedYet(String),
    #[error("invalid vessel: {0}")]
    Invalid(String),
    #[error("{0}")]
    Stdlib(#[from] triya_stdlib::StdlibError),
    #[error("vessel JSON: {0}")]
    Json(String),
}

pub type Result<T> = std::result::Result<T, VesselError>;

impl From<serde_json::Error> for VesselError {
    fn from(e: serde_json::Error) -> Self {
        VesselError::Json(e.to_string())
    }
}

impl From<VesselError> for solidcraft_engine::EngineError {
    fn from(e: VesselError) -> Self {
        solidcraft_engine::EngineError::Other(e.to_string())
    }
}

#[cfg(test)]
mod nozzle_tests;
#[cfg(test)]
mod tests;
