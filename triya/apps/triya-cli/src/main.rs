// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D headless command line: upstream's `solidcraft-cli` (every subcommand: `run`,
//! `eval`, `snapshot`, `commands`, `mcp`, …) with the overlay's `vessel.*` and `drawing.*` commands registered
//! before the first `Session`, and the MCP server reporting `serverInfo.name = "triya-cad-3d"`
//! ([`triya_mcp`]). The upstream source is compiled as is (see `build.rs`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod triya_mcp;

include!(concat!(env!("OUT_DIR"), "/upstream_cli.rs"));

fn main() -> ExitCode {
    for commands in [triya_vessel::COMMANDS, triya_drawing::COMMANDS] {
        if let Err(e) = solidcraft_engine::cmd::triya_extra::register(commands) {
            eprintln!("triya-cli: {e}");
            return ExitCode::FAILURE;
        }
    }
    upstream_main()
}
