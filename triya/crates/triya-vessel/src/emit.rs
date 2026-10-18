// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The plan: ordinary engine commands that build the resolved vessel.
//!
//! Every shell element becomes a closed half-profile in a sketch on the plane through the axis
//! (sketch x = radius, sketch y = position along the axis), revolved 360° about the axis into
//! its own body (`Vessel <id>`). The ids and parameter shapes are upstream's
//! (`crates/engine/src/cmd/sketch.rs`, `sketch_create.rs`, `features.rs`):
//!
//! | step | command | params |
//! |---|---|---|
//! | 0 | `sketch.create` | `{name, plane: {origin: [0,0,0], x_dir, y_dir}}` → `{sketch: id, …}` |
//! | 1… | `sketch.line` / `sketch.arc.center_point` / `sketch.conic` / `sketch.rectangle.two_point` | the profile, segment by segment |
//! | n | `sketch.finish` | `{}` |
//! | n+1 | `solid.revolve` | `{sketch: $ref:0.sketch, axis: "Z" (or "X"), angle: "360 deg", operation: "new", name, body_name}` |
//!
//! The plane is explicit: upstream's named `XZ` plane has its sketch y along −Z
//! (`solidcraft_geom::Plane::XZ`); `{x_dir: +X, y_dir: +Z}` (vertical) or `{x_dir: +Z, y_dir: +X}`
//! (horizontal) keeps sketch y = the vessel's axial coordinate. Segments share endpoints by
//! value, and upstream's `connect` (default on) makes coincident points one point, so the
//! profile closes.
//!
//! # Profiles
//! - cylinder: rectangle from the inside to the outside radius.
//! - head: straight flange lines, then the dish, inside and outside: a semi-ellipse is a quarter
//!   conic with `rho = √2 − 1` (a rational quadratic with weight `√2/2`, which is exactly an
//!   ellipse arc: the affine image of a quarter circle); a hemisphere is an arc; a torispherical
//!   head is a knuckle arc (centre `(R − r, TL)`) and a crown arc (centre on the axis `c =
//!   √((L − r)² − (R − r)²)` behind the TL), meeting on the line through both centres. The
//!   outside curve uses the same centres with `R + t`, `r + t`, `L + t` (`vessel-schema.md` §3.1),
//!   so the outside of a 2:1 head is the ellipse with semi-axes `R + t`, `D/4 + t`.
//! - cone: the inside follows the knuckle arcs and the line tangent to them (`eval::ConeFit`);
//!   the outside is the concentric offset (large knuckle `+ t`, small knuckle `− t`, line `+ t`);
//!   without a knuckle the end is where the offset line meets the end plane.
//! - flat cover: a disc (rectangle from the axis) out to the adjacent shell's outside radius.
//!
//! A later step may need an earlier reply (the sketch's id): a string parameter
//! `"$ref:<step>.<field>"` is replaced by that field of that step's reply when the plan runs
//! ([`substitute`]).
//!
//! # Parameters
//! [`parameters`] lists every dimension as a user parameter `V<vessel>_<element>_<field>` (mm),
//! which `vessel.regenerate` writes to upstream's parameter table so users can read (and later
//! adopt) them.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::eval::{HeadShape, HeadSide, Profile, Resolved, Station};
use crate::model::Orientation;
use crate::{Result, VesselError};

/// One engine command to run, in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandCall {
    pub id: String,
    pub params: Value,
}

/// The prefix of a parameter that refers to an earlier reply.
pub const REF: &str = "$ref:";

/// Most planned steps (a hostile document cannot make the executor loop for long).
pub const MAX_STEPS: usize = 10_000;

/// `rho` of a conic that is exactly a quarter ellipse: weight `w = cos 45°`, `rho = w / (1 + w)`.
pub const QUARTER_ELLIPSE_RHO: f64 = std::f64::consts::SQRT_2 - 1.0;

type P2 = [f64; 2];

/// One piece of a profile, in sketch coordinates (radius, axial).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Line(P2, P2),
    /// A circular arc of less than 180° about `c` between `a` and `b` (either way round).
    Arc {
        c: P2,
        a: P2,
        b: P2,
    },
    /// A quarter ellipse from `a` to `b` whose tangents meet at `apex`.
    QuarterEllipse {
        a: P2,
        b: P2,
        apex: P2,
    },
}

fn sketch_name(st: &Station) -> String {
    format!("Vessel {} profile", st.id)
}

fn body_name(st: &Station) -> String {
    format!("Vessel {}", st.id)
}

/// The commands that draw one segment in the active sketch.
fn draw(seg: &Seg) -> CommandCall {
    match *seg {
        Seg::Line(a, b) => CommandCall { id: "sketch.line".into(), params: json!({"points": [a, b]}) },
        Seg::Arc { c, a, b } => {
            // A circular arc of angle θ is exactly a rational quadratic (a conic) with its apex
            // where the end tangents meet and weight cos(θ/2); upstream's kernel revolves conic
            // profiles that end on the axis reliably, its arcs not always (T2.3 kernel case).
            let (ua, ub) = ([a[0] - c[0], a[1] - c[1]], [b[0] - c[0], b[1] - c[1]]);
            let rad = ua[0].hypot(ua[1]);
            let cos_t = ((ua[0] * ub[0] + ua[1] * ub[1]) / (rad * ub[0].hypot(ub[1]))).clamp(-1.0, 1.0);
            let w = ((1.0 + cos_t) / 2.0).sqrt();
            let (mx, my) = (ua[0] + ub[0], ua[1] + ub[1]);
            let n = mx.hypot(my);
            let apex = if n > 0.0 && w > 0.0 { [c[0] + rad / w * mx / n, c[1] + rad / w * my / n] } else { a };
            CommandCall { id: "sketch.conic".into(), params: json!({"start": a, "end": b, "apex": apex, "rho": w / (1.0 + w)}) }
        }
        Seg::QuarterEllipse { a, b, apex } => {
            CommandCall { id: "sketch.conic".into(), params: json!({"start": a, "end": b, "apex": apex, "rho": QUARTER_ELLIPSE_RHO}) }
        }
    }
}

fn finite(segs: &[Seg]) -> bool {
    segs.iter().all(|s| match s {
        Seg::Line(a, b) => a.iter().chain(b).all(|v| v.is_finite()),
        Seg::Arc { c, a, b } | Seg::QuarterEllipse { a, b, apex: c } => a.iter().chain(b).chain(c).all(|v| v.is_finite()),
    })
}

/// The dish of a head from its tangent line `tl` to the axis, for radius `r_shell` and wall
/// offset `off` (0 inside, t outside). `dir` is −1 when the dish runs toward −axis.
fn dish(shape: HeadShape, r_shell: f64, depth: f64, tl: f64, dir: f64, off: f64) -> Result<Vec<Seg>> {
    let start = [r_shell + off, tl];
    let axis = [0.0, tl + dir * (depth + off)];
    Ok(match shape {
        HeadShape::Ellipsoidal if (depth - r_shell).abs() < 1e-9 => vec![Seg::Arc { c: [0.0, tl], a: start, b: axis }],
        HeadShape::Ellipsoidal => vec![Seg::QuarterEllipse { a: start, b: axis, apex: [start[0], axis[1]] }],
        HeadShape::Torispherical { crown_r: l, knuckle_r: r } => {
            let k = [r_shell - r, tl];
            let c_len = ((l - r).powi(2) - (r_shell - r).powi(2)).max(0.0).sqrt();
            let c = [0.0, tl - dir * c_len];
            let (dx, dy) = (k[0] - c[0], k[1] - c[1]);
            let n = dx.hypot(dy);
            if !(n > 0.0) {
                return Err(VesselError::Invalid("degenerate torispherical head".into()));
            }
            let p = [c[0] + (l + off) * dx / n, c[1] + (l + off) * dy / n];
            vec![Seg::Arc { c: k, a: start, b: p }, Seg::Arc { c, a: p, b: axis }]
        }
    })
}

/// The closed half-profile of a station.
pub fn profile(st: &Station) -> Result<Vec<Seg>> {
    let t = st.thickness;
    let segs = match (st.kind, st.profile) {
        ("cylinder" | "skirt", Profile::Straight) => {
            let (a, b) = ([st.r_from, st.z_from], [st.r_from + t, st.z_to]);
            vec![Seg::Line(a, [b[0], a[1]]), Seg::Line([b[0], a[1]], b), Seg::Line(b, [a[0], b[1]]), Seg::Line([a[0], b[1]], a)]
        }
        ("flat_cover", Profile::Straight) => {
            let (a, b) = ([0.0, st.z_from], [st.r_from, st.z_to]);
            vec![Seg::Line(a, [b[0], a[1]]), Seg::Line([b[0], a[1]], b), Seg::Line(b, [a[0], b[1]]), Seg::Line([a[0], b[1]], a)]
        }
        ("head", Profile::Head { side, sf, dish: depth, shape }) => {
            let (dir, tl, seam, rs) = match side {
                HeadSide::CrownAtStart => (-1.0, st.z_to - sf, st.z_to, st.r_to),
                HeadSide::CrownAtEnd => (1.0, st.z_from + sf, st.z_from, st.r_from),
            };
            let mut segs = vec![Seg::Line([rs, seam], [rs + t, seam])];
            if sf > 0.0 {
                segs.push(Seg::Line([rs + t, seam], [rs + t, tl]));
            }
            segs.extend(dish(shape, rs, depth, tl, dir, t)?);
            segs.push(Seg::Line([0.0, tl + dir * (depth + t)], [0.0, tl + dir * depth]));
            segs.extend(dish(shape, rs, depth, tl, dir, 0.0)?);
            if sf > 0.0 {
                segs.push(Seg::Line([rs, tl], [rs, seam]));
            }
            segs
        }
        ("cone", Profile::Conical { knuckle_from, knuckle_to }) => cone(st, knuckle_from, knuckle_to)?,
        (kind, _) => return Err(VesselError::NotSupportedYet(format!("element `{}`: {kind} geometry", st.id))),
    };
    if !finite(&segs) {
        return Err(VesselError::Invalid(format!("element `{}`: the profile has non-finite points", st.id)));
    }
    Ok(segs)
}

fn cone(st: &Station, knuckle_from: f64, knuckle_to: f64) -> Result<Vec<Seg>> {
    let t = st.thickness;
    let len = st.length();
    let large_first = st.r_from >= st.r_to;
    let (r_l, r_s, k_l, k_s) =
        if large_first { (st.r_from, st.r_to, knuckle_from, knuckle_to) } else { (st.r_to, st.r_from, knuckle_to, knuckle_from) };
    let alpha = st.half_apex_deg.map(f64::to_radians).ok_or_else(|| VesselError::Invalid(format!("element `{}`: unresolved cone", st.id)))?;
    let (sa, ca) = alpha.sin_cos();
    // In (x, u), u from the large end. The inside line: x cos α + u sin α = c.
    let c_in = (r_l - k_l) * ca + k_l;
    let c_out = c_in + t;
    let mut inner = Vec::new();
    let mut outer = Vec::new();
    let (i0, o0, i1, o1);
    if k_l > 0.0 {
        let kc = [r_l - k_l, 0.0];
        i0 = [r_l, 0.0];
        o0 = [r_l + t, 0.0];
        let pi = [kc[0] + k_l * ca, k_l * sa];
        let po = [kc[0] + (k_l + t) * ca, (k_l + t) * sa];
        inner.push(Seg::Arc { c: kc, a: i0, b: pi });
        outer.push(Seg::Arc { c: kc, a: o0, b: po });
        inner.push(Seg::Line(pi, [0.0; 2]));
        outer.push(Seg::Line(po, [0.0; 2]));
    } else {
        i0 = [r_l, 0.0];
        o0 = [c_out / ca, 0.0];
        inner.push(Seg::Line(i0, [0.0; 2]));
        outer.push(Seg::Line(o0, [0.0; 2]));
    }
    if k_s > 0.0 {
        let kc = [r_s + k_s, len];
        i1 = [r_s, len];
        o1 = [r_s + t, len];
        let pi = [kc[0] - k_s * ca, len - k_s * sa];
        let po = [kc[0] - (k_s - t) * ca, len - (k_s - t) * sa];
        set_line_end(&mut inner, pi);
        set_line_end(&mut outer, po);
        inner.push(Seg::Arc { c: kc, a: pi, b: i1 });
        outer.push(Seg::Arc { c: kc, a: po, b: o1 });
    } else {
        i1 = [r_s, len];
        o1 = [(c_out - len * sa) / ca, len];
        set_line_end(&mut inner, i1);
        set_line_end(&mut outer, o1);
    }
    let mut segs = vec![Seg::Line(o0, i0)];
    segs.extend(inner);
    segs.push(Seg::Line(i1, o1));
    segs.extend(outer);
    // u → z.
    let z = |p: P2| if large_first { [p[0], st.z_from + p[1]] } else { [p[0], st.z_to - p[1]] };
    Ok(segs
        .into_iter()
        .map(|s| match s {
            Seg::Line(a, b) => Seg::Line(z(a), z(b)),
            Seg::Arc { c, a, b } => Seg::Arc { c: z(c), a: z(a), b: z(b) },
            Seg::QuarterEllipse { a, b, apex } => Seg::QuarterEllipse { a: z(a), b: z(b), apex: z(apex) },
        })
        .collect())
}

/// Close the open straight segment of a cone side at `end`.
fn set_line_end(segs: &mut [Seg], end: P2) {
    if let Some(Seg::Line(_, b)) = segs.last_mut() {
        *b = end;
    }
}

/// The commands for one station, numbered from `first` (for `$ref`s).
pub fn plan_station(st: &Station, r: &Resolved, first: usize) -> Result<Vec<CommandCall>> {
    let segs = profile(st)?;
    let sketch_ref = format!("{REF}{first}.sketch");
    let (plane, axis) = match r.orientation {
        Orientation::Vertical => (json!({"origin": [0.0, 0.0, 0.0], "x_dir": [1.0, 0.0, 0.0], "y_dir": [0.0, 0.0, 1.0]}), "Z"),
        Orientation::Horizontal => (json!({"origin": [0.0, 0.0, 0.0], "x_dir": [0.0, 0.0, 1.0], "y_dir": [1.0, 0.0, 0.0]}), "X"),
    };
    let mut out = vec![CommandCall { id: "sketch.create".into(), params: json!({"name": sketch_name(st), "plane": plane}) }];
    // A plain rectangle stays one command (the skeleton's plan; upstream constrains it as one).
    let rect = match (st.kind, st.profile) {
        ("cylinder" | "skirt", Profile::Straight) => Some(([st.r_from, st.z_from], [st.r_from + st.thickness, st.z_to])),
        ("flat_cover", Profile::Straight) => Some(([0.0, st.z_from], [st.r_from, st.z_to])),
        _ => None,
    };
    match rect {
        Some((p0, p1)) => {
            out.push(CommandCall { id: "sketch.rectangle.two_point".into(), params: json!({"p0": p0, "p1": p1, "sketch": sketch_ref}) })
        }
        None => out.extend(segs.iter().map(draw)),
    }
    out.push(CommandCall { id: "sketch.finish".into(), params: json!({}) });
    out.push(CommandCall {
        id: "solid.revolve".into(),
        params: json!({"sketch": sketch_ref, "axis": axis, "angle": "360 deg", "operation": "new", "name": body_name(st), "body_name": body_name(st)}),
    });
    Ok(out)
}

/// The whole vessel's commands, element by element.
pub fn plan(r: &Resolved) -> Result<Vec<CommandCall>> {
    let mut out = Vec::new();
    for st in &r.stations {
        let steps = plan_station(st, r, out.len())?;
        out.extend(steps);
        if out.len() > MAX_STEPS {
            return Err(VesselError::Invalid(format!("the plan exceeds {MAX_STEPS} steps")));
        }
    }
    Ok(out)
}

/// One user parameter the regeneration writes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    /// Millimetres (or degrees, for `_half_apex`).
    pub value: f64,
    pub unit: &'static str,
    pub comment: String,
}

/// Letters, digits and `_` only (upstream parameter names are identifiers).
fn ident(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
}

/// The parameter prefix of a vessel: its tag without punctuation, starting with `V`.
pub fn parameter_prefix(vessel_id: &str) -> String {
    let id = ident(vessel_id);
    if id.starts_with('V') || id.starts_with('v') { id } else { format!("V{id}") }
}

/// Every station's dimensions as named parameters (`V101_E3_length`, …).
pub fn parameters(vessel_id: &str, r: &Resolved) -> Vec<Param> {
    let prefix = parameter_prefix(vessel_id);
    let mut out = Vec::new();
    for st in &r.stations {
        let el = ident(&st.id);
        let mut push = |field: &str, value: f64, unit: &'static str, what: &str| {
            if value.is_finite() {
                out.push(Param { name: format!("{prefix}_{el}_{field}"), value, unit, comment: format!("{} {}: {what}", st.kind, st.id) });
            }
        };
        push("diameter", st.diameter_mm, "mm", "diameter on the vessel's basis");
        push("thickness", st.thickness, "mm", "wall thickness");
        push("z_from", st.z_from, "mm", "start from the datum");
        push("length", st.length(), "mm", "axial length");
        match st.profile {
            Profile::Head { sf, dish, shape, .. } => {
                push("sf", sf, "mm", "straight flange");
                push("depth", dish, "mm", "inside dish depth");
                if let HeadShape::Torispherical { crown_r, knuckle_r } = shape {
                    push("crown_r", crown_r, "mm", "inside crown radius");
                    push("knuckle_r", knuckle_r, "mm", "inside knuckle radius");
                }
            }
            Profile::Conical { knuckle_from, knuckle_to } => {
                push("r_from", st.r_from, "mm", "inside radius at the start");
                push("r_to", st.r_to, "mm", "inside radius at the end");
                push("knuckle_from", knuckle_from, "mm", "inside knuckle at the start");
                push("knuckle_to", knuckle_to, "mm", "inside knuckle at the end");
                if let Some(a) = st.half_apex_deg {
                    push("half_apex", a, "deg", "half-apex angle");
                }
            }
            Profile::Straight => {}
        }
    }
    out
}

/// `params` with every `"$ref:<step>.<field>"` string replaced by that field of `replies[step]`.
pub fn substitute(params: &Value, replies: &[Value]) -> Result<Value> {
    fn walk(v: &Value, replies: &[Value], depth: usize) -> Result<Value> {
        if depth > 32 {
            return Err(VesselError::Invalid("parameters nested too deep".into()));
        }
        Ok(match v {
            Value::String(s) if s.starts_with(REF) => {
                let rest = &s[REF.len()..];
                let (step, field) = rest.split_once('.').ok_or_else(|| VesselError::Invalid(format!("bad reference `{s}`")))?;
                let step: usize = step.parse().map_err(|_| VesselError::Invalid(format!("bad reference `{s}`")))?;
                let reply = replies.get(step).ok_or_else(|| VesselError::Invalid(format!("reference `{s}` to a step that has not run")))?;
                reply.get(field).cloned().ok_or_else(|| VesselError::Invalid(format!("step {step} replied without `{field}`")))?
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| walk(x, replies, depth + 1)).collect::<Result<_>>()?),
            Value::Object(o) => Value::Object(o.iter().map(|(k, x)| Ok((k.clone(), walk(x, replies, depth + 1)?))).collect::<Result<_>>()?),
            other => other.clone(),
        })
    }
    walk(params, replies, 0)
}
