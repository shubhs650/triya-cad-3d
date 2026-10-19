// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T4.5 acceptance: `vessel.bom` weighs every shell element and support part of the reference
//! vessels from its measured body; each vessel's weight is within 2 % of the handbook's (heads:
//! F&D 0.9286·D², Klöpper 0.99·D² + π·D·h₁, …; supports: closed forms), every item within 2 %
//! except 2:1 ellipsoidal heads (within 4 %: the model's outside is the ellipse of semi-axes plus
//! t, a little thinner than a constant-thickness head between crown and equator), and the
//! totals add up for empty, operating and test.

use serde_json::{Value, json};
use solidcraft_engine::Session;

fn reference(name: &str) -> Value {
    serde_json::from_str(match name {
        "a" => include_str!("reference/vessel-a.json"),
        "b" => include_str!("reference/vessel-b.json"),
        _ => include_str!("reference/vessel-c.json"),
    })
    .unwrap()
}

/// Regenerate `doc` and return its BOM.
fn bom(doc: Value) -> (Session, Value, Value) {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let r = s.execute("vessel.regenerate", &json!({})).unwrap();
    let b = s.execute("vessel.bom", &json!({})).unwrap();
    (s, r, b)
}

fn without_nozzles(mut doc: Value) -> Value {
    doc["attachments"] = json!(doc["attachments"].as_array().unwrap().iter().filter(|a| a["kind"] != "nozzle").cloned().collect::<Vec<_>>());
    doc
}

/// Every item with a measured weight and a handbook value agrees within `tol` % (2:1 heads 4 %);
/// so does the sum over those items. → how many items were compared.
fn check(name: &str, b: &Value, tol: f64) -> usize {
    let mut n = 0;
    let (mut w, mut h) = (0.0, 0.0);
    for it in b["items"].as_array().unwrap() {
        eprintln!(
            "{name} {:>4} {:<28} {:<40} {:>10} kg  handbook {:>10} ({})",
            it["id"].as_str().unwrap_or(""),
            it["body"].as_str().unwrap_or("-"),
            it["description"].as_str().unwrap_or(""),
            it["weight_kg"],
            it["handbook_kg"],
            it["delta_pct"]
        );
        if let Some(d) = it["delta_pct"].as_f64() {
            let ellipsoidal = it["handbook"].as_str().unwrap_or("").starts_with("1.084");
            assert!(d.abs() < if ellipsoidal { 2.0 * tol } else { tol }, "{name}: {it}");
            w += it["weight_kg"].as_f64().unwrap();
            h += it["handbook_kg"].as_f64().unwrap();
            n += 1;
        }
    }
    eprintln!("{name}: {w:.1} kg measured, {h:.1} kg handbook ({:+.2} %)", (w / h - 1.0) * 100.0);
    assert!((w / h - 1.0).abs() * 100.0 < tol, "{name}: {w} vs {h}");
    n
}

#[test]
fn reference_vessels_weigh_within_two_percent_of_the_handbook() {
    for v in ["a", "b", "c"] {
        let (_, r, b) = bom(without_nozzles(reference(v)));
        for sp in r["supports"].as_array().unwrap() {
            assert_eq!(sp["built"], "built", "{v}: {sp}");
        }
        let checked = check(v, &b, 2.0);
        assert!(checked >= 5, "{v}: {b:#}");
        // Every head and shell is weighed from its body.
        for it in b["items"].as_array().unwrap() {
            assert_ne!(it["basis"], "not built", "{v}: {it}");
        }
        // Totals: empty is the sum of the items; test adds the water.
        let sum: f64 = b["items"].as_array().unwrap().iter().filter_map(|i| i["weight_kg"].as_f64()).sum();
        let t = &b["totals"];
        assert!((t["empty_kg"].as_f64().unwrap() - sum).abs() < 0.1, "{v}: {t}");
        let water = t["water_m3"].as_f64().unwrap();
        assert!(water > 0.5, "{v}: {t}");
        assert!((t["test_kg"].as_f64().unwrap() - (sum + water * 1000.0)).abs() < 1.0, "{v}: {t}");
        eprintln!("{v}: totals {t}");
    }
}

#[test]
fn vessel_b_capacity_matches_its_closed_form() {
    // Vessel B: 1600 ID Klöpper heads (h = 0.1935·Da − 0.455·t… taken here from the shape) on a
    // cylinder; the integrated inside volume against cylinder + two Klöpper dishes (0.1 Da³ by
    // the handbook, within 2 %).
    let doc = without_nozzles(reference("b"));
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc.clone()})).unwrap();
    let b = s.execute("vessel.bom", &json!({})).unwrap();
    assert!(b["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap_or("").contains("regenerate")), "{b}");
    let inspect = s.execute("vessel.inspect", &json!({})).unwrap();
    let st = inspect["stations"].as_array().unwrap();
    let cyl = st.iter().find(|x| x["kind"] == "cylinder").unwrap();
    let ri = cyl["r_from"].as_f64().unwrap();
    let len = cyl["z_to"].as_f64().unwrap() - cyl["z_from"].as_f64().unwrap();
    let mut expect = std::f64::consts::PI * ri * ri * len;
    for h in st.iter().filter(|x| x["kind"] == "head") {
        let sf = h["profile"]["sf"].as_f64().unwrap_or(0.0);
        let da = 2.0 * (ri + h["thickness"].as_f64().unwrap());
        expect += std::f64::consts::PI * ri * ri * sf + 0.1 * da.powi(3) * (2.0 * ri / da).powi(3);
    }
    let got = b["totals"]["water_m3"].as_f64().unwrap() * 1e9;
    assert!((got / expect - 1.0).abs() < 0.02, "{got} vs {expect}");
}
