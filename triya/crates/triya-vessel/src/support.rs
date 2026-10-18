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
use crate::eval::{Profile, Resolved, Station};
use crate::model::{Attachment, AttachmentKind, AxialRef, Orientation, VesselDoc};
use crate::nozzle::{V3, add, mul};
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
    Part { name: name.into(), what: what.into(), calls, volume: a * thk }
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
    Part { name: name.into(), what: what.into(), calls, volume: m * angle_deg.to_radians() }
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
pub fn resolve(doc: &VesselDoc, r: &Resolved, a: &Attachment, _tables: &TableSet) -> Result<SupportGeom> {
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
