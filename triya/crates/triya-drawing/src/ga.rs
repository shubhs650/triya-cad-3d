// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The general-arrangement sheet: views, dimensions, tables and the title block laid out on one
//! sheet (`docs/drawing-model.md`, ADR-0004), from the vessel model alone.
//!
//! # Layout
//! The title block sits in the frame's bottom-right corner; the tables stack above it in
//! columns from the right (revisions and notes nearest the title block, then the nozzle
//! schedule, the design data and the BOM), a new column starting to the left when one is full.
//! The views share one scale, the largest standard scale at which they fit the rest of the
//! frame in one of a few arrangements (side by side, or two rows).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{FRAME_MARGIN, P2, Sheet, SheetSize, Table, TitleBlock, View, ViewKind, bounds};
use crate::plot::{scale_text, text_width, title_block_rect};
use crate::vessel_views::{self, VesselModel};
use crate::{DrawingError, Result, dims, tables};

/// The standard scales tried, largest first (1:n).
pub const SCALES: [f64; 20] =
    [5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 50.0, 60.0, 75.0, 100.0, 125.0, 150.0, 200.0, 250.0, 300.0, 400.0, 500.0, 750.0, 1000.0];

/// What to put on the sheet (`drawing.generate_ga`'s parameters).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GaOptions {
    pub size: SheetSize,
    /// `elevation`, `plan`, `section` (in that order on the sheet).
    pub views: Vec<ViewKind>,
    /// `design_data`, `nozzle_schedule`, `bom`, `notes`, `revisions`.
    pub tables: Vec<crate::model::TableKind>,
    /// A title block template (SVG subset); empty: the OET default.
    pub template: String,
    /// Force a scale (sheet mm per model mm, e.g. 0.04 for 1:25) instead of fitting one.
    pub scale: Option<f64>,
    /// Title block fields laid over the ones read from the document.
    pub title_block: Option<TitleBlock>,
}

impl Default for GaOptions {
    fn default() -> Self {
        use crate::model::TableKind as T;
        GaOptions {
            size: SheetSize::A1,
            views: vec![ViewKind::Elevation, ViewKind::Section, ViewKind::Plan],
            tables: vec![T::DesignData, T::NozzleSchedule, T::Bom, T::Notes, T::Revisions],
            template: String::new(),
            scale: None,
            title_block: None,
        }
    }
}

/// Gap between views, between tables, and the room a view's title takes below it (mm).
const GAP: f64 = 14.0;
const TABLE_GAP: f64 = 4.0;
const TITLE_ROOM: f64 = 20.0;

/// A view's extent on the sheet at its scale, relative to its origin: edges, labels (their text
/// boxes) and dimension lines (with room for their texts).
pub fn extent(v: &View) -> Option<(P2, P2)> {
    let s = v.scale;
    let mut pts: Vec<P2> = v.edges.iter().flat_map(|e| e.edge.sample(16)).map(|p| p.mul(s)).collect();
    for l in &v.labels {
        let (hw, hh) = (text_width(&l.text, l.height) / 2.0 + 2.0, l.height);
        let c = l.at.mul(s);
        pts.push(c.add(P2::new(hw, hh)));
        pts.push(c.sub(P2::new(hw, hh)));
    }
    for d in &v.dimensions {
        match &d.kind {
            crate::model::DimKind::Linear { a, b, line_at, dir } => {
                let n = dir.perp();
                let k = |p: P2| line_at.sub(p).x * n.x + line_at.sub(p).y * n.y;
                for p in [a, b] {
                    let q = p.add(n.mul(k(*p))).mul(s);
                    let out = n.mul(if k(*p) >= 0.0 { 1.0 } else { -1.0 });
                    pts.push(q.add(out.mul(6.0)));
                    pts.push(q);
                }
                if let Some(t) = d.text_at {
                    let c = t.mul(s);
                    pts.push(c.add(P2::new(4.0, 4.0)));
                    pts.push(c.sub(P2::new(4.0, 4.0)));
                }
            }
            crate::model::DimKind::Angular { vertex, arc_radius, .. } => {
                let r = arc_radius * s + 8.0;
                pts.push(vertex.mul(s).add(P2::new(r, r)));
                pts.push(vertex.mul(s).sub(P2::new(r, r)));
            }
            crate::model::DimKind::Diameter { text_at, .. } | crate::model::DimKind::Radius { text_at, .. } => pts.push(text_at.mul(s)),
        }
    }
    bounds(pts)
}

/// The title block's fields from the document (laid under any the options give).
pub fn title_block(m: &VesselModel, scale: f64, opts: &GaOptions) -> TitleBlock {
    let d = &m.doc;
    let extra = |k: &str| d.extra.get(k).and_then(Value::as_str).map(str::to_string);
    let last = d.revisions.last();
    let mut tb = TitleBlock {
        title: d.title.clone().unwrap_or_else(|| format!("VESSEL {}", d.id)).to_ascii_uppercase(),
        drawing_number: extra("drawing_no").unwrap_or_else(|| format!("{}-GA-001", d.id)),
        project: extra("project").unwrap_or_default().to_ascii_uppercase(),
        organization: extra("organization").unwrap_or_else(|| triya_brand::ORG.to_ascii_uppercase()),
        drawn_by: last.map(|r| r.by.clone()).unwrap_or_default(),
        checked_by: extra("checked_by").unwrap_or_default(),
        date: last.map(|r| r.date.clone()).unwrap_or_default(),
        scale: scale_text(scale),
        sheet_of: "1 OF 1".into(),
        rev: last.map(|r| r.rev.clone()).unwrap_or_default(),
        revisions: Vec::new(),
        extra: Default::default(),
    };
    if let Some(o) = &opts.title_block {
        let set = |dst: &mut String, src: &String| {
            if !src.is_empty() {
                *dst = src.clone();
            }
        };
        set(&mut tb.title, &o.title);
        set(&mut tb.drawing_number, &o.drawing_number);
        set(&mut tb.project, &o.project);
        set(&mut tb.organization, &o.organization);
        set(&mut tb.drawn_by, &o.drawn_by);
        set(&mut tb.checked_by, &o.checked_by);
        set(&mut tb.date, &o.date);
        set(&mut tb.sheet_of, &o.sheet_of);
        set(&mut tb.rev, &o.rev);
        tb.extra = o.extra.clone();
    }
    tb
}

/// The views at `scale`, with their dimensions.
fn views_at(m: &VesselModel, kinds: &[ViewKind], scale: f64) -> Result<Vec<View>> {
    let mut out = Vec::new();
    for k in kinds {
        let v = match k {
            ViewKind::Elevation => {
                let mut v = vessel_views::elevation(m, scale)?;
                dims::elevation(m, &mut v);
                v
            }
            ViewKind::Plan => {
                let mut v = vessel_views::plan(m, scale)?;
                dims::plan(m, &mut v);
                v
            }
            ViewKind::Section => vessel_views::section(m, scale)?,
            ViewKind::Detail => return Err(DrawingError::NotSupportedYet("detail views".into())),
        };
        out.push(v);
    }
    Ok(out)
}

/// The arrangements tried: rows of view indices, top to bottom.
fn arrangements(n: usize) -> Vec<Vec<Vec<usize>>> {
    match n {
        0 => vec![vec![]],
        1 => vec![vec![vec![0]]],
        2 => vec![vec![vec![0, 1]], vec![vec![0], vec![1]]],
        _ => {
            let rest: Vec<usize> = (3..n).collect();
            let mut r1 = vec![0, 1, 2];
            r1.extend(&rest);
            vec![
                vec![r1],
                vec![vec![0, 2], [vec![1], rest.clone()].concat()],
                vec![vec![0, 1], [vec![2], rest.clone()].concat()],
                vec![vec![0], [vec![1, 2], rest].concat()],
            ]
        }
    }
}

/// Place `views` in `area` (lo, hi) by the first arrangement that fits; false when none does.
fn place(views: &mut [View], area: (P2, P2)) -> bool {
    let ext: Vec<Option<(P2, P2)>> = views.iter().map(extent).collect();
    let size = |i: usize| ext.get(i).copied().flatten().map_or((0.0, 0.0), |(lo, hi)| (hi.x - lo.x, hi.y - lo.y + TITLE_ROOM));
    let (w, h) = (area.1.x - area.0.x, area.1.y - area.0.y);
    for arr in arrangements(views.len()) {
        let row_w: Vec<f64> = arr.iter().map(|r| r.iter().map(|i| size(*i).0).sum::<f64>() + GAP * r.len().saturating_sub(1) as f64).collect();
        let row_h: Vec<f64> = arr.iter().map(|r| r.iter().map(|i| size(*i).1).fold(0.0, f64::max)).collect();
        let total_h = row_h.iter().sum::<f64>() + GAP * arr.len().saturating_sub(1) as f64;
        if row_w.iter().all(|rw| *rw <= w) && total_h <= h {
            // Rows from the top; the arrangement centred in the area.
            let mut y_top = area.1.y - (h - total_h) / 2.0;
            for (r, rh) in arr.iter().zip(&row_h) {
                let rw = r.iter().map(|i| size(*i).0).sum::<f64>() + GAP * r.len().saturating_sub(1) as f64;
                let mut x = area.0.x + (w - rw) / 2.0;
                for i in r {
                    if let (Some(v), Some(Some((lo, hi)))) = (views.get_mut(*i), ext.get(*i)) {
                        // Top-aligned in the row.
                        v.origin = P2::new(x - lo.x, y_top - hi.y);
                        x += hi.x - lo.x + GAP;
                    }
                }
                y_top -= rh + GAP;
            }
            return true;
        }
    }
    false
}

/// Stack `tables` in columns from the right edge of the frame, the first above the title block
/// (whose top is at `tb_top` and left edge at `tb_left`); returns the left edge of what they and
/// the title block take.
fn place_tables(tables: &mut [Table], sheet_w: f64, sheet_h: f64, tb_top: f64, tb_left: f64) -> f64 {
    let top = sheet_h - FRAME_MARGIN - 4.0;
    let right = sheet_w - FRAME_MARGIN - 4.0;
    // Bottom-up: (column, y of the table's bottom).
    let mut cols: Vec<Vec<(usize, f64)>> = vec![Vec::new()];
    let mut y = tb_top + TABLE_GAP + 2.0;
    for (i, t) in tables.iter().enumerate() {
        let h = t.height();
        if y + h > top && cols.last().is_some_and(|c| !c.is_empty()) {
            cols.push(Vec::new());
            y = FRAME_MARGIN + 4.0;
        }
        if let Some(c) = cols.last_mut() {
            c.push((i, y));
        }
        y += h + TABLE_GAP;
    }
    let mut col_right = right;
    let mut left = tb_left;
    for c in &cols {
        let w = c.iter().filter_map(|(i, _)| tables.get(*i)).map(Table::width).fold(0.0, f64::max);
        for (i, y) in c {
            if let Some(t) = tables.get_mut(*i) {
                t.at = P2::new(col_right - t.width(), y + t.height());
            }
        }
        left = left.min(col_right - w);
        col_right -= w + GAP;
    }
    left
}

/// The GA sheet. `bom` is `vessel.bom`'s reply (the BOM table is left out without it).
pub fn general_arrangement(m: &VesselModel, bom: Option<&Value>, opts: &GaOptions) -> Result<Sheet> {
    use crate::model::TableKind as T;
    let mut sheet = Sheet::new(opts.size);
    sheet.template = opts.template.clone();
    let (w, h) = opts.size.mm();
    let (tb_corner, _, _) = title_block_rect(&sheet)?;
    // Bottom-up next to the title block: revisions, notes, nozzle schedule, design data, BOM.
    let order = [T::Revisions, T::Notes, T::NozzleSchedule, T::DesignData, T::Bom];
    let mut tables: Vec<Table> = Vec::new();
    for k in order {
        if !opts.tables.contains(&k) {
            continue;
        }
        let t = match k {
            T::Revisions => tables::revisions(m),
            T::Notes => tables::notes(m),
            T::NozzleSchedule if !m.nozzles.is_empty() => tables::nozzle_schedule(m),
            T::DesignData => tables::design_data(m, bom),
            T::Bom => match bom {
                Some(b) => tables::bom(m, b),
                None => continue,
            },
            _ => continue,
        };
        tables.push(t);
    }
    for t in &mut tables {
        t.row_height = 5.0;
    }
    let left = place_tables(&mut tables, w, h, tb_corner.y, tb_corner.x);
    sheet.tables = tables;
    let area = (P2::new(FRAME_MARGIN + 8.0, FRAME_MARGIN + 8.0 + TITLE_ROOM), P2::new(left - GAP, h - FRAME_MARGIN - 8.0));
    if area.1.x - area.0.x < 50.0 {
        return Err(DrawingError::Invalid(format!("the tables leave no room for the views on {}: choose a larger sheet", opts.size.name())));
    }
    let candidates: Vec<f64> = match opts.scale {
        Some(s) if s.is_finite() && s > 0.0 && s <= 100.0 => vec![s],
        Some(s) => return Err(DrawingError::Invalid(format!("`scale` must be in (0, 100] (got {s})"))),
        None => SCALES.iter().map(|d| 1.0 / d).collect(),
    };
    let mut placed: Option<(f64, Vec<View>)> = None;
    for s in &candidates {
        let mut views = views_at(m, &opts.views, *s)?;
        if place(&mut views, area) || opts.scale.is_some() {
            if opts.scale.is_some() && views.iter().all(|v| v.origin == P2::default()) {
                // Forced and too big: put the views side by side from the top-left anyway.
                let mut x = area.0.x;
                for v in views.iter_mut() {
                    if let Some((lo, hi)) = extent(v) {
                        v.origin = P2::new(x - lo.x, area.1.y - hi.y);
                        x += hi.x - lo.x + GAP;
                    }
                }
            }
            placed = Some((*s, views));
            break;
        }
    }
    let (scale, views) = placed.ok_or_else(|| DrawingError::Invalid(format!("the vessel does not fit {} at 1:1000", opts.size.name())))?;
    sheet.views = views;
    sheet.title_block = title_block(m, scale, opts);
    sheet.title_block.revisions = m
        .doc
        .revisions
        .iter()
        .map(|r| crate::model::Revision { rev: r.rev.clone(), date: r.date.clone(), description: r.desc.clone(), by: r.by.clone() })
        .collect();
    Ok(sheet)
}
