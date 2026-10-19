// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The `vessel.*` commands: upstream `CommandSpec`s registered through the engine's extension
//! slot (`solidcraft_engine::cmd::triya_extra::register(COMMANDS)`, done by the app at startup).
//!
//! The vessel document lives in the design's metadata under [`METADATA_KEY`] (see the crate
//! docs for why). `vessel.regenerate` runs the planned engine commands *inside* its own run,
//! the way upstream's `selection.delete` runs other commands' bodies (`spec.enabled` then
//! `spec.run`, see `crates/engine/src/cmd/delete.rs::sub`): the outer command owns the undo
//! step and the evaluation, so the whole regeneration is one undo step and a failure half-way
//! leaves the design as it was (`Session::execute` restores it). `Session::execute` itself is
//! not used for the nested calls: it would record one undo step per step.

use std::f64::consts::PI;

use serde_json::{Value, json};
use solidcraft_engine::params::{bad, num, str_};
use solidcraft_engine::{CommandSpec, EngineError, Session};

use crate::emit::{CommandCall, substitute};
use crate::model::{AttachmentKind, VesselDoc};
use crate::{VesselError, bom, emit, eval, nozzle, presets, support};

/// The reverse-DNS metadata key the vessel JSON is kept under.
pub const METADATA_KEY: &str = "in.openengtools.triya.vessel";

/// Toolbar tab and panel. Upstream's tab names are upper-case (`SOLID`, `SHEET METAL`) and the
/// UI translates them with `tr_label`; the `VESSEL` tab is added to `toolbar::TABS` by the
/// overlay's seam, and panels of an unlisted tab are `MAKE`, `ADD-INS`, `UTILITY`, `INSPECT`,
/// `SELECT` (`toolbar::panels`).
pub const TAB: &str = "VESSEL";

pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec::new("vessel.new", "New Vessel", new).at(TAB, "MAKE").icon("cylinder").doc_only().params(
        "doc?: a triya.vessel/0.1 document (replaces any vessel in the design); or diameter_mm, thickness_mm, length_mm (a one-cylinder \
         vessel; default 1000 × 10 × 2000), name?; replace?: bool (default false: refused when the design already has a vessel) → doc",
    ),
    CommandSpec::new("vessel.set", "Change Vessel", set).at(TAB, "MAKE").icon("params").doc_only().params(
        "doc: a whole triya.vessel/0.1 document; or element: {id, …} (added, or replaced by id), attachment: {id, …} (likewise), \
         remove: id (an element or attachment), global: {…} (fields to change); the result is validated before it is stored → doc",
    ),
    CommandSpec::new("vessel.regenerate", "Regenerate Vessel", regenerate).at(TAB, "MAKE").icon("compute").enabled(has_vessel).params(
        "→ steps: [{command, params, reply}], features: [ids] (what it built, kept in the timeline folder `Vessel <id>` → group; only \
         the features of the previous regeneration are deleted first). One undo step. Also writes every dimension as a user parameter \
         V<vessel>_<element>_<field> (mm) → parameters. Nozzles are cut and joined into their host and kept when the host's measured \
         volume matches the reference; otherwise built as a separate body with flags.boolean_failed → nozzles: [{id, built: \
         joined|separate|none, expected_mm3, measured_mm3, warning}]; other attachments are not built yet → warnings. A nozzle that \
         cannot be placed is an error and nothing changes",
    ),
    CommandSpec::new("vessel.inspect", "Inspect Vessel", inspect).at(TAB, "INSPECT").icon("measure").noundo().doc_only().params(
        "→ doc, stations: [{id, kind, z_from, z_to, r_from, r_to, thickness, diameter_mm, envelope, tl, half_apex_deg, profile, detail}], \
         extent: {z_min, z_max, tl_bottom, tl_top, tl_to_tl}, warnings (parameter drift included), flags: [{id, boolean_failed, unverified_table}], parameters: \
         [{name, value, unit}], plan: [{command, params}] | plan_error, nozzles: [{id, host, origin, dir, s_inner, s_outer, s_start, s_face, \
         neck_od, neck_wall, pad, error?}], generated: [feature ids], group",
    ),
    CommandSpec::new("vessel.import_tables", "Import Standard Tables", import_tables).at(TAB, "UTILITY").icon("import").doc_only().params(
        "tables: {flanges?: <flange table>, supports?: <support-preset table>} (the shipped JSON table format, `docs/standards-library.md`; \
         each replaces the design's private table of that kind) | path: a JSON file of that shape (desktop) | clear: true (drops them). \
         Kept in the design, searched before the shipped tables; rows without `verified` count as unverified. At most 60 KiB → private_tables",
    ),
    CommandSpec::new("vessel.bom", "Vessel BOM and Weights", bom_cmd).at(TAB, "INSPECT").icon("measure").noundo().enabled(has_vessel).params(
        "→ items: [{item, id, body, description, material, qty, basis: measured|measured less nozzles|computed|not built, density_kg_m3, \
         volume_mm3, weight_kg, handbook_kg, handbook (the rule), delta_pct}] (shell elements, nozzle necks and flanges, every support \
         part, any other `Vessel …` body), totals: {empty_kg, operating_kg, test_kg, water_m3, placeholders: [what is not modelled]}, \
         warnings. Weights from the measured bodies (regenerate first) × the material's density; handbook values from the dimensions",
    ),
];

fn has_vessel(s: &Session) -> std::result::Result<(), String> {
    if s.doc.metadata.get(METADATA_KEY).is_some() { Ok(()) } else { Err("the design has no vessel (vessel.new first)".into()) }
}

/// The features a regeneration made, and the timeline folder that holds them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Generated {
    pub features: Vec<u64>,
    /// The timeline folder (`Vessel <id>`) the features sit in; the metadata entry is the tag.
    pub group: Option<u64>,
}

/// The vessel JSON goes into numbered chunk keys when the document serialises to more than this
/// (one metadata value is limited to 64 KiB upstream).
pub const CHUNK_BYTES: usize = 60 * 1024;
/// At most this many chunks (upstream limits all metadata to 256 KiB).
pub const MAX_CHUNKS: usize = 4;

/// The metadata key of chunk `n` (1-based); it holds `{"text": <a piece of the JSON>}`.
pub fn chunk_key(n: usize) -> String {
    format!("{METADATA_KEY}.chunk-{n}")
}

/// `text` split into pieces whose JSON string form stays within `limit` bytes.
fn split_escaped(text: &str, limit: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut cost = 2; // the quotes
    for c in text.chars() {
        let n = match c {
            '"' | '\\' | '\n' | '\r' | '\t' => 2,
            c if (c as u32) < 0x20 => 6,
            c => c.len_utf8(),
        };
        if cost + n > limit && !piece.is_empty() {
            pieces.push(std::mem::take(&mut piece));
            cost = 2;
        }
        piece.push(c);
        cost += n;
    }
    if !piece.is_empty() {
        pieces.push(piece);
    }
    pieces
}

/// The stored vessel and what its last regeneration made.
fn load(s: &Session) -> solidcraft_engine::Result<(VesselDoc, Generated)> {
    let v = s.doc.metadata.get(METADATA_KEY).ok_or_else(|| EngineError::Other("the design has no vessel (vessel.new first)".into()))?;
    let doc = match (v.get("doc"), v.get("chunks").and_then(Value::as_u64)) {
        (Some(doc), _) => VesselDoc::from_value(doc)?,
        (None, Some(n)) if (1..=MAX_CHUNKS as u64).contains(&n) => {
            let mut text = String::new();
            for i in 1..=n as usize {
                let part = s.doc.metadata.get(&chunk_key(i)).and_then(|c| c.get("text")).and_then(Value::as_str);
                text.push_str(part.ok_or_else(|| EngineError::Other(format!("the vessel's chunk {i} of {n} is missing")))?);
            }
            VesselDoc::from_json(&text)?
        }
        (None, Some(n)) => return Err(EngineError::Other(format!("the vessel is stored in {n} chunks; at most {MAX_CHUNKS} are read"))),
        (None, None) => return Err(EngineError::Other(format!("`{METADATA_KEY}` holds no `doc`"))),
    };
    let g = v.get("generated");
    let features = g
        .and_then(|g| g.get("features"))
        .and_then(Value::as_array)
        .map(|a| a.iter().take(crate::emit::MAX_STEPS).filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    let group = g.and_then(|g| g.get("group")).and_then(Value::as_u64);
    Ok((doc, Generated { features, group }))
}

/// Store the vessel (validated) and what was generated in the design's metadata: inline when
/// the document is small, else as up to [`MAX_CHUNKS`] text chunks of its JSON.
fn store(s: &mut Session, doc: &VesselDoc, generated: &Generated) -> solidcraft_engine::Result<()> {
    doc.validate()?;
    let value = doc.to_value()?;
    let text = serde_json::to_string(&value).map_err(|e| EngineError::Other(e.to_string()))?;
    let gen_ = json!({"features": generated.features, "group": generated.group});
    let mut next = s.doc.metadata.clone();
    for i in 1..=MAX_CHUNKS {
        next.remove(&chunk_key(i));
    }
    let fail = |e: solidcraft_engine::doc::DocError| EngineError::Other(format!("cannot store the vessel: {e}"));
    let head = if text.len() <= CHUNK_BYTES {
        json!({"schema": crate::model::SCHEMA, "doc": value, "generated": gen_})
    } else {
        // Upstream wants an object under each key: `{"text": …}`, the wrapper inside the budget.
        let pieces = split_escaped(&text, CHUNK_BYTES - 16);
        if pieces.len() > MAX_CHUNKS {
            return Err(EngineError::Other(format!(
                "the vessel document is {} KiB; at most {} KiB can be kept in a design",
                text.len() / 1024,
                MAX_CHUNKS * CHUNK_BYTES / 1024
            )));
        }
        for (i, piece) in pieces.iter().enumerate() {
            next.set(&chunk_key(i + 1), json!({"text": piece})).map_err(fail)?;
        }
        json!({"schema": crate::model::SCHEMA, "chunks": pieces.len(), "generated": gen_})
    };
    next.set(METADATA_KEY, head).map_err(fail)?;
    if next != s.doc.metadata {
        s.doc_mut().metadata = next;
    }
    Ok(())
}

fn new(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "vessel.new";
    if s.doc.metadata.get(METADATA_KEY).is_some() && p.get("replace").and_then(Value::as_bool) != Some(true) {
        return Err(bad(cmd, "the design already has a vessel (vessel.set changes it; replace: true starts over)"));
    }
    let doc = match p.get("doc") {
        Some(v) => VesselDoc::from_value(v)?,
        None => {
            let d = num(p, "diameter_mm").unwrap_or(1000.0);
            let t = num(p, "thickness_mm").unwrap_or(10.0);
            let l = num(p, "length_mm").unwrap_or(2000.0);
            let mut doc = VesselDoc::cylinder("S1", d, t, l);
            doc.title = str_(p, "name").map(str::to_string);
            doc
        }
    };
    store(s, &doc, &Generated::default())?;
    Ok(json!({"doc": doc.to_value()?}))
}

fn set(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "vessel.set";
    let existing = if s.doc.metadata.get(METADATA_KEY).is_some() { Some(load(s)?) } else { None };
    let (mut doc, generated) = match p.get("doc") {
        Some(v) => (VesselDoc::from_value(v)?, existing.map(|(_, g)| g).unwrap_or_default()),
        None => existing.ok_or_else(|| EngineError::Other("the design has no vessel (vessel.new first)".into()))?,
    };
    let mut changed = p.get("doc").is_some();
    if let Some(v) = p.get("element") {
        let e: crate::model::Element = serde_json::from_value(v.clone()).map_err(|e| bad(cmd, format!("`element`: {e}")))?;
        match doc.elements.iter_mut().find(|x| x.id == e.id) {
            Some(slot) => *slot = e,
            None => doc.elements.push(e),
        }
        changed = true;
    }
    if let Some(v) = p.get("attachment") {
        let a: crate::model::Attachment = serde_json::from_value(v.clone()).map_err(|e| bad(cmd, format!("`attachment`: {e}")))?;
        match doc.attachments.iter_mut().find(|x| x.id == a.id) {
            Some(slot) => *slot = a,
            None => doc.attachments.push(a),
        }
        changed = true;
    }
    if let Some(v) = p.get("global") {
        let mut gv = serde_json::to_value(&doc.global).map_err(|e| EngineError::Other(e.to_string()))?;
        match (gv.as_object_mut(), v.as_object()) {
            (Some(into), Some(from)) if from.len() <= 64 => {
                for (k, x) in from {
                    into.insert(k.clone(), x.clone());
                }
            }
            _ => return Err(bad(cmd, "`global` must be an object of fields to change")),
        }
        doc.global = serde_json::from_value(gv).map_err(|e| bad(cmd, format!("`global`: {e}")))?;
        changed = true;
    }
    if let Some(id) = p.get("remove") {
        let id = id.as_str().ok_or_else(|| bad(cmd, "`remove` must be an element or attachment id"))?;
        let n = doc.elements.len() + doc.attachments.len();
        doc.elements.retain(|e| e.id != id);
        doc.attachments.retain(|a| a.id != id);
        if doc.elements.len() + doc.attachments.len() == n {
            return Err(bad(cmd, format!("no element or attachment `{id}`")));
        }
        changed = true;
    }
    if !changed {
        return Err(bad(cmd, "give `doc`, `element`, `attachment`, `global` or `remove`"));
    }
    doc.validate()?;
    store(s, &doc, &generated)?;
    Ok(json!({"doc": doc.to_value()?}))
}

fn station_json(st: &eval::Station) -> Value {
    let profile = match st.profile {
        eval::Profile::Straight => json!({"type": "straight"}),
        eval::Profile::Conical { knuckle_from, knuckle_to } => json!({"type": "conical", "knuckle_from": knuckle_from, "knuckle_to": knuckle_to}),
        eval::Profile::Head { side, sf, dish, shape } => {
            let side = match side {
                eval::HeadSide::CrownAtStart => "crown_at_start",
                eval::HeadSide::CrownAtEnd => "crown_at_end",
            };
            let shape = match shape {
                eval::HeadShape::Torispherical { crown_r, knuckle_r } => json!({"type": "torispherical", "crown_r": crown_r, "knuckle_r": knuckle_r}),
                eval::HeadShape::Ellipsoidal => json!({"type": "ellipsoidal"}),
            };
            json!({"type": "head", "side": side, "sf": sf, "dish": dish, "shape": shape})
        }
    };
    let detail = match st.detail {
        eval::Detail::None => Value::Null,
        eval::Detail::Flange(f) => json!({
            "type": "flange", "od": f.od, "bolt_circle": f.bolt_circle, "bolt_count": f.bolt_count, "bolt_hole": f.bolt_hole,
            "thickness": f.thickness, "hub_od": f.hub_od, "hub_len": f.hub_len, "gap": f.gap, "hub_at_start": f.hub_at_start,
        }),
        eval::Detail::BaseRing { id, od, thickness } => json!({"type": "base_ring", "id": id, "od": od, "thickness": thickness}),
    };
    json!({
        "id": st.id, "kind": st.kind, "z_from": st.z_from, "z_to": st.z_to, "r_from": st.r_from, "r_to": st.r_to,
        "thickness": st.thickness, "diameter_mm": st.diameter_mm, "envelope": st.envelope, "tl": st.tl,
        "half_apex_deg": st.half_apex_deg, "profile": profile, "detail": detail,
    })
}

fn bom_cmd(s: &mut Session, _p: &Value) -> solidcraft_engine::Result<Value> {
    let (mut doc, _) = load(s)?;
    let tables = presets::tables(s)?;
    presets::apply(&mut doc, &tables, presets::private_support_tables(s))?;
    let r = eval::resolve_with(&doc, &tables)?;
    let measured_all = sub(s, &CommandCall { id: "inspect.measure".into(), params: json!({}) })?;
    let mut measured: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for b in measured_all.get("bodies").and_then(Value::as_array).into_iter().flatten() {
        if let (Some(n), Some(v)) = (b.get("name").and_then(Value::as_str), b.get("volume_mm3").and_then(Value::as_f64))
            && n.starts_with("Vessel ")
        {
            measured.insert(n.to_string(), v);
        }
    }
    let mut warnings = Vec::new();
    if measured.is_empty() {
        warnings.push("nothing is built: regenerate the vessel first (weights are handbook values only)".to_string());
    }
    let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let material_of = |m: &Option<String>| m.clone().unwrap_or_else(|| doc.global.default_material.clone());
    let mut items: Vec<bom::Item> = Vec::new();
    // Nozzles first: a joined one's share comes off its host's measured volume.
    let mut joined_share: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    let mut nozzle_items = Vec::new();
    for a in doc.attachments.iter().filter(|a| matches!(a.kind, AttachmentKind::Nozzle { .. })) {
        let material = material_of(&a.material);
        let density = bom::density(&material);
        let g = match nozzle::resolve(&doc, &r, a, &tables) {
            Ok(g) => g,
            Err(e) => {
                warnings.push(format!("nozzle `{}`: {e}", a.id));
                continue;
            }
        };
        let pipe = |from: f64| PI / 4.0 * (g.neck_od.powi(2) - g.neck_id().powi(2)) * (g.s_neck_end - from).max(0.0);
        let separate = nozzle::body_name(&g.id, "neck (separate)");
        let host_body = format!("Vessel {}", g.host);
        let what = format!("nozzle {} neck {} × {}", g.id, g.neck_od, g.neck_wall);
        // A separate neck is measured and checked against its pipe; a joined one is its computed
        // share of the host (neck and pad outside the shell, less the bore through the wall).
        let rule = "pipe table: π/4·(OD²−ID²)·length";
        let (body, basis, volume, handbook, rule) = if let Some(v) = measured.get(&separate) {
            used.insert(separate.clone());
            (Some(separate), "measured", Some(*v), Some(pipe(nozzle::separate_from(&g))), rule)
        } else if measured.contains_key(&host_body) {
            let share = r.station(&g.host).and_then(|host| nozzle::expected_change(&doc.global, host, &g, 64));
            if let Some(v) = share {
                *joined_share.entry(g.host.clone()).or_insert(0.0) += v;
            }
            (Some(host_body), "computed", share, None, "pipe table, joined: neck + pad − bore through the wall")
        } else {
            (None, "not built", None, Some(pipe(g.frame.s_outer)), rule)
        };
        nozzle_items.push(bom::Item {
            id: g.id.clone(),
            body,
            description: if g.pad.is_some() { format!("{what} with pad") } else { what },
            material: material.clone(),
            qty: 1,
            basis,
            volume_mm3: volume,
            handbook_mm3: handbook,
            handbook_rule: Some(rule.into()),
            density,
        });
        if let Some(f) = &g.flange {
            let name = nozzle::body_name(&g.id, "flange");
            let v = measured.get(&name).copied();
            if v.is_some() {
                used.insert(name.clone());
            }
            nozzle_items.push(bom::Item {
                id: g.id.clone(),
                body: v.map(|_| name),
                description: format!("flange {} OD {}", f.kind.code(), f.od),
                material: material.clone(),
                qty: 1,
                basis: if v.is_some() { "measured" } else { "not built" },
                volume_mm3: v,
                handbook_mm3: Some(bom::flange_volume(f, g.neck_id())),
                handbook_rule: Some("flange table: ring − holes + RF + hub".into()),
                density,
            });
        }
    }
    for e in &doc.elements {
        let Some(st) = r.station(&e.id) else { continue };
        let material = material_of(&e.material);
        let density = bom::density(&material);
        let name = format!("Vessel {}", e.id);
        let share = joined_share.get(&e.id).copied().unwrap_or(0.0);
        let v = measured.get(&name).map(|v| v - share);
        if v.is_some() {
            used.insert(name.clone());
        }
        let hb = bom::handbook_volume(e, st);
        items.push(bom::Item {
            id: e.id.clone(),
            body: v.map(|_| name),
            description: e.description.clone().unwrap_or_else(|| format!("{} {} thk", e.kind.name(), e.thickness_mm)),
            material,
            qty: 1,
            basis: match v {
                None => "not built",
                Some(_) if share > 0.0 => "measured less nozzles",
                Some(_) => "measured",
            },
            volume_mm3: v,
            handbook_mm3: hb.map(|h| h.0),
            handbook_rule: hb.map(|h| h.1.to_string()),
            density,
        });
    }
    items.extend(nozzle_items);
    for a in doc.attachments.iter().filter(|a| !matches!(a.kind, AttachmentKind::Nozzle { .. })) {
        let material = material_of(&a.material);
        let density = bom::density(&material);
        let g = match support::resolve(&doc, &r, a, &tables) {
            Ok(g) => g,
            Err(e) => {
                warnings.push(format!("attachment `{}`: {e}", a.id));
                continue;
            }
        };
        for p in &g.parts {
            let v = measured.get(&p.name).copied();
            if v.is_some() {
                used.insert(p.name.clone());
            }
            items.push(bom::Item {
                id: g.id.clone(),
                body: v.map(|_| p.name.clone()),
                description: p.what.clone(),
                material: material.clone(),
                qty: 1,
                basis: if v.is_some() { "measured" } else { "not built" },
                volume_mm3: v,
                handbook_mm3: Some(p.volume),
                handbook_rule: Some("closed form".into()),
                density,
            });
        }
    }
    // Anything else the vessel built (a fallback body) still weighs.
    let default_density = bom::density(&doc.global.default_material);
    for (name, v) in &measured {
        if !used.contains(name) {
            if doc.elements.iter().any(|e| format!("Vessel {}", e.id) == *name) {
                continue;
            }
            items.push(bom::Item {
                id: name.trim_start_matches("Vessel ").split(' ').next().unwrap_or("").to_string(),
                body: Some(name.clone()),
                description: name.trim_start_matches("Vessel ").to_string(),
                material: doc.global.default_material.clone(),
                qty: 1,
                basis: "measured",
                volume_mm3: Some(*v),
                handbook_mm3: None,
                handbook_rule: None,
                density: default_density,
            });
        }
    }
    // A joined nozzle is weighed as its share of its host, so the totals count every body once.
    let empty: f64 = items.iter().filter_map(bom::Item::weight_kg).sum();
    let water_m3 = bom::inside_volume(&r) * 1e-9;
    let test = empty + water_m3 * 1000.0;
    let operating = doc.design_data.operating_weight_kg.unwrap_or(empty);
    let mut placeholders = vec!["test weight: empty + the vessel full of water (1000 kg/m³); hydrotest data not modelled".to_string()];
    if doc.design_data.operating_weight_kg.is_none() {
        placeholders.push("operating weight: the vessel document has no contents, so the empty weight".to_string());
    }
    Ok(json!({
        "items": items.iter().enumerate().map(|(i, it)| it.to_json(i + 1)).collect::<Vec<_>>(),
        "totals": {"empty_kg": bom::round2(empty), "operating_kg": bom::round2(operating), "test_kg": bom::round2(test),
            "water_m3": (water_m3 * 1e4).round() / 1e4, "placeholders": placeholders},
        "warnings": warnings,
    }))
}

/// Rows and sources of the design's private tables.
fn private_summary(s: &Session) -> Value {
    let Some(v) = s.doc.metadata.get(presets::TABLES_KEY) else { return Value::Null };
    let mut out = json!({});
    for k in ["flanges", "supports"] {
        if let Some(t) = v.get(k) {
            out[k] = json!({"source": t.get("source"), "rows": t.get("rows").and_then(Value::as_array).map_or(0, Vec::len)});
        }
    }
    out
}

fn import_tables(s: &mut Session, p: &Value) -> solidcraft_engine::Result<Value> {
    let cmd = "vessel.import_tables";
    let clear = p.get("clear").and_then(Value::as_bool) == Some(true);
    let given = match (p.get("tables"), str_(p, "path")) {
        (Some(t), None) => Some(t.clone()),
        (None, Some(path)) => {
            let meta = std::fs::metadata(path).map_err(|e| bad(cmd, format!("`{path}`: {e}")))?;
            if meta.len() > presets::MAX_TABLES_BYTES as u64 * 4 {
                return Err(bad(cmd, format!("`{path}` is {} KiB; at most 240 KiB of JSON", meta.len() / 1024)));
            }
            let text = std::fs::read_to_string(path).map_err(|e| bad(cmd, format!("`{path}`: {e}")))?;
            Some(serde_json::from_str::<Value>(&text).map_err(|e| bad(cmd, format!("`{path}`: {e}")))?)
        }
        (Some(_), Some(_)) => return Err(bad(cmd, "give `tables` or `path`, not both")),
        (None, None) if clear => None,
        (None, None) => return Err(bad(cmd, "give `tables`, `path` or `clear: true`")),
    };
    let mut next = if clear { json!({}) } else { s.doc.metadata.get(presets::TABLES_KEY).cloned().unwrap_or_else(|| json!({})) };
    if let Some(given) = given {
        let o = given.as_object().ok_or_else(|| bad(cmd, "`tables` must be {flanges?, supports?}"))?;
        if o.is_empty() {
            return Err(bad(cmd, "`tables` names no table"));
        }
        for (k, t) in o {
            if k != "flanges" && k != "supports" {
                return Err(bad(cmd, format!("`tables` holds `flanges` and `supports`, not `{k}`")));
            }
            next[k] = t.clone();
        }
    }
    let text = serde_json::to_string(&next).map_err(|e| EngineError::Other(e.to_string()))?;
    if text.len() > presets::MAX_TABLES_BYTES {
        return Err(bad(cmd, format!("the private tables are {} KiB; a design keeps at most 60 KiB", text.len() / 1024)));
    }
    // Every row is checked before anything is stored.
    presets::with_private(triya_stdlib::TableSet::embedded().map_err(VesselError::from)?, &next).map_err(|e| bad(cmd, e.to_string()))?;
    let mut meta = s.doc.metadata.clone();
    if next.as_object().is_some_and(serde_json::Map::is_empty) {
        meta.remove(presets::TABLES_KEY);
    } else {
        meta.set(presets::TABLES_KEY, next).map_err(|e| bad(cmd, format!("cannot keep the tables: {e}")))?;
    }
    if meta != s.doc.metadata {
        s.doc_mut().metadata = meta;
    }
    Ok(json!({"private_tables": private_summary(s)}))
}

fn inspect(s: &mut Session, _p: &Value) -> solidcraft_engine::Result<Value> {
    let (mut doc, generated) = load(s)?;
    let tables = presets::tables(s)?;
    let (applied, preset_warnings) = presets::apply(&mut doc, &tables, presets::private_support_tables(s))?;
    let flags: Vec<Value> = doc
        .attachments
        .iter()
        .filter(|a| a.flags != crate::model::Flags::default())
        .map(|a| json!({"id": a.id, "boolean_failed": a.flags.boolean_failed, "unverified_table": a.flags.unverified_table}))
        .collect();
    let mut out = json!({
        "doc": doc.to_value()?, "flags": flags,
        "generated": generated.features, "group": generated.group,
        "stations": [], "plan": null, "plan_error": null, "warnings": [], "parameters": [], "nozzles": [], "supports": [],
        "presets": applied.iter().map(presets::Applied::to_json).collect::<Vec<_>>(),
        "private_tables": private_summary(s),
    });
    match eval::resolve_with(&doc, &tables) {
        Ok(r) => {
            out["stations"] = r.stations.iter().map(station_json).collect();
            out["extent"] = json!({"z_min": r.z_min, "z_max": r.z_max, "tl_bottom": r.tl_bottom, "tl_top": r.tl_top, "tl_to_tl": r.tl_to_tl()});
            let emitted = emit::parameters(&doc.id, &r);
            // A parameter the user changed in the table no longer matches the document; the next
            // regenerate rewrites it (`vessel-schema.md` §6).
            let table = sub(s, &CommandCall { id: "parameters.list".into(), params: json!({"filter": "user"}) }).unwrap_or(Value::Null);
            let mut warnings = r.warnings.clone();
            warnings.extend(preset_warnings.iter().cloned());
            for row in table.get("parameters").and_then(Value::as_array).into_iter().flatten() {
                let (Some(name), Some(value)) = (row.get("name").and_then(Value::as_str), row.get("value").and_then(Value::as_f64)) else { continue };
                if let Some(p) = emitted.iter().find(|p| p.name == name)
                    && (p.value - value).abs() > 1e-6 * p.value.abs().max(1.0)
                {
                    warnings
                        .push(format!("parameter `{name}` is {value} in the design but {} in the vessel (the next regenerate rewrites it)", p.value));
                }
            }
            out["warnings"] = json!(warnings);
            out["parameters"] = emitted.into_iter().map(|p| json!({"name": p.name, "value": p.value, "unit": p.unit})).collect();
            match emit::plan(&r) {
                Ok(p) => out["plan"] = p.into_iter().map(|c| json!({"command": c.id, "params": c.params})).collect(),
                Err(e) => out["plan_error"] = json!(e.to_string()),
            }
            out["nozzles"] = doc
                .attachments
                .iter()
                .filter(|a| matches!(a.kind, AttachmentKind::Nozzle { .. }))
                .map(|a| match nozzle::resolve(&doc, &r, a, &tables) {
                    Ok(g) => json!({
                        "id": g.id, "host": g.host, "origin": g.frame.origin, "dir": g.frame.dir, "s_inner": g.frame.s_inner,
                        "s_outer": g.frame.s_outer, "s_start": g.s_start, "s_face": g.s_face, "neck_od": g.neck_od, "neck_wall": g.neck_wall,
                        "pad": g.pad.map(|p| json!({"od": p.od, "thickness": p.thickness})), "manway": g.manway, "davit": g.davit,
                        "s_neck_end": g.s_neck_end, "flange": g.flange.map(|f| json!({"kind": f.kind.code(), "od": f.od, "bolt_circle": f.bolt_circle,
                            "bolt_count": f.bolt_count, "bolt_hole": f.bolt_hole, "thickness": f.thickness, "hub_od": f.hub_od, "rf_d": f.rf_d, "rf_h": f.rf_h})),
                    }),
                    Err(e) => json!({"id": a.id, "error": e.to_string()}),
                })
                .collect();
            out["supports"] = doc
                .attachments
                .iter()
                .filter(|a| !matches!(a.kind, AttachmentKind::Nozzle { .. }))
                .map(|a| match support::resolve(&doc, &r, a, &tables) {
                    Ok(g) => json!({
                        "id": g.id, "kind": g.kind, "host": g.host, "volume_mm3": g.volume(), "warnings": g.warnings,
                        "parts": g.parts.iter().map(|p| json!({"name": p.name, "what": p.what, "volume_mm3": p.volume})).collect::<Vec<_>>(),
                    }),
                    Err(e) => json!({"id": a.id, "kind": a.kind.name(), "error": e.to_string()}),
                })
                .collect();
        }
        Err(e) => out["plan_error"] = json!(e.to_string()),
    }
    Ok(out)
}

/// Run another command's body inside this one (the outer command owns the undo step), the way
/// upstream's `selection.delete` does.
fn sub(s: &mut Session, call: &CommandCall) -> solidcraft_engine::Result<Value> {
    let spec = solidcraft_engine::find_command(&call.id).ok_or_else(|| EngineError::UnknownCommand(call.id.clone()))?;
    (spec.enabled)(s).map_err(|why| EngineError::Disabled(spec.id.to_string(), why))?;
    let r = (spec.run)(s, &call.params)?;
    s.refresh();
    Ok(r)
}

/// The timeline folder name the vessel's features are kept in.
pub fn group_name(doc: &VesselDoc) -> String {
    format!("Vessel {}", doc.id)
}

fn regenerate(s: &mut Session, _p: &Value) -> solidcraft_engine::Result<Value> {
    let (mut doc, previous) = load(s)?;
    let tables = presets::tables(s)?;
    // Presets first: an attachment with a `size_key` takes its row's fields (private table
    // first), so importing a table changes what is built.
    let (applied, preset_warnings) = presets::apply(&mut doc, &tables, presets::private_support_tables(s))?;
    let resolved = eval::resolve_with(&doc, &tables)?;
    // Every nozzle is placed before anything is built: a nozzle that cannot be placed is an
    // error in the document, and the design stays as it was.
    let mut nozzles = Vec::new();
    for a in &doc.attachments {
        if matches!(a.kind, AttachmentKind::Nozzle { .. }) {
            nozzles.push(nozzle::resolve(&doc, &resolved, a, &tables)?);
        }
    }
    // Likewise every support and detail; a kind the beta does not build yet is reported.
    let mut supports = Vec::new();
    let mut pending = Vec::new();
    for a in &doc.attachments {
        if matches!(a.kind, AttachmentKind::Nozzle { .. }) {
            continue;
        }
        match support::resolve(&doc, &resolved, a, &tables) {
            Ok(g) => supports.push(g),
            Err(VesselError::NotSupportedYet(what)) => pending.push(format!("attachment `{}` ({}): not built yet ({what})", a.id, a.kind.name())),
            Err(e) => return Err(e.into()),
        }
    }
    let plan = emit::plan(&resolved)?;
    // Only what the last regeneration made goes (sketches take their revolves with them); a
    // feature the user added, even one moved into the vessel's folder, stays.
    let stale: Vec<u64> = previous.features.iter().copied().filter(|id| s.doc.feature(*id).is_some()).collect();
    if !stale.is_empty() {
        sub(s, &CommandCall { id: "timeline.delete".into(), params: json!({"features": stale}) })?;
    }
    let mut replies: Vec<Value> = Vec::with_capacity(plan.len());
    let mut steps: Vec<Value> = Vec::with_capacity(plan.len());
    let mut features: Vec<u64> = Vec::new();
    for call in &plan {
        let params = substitute(&call.params, &replies)?;
        let reply = sub(s, &CommandCall { id: call.id.clone(), params: params.clone() })
            .map_err(|e| EngineError::Other(format!("vessel step `{}`: {e}", call.id)))?;
        for key in ["sketch", "feature"] {
            if let Some(id) = reply.get(key).and_then(Value::as_u64)
                && call.id != "sketch.finish"
                && !features.contains(&id)
            {
                features.push(id);
            }
        }
        steps.push(json!({"command": call.id, "params": params, "reply": reply}));
        replies.push(reply);
    }
    // A sketch left open by a failed plan would be odd; the plan closes its own.
    s.active_sketch = None;
    // Nozzles, one at a time, each tried on a scratch copy and kept only when the kernel's
    // result measures as expected; otherwise built as separate bodies and flagged.
    let mut warnings: Vec<String> = resolved.warnings.clone();
    warnings.extend(preset_warnings);
    let mut built_nozzles = Vec::new();
    let mut extra_params = Vec::new();
    for mut g in nozzles {
        let Some(host) = resolved.station(&g.host) else { continue };
        let report = build_nozzle(s, &doc, &resolved, host, &mut g, &mut features, &mut steps);
        warnings.extend(g.warnings.iter().cloned());
        if let Some(a) = doc.attachments.iter_mut().find(|a| a.id == g.id) {
            let flange_ok = report.get("flange").is_none_or(|f| f == "built");
            a.flags.boolean_failed = report.get("built").and_then(Value::as_str) != Some("joined") || !flange_ok;
            a.flags.unverified_table = g.unverified_table;
            extra_params.extend(nozzle::parameters(&doc.id, &doc.global, a, &g));
        }
        for key in ["warning", "flange_warning"] {
            if let Some(w) = report.get(key).and_then(Value::as_str) {
                warnings.push(w.to_string());
            }
        }
        built_nozzles.push(report);
    }
    // Supports and details: one body per part, placed against the shell, never joined to it.
    let mut built_supports = Vec::new();
    for g in &supports {
        let report = build_support(s, g, &mut features, &mut steps);
        if let Some(a) = doc.attachments.iter_mut().find(|a| a.id == g.id) {
            a.flags.boolean_failed = report.get("built").and_then(Value::as_str) != Some("built");
            let preset = applied.iter().find(|p| p.id == g.id);
            a.flags.unverified_table = g.unverified_table || preset.is_some_and(|p| !p.verified.is_trusted());
            if let Some(p) = preset.filter(|p| !p.verified.is_trusted()) {
                warnings.push(format!("attachment `{}`: preset {} is {:?} in its table", g.id, p.row, p.verified));
            }
        }
        warnings.extend(g.warnings.iter().cloned());
        if let Some(w) = report.get("warning").and_then(Value::as_str) {
            warnings.push(w.to_string());
        }
        extra_params.extend(g.params.iter().cloned());
        built_supports.push(report);
    }
    warnings.extend(pending);
    // The features go into the vessel's own timeline folder: the previous one when it is still
    // there (the user may have renamed or moved it), else a new one.
    let built: Vec<u64> = features.iter().copied().filter(|id| s.doc.feature(*id).is_some()).collect();
    let new_group = |s: &mut Session, built: &[u64]| -> solidcraft_engine::Result<Option<u64>> {
        let r = sub(s, &CommandCall { id: "timeline.group".into(), params: json!({"name": group_name(&doc), "features": built}) })?;
        Ok(r.get("group").and_then(Value::as_u64))
    };
    let kept = previous.group.filter(|g| s.doc.timeline_groups.iter().any(|x| x.id == *g));
    let group = match kept {
        _ if built.is_empty() => kept,
        Some(g) => {
            // Refused when the folder would stop being one run (the user put a feature of their
            // own in it): the new features get a folder of their own.
            let moved = sub(s, &CommandCall { id: "timeline.move_to_group".into(), params: json!({"features": built, "group": g}) });
            if moved.is_ok() { Some(g) } else { new_group(s, &built)? }
        }
        None => new_group(s, &built)?,
    };
    // Every dimension as a user parameter (created or updated; nothing drives from them yet).
    let mut parameters = Vec::new();
    for p in emit::parameters(&doc.id, &resolved).into_iter().chain(extra_params) {
        let params = json!({"name": p.name, "expression": p.value, "unit": p.unit, "comment": p.comment});
        sub(s, &CommandCall { id: "parameters.change".into(), params })
            .map_err(|e| EngineError::Other(format!("vessel parameter `{}`: {e}", p.name)))?;
        parameters.push(p.name);
    }
    store(s, &doc, &Generated { features: features.clone(), group })?;
    Ok(json!({"steps": steps, "features": features, "group": group, "parameters": parameters, "nozzles": built_nozzles,
        "supports": built_supports, "presets": applied.iter().map(presets::Applied::to_json).collect::<Vec<_>>(), "warnings": warnings}))
}

/// The host body's measured volume.
fn volume(s: &mut Session, body: &str) -> solidcraft_engine::Result<f64> {
    let r = sub(s, &CommandCall { id: "inspect.measure".into(), params: json!({"bodies": [body]}) })?;
    r.get("total").and_then(|t| t.get("volume_mm3")).and_then(Value::as_f64).ok_or_else(|| EngineError::Other(format!("cannot measure `{body}`")))
}

/// Run `calls` on `s`, collecting the features they make and their steps.
fn run_calls(s: &mut Session, calls: &[CommandCall], features: &mut Vec<u64>, steps: &mut Vec<Value>) -> solidcraft_engine::Result<()> {
    let mut replies: Vec<Value> = Vec::with_capacity(calls.len());
    for call in calls {
        let params = substitute(&call.params, &replies)?;
        let reply = sub(s, &CommandCall { id: call.id.clone(), params: params.clone() })
            .map_err(|e| EngineError::Other(format!("vessel step `{}`: {e}", call.id)))?;
        for key in ["sketch", "feature"] {
            if let Some(id) = reply.get(key).and_then(Value::as_u64)
                && call.id != "sketch.finish"
                && !features.contains(&id)
            {
                features.push(id);
            }
        }
        steps.push(json!({"command": call.id, "params": params, "reply": reply}));
        replies.push(reply);
    }
    Ok(())
}

/// Build one nozzle into its host (tried on a scratch copy, kept when the host's measured
/// volume changes by the expected amount), else as a separate body, else not at all: a kernel
/// failure never fails the regeneration; then its flange (bolt holes patterned, else one by
/// one, else none; else no flange). → `{id, built: joined|separate|none, expected_mm3, measured_mm3, warning?,
/// flange?: built|without bolt holes|none, bolt_count?, flange_warning?}`.
fn build_nozzle(
    s: &mut Session,
    doc: &VesselDoc,
    r: &eval::Resolved,
    host: &eval::Station,
    g: &mut nozzle::NozzleGeom,
    features: &mut Vec<u64>,
    steps: &mut Vec<Value>,
) -> Value {
    let host_body = format!("Vessel {}", host.id);
    let joined = (|| -> std::result::Result<(Vec<u64>, Vec<Value>, Session, f64, f64), String> {
        let calls = nozzle::plan(&doc.global, r, host, g).map_err(|e| e.to_string())?;
        let expected = nozzle::expected_change(&doc.global, host, g, 96).ok_or("no reference volume")?;
        let before = volume(s, &host_body).map_err(|e| e.to_string())?;
        let mut trial = s.scratch();
        let (mut f, mut st) = (Vec::new(), Vec::new());
        run_calls(&mut trial, &calls, &mut f, &mut st).map_err(|e| e.to_string())?;
        let after = volume(&mut trial, &host_body).map_err(|e| e.to_string())?;
        let change = after - before;
        if (change - expected).abs() > nozzle::tolerance(expected, before) {
            return Err(format!("the host's volume changed by {change:.0} mm³, {expected:.0} mm³ expected"));
        }
        Ok((f, st, trial, expected, change))
    })();
    let mut out = match joined {
        Ok((f, st, trial, expected, change)) => {
            s.accept_computed(trial);
            features.extend(f.into_iter().filter(|id| !features.contains(id)).collect::<Vec<_>>());
            steps.extend(st);
            json!({"id": g.id, "built": "joined", "expected_mm3": expected, "measured_mm3": change, "manway": g.manway, "davit": g.davit})
        }
        Err(why) => {
            let mut trial = s.scratch();
            let (mut f, mut st) = (Vec::new(), Vec::new());
            match run_calls(&mut trial, &nozzle::plan_separate(g), &mut f, &mut st) {
                Ok(()) => {
                    s.accept_computed(trial);
                    features.extend(f.into_iter().filter(|id| !features.contains(id)).collect::<Vec<_>>());
                    steps.extend(st);
                    json!({"id": g.id, "built": "separate", "manway": g.manway, "davit": g.davit,
                        "warning": format!("nozzle `{}`: {why}; kept as a separate body (flags.boolean_failed)", g.id)})
                }
                Err(e) => {
                    return json!({"id": g.id, "built": "none", "warning": format!("nozzle `{}`: {why}; the separate body failed too ({e}); not built", g.id)});
                }
            }
        }
    };
    // The flange is its own body, built after the neck so a kernel failure on it never costs
    // the neck: its holes patterned, else cut one by one, else the plain ring, else none.
    if let Some(fl) = g.flange {
        let mut why = Vec::new();
        for holes in [nozzle::BoltHoles::Pattern, nozzle::BoltHoles::Each, nozzle::BoltHoles::None] {
            let mut trial = s.scratch();
            let (mut f, mut st) = (Vec::new(), Vec::new());
            match run_calls(&mut trial, &nozzle::flange_calls(&doc.global, g, holes), &mut f, &mut st) {
                Ok(()) => {
                    s.accept_computed(trial);
                    features.extend(f.into_iter().filter(|id| !features.contains(id)).collect::<Vec<_>>());
                    steps.extend(st);
                    out["flange"] = json!(if holes == nozzle::BoltHoles::None { "without bolt holes" } else { "built" });
                    if holes != nozzle::BoltHoles::None {
                        out["bolt_count"] = json!(fl.bolt_count);
                    }
                    break;
                }
                Err(e) => why.push(e.to_string()),
            }
        }
        if out.get("flange").is_none() {
            out["flange"] = json!("none");
        }
        if !why.is_empty() {
            let what = match out["flange"].as_str() {
                Some("built") => "bolt holes cut one by one",
                Some("without bolt holes") => "built without its bolt holes",
                _ => "not built",
            };
            out["flange_warning"] = json!(format!("nozzle `{}`: flange {what} ({})", g.id, why.join("; ")));
        }
    }
    out
}

/// Build a support's parts (all in one go, else part by part so one kernel failure costs only
/// that part) and check each body's measured volume. → `{id, kind, built: built|partly|none,
/// parts: [{name, what, expected_mm3, measured_mm3}], warning?}`.
fn build_support(s: &mut Session, g: &support::SupportGeom, features: &mut Vec<u64>, steps: &mut Vec<Value>) -> Value {
    let mut built: Vec<&support::Part> = Vec::new();
    let mut why: Vec<String> = Vec::new();
    let mut degraded = false;
    let mut trial = s.scratch();
    let (mut f, mut st) = (Vec::new(), Vec::new());
    let all = g.parts.iter().try_for_each(|p| run_calls(&mut trial, &p.calls, &mut f, &mut st));
    if all.is_ok() {
        s.accept_computed(trial);
        features.extend(f.into_iter().filter(|id| !features.contains(id)).collect::<Vec<_>>());
        steps.extend(st);
        built.extend(g.parts.iter());
    } else {
        for first in &g.parts {
            // The part, else its simpler fallbacks in turn.
            let mut next = Some(first);
            while let Some(p) = next {
                let mut trial = s.scratch();
                let (mut f, mut st) = (Vec::new(), Vec::new());
                match run_calls(&mut trial, &p.calls, &mut f, &mut st) {
                    Ok(()) => {
                        s.accept_computed(trial);
                        features.extend(f.into_iter().filter(|id| !features.contains(id)).collect::<Vec<_>>());
                        steps.extend(st);
                        if !std::ptr::eq(p, first) {
                            degraded = true;
                            why.push(format!("{} built as {}", p.name, p.what));
                        }
                        built.push(p);
                        break;
                    }
                    Err(e) => why.push(format!("{}: {e}", p.name)),
                }
                next = p.fallback.as_deref();
            }
        }
    }
    let mut parts = Vec::new();
    for p in &built {
        let measured = volume(s, &p.name).ok();
        if let Some(m) = measured
            && (m - p.volume).abs() > support::tolerance(p.volume)
        {
            why.push(format!("{} measures {m:.0} mm³, {:.0} mm³ expected", p.name, p.volume));
        }
        parts.push(json!({"name": p.name, "what": p.what, "expected_mm3": p.volume, "measured_mm3": measured}));
    }
    let state = match built.len() {
        n if n == g.parts.len() && !degraded => "built",
        0 => "none",
        _ => "partly",
    };
    let mut out = json!({"id": g.id, "kind": g.kind, "built": state, "parts": parts});
    if !why.is_empty() {
        out["warning"] = json!(format!("{} `{}`: {}", g.kind, g.id, why.join("; ")));
    }
    out
}
