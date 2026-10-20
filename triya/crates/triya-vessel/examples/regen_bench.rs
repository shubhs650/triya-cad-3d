// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T7.2: time `vessel.regenerate` for a vessel JSON file, natively or as WebAssembly.
//!
//! ```text
//! cargo run --release -p triya-vessel --example regen_bench -- <vessel.json> [--shell-only] [--no NOZZLE,…] [--no-join]
//! # The same code in V8's WebAssembly engine (Chrome's), with Node's WASI:
//! cargo build --release -p triya-vessel --example regen_bench --target wasm32-wasip1
//! # then run regen_bench.wasm under Node's `node:wasi` (preview1, `.` preopened), same arguments
//! ```
//!
//! Prints one JSON line: the times of `vessel.new`, the plan (`vessel.inspect`) and the
//! regeneration, and what each nozzle became. `--shell-only` drops every attachment; `--no`
//! drops the named ones; `--no-join` builds nozzles as separate bodies (`join_nozzles: false`);
//! `--steps-out FILE` writes the steps the regeneration ran, and `--replay FILE` runs such steps one
//! by one, naming each on stderr (to find the step that stops a WebAssembly run).

use serde_json::{Value, json};
use solidcraft_engine::Session;
use std::time::Instant;

fn main() {
    if let Err(e) = run() {
        eprintln!("regen_bench: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().ok_or("usage: regen_bench <vessel.json> [--shell-only] [--no N1,N4]")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut doc: Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let shell_only = args.iter().any(|a| a == "--shell-only");
    let drop: Vec<String> =
        args.iter().position(|a| a == "--no").and_then(|i| args.get(i + 1)).map(|s| s.split(',').map(str::to_string).collect()).unwrap_or_default();
    if let Some(list) = doc.get_mut("attachments").and_then(Value::as_array_mut) {
        list.retain(|a| !shell_only && !drop.iter().any(|d| a.get("id").and_then(Value::as_str) == Some(d)));
    }
    triya_vessel_register()?;
    let mut s = Session::default();
    // `--replay steps.json`: run a regeneration's steps one by one, naming each on stderr first
    // (finds the step that stops a WebAssembly run, where a panic cannot be caught).
    if let Some(steps) = args.iter().position(|a| a == "--replay").and_then(|i| args.get(i + 1)) {
        let text = std::fs::read_to_string(steps).map_err(|e| format!("{steps}: {e}"))?;
        let steps: Vec<Value> = serde_json::from_str(&text).map_err(|e| format!("{steps}: {e}"))?;
        for (i, step) in steps.iter().enumerate() {
            let id = step["command"].as_str().unwrap_or("");
            eprintln!("step {i}: {id} {}", step["params"]);
            let t = Instant::now();
            let r = s.execute(id, &step["params"]);
            eprintln!("  {:.2} s {}", t.elapsed().as_secs_f64(), r.as_ref().map_or_else(|e| format!("error: {e}"), |_| "ok".into()));
        }
        return Ok(());
    }
    let t0 = Instant::now();
    s.execute("vessel.new", &json!({"doc": doc})).map_err(|e| e.to_string())?;
    let t_new = t0.elapsed().as_secs_f64();
    let t1 = Instant::now();
    let inspect = s.execute("vessel.inspect", &json!({})).map_err(|e| e.to_string())?;
    let t_plan = t1.elapsed().as_secs_f64();
    let t2 = Instant::now();
    let r = s.execute("vessel.regenerate", &json!({"join_nozzles": !args.iter().any(|a| a == "--no-join")})).map_err(|e| e.to_string())?;
    let t_regen = t2.elapsed().as_secs_f64();
    // `--steps-out FILE`: the steps the regeneration ran, for `--replay`.
    if let Some(out) = args.iter().position(|a| a == "--steps-out").and_then(|i| args.get(i + 1)) {
        std::fs::write(out, r["steps"].to_string()).map_err(|e| format!("{out}: {e}"))?;
    }
    let nozzles: Vec<Value> =
        r["nozzles"].as_array().map(|a| a.iter().map(|n| json!({"id": n["id"], "built": n["built"]})).collect()).unwrap_or_default();
    let out = json!({
        "file": path,
        "target": std::env::consts::ARCH,
        "attachments": doc["attachments"].as_array().map_or(0, Vec::len),
        "plan_steps": inspect["plan"].as_array().map_or(0, Vec::len),
        "new_s": t_new,
        "plan_s": t_plan,
        "regenerate_s": t_regen,
        "features": r["features"].as_array().map_or(0, Vec::len),
        "nozzles": nozzles,
        "warnings": r["warnings"].as_array().map_or(0, Vec::len),
    });
    println!("{out}");
    Ok(())
}

fn triya_vessel_register() -> Result<(), String> {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS)
}
