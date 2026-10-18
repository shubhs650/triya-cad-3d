// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D drawings: a 2D sheet model (views of edges, dimensions, tables, a title block,
//! revisions) and plotters that write it out. No UI toolkit here: the egui page that shows a
//! sheet belongs to `triya-ui`; this crate stays buildable for wasm32 and the CLI.
//!
//! Lengths are millimetres on the sheet; angles in radians.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod model;
pub mod plot;
pub mod vessel_views;

pub use model::{Dimension, Edge, Revision, Sheet, Table, TitleBlock, View};
pub use plot::{Plotter, SvgPlotter};

#[derive(Debug, thiserror::Error)]
pub enum DrawingError {
    #[error("not supported yet: {0}")]
    NotSupportedYet(String),
    #[error("invalid drawing: {0}")]
    Invalid(String),
    #[error("{0}")]
    Vessel(#[from] triya_vessel::VesselError),
}

pub type Result<T> = std::result::Result<T, DrawingError>;
