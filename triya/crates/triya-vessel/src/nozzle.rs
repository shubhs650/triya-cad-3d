// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Nozzles: where each nozzle's axis is ([`frame`]), what its neck and pad are ([`resolve`]),
//! and the solid it adds to its host, computed without the kernel ([`expected_change`]) so the
//! kernel's result can be checked.
//!
//! # Frames (`vessel-schema.md` §4.1)
//! A nozzle axis is a line `origin + s · dir` (`dir` outward, unit). The azimuth `u` of
//! `angle_deg` follows `global.angle_zero` and `angle_sense`:
//! - vertical vessels (axis +Z): 0° is plant north = +Y (`plus_x`: +X), clockwise looking down,
//!   so 90° is +X;
//! - horizontal vessels (axis +X): 0° is the top (+Z; `plus_y`: +Y; `plus_x` is the axis and
//!   reads as the top), clockwise looking from the left end along +X, so 90° is −Y.
//!
//! `counterclockwise_from_above` turns the other way. `t = axis × u` is the circumferential
//! tangent (toward increasing counter-clockwise angle).
//!
//! On a cylinder the elevation is `axial.mm` from the datum (`ref: datum`) or from the host's
//! start (`ref: host_from`):
//! - `radial`: through the vessel axis along `u`; `s = 0` on the vessel axis.
//! - `hillside`: parallel to `u`, offset `offset_mm` along `t`; `s = 0` in the plane through the
//!   vessel axis.
//! - `tangential`: as hillside, default offset `R_in − neck_OD/2` (the neck's outside flush with
//!   the inside of the shell).
//! - `tilted`: `u` turned about the circumferential tangent through the point where the axis
//!   meets the shell's outside at the elevation (positive tilt points toward +axis); `s = 0`
//!   where the axis crosses the vessel axis.
//!
//! On a head (`axial` is not used; the head is the host):
//! - `parallel_to_axis`: parallel to the vessel axis, outward, at `radius_mm` from it; `s = 0`
//!   in the head's tangent-line plane.
//! - `head_radial_at_r`: normal to the head's outside surface where that surface is
//!   `radius_mm` from the axis; `s = 0` on the outside surface.
//!
//! # Projection
//! The flange face is `projection.mm` along the axis from: `cl`, the frame's reference (`s = 0`
//! for the cylinder frames, which is the vessel centreline; the tangent-line plane for both head
//! frames); `od`, where the axis leaves the host's outside surface; `insulation`, that plus
//! `global.insulation_thickness_mm`.
//!
//! # Neck and pad
//! The neck is the pipe of the B36.10 table (`schedule`, or the table's OD with `neck_thk_mm`).
//! `set_in` necks pass through the wall and end flush with the inside surface, or
//! `inside_projection_mm` beyond it (a square cut at that distance from where the axis meets
//! the inside surface); `set_on` necks sit on the outside surface over a hole of the neck's
//! bore. Either way the joined solid is the host plus the neck outside it, less the bore; the
//! two differ in the welds, which the drawing shows. A pad is a plate of `thickness_mm` on the
//! host's outside, out to `width_mm` beyond the neck OD, conforming to the host.

use serde_json::json;
use triya_stdlib::{FlangeKind, TableSet};

use crate::emit::CommandCall;
use crate::eval::{HeadShape, HeadSide, Profile, Resolved, Station};
use crate::model::{
    AngleSense, AngleZero, Attachment, AttachmentKind, AxialRef, Facing, FlangeStandard, Frame, Global, NozzleAttach, Orientation, ProjectionFrom,
    VesselDoc,
};
use crate::{Result, VesselError};

pub type V3 = [f64; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn mul(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

pub fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

fn unit(a: V3) -> Option<V3> {
    let n = norm(a);
    (n > 1e-12 && n.is_finite()).then(|| mul(a, 1.0 / n))
}

/// The vessel's axis.
pub fn vessel_axis(o: Orientation) -> V3 {
    match o {
        Orientation::Vertical => [0.0, 0.0, 1.0],
        Orientation::Horizontal => [1.0, 0.0, 0.0],
    }
}

/// `v` (perpendicular to the unit `k`) turned by `angle` about `k` (right hand).
fn turn(v: V3, k: V3, angle: f64) -> V3 {
    let (s, c) = angle.sin_cos();
    add(mul(v, c), mul(cross(k, v), s))
}

/// The outward radial direction at azimuth `deg` (see the module docs).
pub fn azimuth(g: &Global, deg: f64) -> V3 {
    let th = deg.to_radians();
    let ccw = g.angle_sense == AngleSense::CounterclockwiseFromAbove;
    match g.orientation {
        Orientation::Vertical => {
            let zero = if g.angle_zero == AngleZero::PlusX { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
            // Clockwise seen from above (looking along −Z) is a negative turn about +Z.
            turn(zero, [0.0, 0.0, 1.0], if ccw { th } else { -th })
        }
        Orientation::Horizontal => {
            let zero = if g.angle_zero == AngleZero::PlusY { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
            // Clockwise seen from the left end (looking along +X) is a positive turn about +X.
            turn(zero, [1.0, 0.0, 0.0], if ccw { -th } else { th })
        }
    }
}

/// Where a point is relative to a host's wall surface `off` out from the inside (0: the inside
/// surface, the thickness: the outside): negative inside it, positive outside, about the
/// distance near the surface. `rho` is the distance from the vessel axis, `z` the axial
/// coordinate. `None` for hosts nozzles are not built on.
pub fn surface(st: &Station, rho: f64, z: f64, off: f64) -> Option<f64> {
    match (st.kind, st.profile) {
        ("cylinder", Profile::Straight) => Some(rho - (st.r_from + off)),
        ("head", Profile::Head { side, sf, dish, shape }) => {
            let (dir, tl, rs) = match side {
                HeadSide::CrownAtStart => (-1.0, st.z_to - sf, st.r_to),
                HeadSide::CrownAtEnd => (1.0, st.z_from + sf, st.r_from),
            };
            let w = (z - tl) * dir;
            if w <= 0.0 {
                return Some(rho - (rs + off));
            }
            Some(match shape {
                HeadShape::Ellipsoidal => {
                    let (a, b) = (rs + off, dish + off);
                    ((rho / a).hypot(w / b) - 1.0) * a.min(b)
                }
                HeadShape::Torispherical { crown_r: l, knuckle_r: r } => {
                    let c = ((l - r).powi(2) - (rs - r).powi(2)).max(0.0).sqrt();
                    // In the knuckle when the point is further from the axis, seen from the
                    // crown centre, than the knuckle centre is.
                    if rho * c >= (w + c) * (rs - r) { (rho - (rs - r)).hypot(w) - (r + off) } else { rho.hypot(w + c) - (l + off) }
                }
            })
        }
        _ => None,
    }
}

/// The host surface `off` at the point `p`.
fn surface_at(st: &Station, axis: V3, p: V3, off: f64) -> Option<f64> {
    let z = dot(p, axis);
    let rho = norm(sub(p, mul(axis, z)));
    surface(st, rho, z, off)
}

/// The first `s` after `s0` (where the point is inside surface `off`) at which the axis line
/// crosses that surface, searching up to `s1`.
fn crossing(st: &Station, axis: V3, o: V3, d: V3, s0: f64, s1: f64, off: f64) -> Option<f64> {
    let f = |s: f64| surface_at(st, axis, add(o, mul(d, s)), off);
    if !(f(s0)? < 0.0) {
        return None;
    }
    let h = (st.thickness / 4.0).clamp(0.05, 2.0);
    let n = ((s1 - s0) / h).ceil();
    if !(n.is_finite() && n > 0.0 && n <= 1e6) {
        return None;
    }
    let mut a = s0;
    for i in 1..=(n as usize) {
        let b = (s0 + h * i as f64).min(s1);
        if f(b)? >= 0.0 {
            let (mut lo, mut hi) = (a, b);
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if f(mid)? >= 0.0 { hi = mid } else { lo = mid }
            }
            return Some(0.5 * (lo + hi));
        }
        a = b;
    }
    None
}

/// A nozzle's axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NozzleFrame {
    /// The point `s = 0` (the `cl` projection reference) and the outward unit direction.
    pub origin: V3,
    pub dir: V3,
    /// Where the axis leaves the host's inside and outside surfaces.
    pub s_inner: f64,
    pub s_outer: f64,
    /// A point on the axis inside the vessel.
    pub s_cavity: f64,
}

impl NozzleFrame {
    pub fn at(&self, s: f64) -> V3 {
        add(self.origin, mul(self.dir, s))
    }
}

fn invalid(a: &Attachment, msg: impl std::fmt::Display) -> VesselError {
    VesselError::Invalid(format!("nozzle `{}`: {msg}", a.id))
}

/// A head's tangent-line position and the outward axial direction (+1/−1 along the axis).
fn head_tl(st: &Station) -> Option<(f64, f64, f64)> {
    match st.profile {
        Profile::Head { side: HeadSide::CrownAtStart, sf, .. } => Some((st.z_to - sf, -1.0, st.r_to)),
        Profile::Head { side: HeadSide::CrownAtEnd, sf, .. } => Some((st.z_from + sf, 1.0, st.r_from)),
        _ => None,
    }
}

/// The axis of nozzle `a` on `host`, for a neck of outside diameter `neck_od`.
pub fn frame(g: &Global, a: &Attachment, host: &Station, neck_od: f64) -> Result<NozzleFrame> {
    let ax = vessel_axis(g.orientation);
    let u = azimuth(g, a.angle_deg);
    let t = cross(ax, u);
    let ri = host.r_from;
    let ro = ri + host.thickness;
    let (origin, dir, s_cavity) = match (host.kind, a.frame) {
        ("cylinder", f) => {
            let z = match a.axial.reference {
                AxialRef::Datum => a.axial.mm,
                AxialRef::HostFrom => host.z_from + a.axial.mm,
            };
            let on_axis = mul(ax, z);
            match f {
                Frame::Radial => (on_axis, u, 0.0),
                Frame::Hillside { offset_mm } | Frame::Tangential { offset_mm: Some(offset_mm) } => {
                    if offset_mm + neck_od / 2.0 > ri + 1e-9 {
                        return Err(invalid(
                            a,
                            format!("the offset {offset_mm} mm puts the neck (OD {neck_od}) outside the shell's inside radius {ri}"),
                        ));
                    }
                    (add(on_axis, mul(t, offset_mm)), u, 0.0)
                }
                Frame::Tangential { offset_mm: None } => (add(on_axis, mul(t, ri - neck_od / 2.0)), u, 0.0),
                Frame::Tilted { tilt_deg } => {
                    let (s, c) = tilt_deg.to_radians().sin_cos();
                    let d = add(mul(u, c), mul(ax, s));
                    // Through the shell's outside at the elevation; s = 0 on the vessel axis.
                    (sub(add(on_axis, mul(u, ro)), mul(d, ro / c)), d, 0.0)
                }
                Frame::ParallelToAxis { .. } | Frame::HeadRadialAtR { .. } => {
                    return Err(invalid(a, "`parallel_to_axis` and `head_radial_at_r` are for nozzles on heads"));
                }
            }
        }
        ("head", f) => {
            let (tl, out, rs) = head_tl(host).ok_or_else(|| invalid(a, "the host head is not resolved"))?;
            let ax_out = mul(ax, out);
            match f {
                Frame::ParallelToAxis { radius_mm } => {
                    if radius_mm + neck_od / 2.0 > rs {
                        return Err(invalid(a, format!("at radius {radius_mm} mm the neck (OD {neck_od}) leaves the head (inside radius {rs})")));
                    }
                    (add(mul(ax, tl), mul(u, radius_mm)), ax_out, 0.0)
                }
                Frame::HeadRadialAtR { radius_mm } => {
                    if radius_mm >= rs {
                        return Err(invalid(a, format!("radius {radius_mm} mm is not on the dish (inside radius {rs})")));
                    }
                    // Where the outside surface is `radius_mm` from the axis, and its normal.
                    let start = add(mul(ax, tl), mul(u, radius_mm));
                    let w = crossing(host, ax, start, ax_out, 0.0, 4.0 * (rs + host.thickness), host.thickness)
                        .ok_or_else(|| invalid(a, "cannot find the head's outside surface"))?;
                    let p = add(start, mul(ax_out, w));
                    let h = 1e-4 * (rs + host.thickness);
                    let grad = |dv: V3| -> Option<f64> {
                        Some(
                            (surface_at(host, ax, add(p, mul(dv, h)), host.thickness)? - surface_at(host, ax, sub(p, mul(dv, h)), host.thickness)?)
                                / (2.0 * h),
                        )
                    };
                    let n = match (grad(u), grad(ax_out)) {
                        (Some(gu), Some(ga)) => unit(add(mul(u, gu), mul(ax_out, ga))),
                        _ => None,
                    }
                    .ok_or_else(|| invalid(a, "the head's surface normal is undefined there"))?;
                    // Back inside: along −n until inside the inside surface (within the radius).
                    let mut s_cav = None;
                    let step = (host.thickness / 4.0).max(0.05);
                    let mut s = 0.0;
                    while s > -2.0 * (rs + host.thickness) {
                        s -= step;
                        if surface_at(host, ax, add(p, mul(n, s)), 0.0).is_some_and(|v| v < 0.0) {
                            s_cav = Some(s - step);
                            break;
                        }
                    }
                    (p, n, s_cav.ok_or_else(|| invalid(a, "the normal does not enter the vessel"))?)
                }
                _ => return Err(invalid(a, "on a head, use `parallel_to_axis` or `head_radial_at_r`")),
            }
        }
        (kind, _) => return Err(VesselError::NotSupportedYet(format!("nozzle `{}` on a {kind} (`{}`)", a.id, host.id))),
    };
    let reach = 4.0 * (ro + host.thickness) + 1000.0;
    let s_inner = crossing(host, ax, origin, dir, s_cavity, s_cavity + reach, 0.0)
        .ok_or_else(|| invalid(a, "the axis does not meet the host's inside surface"))?;
    let s_outer = crossing(host, ax, origin, dir, s_cavity, s_cavity + reach, host.thickness)
        .ok_or_else(|| invalid(a, "the axis does not meet the host's outside surface"))?;
    Ok(NozzleFrame { origin, dir, s_inner, s_outer, s_cavity })
}

/// A nozzle's reinforcing pad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadGeom {
    pub od: f64,
    pub thickness: f64,
}

/// A nozzle's flange, from its table row (mm). `s_face` is the gasket face: the top of the
/// raised face (RF, RTJ: the RTJ groove is not modelled in the beta) or the flat face (FF).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NozzleFlangeGeom {
    pub kind: FlangeKind,
    pub od: f64,
    pub bolt_circle: f64,
    pub bolt_count: u32,
    pub bolt_hole: f64,
    /// Ring thickness without the raised face.
    pub thickness: f64,
    pub hub_od: f64,
    /// The hub's diameter at the weld end (WN), else the neck OD.
    pub hub_weld_d: f64,
    /// From the hub's back to the ring face (without the raised face).
    pub length_through_hub: f64,
    pub rf_d: f64,
    pub rf_h: f64,
    /// Bore of a slip-on, lap-joint or socket-weld flange (the weld neck's is the neck's bore).
    pub bore: f64,
}

impl NozzleFlangeGeom {
    /// The ring's face and back.
    pub fn ring(&self, s_face: f64) -> (f64, f64) {
        let face = s_face - self.rf_h;
        (face, face - self.thickness)
    }

    /// The back of the hub (the weld end of a weld neck); the ring's back for a blind.
    pub fn back(&self, s_face: f64) -> f64 {
        let (face, back) = self.ring(s_face);
        match self.kind {
            FlangeKind::Blind => back,
            _ => face - self.length_through_hub.max(self.thickness),
        }
    }
}

/// A nozzle, resolved (all mm along its axis).
#[derive(Clone, Debug, PartialEq)]
pub struct NozzleGeom {
    pub id: String,
    /// The host element.
    pub host: String,
    pub frame: NozzleFrame,
    pub neck_od: f64,
    pub neck_wall: f64,
    /// Where the neck starts (its inner end; for a flush `set_in` neck, where the axis meets
    /// the inside surface) and the flange face.
    pub s_start: f64,
    pub s_face: f64,
    /// Where the neck pipe ends: the weld end of a weld-neck flange, inside a slip-on, the back
    /// of a blind; the face without a flange.
    pub s_neck_end: f64,
    pub flange: Option<NozzleFlangeGeom>,
    pub attach: NozzleAttach,
    pub inside_projection: f64,
    pub pad: Option<PadGeom>,
    pub manway: bool,
    /// A manway with a davit: a reference for the drawing and BOM (no davit geometry in the beta).
    pub davit: bool,
    /// The neck came from a table row not marked verified.
    pub unverified_table: bool,
    pub warnings: Vec<String>,
}

impl NozzleGeom {
    pub fn neck_id(&self) -> f64 {
        self.neck_od - 2.0 * self.neck_wall
    }

    /// The largest radius about the axis the nozzle touches the host within.
    pub fn footprint_r(&self) -> f64 {
        self.pad.map_or(self.neck_od / 2.0, |p| p.od / 2.0)
    }
}

/// Nozzle `a` of `doc` against its resolved host, with pipe rows from `tables`.
pub fn resolve(doc: &VesselDoc, r: &Resolved, a: &Attachment, tables: &TableSet) -> Result<NozzleGeom> {
    let AttachmentKind::Nozzle { nps, schedule, neck_thk_mm, projection, inside_projection_mm, attach, pad, manway, flange, neck_type, .. } = &a.kind
    else {
        return Err(VesselError::Invalid(format!("attachment `{}` is not a nozzle", a.id)));
    };
    let host = r.station(&a.host).ok_or_else(|| invalid(a, format!("no element `{}`", a.host)))?;
    let mut warnings = Vec::new();
    let mut unverified_table = false;
    let canonical = triya_stdlib::canonical_nps(nps);
    let (od, wall) = match (schedule, neck_thk_mm) {
        (_, Some(t)) => {
            let row = tables.pipe_rows().find(|p| p.nps == canonical).ok_or_else(|| invalid(a, format!("no pipe NPS {nps} in the tables")))?;
            (row.od, *t)
        }
        (Some(sch), None) => {
            let row = tables.pipe(nps, sch).ok_or_else(|| invalid(a, format!("no pipe NPS {nps} schedule {sch} in the tables")))?;
            if !row.verified.is_trusted() {
                unverified_table = true;
                warnings.push(format!("nozzle `{}`: pipe row {} is {:?} in its table", a.id, row.size_key(), row.verified));
            }
            (row.od, row.wall)
        }
        (None, None) => {
            let row = tables.pipe(nps, "STD").ok_or_else(|| invalid(a, format!("give `schedule` or `neck_thk_mm` (no NPS {nps} STD pipe)")))?;
            warnings.push(format!("nozzle `{}`: no schedule given; STD wall {} mm", a.id, row.wall));
            (row.od, row.wall)
        }
    };
    if !(wall > 0.0 && 2.0 * wall < od) {
        return Err(invalid(a, format!("neck wall {wall} mm does not fit OD {od}")));
    }
    let f = frame(&doc.global, a, host, od)?;
    let s_face = match projection.from {
        ProjectionFrom::Cl => projection.mm,
        ProjectionFrom::Od => f.s_outer + projection.mm,
        ProjectionFrom::Insulation => f.s_outer + doc.global.insulation_thickness_mm + projection.mm,
    };
    let pad = pad.map(|p| PadGeom { od: od + 2.0 * p.width_mm, thickness: p.thickness_mm });
    let clear = f.s_outer + pad.map_or(0.0, |p| p.thickness);
    if !(s_face > clear + 1.0) {
        return Err(invalid(
            a,
            format!(
                "the flange face ({s_face:.1} mm along the axis) is not outside the shell{} ({clear:.1})",
                if pad.is_some() { " and pad" } else { "" }
            ),
        ));
    }
    let flange = match flange {
        Some(f) => Some(nozzle_flange(a, tables, nps, f, od, wall, &mut warnings, &mut unverified_table)?),
        None => None,
    };
    if *neck_type != crate::model::NeckType::Pipe {
        warnings.push(format!("nozzle `{}`: {:?} neck modelled as pipe of the same OD and wall in the beta", a.id, neck_type));
    }
    let s_neck_end = match flange {
        None => s_face,
        Some(f) => match f.kind {
            FlangeKind::Wn | FlangeKind::Lwn | FlangeKind::Blind => f.back(s_face),
            // The pipe stops one wall short of the ring face inside a slip-on (fillet weld).
            FlangeKind::So | FlangeKind::Lj | FlangeKind::Sw => f.ring(s_face).0 - wall,
        },
    };
    if let Some(f) = flange
        && !(f.back(s_face) > clear + 1.0)
    {
        return Err(invalid(
            a,
            format!(
                "the {} flange's back ({:.1} mm along the axis) is not outside the shell ({clear:.1}); lengthen the projection",
                f.kind.code(),
                f.back(s_face)
            ),
        ));
    }
    let s_start = match attach {
        NozzleAttach::SetIn => f.s_inner - inside_projection_mm,
        NozzleAttach::SetOn if *inside_projection_mm > 0.0 => return Err(invalid(a, "a set-on neck has no inside projection")),
        NozzleAttach::SetOn => f.s_outer,
    };
    let g = NozzleGeom {
        id: a.id.clone(),
        host: host.id.clone(),
        frame: f,
        neck_od: od,
        neck_wall: wall,
        s_start,
        s_face,
        s_neck_end,
        flange,
        attach: *attach,
        inside_projection: *inside_projection_mm,
        pad,
        manway: manway.is_some(),
        davit: manway.is_some_and(|m| m.davit),
        unverified_table,
        warnings,
    };
    footprint_on_host(&doc.global, a, host, &g)?;
    Ok(g)
}

/// The standard as the tables name it.
fn standard_name(s: FlangeStandard) -> &'static str {
    match s {
        FlangeStandard::B16_5 => "B16.5",
        FlangeStandard::B16_47A => "B16.47A",
        FlangeStandard::B16_47B => "B16.47B",
    }
}

/// The nozzle's flange from its table row.
#[allow(clippy::too_many_arguments)]
fn nozzle_flange(
    a: &Attachment,
    tables: &TableSet,
    nps: &str,
    f: &crate::model::NozzleFlange,
    neck_od: f64,
    wall: f64,
    warnings: &mut Vec<String>,
    unverified: &mut bool,
) -> Result<NozzleFlangeGeom> {
    let standard = standard_name(f.standard);
    let class = f.class.to_string();
    // Long weld necks are not in the tables: the WN row's ring and drilling, the hub as WN.
    let kind = if f.kind == FlangeKind::Lwn {
        warnings.push(format!("nozzle `{}`: LWN modelled with the WN row's dimensions in the beta", a.id));
        FlangeKind::Wn
    } else {
        f.kind
    };
    let row = tables
        .flange_in(standard, nps, &class, kind)
        .ok_or_else(|| invalid(a, format!("no {standard} flange row {nps}/{class}/{} in the loaded tables", kind.code())))?;
    if !row.verified.is_trusted() {
        *unverified = true;
        warnings.push(format!("nozzle `{}`: flange row {} is {:?} in its table", a.id, row.size_key(), row.verified));
    }
    let bolt_hole = row.bolt_hole_mm().ok_or_else(|| invalid(a, format!("cannot read the bolt size `{}`", row.bolt_size)))?;
    let rf_h = match f.facing {
        Facing::Ff => 0.0,
        Facing::Rf | Facing::Rtj => row.raised_face_h,
    };
    if f.facing == Facing::Rtj {
        warnings.push(format!("nozzle `{}`: RTJ facing modelled as a raised face (no ring groove) in the beta", a.id));
    }
    let g = NozzleFlangeGeom {
        kind: f.kind,
        od: row.od,
        bolt_circle: row.bolt_circle,
        bolt_count: row.bolt_count,
        bolt_hole,
        thickness: row.thickness,
        hub_od: row.hub_od,
        hub_weld_d: row.hub_weld_d.unwrap_or(neck_od),
        length_through_hub: row.length_through_hub,
        rf_d: if rf_h > 0.0 { row.raised_face_d } else { 0.0 },
        rf_h,
        bore: row.bore.unwrap_or(neck_od + 1.6),
    };
    let fits = g.thickness > 0.0
        && g.bolt_count > 0
        && g.bolt_circle / 2.0 + g.bolt_hole / 2.0 < g.od / 2.0
        && (kind == FlangeKind::Blind || g.hub_od / 2.0 < g.bolt_circle / 2.0 - g.bolt_hole / 2.0)
        && (g.rf_h == 0.0 || g.rf_d / 2.0 < g.bolt_circle / 2.0 - g.bolt_hole / 2.0)
        && (kind == FlangeKind::Blind || g.hub_weld_d / 2.0 > neck_od / 2.0 - wall);
    if !fits {
        return Err(invalid(a, format!("flange row {} does not fit a neck of OD {neck_od}", row.size_key())));
    }
    Ok(g)
}

/// The nozzle (pad included) must meet its host only within the host's own axial extent.
fn footprint_on_host(gl: &Global, a: &Attachment, host: &Station, g: &NozzleGeom) -> Result<()> {
    let ax = vessel_axis(gl.orientation);
    let (e1, e2) = perpendiculars(g.frame.dir);
    let r = g.footprint_r();
    for k in 0..16 {
        let th = std::f64::consts::TAU * f64::from(k) / 16.0;
        let side = add(mul(e1, r * th.cos()), mul(e2, r * th.sin()));
        for s in [g.frame.s_inner.min(g.s_start), g.frame.s_outer + g.pad.map_or(0.0, |p| p.thickness)] {
            let z = dot(add(g.frame.at(s), side), ax);
            if z < host.z_from - 1e-6 || z > host.z_to + host.thickness + 1e-6 {
                return Err(invalid(
                    a,
                    format!(
                        "it reaches z = {z:.1}, outside its host `{}` ({:.1} … {:.1}); put it on the element it sits on",
                        host.id, host.z_from, host.z_to
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Two unit vectors perpendicular to `d` and to each other.
pub fn perpendiculars(d: V3) -> (V3, V3) {
    let helper = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let e1 = unit(cross(d, helper)).unwrap_or([0.0, 0.0, 1.0]);
    (e1, cross(d, e1))
}

/// The volume (mm³) the built nozzle adds to its host: neck and pad outside the host's metal,
/// less the bore through the wall, by quadrature over the nozzle's cross-section (an `n × n`
/// grid over the footprint, each line split where any surface crosses it). The reference value
/// [`crate::validate`] compares the kernel's result with.
pub fn expected_change(gl: &Global, host: &Station, g: &NozzleGeom, n: usize) -> Option<f64> {
    let ax = vessel_axis(gl.orientation);
    let (e1, e2) = perpendiculars(g.frame.dir);
    let r_foot = g.footprint_r();
    let (r_od, r_id) = (g.neck_od / 2.0, g.neck_id() / 2.0);
    let t = host.thickness;
    let (lo, hi) = (g.frame.s_cavity, g.s_neck_end);
    let n = n.clamp(8, 2000);
    let step = (t / 6.0).clamp(0.02, 1.0);
    let samples = ((hi - lo) / step).ceil();
    if !(samples.is_finite() && samples > 0.0 && samples < 1e6) {
        return None;
    }
    let samples = samples as usize;
    // A polar grid whose rings break exactly at the bore, the neck and the pad, so the only
    // error left is that of a smooth integrand.
    let mut breaks = vec![0.0, r_id, r_od, r_foot];
    breaks.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let per_ring = (n / 4).max(2);
    let mut cells = Vec::with_capacity(per_ring * 3 * n);
    for w in breaks.windows(2) {
        let (Some(&r0), Some(&r1)) = (w.first(), w.get(1)) else { continue };
        let dr = (r1 - r0) / per_ring as f64;
        for i in 0..per_ring {
            let rr = r0 + (i as f64 + 0.5) * dr;
            for j in 0..n {
                let th = std::f64::consts::TAU * (j as f64 + 0.5) / n as f64;
                cells.push((rr, rr * th.cos(), rr * th.sin(), rr * dr * std::f64::consts::TAU / n as f64));
            }
        }
    }
    let mut total = 0.0;
    for (rr, x, y, area) in cells {
        {
            let o = add(g.frame.origin, add(mul(e1, x), mul(e2, y)));
            // Host metal; the built solid (host, pad, neck, less the bore).
            let host_at = |s: f64| -> Option<bool> {
                let p = add(o, mul(g.frame.dir, s));
                Some(surface_at(host, ax, p, 0.0)? >= 0.0 && surface_at(host, ax, p, t)? < 0.0)
            };
            let built_at = |s: f64| -> Option<bool> {
                let p = add(o, mul(g.frame.dir, s));
                let (inner, outer) = (surface_at(host, ax, p, 0.0)?, surface_at(host, ax, p, t)?);
                let metal = inner >= 0.0 && outer < 0.0;
                let pad =
                    g.pad.is_some_and(|pd| outer >= 0.0 && rr <= pd.od / 2.0 && surface_at(host, ax, p, t + pd.thickness).is_some_and(|v| v < 0.0));
                // The neck: from its start (a set-in neck fills the wall; inside the vessel only
                // as far as its inside projection) to the face.
                let neck = rr <= r_od && s <= g.s_neck_end && (outer >= 0.0 || (s >= g.s_start && (inner >= 0.0 || g.inside_projection > 0.0)));
                // The bore is cut from inside the vessel (`s_cavity`) to the neck's end.
                let bore = rr <= r_id && s <= g.s_neck_end;
                Some((metal || pad || neck) && !bore)
            };
            let len = |f: &dyn Fn(f64) -> Option<bool>| -> Option<f64> {
                let mut sum = 0.0;
                let (mut a, mut prev) = (lo, f(lo)?);
                for k in 1..=samples {
                    let b = (lo + step * k as f64).min(hi);
                    let cur = f(b)?;
                    if cur == prev {
                        if cur {
                            sum += b - a;
                        }
                    } else {
                        // One change within [a, b] (the step is a fraction of the wall).
                        let (mut l, mut h) = (a, b);
                        for _ in 0..40 {
                            let m = 0.5 * (l + h);
                            if f(m)? == prev { l = m } else { h = m }
                        }
                        let x = 0.5 * (l + h);
                        sum += if prev { x - a } else { b - x };
                    }
                    (a, prev) = (b, cur);
                }
                Some(sum)
            };
            total += (len(&built_at)? - len(&host_at)?) * area;
        }
    }
    Some(total)
}

// ---------------------------------------------------------------------------------------------
// The plan

/// Lines parallel to the axis through the footprint of radius `r`: the centre and three rings.
fn footprint_lines(g: &NozzleGeom, r: f64) -> Vec<V3> {
    let (e1, e2) = perpendiculars(g.frame.dir);
    let mut out = vec![g.frame.origin];
    for k in [0.5, 0.85, 1.0] {
        for j in 0..32 {
            let th = std::f64::consts::TAU * f64::from(j) / 32.0;
            out.push(add(g.frame.origin, add(mul(e1, k * r * th.cos()), mul(e2, k * r * th.sin()))));
        }
    }
    out
}

/// The range of `s` at which the lines through the footprint of radius `r` meet surface `off`.
fn crossing_range(gl: &Global, host: &Station, g: &NozzleGeom, r: f64, off: f64) -> Option<(f64, f64)> {
    let ax = vessel_axis(gl.orientation);
    let reach = 4.0 * (host.r_from.max(host.r_to) + host.thickness) + 1000.0;
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for o in footprint_lines(g, r) {
        let s = crossing(host, ax, o, g.frame.dir, g.frame.s_cavity, g.frame.s_cavity + reach, off)?;
        lo = lo.min(s);
        hi = hi.max(s);
    }
    Some((lo, hi))
}

/// A plane across the axis that lies within the wall over the whole footprint of radius `r`
/// (a rod starting there needs no trim to end flush with the inside), if there is one.
fn plane_in_wall(gl: &Global, host: &Station, g: &NozzleGeom, r: f64) -> Option<f64> {
    let (_, inner_max) = crossing_range(gl, host, g, r, 0.0)?;
    let (outer_min, _) = crossing_range(gl, host, g, r, host.thickness)?;
    let margin = (0.05 * host.thickness).max(0.05);
    (inner_max + margin < outer_min - margin).then_some(0.5 * (inner_max + outer_min))
}

/// The body names a nozzle's plan uses.
pub fn body_name(id: &str, part: &str) -> String {
    format!("Vessel {id} {part}")
}

/// How a neck or pad rod gets its inner end inside the wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Start {
    /// A square end at this `s` (within the wall, or the inside projection's end).
    Plane(f64),
    /// From inside the vessel, trimmed by the host's mid-wall surface (cylinder or crown sphere).
    Trimmed,
}

fn rod(name: &str, body: Option<&str>, g: &NozzleGeom, radius: f64, from: f64, to: f64, operation: &str, targets: &[&str]) -> CommandCall {
    let mut p = json!({"radius": radius, "height": to - from, "base": g.frame.at(from), "axis": g.frame.dir, "operation": operation, "name": name});
    if let Some(b) = body {
        p["body_name"] = json!(b);
    }
    if !targets.is_empty() {
        p["targets"] = json!(targets);
    }
    CommandCall { id: "solid.cylinder".into(), params: p }
}

/// The cut that trims a rod body to the host's mid-wall surface: a cylinder about the vessel
/// axis over the whole vessel, or the crown's sphere. `None` where the beta has no such cut
/// (an ellipsoidal dish, a torispherical knuckle).
fn mid_wall_cut(gl: &Global, r: &Resolved, host: &Station, g: &NozzleGeom, target: &str, name: &str) -> Option<CommandCall> {
    let ax = vessel_axis(gl.orientation);
    let mid = host.thickness / 2.0;
    match (host.kind, host.profile) {
        ("cylinder", _) => {
            let (z0, z1) = (r.z_min - 1000.0, r.z_max + 1000.0);
            Some(CommandCall {
                id: "solid.cylinder".into(),
                params: json!({"radius": host.r_from + mid, "height": z1 - z0, "base": mul(ax, z0), "axis": ax, "operation": "cut", "targets": [target], "name": name}),
            })
        }
        ("head", Profile::Head { shape: HeadShape::Torispherical { crown_r: l, knuckle_r: kr }, .. }) => {
            let (tl, out, rs) = head_tl(host)?;
            let c = ((l - kr).powi(2) - (rs - kr).powi(2)).max(0.0).sqrt();
            // Only when the whole footprint meets the crown: radially within the knuckle line.
            let ok = footprint_lines(g, g.footprint_r()).iter().all(|o| {
                let s = crossing(host, ax, *o, g.frame.dir, g.frame.s_cavity, g.frame.s_cavity + 4.0 * (rs + l), 0.0);
                s.is_some_and(|s| {
                    let p = add(*o, mul(g.frame.dir, s));
                    let z = dot(p, ax);
                    let rho = norm(sub(p, mul(ax, z)));
                    rho * c < ((z - tl) * out + c) * (rs - kr)
                })
            });
            ok.then(|| CommandCall {
                id: "solid.sphere".into(),
                params: json!({"radius": l + mid, "center": mul(ax, tl - out * c), "operation": "cut", "targets": [target], "name": name}),
            })
        }
        _ => None,
    }
}

/// The commands that build nozzle `g` into its host body: the pad (cylinder hosts), the neck
/// joined, the bore cut from inside the vessel to the face. `g` may be adjusted: where the
/// neck cannot be trimmed flush (an ellipsoidal dish too curved under a wide neck), it starts
/// square at the deepest point of the inside surface and projects inside (with a warning).
pub fn plan(gl: &Global, r: &Resolved, host: &Station, g: &mut NozzleGeom) -> Result<Vec<CommandCall>> {
    let id = g.id.clone();
    let host_body = format!("Vessel {}", host.id);
    let hb = host_body.as_str();
    let mut out = Vec::new();
    if let Some(pad) = g.pad {
        if host.kind != "cylinder" {
            g.warnings.push(format!("nozzle `{id}`: the pad is not built on a {} (not supported yet; drawn on the sheet)", host.kind));
            g.pad = None;
        } else {
            let rp = pad.od / 2.0;
            let pad_body = body_name(&id, "pad");
            let trim_body = body_name(&id, "pad trim");
            let (_, top) = crossing_range(gl, host, g, rp, host.thickness + pad.thickness)
                .ok_or_else(|| VesselError::Invalid(format!("nozzle `{id}`: the pad does not fit its host")))?;
            let s0 = match plane_in_wall(gl, host, g, rp) {
                Some(s0) => {
                    out.push(rod(&pad_body, Some(&pad_body), g, rp, s0, top + 1.0, "new", &[]));
                    s0
                }
                None => {
                    out.push(rod(&pad_body, Some(&pad_body), g, rp, g.frame.s_cavity, top + 1.0, "new", &[]));
                    out.extend(mid_wall_cut(gl, r, host, g, &pad_body, &body_name(&id, "pad inner trim")));
                    g.frame.s_cavity
                }
            };
            // The outside of the pad: everything beyond the host's OD + pad thickness goes. The
            // trim tube only spans the pad rod (with 10 mm to spare): a tube as long as the course
            // made the kernel's cut fail, slowly, on a 3 m course (vessel A, 18 Oct).
            let ax = vessel_axis(gl.orientation);
            let ends = [g.frame.at(s0), g.frame.at(top + 1.0)];
            let spread = rp * (1.0 - dot(g.frame.dir, ax).powi(2)).max(0.0).sqrt();
            let zs = ends.map(|p| dot(p, ax));
            let z0 = zs[0].min(zs[1]) - spread - 10.0;
            let z1 = zs[0].max(zs[1]) + spread + 10.0;
            let reach = ends.iter().map(|p| norm(sub(*p, mul(ax, dot(*p, ax))))).fold(0.0, f64::max) + rp + 10.0;
            let r_out = host.r_from + host.thickness + pad.thickness;
            out.push(CommandCall {
                id: "solid.cylinder".into(),
                params: json!({"radius": reach.max(r_out + 10.0), "height": z1 - z0, "base": mul(ax, z0), "axis": ax, "operation": "new",
                    "name": trim_body, "body_name": trim_body}),
            });
            out.push(CommandCall {
                id: "solid.cylinder".into(),
                params: json!({"radius": r_out, "height": z1 - z0 + 2.0, "base": mul(ax, z0 - 1.0), "axis": ax, "operation": "cut",
                    "targets": [trim_body], "name": body_name(&id, "pad trim bore")}),
            });
            out.push(CommandCall {
                id: "solid.combine".into(),
                params: json!({"target": pad_body, "tools": [trim_body], "operation": "cut", "name": body_name(&id, "pad shape")}),
            });
            out.push(CommandCall {
                id: "solid.combine".into(),
                params: json!({"target": hb, "tools": [pad_body], "operation": "join", "name": body_name(&id, "pad join")}),
            });
        }
    }
    let r_od = g.neck_od / 2.0;
    let start = if g.inside_projection > 0.0 {
        Start::Plane(g.s_start)
    } else {
        match plane_in_wall(gl, host, g, r_od) {
            Some(s) => Start::Plane(s),
            None if mid_wall_cut(gl, r, host, g, "", "").is_some() => Start::Trimmed,
            None => {
                let (deepest, highest) =
                    crossing_range(gl, host, g, r_od, 0.0).ok_or_else(|| VesselError::Invalid(format!("nozzle `{id}`: the neck misses its host")))?;
                g.warnings.push(format!(
                    "nozzle `{id}`: the neck cannot be trimmed flush on this {} in the beta; it projects up to {:.1} mm inside",
                    host.kind,
                    highest - deepest
                ));
                g.inside_projection = g.frame.s_inner - deepest;
                g.s_start = deepest;
                Start::Plane(deepest)
            }
        }
    };
    let neck = body_name(&id, "neck");
    match start {
        Start::Plane(s0) => out.push(rod(&neck, None, g, r_od, s0, g.s_neck_end, "join", &[hb])),
        Start::Trimmed => {
            out.push(rod(&neck, Some(&neck), g, r_od, g.frame.s_cavity, g.s_neck_end, "new", &[]));
            out.extend(mid_wall_cut(gl, r, host, g, &neck, &body_name(&id, "neck trim")));
            out.push(CommandCall {
                id: "solid.combine".into(),
                params: json!({"target": hb, "tools": [neck], "operation": "join", "name": body_name(&id, "neck join")}),
            });
        }
    }
    out.push(rod(&body_name(&id, "bore"), None, g, g.neck_id() / 2.0, g.frame.s_cavity, g.s_neck_end + 1.0, "cut", &[hb]));
    Ok(out)
}

/// The fallback (T3.5): the neck as its own body, square from where it meets the host to the
/// face, bored through, with no boolean against the host.
pub fn plan_separate(g: &NozzleGeom) -> Vec<CommandCall> {
    let body = body_name(&g.id, "neck (separate)");
    let from = match g.attach {
        NozzleAttach::SetIn if g.inside_projection > 0.0 => g.s_start,
        _ => g.frame.s_outer,
    };
    vec![
        rod(&body, Some(&body), g, g.neck_od / 2.0, from, g.s_neck_end, "new", &[]),
        rod(&body_name(&g.id, "bore (separate)"), None, g, g.neck_id() / 2.0, from - 1.0, g.s_neck_end + 1.0, "cut", &[body.as_str()]),
    ]
}

/// The flange's half-section in (radius from the nozzle axis, `s`), closed.
pub fn flange_section(g: &NozzleGeom) -> Option<Vec<[f64; 2]>> {
    let f = g.flange?;
    let (face, back) = f.ring(g.s_face);
    let hub_back = f.back(g.s_face);
    let mut pts = match f.kind {
        FlangeKind::Blind => vec![[0.0, back], [f.od / 2.0, back], [f.od / 2.0, face]],
        FlangeKind::Wn | FlangeKind::Lwn => {
            let rb = g.neck_id() / 2.0;
            vec![[rb, hub_back], [f.hub_weld_d / 2.0, hub_back], [f.hub_od / 2.0, back], [f.od / 2.0, back], [f.od / 2.0, face]]
        }
        FlangeKind::So | FlangeKind::Lj | FlangeKind::Sw => {
            let rb = f.bore / 2.0;
            let mut v = vec![[rb, hub_back]];
            if hub_back < back - 1e-9 {
                v.push([f.hub_od / 2.0, hub_back]);
                v.push([f.hub_od / 2.0, back]);
            } else {
                v.push([f.od / 2.0, back]);
            }
            v.push([f.od / 2.0, back]);
            v.push([f.od / 2.0, face]);
            v
        }
    };
    if f.rf_h > 0.0 {
        pts.push([f.rf_d / 2.0, face]);
        pts.push([f.rf_d / 2.0, g.s_face]);
    }
    let inner = match f.kind {
        FlangeKind::Blind => 0.0,
        FlangeKind::Wn | FlangeKind::Lwn => g.neck_id() / 2.0,
        _ => f.bore / 2.0,
    };
    pts.push([inner, g.s_face]);
    pts.dedup();
    Some(pts)
}

/// The plane the flange is sketched on: through the nozzle axis, its x along the vessel axis
/// seen across the nozzle (so the bolt holes straddle the principal centrelines), or any
/// perpendicular when the nozzle runs along the vessel axis.
fn flange_x(gl: &Global, g: &NozzleGeom) -> V3 {
    let ax = vessel_axis(gl.orientation);
    let d = g.frame.dir;
    unit(sub(ax, mul(d, dot(ax, d)))).unwrap_or_else(|| perpendiculars(d).0)
}

/// How [`flange_calls`] makes the bolt holes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoltHoles {
    /// One hole cut through the ring and patterned (the first choice: one feature to edit).
    Pattern,
    /// Each hole its own cut (when the kernel fails on the patterned cut).
    Each,
    /// The plain ring (when every cut failed; the drawing still shows the holes).
    None,
}

/// The flange as its own body: its section revolved about the nozzle axis, then its bolt holes
/// (straddling the centrelines: the first at 180°/n) as `holes` says. Run on its own: its
/// `$ref`s count from its first call.
pub fn flange_calls(gl: &Global, g: &NozzleGeom, holes: BoltHoles) -> Vec<CommandCall> {
    let (Some(f), Some(pts)) = (g.flange, flange_section(g)) else { return Vec::new() };
    let x = flange_x(gl, g);
    let y = g.frame.dir;
    let body = body_name(&g.id, "flange");
    let mut out = vec![CommandCall {
        id: "sketch.create".into(),
        params: json!({"name": body_name(&g.id, "flange section"), "plane": {"origin": g.frame.origin, "x_dir": x, "y_dir": y}}),
    }];
    for i in 0..pts.len() {
        if let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % pts.len())) {
            out.push(CommandCall { id: "sketch.line".into(), params: json!({"points": [a, b]}) });
        }
    }
    out.push(CommandCall { id: "sketch.finish".into(), params: json!({}) });
    out.push(CommandCall {
        id: "solid.revolve".into(),
        params: json!({"sketch": format!("{}0.sketch", crate::emit::REF), "axis": "y", "angle": "360 deg", "operation": "new", "name": body, "body_name": body}),
    });
    let (face, back) = f.ring(g.s_face);
    let n = f.bolt_count.max(1);
    let z = cross(y, x);
    let hole = |k: u32, name: String| {
        let a = std::f64::consts::PI * f64::from(2 * k + 1) / f64::from(n);
        let off = add(mul(x, f.bolt_circle / 2.0 * a.cos()), mul(z, f.bolt_circle / 2.0 * a.sin()));
        CommandCall {
            id: "solid.cylinder".into(),
            params: json!({"radius": f.bolt_hole / 2.0, "height": face - back + 2.0, "base": add(g.frame.at(back - 1.0), off), "axis": y,
                "operation": "cut", "targets": [body], "name": name}),
        }
    };
    match holes {
        BoltHoles::Pattern => {
            let first = body_name(&g.id, "bolt hole");
            out.push(hole(0, first.clone()));
            if n > 1 {
                out.push(CommandCall {
                    id: "solid.pattern.circular".into(),
                    params: json!({"features": [first], "axis": {"origin": g.frame.origin, "dir": y}, "count": n, "name": body_name(&g.id, "bolt holes")}),
                });
            }
        }
        BoltHoles::Each => out.extend((0..n).map(|k| hole(k, body_name(&g.id, &format!("bolt hole {}", k + 1))))),
        BoltHoles::None => {}
    }
    out
}

/// The tolerance (mm³) on the measured change of the host's volume: 2 % of the expected change
/// plus the measuring error of a large body (its tessellated volume is good to about 3e-4).
pub fn tolerance(expected: f64, host_volume: f64) -> f64 {
    0.02 * expected.abs() + 3e-4 * host_volume.abs() + 1.0
}

/// A nozzle's dimensions as user parameters `V<vessel>_<nozzle>_<field>` (mm, degrees).
pub fn parameters(vessel_id: &str, gl: &Global, a: &Attachment, g: &NozzleGeom) -> Vec<crate::emit::Param> {
    let prefix = crate::emit::parameter_prefix(vessel_id);
    let id = crate::emit::ident(&g.id);
    let ax = vessel_axis(gl.orientation);
    let mut out = Vec::new();
    let mut push = |field: &str, value: f64, unit: &'static str, what: &str| {
        if value.is_finite() {
            out.push(crate::emit::Param { name: format!("{prefix}_{id}_{field}"), value, unit, comment: format!("nozzle {}: {what}", g.id) });
        }
    };
    push("neck_od", g.neck_od, "mm", "neck outside diameter");
    push("neck_wall", g.neck_wall, "mm", "neck wall");
    if let AttachmentKind::Nozzle { projection, .. } = &a.kind {
        push("projection", projection.mm, "mm", "flange face from the projection reference");
    }
    push("elevation", dot(g.frame.at(g.frame.s_outer), ax), "mm", "where the axis meets the host's outside, from the datum");
    push("azimuth", a.angle_deg, "deg", "azimuth");
    if let Some(p) = g.pad {
        push("pad_od", p.od, "mm", "pad outside diameter");
        push("pad_thk", p.thickness, "mm", "pad thickness");
    }
    if let Some(f) = g.flange {
        push("flange_od", f.od, "mm", "flange outside diameter");
        push("bolt_circle", f.bolt_circle, "mm", "bolt circle diameter");
        push("bolt_count", f64::from(f.bolt_count), "", "bolt count");
        push("bolt_hole", f.bolt_hole, "mm", "bolt hole diameter");
        push("flange_thk", f.thickness, "mm", "flange ring thickness");
    }
    out
}
