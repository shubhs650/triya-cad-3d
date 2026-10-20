// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The built `triya-cli` knows the vessel and drawing commands and exports the example cylinder as
//! STEP and its GA drawing as PDF.

use std::path::PathBuf;
use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_triya-cli"))
}

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/triya-vessel/examples/cylinder.json")
}

#[test]
fn commands_lists_the_vessel_and_drawing_commands_and_upstreams() {
    let out = cli().arg("commands").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8_lossy(&out.stdout);
    for id in
        ["vessel.new", "vessel.set", "vessel.regenerate", "vessel.inspect", "drawing.generate_ga", "drawing.export", "solid.revolve", "sketch.create"]
    {
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
fn run_writes_the_ga_drawing_of_a_script_as_pdf() {
    let dir = std::env::temp_dir();
    let pdf = dir.join(format!("triya-cli-ga-{}.pdf", std::process::id()));
    let script = dir.join(format!("triya-cli-ga-{}.json", std::process::id()));
    let steps = serde_json::json!({"commands": [
        {"command": "vessel.new", "params": {"diameter_mm": 1000, "thickness_mm": 10, "length_mm": 2000, "name": "Drum"}},
        {"command": "drawing.generate_ga", "params": {"size": "A3"}},
        {"command": "drawing.export", "params": {"path": pdf.to_string_lossy()}}
    ]});
    std::fs::write(&script, steps.to_string()).unwrap();
    let out = cli().arg("run").arg(&script).arg("--quiet").output().unwrap();
    let _ = std::fs::remove_file(&script);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let bytes = std::fs::read(&pdf).unwrap();
    let _ = std::fs::remove_file(&pdf);
    assert!(bytes.starts_with(b"%PDF-1.4"), "not a PDF");
    // One A3 landscape page (420 × 297 mm in points).
    assert!(String::from_utf8_lossy(&bytes).contains("/MediaBox [0 0 1190.551 841.89]"));
}

#[test]
fn eval_measures_the_example_cylinder() {
    let out = cli().arg("eval").arg(example()).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let text = v.to_string();
    assert!(text.contains("Vessel S1"), "{text}");
}

#[test]
fn mcp_handshake_reports_triya_and_lists_the_vessel_commands() {
    use std::io::Write;
    let mut child = cli()
        .arg("mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        // Two requests in one write, the second split across writes, then EOF ends the server.
        stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"t\",\"version\":\"0\"}}}\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .unwrap();
        stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"resources/read\",").unwrap();
        stdin.write_all(b"\"params\":{\"uri\":\"solidcraft://commands\"}}\n").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let init: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    let info = &init["result"]["serverInfo"];
    assert_eq!(info["name"], "triya-cad-3d", "{init}");
    assert_eq!(info["title"], "Triya CAD 3D");
    assert_eq!(info["version"], triya_brand::VERSION);
    let commands = lines.next().unwrap();
    assert!(commands.contains("vessel.regenerate"), "{commands}");
}
