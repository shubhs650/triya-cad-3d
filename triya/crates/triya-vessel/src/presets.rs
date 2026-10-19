// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Standard references and presets (T4.4, `docs/standards-library.md` §2–3).
//!
//! An attachment with a `size_key` takes the fields of the matching support-preset row: a
//! user's private table first (imported with `vessel.import_tables` and kept in the design's
//! metadata under [`TABLES_KEY`]), then the shipped proportional presets (`OD<mm>` keys). The
//! row's fields replace the attachment's own, field by field (an object field whole), at every
//! regeneration, so importing a table changes what is built. A key no table has leaves the
//! attachment's own dimensions, with a warning when it names a standard. The repository ships
//! no company standard: EIL and other private sheets reach a design only through the import.

use serde_json::{Map, Value, json};
use solidcraft_engine::Session;
use triya_stdlib::TableSet;

use crate::model::{Attachment, AttachmentKind, VesselDoc};
use crate::{Result, VesselError};

/// The reverse-DNS metadata key the private tables are kept under:
/// `{"flanges"?: <flange table>, "supports"?: <support-preset table>}`.
pub const TABLES_KEY: &str = "in.openengtools.triya.tables";

/// Largest private-table entry a design keeps (upstream allows 64 KiB a metadata value).
pub const MAX_TABLES_BYTES: usize = 60 * 1024;

/// What one attachment took from a preset.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub id: String,
    /// `kind/size_key` of the row.
    pub row: String,
    /// The row came from the design's private table.
    pub private: bool,
    pub verified: triya_stdlib::Verified,
    pub fields: Vec<String>,
}

impl Applied {
    pub fn to_json(&self) -> Value {
        json!({"id": self.id, "row": self.row, "private": self.private, "verified": self.verified, "fields": self.fields})
    }
}

/// The shipped tables plus the design's private ones.
pub fn tables(s: &Session) -> Result<TableSet> {
    let set = TableSet::embedded()?;
    match s.doc.metadata.get(TABLES_KEY) {
        Some(v) => with_private(set, v),
        None => Ok(set),
    }
}

/// `set` with the private tables of a [`TABLES_KEY`] value searched first.
pub fn with_private(mut set: TableSet, v: &Value) -> Result<TableSet> {
    let o = v.as_object().ok_or_else(|| VesselError::Invalid("the private tables must be an object".into()))?;
    for k in o.keys() {
        if k != "flanges" && k != "supports" {
            return Err(VesselError::Invalid(format!("the private tables hold `flanges` and `supports`, not `{k}`")));
        }
    }
    let text = |t: &Value| serde_json::to_string(t).map_err(|e| VesselError::Invalid(e.to_string()));
    if let Some(t) = o.get("flanges") {
        set = set.with_private(&text(t)?)?;
    }
    if let Some(t) = o.get("supports") {
        set = set.with_private_supports(&text(t)?)?;
    }
    Ok(set)
}

/// Apply the presets to every support and detail of `doc` that has a `size_key`; returns what
/// was applied and warnings for keys that matched no row.
pub fn apply(doc: &mut VesselDoc, tables: &TableSet, private: usize) -> Result<(Vec<Applied>, Vec<String>)> {
    let mut applied = Vec::new();
    let mut warnings = Vec::new();
    for a in doc.attachments.iter_mut() {
        if matches!(a.kind, AttachmentKind::Nozzle { .. }) {
            continue;
        }
        let Some(key) = a.size_key.clone() else { continue };
        let kind = a.kind.name();
        let Some(row) = tables.support(kind, &key) else {
            if let Some(r) = &a.standard_ref {
                warnings.push(format!("attachment `{}`: no preset `{key}` for {r} in the tables (import it); its own dimensions are used", a.id));
            }
            continue;
        };
        let is_private = tables.supports.iter().take(private).any(|t| t.rows.iter().any(|r| std::ptr::eq(r, row)));
        let merged = merge(a, &row.fields)?;
        *a = merged;
        applied.push(Applied {
            id: a.id.clone(),
            row: row.size_key(),
            private: is_private,
            verified: row.verified,
            fields: row.fields.keys().cloned().collect(),
        });
    }
    Ok((applied, warnings))
}

/// `a` with `fields` laid over it (each field replaced whole).
fn merge(a: &Attachment, fields: &Map<String, Value>) -> Result<Attachment> {
    let mut v = serde_json::to_value(a).map_err(|e| VesselError::Invalid(e.to_string()))?;
    let o = v.as_object_mut().ok_or_else(|| VesselError::Invalid("an attachment is an object".into()))?;
    for (k, x) in fields {
        o.insert(k.clone(), x.clone());
    }
    serde_json::from_value(v)
        .map_err(|e| VesselError::Invalid(format!("attachment `{}`: the preset {} does not fit: {e}", a.id, a.size_key.as_deref().unwrap_or(""))))
}

/// How many support tables of a [`TABLES_KEY`] value come before the shipped ones.
pub fn private_support_tables(s: &Session) -> usize {
    usize::from(s.doc.metadata.get(TABLES_KEY).and_then(|v| v.get("supports")).is_some())
}
