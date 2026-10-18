// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The built `triya-cli` knows the vessel commands and exports the example cylinder as STEP.

use std::path::PathBuf;
use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_triya-cli"))
}

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/triya-vessel/examples/cylinder.json")
}

#[test]
fn commands_lists_the_vessel_commands_and_upstreams() {
    let out = cli().arg("commands").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    for id in ["vessel.new", "vessel.set", "vessel.regenerate", "vessel.inspect", "solid.revolve", "sketch.create"] {
        assert!(text.contains(&format!("\"{id}\"")), "{id} missing from `triya-cli commands`");
    }
}

#[test]
fn run_exports_the_example_cylinder_as_step() {
    let out_path = std::env::temp_dir().join(format!("triya-cli-cyl-{}.step", std::process::id()));
    let out = cli().arg("run").arg(example()).arg("--out").arg(&out_path).arg("--quiet").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let step = std::fs::read_to_string(&out_path).unwrap();
    let _ = std::fs::remove_file(&out_path);
    assert!(step.starts_with("ISO-10303-21;"), "not a STEP file");
    assert!(step.contains("MANIFOLD_SOLID_BREP") || step.contains("BREP_WITH_VOIDS"), "no solid in the STEP file");
}

#[test]
fn eval_measures_the_example_cylinder() {
    let out = cli().arg("eval").arg(example()).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text = v.to_string();
    assert!(text.contains("Vessel S1"), "{text}");
}
