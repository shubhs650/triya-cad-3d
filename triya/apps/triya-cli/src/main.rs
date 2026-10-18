// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D headless command line: upstream's `solidcraft-cli` (every subcommand: `run`,
//! `eval`, `snapshot`, `commands`, `mcp`, …) with the overlay's `vessel.*` commands registered
//! before the first `Session`. The upstream source is compiled as is (see `build.rs`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

include!(concat!(env!("OUT_DIR"), "/upstream_cli.rs"));

fn main() -> ExitCode {
    if let Err(e) = solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS) {
        eprintln!("triya-cli: {e}");
        return ExitCode::FAILURE;
    }
    upstream_main()
}
