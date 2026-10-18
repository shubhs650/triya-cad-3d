// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Nozzle frames and the reference volume, without the kernel (T3.3).

use serde_json::{Value, json};
use triya_stdlib::TableSet;

use crate::eval;
use crate::model::{Attachment, Global, Orientation, VesselDoc};
use crate::nozzle::{self, NozzleGeom, V3, azimuth, dot, norm, sub};

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn near(a: V3, b: V3) -> bool {
    norm(sub(a, b)) < 1e-9
}

fn global(v: Value) -> Global {
    serde_json::from_value(v).unwrap()
}

#[test]
fn azimuths_follow_the_angle_convention() {
    let g = Global::default();
    assert!(near(azimuth(&g, 0.0), [0.0, 1.0, 0.0]), "0° is plant north (+Y)");
    assert!(near(azimuth(&g, 90.0), [1.0, 0.0, 0.0]), "clockwise looking down: 90° is +X");
    assert!(near(azimuth(&g, 180.0), [0.0, -1.0, 0.0]));
    let ccw = global(json!({"angle_sense": "counterclockwise_from_above"}));
    assert!(near(azimuth(&ccw, 90.0), [-1.0, 0.0, 0.0]));
    let px = global(json!({"angle_zero": "plus_x"}));
    assert!(near(azimuth(&px, 0.0), [1.0, 0.0, 0.0]) && near(azimuth(&px, 90.0), [0.0, -1.0, 0.0]));
    let h = global(json!({"orientation": "horizontal"}));
    assert!(near(azimuth(&h, 0.0), [0.0, 0.0, 1.0]), "horizontal: 0° at the top");
    assert!(near(azimuth(&h, 90.0), [0.0, -1.0, 0.0]), "clockwise looking from the left end: 90° is −Y");
    assert!(near(azimuth(&h, 270.0), [0.0, 1.0, 0.0]));
}

/// A 1600 ID × 12 shell (3000 long) with a 2:1 head on top (sf 40) and on the bottom.
fn doc_with(attachments: Vec<Value>, orientation: &str) -> VesselDoc {
    VesselDoc::from_value(&json!({
        "schema": "triya.vessel/0.1", "id": "T",
        "global": {"orientation": orientation, "datum": if orientation == "horizontal" { "left_tl" } else { "bottom_tl" }},
        "elements": [
            {"id": "H0", "kind": "head", "preset": "ellipsoidal_2_1", "diameter_mm": 1600, "thickness_mm": 12, "straight_flange_mm": 40},
            {"id": "S1", "kind": "cylinder", "length_mm": 3000, "thickness_mm": 12},
            {"id": "H1", "kind": "head", "preset": "asme_fd", "thickness_mm": 12, "straight_flange_mm": 40}
        ],
        "attachments": attachments,
    }))
    .unwrap()
}

fn nozzle_json(id: &str, host: &str, frame: Value, nps: &str, sch: &str, projection: Value) -> Value {
    json!({"id": id, "kind": "nozzle", "host": host, "axial": {"ref": "datum", "mm": 1500}, "angle_deg": 90, "frame": frame,
        "nps": nps, "schedule": sch, "projection": projection})
}

fn resolved(doc: &VesselDoc) -> Vec<NozzleGeom> {
    let r = eval::resolve(doc).unwrap();
    let t = TableSet::embedded().unwrap();
    doc.attachments.iter().map(|a: &Attachment| nozzle::resolve(doc, &r, a, &t).unwrap_or_else(|e| panic!("{}: {e}", a.id))).collect()
}

#[test]
fn cylinder_frames_meet_the_shell_where_the_geometry_says() {
    let (ri, ro) = (800.0f64, 812.0f64);
    let doc = doc_with(
        vec![
            nozzle_json("N1", "S1", json!({"type": "radial"}), "6", "STD", json!({"from": "cl", "mm": 1100})),
            nozzle_json("N2", "S1", json!({"type": "hillside", "offset_mm": 300}), "3", "XS", json!({"from": "od", "mm": 200})),
            nozzle_json("N3", "S1", json!({"type": "tangential"}), "2", "80", json!({"from": "od", "mm": 150})),
            nozzle_json("N4", "S1", json!({"type": "tilted", "tilt_deg": 15}), "2", "XS", json!({"from": "od", "mm": 200})),
        ],
        "vertical",
    );
    let n = resolved(&doc);
    // Radial at 90° (+X), elevation 1500 from the bottom TL.
    let f = n[0].frame;
    assert!(near(f.dir, [1.0, 0.0, 0.0]) && near(f.origin, [0.0, 0.0, 1500.0]));
    assert!(close(f.s_inner, ri, 1e-6) && close(f.s_outer, ro, 1e-6), "{f:?}");
    assert!(close(n[0].s_face, 1100.0, 1e-9) && close(n[0].neck_od, 168.3, 1e-9) && close(n[0].neck_wall, 7.11, 1e-9));
    // Hillside: offset along the tangent (+X × … at 90° is −Y? t = axis × u = Z × X = +Y).
    let f = n[1].frame;
    assert!(near(f.origin, [0.0, 300.0, 1500.0]), "{:?}", f.origin);
    assert!(close(f.s_outer, (ro * ro - 300.0f64.powi(2)).sqrt(), 1e-6));
    assert!(close(n[1].s_face, f.s_outer + 200.0, 1e-9));
    // Tangential: the neck's outside flush with the inside surface.
    let e = ri - 60.3 / 2.0;
    let f = n[2].frame;
    assert!(close(dot(f.origin, [0.0, 1.0, 0.0]), e, 1e-9));
    assert!(close(f.s_inner, (ri * ri - e * e).sqrt(), 1e-6) && close(f.s_outer, (ro * ro - e * e).sqrt(), 1e-6));
    // Tilted 15° up: through the outside surface at the elevation.
    let f = n[3].frame;
    let c = 15f64.to_radians();
    assert!(near(f.dir, [c.cos(), 0.0, c.sin()]));
    assert!(close(f.s_outer, ro / c.cos(), 1e-6), "{} vs {}", f.s_outer, ro / c.cos());
    assert!(near(f.at(f.s_outer), [ro, 0.0, 1500.0]));
    assert!(close(f.s_inner, ri / c.cos(), 1e-6));
}

#[test]
fn head_frames_meet_the_head_where_the_geometry_says() {
    let doc = doc_with(
        vec![
            // On the bottom 2:1 head (crown down), normal to it at 400 from the axis.
            json!({"id": "N5", "kind": "nozzle", "host": "H0", "axial": {"ref": "host_from", "mm": 0}, "angle_deg": 270,
                "frame": {"type": "head_radial_at_r", "radius_mm": 400}, "nps": "2", "schedule": "XS", "projection": {"from": "od", "mm": 200}}),
            // On the top F&D head, parallel to the axis at 600.
            json!({"id": "M1", "kind": "nozzle", "host": "H1", "axial": {"ref": "host_from", "mm": 0}, "angle_deg": 180,
                "frame": {"type": "parallel_to_axis", "radius_mm": 400}, "nps": "24", "schedule": "STD", "projection": {"from": "cl", "mm": 900}}),
            json!({"id": "N6", "kind": "nozzle", "host": "H0", "axial": {"ref": "host_from", "mm": 0}, "angle_deg": 0,
                "frame": {"type": "parallel_to_axis", "radius_mm": 300}, "nps": "2", "schedule": "XS", "projection": {"from": "od", "mm": 150}}),
        ],
        "vertical",
    );
    let r = eval::resolve(&doc).unwrap();
    let n = resolved(&doc);
    // Bottom head: TL at the datum (z = 0), dish 400 deep downward; outside ellipse 812 × 412.
    let (a, b) = (812.0f64, 412.0f64);
    let f = n[0].frame;
    let p = f.at(0.0);
    assert!(near(p, [-400.0, 0.0, p[2]]), "270° is −X: {p:?}");
    let w = b * (1.0 - (400.0 / a).powi(2)).sqrt();
    assert!(close(p[2], -w, 1e-6), "{} vs {}", p[2], -w);
    // The ellipse's outward normal (ρ/a², w/b²), pointing down and out.
    let (nr, nz) = (400.0 / (a * a), w / (b * b));
    let k = nr.hypot(nz);
    assert!(norm(sub(f.dir, [-nr / k, 0.0, -nz / k])) < 1e-5, "{:?}", f.dir);
    // The inside surface along the normal: about one wall in (a normal offset is exactly t only
    // for a true offset surface; Triya's outside ellipse is R + t, D/4 + t).
    assert!(f.s_inner < 0.0 && f.s_inner > -14.0, "{}", f.s_inner);
    // Parallel to the axis on the bottom head: straight down at 300 from the axis.
    let f = n[2].frame;
    assert!(near(f.dir, [0.0, 0.0, -1.0]) && near(f.origin, [0.0, 300.0, 0.0]));
    assert!(close(f.s_outer, b * (1.0 - (300.0 / a).powi(2)).sqrt(), 1e-6));
    assert!(close(f.s_inner, 400.0 * (1.0 - (300.0f64 / 800.0).powi(2)).sqrt(), 1e-6));
    // F&D top head: crown L = 1600, knuckle r = 96; at 400 from the axis we are on the crown.
    let top = r.station("H1").unwrap();
    let tl = top.tl.unwrap();
    let f = n[1].frame;
    assert!(near(f.dir, [0.0, 0.0, 1.0]) && near(f.origin, [0.0, -400.0, tl]), "{f:?}");
    let c = ((1600.0f64 - 96.0).powi(2) - (800.0f64 - 96.0).powi(2)).sqrt();
    assert!(close(f.s_inner, (1600.0f64.powi(2) - 400.0f64.powi(2)).sqrt() - c, 1e-6), "{}", f.s_inner);
    assert!(close(f.s_outer, (1612.0f64.powi(2) - 400.0f64.powi(2)).sqrt() - c, 1e-6));
    assert!(close(n[1].s_face, 900.0, 1e-9), "cl on a head is from its tangent line");
}

#[test]
fn horizontal_vessels_put_0_degrees_on_top() {
    let doc = doc_with(vec![nozzle_json("N1", "S1", json!({"type": "radial"}), "4", "STD", json!({"from": "od", "mm": 150}))], "horizontal");
    let mut doc = doc;
    doc.attachments[0].angle_deg = 0.0;
    let n = resolved(&doc);
    assert!(near(n[0].frame.dir, [0.0, 0.0, 1.0]) && near(n[0].frame.origin, [1500.0, 0.0, 0.0]));
    assert!(doc.global.orientation == Orientation::Horizontal);
}

#[test]
fn nozzles_that_cannot_be_placed_are_errors() {
    let bad = |a: Value| {
        let doc = doc_with(vec![a], "vertical");
        let r = eval::resolve(&doc).unwrap();
        nozzle::resolve(&doc, &r, &doc.attachments[0], &TableSet::embedded().unwrap()).is_err()
    };
    assert!(bad(nozzle_json("N1", "S1", json!({"type": "hillside", "offset_mm": 790}), "6", "STD", json!({"from": "od", "mm": 150}))));
    assert!(bad(nozzle_json("N1", "S1", json!({"type": "radial"}), "6", "STD", json!({"from": "cl", "mm": 700}))), "face inside the shell");
    assert!(bad(nozzle_json("N1", "S1", json!({"type": "radial"}), "7", "STD", json!({"from": "od", "mm": 150}))), "no NPS 7 pipe");
    assert!(bad(nozzle_json("N1", "H0", json!({"type": "radial"}), "2", "STD", json!({"from": "od", "mm": 150}))), "radial on a head");
    assert!(bad(nozzle_json("N1", "S1", json!({"type": "parallel_to_axis", "radius_mm": 200}), "2", "STD", json!({"from": "od", "mm": 150}))));
    let mut off_end = nozzle_json("N1", "S1", json!({"type": "radial"}), "6", "STD", json!({"from": "od", "mm": 150}));
    off_end["axial"]["mm"] = json!(3010);
    assert!(bad(off_end), "runs off the end of its course");
    assert!(
        bad(
            json!({"id": "N1", "kind": "nozzle", "host": "H1", "axial": {"ref": "host_from", "mm": 0}, "frame": {"type": "parallel_to_axis", "radius_mm": 780},
        "nps": "2", "schedule": "XS", "projection": {"from": "od", "mm": 150}})
        ),
        "leaves the dish"
    );
}

#[test]
fn the_reference_volume_of_a_radial_nozzle_matches_the_hand_quadrature() {
    // 6" STD (168.3 × 7.11) set in a 1600 × 12 shell, face 1100 from the centreline; the same
    // integral computed separately: neck outside the shell minus the bore through the wall.
    let doc = doc_with(vec![nozzle_json("N1", "S1", json!({"type": "radial"}), "6", "STD", json!({"from": "cl", "mm": 1100}))], "vertical");
    let r = eval::resolve(&doc).unwrap();
    let n = resolved(&doc);
    let v = nozzle::expected_change(&doc.global, r.station("S1").unwrap(), &n[0], 300).unwrap();
    let (a, b) = (84.15f64, 77.04f64);
    let m = 1200;
    let h = 2.0 * a / m as f64;
    let mut exact = 0.0;
    for i in 0..m {
        for j in 0..m {
            let (y, z) = (-a + (i as f64 + 0.5) * h, -a + (j as f64 + 0.5) * h);
            let rr = y * y + z * z;
            if rr > a * a {
                continue;
            }
            let (xo, xi) = ((812.0f64 * 812.0 - y * y).sqrt(), (800.0f64 * 800.0 - y * y).sqrt());
            exact += if rr < b * b { -(xo - xi) } else { 1100.0 - xo } * h * h;
        }
    }
    assert!((v - exact).abs() < 2e-3 * exact.abs(), "{v:.0} vs {exact:.0}");
}

#[test]
fn a_pad_adds_its_conforming_plate() {
    let mut a = nozzle_json("N1", "S1", json!({"type": "radial"}), "6", "STD", json!({"from": "cl", "mm": 1100}));
    let plain = doc_with(vec![a.clone()], "vertical");
    a["pad"] = json!({"width_mm": 50, "thickness_mm": 12});
    let padded = doc_with(vec![a], "vertical");
    let r = eval::resolve(&plain).unwrap();
    let host = r.station("S1").unwrap();
    let v0 = nozzle::expected_change(&plain.global, host, &resolved(&plain)[0], 300).unwrap();
    let v1 = nozzle::expected_change(&padded.global, host, &resolved(&padded)[0], 300).unwrap();
    // The pad: the band 812 … 824 inside the 268.3 circle, less the neck's 168.3 circle.
    let ring = |rad: f64| {
        let m = 1200;
        let h = 2.0 * rad / m as f64;
        let mut s = 0.0;
        for i in 0..m {
            for j in 0..m {
                let (y, z) = (-rad + (i as f64 + 0.5) * h, -rad + (j as f64 + 0.5) * h);
                if y * y + z * z <= rad * rad {
                    s += ((824.0f64 * 824.0 - y * y).sqrt() - (812.0f64 * 812.0 - y * y).sqrt()) * h * h;
                }
            }
        }
        s
    };
    let pad = ring(134.15) - ring(84.15);
    assert!(((v1 - v0) - pad).abs() < 3e-3 * pad, "{:.0} vs {pad:.0}", v1 - v0);
}
