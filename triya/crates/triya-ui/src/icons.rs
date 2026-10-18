// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The Triya CAD 3D mark for the egui shells, drawn in code by [`triya_brand::mark`].
//!
//! The web app's PNG icons are exports of the same drawing ([`EXPORTS`]); the
//! `icons_match_the_code_drawn_mark` test fails when they drift, and
//! `TRIYA_WRITE_ICONS=1 cargo test -p triya-ui --test icons` rewrites them.

pub use triya_brand::mark::Variant;

/// Every PNG export of the mark: path under `triya/apps/triya-web/`, pixel size, tile variant.
pub const EXPORTS: &[(&str, u32, Variant)] = &[
    ("icons/32x32/in.openengtools.triya.png", 32, Variant::Rounded),
    ("icons/192x192/in.openengtools.triya.png", 192, Variant::Rounded),
    ("icons/256x256/in.openengtools.triya.png", 256, Variant::Rounded),
    ("icons/512x512/in.openengtools.triya.png", 512, Variant::Rounded),
    ("icons/512x512/in.openengtools.triya-maskable.png", 512, Variant::Maskable),
];

/// The mark as an egui image (`size` pixels square), e.g. for the About dialog; `None` when
/// `size` is 0 or above `triya_brand::mark::MAX_SIZE`.
pub fn mark_image(size: u32) -> Option<egui::ColorImage> {
    let rgba = triya_brand::mark::rgba(size, Variant::Rounded)?;
    let side = usize::try_from(size).ok()?;
    Some(egui::ColorImage::from_rgba_unmultiplied([side, side], &rgba))
}

/// The mark as a window icon for the desktop shell.
pub fn window_icon(size: u32) -> Option<egui::IconData> {
    let rgba = triya_brand::mark::rgba(size, Variant::Rounded)?;
    Some(egui::IconData { rgba, width: size, height: size })
}
