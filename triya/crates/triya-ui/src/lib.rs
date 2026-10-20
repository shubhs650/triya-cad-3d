// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D's hooks on the SolidCraft egui front end (L5, beside `solidcraft-ui-egui`).
//!
//! Upstream's toolbar lists its tabs in `solidcraft_ui_egui::toolbar::TABS` and fills each tab's
//! panels from `CommandSpec.tab`/`.panel` automatically (`toolbar_custom::panel_lists`), so the
//! `vessel.*` commands appear on the toolbar as soon as they are registered and `TABS` holds
//! `VESSEL` (the overlay's one-line seam in `crates/ui-egui/src/toolbar.rs`). What this crate adds:
//!
//! - [`register`]: puts `triya_vessel::COMMANDS` into the engine's registry (call it once, before
//!   the first `Session`).
//! - [`VesselPanel`]: the side panel shown while the `VESSEL` tab is active ([`vessel`]): the
//!   stack editor, the vessel JSON, Regenerate off the window thread with a progress bar. It reads
//!   engine state and acts only through `app.run("vessel.*", params)`, like every upstream panel.
//! - [`about_text`]: the brand and attribution sentence for the app shell's About page.
//! - [`about::About`]: the About window, which takes over upstream's Help ▸ About.
//! - [`icons`]: the code-drawn Triya mark as egui images and the window icon.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod about;
pub mod icons;
pub mod vessel;

pub use vessel::VesselPanel;

/// Register the overlay's commands with the engine. Idempotent; `Err` names a clashing id.
pub fn register() -> Result<(), String> {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS)
}

/// The brand and attribution line (`triya_brand::about_text`).
pub fn about_text() -> String {
    triya_brand::about_text()
}

#[cfg(test)]
mod tests {
    #[test]
    fn register_is_idempotent_and_about_names_upstream() {
        assert!(super::register().is_ok());
        assert!(super::register().is_ok());
        assert!(solidcraft_engine::find_command("vessel.regenerate").is_some());
        assert!(super::about_text().contains("SolidCraft"));
        assert!(solidcraft_ui_egui::toolbar::TABS.contains(&triya_vessel::commands::TAB), "the VESSEL tab seam is in place");
        assert_eq!(solidcraft_ui_egui::toolbar::panels(triya_vessel::commands::TAB), triya_vessel::commands::PANELS, "the panels seam is in place");
        // Every vessel command is on one of those panels, so the ribbon shows each of them.
        let lists = solidcraft_ui_egui::toolbar_custom::panel_lists(triya_vessel::commands::TAB);
        for spec in triya_vessel::COMMANDS {
            assert_eq!(lists.iter().filter(|(_, cmds, _)| cmds.iter().any(|c| c.id == spec.id)).count(), 1, "{}", spec.id);
        }
    }
}
