// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! PLAN T1.1: the web app's icons are exports of the code-drawn mark, and the manifest and page
//! list exactly those files. `TRIYA_WRITE_ICONS=1` rewrites the PNGs from the code.

use std::path::PathBuf;
use triya_ui::icons::{EXPORTS, Variant};

fn web() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/triya-web")
}

fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!((info.color_type, info.bit_depth), (png::ColorType::Rgba, png::BitDepth::Eight));
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

fn encode(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::High);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(rgba).unwrap();
    }
    out
}

#[test]
fn icons_match_the_code_drawn_mark() {
    let write = std::env::var_os("TRIYA_WRITE_ICONS").is_some();
    for &(path, size, variant) in EXPORTS {
        let rgba = triya_brand::mark::rgba(size, variant).unwrap();
        let file = web().join(path);
        if write {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, encode(size, &rgba)).unwrap();
        }
        let bytes = std::fs::read(&file).unwrap_or_else(|e| panic!("{path}: {e} (TRIYA_WRITE_ICONS=1 writes it)"));
        let (w, h, pixels) = decode(&bytes);
        assert_eq!((w, h), (size, size), "{path}");
        assert!(pixels == rgba, "{path} differs from triya_brand::mark (TRIYA_WRITE_ICONS=1 rewrites it)");
    }
}

#[test]
fn manifest_lists_every_export_with_its_purpose() {
    let text = std::fs::read_to_string(web().join("manifest.webmanifest")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(manifest["name"], triya_brand::APP_NAME);
    assert_eq!(manifest["id"], triya_brand::APP_ID);
    let icons = manifest["icons"].as_array().unwrap();
    assert_eq!(icons.len(), EXPORTS.len());
    for &(path, size, variant) in EXPORTS {
        let icon = icons.iter().find(|i| i["src"] == path).unwrap_or_else(|| panic!("manifest misses {path}"));
        assert_eq!(icon["sizes"], format!("{size}x{size}"));
        assert_eq!(icon["type"], "image/png");
        let purpose = if variant == Variant::Maskable { "maskable" } else { "any" };
        assert_eq!(icon["purpose"], purpose, "{path}");
    }
    // Chrome's install criteria: a 192 px and a 512 px icon, standalone display, start_url.
    assert!(EXPORTS.iter().any(|e| e.1 == 192) && EXPORTS.iter().any(|e| e.1 == 512));
    assert_eq!(manifest["display"], "standalone");
    assert!(manifest["start_url"].is_string() && manifest["short_name"].is_string());
}

#[test]
fn page_links_the_manifest_icons_and_triya_binary() {
    let html = std::fs::read_to_string(web().join("index.html")).unwrap();
    assert!(html.contains("<title>Triya CAD 3D</title>"));
    assert!(html.contains(r#"data-bin="triya-web""#));
    assert!(html.contains(r#"<link rel="manifest" href="manifest.webmanifest">"#));
    assert!(html.contains(r#"href="icons/32x32/in.openengtools.triya.png""#));
    assert!(html.contains(r#"<link data-trunk rel="copy-dir" href="icons" />"#));
    // og:image must be a file the site actually serves.
    let og = html.split(r#"property="og:image" content=""#).nth(1).and_then(|s| s.split('"').next()).unwrap();
    let served = og.split("/app/").nth(1).unwrap_or_else(|| panic!("og:image {og} is not under /app/"));
    assert!(web().join(served).is_file(), "og:image {og} names no file in triya-web");
    assert!(!html.contains("SolidCraft_") && !html.to_lowercase().contains("solidcraft.png"));
}

#[test]
fn egui_images_come_from_the_same_drawing() {
    let img = triya_ui::icons::mark_image(48).unwrap();
    assert_eq!(img.size, [48, 48]);
    assert!(triya_ui::icons::window_icon(0).is_none());
    assert_eq!(triya_ui::icons::window_icon(64).unwrap().rgba.len(), 64 * 64 * 4);
}
