// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The `drawing.*` commands (T6.7), registered next to `triya_vessel::COMMANDS` through the
//! engine's extension slot. Both sit on the `VESSEL` tab's INSPECT panel.
//!
//! The GA sheet is built from the vessel document (ADR-0004), so nothing on the sheet can go
//! stale: the design keeps only the sheet's *options* under [`SHEET_KEY`] (a whole A1 sheet is
//! far larger than upstream's 64 KiB per metadata value), and every export rebuilds the sheet
//! from the current vessel. Weights come from `vessel.bom` (measured bodies after a regenerate,
//! handbook values before), run as a nested command the way upstream's `selection.delete` runs
//! others.

use serde_json::{Value, json};
use solidcraft_engine::params::{bad, str_};
use solidcraft_engine::{CommandSpec, EngineError, Session};
use triya_vessel::commands::{METADATA_KEY, TAB, stored_doc};
use triya_vessel::presets;

use crate::ga::{self, GaOptions};
use crate::model::{Sheet, SheetSize, TableKind, TitleBlock, ViewKind};
use crate::vessel_views::VesselModel;
use crate::{DrawingError, to_dxf, to_pdf, to_svg};

/// The reverse-DNS metadata key the GA sheet's options are kept under.
pub const SHEET_KEY: &str = "in.openengtools.triya.sheet";
/// The largest title block template a design keeps (upstream allows 64 KiB per metadata value).
pub const MAX_TEMPLATE: usize = 48 * 1024;

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("drawing.generate_ga", "Generate GA Drawing", generate_ga).at(TAB, "INSPECT").icon("dimension").enabled(has_vessel).params(
        "size?: A0|A1|A2|A3|A4 (default A1 landscape); views?: [elevation|plan|section] (default all three); tables?: \
         [design_data|nozzle_schedule|bom|notes|revisions] (default all); scale?: 25 | \"1:25\" | null (default: the largest standard \
         scale that fits); template?: a title block SVG (placeholders {{title}}, {{drawing_no}}, …; \"\" = the OET default); \
         title_block?: {title, drawing_number, project, organization, drawn_by, checked_by, date, sheet_of, rev, extra: {key: text}} \
         (laid over the values read from the vessel); sheet?: bool (also return the whole sheet). Unset keys keep the design's \
         stored options. Stores the options in the design (the sheet is rebuilt from the vessel on every export) → size, scale, \
         views, tables, options, warnings, sheet?",
    ),
    CommandSpec::new("drawing.export", "Export Drawing", export).at(TAB, "INSPECT").icon("svg").noundo().enabled(has_vessel).params(
        "format?: svg|dxf|pdf|json (default: from the path's extension, else svg; json = the sheet model); path?: where to write it \
         (desktop path; in the browser `/downloads/<name>` becomes a download). Uses the options of drawing.generate_ga (defaults when \
         it never ran) → path, filename, bytes | text (svg, dxf), base64 (pdf), sheet (json), filename; warnings",
    ),
];

fn has_vessel(s: &Session) -> std::result::Result<(), String> {
    if s.doc.metadata.get(METADATA_KEY).is_some() { Ok(()) } else { Err("the design has no vessel (vessel.new first)".into()) }
}

impl From<DrawingError> for EngineError {
    fn from(e: DrawingError) -> Self {
        match e {
            DrawingError::NotSupportedYet(m) => EngineError::Other(format!("not supported yet: {m}")),
            other => EngineError::Other(other.to_string()),
        }
    }
}

/// The options the design keeps (defaults when there are none or someone broke them).
pub fn stored_options(s: &Session) -> GaOptions {
    s.doc.metadata.get(SHEET_KEY).and_then(|v| v.get("options")).and_then(|o| serde_json::from_value(o.clone()).ok()).unwrap_or_default()
}

fn view_kind(v: &str) -> Option<ViewKind> {
    Some(match v {
        "elevation" => ViewKind::Elevation,
        "plan" => ViewKind::Plan,
        "section" => ViewKind::Section,
        _ => return None,
    })
}

fn table_kind(v: &str) -> Option<TableKind> {
    Some(match v {
        "design_data" => TableKind::DesignData,
        "nozzle_schedule" => TableKind::NozzleSchedule,
        "bom" => TableKind::Bom,
        "notes" => TableKind::Notes,
        "revisions" => TableKind::Revisions,
        _ => return None,
    })
}

/// A list of distinct kinds named by strings (at most 16 entries).
fn kinds<T: PartialEq>(cmd: &str, key: &str, v: &Value, parse: fn(&str) -> Option<T>) -> solidcraft_engine::Result<Vec<T>> {
    let list = v.as_array().ok_or_else(|| bad(cmd, format!("`{key}` is a list of names")))?;
    if list.len() > 16 {
        return Err(bad(cmd, format!("`{key}` has too many entries")));
    }
    let mut out = Vec::new();
    for x in list {
        let k = x.as_str().and_then(parse).ok_or_else(|| bad(cmd, format!("`{key}`: unknown entry {x}")))?;
        if !out.contains(&k) {
            out.push(k);
        }
    }
    Ok(out)
}

/// The scale's denominator: `25`, `"1:25"` or `"25"`.
fn scale_denominator(v: &Value) -> Option<f64> {
    let d = match v {
        Value::Number(n) => n.as_f64()?,
        Value::String(t) => {
            let t = t.trim();
            let t = t.strip_prefix("1:").unwrap_or(t);
            t.trim().parse::<f64>().ok()?
        }
        _ => return None,
    };
    (d.is_finite() && (1.0..=1000.0).contains(&d)).then_some(d)
}

/// `p` laid over `base`.
pub fn options_from(cmd: &str, base: GaOptions, p: &Value) -> solidcraft_engine::Result<GaOptions> {
    let mut o = base;
    if p.is_null() {
        return Ok(o);
    }
    if !p.is_object() {
        return Err(bad(cmd, "parameters must be an object"));
    }
    if let Some(v) = p.get("size").filter(|v| !v.is_null()) {
        o.size = v.as_str().and_then(SheetSize::parse).ok_or_else(|| bad(cmd, "`size` is A0, A1, A2, A3 or A4"))?;
    }
    if let Some(v) = p.get("views").filter(|v| !v.is_null()) {
        o.views = kinds(cmd, "views", v, view_kind)?;
        if o.views.is_empty() {
            return Err(bad(cmd, "`views` names at least one of elevation, plan, section"));
        }
    }
    if let Some(v) = p.get("tables").filter(|v| !v.is_null()) {
        o.tables = kinds(cmd, "tables", v, table_kind)?;
    }
    if let Some(v) = p.get("scale") {
        o.scale = match v {
            Value::Null => None,
            v => Some(1.0 / scale_denominator(v).ok_or_else(|| bad(cmd, "`scale` is a denominator from 1 to 1000 (25 or \"1:25\")"))?),
        };
    }
    if let Some(v) = p.get("template").filter(|v| !v.is_null()) {
        let t = v.as_str().ok_or_else(|| bad(cmd, "`template` is SVG text"))?;
        if t.len() > MAX_TEMPLATE {
            return Err(bad(cmd, format!("`template` is larger than {} KiB", MAX_TEMPLATE / 1024)));
        }
        if !t.trim().is_empty() {
            crate::template::Template::parse(t).map_err(|e| bad(cmd, format!("`template`: {e}")))?;
        }
        o.template = t.to_string();
    }
    if let Some(v) = p.get("title_block") {
        o.title_block = match v {
            Value::Null => None,
            v => Some(serde_json::from_value::<TitleBlock>(v.clone()).map_err(|e| bad(cmd, format!("`title_block`: {e}")))?),
        };
    }
    Ok(o)
}

/// The GA sheet of the design's vessel with `opts`, and what to warn about.
pub fn sheet(s: &mut Session, opts: &GaOptions) -> solidcraft_engine::Result<(Sheet, Vec<String>)> {
    let doc = stored_doc(s)?;
    let tables = presets::tables(s)?;
    let m = VesselModel::new(&doc, tables, presets::private_support_tables(s))?;
    let mut warnings = m.warnings.clone();
    let bom = if opts.tables.contains(&TableKind::Bom) || opts.tables.contains(&TableKind::DesignData) {
        match bom(s) {
            Ok(b) => {
                warnings.extend(b.get("warnings").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string));
                Some(b)
            }
            Err(e) => {
                warnings.push(format!("no BOM on the sheet: {e}"));
                None
            }
        }
    } else {
        None
    };
    let sheet = ga::general_arrangement(&m, bom.as_ref(), opts)?;
    Ok((sheet, warnings))
}

/// `vessel.bom`, run inside this command.
fn bom(s: &mut Session) -> solidcraft_engine::Result<Value> {
    let spec = solidcraft_engine::find_command("vessel.bom").ok_or_else(|| EngineError::Other("vessel.bom is not registered".into()))?;
    (spec.enabled)(s).map_err(|e| EngineError::Other(format!("vessel.bom: {e}")))?;
    (spec.run)(s, &json!({}))
}

fn summary(sheet: &Sheet, opts: &GaOptions, warnings: &[String]) -> Value {
    json!({
        "size": sheet.size.name(),
        "scale": sheet.title_block.scale,
        "views": sheet.views.iter().map(|v| json!({"name": v.name, "title": v.title, "scale": crate::plot::scale_text(v.scale)})).collect::<Vec<_>>(),
        "tables": sheet.tables.iter().map(|t| json!({"title": t.title, "rows": t.rows.len()})).collect::<Vec<_>>(),
        "options": serde_json::to_value(opts).unwrap_or(Value::Null),
        "warnings": warnings,
    })
}

fn generate_ga(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "drawing.generate_ga";
    let opts = options_from(cmd, stored_options(s), p)?;
    let (sheet, warnings) = sheet(s, &opts)?;
    let value = serde_json::to_value(&opts).map_err(|e| EngineError::Other(e.to_string()))?;
    let mut next = s.doc.metadata.clone();
    next.set(SHEET_KEY, json!({"options": value})).map_err(|e| EngineError::Other(format!("cannot store the drawing options: {e}")))?;
    if next != s.doc.metadata {
        s.doc_mut().metadata = next;
    }
    let mut out = summary(&sheet, &opts, &warnings);
    if p.get("sheet").and_then(Value::as_bool) == Some(true) {
        out["sheet"] = serde_json::to_value(&sheet).map_err(|e| EngineError::Other(e.to_string()))?;
    }
    Ok(out)
}

/// A file name from the drawing number.
fn file_stem(sheet: &Sheet) -> String {
    let raw = if sheet.title_block.drawing_number.trim().is_empty() { "drawing" } else { sheet.title_block.drawing_number.trim() };
    raw.chars().take(80).map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' }).collect()
}

fn export(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "drawing.export";
    if !(p.is_null() || p.is_object()) {
        return Err(bad(cmd, "parameters must be an object"));
    }
    let path = str_(p, "path").filter(|t| !t.trim().is_empty());
    let by_ext = path.and_then(|t| t.rsplit_once('.')).map(|(_, e)| e.to_ascii_lowercase());
    let format = match str_(p, "format") {
        Some(f) => f.to_ascii_lowercase(),
        None => by_ext.filter(|e| ["svg", "dxf", "pdf", "json"].contains(&e.as_str())).unwrap_or_else(|| "svg".into()),
    };
    if !["svg", "dxf", "pdf", "json"].contains(&format.as_str()) {
        return Err(bad(cmd, "`format` is svg, dxf, pdf or json"));
    }
    let opts = stored_options(s);
    let (sheet, warnings) = sheet(s, &opts)?;
    let filename = format!("{}.{format}", file_stem(&sheet));
    let bytes: Vec<u8> = match format.as_str() {
        "svg" => to_svg(&sheet)?.into_bytes(),
        "dxf" => to_dxf(&sheet)?.into_bytes(),
        "pdf" => to_pdf(&sheet)?,
        _ => serde_json::to_vec_pretty(&sheet).map_err(|e| EngineError::Other(e.to_string()))?,
    };
    if let Some(path) = path {
        solidcraft_engine::io::vfs::write(path, &bytes).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
        return Ok(json!({"path": path, "filename": filename, "bytes": bytes.len(), "format": format, "warnings": warnings}));
    }
    let mut out = json!({"filename": filename, "format": format, "bytes": bytes.len(), "warnings": warnings});
    match format.as_str() {
        "pdf" => out["base64"] = json!(solidcraft_engine::doc::canvas::base64_encode(&bytes)),
        "json" => out["sheet"] = serde_json::to_value(&sheet).map_err(|e| EngineError::Other(e.to_string()))?,
        _ => out["text"] = json!(String::from_utf8_lossy(&bytes)),
    }
    Ok(out)
}
