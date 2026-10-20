// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The vessel's views, drawn analytically from the model (`docs/drawing-model.md` §2, ADR-0004):
//! no regenerate and no kernel. The shell comes from the same half-profiles the revolve is built
//! from (`triya_vessel::emit::profile`), nozzles from their resolved frames, supports from the
//! commands that build them ([`crate::parts`]).
//!
//! # Projections
//! - **Elevation**: seen from the south looking north (+Y), +X to the right, +Z up; a vertical
//!   vessel stands on the sheet, a horizontal one lies left (datum TL) to right. Edges of nozzles
//!   behind the shell, or inside it, are dashed (`HIDDEN`); supports behind the shell are left out.
//! - **Plan / orientation**: looking along the axis the way the azimuths are counted (down for a
//!   vertical vessel, +X from the left end for a horizontal one), so clockwise on the sheet is
//!   clockwise in `angle_sense` terms; the same convention as the panel's plan
//!   (`triya_ui::vessel::plan::screen_dir`).
//! - **Section A-A**: the plane through the axis at the azimuth with the most nozzles (a nozzle
//!   at `az` and one at `az + 180°` both lie in it): the profile of every element on both sides,
//!   hatched at 45°, with the nozzles in the plane cut through their walls.
//!
//! View coordinates are model millimetres in the view's (right, up) frame; the GA layout scales
//! and places them ([`crate::ga`]).

use triya_stdlib::TableSet;
use triya_vessel::emit::{Seg, profile};
use triya_vessel::eval::{Resolved, Station};
use triya_vessel::model::{AngleZero, Attachment, AttachmentKind, Datum, Orientation, VesselDoc};
use triya_vessel::nozzle::{NozzleGeom, V3, add, azimuth, dot, mul, sub, vessel_axis};
use triya_vessel::support::{SupportGeom, outside_radius};

use crate::model::{Edge, HatchRegion, Label, LabelStyle, Layer, LineStyle, P2, StyledEdge, View, ViewKind, angle_pt};
use crate::parts;
use crate::proj::{Basis, NoOcclusion, Occluder, Wire, dedupe};
use crate::{DrawingError, Result};

/// The vessel with everything a drawing reads: presets applied, stations resolved, every
/// nozzle and support resolved (those that fail are listed in `warnings` and left out).
pub struct VesselModel {
    pub doc: VesselDoc,
    pub r: Resolved,
    pub tables: TableSet,
    pub nozzles: Vec<NozzleGeom>,
    pub supports: Vec<SupportGeom>,
    pub warnings: Vec<String>,
}

impl VesselModel {
    /// `private` is how many support tables come before the shipped ones
    /// (`triya_vessel::presets::private_support_tables`).
    pub fn new(doc: &VesselDoc, tables: TableSet, private: usize) -> Result<VesselModel> {
        let mut doc = doc.clone();
        let (_, mut warnings) = triya_vessel::presets::apply(&mut doc, &tables, private)?;
        let r = triya_vessel::eval::resolve_with(&doc, &tables)?;
        warnings.extend(r.warnings.iter().cloned());
        let mut nozzles = Vec::new();
        let mut supports = Vec::new();
        for a in &doc.attachments {
            if matches!(a.kind, AttachmentKind::Nozzle { .. }) {
                match triya_vessel::nozzle::resolve(&doc, &r, a, &tables) {
                    Ok(g) => nozzles.push(g),
                    Err(e) => warnings.push(format!("nozzle `{}` is not drawn: {e}", a.id)),
                }
            } else {
                match triya_vessel::support::resolve(&doc, &r, a, &tables) {
                    Ok(g) => supports.push(g),
                    Err(e) => warnings.push(format!("{} `{}` is not drawn: {e}", a.kind.name(), a.id)),
                }
            }
        }
        Ok(VesselModel { doc, r, tables, nozzles, supports, warnings })
    }

    /// With the shipped tables only.
    pub fn embedded(doc: &VesselDoc) -> Result<VesselModel> {
        let tables = TableSet::embedded().map_err(triya_vessel::VesselError::from)?;
        VesselModel::new(doc, tables, 0)
    }

    pub fn axis(&self) -> V3 {
        vessel_axis(self.doc.global.orientation)
    }

    pub fn vertical(&self) -> bool {
        self.doc.global.orientation == Orientation::Vertical
    }

    pub fn attachment(&self, id: &str) -> Option<&Attachment> {
        self.doc.attachments.iter().find(|a| a.id == id)
    }

    /// The outside radius of whatever is at axial position `z` (shell, heads, skirt).
    pub fn outside_at(&self, z: f64) -> Option<f64> {
        self.r
            .stations
            .iter()
            .filter(|st| z >= st.z_from - 1e-6 && z <= st.z_to + 1e-6)
            .filter_map(|st| outside_radius(st, z.clamp(st.z_from, st.z_to)))
            .fold(None, |acc: Option<f64>, r| Some(acc.map_or(r, |a| a.max(r))))
    }

    /// The largest outside radius of the stack (base rings and body flanges included).
    pub fn max_radius(&self) -> f64 {
        self.r
            .stations
            .iter()
            .map(|st| match st.detail {
                triya_vessel::eval::Detail::BaseRing { od, .. } => od / 2.0,
                triya_vessel::eval::Detail::Flange(f) => f.od / 2.0,
                triya_vessel::eval::Detail::None => st.r_from.max(st.r_to) + st.thickness,
            })
            .fold(0.0, f64::max)
    }

    /// The elevation's (and section's) frame for a cut through azimuth direction `u` (unit,
    /// perpendicular to the axis).
    pub fn side_basis(&self, u: V3) -> Basis {
        if self.vertical() { Basis::new(u, [0.0, 0.0, 1.0]) } else { Basis::new([1.0, 0.0, 0.0], u) }
    }

    /// The elevation's frame.
    pub fn elevation_basis(&self) -> Basis {
        if self.vertical() { self.side_basis([1.0, 0.0, 0.0]) } else { self.side_basis([0.0, 0.0, 1.0]) }
    }

    /// The plan (orientation) view's frame.
    pub fn plan_basis(&self) -> Basis {
        if self.vertical() { Basis::new([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]) } else { Basis::new([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]) }
    }

    /// A profile point (distance from the axis on side `side` = ±1, axial) in a side view.
    pub fn side_pt(&self, rho: f64, z: f64, side: f64) -> P2 {
        if self.vertical() { P2::new(side * rho, z) } else { P2::new(z, side * rho) }
    }

    /// The axial coordinate's direction in a side view.
    pub fn axial_dir(&self) -> P2 {
        if self.vertical() { P2::new(0.0, 1.0) } else { P2::new(1.0, 0.0) }
    }

    /// The datum's name for notes and dimensions.
    pub fn datum_name(&self) -> &'static str {
        match self.doc.global.datum {
            Datum::BottomTl => "BOTTOM TL",
            Datum::LeftTl => "LEFT TL",
            Datum::BottomSeam => "BOTTOM SEAM",
            Datum::SkirtBase => "SKIRT BASE",
            Datum::PlantElevation => "GRADE",
        }
    }

    /// Where a nozzle's elevation is read: the attachment's axial position from the datum on a
    /// shell element, `None` for head frames.
    pub fn nozzle_z(&self, g: &NozzleGeom) -> Option<f64> {
        let a = self.attachment(&g.id)?;
        let host = self.r.station(&g.host)?;
        if host.kind == "head" {
            return None;
        }
        Some(triya_vessel::support::axial_z(a, host))
    }
}

/// The shell (and skirt) as the occluder of a side view.
struct ShellOccluder<'a> {
    m: &'a VesselModel,
    basis: Basis,
}

impl Occluder for ShellOccluder<'_> {
    fn hidden(&self, p: V3) -> bool {
        let axis = self.m.axis();
        let z = dot(p, axis);
        let Some(r) = self.m.outside_at(z) else { return false };
        let rv = sub(p, mul(axis, z));
        let rho = dot(rv, rv).sqrt();
        let d = dot(rv, self.basis.toward);
        let lat = (rho * rho - d * d).max(0.0).sqrt();
        lat < r - 0.5 && (rho < r - 0.5 || d > 0.0)
    }
}

/// One profile segment as a view edge, through `map` (a profile point → view); `flip` when the
/// map reverses orientation (arcs then run the other way).
fn seg_edge(seg: &Seg, map: &dyn Fn([f64; 2]) -> P2) -> Edge {
    match *seg {
        Seg::Line(a, b) => Edge::Line { a: map(a), b: map(b) },
        Seg::Arc { c, a, b } => {
            let (c, a, b) = (map(c), map(a), map(b));
            let r = a.dist(c);
            let (ta, tb) = ((a.y - c.y).atan2(a.x - c.x), (b.y - c.y).atan2(b.x - c.x));
            let sweep = (tb - ta).rem_euclid(std::f64::consts::TAU);
            // Profile arcs are under 180°: the short way round.
            if sweep <= std::f64::consts::PI {
                Edge::Arc { center: c, radius: r, start: ta, end: tb }
            } else {
                Edge::Arc { center: c, radius: r, start: tb, end: ta }
            }
        }
        Seg::QuarterEllipse { a, b, apex } => {
            let (a, b, apex) = (map(a), map(b), map(apex));
            let c = a.add(b).sub(apex);
            let (va, vb) = (a.sub(c), b.sub(c));
            let rotation = va.y.atan2(va.x);
            let ccw = va.x * vb.y - va.y * vb.x > 0.0;
            let (start, end) = if ccw { (0.0, std::f64::consts::FRAC_PI_2) } else { (-std::f64::consts::FRAC_PI_2, 0.0) };
            Edge::Ellipse { center: c, rx: va.len(), ry: vb.len(), rotation, start, end }
        }
    }
}

fn seg_ends(seg: &Seg) -> ([f64; 2], [f64; 2]) {
    match *seg {
        Seg::Line(a, b) | Seg::Arc { a, b, .. } | Seg::QuarterEllipse { a, b, .. } => (a, b),
    }
}

fn seg_mid(seg: &Seg) -> [f64; 2] {
    match *seg {
        Seg::Line(a, b) => [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0],
        Seg::Arc { c, a, b } => {
            let r = (a[0] - c[0]).hypot(a[1] - c[1]);
            let (mx, my) = ((a[0] + b[0]) / 2.0 - c[0], (a[1] + b[1]) / 2.0 - c[1]);
            let n = mx.hypot(my).max(1e-12);
            [c[0] + mx / n * r, c[1] + my / n * r]
        }
        Seg::QuarterEllipse { a, b, apex } => {
            // The rational quadratic at t = ½ with weight √2/2.
            let w = std::f64::consts::FRAC_1_SQRT_2;
            let d = 0.5 + w / 2.0;
            [(a[0] * 0.25 + apex[0] * w / 2.0 + b[0] * 0.25) / d, (a[1] * 0.25 + apex[1] * w / 2.0 + b[1] * 0.25) / d]
        }
    }
}

/// The closed profile sampled as a polygon (profile coordinates). Segments are chained by
/// their shared end points (the profiles list some the other way round, which the sketch's
/// point merging does not mind).
pub fn sample_profile(segs: &[Seg]) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    let mut last: Option<[f64; 2]> = None;
    for s in segs {
        let (a, b) = seg_ends(s);
        let near = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) < 1e-6;
        let mut pts: Vec<[f64; 2]> = seg_edge(s, &|p| P2::new(p[0], p[1])).sample(24).iter().map(|p| [p.x, p.y]).collect();
        // Start where the previous segment ended (or at the segment's own start).
        let start = match last {
            Some(l) if near(b, l) && !near(a, l) => b,
            _ => a,
        };
        if pts.first().is_some_and(|p0| !near(*p0, start)) {
            pts.reverse();
        }
        last = pts.last().copied();
        pts.pop();
        out.extend(pts);
    }
    out
}

/// A station's profile, or a warning.
fn station_profile(st: &Station, warnings: &mut Vec<String>) -> Option<Vec<Seg>> {
    match profile(st) {
        Ok(s) => Some(s),
        Err(e) => {
            warnings.push(format!("element `{}` is not drawn: {e}", st.id));
            None
        }
    }
}

/// Model millimetres for `mm` on the sheet at `scale`.
pub fn sheet_mm(mm: f64, scale: f64) -> f64 {
    if scale > 0.0 { mm / scale } else { mm }
}

/// The shell's outline in a side view: the outside of every element's profile on both sides,
/// circumferential seams and end faces as lines across, tangent lines, the axis.
fn shell_outline(m: &VesselModel, view: &mut View, scale: f64, warnings: &mut Vec<String>) {
    let ext = sheet_mm(8.0, scale);
    for st in &m.r.stations {
        let Some(segs) = station_profile(st, warnings) else { continue };
        let flat = st.kind == "flat_cover";
        for seg in &segs {
            let (a, b) = seg_ends(seg);
            if a[0].abs() < 1e-6 && b[0].abs() < 1e-6 {
                continue;
            }
            if (a[1] - b[1]).abs() < 1e-9 && matches!(seg, Seg::Line(..)) {
                let rmax = a[0].max(b[0]);
                let ri = st.radius_at(a[1]).unwrap_or(0.0);
                if flat || rmax > ri + 0.5 * st.thickness {
                    view.line(m.side_pt(rmax, a[1], -1.0), m.side_pt(rmax, a[1], 1.0), Layer::Visible);
                }
                continue;
            }
            let mid = seg_mid(seg);
            let outside = match st.radius_at(mid[1]) {
                Some(ri) => mid[0] > ri + 0.5 * st.thickness,
                None => true,
            };
            if flat || outside {
                for side in [1.0, -1.0] {
                    view.edges.push(StyledEdge::new(seg_edge(seg, &|p| m.side_pt(p[0], p[1], side)), Layer::Visible));
                }
            }
        }
        if let Some(tl) = st.tl {
            let r = st.r_from.max(st.r_to) + st.thickness;
            view.edges.push(StyledEdge {
                edge: Edge::Line { a: m.side_pt(r + ext, tl, -1.0), b: m.side_pt(r + ext, tl, 1.0) },
                layer: Layer::Center,
                style: LineStyle::Continuous,
            });
            let at = m.side_pt(r + ext + sheet_mm(4.0, scale), tl, 1.0);
            view.labels.push(Label { at, text: "TL".into(), leader: None, style: LabelStyle::Note, height: 2.5, angle: 0.0 });
        }
    }
    // The axis.
    view.line(m.side_pt(0.0, m.r.z_min - ext * 2.0, 1.0), m.side_pt(0.0, m.r.z_max + ext * 2.0, 1.0), Layer::Center);
}

/// A nozzle's solids as frusta: neck, pad, flange hub, ring and raised face.
fn draw_nozzle(wire: &mut Wire, g: &NozzleGeom) {
    let (o, d) = (g.frame.origin, g.frame.dir);
    let at = |s: f64| add(o, mul(d, s));
    wire.frustum(at(g.s_start), d, g.s_neck_end - g.s_start, g.neck_od / 2.0, g.neck_od / 2.0, Layer::Visible);
    if let Some(p) = g.pad {
        wire.frustum(at(g.frame.s_outer), d, p.thickness, p.od / 2.0, p.od / 2.0, Layer::Visible);
    }
    if let Some(f) = &g.flange {
        let (face, back) = f.ring(g.s_face);
        let hub_back = f.back(g.s_face);
        if back - hub_back > 1e-6 {
            wire.frustum(at(hub_back), d, back - hub_back, f.hub_weld_d / 2.0, f.hub_od / 2.0, Layer::Visible);
        }
        wire.frustum(at(back), d, face - back, f.od / 2.0, f.od / 2.0, Layer::Visible);
        if f.rf_h > 1e-6 && f.rf_d > 0.0 {
            wire.frustum(at(face), d, f.rf_h, f.rf_d / 2.0, f.rf_d / 2.0, Layer::Visible);
        }
    }
}

/// The nozzle's centre line and mark (a boxed id with a leader to the flange face).
fn nozzle_mark(view: &mut View, basis: &Basis, g: &NozzleGeom, scale: f64) {
    let (o, d) = (g.frame.origin, g.frame.dir);
    let face = basis.p(add(o, mul(d, g.s_face)));
    let d2 = basis.d(d);
    match d2.unit() {
        Some(u) if d2.len() > 0.2 => {
            let from = basis.p(add(o, mul(d, g.frame.s_outer)));
            view.line(from, face.add(u.mul(sheet_mm(4.0, scale))), Layer::Center);
            let at = face.add(u.mul(sheet_mm(9.0, scale)));
            view.labels.push(Label { at, text: g.id.clone(), leader: None, style: LabelStyle::Mark, height: 3.5, angle: 0.0 });
        }
        _ => {
            // End-on: a cross through the face and the mark beside it.
            let r = g.flange.map_or(g.neck_od, |f| f.od) / 2.0 + sheet_mm(2.0, scale);
            view.line(face.sub(P2::new(r, 0.0)), face.add(P2::new(r, 0.0)), Layer::Center);
            view.line(face.sub(P2::new(0.0, r)), face.add(P2::new(0.0, r)), Layer::Center);
            let at = face.add(P2::new(r + sheet_mm(5.0, scale), r + sheet_mm(5.0, scale)));
            view.labels.push(Label { at, text: g.id.clone(), leader: Some(face), style: LabelStyle::Mark, height: 3.5, angle: 0.0 });
        }
    }
}

/// Parts that are not drawn in plan (they would only add circles round the shell).
fn plan_skips(kind: &str) -> bool {
    matches!(kind, "stiffening_ring" | "limpet_coil" | "insulation_ring")
}

/// A limpet coil in a side view: a hatched band on each side of the shell over the coil's
/// height, and a note with its size and pitch.
fn coil_band(m: &VesselModel, view: &mut View, g: &SupportGeom, scale: f64) {
    let Some(a) = m.attachment(&g.id) else { return };
    let AttachmentKind::LimpetCoil { pipe_nps, schedule, pitch_mm, turns, end_axial_mm, half_pipe, .. } = &a.kind else { return };
    let Some(host) = m.r.station(&a.host) else { return };
    let z0 = triya_vessel::support::axial_z(a, host);
    let z1 = match (turns, end_axial_mm) {
        (Some(t), _) => z0 + t * pitch_mm,
        (None, Some(e)) => *e,
        _ => z0,
    };
    let od = m.tables.pipe(pipe_nps, schedule).map_or(60.0, |p| p.od);
    let r0 = host.r_from + host.thickness;
    let r1 = r0 + if *half_pipe { od / 2.0 } else { od };
    if !(z1 > z0 && z0.is_finite() && z1.is_finite()) {
        return;
    }
    for side in [1.0, -1.0] {
        let pts = [m.side_pt(r0, z0, side), m.side_pt(r1, z0, side), m.side_pt(r1, z1, side), m.side_pt(r0, z1, side)];
        view.edges.push(StyledEdge::new(Edge::Polyline { points: pts.to_vec(), closed: true }, Layer::Visible));
        view.hatches.push(HatchRegion { loops: vec![pts.to_vec()], angle: std::f64::consts::FRAC_PI_4, spacing: 2.0 });
    }
    let n = turns.map_or(String::new(), |t| format!(", {} TURNS", crate::fmt::num(t)));
    let text = format!("{} {pipe_nps}\" SCH {schedule} LIMPET COIL{n} @ {} PITCH", g.id, crate::fmt::num(*pitch_mm));
    let at = m.side_pt(r1 + sheet_mm(4.0, scale), (z0 + z1) / 2.0, 1.0);
    view.labels.push(Label {
        at: if m.vertical() { at.add(P2::new(sheet_mm(text_w(&text) / 2.0, scale), 0.0)) } else { at },
        text,
        leader: Some(m.side_pt(r1, (z0 + z1) / 2.0, 1.0)),
        style: LabelStyle::Note,
        height: 2.5,
        angle: 0.0,
    });
}

fn text_w(t: &str) -> f64 {
    crate::plot::text_width(t, 2.5)
}

/// The side elevation at `scale` (sheet mm per model mm: label offsets are sheet distances).
pub fn elevation(m: &VesselModel, scale: f64) -> Result<View> {
    check_scale(scale)?;
    let mut view = View::new("elevation", P2::default(), scale);
    view.kind = ViewKind::Elevation;
    view.title = "ELEVATION".into();
    let mut warnings = Vec::new();
    shell_outline(m, &mut view, scale, &mut warnings);
    let basis = m.elevation_basis();
    let occ = ShellOccluder { m, basis };
    let mut wire = Wire::new(basis, &occ, true);
    for g in &m.nozzles {
        draw_nozzle(&mut wire, g);
    }
    let mut supports = Wire::new(basis, &occ, false);
    for g in &m.supports {
        if g.kind == "limpet_coil" {
            continue;
        }
        for p in &g.parts {
            parts::draw_part(&mut supports, &p.calls, Layer::Visible);
        }
    }
    wire.into_view(&mut view);
    supports.into_view(&mut view);
    for g in &m.supports {
        if g.kind == "limpet_coil" {
            coil_band(m, &mut view, g, scale);
        }
    }
    for g in &m.nozzles {
        nozzle_mark(&mut view, &basis, g, scale);
    }
    support_marks(m, &mut view, &basis, scale, false);
    dedupe(&mut view.edges);
    Ok(view)
}

/// A boxed id beside each support (saddles, legs, brackets, lugs, chairs, clips).
fn support_marks(m: &VesselModel, view: &mut View, basis: &Basis, scale: f64, plan: bool) {
    for g in &m.supports {
        if plan && plan_skips(g.kind) || g.kind == "limpet_coil" {
            continue;
        }
        let Some(first) = g.parts.first() else { continue };
        let Some(p) = parts::anchor(&first.calls) else { continue };
        if !plan {
            let occ = ShellOccluder { m, basis: *basis };
            if occ.hidden(p) {
                continue;
            }
        }
        let at2 = basis.p(p);
        // Outward from the axis in the view.
        let axis2 = basis.p(mul(m.axis(), dot(p, m.axis())));
        let out = at2.sub(axis2).unit().unwrap_or(P2::new(1.0, 0.0));
        let at = at2.add(out.mul(sheet_mm(12.0, scale)));
        view.labels.push(Label { at, text: g.id.clone(), leader: Some(at2), style: LabelStyle::Mark, height: 3.0, angle: 0.0 });
    }
}

/// The nozzle orientation plan (vertical vessel: from above; horizontal: from the left end).
pub fn plan(m: &VesselModel, scale: f64) -> Result<View> {
    check_scale(scale)?;
    let mut view = View::new("plan", P2::default(), scale);
    view.kind = ViewKind::Plan;
    view.title = if m.vertical() { "PLAN (NOZZLE ORIENTATION)".into() } else { "END VIEW (NOZZLE ORIENTATION)".into() };
    // Every distinct outside diameter (and the base ring) as a circle.
    let mut radii: Vec<(f64, Layer)> = Vec::new();
    for st in &m.r.stations {
        let mut add_r = |r: f64, layer: Layer| {
            if r > 0.0 && !radii.iter().any(|(x, _)| (x - r).abs() < 0.5) {
                radii.push((r, layer));
            }
        };
        match st.detail {
            triya_vessel::eval::Detail::BaseRing { od, .. } => add_r(od / 2.0, Layer::Visible),
            triya_vessel::eval::Detail::Flange(f) => add_r(f.od / 2.0, Layer::Visible),
            triya_vessel::eval::Detail::None => {}
        }
        if st.kind != "head" {
            add_r(st.r_from + st.thickness, Layer::Visible);
            add_r(st.r_to + st.thickness, Layer::Visible);
        }
    }
    for (r, layer) in &radii {
        view.edge(Edge::Circle { center: P2::default(), radius: *r }, *layer);
    }
    let basis = m.plan_basis();
    let r_max = m.max_radius();
    let reach = m
        .nozzles
        .iter()
        .map(|g| {
            let f = basis.p(g.frame.at(g.s_face));
            f.len() + g.flange.map_or(g.neck_od, |f| f.od) / 2.0
        })
        .fold(r_max, f64::max);
    let cl = reach + sheet_mm(6.0, scale);
    view.line(P2::new(-cl, 0.0), P2::new(cl, 0.0), Layer::Center);
    view.line(P2::new(0.0, -cl), P2::new(0.0, cl), Layer::Center);
    let mut wire = Wire::new(basis, &NoOcclusion, false);
    for g in &m.nozzles {
        draw_nozzle(&mut wire, g);
    }
    for g in &m.supports {
        if plan_skips(g.kind) {
            continue;
        }
        for p in &g.parts {
            parts::draw_part(&mut wire, &p.calls, Layer::Visible);
        }
    }
    wire.into_view(&mut view);
    for g in &m.nozzles {
        nozzle_mark(&mut view, &basis, g, scale);
    }
    support_marks(m, &mut view, &basis, scale, true);
    // The 0° reference and, for plant north, a north arrow.
    let gl = &m.doc.global;
    let zero = basis.d(azimuth(gl, 0.0)).unit().unwrap_or(P2::new(0.0, 1.0));
    view.labels.push(Label::note(zero.mul(cl + sheet_mm(5.0, scale)), "0°"));
    if m.vertical() && gl.angle_zero == AngleZero::PlantNorth {
        let base = P2::new(cl * 0.85, cl * 0.85);
        let tip = base.add(P2::new(0.0, sheet_mm(15.0, scale)));
        view.line(base, tip, Layer::Visible);
        let w = sheet_mm(2.0, scale);
        view.edge(Edge::Polyline { points: vec![tip, tip.add(P2::new(-w, -2.0 * w)), tip.add(P2::new(w, -2.0 * w))], closed: true }, Layer::Visible);
        view.labels.push(Label::note(tip.add(P2::new(0.0, sheet_mm(4.0, scale))), "PLANT N"));
    }
    // The section's cutting plane.
    let (az, _) = section_azimuth(m);
    let u = basis.d(azimuth(gl, az)).unit().unwrap_or(P2::new(1.0, 0.0));
    let reach_a = cl + sheet_mm(2.0, scale);
    for s in [1.0, -1.0] {
        let end = u.mul(s * reach_a);
        view.edges.push(StyledEdge {
            edge: Edge::Line { a: end, b: u.mul(s * (reach_a - sheet_mm(10.0, scale))) },
            layer: Layer::Visible,
            style: LineStyle::Continuous,
        });
        view.labels.push(Label {
            at: u.mul(s * (reach_a + sheet_mm(4.0, scale))),
            text: "A".into(),
            leader: None,
            style: LabelStyle::Note,
            height: 5.0,
            angle: 0.0,
        });
    }
    dedupe(&mut view.edges);
    Ok(view)
}

/// The section's azimuth (degrees): the one with the most nozzles in its plane (ties: the
/// smallest angle), and how many.
pub fn section_azimuth(m: &VesselModel) -> (f64, usize) {
    let mut best = (0.0, 0usize);
    let mut cands: Vec<f64> =
        m.doc.attachments.iter().filter(|a| matches!(a.kind, AttachmentKind::Nozzle { .. })).map(|a| a.angle_deg.rem_euclid(180.0)).collect();
    cands.sort_by(f64::total_cmp);
    cands.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    for c in cands {
        let n = m.nozzles.iter().filter(|g| in_plane(m, g, c)).count();
        if n > best.1 {
            best = (c, n);
        }
    }
    best
}

/// Does nozzle `g` lie in the plane through the axis at azimuth `az`?
fn in_plane(m: &VesselModel, g: &NozzleGeom, az: f64) -> bool {
    let basis = m.side_basis(azimuth(&m.doc.global, az));
    dot(g.frame.dir, basis.toward).abs() < 0.02 && dot(g.frame.origin, basis.toward).abs() < 1.0
}

/// A rectangle in a section along a nozzle's axis: from `s0` to `s1`, between `w0` and `w1`
/// off the axis (view coordinates), on the side `side` = ±1.
fn strip(basis: &Basis, g: &NozzleGeom, s0: f64, s1: f64, w0: f64, w1: f64, side: f64) -> Option<Vec<P2>> {
    let (o, d) = (g.frame.origin, g.frame.dir);
    let a0 = basis.p(add(o, mul(d, s0)));
    let a1 = basis.p(add(o, mul(d, s1)));
    let n = a1.sub(a0).unit()?.perp().mul(side);
    Some(vec![a0.add(n.mul(w0)), a1.add(n.mul(w0)), a1.add(n.mul(w1)), a0.add(n.mul(w1))])
}

/// Section A-A through the axis.
pub fn section(m: &VesselModel, scale: f64) -> Result<View> {
    check_scale(scale)?;
    let (az, _) = section_azimuth(m);
    let u = azimuth(&m.doc.global, az);
    let basis = m.side_basis(u);
    let mut view = View::new("section", P2::default(), scale);
    view.kind = ViewKind::Section;
    view.title = "SECTION A-A".into();
    let mut warnings = Vec::new();
    let in_plane_nozzles: Vec<&NozzleGeom> = m.nozzles.iter().filter(|g| in_plane(m, g, az)).collect();
    for st in &m.r.stations {
        let Some(segs) = station_profile(st, &mut warnings) else { continue };
        for seg in &segs {
            let (a, b) = seg_ends(seg);
            if a[0].abs() < 1e-6 && b[0].abs() < 1e-6 {
                continue;
            }
            for side in [1.0, -1.0] {
                view.edges.push(StyledEdge::new(seg_edge(seg, &|p| m.side_pt(p[0], p[1], side)), Layer::Visible));
            }
        }
        let poly = sample_profile(&segs);
        let mut loops: Vec<Vec<P2>> = [1.0, -1.0].iter().map(|s| poly.iter().map(|p| m.side_pt(p[0], p[1], *s)).collect()).collect();
        // The bore of a nozzle through a cylinder's wall is not metal.
        if st.kind == "cylinder" {
            for g in in_plane_nozzles.iter().filter(|g| g.host == st.id) {
                let r = g.neck_id() / 2.0;
                loops.extend(strip(&basis, g, g.frame.s_inner, g.frame.s_outer, -r, r, 1.0));
            }
        }
        view.hatches.push(HatchRegion { loops, angle: std::f64::consts::FRAC_PI_4, spacing: 2.5 });
    }
    for g in &in_plane_nozzles {
        let (ro, ri) = (g.neck_od / 2.0, g.neck_id() / 2.0);
        let mut rects: Vec<Vec<P2>> = Vec::new();
        for side in [1.0, -1.0] {
            rects.extend(strip(&basis, g, g.s_start, g.s_neck_end, ri, ro, side));
            if let Some(f) = &g.flange {
                let (face, back) = f.ring(g.s_face);
                let hub_back = f.back(g.s_face);
                rects.extend(strip(&basis, g, back, face, ri, f.od / 2.0, side));
                if back - hub_back > 1e-6 {
                    rects.extend(strip(&basis, g, hub_back, back, ri, f.hub_od / 2.0, side));
                }
                if f.rf_h > 1e-6 {
                    rects.extend(strip(&basis, g, face, g.s_face, ri, f.rf_d / 2.0, side));
                }
            }
            if let Some(p) = g.pad {
                rects.extend(strip(&basis, g, g.frame.s_outer, g.frame.s_outer + p.thickness, ro, p.od / 2.0, side));
            }
        }
        for r in rects {
            view.edges.push(StyledEdge::new(Edge::Polyline { points: r.clone(), closed: true }, Layer::Visible));
            view.hatches.push(HatchRegion { loops: vec![r], angle: -std::f64::consts::FRAC_PI_4, spacing: 2.0 });
        }
        nozzle_mark(&mut view, &basis, g, scale);
    }
    view.line(m.side_pt(0.0, m.r.z_min - sheet_mm(16.0, scale), 1.0), m.side_pt(0.0, m.r.z_max + sheet_mm(16.0, scale), 1.0), Layer::Center);
    dedupe(&mut view.edges);
    Ok(view)
}

/// Where the angle `deg` (azimuth) points in the plan view (unit).
pub fn plan_dir(m: &VesselModel, deg: f64) -> P2 {
    m.plan_basis().d(azimuth(&m.doc.global, deg)).unit().unwrap_or(P2::new(0.0, 1.0))
}

/// The point at angle `a` (radians, sheet) on a circle about the plan's centre.
pub fn plan_pt(r: f64, a: f64) -> P2 {
    angle_pt(P2::default(), r, a)
}

fn check_scale(scale: f64) -> Result<()> {
    if scale.is_finite() && scale > 0.0 && scale <= 100.0 {
        Ok(())
    } else {
        Err(DrawingError::Invalid(format!("view scale must be in (0, 100] (got {scale})")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cylinder_vessel_draws_its_outline_and_invalid_vessels_are_reported() {
        let doc = VesselDoc::cylinder("S1", 1000.0, 10.0, 2000.0);
        let m = VesselModel::embedded(&doc).unwrap();
        let v = elevation(&m, 0.05).unwrap();
        // Two silhouettes and the two end faces across, plus the axis.
        let outline = v.edges.iter().filter(|e| e.layer == Layer::Visible).count();
        assert_eq!(outline, 4, "{:?}", v.edges);
        let (lo, hi) = v.edge_bounds().unwrap();
        assert!((hi.x - 510.0).abs() < 1e-9 && (lo.x + 510.0).abs() < 1e-9, "{lo:?} {hi:?}");
        let empty = VesselDoc::new(None);
        assert!(matches!(VesselModel::embedded(&empty), Err(DrawingError::Vessel(_))));
        assert!(elevation(&m, f64::NAN).is_err());
    }

    #[test]
    fn profile_sampling_keeps_the_loop_closed_and_in_order() {
        let segs = vec![
            Seg::Line([0.0, 0.0], [10.0, 0.0]),
            Seg::Arc { c: [10.0, 10.0], a: [10.0, 0.0], b: [20.0, 10.0] },
            Seg::Line([20.0, 10.0], [0.0, 10.0]),
            Seg::Line([0.0, 10.0], [0.0, 0.0]),
        ];
        let p = sample_profile(&segs);
        assert_eq!(p.first(), Some(&[0.0, 0.0]));
        for w in p.windows(2) {
            assert!((w[0][0] - w[1][0]).hypot(w[0][1] - w[1][1]) < 20.0 + 1e-9, "{w:?}");
        }
        // The arc is sampled from its start (10, 0) to (20, 10).
        let i = p.iter().position(|q| *q == [10.0, 0.0]).unwrap();
        assert!((p[i + 1][0] - 10.0).abs() < 1.0 && p[i + 1][1] < 1.0, "{p:?}");
        {}
    }
}
