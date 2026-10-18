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

use serde_json::{Value, json};
use solidcraft_engine::params::{bad, num, str_};
use solidcraft_engine::{CommandSpec, EngineError, Session};

use crate::emit::{CommandCall, substitute};
use crate::model::{AttachmentKind, VesselDoc};
use crate::{VesselError, emit, eval, nozzle};

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

fn inspect(s: &mut Session, _p: &Value) -> solidcraft_engine::Result<Value> {
    let (doc, generated) = load(s)?;
    let flags: Vec<Value> = doc
        .attachments
        .iter()
        .filter(|a| a.flags != crate::model::Flags::default())
        .map(|a| json!({"id": a.id, "boolean_failed": a.flags.boolean_failed, "unverified_table": a.flags.unverified_table}))
        .collect();
    let mut out = json!({
        "doc": doc.to_value()?, "flags": flags,
        "generated": generated.features, "group": generated.group,
        "stations": [], "plan": null, "plan_error": null, "warnings": [], "parameters": [], "nozzles": [],
    });
    match eval::resolve(&doc) {
        Ok(r) => {
            out["stations"] = r.stations.iter().map(station_json).collect();
            out["extent"] = json!({"z_min": r.z_min, "z_max": r.z_max, "tl_bottom": r.tl_bottom, "tl_top": r.tl_top, "tl_to_tl": r.tl_to_tl()});
            let emitted = emit::parameters(&doc.id, &r);
            // A parameter the user changed in the table no longer matches the document; the next
            // regenerate rewrites it (`vessel-schema.md` §6).
            let table = sub(s, &CommandCall { id: "parameters.list".into(), params: json!({"filter": "user"}) }).unwrap_or(Value::Null);
            let mut warnings = r.warnings.clone();
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
            let tables = triya_stdlib::TableSet::embedded().map_err(VesselError::from)?;
            out["nozzles"] = doc
                .attachments
                .iter()
                .filter(|a| matches!(a.kind, AttachmentKind::Nozzle { .. }))
                .map(|a| match nozzle::resolve(&doc, &r, a, &tables) {
                    Ok(g) => json!({
                        "id": g.id, "host": g.host, "origin": g.frame.origin, "dir": g.frame.dir, "s_inner": g.frame.s_inner,
                        "s_outer": g.frame.s_outer, "s_start": g.s_start, "s_face": g.s_face, "neck_od": g.neck_od, "neck_wall": g.neck_wall,
                        "pad": g.pad.map(|p| json!({"od": p.od, "thickness": p.thickness})), "manway": g.manway,
                    }),
                    Err(e) => json!({"id": a.id, "error": e.to_string()}),
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
    let tables = triya_stdlib::TableSet::embedded().map_err(VesselError::from)?;
    let resolved = eval::resolve_with(&doc, &tables)?;
    // Every nozzle is placed before anything is built: a nozzle that cannot be placed is an
    // error in the document, and the design stays as it was.
    let mut nozzles = Vec::new();
    for a in &doc.attachments {
        if matches!(a.kind, AttachmentKind::Nozzle { .. }) {
            nozzles.push(nozzle::resolve(&doc, &resolved, a, &tables)?);
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
    let mut built_nozzles = Vec::new();
    let mut extra_params = Vec::new();
    for mut g in nozzles {
        let Some(host) = resolved.station(&g.host) else { continue };
        let report = build_nozzle(s, &doc, &resolved, host, &mut g, &mut features, &mut steps);
        warnings.extend(g.warnings.iter().cloned());
        if let Some(a) = doc.attachments.iter_mut().find(|a| a.id == g.id) {
            a.flags.boolean_failed = report.get("built").and_then(Value::as_str) != Some("joined");
            a.flags.unverified_table = g.unverified_table;
            extra_params.extend(nozzle::parameters(&doc.id, &doc.global, a, &g));
        }
        if let Some(w) = report.get("warning").and_then(Value::as_str) {
            warnings.push(w.to_string());
        }
        built_nozzles.push(report);
    }
    for a in &doc.attachments {
        if !matches!(a.kind, AttachmentKind::Nozzle { .. }) {
            warnings.push(format!("attachment `{}` ({}): not built yet (supports and details come in Phase 4)", a.id, a.kind.name()));
        }
    }
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
    Ok(json!({"steps": steps, "features": features, "group": group, "parameters": parameters, "nozzles": built_nozzles, "warnings": warnings}))
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
/// failure never fails the regeneration. → `{id, built: joined|separate|none, expected_mm3,
/// measured_mm3, warning?}`.
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
    match joined {
        Ok((f, st, trial, expected, change)) => {
            s.accept_computed(trial);
            features.extend(f.into_iter().filter(|id| !features.contains(id)).collect::<Vec<_>>());
            steps.extend(st);
            json!({"id": g.id, "built": "joined", "expected_mm3": expected, "measured_mm3": change})
        }
        Err(why) => {
            let mut trial = s.scratch();
            let (mut f, mut st) = (Vec::new(), Vec::new());
            match run_calls(&mut trial, &nozzle::plan_separate(g), &mut f, &mut st) {
                Ok(()) => {
                    s.accept_computed(trial);
                    features.extend(f.into_iter().filter(|id| !features.contains(id)).collect::<Vec<_>>());
                    steps.extend(st);
                    json!({"id": g.id, "built": "separate", "warning": format!("nozzle `{}`: {why}; kept as a separate body (flags.boolean_failed)", g.id)})
                }
                Err(e) => {
                    json!({"id": g.id, "built": "none", "warning": format!("nozzle `{}`: {why}; the separate body failed too ({e}); not built", g.id)})
                }
            }
        }
    }
}
