// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Phase 3 exit: reference vessel A's six nozzles (radial with pad, hillside, tangential, tilted
//! with inside projection, normal to the bottom head, manway parallel to the axis on the top
//! head), each with its B16.5 flange, build on the regenerated shell; anything the kernel cannot
//! join degrades to a separate body with its flag, and the regeneration never fails.

use serde_json::{Value, json};
use solidcraft_engine::Session;

/// About 17 minutes (two nozzles take the slow fallback), so it runs in its own CI job
/// (`triya-ci.yml`, "Phase 3 exit") with `--ignored` rather than in the crates job.
#[test]
#[ignore = "runs in its own CI job: cargo test -p triya-vessel --test phase3_exit -- --ignored"]
fn vessel_a_builds_its_six_nozzles() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let doc: Value = serde_json::from_str(include_str!("reference/vessel-a.json")).unwrap();
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let t = std::time::Instant::now();
    let r = s.execute("vessel.regenerate", &json!({})).expect("the regeneration never fails on a kernel failure");
    eprintln!("regenerate: {:?}\nnozzles: {:#}\nwarnings: {:#}", t.elapsed(), r["nozzles"], r["warnings"]);
    let nozzles = r["nozzles"].as_array().unwrap();
    let ids: Vec<&str> = nozzles.iter().filter_map(|n| n["id"].as_str()).collect();
    assert_eq!(ids, ["N1", "N2", "N3", "N4", "N5", "M1"]);
    let names: Vec<String> = s.model.state().bodies.iter().map(|b| b.name.clone()).collect();
    for n in nozzles {
        let id = n["id"].as_str().unwrap();
        match n["built"].as_str() {
            Some("joined") => {}
            Some("separate") => assert!(names.contains(&format!("Vessel {id} neck (separate)")), "{names:?}"),
            other => panic!("{id}: {other:?} {n}"),
        }
        // Every nozzle of vessel A has a flange: built (holes patterned or cut one by one) or
        // the plain ring, never lost with the neck.
        assert!(matches!(n["flange"].as_str(), Some("built" | "without bolt holes")), "{id}: {n}");
        assert!(names.contains(&format!("Vessel {id} flange")), "{id}: {names:?}");
    }
    // The flags in the stored document say which ones fell back.
    let inspect = s.execute("vessel.inspect", &json!({})).unwrap();
    let separate: Vec<&str> =
        nozzles.iter().filter(|n| n["built"] == "separate" || n["flange"] != "built").filter_map(|n| n["id"].as_str()).collect();
    let flagged: Vec<&str> =
        inspect["flags"].as_array().unwrap().iter().filter(|f| f["boolean_failed"] == true).filter_map(|f| f["id"].as_str()).collect();
    assert_eq!(separate, flagged);
    // The supports and details of Phase 4 are reported, not built.
    for id in ["L1", "L2", "R1", "C1", "AC1"] {
        assert!(r["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap_or("").contains(&format!("`{id}`"))), "{id}");
    }
}
