// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T3.5 acceptance: a deliberately degenerate nozzle falls back to a separate body with
//! `flags.boolean_failed`, and the regeneration still succeeds.
//!
//! PLAN names equal-radius crossing cylinders, but the kernel joins that case correctly (a 6 in
//! STD neck on a 6 in STD pipe: +476,885 mm³ measured against 473,544 expected; an 8 in neck
//! through the same pipe joins too; 18 Oct 2026). The degenerate case here is a pad wider
//! than its shell, which no conforming plate can be.

use serde_json::json;
use solidcraft_engine::Session;

#[test]
fn a_pad_wider_than_its_shell_engages_the_fallback() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let doc = json!({
        "schema": "triya.vessel/0.1", "id": "T9",
        // 6 in STD pipe as the shell: OD 168.3.
        "elements": [{"id": "S1", "kind": "cylinder", "diameter_mm": 154.08, "length_mm": 800, "thickness_mm": 7.11}],
        "attachments": [
            {"id": "N1", "kind": "nozzle", "host": "S1", "axial": {"ref": "host_from", "mm": 400}, "angle_deg": 0, "frame": {"type": "radial"},
                "nps": "2", "schedule": "STD", "projection": {"from": "od", "mm": 150}, "pad": {"width_mm": 60, "thickness_mm": 6}}
        ],
    });
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": doc})).unwrap();
    let r = s.execute("vessel.regenerate", &json!({})).expect("a kernel failure never fails the regeneration");
    assert_eq!(r["nozzles"][0]["built"], json!("separate"), "{}", r["nozzles"]);
    assert!(r["nozzles"][0]["warning"].as_str().unwrap().contains("separate body"));
    let names: Vec<String> = s.model.state().bodies.iter().map(|b| b.name.clone()).collect();
    assert!(names.contains(&"Vessel N1 neck (separate)".to_string()), "{names:?}");
    let inspect = s.execute("vessel.inspect", &json!({})).unwrap();
    assert_eq!(inspect["flags"], json!([{"id": "N1", "boolean_failed": true, "unverified_table": false}]));
    // Undo takes the whole regeneration away, fallback included.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.model.state().bodies.is_empty());
}

/// The captured kernel cases (AGENTS.md §5) stay replayable: each names its symptom and uses
/// only commands the registry knows. They are not replayed here (each takes minutes).
#[test]
fn nozzle_kernel_cases_are_well_formed() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    for text in [
        include_str!("kernel-cases/nozzle-hole-then-equal-neck-join.json"),
        include_str!("kernel-cases/pad-rod-cylinder-trim.json"),
        include_str!("kernel-cases/rod-cut-by-revolved-dish.json"),
    ] {
        let case: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(case["note"].as_str().is_some_and(|n| n.contains("Fallback")), "{}", case["note"]);
        for step in case["commands"].as_array().unwrap() {
            let id = step["command"].as_str().unwrap();
            assert!(solidcraft_engine::find_command(id).is_some(), "unknown command {id}");
            assert!(step["params"].is_object());
        }
    }
}
