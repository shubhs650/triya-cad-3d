// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Builds `triya-cli` from upstream's CLI source without editing it.
//!
//! `apps/solidcraft-cli/src/main.rs` is a binary crate root whose `main` is private, so it can
//! neither be called as a library nor wrapped from a `#[path]` module. This script writes a copy to
//! `$OUT_DIR/upstream_cli.rs` with four mechanical changes, and `src/main.rs` `include!`s it at
//! its own crate root (so upstream's `crate::` paths keep working):
//!
//! - the crate-level `//!` docs and `#![…]` attributes are dropped (`src/main.rs` carries its own);
//! - `fn main() -> ExitCode {` becomes `fn upstream_main() -> ExitCode {`;
//! - `mod x;` and relative `#[path = "…"]` lines point at upstream's files by absolute path;
//! - the MCP server's output goes through `crate::triya_mcp::Renamed` (`serverInfo` says Triya).
//!
//! Any of those anchors missing (upstream changed shape) fails the build with a message, never a
//! silently different CLI.

use std::path::{Path, PathBuf};

fn main() -> Result<(), String> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| format!("CARGO_MANIFEST_DIR: {e}"))?);
    let src = manifest.join("../../../apps/solidcraft-cli/src");
    let src = src.canonicalize().map_err(|e| format!("{}: {e}", src.display()))?;
    let main_rs = src.join("main.rs");
    let text = std::fs::read_to_string(&main_rs).map_err(|e| format!("{}: {e}", main_rs.display()))?;
    println!("cargo:rerun-if-changed={}", src.display());
    if let Ok(dir) = std::fs::read_dir(&src) {
        for entry in dir.flatten() {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }

    let out_text = rewrite(&text, &src)?;
    let out = PathBuf::from(std::env::var("OUT_DIR").map_err(|e| format!("OUT_DIR: {e}"))?).join("upstream_cli.rs");
    std::fs::write(&out, out_text).map_err(|e| format!("{}: {e}", out.display()))
}

fn rewrite(text: &str, src: &Path) -> Result<String, String> {
    const MAIN: &str = "fn main() -> ExitCode {";
    const SERVE: &str = ".serve(stdin.lock(), stdout.lock())";
    let mut out = String::with_capacity(text.len() + 1024);
    let (mut mains, mut mods, mut serves) = (0, 0, 0);
    // A `mod x;` right after a `#[path]` line already says where its file is.
    let mut after_path = false;
    for line in text.lines() {
        let was_after_path = std::mem::replace(&mut after_path, false);
        let t = line.trim();
        if t.starts_with("//!") || t.starts_with("#![") {
            continue;
        }
        if line == MAIN {
            mains += 1;
            out.push_str("fn upstream_main() -> ExitCode {\n");
            continue;
        }
        if let Some(name) = t.strip_prefix("mod ").and_then(|r| r.strip_suffix(';'))
            && !was_after_path
            && !line.starts_with(' ')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            mods += 1;
            let file = src.join(format!("{name}.rs"));
            out.push_str(&format!("#[path = {:?}]\n{line}\n", file.display().to_string()));
            continue;
        }
        if let Some(rel) = t.strip_prefix("#[path = \"").and_then(|r| r.strip_suffix("\"]")) {
            let file = src.join(rel);
            out.push_str(&format!("#[path = {:?}]\n", file.display().to_string()));
            after_path = true;
            continue;
        }
        if line.contains(SERVE) {
            serves += 1;
            out.push_str(&line.replace(SERVE, ".serve(stdin.lock(), crate::triya_mcp::Renamed::new(stdout.lock()))"));
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if mains != 1 {
        return Err(format!("upstream CLI: expected one `{MAIN}` line in main.rs, found {mains}"));
    }
    if serves != 1 {
        return Err(format!("upstream CLI: expected one `{SERVE}` (the MCP server) in main.rs, found {serves}"));
    }
    if mods == 0 {
        return Err("upstream CLI: no `mod …;` lines found in main.rs".into());
    }
    Ok(out)
}
