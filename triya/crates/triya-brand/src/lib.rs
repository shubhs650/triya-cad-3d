// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Triya CAD 3D: the product's name, identifiers and its attribution to upstream SolidCraft.
//!
//! Lowest layer of the Triya overlay (no dependencies, L0 standalone in `cargo xtask layers`),
//! so every other overlay crate and both apps name the product from one place.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod mark;

/// The product name shown to people.
pub const APP_NAME: &str = "Triya CAD 3D";
/// Reverse-DNS application id (desktop entries, the web manifest `id`, metadata keys).
pub const APP_ID: &str = "in.openengtools.triya";
/// The organisation that publishes Triya CAD 3D.
pub const ORG: &str = "Open Engineering Tools";
/// Triya CAD 3D's own version (the `triya-vX.Y.Z` tag it is released as). The crates keep
/// upstream's workspace version, which says which SolidCraft release is underneath.
pub const VERSION: &str = "0.1.0-dev";
/// The product's home page (the GitHub Pages site for the beta; PROGRESS.md "Decisions").
pub const HOMEPAGE: &str = "https://shubhs650.github.io/triya-cad-3d/";
/// The product's source repository.
pub const REPO: &str = "https://github.com/shubhs650/triya-cad-3d";
/// The name the MCP server reports in `serverInfo.name` (`triya-cli mcp`).
pub const MCP_SERVER_NAME: &str = "triya-cad-3d";
/// The upstream project Triya CAD 3D is a downstream fork of.
pub const UPSTREAM_NAME: &str = "SolidCraft";
/// The upstream release this overlay was built against.
pub const UPSTREAM_VERSION: &str = "0.14.0";
/// The upstream commit (`v0.14.0`) this overlay was built against.
pub const UPSTREAM_SHA: &str = "2bb9c81";
/// The upstream project's repository.
pub const UPSTREAM_REPO: &str = "https://github.com/bherbruck/solidcraft";
/// The licence both the upstream code and the overlay carry.
pub const LICENSE: &str = "MIT OR Apache-2.0";

/// The attribution sentence for the About dialog, the web page and the docs.
pub fn about_text() -> String {
    format!(
        "{APP_NAME} is based on {UPSTREAM_NAME} v{UPSTREAM_VERSION} (commit {UPSTREAM_SHA}, {UPSTREAM_REPO}), \
         © {UPSTREAM_NAME} contributors, {LICENSE}. Not affiliated with the {UPSTREAM_NAME} project. \
         {APP_NAME} is published by {ORG} ({HOMEPAGE})."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_text_names_both_products_and_the_licence() {
        let t = about_text();
        assert!(t.starts_with("Triya CAD 3D is based on SolidCraft v0.14.0"));
        assert!(t.contains("© SolidCraft contributors, MIT OR Apache-2.0."));
        assert!(t.contains("Not affiliated with the SolidCraft project."));
        assert!(t.contains(UPSTREAM_SHA));
    }

    #[test]
    fn app_id_is_reverse_dns() {
        assert!(APP_ID.split('.').count() >= 2);
        assert!(APP_ID.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_'));
    }
}
