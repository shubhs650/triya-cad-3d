// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The PDF plotter (T6.6): a small vector PDF 1.4 writer of our own instead of `printpdf`.
//! A GA sheet needs only stroked lines, Bézier arcs, filled arrowheads and single-line texts, so
//! the writer emits one page the size of the sheet, draws in millimetres (one `cm` scale), and
//! sets texts in the standard Helvetica face with WinAnsi encoding: nothing is embedded, so the
//! same code runs natively and in wasm with no font files. Characters outside WinAnsi print `?`.

use std::f64::consts::{FRAC_PI_2, TAU};

use crate::model::{Layer, LineStyle, P2, Sheet};
use crate::plot::{Align, Plotter, text_width};

/// Points per millimetre.
pub const PT_PER_MM: f64 = 72.0 / 25.4;
/// Largest content stream (a hostile sheet cannot make the file unbounded).
pub const MAX_CONTENT: usize = 256 * 1024 * 1024;

pub struct PdfPlotter {
    width: f64,
    height: f64,
    title: String,
    content: String,
    layer: Layer,
    style: LineStyle,
    stroke_w: f64,
    /// The graphics state last written (layer colour, dash, width), to skip repeats.
    state: Option<(Layer, LineStyle, u64)>,
}

fn n(x: f64) -> String {
    if !x.is_finite() {
        return "0".into();
    }
    let s = format!("{x:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn colour(layer: Layer) -> &'static str {
    match layer {
        Layer::Visible | Layer::Frame | Layer::Text => "0 0 0",
        Layer::Hidden => "0.251 0.251 0.251",
        Layer::Center => "0.627 0 0",
        Layer::Dimension => "0 0.22 0.659",
        Layer::Hatch => "0.251 0.376 0.251",
    }
}

fn dash(style: LineStyle) -> &'static str {
    match style {
        LineStyle::Continuous => "[] 0 d",
        LineStyle::Dashed => "[3 1.5] 0 d",
        LineStyle::Center => "[12 2 2 2] 0 d",
        LineStyle::Hidden => "[2 1] 0 d",
        LineStyle::Phantom => "[12 2 2 2 2 2] 0 d",
    }
}

/// `text` as a PDF literal string in WinAnsi encoding.
pub fn win_ansi(text: &str) -> String {
    let mut out = String::from("(");
    for c in text.chars() {
        let byte: u8 = match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
                continue;
            }
            ' '..='~' => {
                out.push(c);
                continue;
            }
            '°' => 0xB0,
            '·' => 0xB7,
            '×' => 0xD7,
            '±' => 0xB1,
            'Ø' => 0xD8,
            'ø' => 0xF8,
            '²' => 0xB2,
            '³' => 0xB3,
            '–' => 0x96,
            '—' => 0x97,
            '→' | '↑' | '↓' => b'>',
            c if c.is_control() => continue,
            _ => b'?',
        };
        if byte.is_ascii() {
            out.push(byte as char);
        } else {
            out.push_str(&format!("\\{byte:03o}"));
        }
    }
    out.push(')');
    out
}

impl PdfPlotter {
    pub fn new(width_mm: f64, height_mm: f64, title: &str) -> PdfPlotter {
        PdfPlotter {
            width: if width_mm.is_finite() { width_mm.clamp(1.0, 5000.0) } else { 1.0 },
            height: if height_mm.is_finite() { height_mm.clamp(1.0, 5000.0) } else { 1.0 },
            title: title.chars().take(200).collect(),
            content: String::new(),
            layer: Layer::Visible,
            style: LineStyle::Continuous,
            stroke_w: 0.35,
            state: None,
        }
    }

    pub fn for_sheet(sheet: &Sheet) -> PdfPlotter {
        let (w, h) = sheet.size.mm();
        PdfPlotter::new(w, h, &sheet.title_block.title)
    }

    fn op(&mut self, s: &str) {
        if self.content.len() + s.len() < MAX_CONTENT {
            self.content.push_str(s);
            self.content.push('\n');
        }
    }

    fn graphics(&mut self) {
        let key = (self.layer, self.style, (self.stroke_w * 1000.0).round() as u64);
        if self.state != Some(key) {
            let c = colour(self.layer);
            let s = format!("{c} RG {c} rg {} {} w", dash(self.style), n(self.stroke_w));
            self.op(&s);
            self.state = Some(key);
        }
    }

    /// Bézier segments of the arc of an affine image of the unit circle (`map`) from `a0` by
    /// `sweep` (radians, either sign).
    fn bezier_arc(&mut self, map: &dyn Fn(f64, f64) -> P2, a0: f64, sweep: f64, move_to: bool) {
        let pieces = ((sweep.abs() / FRAC_PI_2).ceil() as usize).clamp(1, 16);
        let step = sweep / pieces as f64;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let mut s = String::new();
        let p0 = map(a0.cos(), a0.sin());
        if move_to {
            s.push_str(&format!("{} {} m ", n(p0.x), n(p0.y)));
        }
        for i in 0..pieces {
            let t0 = a0 + step * i as f64;
            let t1 = t0 + step;
            let (c0, s0, c1, s1) = (t0.cos(), t0.sin(), t1.cos(), t1.sin());
            let q1 = map(c0 - k * s0, s0 + k * c0);
            let q2 = map(c1 + k * s1, s1 - k * c1);
            let q3 = map(c1, s1);
            s.push_str(&format!("{} {} {} {} {} {} c ", n(q1.x), n(q1.y), n(q2.x), n(q2.y), n(q3.x), n(q3.y)));
        }
        self.op(s.trim_end());
    }

    /// The PDF file.
    pub fn finish(self) -> Vec<u8> {
        let (wpt, hpt) = (self.width * PT_PER_MM, self.height * PT_PER_MM);
        let stream = format!("q {k} 0 0 {k} 0 0 cm 1 J 1 j\n{}Q\n", self.content, k = n(PT_PER_MM));
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>", n(wpt), n(hpt)),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}endstream", stream.len()),
            format!("<< /Title {} /Producer {} >>", win_ansi(&self.title), win_ansi(triya_brand::APP_NAME)),
        ];
        let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::with_capacity(objects.len());
        for (i, o) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
        for o in offsets {
            tail.push_str(&format!("{o:010} 00000 n \n"));
        }
        tail.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R /Info {} 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1, objects.len()));
        out.extend_from_slice(tail.as_bytes());
        out
    }
}

impl Plotter for PdfPlotter {
    fn set_layer(&mut self, layer: Layer) {
        self.layer = layer;
    }
    fn set_line_style(&mut self, style: LineStyle) {
        self.style = style;
    }
    fn set_width(&mut self, width: f64) {
        if width.is_finite() && width > 0.0 {
            self.stroke_w = width.min(10.0);
        }
    }
    fn line(&mut self, a: P2, b: P2) {
        self.graphics();
        let s = format!("{} {} m {} {} l S", n(a.x), n(a.y), n(b.x), n(b.y));
        self.op(&s);
    }
    fn polyline(&mut self, pts: &[P2], closed: bool) {
        if pts.len() < 2 {
            return;
        }
        self.graphics();
        let mut s = String::new();
        for (i, p) in pts.iter().enumerate() {
            s.push_str(&format!("{} {} {} ", n(p.x), n(p.y), if i == 0 { "m" } else { "l" }));
        }
        s.push_str(if closed { "s" } else { "S" });
        self.op(&s);
    }
    fn arc(&mut self, center: P2, radius: f64, start: f64, end: f64) {
        if !(radius > 0.0 && radius.is_finite() && start.is_finite() && end.is_finite()) {
            return;
        }
        let sweep = (end - start).rem_euclid(TAU);
        if sweep < 1e-12 {
            return self.circle(center, radius);
        }
        self.graphics();
        self.bezier_arc(&|c, s| P2::new(center.x + radius * c, center.y + radius * s), start, sweep, true);
        self.op("S");
    }
    fn circle(&mut self, center: P2, radius: f64) {
        if !(radius > 0.0 && radius.is_finite()) {
            return;
        }
        self.graphics();
        self.bezier_arc(&|c, s| P2::new(center.x + radius * c, center.y + radius * s), 0.0, TAU, true);
        self.op("s");
    }
    fn ellipse(&mut self, center: P2, rx: f64, ry: f64, rotation: f64, start: f64, end: f64) {
        if !(rx > 0.0 && ry > 0.0 && rx.is_finite() && ry.is_finite() && rotation.is_finite()) {
            return;
        }
        let full = (end - start).abs() >= TAU - 1e-9 || (end - start).rem_euclid(TAU) < 1e-12;
        let sweep = if full { TAU } else { (end - start).rem_euclid(TAU) };
        let (cr, sr) = (rotation.cos(), rotation.sin());
        self.graphics();
        self.bezier_arc(
            &|c, s| P2::new(center.x + rx * c * cr - ry * s * sr, center.y + rx * c * sr + ry * s * cr),
            if full { 0.0 } else { start },
            sweep,
            true,
        );
        self.op(if full { "s" } else { "S" });
    }
    fn fill(&mut self, pts: &[P2]) {
        if pts.len() < 3 {
            return;
        }
        self.graphics();
        let mut s = String::new();
        for (i, p) in pts.iter().enumerate() {
            s.push_str(&format!("{} {} {} ", n(p.x), n(p.y), if i == 0 { "m" } else { "l" }));
        }
        s.push_str("h f");
        self.op(&s);
    }
    fn text(&mut self, at: P2, height: f64, angle: f64, align: Align, text: &str) {
        if !(height > 0.0 && height.is_finite() && angle.is_finite()) || text.is_empty() {
            return;
        }
        let w = text_width(text, height);
        let shift = match align {
            Align::Left => 0.0,
            Align::Center => -w / 2.0,
            Align::Right => -w,
        };
        let (c, s) = (angle.cos(), angle.sin());
        let o = P2::new(at.x + shift * c, at.y + shift * s);
        self.graphics();
        let t = format!("BT /F1 {} Tf {} {} {} {} {} {} Tm {} Tj ET", n(height), n(c), n(s), n(-s), n(c), n(o.x), n(o.y), win_ansi(text));
        self.op(&t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_page_of_the_sheet_size_with_vector_content() {
        let mut p = PdfPlotter::new(841.0, 594.0, "Drum (A1)");
        p.line(P2::new(0.0, 0.0), P2::new(10.0, 0.0));
        p.arc(P2::new(0.0, 0.0), 5.0, 0.0, 1.0);
        p.ellipse(P2::new(0.0, 0.0), 2.0, 4.0, 0.3, 0.0, TAU);
        p.fill(&[P2::new(0.0, 0.0), P2::new(1.0, 0.0), P2::new(0.0, 1.0)]);
        p.text(P2::new(0.0, 0.0), 3.5, 0.0, Align::Center, "90° (N1)");
        let bytes = p.finish();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.starts_with("%PDF-1.4"));
        assert_eq!(s.matches("/Type /Page ").count(), 1);
        assert!(s.contains("/MediaBox [0 0 2383.937 1683.78]"), "{s}");
        assert!(s.contains("/BaseFont /Helvetica"));
        assert!(s.contains("(90\\260 \\(N1\\)) Tj"));
        assert!(s.contains(" c ") || s.contains(" c\n"));
        assert!(s.trim_end().ends_with("%%EOF"));
        // The xref offsets point at the objects (byte offsets: the header has binary bytes).
        let find = |needle: &[u8]| bytes.windows(needle.len()).rposition(|w| w == needle);
        let xref = find(b"\nxref\n").map(|i| i + 1).unwrap_or(0);
        let start: usize = s.rsplit("startxref\n").next().and_then(|t| t.lines().next()).and_then(|t| t.parse().ok()).unwrap_or(0);
        assert_eq!(xref, start);
        let table = String::from_utf8_lossy(&bytes[xref..]).to_string();
        for (i, line) in table.lines().skip(3).take(6).enumerate() {
            let off: usize = line[..10].parse().unwrap_or(0);
            assert!(bytes[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "{line}");
        }
    }

    #[test]
    fn texts_escape_and_map_to_win_ansi() {
        assert_eq!(win_ansi("a(b)\\"), "(a\\(b\\)\\\\)");
        assert_eq!(win_ansi("45° ×2 ✕"), "(45\\260 \\3272 ?)");
    }
}
