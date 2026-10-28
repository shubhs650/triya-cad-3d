// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! A new install starts on Triya's warm light theme (Clay); preferences someone already saved,
//! dark included, load unchanged.

use solidcraft_engine::Session;
use solidcraft_ui_egui::{Services, SolveApp};

fn app() -> SolveApp {
    SolveApp::new(Session::default(), Services::default())
}

#[test]
fn a_new_install_starts_on_clay_with_a_light_view() {
    let mut a = app();
    triya_ui::load_prefs_or_defaults(&mut a, None);
    assert_eq!(a.preferences.theme, triya_ui::DEFAULT_THEME);
    assert_eq!(triya_ui::DEFAULT_THEME, "clay");
    assert!(solidcraft_ui_egui::theme_files::find("clay", &a.preferences.custom_themes).is_some_and(|t| !t.base.is_dark()), "Clay is a built-in light theme");
    assert_eq!(a.preferences.viewport_theme, solidcraft_ui_egui::theme::ViewportTheme::Theme, "the 3D view follows the theme");
}

#[test]
fn saved_preferences_keep_their_theme() {
    for theme in ["dark", "light", "system", "gruvbox-dark"] {
        let mut a = app();
        let mut saved = app();
        saved.preferences.theme = theme.into();
        triya_ui::load_prefs_or_defaults(&mut a, Some(saved.prefs()));
        assert_eq!(a.preferences.theme, theme);
    }
}
