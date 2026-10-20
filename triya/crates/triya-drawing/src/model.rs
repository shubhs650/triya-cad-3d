// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The sheet model (`docs/drawing-model.md` §1). Coordinates are sheet millimetres, origin at the
//! bottom-left corner, y up; a view's edges are in view millimetres (the model's, projected) and
//! placed on the sheet at `origin` with `scale`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A point on the sheet or in a view (mm).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct P2 {
    pub x: f64,
    pub y: f64,
}

// Plain methods rather than operator traits: the geometry code reads as a chain of calls.
#[allow(clippy::should_implement_trait)]
impl P2 {
    pub const fn new(x: f64, y: f64) -> P2 {
        P2 { x, y }
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
    pub fn add(self, o: P2) -> P2 {
        P2::new(self.x + o.x, self.y + o.y)
    }
    pub fn sub(self, o: P2) -> P2 {
        P2::new(self.x - o.x, self.y - o.y)
    }
    pub fn mul(self, k: f64) -> P2 {
        P2::new(self.x * k, self.y * k)
    }
    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn dist(self, o: P2) -> f64 {
        self.sub(o).len()
    }
    /// The unit vector, or `None` for a (near) zero one.
    pub fn unit(self) -> Option<P2> {
        let l = self.len();
        (l > 1e-12 && l.is_finite()).then(|| self.mul(1.0 / l))
    }
    /// Turned 90° counter-clockwise.
    pub fn perp(self) -> P2 {
        P2::new(-self.y, self.x)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    #[default]
    Continuous,
    Dashed,
    /// Long-short (centre lines).
    Center,
    /// Short dashes (hidden edges).
    Hidden,
    Phantom,
}

/// The layer an edge or dimension is drawn on (its pen). The DXF layer names are
/// [`Layer::dxf_name`]: `OUTLINE`, `HIDDEN`, `CENTER`, `DIM`, `HATCH`, `TEXT`, `TITLE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    #[default]
    Visible,
    Hidden,
    Center,
    Dimension,
    Hatch,
    Text,
    /// The frame, title block and tables' rules.
    Frame,
}

impl Layer {
    pub const ALL: [Layer; 7] = [Layer::Visible, Layer::Hidden, Layer::Center, Layer::Dimension, Layer::Hatch, Layer::Text, Layer::Frame];

    /// The layer's name in DXF and SVG (`<g id="…">`).
    pub fn dxf_name(self) -> &'static str {
        match self {
            Layer::Visible => "OUTLINE",
            Layer::Hidden => "HIDDEN",
            Layer::Center => "CENTER",
            Layer::Dimension => "DIM",
            Layer::Hatch => "HATCH",
            Layer::Text => "TEXT",
            Layer::Frame => "TITLE",
        }
    }

    /// Pen width (mm on the sheet).
    pub fn width(self) -> f64 {
        match self {
            Layer::Visible => 0.5,
            Layer::Frame => 0.35,
            Layer::Hidden | Layer::Center | Layer::Text => 0.25,
            Layer::Dimension | Layer::Hatch => 0.18,
        }
    }

    /// The line style the layer draws with unless an edge says otherwise.
    pub fn style(self) -> LineStyle {
        match self {
            Layer::Hidden => LineStyle::Hidden,
            Layer::Center => LineStyle::Center,
            _ => LineStyle::Continuous,
        }
    }
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
    /// Axes `rx`, `ry`, the x axis turned by `rotation` (radians); counter-clockwise from the
    /// parameter `start` to `end` (radians; a whole ellipse by default). The point at `t` is
    /// `center + rot(rx cos t, ry sin t)`.
    Ellipse {
        center: P2,
        rx: f64,
        ry: f64,
        rotation: f64,
        #[serde(default)]
        start: f64,
        #[serde(default = "tau")]
        end: f64,
    },
    Polyline {
        points: Vec<P2>,
        closed: bool,
    },
}

fn tau() -> f64 {
    std::f64::consts::TAU
}

impl Edge {
    /// Points along the edge (arcs and ellipses in `n` pieces), for bounds and hit tests.
    pub fn sample(&self, n: u32) -> Vec<P2> {
        let n = n.clamp(1, 4096);
        match self {
            Edge::Line { a, b } => vec![*a, *b],
            Edge::Arc { center, radius, start, end } => {
                let mut sweep = end - start;
                while sweep < 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                (0..=n).map(|i| angle_pt(*center, *radius, start + sweep * f64::from(i) / f64::from(n))).collect()
            }
            Edge::Circle { center, radius } => {
                (0..=n).map(|i| angle_pt(*center, *radius, std::f64::consts::TAU * f64::from(i) / f64::from(n))).collect()
            }
            Edge::Ellipse { center, rx, ry, rotation, start, end } => {
                let mut sweep = end - start;
                while sweep < 0.0 {
                    sweep += std::f64::consts::TAU;
                }
                (0..=n).map(|i| ellipse_pt(*center, *rx, *ry, *rotation, start + sweep * f64::from(i) / f64::from(n))).collect()
            }
            Edge::Polyline { points, closed } => {
                let mut v = points.clone();
                if *closed && let Some(f) = points.first() {
                    v.push(*f);
                }
                v
            }
        }
    }
}

/// The point at `angle` on the circle about `c` of radius `r`.
pub fn angle_pt(c: P2, r: f64, angle: f64) -> P2 {
    P2::new(c.x + r * angle.cos(), c.y + r * angle.sin())
}

/// The point at parameter `t` on an ellipse.
pub fn ellipse_pt(c: P2, rx: f64, ry: f64, rotation: f64, t: f64) -> P2 {
    let (sr, cr) = rotation.sin_cos();
    let (x, y) = (rx * t.cos(), ry * t.sin());
    P2::new(c.x + x * cr - y * sr, c.y + x * sr + y * cr)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StyledEdge {
    pub edge: Edge,
    #[serde(default)]
    pub layer: Layer,
    #[serde(default)]
    pub style: LineStyle,
}

impl StyledEdge {
    pub fn new(edge: Edge, layer: Layer) -> StyledEdge {
        StyledEdge { edge, layer, style: layer.style() }
    }
    pub fn line(a: P2, b: P2, layer: Layer) -> StyledEdge {
        StyledEdge::new(Edge::Line { a, b }, layer)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DimKind {
    /// Between `a` and `b` measured along `dir` (a unit vector; the dimension line is
    /// perpendicular to the extension lines), the dimension line through `line_at`.
    Linear {
        a: P2,
        b: P2,
        line_at: P2,
        dir: P2,
    },
    Diameter {
        center: P2,
        radius: f64,
        text_at: P2,
    },
    Radius {
        center: P2,
        radius: f64,
        text_at: P2,
    },
    /// Between the directions `a` and `b` (radians, counter-clockwise from `a`) seen from
    /// `vertex`, the arc at `arc_radius` (view mm).
    Angular {
        vertex: P2,
        a: f64,
        b: f64,
        arc_radius: f64,
    },
}

/// A dimension: geometry in view coordinates, its text, and the vessel field(s) it shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dimension {
    pub kind: DimKind,
    pub text: String,
    /// The vessel field ids it comes from (`E2.length_mm`, `N1.axial.mm`, comma-separated), so
    /// a regenerate re-derives it instead of re-detecting geometry; empty for a free dimension.
    #[serde(default)]
    pub source: String,
    /// Where the text goes (view coordinates) when the collision nudge moved it.
    #[serde(default)]
    pub text_at: Option<P2>,
}

impl Dimension {
    /// A horizontal (`dir` = x) or vertical (`dir` = y) linear dimension.
    pub fn linear(a: P2, b: P2, line_at: P2, dir: P2, text: impl Into<String>, source: impl Into<String>) -> Dimension {
        Dimension { kind: DimKind::Linear { a, b, line_at, dir }, text: text.into(), source: source.into(), text_at: None }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelStyle {
    /// Plain text.
    #[default]
    Note,
    /// The text in a circle (item numbers).
    Balloon,
    /// The text in a box (nozzle marks).
    Mark,
}

/// A text in a view, optionally with a leader to `leader` (view coordinates).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub at: P2,
    pub text: String,
    #[serde(default)]
    pub leader: Option<P2>,
    #[serde(default)]
    pub style: LabelStyle,
    /// Text height on the sheet (mm).
    #[serde(default = "label_height")]
    pub height: f64,
    /// Counter-clockwise (radians).
    #[serde(default)]
    pub angle: f64,
}

fn label_height() -> f64 {
    TEXT_H
}

impl Label {
    pub fn note(at: P2, text: impl Into<String>) -> Label {
        Label { at, text: text.into(), leader: None, style: LabelStyle::Note, height: TEXT_H, angle: 0.0 }
    }
}

/// Text height of dimensions and notes (mm on the sheet; ISO 3098).
pub const TEXT_H: f64 = 3.5;
/// Text height of view titles and the title block's title.
pub const TITLE_H: f64 = 5.0;

/// A hatched region: closed loops (view coordinates) filled even-odd with parallel lines at
/// `angle` (radians), `spacing` mm apart on the sheet (ANSI 31: 45°).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HatchRegion {
    pub loops: Vec<Vec<P2>>,
    #[serde(default = "hatch_angle")]
    pub angle: f64,
    #[serde(default = "hatch_spacing")]
    pub spacing: f64,
}

fn hatch_angle() -> f64 {
    std::f64::consts::FRAC_PI_4
}

fn hatch_spacing() -> f64 {
    2.5
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    #[default]
    Elevation,
    Plan,
    Section,
    Detail,
}

/// A view: edges, dimensions, labels and hatching in view coordinates, placed on the sheet at
/// `origin` with `scale` (sheet mm per model mm).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub name: String,
    #[serde(default)]
    pub kind: ViewKind,
    /// Shown under the view (`ELEVATION`, `SECTION A-A`), with the scale.
    #[serde(default)]
    pub title: String,
    pub origin: P2,
    pub scale: f64,
    #[serde(default)]
    pub edges: Vec<StyledEdge>,
    #[serde(default)]
    pub dimensions: Vec<Dimension>,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub hatches: Vec<HatchRegion>,
}

impl View {
    pub fn new(name: &str, origin: P2, scale: f64) -> View {
        View {
            name: name.into(),
            kind: ViewKind::default(),
            title: String::new(),
            origin,
            scale,
            edges: Vec::new(),
            dimensions: Vec::new(),
            labels: Vec::new(),
            hatches: Vec::new(),
        }
    }
    /// A view point on the sheet.
    pub fn to_sheet(&self, p: P2) -> P2 {
        P2::new(self.origin.x + p.x * self.scale, self.origin.y + p.y * self.scale)
    }
    pub fn edge(&mut self, edge: Edge, layer: Layer) {
        self.edges.push(StyledEdge::new(edge, layer));
    }
    pub fn line(&mut self, a: P2, b: P2, layer: Layer) {
        self.edges.push(StyledEdge::line(a, b, layer));
    }
    /// The bounding box of the edges (view coordinates), `None` when empty.
    pub fn edge_bounds(&self) -> Option<(P2, P2)> {
        bounds(self.edges.iter().flat_map(|e| e.edge.sample(16)))
    }
}

/// The bounding box of `pts` (finite ones), `None` when there are none.
pub fn bounds(pts: impl IntoIterator<Item = P2>) -> Option<(P2, P2)> {
    pts.into_iter().filter(|p| p.is_finite()).fold(None, |acc, p| {
        Some(match acc {
            None => (p, p),
            Some((lo, hi)) => (P2::new(lo.x.min(p.x), lo.y.min(p.y)), P2::new(hi.x.max(p.x), hi.y.max(p.y))),
        })
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableKind {
    #[default]
    Other,
    DesignData,
    NozzleSchedule,
    Bom,
    Notes,
    Revisions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    #[serde(default)]
    pub kind: TableKind,
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
    6.0
}

/// Width of a column without a stated width (mm).
pub const DEFAULT_COLUMN_W: f64 = 30.0;

impl Table {
    pub fn new(kind: TableKind, title: &str, columns: &[&str], widths: &[f64]) -> Table {
        Table {
            kind,
            title: title.into(),
            at: P2::default(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows: Vec::new(),
            row_height: default_row_height(),
            column_widths: widths.to_vec(),
        }
    }
    pub fn column_w(&self, i: usize) -> f64 {
        self.column_widths.get(i).copied().filter(|w| w.is_finite() && *w > 0.0).unwrap_or(DEFAULT_COLUMN_W)
    }
    pub fn width(&self) -> f64 {
        (0..self.columns.len()).map(|i| self.column_w(i)).sum()
    }
    fn rh(&self) -> f64 {
        if self.row_height.is_finite() && self.row_height > 0.0 { self.row_height } else { default_row_height() }
    }
    /// Height on the sheet: the title line, the header and the rows.
    pub fn height(&self) -> f64 {
        TABLE_TITLE_H + self.rh() * (1 + self.rows.len()) as f64
    }
    pub fn row_h(&self) -> f64 {
        self.rh()
    }
}

/// Height of a table's title line (mm).
pub const TABLE_TITLE_H: f64 = 7.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    pub rev: String,
    pub date: String,
    pub description: String,
    pub by: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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
    pub rev: String,
    #[serde(default)]
    pub revisions: Vec<Revision>,
    /// More placeholders for a user template (`{{key}}` → value).
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

impl TitleBlock {
    /// The value of placeholder `{{key}}` (`None` for an unknown key).
    pub fn field(&self, key: &str) -> Option<String> {
        Some(match key {
            "title" => self.title.clone(),
            "drawing_no" | "drawing_number" => self.drawing_number.clone(),
            "project" => self.project.clone(),
            "org" | "organization" => self.organization.clone(),
            "drawn_by" => self.drawn_by.clone(),
            "checked_by" => self.checked_by.clone(),
            "date" => self.date.clone(),
            "scale" => self.scale.clone(),
            "sheet" | "sheet_of" => self.sheet_of.clone(),
            "rev" => self.rev.clone(),
            _ => return self.extra.get(key).cloned(),
        })
    }
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
    pub fn name(self) -> &'static str {
        match self {
            SheetSize::A0 => "A0",
            SheetSize::A1 => "A1",
            SheetSize::A2 => "A2",
            SheetSize::A3 => "A3",
            SheetSize::A4 => "A4",
        }
    }
    pub fn parse(s: &str) -> Option<SheetSize> {
        Some(match s.trim().to_ascii_uppercase().as_str() {
            "A0" => SheetSize::A0,
            "A1" => SheetSize::A1,
            "A2" => SheetSize::A2,
            "A3" => SheetSize::A3,
            "A4" => SheetSize::A4,
            _ => return None,
        })
    }
}

/// Margin between the sheet edge and the frame (mm).
pub const FRAME_MARGIN: f64 = 10.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    pub size: SheetSize,
    #[serde(default)]
    pub views: Vec<View>,
    #[serde(default)]
    pub tables: Vec<Table>,
    #[serde(default)]
    pub title_block: TitleBlock,
    /// The title block template (SVG subset with `{{placeholders}}`, see [`crate::template`]);
    /// empty: the OET default.
    #[serde(default)]
    pub template: String,
    /// Edges drawn directly on the sheet.
    #[serde(default)]
    pub edges: Vec<StyledEdge>,
    /// Texts placed directly on the sheet.
    #[serde(default)]
    pub labels: Vec<Label>,
}

impl Sheet {
    pub fn new(size: SheetSize) -> Sheet {
        Sheet {
            size,
            views: Vec::new(),
            tables: Vec::new(),
            title_block: TitleBlock::default(),
            template: String::new(),
            edges: Vec::new(),
            labels: Vec::new(),
        }
    }
}
