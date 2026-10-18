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
use crate::model::VesselDoc;
use crate::{VesselError, emit, eval};

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
        "→ steps: [{command, params, reply}], features: [ids] (what it built; the features of the previous regeneration are deleted \
         first). One undo step. Refused (not supported yet) for elements other than cylinders and for attachments",
    ),
    CommandSpec::new("vessel.inspect", "Inspect Vessel", inspect).at(TAB, "INSPECT").icon("measure").noundo().doc_only().params(
        "→ doc, stations: [{id, kind, z_from, z_to, r_from, r_to, thickness}], plan: [{command, params}] | plan_error, generated: [feature ids]",
    ),
];

fn has_vessel(s: &Session) -> std::result::Result<(), String> {
    if s.doc.metadata.get(METADATA_KEY).is_some() { Ok(()) } else { Err("the design has no vessel (vessel.new first)".into()) }
}

/// The stored vessel and the features its last regeneration made.
fn load(s: &Session) -> solidcraft_engine::Result<(VesselDoc, Vec<u64>)> {
    let v = s.doc.metadata.get(METADATA_KEY).ok_or_else(|| EngineError::Other("the design has no vessel (vessel.new first)".into()))?;
    let doc = v.get("doc").ok_or_else(|| EngineError::Other(format!("`{METADATA_KEY}` holds no `doc`")))?;
    let doc = VesselDoc::from_value(doc)?;
    let generated = v
        .get("generated")
        .and_then(|g| g.get("features"))
        .and_then(Value::as_array)
        .map(|a| a.iter().take(crate::emit::MAX_STEPS).filter_map(Value::as_u64).collect())
        .unwrap_or_default();
    Ok((doc, generated))
}

/// Store the vessel (validated) and the generated feature ids in the design's metadata.
fn store(s: &mut Session, doc: &VesselDoc, generated: &[u64]) -> solidcraft_engine::Result<()> {
    doc.validate()?;
    let value = json!({"schema": crate::model::SCHEMA, "doc": doc.to_value()?, "generated": {"features": generated}});
    let mut next = s.doc.metadata.clone();
    next.set(METADATA_KEY, value).map_err(|e| EngineError::Other(format!("cannot store the vessel: {e}")))?;
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
    store(s, &doc, &[])?;
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

fn inspect(s: &mut Session, _p: &Value) -> solidcraft_engine::Result<Value> {
    let (doc, generated) = load(s)?;
    let resolved = eval::resolve(&doc);
    let (stations, plan, plan_error) = match &resolved {
        Ok(r) => {
            let stations: Vec<Value> = r
                .stations
                .iter()
                .map(|st| json!({"id": st.id, "kind": st.kind, "z_from": st.z_from, "z_to": st.z_to, "r_from": st.r_from, "r_to": st.r_to, "thickness": st.thickness}))
                .collect();
            match emit::plan(r) {
                Ok(p) => (stations, Some(p), None),
                Err(e) => (stations, None, Some(e.to_string())),
            }
        }
        Err(e) => (Vec::new(), None, Some(e.to_string())),
    };
    let plan: Option<Vec<Value>> = plan.map(|p| p.into_iter().map(|c| json!({"command": c.id, "params": c.params})).collect());
    Ok(json!({"doc": doc.to_value()?, "stations": stations, "plan": plan, "plan_error": plan_error, "generated": generated}))
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

fn regenerate(s: &mut Session, _p: &Value) -> solidcraft_engine::Result<Value> {
    let (doc, previous) = load(s)?;
    let resolved = eval::resolve(&doc)?;
    if !doc.attachments.is_empty() {
        return Err(
            VesselError::NotSupportedYet(format!("{} attachment(s): nozzles, saddles, legs… are not built yet", doc.attachments.len())).into()
        );
    }
    let plan = emit::plan(&resolved)?;
    // The last regeneration's features go first (sketches take their revolves with them).
    let stale: Vec<Value> = previous.iter().filter(|id| s.doc.feature(**id).is_some()).map(|id| json!({"type": "feature", "id": id})).collect();
    if !stale.is_empty() {
        sub(s, &CommandCall { id: "selection.delete".into(), params: json!({"items": stale}) })?;
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
    store(s, &doc, &features)?;
    Ok(json!({"steps": steps, "features": features}))
}
