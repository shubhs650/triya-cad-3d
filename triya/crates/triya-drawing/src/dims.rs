// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Rule-based dimensions and notes (`docs/drawing-model.md` §3), driven from the resolved
//! stations and attachments rather than detected on the geometry, each recording the vessel
//! field(s) it shows (`Dimension::source`, e.g. `E2.length_mm`, `N1.axial.mm`).
//!
//! In the elevation, rows of chained dimensions run along the axis on one side of the vessel
//! (left of a vertical vessel, above a horizontal one), outward from the outline:
//! 1. course lengths and head depths (outer crown to TL, TL to seam, seam to seam);
//! 2. nozzle and attachment centrelines chained from the datum, each with its mark;
//! 3. support base to the first TL, then TL to TL;
//! 4. the overall length.
//!
//! Nozzle projections are dimensioned along each nozzle (`1100 FROM CL`), saddles from the left
//! TL below a horizontal vessel, a skirt's base ring across its base, legs from their foot to the
//! bottom TL. Each element gets a note (`1600 ID x 12 THK`). In the plan, every nozzle's azimuth
//! is an angular dimension from the 0° reference (in the document's sense), hillside offsets a
//! linear one. A text that would overlap its neighbour in the same row moves out by one lane.

use serde_json::Value;
use triya_vessel::eval::{HeadShape, HeadSide, Profile};
use triya_vessel::model::{AngleSense, AttachmentKind, Frame, ProjectionFrom};
use triya_vessel::nozzle::{NozzleGeom, add, azimuth, dot, mul};

use crate::fmt::{deg, mm, num};
use crate::model::{DimKind, Dimension, Label, LabelStyle, P2, TEXT_H, View};
use crate::parts;
use crate::plot::text_width;
use crate::proj::{NoOcclusion, Wire};
use crate::vessel_views::{VesselModel, plan_dir, sheet_mm};

/// Distance from the outline to the first row, and between rows (sheet mm).
pub const FIRST_ROW: f64 = 14.0;
pub const ROW: f64 = 12.0;
/// How far a nudged text moves out (sheet mm).
const LANE: f64 = TEXT_H + 1.5;

/// Does `path` (dotted) exist under the element or attachment `id` in the document's JSON?
pub fn field_exists(doc: &Value, field: &str) -> bool {
    let mut parts = field.split('.');
    let Some(id) = parts.next() else { return false };
    let holder = ["elements", "attachments"]
        .iter()
        .filter_map(|k| doc.get(*k).and_then(Value::as_array))
        .flatten()
        .find(|e| e.get("id").and_then(Value::as_str) == Some(id));
    let Some(mut v) = holder else { return false };
    for p in parts {
        match v.get(p) {
            Some(x) => v = x,
            None => return false,
        }
    }
    true
}

/// The fields of `candidates` the document has, comma-separated.
fn source(doc: &Value, candidates: &[String]) -> String {
    let mut out: Vec<&str> = Vec::new();
    for c in candidates {
        if field_exists(doc, c) && !out.contains(&c.as_str()) {
            out.push(c);
        }
    }
    out.join(",")
}

/// One dimension along the axis in a row: from axial `z0` to `z1`.
struct AxialDim {
    z0: f64,
    z1: f64,
    text: String,
    source: String,
}

/// Lays dimensions out in the elevation's side rows.
struct Rows<'a> {
    m: &'a VesselModel,
    scale: f64,
    /// −1: rows to the left / below the axis; +1: right / above.
    side: f64,
    /// The outline's lateral extent on that side (view mm, signed).
    base: f64,
}

impl Rows<'_> {
    fn lat(&self, row: usize) -> f64 {
        self.base + self.side * sheet_mm(FIRST_ROW + ROW * row as f64, self.scale)
    }

    /// Where an extension line starts: the outline at `z` on the rows' side.
    fn on_outline(&self, z: f64) -> P2 {
        let r = self.m.outside_at(z).unwrap_or(0.0);
        self.m.side_pt(r, z, self.side)
    }

    fn point(&self, lat: f64, z: f64) -> P2 {
        if self.m.vertical() { P2::new(lat, z) } else { P2::new(z, lat) }
    }

    /// Chained dimensions in `row`; a text that would overlap the previous one moves out.
    fn place(&self, view: &mut View, row: usize, dims: &[AxialDim]) {
        let lat = self.lat(row);
        let dir = self.m.axial_dir();
        // The far edge of the text in each lane so far (axial, view mm).
        let mut lanes: [f64; 2] = [f64::NEG_INFINITY; 2];
        let mut sorted: Vec<&AxialDim> = dims.iter().filter(|d| (d.z1 - d.z0).abs() > 0.5).collect();
        sorted.sort_by(|a, b| a.z0.min(a.z1).total_cmp(&b.z0.min(b.z1)));
        for d in sorted {
            let (lo, hi) = (d.z0.min(d.z1), d.z0.max(d.z1));
            let half = sheet_mm(text_width(&d.text, TEXT_H) / 2.0 + 1.0, self.scale);
            let mid = (lo + hi) / 2.0;
            let lane = if mid - half > lanes[0] { 0 } else { 1 };
            if let Some(l) = lanes.get_mut(lane) {
                *l = mid + half;
            }
            let mut dim =
                Dimension::linear(self.on_outline(d.z0), self.on_outline(d.z1), self.point(lat, d.z0), dir, d.text.clone(), d.source.clone());
            if lane > 0 {
                // Text above the line reads outward: one lane further out.
                let out = sheet_mm(1.0 + LANE * lane as f64, self.scale) * self.side;
                dim.text_at = Some(self.point(lat + out, mid));
            }
            view.dimensions.push(dim);
        }
    }
}

fn elements_source(m: &VesselModel, doc: &Value, fields: &[&str]) -> String {
    let c: Vec<String> = m.r.stations.iter().filter(|s| s.envelope).flat_map(|s| fields.iter().map(move |f| format!("{}.{f}", s.id))).collect();
    source(doc, &c)
}

/// A head's outer crown (axial).
fn crown_outer(st: &triya_vessel::eval::Station) -> Option<f64> {
    match st.profile {
        Profile::Head { side: HeadSide::CrownAtStart, .. } => Some(st.z_from - st.thickness),
        Profile::Head { side: HeadSide::CrownAtEnd, .. } => Some(st.z_to + st.thickness),
        _ => None,
    }
}

/// The elevation's dimensions and notes.
pub fn elevation(m: &VesselModel, view: &mut View) {
    let scale = view.scale;
    let doc = m.doc.to_value().unwrap_or(Value::Null);
    let Some((lo, hi)) = view.edge_bounds() else { return };
    let side = if m.vertical() { -1.0 } else { 1.0 };
    let base = if m.vertical() { lo.x } else { hi.y };
    let rows = Rows { m, scale, side, base };

    // Row 1: elements.
    let mut row1 = Vec::new();
    for st in m.r.stations.iter().filter(|s| s.envelope) {
        let id = &st.id;
        match st.profile {
            Profile::Head { side: hs, sf, .. } => {
                let (Some(tl), Some(crown)) = (st.tl, crown_outer(st)) else { continue };
                let seam = if hs == HeadSide::CrownAtStart { st.z_to } else { st.z_from };
                row1.push(AxialDim {
                    z0: crown,
                    z1: tl,
                    text: mm((tl - crown).abs()),
                    source: source(&doc, &[format!("{id}.preset"), format!("{id}.thickness_mm"), format!("{id}.diameter_mm")]),
                });
                if sf > 0.5 {
                    row1.push(AxialDim { z0: tl, z1: seam, text: mm(sf), source: source(&doc, &[format!("{id}.straight_flange_mm")]) });
                }
            }
            _ => row1.push(AxialDim { z0: st.z_from, z1: st.z_to, text: mm(st.length()), source: source(&doc, &[format!("{id}.length_mm")]) }),
        }
    }
    rows.place(view, 0, &row1);

    // Row 2: attachments chained from the datum.
    let mut marks: Vec<(f64, Vec<String>)> = Vec::new();
    let mut add_mark = |z: f64, id: &str| {
        if !z.is_finite() {
            return;
        }
        match marks.iter_mut().find(|(x, _)| (x - z).abs() < 0.5) {
            Some((_, ids)) => ids.push(id.to_string()),
            None => marks.push((z, vec![id.to_string()])),
        }
    };
    for g in &m.nozzles {
        if let Some(z) = m.nozzle_z(g) {
            add_mark(z, &g.id);
        }
    }
    for a in &m.doc.attachments {
        let positioned = matches!(
            a.kind,
            AttachmentKind::StiffeningRing { .. }
                | AttachmentKind::LiftingLug { .. }
                | AttachmentKind::Bracket { .. }
                | AttachmentKind::Saddle { .. }
        );
        if !positioned || matches!(a.kind, AttachmentKind::Saddle { .. }) && !m.vertical() {
            continue;
        }
        if let Some(host) = m.r.station(&a.host)
            && host.kind != "head"
        {
            add_mark(triya_vessel::support::axial_z(a, host), &a.id);
        }
    }
    marks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut row2 = Vec::new();
    let (mut prev_z, mut prev_ids): (f64, Vec<String>) = (0.0, Vec::new());
    for (z, ids) in &marks {
        let mut c: Vec<String> = prev_ids.iter().chain(ids.iter()).map(|i| format!("{i}.axial.mm")).collect();
        c.dedup();
        row2.push(AxialDim { z0: prev_z, z1: *z, text: mm(z - prev_z), source: source(&doc, &c) });
        let at = rows.point(rows.lat(1) + side * sheet_mm(4.0, scale), *z);
        let text = ids.join(", ");
        let shift =
            if m.vertical() { P2::new(side * sheet_mm(text_width(&text, 2.5) / 2.0, scale), 0.0) } else { P2::new(0.0, sheet_mm(2.0, scale)) };
        view.labels.push(Label { at: at.add(shift), text, leader: None, style: LabelStyle::Note, height: 2.5, angle: 0.0 });
        (prev_z, prev_ids) = (*z, ids.clone());
    }
    if !row2.is_empty() {
        view.labels.push(Label {
            at: rows.point(rows.lat(1) + side * sheet_mm(4.0, scale), 0.0).add(if m.vertical() {
                P2::new(0.0, -sheet_mm(4.0, scale))
            } else {
                P2::new(-sheet_mm(4.0, scale), 0.0)
            }),
            text: format!("DATUM ({})", m.datum_name()),
            leader: None,
            style: LabelStyle::Note,
            height: 2.5,
            angle: 0.0,
        });
    }
    rows.place(view, 1, &row2);

    // Row 3: the support's base to the first TL, then TL to TL; row 4: overall.
    let mut row3 = Vec::new();
    let tl_src = elements_source(m, &doc, &["length_mm", "straight_flange_mm"]);
    if let (Some(b), Some(t)) = (m.r.tl_bottom, m.r.tl_top) {
        row3.push(AxialDim { z0: b, z1: t, text: format!("{} TL-TL", mm(t - b)), source: tl_src.clone() });
    }
    let skirt = m.r.stations.iter().find(|s| s.kind == "skirt");
    if let (Some(sk), Some(b)) = (skirt, m.r.tl_bottom) {
        row3.push(AxialDim {
            z0: sk.z_from,
            z1: b,
            text: mm(b - sk.z_from),
            source: source(&doc, &[format!("{}.height_mm", sk.id), format!("{}.top_offset_mm", sk.id)]),
        });
    }
    // Legs: from the foot to the bottom TL.
    if let (Some(foot), Some(b), Some(g)) = (leg_foot(m), m.r.tl_bottom, m.supports.iter().find(|g| g.kind == "leg"))
        && foot < b - 1.0
    {
        row3.push(AxialDim { z0: foot, z1: b, text: mm(b - foot), source: source(&doc, &[format!("{}.length_mm", g.id)]) });
    }
    rows.place(view, 2, &row3);
    // Overall: the lowest support foot (skirt base, legs) or outer crown to the far outer crown.
    let first = m.r.stations.iter().find(|s| s.envelope);
    let last = m.r.stations.iter().rev().find(|s| s.envelope);
    let mut oal_lo = first.and_then(crown_outer).unwrap_or(m.r.z_min).min(m.r.z_min);
    let oal_hi = last.and_then(crown_outer).unwrap_or(m.r.z_max).max(m.r.z_max);
    if let Some(f) = leg_foot(m) {
        oal_lo = oal_lo.min(f);
    }
    rows.place(
        view,
        3,
        &[AxialDim {
            z0: oal_lo,
            z1: oal_hi,
            text: format!("{} OVERALL", mm(oal_hi - oal_lo)),
            source: elements_source(m, &doc, &["length_mm", "straight_flange_mm", "thickness_mm", "height_mm"]),
        }],
    );

    nozzle_projections(m, view, &doc);
    if !m.vertical() {
        saddles(m, view, &doc, lo.y);
    }
    if let Some(sk) = skirt
        && let triya_vessel::eval::Detail::BaseRing { od, .. } = sk.detail
    {
        let z = sk.z_from;
        let below = z - sheet_mm(FIRST_ROW, scale);
        view.dimensions.push(Dimension::linear(
            m.side_pt(od / 2.0, z, -1.0),
            m.side_pt(od / 2.0, z, 1.0),
            m.side_pt(0.0, below, 1.0),
            P2::new(1.0, 0.0),
            format!("{} BASE RING OD", mm(od)),
            source(&doc, &[format!("{}.base_ring.od_mm", sk.id)]),
        ));
    }
    notes(m, view);
}

/// The lowest point of a vertical vessel's legs (axial), from their parts.
fn leg_foot(m: &VesselModel) -> Option<f64> {
    if !m.vertical() {
        return None;
    }
    let mut w = Wire::new(m.elevation_basis(), &NoOcclusion, false);
    for g in m.supports.iter().filter(|g| g.kind == "leg") {
        for p in &g.parts {
            parts::draw_part(&mut w, &p.calls, crate::model::Layer::Visible);
        }
    }
    let foot = w.edges.iter().flat_map(|e| e.edge.sample(4)).map(|p| p.y).fold(f64::INFINITY, f64::min);
    foot.is_finite().then_some(foot)
}

/// Each nozzle's projection, along the nozzle, when the view sees it side-on.
fn nozzle_projections(m: &VesselModel, view: &mut View, doc: &Value) {
    let scale = view.scale;
    let basis = m.elevation_basis();
    for g in &m.nozzles {
        if dot(g.frame.dir, basis.toward).abs() > 0.1 {
            continue;
        }
        let Some(a) = m.attachment(&g.id) else { continue };
        let AttachmentKind::Nozzle { projection, .. } = &a.kind else { continue };
        let s_ref = match projection.from {
            ProjectionFrom::Cl => 0.0,
            ProjectionFrom::Od => g.frame.s_outer,
            ProjectionFrom::Insulation => g.frame.s_outer + m.doc.global.insulation_thickness_mm,
        };
        let from = match projection.from {
            ProjectionFrom::Cl => {
                if m.r.station(&g.host).is_some_and(|h| h.kind == "head") {
                    "TL"
                } else {
                    "CL"
                }
            }
            ProjectionFrom::Od => "OD",
            ProjectionFrom::Insulation => "INSUL.",
        };
        let p0 = basis.p(g.frame.at(s_ref));
        let p1 = basis.p(g.frame.at(g.s_face));
        let Some(u) = p1.sub(p0).unit() else { continue };
        // Offset to the side of +axis (above a horizontal nozzle on a vertical vessel).
        let mut n = u.perp();
        if dot2(n, m.axial_dir()) < 0.0 || dot2(n, m.axial_dir()).abs() < 1e-9 && n.x < 0.0 {
            n = n.mul(-1.0);
        }
        let r = g.flange.map_or(g.neck_od, |f| f.od) / 2.0;
        let line_at = p1.add(n.mul(r + sheet_mm(8.0, scale)));
        view.dimensions.push(Dimension::linear(
            p0,
            p1,
            line_at,
            u,
            format!("{} FROM {from}", mm(projection.mm)),
            source(doc, &[format!("{}.projection.mm", g.id)]),
        ));
    }
}

fn dot2(a: P2, b: P2) -> f64 {
    a.x * b.x + a.y * b.y
}

/// Saddles chained from the left TL, below a horizontal vessel.
fn saddles(m: &VesselModel, view: &mut View, doc: &Value, bottom: f64) {
    let scale = view.scale;
    let Some(tl) = m.r.tl_bottom else { return };
    let mut zs: Vec<(f64, String)> = m
        .doc
        .attachments
        .iter()
        .filter(|a| matches!(a.kind, AttachmentKind::Saddle { .. }))
        .filter_map(|a| Some((triya_vessel::support::axial_z(a, m.r.station(&a.host)?), a.id.clone())))
        .collect();
    zs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let lat = bottom - sheet_mm(FIRST_ROW, scale);
    let mut prev = (tl, String::new());
    for (z, id) in zs {
        let c: Vec<String> = [prev.1.clone(), id.clone()].iter().filter(|i| !i.is_empty()).map(|i| format!("{i}.axial.mm")).collect();
        let top = |z: f64| P2::new(z, bottom);
        view.dimensions.push(Dimension::linear(top(prev.0), top(z), P2::new(prev.0, lat), P2::new(1.0, 0.0), mm(z - prev.0), source(doc, &c)));
        prev = (z, id);
    }
}

/// A note per element: `1600 ID x 12 THK`, the head's type, a cone's angle.
fn notes(m: &VesselModel, view: &mut View) {
    for st in &m.r.stations {
        let t = num(st.thickness);
        let (text, z) = match (st.kind, st.profile) {
            ("cylinder", _) => (format!("{} ID x {t} THK", mm(2.0 * st.r_from)), (st.z_from + st.z_to) / 2.0),
            ("skirt", _) => (format!("SKIRT {} ID x {t} THK", mm(2.0 * st.r_from)), (st.z_from + st.z_to) / 2.0),
            ("cone", _) => (
                format!("CONE {}/{} ID x {t} THK, {} HALF APEX", mm(2.0 * st.r_from), mm(2.0 * st.r_to), deg(st.half_apex_deg.unwrap_or(0.0))),
                (st.z_from + st.z_to) / 2.0,
            ),
            ("head", Profile::Head { side, sf, dish, shape }) => {
                let r = st.r_from.max(st.r_to);
                let kind = match shape {
                    HeadShape::Torispherical { crown_r, knuckle_r } => format!("TORISPH. HEAD L {} r {}", mm(crown_r), mm(knuckle_r)),
                    HeadShape::Ellipsoidal if (dish - r).abs() < 1e-6 => "HEMISPH. HEAD".to_string(),
                    HeadShape::Ellipsoidal => "2:1 ELLIPS. HEAD".to_string(),
                };
                let tl = st.tl.unwrap_or(st.z_from);
                let z = if side == HeadSide::CrownAtStart { tl - dish * 0.45 } else { tl + dish * 0.45 };
                let _ = sf;
                (format!("{kind} {t} THK"), z)
            }
            ("body_flange", _) => (format!("BODY FLANGE {} ID", mm(2.0 * st.r_from)), (st.z_from + st.z_to) / 2.0),
            ("flat_cover", _) => (format!("FLAT COVER {t} THK"), (st.z_from + st.z_to) / 2.0),
            _ => continue,
        };
        // A horizontal vessel's heads are narrow on the sheet: their notes run across the axis;
        // shell notes sit below the centre line, clear of nozzles on it.
        let head = st.kind == "head";
        let (lat, angle) = if m.vertical() {
            (0.0, 0.0)
        } else if head {
            (0.0, std::f64::consts::FRAC_PI_2)
        } else {
            (0.45 * st.r_from, 0.0)
        };
        view.labels.push(Label { at: m.side_pt(lat, z, -1.0), text, leader: None, style: LabelStyle::Note, height: 2.5, angle });
    }
}

/// The plan's azimuth dimensions (from the 0° reference, in the document's sense) and hillside
/// offsets.
pub fn plan(m: &VesselModel, view: &mut View) {
    let scale = view.scale;
    let doc = m.doc.to_value().unwrap_or(Value::Null);
    let gl = &m.doc.global;
    let a0 = {
        let z = plan_dir(m, 0.0);
        z.y.atan2(z.x)
    };
    let r_shell = m.r.stations.iter().filter(|s| s.envelope && s.kind != "head").map(|s| s.r_from.min(s.r_to)).fold(f64::INFINITY, f64::min);
    let r_shell = if r_shell.is_finite() { r_shell } else { m.max_radius() };
    let mut angles: Vec<(f64, Vec<String>)> = Vec::new();
    for g in &m.nozzles {
        let Some(a) = m.attachment(&g.id) else { continue };
        let ang = a.angle_deg.rem_euclid(360.0);
        if ang.abs() < 1e-6 {
            continue;
        }
        match angles.iter_mut().find(|(x, _)| (x - ang).abs() < 1e-6) {
            Some((_, ids)) => ids.push(g.id.clone()),
            None => angles.push((ang, vec![g.id.clone()])),
        }
    }
    angles.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = angles.len().max(1) as f64;
    for (i, (ang, ids)) in angles.iter().enumerate() {
        let an = {
            let d = plan_dir(m, *ang);
            d.y.atan2(d.x)
        };
        let (a, b) = if gl.angle_sense == AngleSense::CounterclockwiseFromAbove { (a0, an) } else { (an, a0) };
        let r = r_shell * (0.3 + 0.6 * (i as f64 + 0.5) / n);
        let src: Vec<String> = ids.iter().map(|id| format!("{id}.angle_deg")).collect();
        view.dimensions.push(Dimension {
            kind: DimKind::Angular { vertex: P2::default(), a, b, arc_radius: r },
            text: deg(*ang),
            source: source(&doc, &src),
            text_at: None,
        });
    }
    let basis = m.plan_basis();
    for g in &m.nozzles {
        let Some(a) = m.attachment(&g.id) else { continue };
        let (offset_field, has) = match a.frame {
            Frame::Hillside { .. } => (vec![format!("{}.frame.offset_mm", a.id)], true),
            Frame::Tangential { offset_mm: Some(_) } => (vec![format!("{}.frame.offset_mm", a.id)], true),
            Frame::Tangential { offset_mm: None } => (vec![format!("{}.frame.type", a.id), format!("{}.nps", a.id)], true),
            _ => (Vec::new(), false),
        };
        if !has {
            continue;
        }
        let p_axis = basis.p(g.frame.origin);
        // The radial through the vessel axis parallel to the nozzle.
        let u = azimuth(gl, a.angle_deg);
        let foot = basis.p(mul(u, dot(g.frame.origin, u)));
        let Some(dir) = p_axis.sub(foot).unit() else { continue };
        let face = basis.p(add(g.frame.origin, mul(g.frame.dir, g.s_face)));
        let along = basis.d(g.frame.dir).unit().unwrap_or(P2::new(1.0, 0.0));
        let line_at = face.add(along.mul(sheet_mm(16.0, scale)));
        view.dimensions.push(Dimension::linear(
            foot.add(along.mul(face.sub(p_axis).len())),
            face,
            line_at,
            dir,
            mm(p_axis.dist(foot)),
            source(&doc, &offset_field),
        ));
    }
}

/// The datum elevation of nozzle `g` for the schedule (`None` for head frames).
pub fn nozzle_elevation(m: &VesselModel, g: &NozzleGeom) -> Option<f64> {
    m.nozzle_z(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fields_resolve_by_id_and_dotted_path() {
        let doc = json!({"elements": [{"id": "E1", "length_mm": 10}], "attachments": [{"id": "N1", "axial": {"mm": 5}}]});
        assert!(field_exists(&doc, "E1.length_mm"));
        assert!(field_exists(&doc, "N1.axial.mm"));
        assert!(!field_exists(&doc, "N1.axial.ref"));
        assert!(!field_exists(&doc, "N9.axial.mm"));
        assert_eq!(source(&doc, &["E1.length_mm".into(), "E1.nope".into(), "N1.axial.mm".into()]), "E1.length_mm,N1.axial.mm");
    }
}
