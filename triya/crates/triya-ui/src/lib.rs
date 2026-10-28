// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D's hooks on the SolidCraft egui front end (L5, beside `solidcraft-ui-egui`).
//!
//! Upstream's toolbar lists its tabs in `solidcraft_ui_egui::toolbar::TABS` and fills each tab's
//! panels from `CommandSpec.tab`/`.panel` automatically (`toolbar_custom::panel_lists`), so the
//! `vessel.*` commands appear on the toolbar as soon as they are registered and `TABS` holds
//! `VESSEL` (the overlay's one-line seam in `crates/ui-egui/src/toolbar.rs`). What this crate adds:
//!
//! - [`register`]: puts `triya_vessel::COMMANDS` and `triya_drawing::COMMANDS` into the engine's
//!   registry (call it once, before the first `Session`).
//! - [`VesselPanel`]: the side panel shown while the `VESSEL` tab is active ([`vessel`]): the
//!   stack editor, the vessel JSON, Regenerate off the window thread with a progress bar. It reads
//!   engine state and acts only through `app.run("vessel.*", params)`, like every upstream panel.
//! - [`load_prefs_or_defaults`]: saved preferences, or the Clay theme on a new install.
//! - [`about_text`]: the brand and attribution sentence for the app shell's About page.
//! - [`about::About`]: the About window, which takes over upstream's Help ▸ About.
//! - [`icons`]: the code-drawn Triya mark as egui images and the window icon.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod about;
pub mod icons;
pub mod vessel;

pub use vessel::VesselPanel;

/// The Help menu's About item.
pub const ABOUT_LABEL: &str = "About Triya CAD 3D";

/// Register the overlay's commands with the engine and name the product in upstream's app bar,
/// start page and Help menu, with Triya's mark in the app bar (seams `toolbar::APP_NAME`, `BRAND_MARK`). Idempotent; `Err` names a clashing id.
pub fn register() -> Result<(), String> {
    // Already set (a second call) is fine: the value is the same.
    let _ = solidcraft_ui_egui::toolbar::APP_NAME.set((triya_brand::APP_NAME, ABOUT_LABEL));
    let _ = solidcraft_ui_egui::toolbar::BRAND_MARK.set(icons::paint_app_bar_mark);
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS)?;
    solidcraft_engine::cmd::triya_extra::register(triya_drawing::COMMANDS)
}

/// The theme a new install starts on: upstream's built-in Clay, a warm light theme (owner
/// decision 2026-10-28). Upstream's own default is `dark`.
pub const DEFAULT_THEME: &str = "clay";

/// Restore the preferences the shell saved (`prefs::stored`), or, when there are none (a new
/// install), start on [`DEFAULT_THEME`]. Saved preferences keep their theme, dark included.
pub fn load_prefs_or_defaults(app: &mut solidcraft_ui_egui::SolveApp, stored: Option<String>) {
    match stored {
        Some(p) => app.load_prefs(&p),
        None => {
            app.preferences.theme = DEFAULT_THEME.into();
            app.ui.dark = false;
        }
    }
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
        assert!(solidcraft_engine::find_command("drawing.export").is_some());
        assert_eq!(solidcraft_ui_egui::toolbar::app_name(), (triya_brand::APP_NAME, super::ABOUT_LABEL));
        assert!(super::about_text().contains("SolidCraft"));
        assert!(solidcraft_ui_egui::toolbar::TABS.contains(&triya_vessel::commands::TAB), "the VESSEL tab seam is in place");
        assert_eq!(solidcraft_ui_egui::toolbar::panels(triya_vessel::commands::TAB), triya_vessel::commands::PANELS, "the panels seam is in place");
        // Every vessel command is on one of those panels, so the ribbon shows each of them.
        let lists = solidcraft_ui_egui::toolbar_custom::panel_lists(triya_vessel::commands::TAB);
        for spec in triya_vessel::COMMANDS.iter().chain(triya_drawing::COMMANDS) {
            assert_eq!(lists.iter().filter(|(_, cmds, _)| cmds.iter().any(|c| c.id == spec.id)).count(), 1, "{}", spec.id);
        }
    }
}
