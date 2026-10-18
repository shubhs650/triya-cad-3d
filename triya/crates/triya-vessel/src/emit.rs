// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The plan: ordinary engine commands that build the resolved vessel.
//!
//! Only the cylinder is emitted so far. It becomes a profile sketch on the XZ plane (a rectangle
//! from the inside to the outside radius, from `z_from` to `z_to`) revolved 360° about the world
//! Z axis. The ids and parameter shapes are upstream's (`crates/engine/src/cmd/sketch.rs`,
//! `features.rs`; the same sequence as `revolve_convention_tests.rs` uses):
//!
//! | step | command | params |
//! |---|---|---|
//! | 0 | `sketch.create` | `{name, plane: {origin: [0,0,0], x_dir: [1,0,0], y_dir: [0,0,1]}}` → `{sketch: id, …}` |
//! | 1 | `sketch.rectangle.two_point` | `{p0: [r_i, z_from], p1: [r_o, z_to], sketch: $ref:0.sketch}` |
//! | 2 | `sketch.finish` | `{}` |
//! | 3 | `solid.revolve` | `{sketch: $ref:0.sketch, axis: "Z", angle: "360 deg", operation: "new", name, body_name}` |
//!
//! The plane is given explicitly rather than as `"XZ"`: upstream's named `XZ` plane has its
//! sketch y axis along −Z (`solidcraft_geom::Plane::XZ`), and an explicit `{x_dir: +X, y_dir: +Z}`
//! keeps sketch y = vessel z, so the rectangle's coordinates read as radius and height.
//!
//! A later step may need an earlier reply (the sketch's id): a string parameter
//! `"$ref:<step>.<field>"` is replaced by that field of that step's reply when the plan runs
//! ([`substitute`]). Every other element and every attachment is `NotSupportedYet`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::eval::{Profile, Resolved, Station};
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

fn sketch_name(st: &Station) -> String {
    format!("Vessel {} profile", st.id)
}

fn body_name(st: &Station) -> String {
    format!("Vessel {}", st.id)
}

/// The commands for one station, numbered from `first` (for `$ref`s).
pub fn plan_station(st: &Station, first: usize) -> Result<Vec<CommandCall>> {
    match (st.kind, st.profile) {
        ("cylinder", Profile::Straight) => {
            let r_i = st.r_from;
            let r_o = st.r_from + st.thickness;
            let sketch_ref = format!("{REF}{first}.sketch");
            Ok(vec![
                CommandCall {
                    id: "sketch.create".into(),
                    params: json!({"name": sketch_name(st), "plane": {"origin": [0.0, 0.0, 0.0], "x_dir": [1.0, 0.0, 0.0], "y_dir": [0.0, 0.0, 1.0]}}),
                },
                CommandCall {
                    id: "sketch.rectangle.two_point".into(),
                    params: json!({"p0": [r_i, st.z_from], "p1": [r_o, st.z_to], "sketch": sketch_ref}),
                },
                CommandCall { id: "sketch.finish".into(), params: json!({}) },
                CommandCall {
                    id: "solid.revolve".into(),
                    params: json!({"sketch": sketch_ref, "axis": "Z", "angle": "360 deg", "operation": "new", "name": body_name(st), "body_name": body_name(st)}),
                },
            ])
        }
        (kind, _) => Err(VesselError::NotSupportedYet(format!("element `{}`: {kind} geometry", st.id))),
    }
}

/// The whole vessel's commands. Attachments are not built yet: a vessel with any is refused.
pub fn plan(r: &Resolved) -> Result<Vec<CommandCall>> {
    let mut out = Vec::new();
    for st in &r.stations {
        let steps = plan_station(st, out.len())?;
        out.extend(steps);
        if out.len() > MAX_STEPS {
            return Err(VesselError::Invalid(format!("the plan exceeds {MAX_STEPS} steps")));
        }
    }
    Ok(out)
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
