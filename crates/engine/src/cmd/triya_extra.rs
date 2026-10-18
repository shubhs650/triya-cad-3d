// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Modified by Triya CAD 3D: an extension slot for command slices registered by crates *above*
//! the engine.
//!
//! The overlay's commands (`triya-vessel`) call engine commands, so `triya-vessel` depends on
//! `solidcraft-engine`; the engine therefore cannot list `triya_vessel::COMMANDS` in
//! [`super::command_specs`] without a dependency cycle. Instead an app registers the slice here
//! at startup (before its first `Session`), and `command_specs()` appends every registered slice,
//! so the toolbar, palette, scripts, CLI, control channel and MCP all see the same commands.
//!
//! Registration is idempotent for the same slice and refuses an id that is already a command,
//! so `registry_is_consistent` keeps holding. A poisoned lock is treated as "nothing registered"
//! (never a panic).

use std::sync::RwLock;

use super::CommandSpec;

static EXTRA: RwLock<Vec<&'static [CommandSpec]>> = RwLock::new(Vec::new());

/// Add `specs` to the registry. `Err` names the first id that is already a command (from the
/// built-in registry or another registered slice); registering the same slice twice is fine.
pub fn register(specs: &'static [CommandSpec]) -> Result<(), String> {
    // Built-in commands and the slices registered so far (its read lock is released before the
    // write lock below is taken: the lock is not reentrant).
    let existing = super::command_specs();
    let mut extra = EXTRA.write().map_err(|_| "command registry lock is poisoned".to_string())?;
    if extra.iter().any(|s| std::ptr::eq(s.as_ptr(), specs.as_ptr()) && s.len() == specs.len()) {
        return Ok(());
    }
    for spec in specs {
        if existing.iter().any(|c| c.id.eq_ignore_ascii_case(spec.id))
            || extra.iter().flat_map(|s| s.iter()).any(|c| c.id.eq_ignore_ascii_case(spec.id))
        {
            return Err(format!("command id `{}` is already registered", spec.id));
        }
        if specs.iter().filter(|c| c.id.eq_ignore_ascii_case(spec.id)).count() > 1 {
            return Err(format!("command id `{}` appears twice in the slice", spec.id));
        }
    }
    extra.push(specs);
    Ok(())
}

/// Append every registered slice to `v` (nothing when none is registered or the lock is poisoned).
pub(super) fn extend(v: &mut Vec<&'static CommandSpec>) {
    if let Ok(extra) = EXTRA.read() {
        for slice in extra.iter() {
            v.extend(slice.iter());
        }
    }
}
