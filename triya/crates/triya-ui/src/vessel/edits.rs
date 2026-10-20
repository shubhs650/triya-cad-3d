// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! What each edit in the vessel panels sends: always a `vessel.*` command, and for a change to
//! the document a `vessel.set {patch}` with RFC 6902 operations on JSON pointers into the stored
//! document. Pure functions of the document, so a test can check that an edit in the panel is
//! exactly the JSON a script or an MCP client would send.

use serde_json::{Value, json};
use triya_vessel::eval::Resolved;
use triya_vessel::model::{AxialRef, ElementKind, VesselDoc};
use triya_vessel::patch::escape;

/// One command the panel runs through `app.run`. Only `vessel.*` commands are ever made here.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub id: &'static str,
    pub params: Value,
}

impl Action {
    fn set(ops: Vec<Value>) -> Action {
        Action { id: "vessel.set", params: json!({ "patch": ops }) }
    }
}

/// A JSON pointer from plain segments.
pub fn pointer(segments: &[&str]) -> String {
    segments.iter().map(|s| format!("/{}", escape(s))).collect()
}

fn element_index(doc: &VesselDoc, id: &str) -> Option<usize> {
    doc.elements.iter().position(|e| e.id == id)
}

fn attachment_index(doc: &VesselDoc, id: &str) -> Option<usize> {
    doc.attachments.iter().position(|a| a.id == id)
}

/// Set (or, with `None`, clear) a field of an element: `field` may be nested (`["base_ring",
/// "od_mm"]`).
pub fn element_field(doc: &VesselDoc, id: &str, field: &[&str], value: Option<Value>) -> Option<Action> {
    let i = element_index(doc, id)?.to_string();
    let mut segs = vec!["elements", i.as_str()];
    segs.extend_from_slice(field);
    let path = pointer(&segs);
    Some(Action::set(vec![match value {
        Some(v) => json!({"op": "add", "path": path, "value": v}),
        None => json!({"op": "remove", "path": path}),
    }]))
}

/// Set (or clear) a field of an attachment.
pub fn attachment_field(doc: &VesselDoc, id: &str, field: &[&str], value: Option<Value>) -> Option<Action> {
    let j = attachment_index(doc, id)?.to_string();
    let mut segs = vec!["attachments", j.as_str()];
    segs.extend_from_slice(field);
    let path = pointer(&segs);
    Some(Action::set(vec![match value {
        Some(v) => json!({"op": "add", "path": path, "value": v}),
        None => json!({"op": "remove", "path": path}),
    }]))
}

/// Set one field of `global`.
pub fn global_field(field: &str, value: Value) -> Action {
    Action::set(vec![json!({"op": "add", "path": pointer(&["global", field]), "value": value})])
}

/// Move element `id` one place up (`-1`) or down (`+1`) the stack.
pub fn move_element(doc: &VesselDoc, r: Option<&Resolved>, id: &str, by: isize) -> Option<Action> {
    let i = element_index(doc, id)?;
    let to = i.checked_add_signed(by).filter(|t| *t < doc.elements.len())?;
    let mut ops = vec![json!({"op": "move", "from": pointer(&["elements", &i.to_string()]), "path": pointer(&["elements", &to.to_string()])})];
    // The new bottom element has nothing below to inherit from: it keeps the diameter it has now.
    let bottom = if to == 0 {
        doc.elements.get(i)
    } else if i == 0 {
        doc.elements.get(1)
    } else {
        None
    };
    if let Some(e) = bottom
        && e.diameter_mm.is_none()
        && let Some(st) = r.and_then(|r| r.station(&e.id))
    {
        ops.push(json!({"op": "add", "path": pointer(&["elements", "0", "diameter_mm"]), "value": st.diameter_mm}));
    }
    Some(Action::set(ops))
}

/// Delete element `id` and the attachments that sit on it (one patch, one undo step).
pub fn delete_element(doc: &VesselDoc, r: Option<&Resolved>, id: &str) -> Option<Action> {
    let i = element_index(doc, id)?;
    let mut ops: Vec<Value> = doc
        .attachments
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, a)| a.host == id)
        .map(|(j, _)| json!({"op": "remove", "path": pointer(&["attachments", &j.to_string()])}))
        .collect();
    ops.push(json!({"op": "remove", "path": pointer(&["elements", &i.to_string()])}));
    // The element that becomes the bottom keeps the diameter it inherited from this one.
    if i == 0
        && let Some(next) = doc.elements.get(1)
        && next.diameter_mm.is_none()
        && let Some(st) = r.and_then(|r| r.station(&next.id))
    {
        ops.push(json!({"op": "add", "path": pointer(&["elements", "0", "diameter_mm"]), "value": st.diameter_mm}));
    }
    Some(Action::set(ops))
}

/// Delete attachment `id`.
pub fn delete_attachment(doc: &VesselDoc, id: &str) -> Option<Action> {
    let j = attachment_index(doc, id)?;
    Some(Action::set(vec![json!({"op": "remove", "path": pointer(&["attachments", &j.to_string()])})]))
}

/// Change element `id` to another kind: the id, wall, diameter, material and description stay;
/// the new kind's fields take their defaults (what `vessel.element.add` would give it there).
pub fn change_kind(doc: &VesselDoc, r: Option<&Resolved>, id: &str, kind: &str) -> Option<Action> {
    let i = element_index(doc, id)?;
    let e = doc.elements.get(i)?;
    if e.kind.name() == kind {
        return None;
    }
    let near = r.and_then(|r| r.station(id));
    let mut m = triya_vessel::edit::element_defaults("vessel.set", kind, near).ok()?;
    m.insert("id".into(), json!(e.id));
    m.insert("thickness_mm".into(), json!(e.thickness_mm));
    if let Some(d) = e.diameter_mm {
        m.insert("diameter_mm".into(), json!(d));
    }
    if let Some(x) = &e.material {
        m.insert("material".into(), json!(x));
    }
    if let Some(x) = &e.description {
        m.insert("description".into(), json!(x));
    }
    if kind == "body_flange" {
        // A table row to start from: the shell's flange at Cl 150.
        let nps = (near.map_or(1000.0, |s| s.diameter_mm) / 25.4).round();
        let standard = if nps >= 26.0 { "B16.47A" } else { "B16.5" };
        m.insert("flange_ref".into(), json!({"standard": standard, "nps_or_dn": fmt_num(nps), "class": 150, "type": "wn"}));
    }
    Some(Action::set(vec![json!({"op": "replace", "path": pointer(&["elements", &i.to_string()]), "value": Value::Object(m)})]))
}

/// Insert a new element of `kind` after `after` (or at the default place).
pub fn insert_element(kind: &str, after: Option<&str>) -> Action {
    let mut p = json!({ "kind": kind });
    if let Some(a) = after {
        p["after"] = json!(a);
    }
    Action { id: "vessel.element.add", params: p }
}

/// Add an attachment of `kind` on `host` (`None`: the command's default host).
pub fn add_attachment(kind: &str, host: Option<&str>) -> Action {
    let mut a = json!({ "kind": kind });
    if let Some(h) = host {
        a["host"] = json!(h);
    }
    Action { id: "vessel.attachment.add", params: json!({ "attachment": a }) }
}

/// Change an attachment's axial reference, rewriting `mm` so it stays where it is.
pub fn axial_ref(doc: &VesselDoc, r: &Resolved, id: &str, to: AxialRef) -> Option<Action> {
    let j = attachment_index(doc, id)?;
    let a = doc.attachments.get(j)?;
    if a.axial.reference == to {
        return None;
    }
    let host = r.station(&a.host)?;
    let mm = match to {
        AxialRef::Datum => a.axial.mm + host.z_from,
        AxialRef::HostFrom => a.axial.mm - host.z_from,
    };
    let js = j.to_string();
    Some(Action::set(vec![json!({"op": "add", "path": pointer(&["attachments", &js, "axial"]), "value": {"ref": to, "mm": round3(mm)}})]))
}

/// Move an attachment to another host, keeping its place: a datum-referenced elevation stays;
/// one measured from its host is re-measured from the new host's start.
pub fn rehost(doc: &VesselDoc, r: &Resolved, id: &str, host: &str) -> Option<Action> {
    let j = attachment_index(doc, id)?;
    let a = doc.attachments.get(j)?;
    if a.host == host || doc.element(host).is_none() {
        return None;
    }
    let js = j.to_string();
    let mut ops = vec![json!({"op": "add", "path": pointer(&["attachments", &js, "host"]), "value": host})];
    if a.axial.reference == AxialRef::HostFrom
        && let (Some(old), Some(new)) = (r.station(&a.host), r.station(host))
    {
        ops.push(json!({"op": "add", "path": pointer(&["attachments", &js, "axial", "mm"]), "value": round3(a.axial.mm + old.z_from - new.z_from)}));
    }
    Some(Action::set(ops))
}

/// The angle step an attachment snaps to: 22.5° on a shell, 5° on a head.
pub fn snap_step(doc: &VesselDoc, host: &str) -> f64 {
    match doc.element(host).map(|e| &e.kind) {
        Some(ElementKind::Head { .. } | ElementKind::FlatCover { .. }) => 5.0,
        _ => 22.5,
    }
}

/// `deg` snapped to `step` (when > 0), in [0, 360).
pub fn snap(deg: f64, step: f64) -> f64 {
    let a = if step > 0.0 { (deg / step).round() * step } else { deg };
    let a = a.rem_euclid(360.0);
    if (a - 360.0).abs() < 1e-9 { 0.0 } else { round3(a) }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// A number typed in a field: `1200`, `1200 mm`, `1,200.5`; `None` for anything else.
pub fn parse_mm(text: &str) -> Option<f64> {
    let t = text.trim().trim_end_matches("mm").trim_end_matches('°').trim_end_matches("deg").trim().replace(',', "");
    t.parse::<f64>().ok().filter(|x| x.is_finite())
}

/// A number for display: no trailing zeros, at most three decimals.
pub fn fmt_num(x: f64) -> String {
    let s = format!("{:.3}", round3(x));
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_c() -> VesselDoc {
        VesselDoc::from_json(include_str!("../../../triya-vessel/tests/reference/vessel-c.json")).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn edits_are_patches_on_json_pointers() {
        let doc = doc_c();
        let a = element_field(&doc, "E2", &["length_mm"], Some(json!(2600.0))).unwrap();
        assert_eq!(a, Action { id: "vessel.set", params: json!({"patch": [{"op": "add", "path": "/elements/1/length_mm", "value": 2600.0}]}) });
        let a = element_field(&doc, "E1", &["diameter_mm"], None).unwrap();
        assert_eq!(a.params["patch"][0], json!({"op": "remove", "path": "/elements/0/diameter_mm"}));
        let a = move_element(&doc, None, "E2", 1).unwrap();
        assert_eq!(a.params["patch"][0], json!({"op": "move", "from": "/elements/1", "path": "/elements/2"}));
        assert!(move_element(&doc, None, "E3", 1).is_none());
        assert!(move_element(&doc, None, "E1", -1).is_none());
        let a = delete_element(&doc, None, "E2").unwrap();
        let ops = a.params["patch"].as_array().unwrap();
        // N1, N2, B1, I1, CL1 sit on E2: removed last first, then the element.
        assert_eq!(ops.len(), 6);
        assert_eq!(ops[0]["path"], "/attachments/6");
        assert_eq!(ops[5]["path"], "/elements/1");
        assert_eq!(pointer(&["global", "a/b~c"]), "/global/a~1b~0c");
        assert_eq!(snap(47.0, 22.5), 45.0);
        assert_eq!(snap(359.0, 22.5), 0.0);
        assert_eq!(snap(-3.0, 5.0), 355.0);
        assert_eq!(snap_step(&doc, "E3"), 5.0);
        assert_eq!(snap_step(&doc, "E2"), 22.5);
        assert_eq!(parse_mm(" 1,200.5 mm"), Some(1200.5));
        assert_eq!(parse_mm("abc"), None);
        assert_eq!(fmt_num(2400.0), "2400");
        assert_eq!(fmt_num(12.3456), "12.346");
    }

    #[test]
    fn a_new_bottom_element_keeps_its_diameter() {
        let doc = doc_c();
        let r = triya_vessel::eval::resolve(&doc).unwrap();
        let d = r.station("E2").unwrap().diameter_mm;
        assert!(doc.element("E2").unwrap().diameter_mm.is_none(), "E2 inherits");
        // Moving E2 to the bottom, or E1 up past it, or deleting E1: E2 gets E1's diameter.
        let want = json!({"op": "add", "path": "/elements/0/diameter_mm", "value": d});
        assert_eq!(move_element(&doc, Some(&r), "E2", -1).unwrap().params["patch"][1], want);
        assert_eq!(move_element(&doc, Some(&r), "E1", 1).unwrap().params["patch"][1], want);
        let ops = delete_element(&doc, Some(&r), "E1").unwrap().params["patch"].as_array().cloned().unwrap();
        assert_eq!(ops.last(), Some(&want));
        // Elsewhere nothing is added.
        assert_eq!(move_element(&doc, Some(&r), "E3", -1).unwrap().params["patch"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn axial_reference_changes_keep_the_place() {
        let doc = doc_c();
        let r = triya_vessel::eval::resolve(&doc).unwrap();
        // N1 is 300 from the datum on E2, whose start is the straight flange's top (25).
        let a = axial_ref(&doc, &r, "N1", AxialRef::HostFrom).unwrap();
        assert_eq!(a.params["patch"][0]["value"], json!({"ref": "host_from", "mm": 275.0}));
        let a = rehost(&doc, &r, "I1", "E1").unwrap();
        // I1: 1200 from E2's start → from E1's start (the hemisphere's crown, −450 below the TL).
        let e1 = r.station("E1").unwrap().z_from;
        let e2 = r.station("E2").unwrap().z_from;
        assert_eq!(a.params["patch"][1]["value"], json!(round3(1200.0 + e2 - e1)));
        let k = change_kind(&doc, Some(&r), "E2", "cone").unwrap();
        assert_eq!(k.params["patch"][0]["value"]["kind"], "cone");
        assert_eq!(k.params["patch"][0]["value"]["thickness_mm"], 8.0);
    }
}
