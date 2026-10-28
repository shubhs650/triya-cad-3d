// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D drawings: a 2D sheet model (views of edges, dimensions, tables, a title block,
//! revisions) and plotters that write it out. No UI toolkit here: the egui page that shows a
//! sheet belongs to `triya-ui`; this crate stays buildable for wasm32 and the CLI.
//!
//! Lengths are millimetres on the sheet; angles in radians.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod commands;
pub mod dims;
pub mod dxf;
pub mod fmt;
pub mod ga;
pub mod hatch;
pub mod model;
pub mod parts;
pub mod pdf;
pub mod plot;
pub mod proj;
pub mod svg;
pub mod tables;
pub mod template;
pub mod vessel_views;

pub use commands::COMMANDS;
pub use model::{Dimension, Edge, Revision, Sheet, Table, TitleBlock, View};
pub use plot::{Plotter, plot_sheet};
pub use svg::SvgPlotter;

/// A weight rounded to 0.1 kg.
pub fn bom_round(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// The sheet as SVG text.
pub fn to_svg(sheet: &Sheet) -> Result<String> {
    let mut p = SvgPlotter::for_sheet(sheet);
    plot_sheet(&mut p, sheet)?;
    Ok(p.finish())
}

/// The sheet as DXF (AutoCAD 2000 text).
pub fn to_dxf(sheet: &Sheet) -> Result<String> {
    let mut p = dxf::DxfPlotter::new();
    plot_sheet(&mut p, sheet)?;
    p.finish()
}

/// The sheet as a one-page vector PDF.
pub fn to_pdf(sheet: &Sheet) -> Result<Vec<u8>> {
    let mut p = pdf::PdfPlotter::for_sheet(sheet);
    plot_sheet(&mut p, sheet)?;
    Ok(p.finish())
}

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
