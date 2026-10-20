// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Support parts as wireframes: each [`triya_vessel::support::Part`] is built from a short list
//! of engine commands (a profile sketched on an explicit plane, then extruded or revolved; or a
//! `solid.cylinder`), so the drawing reads the same calls back into edges instead of asking the
//! kernel: a plate is its profile on both faces joined at the corners, a ring its profile's
//! corners swept round the axis plus the profile where the view sees it edge-on, a pipe its end
//! circles and silhouettes. Bolt holes (cut cylinders) are end circles only.

use serde_json::Value;
use triya_vessel::emit::CommandCall;
use triya_vessel::nozzle::{V3, add, cross, dot, mul, sub};

use crate::model::Layer;
use crate::proj::{Wire, unit};

fn v3(v: Option<&Value>) -> Option<V3> {
    let a = v?.as_array()?;
    let (x, y, z) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?);
    [x, y, z].iter().all(|c| c.is_finite()).then_some([x, y, z])
}

fn p2(v: Option<&Value>) -> Option<[f64; 2]> {
    let a = v?.as_array()?;
    let (x, y) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?);
    (x.is_finite() && y.is_finite()).then_some([x, y])
}

fn num(v: &Value, key: &str) -> Option<f64> {
    let x = match v.get(key)? {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().trim_end_matches("deg").trim_end_matches("mm").trim().parse().ok()?,
        _ => return None,
    };
    x.is_finite().then_some(x)
}

/// One sketched segment, sampled (2D sketch coordinates); `corner` marks its end points.
fn sample_seg(call: &CommandCall) -> Option<Vec<[f64; 2]>> {
    match call.id.as_str() {
        "sketch.line" => {
            let pts = call.params.get("points")?.as_array()?;
            Some(vec![p2(pts.first())?, p2(pts.get(1))?])
        }
        "sketch.conic" => {
            let (a, b, c) = (p2(call.params.get("start"))?, p2(call.params.get("end"))?, p2(call.params.get("apex"))?);
            let rho = num(&call.params, "rho")?.clamp(1e-6, 1.0 - 1e-6);
            let w = rho / (1.0 - rho);
            Some(
                (0..=16)
                    .map(|i| {
                        let t = f64::from(i) / 16.0;
                        let (b0, b1, b2) = ((1.0 - t).powi(2), 2.0 * t * (1.0 - t) * w, t * t);
                        let d = b0 + b1 + b2;
                        [(a[0] * b0 + c[0] * b1 + b[0] * b2) / d, (a[1] * b0 + c[1] * b1 + b[1] * b2) / d]
                    })
                    .collect(),
            )
        }
        _ => None,
    }
}

/// Most calls read from one part.
const MAX_CALLS: usize = 4_096;

/// Add the wireframe of one part's calls to `wire`.
pub fn draw_part(wire: &mut Wire, calls: &[CommandCall], layer: Layer) {
    let mut plane: Option<(V3, V3, V3)> = None;
    let mut segs: Vec<Vec<[f64; 2]>> = Vec::new();
    for call in calls.iter().take(MAX_CALLS) {
        match call.id.as_str() {
            "sketch.create" => {
                let p = call.params.get("plane");
                plane = match (v3(p.and_then(|p| p.get("origin"))), v3(p.and_then(|p| p.get("x_dir"))), v3(p.and_then(|p| p.get("y_dir")))) {
                    (Some(o), Some(x), Some(y)) => match (unit(x), unit(y)) {
                        (Some(x), Some(y)) => Some((o, x, y)),
                        _ => None,
                    },
                    _ => None,
                };
                segs.clear();
            }
            "sketch.line" | "sketch.conic" => {
                if let Some(s) = sample_seg(call) {
                    segs.push(s);
                }
            }
            "solid.extrude" => {
                let Some((o, x, y)) = plane else { continue };
                let Some(n) = unit(cross(x, y)) else { continue };
                let d = num(&call.params, "distance").unwrap_or(0.0);
                let (d0, d1) = if call.params.get("direction").and_then(Value::as_str) == Some("symmetric") { (-d / 2.0, d / 2.0) } else { (0.0, d) };
                let at = |q: [f64; 2], k: f64| add(add(o, add(mul(x, q[0]), mul(y, q[1]))), mul(n, k));
                for s in &segs {
                    for k in [d0, d1] {
                        let pts: Vec<V3> = s.iter().map(|q| at(*q, k)).collect();
                        wire.polyline(&pts, layer);
                    }
                    if let Some(q) = s.first() {
                        wire.line(at(*q, d0), at(*q, d1), layer);
                    }
                }
            }
            "solid.revolve" => {
                let Some((o, radial, axis)) = plane else { continue };
                let angle = num(&call.params, "angle").unwrap_or(360.0).clamp(0.0, 360.0).to_radians();
                let full = angle >= std::f64::consts::TAU - 1e-9;
                let a0 = if full { 0.0 } else { -angle / 2.0 };
                let side = cross(axis, radial);
                let at = |q: [f64; 2], phi: f64| add(o, add(mul(axis, q[1]), mul(add(mul(radial, phi.cos()), mul(side, phi.sin())), q[0])));
                // Corners swept round the axis.
                for s in &segs {
                    if let Some(q) = s.first()
                        && q[0] > 1e-6
                    {
                        let c = add(o, mul(axis, q[1]));
                        if full {
                            wire.circle(c, axis, q[0], layer);
                        } else {
                            wire.arc(c, axis, q[0], radial, a0, angle, layer);
                        }
                    }
                }
                // The profile at the sector's ends and where the view sees the surface edge-on.
                let mut at_phi: Vec<f64> = Vec::new();
                if !full {
                    at_phi.extend([a0, a0 + angle]);
                }
                if let Some(w) = unit(cross(axis, wire.basis.toward)) {
                    for dir in [w, mul(w, -1.0)] {
                        let phi = dot(dir, side).atan2(dot(dir, radial));
                        let rel = (phi - a0).rem_euclid(std::f64::consts::TAU);
                        if full || rel <= angle {
                            at_phi.push(phi);
                        }
                    }
                }
                for phi in at_phi {
                    for s in &segs {
                        let pts: Vec<V3> = s.iter().map(|q| at(*q, phi)).collect();
                        wire.polyline(&pts, layer);
                    }
                }
            }
            "solid.cylinder" => {
                let (Some(base), Some(axis), Some(r), Some(h)) =
                    (v3(call.params.get("base")), v3(call.params.get("axis")), num(&call.params, "radius"), num(&call.params, "height"))
                else {
                    continue;
                };
                let Some(axis) = unit(axis) else { continue };
                if call.params.get("operation").and_then(Value::as_str) == Some("cut") {
                    // A hole: its ends only.
                    wire.circle(base, axis, r, layer);
                    wire.circle(add(base, mul(axis, h)), axis, r, layer);
                } else {
                    wire.frustum(base, axis, h, r, r, layer);
                }
            }
            _ => {}
        }
    }
}

/// The 3D bounds' centre of a part's sketch planes and cylinders (for labels).
pub fn anchor(calls: &[CommandCall]) -> Option<V3> {
    let mut pts: Vec<V3> = Vec::new();
    for c in calls.iter().take(MAX_CALLS) {
        if let Some(o) = v3(c.params.get("plane").and_then(|p| p.get("origin"))) {
            pts.push(o);
        }
        if let Some(b) = v3(c.params.get("base")) {
            pts.push(b);
        }
    }
    let n = pts.len() as f64;
    (n > 0.0).then(|| mul(pts.iter().fold([0.0; 3], |acc, p| add(acc, *p)), 1.0 / n))
}

/// The distance from `p` to the line through the origin along the unit `axis`.
pub fn radial_distance(p: V3, axis: V3) -> f64 {
    let along = dot(p, axis);
    let r = sub(p, mul(axis, along));
    dot(r, r).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proj::{Basis, NoOcclusion};
    use serde_json::json;

    fn call(id: &str, params: Value) -> CommandCall {
        CommandCall { id: id.into(), params }
    }

    #[test]
    fn a_plate_draws_both_faces_and_its_corner_lines() {
        let calls = vec![
            call("sketch.create", json!({"plane": {"origin": [0, 0, 0], "x_dir": [1, 0, 0], "y_dir": [0, 0, 1]}})),
            call("sketch.line", json!({"points": [[0, 0], [100, 0]]})),
            call("sketch.line", json!({"points": [[100, 0], [100, 50]]})),
            call("sketch.line", json!({"points": [[100, 50], [0, 50]]})),
            call("sketch.line", json!({"points": [[0, 50], [0, 0]]})),
            call("sketch.finish", json!({})),
            call("solid.extrude", json!({"distance": 10, "direction": "symmetric"})),
            call("solid.cylinder", json!({"radius": 5, "height": 12, "base": [50, -6, 25], "axis": [0, 1, 0], "operation": "cut"})),
        ];
        // Seen from above: the plate is a 100 × 10 rectangle; the hole is a line pair edge-on.
        let mut w = Wire::new(Basis::new([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]), &NoOcclusion, true);
        draw_part(&mut w, &calls, Layer::Visible);
        assert_eq!(w.edges.len(), 4 * 2 + 4 + 2, "{:?}", w.edges);
        let ys: Vec<f64> = w.edges.iter().flat_map(|e| e.edge.sample(1)).map(|p| p.y).collect();
        assert!(ys.iter().all(|y| (y.abs() - 5.0).abs() < 1e-9 || (y.abs() - 6.0).abs() < 1e-9 || y.abs() < 5.0), "{ys:?}");
        assert_eq!(anchor(&calls), Some([25.0, -3.0, 12.5]));
    }

    #[test]
    fn a_full_ring_seen_from_the_side_is_lines_and_profiles() {
        let calls = vec![
            call("sketch.create", json!({"plane": {"origin": [0, 0, 0], "x_dir": [1, 0, 0], "y_dir": [0, 0, 1]}})),
            call("sketch.line", json!({"points": [[500, 0], [600, 0]]})),
            call("sketch.line", json!({"points": [[600, 0], [600, 10]]})),
            call("sketch.line", json!({"points": [[600, 10], [500, 10]]})),
            call("sketch.line", json!({"points": [[500, 10], [500, 0]]})),
            call("solid.revolve", json!({"axis": "y", "angle": "360 deg"})),
        ];
        let mut w = Wire::new(Basis::new([1.0, 0.0, 0.0], [0.0, 0.0, 1.0]), &NoOcclusion, true);
        draw_part(&mut w, &calls, Layer::Visible);
        // Four corner circles edge-on (lines) and the profile on both sides (4 lines each).
        assert_eq!(w.edges.len(), 4 + 8, "{:?}", w.edges);
        assert_eq!(radial_distance([3.0, 4.0, 7.0], [0.0, 0.0, 1.0]), 5.0);
    }
}
