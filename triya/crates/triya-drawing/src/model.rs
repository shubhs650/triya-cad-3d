// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The sheet model. Coordinates are sheet millimetres, origin at the bottom-left corner, y up.

use serde::{Deserialize, Serialize};

/// A point on the sheet or in a view (mm).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct P2 {
    pub x: f64,
    pub y: f64,
}

impl P2 {
    pub const fn new(x: f64, y: f64) -> P2 {
        P2 { x, y }
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    #[default]
    Continuous,
    Dashed,
    /// Long-short-short (centre lines).
    Center,
    /// Long-short (hidden edges in some conventions).
    Hidden,
    Phantom,
}

/// The layer an edge or dimension is drawn on (its pen).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    #[default]
    Visible,
    Hidden,
    Center,
    Dimension,
    Hatch,
    Text,
    Frame,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Edge {
    Line {
        a: P2,
        b: P2,
    },
    /// Counter-clockwise from `start` to `end` (radians) about `center`.
    Arc {
        center: P2,
        radius: f64,
        start: f64,
        end: f64,
    },
    Circle {
        center: P2,
        radius: f64,
    },
    /// Axes `rx`, `ry`, the x axis turned by `rotation` (radians).
    Ellipse {
        center: P2,
        rx: f64,
        ry: f64,
        rotation: f64,
    },
    Polyline {
        points: Vec<P2>,
        closed: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StyledEdge {
    pub edge: Edge,
    #[serde(default)]
    pub layer: Layer,
    #[serde(default)]
    pub style: LineStyle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Dimension {
    /// Between `a` and `b`, the dimension line offset to pass through `line_at`.
    Linear {
        a: P2,
        b: P2,
        line_at: P2,
        text: String,
    },
    Diameter {
        center: P2,
        radius: f64,
        text_at: P2,
        text: String,
    },
    Radius {
        center: P2,
        radius: f64,
        text_at: P2,
        text: String,
    },
    /// Between the directions `a` and `b` (radians) seen from `vertex`, the arc at `arc_radius`.
    Angular {
        vertex: P2,
        a: f64,
        b: f64,
        arc_radius: f64,
        text: String,
    },
}

/// A view: edges and dimensions in view coordinates, placed on the sheet at `origin` with `scale`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    pub origin: P2,
    pub scale: f64,
    #[serde(default)]
    pub edges: Vec<StyledEdge>,
    #[serde(default)]
    pub dimensions: Vec<Dimension>,
}

impl View {
    pub fn new(name: &str, origin: P2, scale: f64) -> View {
        View { name: name.into(), origin, scale, edges: Vec::new(), dimensions: Vec::new() }
    }
    /// A view point on the sheet.
    pub fn to_sheet(&self, p: P2) -> P2 {
        P2::new(self.origin.x + p.x * self.scale, self.origin.y + p.y * self.scale)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub title: String,
    /// Top-left corner on the sheet.
    pub at: P2,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    #[serde(default = "default_row_height")]
    pub row_height: f64,
    #[serde(default)]
    pub column_widths: Vec<f64>,
}

fn default_row_height() -> f64 {
    8.0
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub rev: String,
    pub date: String,
    pub description: String,
    pub by: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TitleBlock {
    pub title: String,
    pub drawing_number: String,
    pub project: String,
    pub organization: String,
    pub drawn_by: String,
    pub checked_by: String,
    pub date: String,
    pub scale: String,
    pub sheet_of: String,
    #[serde(default)]
    pub revisions: Vec<Revision>,
}

/// Standard sheet sizes (landscape, mm).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetSize {
    A0,
    A1,
    A2,
    A3,
    A4,
}

impl SheetSize {
    /// Width and height in mm (ISO 216, landscape).
    pub fn mm(self) -> (f64, f64) {
        match self {
            SheetSize::A0 => (1189.0, 841.0),
            SheetSize::A1 => (841.0, 594.0),
            SheetSize::A2 => (594.0, 420.0),
            SheetSize::A3 => (420.0, 297.0),
            SheetSize::A4 => (297.0, 210.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    pub size: SheetSize,
    #[serde(default)]
    pub views: Vec<View>,
    #[serde(default)]
    pub tables: Vec<Table>,
    #[serde(default)]
    pub title_block: TitleBlock,
    /// Edges drawn directly on the sheet (the frame, notes' leaders).
    #[serde(default)]
    pub edges: Vec<StyledEdge>,
}

impl Sheet {
    pub fn new(size: SheetSize) -> Sheet {
        Sheet { size, views: Vec::new(), tables: Vec::new(), title_block: TitleBlock::default(), edges: Vec::new() }
    }
}
