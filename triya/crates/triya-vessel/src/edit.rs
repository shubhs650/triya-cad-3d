// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The ribbon's editing commands (PLAN T5.1): add an element, an attachment or a support with
//! sensible defaults, and export the BOM. Each one changes only the vessel document (one undo
//! step, like `vessel.set`) and replies with the document and what it added, so the toolbar
//! (`{}`), the panels, scripts and MCP all make the same JSON.
//!
//! Defaults come from the stack around the new item: an element takes its neighbour's wall
//! thickness and inherits the diameter; a support takes its size from the preset tables
//! (`triya-stdlib/data/support-presets.json`, or the design's private table) by the host's
//! outside diameter and keeps that `size_key`. Fields given in `element` / `attachment` are laid
//! over the defaults (each field replaced whole).

use serde_json::{Map, Value, json};
use solidcraft_engine::params::{bad, num, str_};
use solidcraft_engine::{EngineError, Session};

use crate::commands::{load, store};
use crate::eval::{Resolved, Station};
use crate::model::{Attachment, Element, ElementKind, Orientation, VesselDoc};
use crate::{eval, presets};

/// At most this many fields in a given `element` / `attachment` object.
const MAX_FIELDS: usize = 64;

/// The first `<prefix><n>` (n ≥ 1) no element or attachment uses.
pub fn free_id(doc: &VesselDoc, prefix: &str) -> String {
    let used = |id: &str| doc.elements.iter().any(|e| e.id == id) || doc.attachments.iter().any(|a| a.id == id);
    (1..=10_000).map(|n| format!("{prefix}{n}")).find(|id| !used(id)).unwrap_or_else(|| format!("{prefix}x"))
}

/// The id prefix of an attachment kind (`N1`, `SD1`, `LG1`, …).
pub fn prefix(kind: &str) -> &'static str {
    match kind {
        "nozzle" => "N",
        "saddle" => "SD",
        "leg" => "LG",
        "bracket" => "B",
        "anchor_chair" => "AC",
        "stiffening_ring" => "R",
        "limpet_coil" => "C",
        "lifting_lug" => "L",
        "insulation_ring" => "I",
        "clip" => "CL",
        _ => "A",
    }
}

/// The element kinds, as the JSON writes them.
pub const ELEMENT_KINDS: [&str; 6] = ["cylinder", "head", "cone", "skirt", "flat_cover", "body_flange"];
/// The attachment kinds, as the JSON writes them.
pub const ATTACHMENT_KINDS: [&str; 10] =
    ["nozzle", "saddle", "leg", "bracket", "anchor_chair", "stiffening_ring", "limpet_coil", "lifting_lug", "insulation_ring", "clip"];

/// `given` (an object) laid over `base`, field by field.
fn overlay(cmd: &str, base: &mut Map<String, Value>, given: Option<&Value>, what: &str) -> solidcraft_engine::Result<()> {
    let Some(g) = given else { return Ok(()) };
    let o = g.as_object().ok_or_else(|| bad(cmd, format!("`{what}` must be an object of fields")))?;
    if o.len() > MAX_FIELDS {
        return Err(bad(cmd, format!("`{what}` has more than {MAX_FIELDS} fields")));
    }
    for (k, v) in o {
        base.insert(k.clone(), v.clone());
    }
    Ok(())
}

fn obj(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// Outside diameter of a station (its inside radius at the start plus the wall).
pub fn outside_diameter(st: &Station) -> f64 {
    2.0 * (st.r_from.max(st.r_to) + st.thickness)
}

/// Where a new element goes when no position is given: a head or flat cover closes the open end
/// (the top, else the bottom once the top is closed); anything else goes before a closing head,
/// else at the end.
fn default_index(doc: &VesselDoc, kind: &str) -> usize {
    let n = doc.elements.len();
    let closes = |e: &Element| matches!(e.kind, ElementKind::Head { .. } | ElementKind::FlatCover { .. });
    let top_closed = n >= 2 && doc.elements.last().is_some_and(closes);
    let bottom_closed = doc.elements.first().is_some_and(|e| closes(e) || matches!(e.kind, ElementKind::Skirt { .. }));
    match kind {
        "head" | "flat_cover" if top_closed && !bottom_closed => 0,
        "head" | "flat_cover" => n,
        _ if top_closed => n - 1,
        _ => n,
    }
}

/// The insert position from `index`, `before` or `after`.
fn insert_index(cmd: &str, doc: &VesselDoc, kind: &str, p: &Value) -> solidcraft_engine::Result<usize> {
    let find = |id: &str| doc.elements.iter().position(|e| e.id == id).ok_or_else(|| bad(cmd, format!("no element `{id}`")));
    match (num(p, "index"), str_(p, "before"), str_(p, "after")) {
        (Some(i), None, None) if i.is_finite() && i >= 0.0 && i <= doc.elements.len() as f64 => Ok(i as usize),
        (Some(_), None, None) => Err(bad(cmd, format!("`index` must be 0 … {}", doc.elements.len()))),
        (None, Some(id), None) => find(id),
        (None, None, Some(id)) => Ok(find(id)? + 1),
        (None, None, None) => Ok(default_index(doc, kind)),
        _ => Err(bad(cmd, "give at most one of `index`, `before`, `after`")),
    }
}

/// Default fields of a new element of `kind` next to station `near` (the previous element, or
/// the next one when inserting first).
pub fn element_defaults(cmd: &str, kind: &str, near: Option<&Station>) -> solidcraft_engine::Result<Map<String, Value>> {
    let t = near.map_or(10.0, |s| s.thickness);
    let d = near.map_or(1000.0, |s| s.diameter_mm);
    let od = near.map_or(d + 2.0 * t, outside_diameter);
    let fields = match kind {
        "cylinder" => json!({"length_mm": 1000.0}),
        "head" => json!({"preset": "ellipsoidal_2_1"}),
        "cone" => json!({"length_mm": (d / 2.0).round(), "diameter_large_mm": d, "diameter_small_mm": (d / 2.0).round()}),
        "skirt" => json!({"height_mm": 1500.0, "base_ring": {"od_mm": (od + 200.0).round(), "id_mm": (od - 200.0).round(), "thickness_mm": 20.0}}),
        "flat_cover" => json!({"bolted": false}),
        "body_flange" => json!({}),
        other => {
            return Err(bad(
                cmd,
                format!("element kind must be one of {} (got `{}`)", ELEMENT_KINDS.join(", "), other.chars().take(40).collect::<String>()),
            ));
        }
    };
    let mut m = obj(fields);
    m.insert("kind".into(), json!(kind));
    m.insert("thickness_mm".into(), json!(t));
    Ok(m)
}

fn element_add(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "vessel.element.add";
    let (mut doc, generated) = load(s)?;
    let given = p.get("element");
    let kind = given.and_then(|g| g.get("kind")).and_then(Value::as_str).or_else(|| str_(p, "kind")).unwrap_or("cylinder").to_string();
    let index = insert_index(cmd, &doc, &kind, p)?;
    let r = eval::resolve(&doc).ok();
    let near_id = doc.elements.get(index.wrapping_sub(1)).or_else(|| doc.elements.get(index)).map(|e| e.id.clone());
    let near = r.as_ref().zip(near_id.as_deref()).and_then(|(r, id)| r.station(id));
    let mut fields = element_defaults(cmd, &kind, near)?;
    // The bottom element has nothing below to inherit its diameter from: it takes its neighbour's.
    if index == 0
        && let Some(st) = near
    {
        fields.insert("diameter_mm".into(), json!(st.diameter_mm));
    }
    overlay(cmd, &mut fields, given, "element")?;
    if kind == "body_flange" && !fields.contains_key("flange_ref") && !fields.contains_key("explicit") {
        return Err(bad(cmd, "a body flange needs `element.flange_ref` (a table row) or `element.explicit` dimensions"));
    }
    let id = match fields.get("id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => free_id(&doc, "E"),
    };
    fields.insert("id".into(), json!(id));
    let e: Element = serde_json::from_value(Value::Object(fields)).map_err(|e| bad(cmd, format!("`element`: {e}")))?;
    if doc.element(&e.id).is_some() || doc.attachments.iter().any(|a| a.id == e.id) {
        return Err(bad(cmd, format!("`{}` is already used", e.id)));
    }
    doc.elements.insert(index.min(doc.elements.len()), e);
    store(s, &doc, &generated)?;
    Ok(json!({"doc": doc.to_value()?, "id": id, "index": index}))
}

/// The host a new attachment of `kind` goes on when none is given.
fn default_host<'a>(doc: &'a VesselDoc, kind: &str) -> Option<&'a Element> {
    let cylinders = || doc.elements.iter().filter(|e| matches!(e.kind, ElementKind::Cylinder { .. }));
    match kind {
        "anchor_chair" => doc.elements.iter().find(|e| matches!(e.kind, ElementKind::Skirt { .. })),
        // Lugs near the top of the stack, everything else low on the shell.
        "lifting_lug" => cylinders().next_back(),
        _ => cylinders().next(),
    }
    .or_else(|| doc.elements.first())
}

/// A support's preset fields by the host's outside diameter, and the `size_key` they came from.
fn preset_fields(s: &Session, kind: &str, st: &Station) -> Option<(String, Map<String, Value>)> {
    let tables = presets::tables(s).ok()?;
    let key = format!("OD{}", outside_diameter(st).round());
    let row = tables.support(kind, &key)?;
    Some((key, row.fields.clone()))
}

/// Default fields of a new attachment of `kind` on `host` (its station `st`).
fn attachment_defaults(
    cmd: &str,
    s: &Session,
    doc: &VesselDoc,
    kind: &str,
    host: &Element,
    st: &Station,
) -> solidcraft_engine::Result<Map<String, Value>> {
    let on_head = matches!(host.kind, ElementKind::Head { .. } | ElementKind::FlatCover { .. });
    let length = st.z_to - st.z_from;
    let horizontal = doc.global.orientation == Orientation::Horizontal;
    let mid = json!({"ref": "host_from", "mm": (length / 2.0).round()});
    let fields = match kind {
        "nozzle" => {
            let mut m = obj(json!({
                "nps": "4", "schedule": "40",
                "flange": {"standard": "B16.5", "class": 150, "type": "wn", "facing": "rf"},
                "projection": {"from": "od", "mm": 200.0},
            }));
            if on_head {
                m.insert("axial".into(), json!({"ref": "host_from", "mm": 0.0}));
                m.insert("frame".into(), json!({"type": "parallel_to_axis", "radius_mm": 0.0}));
                m.insert("projection".into(), json!({"from": "cl", "mm": (st.r_from.max(st.r_to) + 300.0).round()}));
            } else {
                m.insert("axial".into(), mid);
            }
            m
        }
        "saddle" => obj(json!({"axial": mid, "angle_deg": if horizontal { 180.0 } else { 0.0 }, "height_mm": 800.0})),
        "leg" => obj(
            json!({"axial": {"ref": "host_from", "mm": 0.0}, "length_mm": 1500.0, "count": 4, "section": {"pipe": {"nps": "4", "schedule": "40"}}}),
        ),
        "bracket" => obj(json!({
            "axial": mid, "angle_deg": 0.0, "count": 2,
            "gusset": {"height_mm": 250.0, "width_mm": 150.0, "thk_mm": 10.0},
            "top_plate": {"len_mm": 200.0, "wid_mm": 160.0, "thk_mm": 12.0},
        })),
        "anchor_chair" => obj(json!({
            "axial": {"ref": "host_from", "mm": 0.0}, "count": 8, "height_mm": 300.0, "width_mm": 200.0,
            "gusset_thk_mm": 16.0, "top_plate_thk_mm": 25.0, "bolt_dia_mm": 30.0,
        })),
        "stiffening_ring" => obj(json!({"axial": mid, "section": {"bar": {"height_mm": 100.0, "thk_mm": 12.0}}})),
        "limpet_coil" => obj(json!({"axial": {"ref": "host_from", "mm": 100.0}, "pipe_nps": "3", "schedule": "40", "pitch_mm": 150.0, "turns": 4.0})),
        "lifting_lug" => obj(json!({
            "axial": {"ref": "host_from", "mm": (length - 300.0).max(0.0).round()}, "style": "longitudinal",
            "plate": {"len_mm": 300.0, "height_mm": 150.0, "thk_mm": 16.0}, "hole_dia_mm": 40.0,
        })),
        "insulation_ring" => obj(json!({"axial": mid, "width_mm": 40.0, "thk_mm": 6.0, "segments": 4})),
        "clip" => {
            obj(json!({"axial": mid, "angle_deg": 0.0, "plate": {"len_mm": 100.0, "wid_mm": 75.0, "thk_mm": 8.0}, "count": 1, "purpose": "earthing"}))
        }
        other => {
            return Err(bad(
                cmd,
                format!("attachment kind must be one of {} (got `{}`)", ATTACHMENT_KINDS.join(", "), other.chars().take(40).collect::<String>()),
            ));
        }
    };
    let mut m = fields;
    if let Some((key, preset)) = preset_fields(s, kind, st) {
        for (k, v) in preset {
            m.insert(k, v);
        }
        m.insert("size_key".into(), json!(key));
    }
    m.insert("kind".into(), json!(kind));
    m.insert("host".into(), json!(host.id));
    Ok(m)
}

/// Build one attachment of `kind` from its defaults with `given` laid over them.
fn make_attachment(
    cmd: &str,
    s: &Session,
    doc: &VesselDoc,
    r: &Resolved,
    kind: &str,
    given: Option<&Value>,
) -> solidcraft_engine::Result<Attachment> {
    let host_id = given.and_then(|g| g.get("host")).and_then(Value::as_str).map(str::to_string);
    let host = match &host_id {
        Some(id) => doc.element(id).ok_or_else(|| bad(cmd, format!("no element `{}`", id.chars().take(40).collect::<String>())))?,
        None => default_host(doc, kind).ok_or_else(|| bad(cmd, "the vessel has no elements to attach to"))?,
    };
    let st = r.station(&host.id).ok_or_else(|| bad(cmd, format!("element `{}` did not resolve", host.id)))?;
    if kind == "anchor_chair" && !matches!(host.kind, ElementKind::Skirt { .. }) {
        return Err(bad(cmd, "anchor chairs sit on a skirt (add a skirt element first)"));
    }
    let mut fields = attachment_defaults(cmd, s, doc, kind, host, st)?;
    overlay(cmd, &mut fields, given, "attachment")?;
    let id = match fields.get("id").and_then(Value::as_str) {
        Some(id) => id.to_string(),
        None => free_id(doc, prefix(kind)),
    };
    fields.insert("id".into(), json!(id));
    let a: Attachment = serde_json::from_value(Value::Object(fields)).map_err(|e| bad(cmd, format!("`attachment`: {e}")))?;
    if doc.element(&a.id).is_some() || doc.attachments.iter().any(|x| x.id == a.id) {
        return Err(bad(cmd, format!("`{}` is already used", a.id)));
    }
    Ok(a)
}

fn resolved(doc: &VesselDoc) -> solidcraft_engine::Result<Resolved> {
    Ok(eval::resolve(doc)?)
}

fn attachment_add(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "vessel.attachment.add";
    let (mut doc, generated) = load(s)?;
    let given = p.get("attachment");
    let kind = given.and_then(|g| g.get("kind")).and_then(Value::as_str).or_else(|| str_(p, "kind")).unwrap_or("nozzle").to_string();
    let r = resolved(&doc)?;
    let a = make_attachment(cmd, s, &doc, &r, &kind, given)?;
    let id = a.id.clone();
    doc.attachments.push(a);
    store(s, &doc, &generated)?;
    Ok(json!({"doc": doc.to_value()?, "id": id, "ids": [id]}))
}

/// The support kind a vessel gets when none is named: saddles under a horizontal vessel, anchor
/// chairs on a skirt, else legs.
pub fn default_support(doc: &VesselDoc) -> &'static str {
    if doc.global.orientation == Orientation::Horizontal {
        "saddle"
    } else if doc.elements.iter().any(|e| matches!(e.kind, ElementKind::Skirt { .. })) {
        "anchor_chair"
    } else {
        "leg"
    }
}

/// The cylinder whose station holds `z`, else the first cylinder.
fn cylinder_at<'a>(doc: &'a VesselDoc, r: &Resolved, z: f64) -> Option<&'a Element> {
    let cyl = |e: &&Element| matches!(e.kind, ElementKind::Cylinder { .. });
    doc.elements
        .iter()
        .filter(cyl)
        .find(|e| r.station(&e.id).is_some_and(|st| st.z_from <= z && z <= st.z_to))
        .or_else(|| doc.elements.iter().find(cyl))
}

fn support_add(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "vessel.support.add";
    let (mut doc, generated) = load(s)?;
    let given = p.get("attachment");
    let kind = match given.and_then(|g| g.get("kind")).and_then(Value::as_str).or_else(|| str_(p, "kind")) {
        Some(k) => k.to_string(),
        None => default_support(&doc).to_string(),
    };
    if !matches!(kind.as_str(), "saddle" | "leg" | "bracket" | "anchor_chair" | "lifting_lug") {
        return Err(bad(cmd, "a support is a saddle, leg, bracket, anchor_chair or lifting_lug (vessel.attachment.add places the rest)"));
    }
    let r = resolved(&doc)?;
    let mut ids = Vec::new();
    // Saddles come in pairs at a fifth of the TL–TL from each tangent line, unless placed.
    let paired = kind == "saddle" && p.get("pair").and_then(Value::as_bool) != Some(false) && given.and_then(|g| g.get("axial")).is_none();
    if paired {
        let (a, b) = match (r.tl_bottom, r.tl_top) {
            (Some(a), Some(b)) => (a, b),
            _ => (r.z_min, r.z_max),
        };
        for z in [a + 0.2 * (b - a), a + 0.8 * (b - a)] {
            let mut g = obj(given.cloned().unwrap_or_else(|| json!({})));
            g.insert("axial".into(), json!({"ref": "datum", "mm": z.round()}));
            if !g.contains_key("host") {
                let host = cylinder_at(&doc, &r, z).ok_or_else(|| bad(cmd, "saddles sit on a cylinder (the vessel has none)"))?;
                g.insert("host".into(), json!(host.id));
            }
            g.remove("id");
            let a = make_attachment(cmd, s, &doc, &r, &kind, Some(&Value::Object(g)))?;
            ids.push(a.id.clone());
            doc.attachments.push(a);
        }
    } else {
        let a = make_attachment(cmd, s, &doc, &r, &kind, given)?;
        ids.push(a.id.clone());
        doc.attachments.push(a);
    }
    store(s, &doc, &generated)?;
    Ok(json!({"doc": doc.to_value()?, "id": ids.first(), "ids": ids}))
}

fn datum(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "vessel.datum";
    let (mut doc, generated) = load(s)?;
    let before = resolved(&doc)?;
    let mut next = doc.clone();
    if let Some(d) = p.get("datum") {
        next.global.datum = serde_json::from_value(d.clone()).map_err(|e| bad(cmd, format!("`datum`: {e}")))?;
    }
    if let Some(v) = p.get("plant_elevation_mm") {
        next.global.plant_elevation_mm = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(cmd, "`plant_elevation_mm` must be a number"))?;
    }
    if p.get("datum").is_none() && p.get("plant_elevation_mm").is_none() {
        return Err(bad(cmd, "give `datum` and/or `plant_elevation_mm`"));
    }
    next.validate()?;
    let after = resolved(&next)?;
    // Every station moves by the same amount when only the datum changes.
    let shift = match (before.stations.first(), after.stations.first()) {
        (Some(a), Some(b)) => b.z_from - a.z_from,
        _ => 0.0,
    };
    let keep = p.get("keep_positions").and_then(Value::as_bool).unwrap_or(true);
    let mut moved = Vec::new();
    if keep && shift != 0.0 {
        for a in next.attachments.iter_mut().filter(|a| a.axial.reference == crate::model::AxialRef::Datum) {
            a.axial.mm += shift;
            moved.push(a.id.clone());
        }
    }
    doc = next;
    store(s, &doc, &generated)?;
    Ok(json!({"doc": doc.to_value()?, "shift_mm": shift, "moved": moved}))
}

/// CSV field: quoted when it holds a comma, quote or line break.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => csv_field(s),
        other => csv_field(&other.to_string()),
    }
}

/// The BOM reply of `vessel.bom` as CSV: one row per item, then the totals.
pub fn bom_csv(bom: &Value) -> String {
    const COLS: [&str; 12] =
        ["item", "id", "description", "material", "qty", "basis", "volume_mm3", "density_kg_m3", "weight_kg", "handbook_kg", "delta_pct", "handbook"];
    let mut out = COLS.join(",");
    out.push('\n');
    for it in bom.get("items").and_then(Value::as_array).into_iter().flatten().take(10_000) {
        let row: Vec<String> = COLS.iter().map(|c| cell(it.get(*c).unwrap_or(&Value::Null))).collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }
    if let Some(t) = bom.get("totals") {
        for (k, label) in [
            ("empty_kg", "Empty weight (kg)"),
            ("operating_kg", "Operating weight (kg)"),
            ("test_kg", "Test weight (kg)"),
            ("water_m3", "Water capacity (m3)"),
        ] {
            out.push_str(&format!("{},,,,,,,,{},,,\n", csv_field(label), cell(t.get(k).unwrap_or(&Value::Null))));
        }
    }
    out
}

fn bom_export(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "vessel.bom.export";
    let format = str_(p, "format").unwrap_or("csv");
    let bom = crate::commands::bom_cmd(s, &json!({}))?;
    let (doc, _) = load(s)?;
    let (text, ext) = match format {
        "csv" => (bom_csv(&bom), "csv"),
        "json" => (serde_json::to_string_pretty(&bom).map_err(|e| EngineError::Other(e.to_string()))?, "json"),
        _ => return Err(bad(cmd, "`format` is csv or json")),
    };
    let filename = format!("{}-bom.{ext}", doc.id);
    if let Some(path) = str_(p, "path") {
        solidcraft_engine::io::vfs::write(path, text.as_bytes()).map_err(|e| EngineError::Other(format!("{path}: {e}")))?;
        return Ok(json!({"path": path, "filename": filename, "bytes": text.len(), "warnings": bom.get("warnings")}));
    }
    Ok(json!({"text": text, "filename": filename, "warnings": bom.get("warnings")}))
}

pub(crate) mod specs {
    use super::*;
    use solidcraft_engine::CommandSpec;

    use crate::commands::{TAB, has_vessel};

    pub const ELEMENT_ADD: CommandSpec =
        CommandSpec::new("vessel.element.add", "Add Element", element_add).at(TAB, "CREATE").icon("cylinder").enabled(has_vessel).doc_only().params(
            "element?: {kind, thickness_mm?, diameter_mm?, length_mm?, …} (laid over the defaults: the neighbour's thickness, the inherited \
             diameter, a 1000 mm cylinder / 2:1 head / half-diameter cone / 1500 mm skirt); kind?: cylinder|head|cone|skirt|flat_cover|body_flange \
             (default cylinder); index? | before?: id | after?: id (default: before a closing head, else last) → doc, id, index",
        );
    pub const ATTACHMENT_ADD: CommandSpec = CommandSpec::new("vessel.attachment.add", "Add Nozzle or Attachment", attachment_add)
        .at(TAB, "INSERT")
        .icon("hole")
        .enabled(has_vessel)
        .doc_only()
        .params(
            "attachment?: {kind, host, axial, angle_deg, …} (laid over the defaults: a 4 in Sch 40 nozzle with a B16.5 Cl 150 WN RF flange, \
             200 from the OD, radial at the middle of the first cylinder, or axial on a head; supports sized from the presets by the host's OD \
             with that size_key); kind?: nozzle|saddle|leg|bracket|anchor_chair|stiffening_ring|limpet_coil|lifting_lug|insulation_ring|clip \
             (default nozzle) → doc, id, ids",
        );
    pub const SUPPORT_ADD: CommandSpec =
        CommandSpec::new("vessel.support.add", "Add Support", support_add).at(TAB, "INSERT").icon("component").enabled(has_vessel).doc_only().params(
            "kind?: saddle|leg|bracket|anchor_chair|lifting_lug (default: saddles under a horizontal vessel, anchor chairs on a skirt, else \
             legs); attachment?: fields laid over the preset; pair?: bool (saddles: two at 20 % of the TL–TL from each TL unless \
             attachment.axial is given; default true) → doc, id, ids",
        );
    pub const DATUM: CommandSpec =
        CommandSpec::new("vessel.datum", "Change Datum", datum).at(TAB, "MODIFY").icon("axis").enabled(has_vessel).doc_only().params(
            "datum?: bottom_tl|bottom_seam|skirt_base|plant_elevation|left_tl; plant_elevation_mm?: number; keep_positions?: bool (default \
             true: attachments measured from the datum get their `axial.mm` rewritten so they stay where they are) → doc, shift_mm (how far \
             every elevation moved), moved: [ids]",
        );
    pub const BOM_EXPORT: CommandSpec =
        CommandSpec::new("vessel.bom.export", "Export BOM", bom_export).at(TAB, "INSPECT").icon("export").noundo().enabled(has_vessel).params(
            "format?: csv|json (default csv); path?: where to write it (desktop path; in the browser `/downloads/<name>` becomes a download) \
             → path, filename, bytes | text, filename; warnings (as vessel.bom: regenerate first for measured weights)",
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_ids_skip_used_ones_and_csv_quotes() {
        let doc = VesselDoc::cylinder("E1", 1000.0, 10.0, 2000.0);
        assert_eq!(free_id(&doc, "E"), "E2");
        assert_eq!(free_id(&doc, "N"), "N1");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        let csv = bom_csv(&json!({"items": [{"item": 1, "id": "E1", "description": "Shell, rolled", "qty": 1}], "totals": {"empty_kg": 12.5}}));
        assert!(csv.starts_with("item,id,description,"), "{csv}");
        assert!(csv.contains("1,E1,\"Shell, rolled\",,1,"), "{csv}");
        assert!(csv.contains("Empty weight (kg),,,,,,,,12.5,,,"), "{csv}");
    }
}
