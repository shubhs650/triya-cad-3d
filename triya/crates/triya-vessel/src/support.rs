// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Supports and details (Phase 4): saddles, legs, bracket lugs, anchor chairs, rings, coils,
//! lifting lugs and clips, each resolved into [`Part`]s: one body per plate, pipe or ring, with
//! the commands that build it and its volume computed without the kernel.
//!
//! Every part is its own body (`Vessel <attachment> <part>`), placed against the shell and never
//! joined to it: welds are drawn on the sheet (CLAUDE.md §4: no fillets on curved edges), a
//! boolean against a curved shell is what the kernel is slow and fragile at (Phase 3), and one
//! body per plate is what the BOM counts. Parts touch the shell's outside surface (or a wear
//! plate's) but do not overlap it or each other, so measured volumes add up.
//!
//! The commands are upstream's: a plate is a closed profile sketched on an explicit plane and
//! extruded (`solid.extrude`, `direction: symmetric` about the plate's mid-plane or `positive`
//! from its face); a ring or wear plate is a profile through the vessel axis revolved about it
//! (`solid.revolve`, `axis: y`, partial rings `direction: symmetric` about their centre
//! azimuth); a pipe is `solid.cylinder` bored by a second one; a hole is a `solid.cylinder`
//! cut through its plate. Each part's `$ref`s count from its own first call.
//!
//! # Saddles (T4.1)
//! On a horizontal vessel's cylinder, at `axial` along the axis; a saddle always stands under the
//! vessel (−Z): `angle_deg` is not used. `height_mm` runs from the underside of the base plate to
//! the shell's bottom (outside). The saddle's contact radius is the shell's outside radius, plus
//! the wear plate's thickness when there is one, and its horns are at `±contact_angle_deg / 2`
//! from the bottom. Parts:
//! - web: a plate across the vessel (normal = the axis), `web_thk_mm`, from the base plate's top
//!   up to the contact arc, as wide as the horns;
//! - ribs: `rib_count` plates along the axis, evenly from horn to horn (one rib: at the bottom),
//!   `rib_thk_mm`, `width_mm` long, each in two halves either side of the web, up to the contact
//!   arc at the rib face nearer the bottom;
//! - base plate: `len_mm` across, `wid_mm` along the axis, `thk_mm`; anchor bolt holes (bolt
//!   diameter + 6 mm) in a row across, midway between the web and the plate's +axis edge;
//! - wear plate: a sector of a cylinder on the shell (inside radius = the shell's outside
//!   radius), `width_mm` along the axis, `angle_deg` about the bottom.

use serde_json::json;
use std::f64::consts::PI;
use triya_stdlib::TableSet;

use crate::emit::{CommandCall, Param, REF, Seg, ident, parameter_prefix};
use crate::eval::{Detail, Profile, Resolved, Station};
use crate::model::{Attachment, AttachmentKind, AxialRef, LegAttach, LegSection, Orientation, VesselDoc};
use crate::nozzle::{V3, add, azimuth, cross, mul, sub, vessel_axis};
use crate::{Result, VesselError};

/// At most this many bodies per attachment (a hostile document cannot make thousands).
pub const MAX_PARTS: usize = 256;

/// Clearance of an anchor bolt hole over the bolt (mm).
pub const BOLT_CLEARANCE_MM: f64 = 6.0;

/// One body of a support: what it is, the commands that build it, its volume.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    /// The body's name (`Vessel S1 web`).
    pub name: String,
    /// For the BOM: `web plate 12 thk`.
    pub what: String,
    pub calls: Vec<CommandCall>,
    /// mm³, computed without the kernel.
    pub volume: f64,
    /// A simpler body to build in its place when the kernel fails on this one.
    pub fallback: Option<Box<Part>>,
}

/// A support or detail, resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportGeom {
    pub id: String,
    pub kind: &'static str,
    pub host: String,
    pub parts: Vec<Part>,
    pub params: Vec<Param>,
    pub warnings: Vec<String>,
    /// A dimension came from a table row not marked verified.
    pub unverified_table: bool,
}

impl SupportGeom {
    pub fn volume(&self) -> f64 {
        self.parts.iter().map(|p| p.volume).sum()
    }
}

fn invalid(a: &Attachment, msg: impl std::fmt::Display) -> VesselError {
    VesselError::Invalid(format!("{} `{}`: {msg}", a.kind.name(), a.id))
}

type P2 = [f64; 2];

/// A closed polygon through `pts`.
pub fn polygon(pts: &[P2]) -> Vec<Seg> {
    (0..pts.len()).filter_map(|i| Some(Seg::Line(*pts.get(i)?, *pts.get((i + 1) % pts.len())?))).collect()
}

/// A rectangle from `a` to `b` (opposite corners).
pub fn rect(a: P2, b: P2) -> Vec<Seg> {
    polygon(&[a, [b[0], a[1]], b, [a[0], b[1]]])
}

/// The loop sampled finely (arcs the short way round, as `emit` draws them).
fn sample(segs: &[Seg]) -> Vec<P2> {
    let mut out = Vec::new();
    for s in segs {
        match *s {
            Seg::Line(a, _) => out.push(a),
            Seg::Arc { c, a, b } => {
                let (t0, t1) = ((a[1] - c[1]).atan2(a[0] - c[0]), (b[1] - c[1]).atan2(b[0] - c[0]));
                let mut d = t1 - t0;
                while d > PI {
                    d -= 2.0 * PI;
                }
                while d <= -PI {
                    d += 2.0 * PI;
                }
                let r = (a[0] - c[0]).hypot(a[1] - c[1]);
                let n = 2048;
                for i in 0..n {
                    let t = t0 + d * f64::from(i) / f64::from(n);
                    out.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
                }
            }
            Seg::QuarterEllipse { a, b, apex } => {
                // The rational quadratic with weight √2/2 (an exact quarter ellipse).
                let w = std::f64::consts::FRAC_1_SQRT_2;
                let n = 2048;
                for i in 0..n {
                    let t = f64::from(i) / f64::from(n);
                    let (b0, b1, b2) = ((1.0 - t).powi(2), 2.0 * t * (1.0 - t) * w, t * t);
                    let d = b0 + b1 + b2;
                    out.push([(a[0] * b0 + apex[0] * b1 + b[0] * b2) / d, (a[1] * b0 + apex[1] * b1 + b[1] * b2) / d]);
                }
            }
        }
    }
    out
}

/// The loop's area and its first moment about the sketch y axis (`∫ x dA`), both unsigned
/// (the loop may run either way).
pub fn area_moment(segs: &[Seg]) -> (f64, f64) {
    let p = sample(segs);
    let (mut a, mut m) = (0.0, 0.0);
    for i in 0..p.len() {
        let (Some(u), Some(v)) = (p.get(i), p.get((i + 1) % p.len())) else { continue };
        let cr = u[0] * v[1] - v[0] * u[1];
        a += cr;
        m += (u[0] + v[0]) * cr;
    }
    let sign = if a < 0.0 { -1.0 } else { 1.0 };
    (sign * a / 2.0, sign * m / 6.0)
}

fn finite(v: &[f64]) -> bool {
    v.iter().all(|x| x.is_finite())
}

/// A plate: `segs` sketched on the plane `(origin, x, y)` and extruded `thk` along `x × y`,
/// about the plane (`symmetric`) or from it.
pub fn plate(name: &str, what: &str, origin: V3, x: V3, y: V3, segs: &[Seg], thk: f64, symmetric: bool) -> Part {
    let mut calls = vec![CommandCall {
        id: "sketch.create".into(),
        params: json!({"name": format!("{name} sketch"), "plane": {"origin": origin, "x_dir": x, "y_dir": y}}),
    }];
    calls.extend(segs.iter().map(crate::emit::draw));
    calls.push(CommandCall { id: "sketch.finish".into(), params: json!({}) });
    calls.push(CommandCall {
        id: "solid.extrude".into(),
        params: json!({"sketch": format!("{REF}0.sketch"), "distance": thk, "direction": if symmetric { "symmetric" } else { "positive" },
            "operation": "new", "name": name, "body_name": name}),
    });
    let (a, _) = area_moment(segs);
    Part { name: name.into(), what: what.into(), calls, volume: a * thk, fallback: None }
}

/// A ring (or a sector of one, `angle_deg` < 360 centred on `radial`): `segs` in (distance from
/// the vessel axis, axial) on the plane through the axis and `radial`, revolved about the axis.
pub fn ring(name: &str, what: &str, axis: V3, radial: V3, segs: &[Seg], angle_deg: f64) -> Part {
    let mut calls = vec![CommandCall {
        id: "sketch.create".into(),
        params: json!({"name": format!("{name} sketch"), "plane": {"origin": [0.0, 0.0, 0.0], "x_dir": radial, "y_dir": axis}}),
    }];
    calls.extend(segs.iter().map(crate::emit::draw));
    calls.push(CommandCall { id: "sketch.finish".into(), params: json!({}) });
    let mut p = json!({"sketch": format!("{REF}0.sketch"), "axis": "y", "angle": format!("{angle_deg} deg"), "operation": "new", "name": name,
        "body_name": name});
    if angle_deg < 360.0 {
        p["direction"] = json!("symmetric");
    }
    calls.push(CommandCall { id: "solid.revolve".into(), params: p });
    let (_, m) = area_moment(segs);
    Part { name: name.into(), what: what.into(), calls, volume: m * angle_deg.to_radians(), fallback: None }
}

/// A hole of `dia` through a plate part, along `dir` from `from` over `len` (with 1 mm to spare
/// at both ends).
pub fn hole(part: &mut Part, label: &str, from: V3, dir: V3, len: f64, dia: f64) {
    part.calls.push(CommandCall {
        id: "solid.cylinder".into(),
        params: json!({"radius": dia / 2.0, "height": len + 2.0, "base": add(from, mul(dir, -1.0)), "axis": dir, "operation": "cut",
            "targets": [part.name], "name": format!("{} {label}", part.name)}),
    });
    part.volume -= PI * dia * dia / 4.0 * len;
}

/// The axial position of `a` on `host`: from the datum or from the host's start.
pub fn axial_z(a: &Attachment, host: &Station) -> f64 {
    match a.axial.reference {
        AxialRef::Datum => a.axial.mm,
        AxialRef::HostFrom => host.z_from + a.axial.mm,
    }
}

/// The body name of part `part` of attachment `id`.
pub fn body_name(id: &str, part: &str) -> String {
    format!("Vessel {id} {part}")
}

/// Attachment `a` (not a nozzle) of `doc`, against the resolved stack.
pub fn resolve(doc: &VesselDoc, r: &Resolved, a: &Attachment, tables: &TableSet) -> Result<SupportGeom> {
    let host = r.station(&a.host).ok_or_else(|| invalid(a, format!("no element `{}`", a.host)))?;
    let mut g = SupportGeom {
        id: a.id.clone(),
        kind: a.kind.name(),
        host: host.id.clone(),
        parts: Vec::new(),
        params: Vec::new(),
        warnings: Vec::new(),
        unverified_table: false,
    };
    match &a.kind {
        AttachmentKind::Saddle { .. } => saddle(doc, r, a, host, &mut g)?,
        AttachmentKind::Leg { .. } => legs(doc, r, a, host, tables, &mut g)?,
        AttachmentKind::Bracket { .. } => bracket(doc, r, a, host, &mut g)?,
        AttachmentKind::AnchorChair { .. } => anchor_chair(doc, a, host, &mut g)?,
        AttachmentKind::Nozzle { .. } => return Err(invalid(a, "nozzles are built by `nozzle`")),
        other => return Err(VesselError::NotSupportedYet(format!("{} `{}`", other.name(), a.id))),
    }
    if g.parts.len() > MAX_PARTS {
        return Err(invalid(a, format!("{} bodies; at most {MAX_PARTS} per attachment", g.parts.len())));
    }
    for p in &g.parts {
        let ok =
            p.volume.is_finite() && p.volume > 0.0 && p.calls.iter().all(|c| !serde_json::to_string(&c.params).unwrap_or_default().contains("null"));
        if !ok {
            return Err(invalid(a, format!("part `{}` is degenerate (check its dimensions)", p.name)));
        }
    }
    Ok(g)
}

/// Parameters of an attachment: `V<vessel>_<id>_<field>`.
fn push_params(doc: &VesselDoc, a: &Attachment, g: &mut SupportGeom, fields: &[(&str, f64, &'static str, &str)]) {
    let prefix = parameter_prefix(&doc.id);
    let id = ident(&a.id);
    for (field, value, unit, what) in fields {
        if value.is_finite() {
            g.params.push(Param {
                name: format!("{prefix}_{id}_{field}"),
                value: *value,
                unit,
                comment: format!("{} {}: {what}", a.kind.name(), a.id),
            });
        }
    }
}

fn saddle(doc: &VesselDoc, r: &Resolved, a: &Attachment, host: &Station, g: &mut SupportGeom) -> Result<()> {
    let AttachmentKind::Saddle { width_mm, height_mm, contact_angle_deg, web_thk_mm, rib_count, rib_thk_mm, base_plate, wear_plate, anchor_bolts } =
        &a.kind
    else {
        return Err(invalid(a, "not a saddle"));
    };
    if r.orientation != Orientation::Horizontal {
        return Err(invalid(a, "saddles carry horizontal vessels (`global.orientation: horizontal`)"));
    }
    if !(host.kind == "cylinder" && host.profile == Profile::Straight) {
        return Err(invalid(a, format!("the host `{}` must be a cylinder (it is a {})", host.id, host.kind)));
    }
    if *rib_count > 64 {
        return Err(invalid(a, "at most 64 ribs"));
    }
    let x0 = axial_z(a, host);
    if !(x0 >= host.z_from && x0 <= host.z_to) {
        return Err(invalid(a, format!("at {x0} mm it is not on its host `{}` ({} … {})", host.id, host.z_from, host.z_to)));
    }
    let ro = host.r_from + host.thickness;
    let wear_t = wear_plate.map_or(0.0, |w| w.thk_mm);
    let rc = ro + wear_t;
    let half = contact_angle_deg.to_radians() / 2.0;
    let (h, zh) = (rc * half.sin(), -rc * half.cos());
    let z_under = -ro - height_mm;
    let zb = z_under + base_plate.thk_mm;
    if !(zb < -rc - 1.0) {
        return Err(invalid(
            a,
            format!("height {height_mm} mm leaves no room for the web under the vessel (base plate {} thk, wear plate {wear_t})", base_plate.thk_mm),
        ));
    }
    if !(width_mm > web_thk_mm) {
        return Err(invalid(a, format!("width {width_mm} mm must exceed the web's thickness {web_thk_mm}")));
    }
    if 2.0 * h > base_plate.len_mm + 1e-9 {
        g.warnings.push(format!("saddle `{}`: the horns span {:.0} mm, more than the base plate's length {} mm", a.id, 2.0 * h, base_plate.len_mm));
    }
    if wear_plate.is_some_and(|w| w.angle_deg < *contact_angle_deg) {
        g.warnings.push(format!("saddle `{}`: the wear plate's angle is smaller than the contact angle", a.id));
    }
    let (ex, ey, ez) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
    // Web: across the vessel at x0, sketch (y, z).
    let web = vec![
        Seg::Line([-h, zb], [h, zb]),
        Seg::Line([h, zb], [h, zh]),
        Seg::Arc { c: [0.0, 0.0], a: [h, zh], b: [0.0, -rc] },
        Seg::Arc { c: [0.0, 0.0], a: [0.0, -rc], b: [-h, zh] },
        Seg::Line([-h, zh], [-h, zb]),
    ];
    g.parts.push(plate(&body_name(&a.id, "web"), &format!("saddle web {web_thk_mm} thk"), [x0, 0.0, 0.0], ey, ez, &web, *web_thk_mm, true));
    // Ribs: along the axis, two halves either side of the web.
    let n = *rib_count;
    let tr = *rib_thk_mm;
    for i in 0..n {
        let y = if n == 1 { 0.0 } else { -(h - tr / 2.0) + f64::from(i) * 2.0 * (h - tr / 2.0) / f64::from(n - 1) };
        let near = (y.abs() - tr / 2.0).max(0.0);
        let top = -(rc * rc - near * near).max(0.0).sqrt();
        if !(top > zb + 1.0) {
            return Err(invalid(a, format!("rib {} has no height (rib thickness {tr} mm against horns at ±{h:.0})", i + 1)));
        }
        for (side, x_in, x_out) in [("a", x0 + web_thk_mm / 2.0, x0 + width_mm / 2.0), ("b", x0 - width_mm / 2.0, x0 - web_thk_mm / 2.0)] {
            g.parts.push(plate(
                &body_name(&a.id, &format!("rib {}{side}", i + 1)),
                &format!("saddle rib {tr} thk"),
                [0.0, y, 0.0],
                ex,
                ez,
                &rect([x_in, zb], [x_out, top]),
                tr,
                true,
            ));
        }
    }
    // Base plate, with the anchor bolt holes.
    let (bl, bw, bt) = (base_plate.len_mm, base_plate.wid_mm, base_plate.thk_mm);
    let mut base = plate(
        &body_name(&a.id, "base plate"),
        &format!("saddle base plate {bt} thk"),
        [0.0, 0.0, z_under],
        ex,
        ey,
        &rect([x0 - bw / 2.0, -bl / 2.0], [x0 + bw / 2.0, bl / 2.0]),
        bt,
        false,
    );
    if let Some(b) = anchor_bolts {
        let d = b.dia_mm + BOLT_CLEARANCE_MM;
        let hx = x0 + (web_thk_mm / 2.0 + bw / 2.0) / 2.0;
        let span = bl / 2.0 - d;
        if b.count > 16 || !(span > 0.0) || bw / 2.0 - web_thk_mm / 2.0 < d {
            return Err(invalid(a, format!("{} anchor bolts of {} mm do not fit the base plate", b.count, b.dia_mm)));
        }
        for k in 0..b.count {
            let y = if b.count == 1 { 0.0 } else { -span + f64::from(k) * 2.0 * span / f64::from(b.count - 1) };
            hole(&mut base, &format!("bolt hole {}", k + 1), [hx, y, z_under], ez, bt, d);
        }
    }
    g.parts.push(base);
    // Wear plate: a sector of a cylinder on the shell, about the bottom.
    if let Some(w) = wear_plate {
        if !(x0 - w.width_mm / 2.0 >= host.z_from - 1e-6 && x0 + w.width_mm / 2.0 <= host.z_to + 1e-6) {
            g.warnings.push(format!("saddle `{}`: the wear plate runs past its host `{}`", a.id, host.id));
        }
        let segs = rect([ro, x0 - w.width_mm / 2.0], [ro + w.thk_mm, x0 + w.width_mm / 2.0]);
        g.parts.push(ring(&body_name(&a.id, "wear plate"), &format!("wear plate {} thk", w.thk_mm), ex, [0.0, 0.0, -1.0], &segs, w.angle_deg));
    }
    let mut fields = vec![
        ("position", x0, "mm", "centre along the axis, from the datum"),
        ("height", *height_mm, "mm", "base underside to the shell's bottom"),
        ("width", *width_mm, "mm", "rib length along the axis"),
        ("contact_angle", *contact_angle_deg, "deg", "contact angle"),
        ("web_thk", *web_thk_mm, "mm", "web thickness"),
        ("rib_count", f64::from(*rib_count), "", "rib count"),
        ("rib_thk", *rib_thk_mm, "mm", "rib thickness"),
        ("base_len", bl, "mm", "base plate length across"),
        ("base_wid", bw, "mm", "base plate width along the axis"),
        ("base_thk", bt, "mm", "base plate thickness"),
    ];
    if let Some(w) = wear_plate {
        fields.extend([
            ("wear_thk", w.thk_mm, "mm", "wear plate thickness"),
            ("wear_width", w.width_mm, "mm", "wear plate width along the axis"),
            ("wear_angle", w.angle_deg, "deg", "wear plate angle"),
        ]);
    }
    push_params(doc, a, g, &fields);
    if !finite(&[x0, ro, rc, h, zh, zb]) {
        return Err(invalid(a, "non-finite geometry"));
    }
    Ok(())
}

/// The tolerance (mm³) on a part's measured volume: the tessellated measure is good to about
/// 1e-4 on these bodies; 0.5 % flags a kernel result that is wrong, not one that is coarse.
pub fn tolerance(expected: f64) -> f64 {
    0.005 * expected.abs() + 1.0
}

/// The local frame of an attachment at azimuth `deg`: the vessel axis, the outward radial and
/// the circumferential tangent (`axis × radial`).
fn frame_at(doc: &VesselDoc, deg: f64) -> (V3, V3, V3) {
    let ax = vessel_axis(doc.global.orientation);
    let u = azimuth(&doc.global, deg);
    (ax, u, cross(ax, u))
}

/// `count` azimuths evenly from `start` (degrees).
fn azimuths(start: f64, count: u32) -> Vec<f64> {
    (0..count).map(|k| start + 360.0 * f64::from(k) / f64::from(count.max(1))).collect()
}

/// The host's outside radius at `z` (heads and cones from their surfaces), `None` off the host.
pub fn outside_radius(st: &Station, z: f64) -> Option<f64> {
    match (st.kind, st.profile) {
        ("cylinder" | "skirt" | "body_flange", Profile::Straight) => Some(st.r_from + st.thickness),
        ("cone", Profile::Conical { .. }) => {
            let ri = st.radius_at(z)?;
            let a = st.half_apex_deg.unwrap_or(0.0).to_radians();
            Some(ri + st.thickness / a.cos().max(0.1))
        }
        ("head", Profile::Head { .. }) => {
            let r_max = st.r_from.max(st.r_to) + st.thickness;
            let f = |rho: f64| crate::nozzle::surface(st, rho, z, st.thickness);
            // Outside the surface at r_max; inside at the axis when z is within the dish.
            if !(f(0.0)? < 0.0) {
                return None;
            }
            let (mut lo, mut hi) = (0.0, r_max + 1.0);
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if f(mid)? < 0.0 { lo = mid } else { hi = mid }
            }
            Some(0.5 * (lo + hi))
        }
        _ => None,
    }
}

/// The largest outside radius of the host over `z0 … z1` (a plate's inner edge there clears it).
fn outside_radius_over(st: &Station, z0: f64, z1: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for i in 0..=32 {
        let z = z0 + (z1 - z0) * f64::from(i) / 32.0;
        if let Some(r) = outside_radius(st, z.clamp(st.z_from, st.z_to)) {
            best = Some(best.map_or(r, |b| b.max(r)));
        }
    }
    best
}

/// `"ISA 100x100x10"`, `"L75x75x8"`, `"75 x 75 x 8"` → (a, b, t) mm.
pub fn angle_size(s: &str) -> Option<(f64, f64, f64)> {
    let nums: Vec<f64> = s
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|p| !p.is_empty() && p.chars().any(|c| c.is_ascii_digit()))
        .filter_map(|p| p.parse().ok())
        .collect();
    match nums.as_slice() {
        [a, b, t] if *a > 0.0 && *b > 0.0 && *t > 0.0 && *t < a.min(*b) && a.max(*b) <= 1000.0 => Some((*a, *b, *t)),
        _ => None,
    }
}

/// A solid rod from `from` along `dir` (unit), `len` long.
fn rod(name: &str, what: &str, from: V3, dir: V3, radius: f64, len: f64) -> Part {
    Part {
        name: name.into(),
        what: what.into(),
        calls: vec![CommandCall {
            id: "solid.cylinder".into(),
            params: json!({"radius": radius, "height": len, "base": from, "axis": dir, "operation": "new", "name": name, "body_name": name}),
        }],
        volume: PI * radius * radius * len,
        fallback: None,
    }
}

fn legs(doc: &VesselDoc, r: &Resolved, a: &Attachment, host: &Station, tables: &TableSet, g: &mut SupportGeom) -> Result<()> {
    let AttachmentKind::Leg { count, section, length_mm, attach, base_plate, start_angle_deg } = &a.kind else {
        return Err(invalid(a, "not a leg"));
    };
    if r.orientation != Orientation::Vertical {
        return Err(invalid(a, "legs carry vertical vessels"));
    }
    if *count > 64 {
        return Err(invalid(a, "at most 64 legs"));
    }
    let start = if *start_angle_deg != 0.0 { *start_angle_deg } else { a.angle_deg };
    // The section's footprint about the leg's own axis: a pipe (radius), or an angle's (a, b, t).
    enum Sec {
        Pipe { od: f64, wall: f64 },
        Angle { a: f64, b: f64, t: f64 },
    }
    let sec = match section {
        LegSection::Pipe { nps, schedule } => {
            let row = tables.pipe(nps, schedule).ok_or_else(|| invalid(a, format!("no pipe NPS {nps} schedule {schedule} in the tables")))?;
            if !row.verified.is_trusted() {
                g.unverified_table = true;
            }
            Sec::Pipe { od: row.od, wall: row.wall }
        }
        LegSection::Angle { size } => {
            let (aa, bb, t) =
                angle_size(size).ok_or_else(|| invalid(a, format!("cannot read the angle size `{size}` (write it as `ISA 100x100x10`)")))?;
            Sec::Angle { a: aa, b: bb, t }
        }
        LegSection::Channel { size } => {
            return Err(VesselError::NotSupportedYet(format!("leg `{}`: channel sections (`{size}`; no open channel table in the beta)", a.id)));
        }
    };
    let ri = host.r_from.max(host.r_to);
    let ro = ri + host.thickness;
    for (k, deg) in azimuths(start, *count).into_iter().enumerate() {
        let (ax, u, t) = frame_at(doc, deg);
        let n = k + 1;
        // Where the leg's axis is (distance from the vessel axis) and its top.
        let (rho, top, what) = match (&sec, attach) {
            (Sec::Pipe { od, .. }, LegAttach::Shell) => (ro + od / 2.0, axial_z(a, host), format!("leg pipe OD {od}")),
            (Sec::Angle { a: aa, b: bb, .. }, LegAttach::Shell) => (ro, axial_z(a, host), format!("leg angle {aa}x{bb}")),
            (s, LegAttach::Head) => {
                if host.kind != "head" {
                    return Err(invalid(a, "`attach: head` needs a head as the host"));
                }
                let (half_w, centre) = match s {
                    Sec::Pipe { od, .. } => (od / 2.0, ri - od / 2.0),
                    Sec::Angle { a: aa, b: bb, .. } => (aa.max(*bb) / std::f64::consts::SQRT_2, ri - aa.max(*bb) / std::f64::consts::SQRT_2),
                };
                if !(centre - half_w > 0.0) {
                    return Err(invalid(a, "the leg is wider than the head"));
                }
                // Under the head: the top is square at the lowest point of the head's outside
                // under the leg's footprint (its inner edge).
                let top = head_surface_z(host, centre - half_w).ok_or_else(|| invalid(a, "cannot find the head's outside under the leg"))?;
                let what = match s {
                    Sec::Pipe { od, .. } => format!("leg pipe OD {od}"),
                    Sec::Angle { a: aa, b: bb, .. } => format!("leg angle {aa}x{bb}"),
                };
                (centre, top, what)
            }
        };
        let bottom = top - length_mm;
        let name = body_name(&a.id, &format!("leg {n}"));
        let base_centre = match &sec {
            Sec::Pipe { od, wall } => {
                let mut leg = rod(&name, &format!("{what} × {wall}"), add(mul(ax, bottom), mul(u, rho)), ax, od / 2.0, *length_mm);
                leg.calls.push(CommandCall {
                    id: "solid.cylinder".into(),
                    params: json!({"radius": od / 2.0 - wall, "height": length_mm + 2.0, "base": add(mul(ax, bottom - 1.0), mul(u, rho)), "axis": ax,
                        "operation": "cut", "targets": [name], "name": format!("{name} bore")}),
                });
                leg.volume -= PI * (od / 2.0 - wall).powi(2) * length_mm;
                g.parts.push(leg);
                mul(u, rho)
            }
            Sec::Angle { a: aa, b: bb, t: tt } => {
                // Heel toward the vessel, flanges at ±45° from the radial, outward.
                let s2 = std::f64::consts::FRAC_1_SQRT_2;
                let d1 = mul(add(u, t), s2);
                let d2 = mul(sub(u, t), s2);
                let heel = mul(u, rho);
                let p = |x: f64, y: f64| -> P2 {
                    let w = add(heel, add(mul(d1, x), mul(d2, y)));
                    [w[0], w[1]]
                };
                let pts = [p(0.0, 0.0), p(*aa, 0.0), p(*aa, *tt), p(*tt, *tt), p(*tt, *bb), p(0.0, *bb)];
                g.parts.push(plate(
                    &name,
                    &format!("{what}x{tt}"),
                    [0.0, 0.0, bottom],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    &polygon(&pts),
                    *length_mm,
                    false,
                ));
                add(heel, add(mul(d1, aa / 2.0), mul(d2, bb / 2.0)))
            }
        };
        if let Some(bp) = base_plate {
            let c = base_centre;
            let corner = |x: f64, y: f64| -> P2 {
                let w = add(c, add(mul(u, x), mul(t, y)));
                [w[0], w[1]]
            };
            let (l, w) = (bp.len_mm / 2.0, bp.wid_mm / 2.0);
            let pts = [corner(-l, -w), corner(l, -w), corner(l, w), corner(-l, w)];
            g.parts.push(plate(
                &body_name(&a.id, &format!("leg {n} base plate")),
                &format!("leg base plate {} thk", bp.thk_mm),
                [0.0, 0.0, bottom - bp.thk_mm],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                &polygon(&pts),
                bp.thk_mm,
                false,
            ));
        }
        if k == 0 {
            push_params(
                doc,
                a,
                g,
                &[
                    ("count", f64::from(*count), "", "leg count"),
                    ("start_angle", start, "deg", "azimuth of the first leg"),
                    ("length", *length_mm, "mm", "leg length"),
                    ("top", top, "mm", "top of the legs from the datum"),
                    ("radius", rho, "mm", "leg axis from the vessel axis"),
                ],
            );
        }
    }
    if *attach == LegAttach::Head {
        // A top moulded to the head (the pipe cut by the crown's sphere) builds, but takes the
        // kernel about 70 s a leg (vessel C, 19 Oct): the beta keeps square tops.
        g.warnings.push(format!("leg `{}`: the tops are square under the head (a head-moulded top is drawn on the sheet)", a.id));
    }
    Ok(())
}

/// The axial position of a head's outside surface at `rho` from the axis.
fn head_surface_z(st: &Station, rho: f64) -> Option<f64> {
    let Profile::Head { side, sf, dish, .. } = st.profile else { return None };
    let (dir, tl) = match side {
        crate::eval::HeadSide::CrownAtStart => (-1.0, st.z_to - sf),
        crate::eval::HeadSide::CrownAtEnd => (1.0, st.z_from + sf),
    };
    let f = |w: f64| crate::nozzle::surface(st, rho, tl + dir * w, st.thickness);
    // Inside the surface at w = 0 (the tangent line), outside beyond the crown.
    let (mut lo, mut hi) = (0.0, dish + st.thickness + 1.0);
    if !(f(lo)? < 0.0 && f(hi)? >= 0.0) {
        return None;
    }
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if f(mid)? < 0.0 { lo = mid } else { hi = mid }
    }
    Some(tl + dir * 0.5 * (lo + hi))
}

/// A vertical plate in the plane of the vessel axis and the radial `u`, offset `tau` along the
/// tangent `t`: `pts` in (distance along `u`, axial).
fn radial_plate(name: &str, what: &str, ax: V3, u: V3, t: V3, tau: f64, pts: &[P2], thk: f64) -> Part {
    plate(name, what, mul(t, tau), u, ax, &polygon(pts), thk, true)
}

/// A plate square to the vessel axis at `z` (its underside), `pts` in (along `u`, along `t`).
fn cross_plate(name: &str, what: &str, ax: V3, u: V3, t: V3, z: f64, pts: &[P2], thk: f64) -> Part {
    plate(name, what, mul(ax, z), u, t, &polygon(pts), thk, false)
}

fn bracket(doc: &VesselDoc, r: &Resolved, a: &Attachment, host: &Station, g: &mut SupportGeom) -> Result<()> {
    let AttachmentKind::Bracket { count, gusset, top_plate, bottom_plate, bolt_holes } = &a.kind else {
        return Err(invalid(a, "not a bracket"));
    };
    if r.orientation != Orientation::Vertical {
        return Err(invalid(a, "bracket supports carry vertical vessels"));
    }
    if *count > 64 {
        return Err(invalid(a, "at most 64 brackets"));
    }
    let z_top = axial_z(a, host);
    let z_under = z_top - top_plate.thk_mm;
    let z_toe = z_under - gusset.height_mm;
    let bw = bottom_plate.map_or(0.0, |b| b.wid_mm);
    let ro = outside_radius_over(host, z_toe - bottom_plate.map_or(0.0, |b| b.thk_mm), z_top)
        .ok_or_else(|| invalid(a, format!("not on its host `{}` at {z_top} mm", host.id)))?;
    if gusset.width_mm > top_plate.len_mm + 1e-9 || bw > gusset.width_mm + 1e-9 {
        return Err(invalid(a, "the gussets must fit under the top plate, the bottom plate under the gussets"));
    }
    if !(top_plate.wid_mm > 2.0 * gusset.thk_mm) {
        return Err(invalid(a, "the top plate is narrower than its two gussets"));
    }
    let tau = top_plate.wid_mm / 2.0 - gusset.thk_mm / 2.0;
    for (k, deg) in azimuths(a.angle_deg, *count).into_iter().enumerate() {
        let (ax, u, t) = frame_at(doc, deg);
        let n = k + 1;
        let mut gp = vec![[ro, z_under], [ro + gusset.width_mm, z_under], [ro + bw, z_toe], [ro, z_toe]];
        gp.dedup();
        for (side, off) in [("a", tau), ("b", -tau)] {
            g.parts.push(radial_plate(
                &body_name(&a.id, &format!("{n} gusset {side}")),
                &format!("bracket gusset {} thk", gusset.thk_mm),
                ax,
                u,
                t,
                off,
                &gp,
                gusset.thk_mm,
            ));
        }
        let (l, w) = (top_plate.len_mm, top_plate.wid_mm / 2.0);
        let mut top = cross_plate(
            &body_name(&a.id, &format!("{n} top plate")),
            &format!("bracket top plate {} thk", top_plate.thk_mm),
            ax,
            u,
            t,
            z_under,
            &[[ro, -w], [ro + l, -w], [ro + l, w], [ro, w]],
            top_plate.thk_mm,
        );
        if let Some(h) = bolt_holes {
            let gap = top_plate.wid_mm - 2.0 * gusset.thk_mm;
            let outboard = l - gusset.width_mm >= h.dia_mm + 10.0;
            let rho = if outboard { ro + (gusset.width_mm + l) / 2.0 } else { ro + l / 2.0 };
            let span = if outboard { top_plate.wid_mm / 2.0 - h.dia_mm } else { gap / 2.0 - h.dia_mm };
            if h.count > 8 || !(span >= 0.0) || (h.count > 1 && !(2.0 * span / f64::from(h.count - 1) > h.dia_mm)) {
                return Err(invalid(a, format!("{} bolt holes of {} mm do not fit the top plate", h.count, h.dia_mm)));
            }
            for j in 0..h.count {
                let y = if h.count == 1 { 0.0 } else { -span + f64::from(j) * 2.0 * span / f64::from(h.count - 1) };
                hole(&mut top, &format!("bolt hole {}", j + 1), add(mul(ax, z_under), add(mul(u, rho), mul(t, y))), ax, top_plate.thk_mm, h.dia_mm);
            }
        }
        g.parts.push(top);
        if let Some(b) = bottom_plate {
            let w = b.len_mm / 2.0;
            g.parts.push(cross_plate(
                &body_name(&a.id, &format!("{n} bottom plate")),
                &format!("bracket bottom plate {} thk", b.thk_mm),
                ax,
                u,
                t,
                z_toe - b.thk_mm,
                &[[ro, -w], [ro + b.wid_mm, -w], [ro + b.wid_mm, w], [ro, w]],
                b.thk_mm,
            ));
        }
    }
    push_params(
        doc,
        a,
        g,
        &[
            ("count", f64::from(*count), "", "bracket count"),
            ("top", z_top, "mm", "top of the top plate from the datum"),
            ("gusset_height", gusset.height_mm, "mm", "gusset height"),
            ("gusset_width", gusset.width_mm, "mm", "gusset width"),
            ("gusset_thk", gusset.thk_mm, "mm", "gusset thickness"),
            ("top_len", top_plate.len_mm, "mm", "top plate radial length"),
            ("top_wid", top_plate.wid_mm, "mm", "top plate width"),
            ("top_thk", top_plate.thk_mm, "mm", "top plate thickness"),
        ],
    );
    Ok(())
}

fn anchor_chair(doc: &VesselDoc, a: &Attachment, host: &Station, g: &mut SupportGeom) -> Result<()> {
    let AttachmentKind::AnchorChair { count, height_mm, width_mm, gusset_thk_mm, top_plate_thk_mm, bolt_dia_mm } = &a.kind else {
        return Err(invalid(a, "not an anchor chair"));
    };
    let Detail::BaseRing { od, thickness: ring_t, .. } = host.detail else {
        return Err(invalid(a, format!("the host `{}` must be a skirt with its base ring", host.id)));
    };
    if host.kind != "skirt" {
        return Err(invalid(a, format!("the host `{}` must be a skirt", host.id)));
    }
    if *count > 64 {
        return Err(invalid(a, "at most 64 chairs"));
    }
    let rs = host.r_from + host.thickness;
    let r_out = od / 2.0;
    if !(r_out > rs + 1.0) {
        return Err(invalid(a, "the base ring does not stand out from the skirt"));
    }
    let z_ring = host.z_from + ring_t;
    let z_top = z_ring + height_mm;
    let z_plate = z_top - top_plate_thk_mm;
    if !(z_plate > z_ring) {
        return Err(invalid(a, "the chair is lower than its top plate is thick"));
    }
    let hole_d = bolt_dia_mm + BOLT_CLEARANCE_MM;
    let tau = width_mm / 2.0 - gusset_thk_mm / 2.0;
    if !(tau - gusset_thk_mm / 2.0 > hole_d / 2.0) || !(r_out - rs > hole_d) {
        return Err(invalid(a, format!("a {bolt_dia_mm} mm bolt does not fit between the gussets and the skirt")));
    }
    let rho_b = (rs + r_out) / 2.0;
    for (k, deg) in azimuths(a.angle_deg, *count).into_iter().enumerate() {
        let (ax, u, t) = frame_at(doc, deg);
        let n = k + 1;
        let gp = [[rs, z_ring], [r_out, z_ring], [r_out, z_plate], [rs, z_plate]];
        for (side, off) in [("a", tau), ("b", -tau)] {
            g.parts.push(radial_plate(
                &body_name(&a.id, &format!("{n} gusset {side}")),
                &format!("anchor chair gusset {gusset_thk_mm} thk"),
                ax,
                u,
                t,
                off,
                &gp,
                *gusset_thk_mm,
            ));
        }
        let w = width_mm / 2.0;
        let mut top = cross_plate(
            &body_name(&a.id, &format!("{n} top plate")),
            &format!("anchor chair top plate {top_plate_thk_mm} thk"),
            ax,
            u,
            t,
            z_plate,
            &[[rs, -w], [r_out, -w], [r_out, w], [rs, w]],
            *top_plate_thk_mm,
        );
        hole(&mut top, "bolt hole", add(mul(ax, z_plate), mul(u, rho_b)), ax, *top_plate_thk_mm, hole_d);
        g.parts.push(top);
    }
    push_params(
        doc,
        a,
        g,
        &[
            ("count", f64::from(*count), "", "chair count"),
            ("height", *height_mm, "mm", "base ring top to the chair top"),
            ("width", *width_mm, "mm", "chair width"),
            ("bolt_circle", 2.0 * rho_b, "mm", "anchor bolt circle diameter"),
            ("bolt_dia", *bolt_dia_mm, "mm", "anchor bolt diameter"),
        ],
    );
    Ok(())
}
