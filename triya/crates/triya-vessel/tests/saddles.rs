// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T4.1 acceptance: two saddles (web, ribs, base plate with anchor bolt holes, wear plate) under
//! reference vessel B's shell. Each part's measured volume matches its closed form, the wear
//! plate lies on the shell (no gap over 0.01 mm), the saddles sit where `axial` says, and one undo
//! takes everything away.

use serde_json::{Value, json};
use solidcraft_engine::Session;
use std::f64::consts::PI;

fn vessel_b_on_saddles() -> Value {
    let mut doc: Value = serde_json::from_str(include_str!("reference/vessel-b.json")).unwrap();
    // The saddles only: nozzles and trunnions are other tests' business.
    doc["attachments"] = json!(doc["attachments"].as_array().unwrap().iter().filter(|a| a["kind"] == "saddle").cloned().collect::<Vec<_>>());
    doc
}

fn body(s: &Session, name: &str) -> solidcraft_engine::kernel::Body {
    let st = s.model.state();
    let b = st.bodies.iter().find(|b| b.name == name);
    b.map(|b| b.body.clone()).unwrap_or_else(|| panic!("no body `{name}`: {:?}", st.bodies.iter().map(|b| &b.name).collect::<Vec<_>>()))
}

#[test]
fn two_saddles_with_wear_plates_match_their_closed_forms() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": vessel_b_on_saddles()})).unwrap();
    let t = std::time::Instant::now();
    let r = s.execute("vessel.regenerate", &json!({})).unwrap();
    eprintln!("regenerate: {:?}\nsupports: {:#}\nwarnings: {:#}", t.elapsed(), r["supports"], r["warnings"]);
    let supports = r["supports"].as_array().unwrap();
    assert_eq!(supports.len(), 2);
    for sp in supports {
        assert_eq!(sp["built"], "built", "{sp}");
        assert!(sp.get("warning").is_none(), "{sp}");
    }

    // Shell: 1200 ID, 10 thk → outside radius 610; wear plate 10 → contact radius 620.
    let (ro, rc) = (610.0_f64, 620.0_f64);
    let half = 60.0_f64.to_radians();
    let h = rc * half.sin();
    let z_under = -ro - 800.0;
    let zb = z_under + 16.0;
    // Web: the region from the base plate's top to the contact arc between the horns.
    let web_area = -2.0 * h * zb - (h * (rc * rc - h * h).sqrt() + rc * rc * (h / rc).asin());
    // Ribs: four, 10 thk, horn to horn, each in two halves of (200 − 12) / 2 along the axis.
    let rib_ys: Vec<f64> = (0..4).map(|i| -(h - 5.0) + f64::from(i) * 2.0 * (h - 5.0) / 3.0).collect();
    let rib_total: f64 = rib_ys.iter().map(|y| (-(rc * rc - (y.abs() - 5.0).powi(2)).sqrt() - zb) * 94.0 * 10.0 * 2.0).sum();
    let base = 1100.0 * 200.0 * 16.0 - 2.0 * PI * 15.0 * 15.0 * 16.0; // two 24 bolts in 30 holes
    let wear = 132.0_f64.to_radians() / 2.0 * (620.0_f64.powi(2) - 610.0_f64.powi(2)) * 300.0;

    let vol = |name: &str| solidcraft_engine::kernel::measure(&body(&s, name)).unwrap().volume;
    for id in ["S1", "S2"] {
        let check = |what: &str, measured: f64, expected: f64| {
            let d = (measured - expected) / expected;
            eprintln!("{id} {what}: measured {measured:.0} expected {expected:.0} ({:+.4} %)", d * 100.0);
            assert!(d.abs() < 1e-3, "{id} {what}: {measured} vs {expected}");
        };
        check("web", vol(&format!("Vessel {id} web")), web_area * 12.0);
        let ribs: f64 = (1..=4).flat_map(|i| ["a", "b"].map(|side| vol(&format!("Vessel {id} rib {i}{side}")))).sum();
        check("ribs", ribs, rib_total);
        check("base plate", vol(&format!("Vessel {id} base plate")), base);
        check("wear plate", vol(&format!("Vessel {id} wear plate")), wear);

        // The wear plate conforms: every vertex of its inner face is on the shell's outside.
        let mesh = body(&s, &format!("Vessel {id} wear plate")).tessellate(0.01).unwrap();
        let mut inner = 0;
        for p in &mesh.positions {
            let rho = (p.y * p.y + p.z * p.z).sqrt();
            if rho < ro + 5.0 {
                inner += 1;
                assert!((rho - ro).abs() <= 0.01, "{id}: wear plate vertex at radius {rho}, shell at {ro}");
            }
        }
        assert!(inner > 10, "{id}: {inner} inner vertices");
    }
    // Where `axial` puts them: 1000 and 4000 from the left tangent line, 3000 apart.
    let centre = |name: &str| {
        let b = body(&s, name).tessellate(0.1).unwrap();
        let xs: Vec<f64> = b.positions.iter().map(|p| p.x).collect();
        (xs.iter().cloned().fold(f64::MAX, f64::min) + xs.iter().cloned().fold(f64::MIN, f64::max)) / 2.0
    };
    let (x1, x2) = (centre("Vessel S1 web"), centre("Vessel S2 web"));
    assert!((x1 - 1000.0).abs() < 0.01 && (x2 - 4000.0).abs() < 0.01, "{x1} {x2}");
    // Parameters for the table; one undo takes the vessel and its saddles away.
    let params: Vec<&str> = r["parameters"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    for p in ["V201_S1_position", "V201_S2_position", "V201_S1_wear_angle", "V201_S1_rib_count"] {
        assert!(params.contains(&p), "{p}: {params:?}");
    }
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.model.state().bodies.iter().all(|b| !b.name.starts_with("Vessel S")));
}

#[test]
fn saddles_refuse_what_they_cannot_carry() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let saddle = json!({"id": "S1", "kind": "saddle", "host": "E1", "axial": {"ref": "datum", "mm": 500}, "width_mm": 200, "height_mm": 600,
        "web_thk_mm": 12, "rib_count": 2, "rib_thk_mm": 10, "base_plate": {"len_mm": 900, "wid_mm": 200, "thk_mm": 16}});
    let cyl = |orientation: &str, datum: &str| {
        json!({"schema": "triya.vessel/0.1", "id": "T9", "global": {"orientation": orientation, "datum": datum},
            "elements": [{"id": "E1", "kind": "cylinder", "diameter_mm": 1000, "length_mm": 2000, "thickness_mm": 10}], "attachments": [saddle.clone()]})
    };
    // A vertical vessel does not stand on saddles: an error, nothing built.
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": cyl("vertical", "bottom_tl")})).unwrap();
    let e = s.execute("vessel.regenerate", &json!({})).unwrap_err().to_string();
    assert!(e.contains("horizontal"), "{e}");
    assert!(s.model.state().bodies.is_empty());
    // Too low for its own base plate and the shell's bottom.
    let mut doc = cyl("horizontal", "left_tl");
    doc["attachments"][0]["height_mm"] = json!(10);
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let e = s.execute("vessel.regenerate", &json!({})).unwrap_err().to_string();
    assert!(e.contains("no room"), "{e}");
    // Off its host.
    let mut doc = cyl("horizontal", "left_tl");
    doc["attachments"][0]["axial"]["mm"] = json!(2500);
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let e = s.execute("vessel.regenerate", &json!({})).unwrap_err().to_string();
    assert!(e.contains("not on its host"), "{e}");
}
