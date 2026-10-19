// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Phase 4 exit: reference vessels B (saddles, trunnions) and C (legs, brackets, insulation
//! ring, clip) regenerate whole, nozzles included, with every support and detail built, and
//! their BOM weighs within 2 % of the handbook (vessel A's supports and BOM are checked in the
//! Phase 3 exit, which regenerates it whole).

use serde_json::{Value, json};
use solidcraft_engine::Session;

/// Regenerate `doc` and check what Phase 4 promises; → the BOM.
pub fn check_vessel(name: &str, doc: Value) -> Value {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let t = std::time::Instant::now();
    let r = s.execute("vessel.regenerate", &json!({})).expect("the regeneration never fails on a kernel failure");
    eprintln!("{name}: regenerate {:?}\nwarnings: {:#}", t.elapsed(), r["warnings"]);
    for sp in r["supports"].as_array().unwrap() {
        assert_eq!(sp["built"], "built", "{name}: {sp}");
    }
    assert!(!r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap_or("").contains("not built yet")), "{name}");
    for n in r["nozzles"].as_array().unwrap() {
        assert!(matches!(n["built"].as_str(), Some("joined" | "separate")), "{name}: {n}");
    }
    let b = s.execute("vessel.bom", &json!({})).unwrap();
    let (mut w, mut h) = (0.0, 0.0);
    for it in b["items"].as_array().unwrap() {
        eprintln!(
            "{name} {:>4} {:<30} {:>10} kg  handbook {:>10} ({})",
            it["id"].as_str().unwrap_or(""),
            it["description"].as_str().unwrap_or(""),
            it["weight_kg"],
            it["handbook_kg"],
            it["delta_pct"]
        );
        assert_ne!(it["basis"], "not built", "{name}: {it}");
        if let (Some(x), Some(y)) = (it["weight_kg"].as_f64(), it["handbook_kg"].as_f64()) {
            w += x;
            h += y;
        }
    }
    eprintln!("{name}: {w:.1} kg, handbook {h:.1} kg; totals {}", b["totals"]);
    assert!((w / h - 1.0).abs() < 0.02, "{name}: {w} vs {h}");
    b
}

#[test]
#[ignore = "runs in its own CI job: cargo test -p triya-vessel --test phase4_exit -- --ignored"]
fn vessels_b_and_c_build_every_attachment_and_weigh_right() {
    for (name, text) in [("B", include_str!("reference/vessel-b.json")), ("C", include_str!("reference/vessel-c.json"))] {
        check_vessel(name, serde_json::from_str(text).unwrap());
    }
}
