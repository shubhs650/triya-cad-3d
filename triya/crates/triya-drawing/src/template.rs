// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Title block templates (`docs/drawing-model.md` §5): a small SVG file with `{{placeholders}}`,
//! read as a subset (`<svg viewBox>` or `width`/`height`, `<line>`, `<rect>`, `<text x y
//! font-size text-anchor>`), so every plotter (SVG, DXF, PDF, the egui preview) draws the same
//! block without an SVG renderer. Units are sheet mm, y down from the block's top-left corner;
//! the block sits in the frame's bottom-right corner. The Open Engineering Tools default
//! ([`OET_DEFAULT`]) is original work.

use crate::model::{Layer, LineStyle, P2};
use crate::plot::{Align, Plotter};
use crate::{DrawingError, Result};

/// The default template (`templates/oet-title-block.svg`).
pub const OET_DEFAULT: &str = include_str!("../templates/oet-title-block.svg");

/// Largest template accepted (bytes) and most elements in one.
pub const MAX_TEMPLATE_BYTES: usize = 64 * 1024;
pub const MAX_ELEMENTS: usize = 2_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Line(P2, P2),
    Rect { at: P2, w: f64, h: f64 },
    Text { at: P2, size: f64, align: Align, text: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub width: f64,
    pub height: f64,
    pub items: Vec<Item>,
}

fn attrs(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].split_whitespace().last().unwrap_or("").to_string();
        let after = &rest[eq + 1..];
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let after = &after[1..];
        let Some(end) = after.find(q) else { break };
        out.push((key, after[..end].to_string()));
        rest = &after[end + 1..];
    }
    out
}

fn num(a: &[(String, String)], key: &str) -> Option<f64> {
    let v = a.iter().find(|(k, _)| k == key)?.1.trim().trim_end_matches("mm").trim();
    v.parse::<f64>().ok().filter(|x| x.is_finite() && x.abs() < 1e5)
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

impl Template {
    /// Read a template; an empty string is the OET default.
    pub fn parse_or_default(text: &str) -> Result<Template> {
        if text.trim().is_empty() { Template::parse(OET_DEFAULT) } else { Template::parse(text) }
    }

    pub fn parse(text: &str) -> Result<Template> {
        let bad = |m: &str| DrawingError::Invalid(format!("title block template: {m}"));
        if text.len() > MAX_TEMPLATE_BYTES {
            return Err(bad("larger than 64 KiB"));
        }
        let mut t = Template { width: 0.0, height: 0.0, items: Vec::new() };
        let mut rest = text;
        while let Some(lt) = rest.find('<') {
            rest = &rest[lt..];
            if let Some(r) = rest.strip_prefix("<!--") {
                rest = r.find("-->").map_or("", |e| &r[e + 3..]);
                continue;
            }
            let Some(gt) = rest.find('>') else { break };
            let tag = &rest[1..gt];
            rest = &rest[gt + 1..];
            let name = tag.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
            let a = attrs(tag);
            match name {
                "svg" => {
                    let vb: Vec<f64> = a
                        .iter()
                        .find(|(k, _)| k == "viewBox")
                        .map(|(_, v)| v.split(|c: char| c.is_whitespace() || c == ',').filter_map(|x| x.parse().ok()).collect())
                        .unwrap_or_default();
                    match vb.as_slice() {
                        [_, _, w, h] => (t.width, t.height) = (*w, *h),
                        _ => (t.width, t.height) = (num(&a, "width").unwrap_or(0.0), num(&a, "height").unwrap_or(0.0)),
                    }
                }
                "line" => {
                    let (Some(x1), Some(y1), Some(x2), Some(y2)) = (num(&a, "x1"), num(&a, "y1"), num(&a, "x2"), num(&a, "y2")) else {
                        return Err(bad("a <line> needs x1, y1, x2, y2"));
                    };
                    t.items.push(Item::Line(P2::new(x1, y1), P2::new(x2, y2)));
                }
                "rect" => {
                    let (Some(x), Some(y), Some(w), Some(h)) = (num(&a, "x"), num(&a, "y"), num(&a, "width"), num(&a, "height")) else {
                        return Err(bad("a <rect> needs x, y, width, height"));
                    };
                    t.items.push(Item::Rect { at: P2::new(x, y), w, h });
                }
                "text" => {
                    let end = rest.find("</text>").ok_or_else(|| bad("<text> is not closed"))?;
                    let content = unescape(rest[..end].trim());
                    rest = &rest[end + 7..];
                    let (Some(x), Some(y)) = (num(&a, "x"), num(&a, "y")) else { return Err(bad("a <text> needs x and y")) };
                    let align = match a.iter().find(|(k, _)| k == "text-anchor").map(|(_, v)| v.as_str()) {
                        Some("middle") => Align::Center,
                        Some("end") => Align::Right,
                        _ => Align::Left,
                    };
                    let size = num(&a, "font-size").unwrap_or(2.5).clamp(0.5, 50.0);
                    t.items.push(Item::Text { at: P2::new(x, y), size, align, text: content });
                }
                _ => {}
            }
            if t.items.len() > MAX_ELEMENTS {
                return Err(bad("more than 2000 elements"));
            }
        }
        if !(t.width > 0.0 && t.height > 0.0 && t.width < 2000.0 && t.height < 2000.0) {
            return Err(bad("the <svg> needs a viewBox (or width and height) in mm"));
        }
        Ok(t)
    }

    /// Fill `{{key}}` placeholders from `field` (an unknown one becomes empty).
    pub fn fill(text: &str, field: &dyn Fn(&str) -> Option<String>) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(i) = rest.find("{{") {
            out.push_str(&rest[..i]);
            let after = &rest[i + 2..];
            match after.find("}}") {
                Some(j) => {
                    out.push_str(&field(after[..j].trim()).unwrap_or_default());
                    rest = &after[j + 2..];
                }
                None => {
                    out.push_str(&rest[i..]);
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Draw the block with its top-left corner at `corner` (sheet mm, y up).
    pub fn plot(&self, plotter: &mut dyn Plotter, corner: P2, field: &dyn Fn(&str) -> Option<String>) -> Result<()> {
        let at = |p: P2| P2::new(corner.x + p.x, corner.y - p.y);
        plotter.set_line_style(LineStyle::Continuous);
        for item in &self.items {
            match item {
                Item::Line(a, b) => {
                    plotter.set_layer(Layer::Frame);
                    plotter.set_width(Layer::Frame.width());
                    plotter.line(at(*a), at(*b));
                }
                Item::Rect { at: p, w, h } => {
                    plotter.set_layer(Layer::Frame);
                    plotter.set_width(0.5);
                    let (a, b) = (at(*p), at(P2::new(p.x + w, p.y + h)));
                    plotter.polyline(&[a, P2::new(b.x, a.y), b, P2::new(a.x, b.y)], true);
                }
                Item::Text { at: p, size, align, text } => {
                    let s = Template::fill(text, field);
                    if !s.is_empty() {
                        plotter.set_layer(Layer::Frame);
                        plotter.set_width(Layer::Text.width());
                        plotter.text(at(*p), *size, 0.0, *align, &s);
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_template_reads_and_has_every_placeholder() {
        let t = Template::parse(OET_DEFAULT).unwrap();
        assert_eq!((t.width, t.height), (180.0, 56.0));
        let texts: Vec<&str> = t.items.iter().filter_map(|i| if let Item::Text { text, .. } = i { Some(text.as_str()) } else { None }).collect();
        for key in ["org", "project", "title", "drawing_no", "rev", "size", "drawn_by", "checked_by", "date", "scale", "sheet"] {
            assert!(texts.contains(&format!("{{{{{key}}}}}").as_str()), "{key}");
        }
        assert!(t.items.iter().any(|i| matches!(i, Item::Rect { .. })));
    }

    #[test]
    fn placeholders_fill_and_bad_templates_are_refused() {
        let f = |k: &str| (k == "title").then(|| "DRUM".to_string());
        assert_eq!(Template::fill("A {{title}} B {{nope}} {{open", &f), "A DRUM B  {{open");
        assert!(Template::parse("<svg viewBox=\"0 0 10 10\"><line x1=\"0\"/></svg>").is_err());
        assert!(Template::parse("<svg><text x=\"1\" y=\"1\">no size</text></svg>").is_err());
        assert!(Template::parse(&"<line x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"/>".repeat(3000)).is_err());
        let t = Template::parse("<svg width=\"100mm\" height=\"30mm\"><text x=\"1\" y=\"2\" text-anchor=\"middle\">a &amp; b</text></svg>").unwrap();
        assert_eq!(t.items, vec![Item::Text { at: P2::new(1.0, 2.0), size: 2.5, align: Align::Center, text: "a & b".into() }]);
    }
}
