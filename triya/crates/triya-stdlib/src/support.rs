// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Support and detail presets: rows keyed by attachment kind and `size_key`, each holding the
//! attachment fields it sets (`docs/standards-library.md` §2–3).
//!
//! The shipped table (`data/support-presets.json`) holds Triya's own proportional rules keyed by
//! the vessel's outside diameter (`OD1600`); a key of that form that has no row takes the
//! nearest smaller OD of the same kind. A user's private table (EIL standard sheets, a company
//! standard) uses the same format with its own keys (`7-12-0024/LL-25`) and is searched first.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Verified;

/// One preset: the fields it sets on an attachment of `kind` whose `size_key` matches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SupportRow {
    /// The attachment kind as the vessel JSON writes it (`saddle`, `leg`, `lifting_lug`, …).
    pub kind: String,
    pub size_key: String,
    /// Attachment fields, as in the vessel JSON (`{"height_mm": 800, "base_plate": {…}}`); an
    /// object replaces the attachment's object of that name whole.
    pub fields: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub source: Vec<String>,
    #[serde(default)]
    pub verified: Verified,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Most fields one row may set.
pub const MAX_FIELDS: usize = 64;

impl SupportRow {
    pub fn validate(&self) -> crate::Result<()> {
        let bad = |m: &str| Err(crate::StdlibError::Invalid(format!("support preset `{}`/`{}`: {m}", self.kind, self.size_key)));
        if self.kind.trim().is_empty() || self.kind.len() > 64 {
            return bad("`kind` must be an attachment kind");
        }
        if self.size_key.trim().is_empty() || self.size_key.len() > 128 {
            return bad("`size_key` must be 1 … 128 characters");
        }
        if self.fields.is_empty() || self.fields.len() > MAX_FIELDS {
            return bad("`fields` must set 1 … 64 fields");
        }
        for k in ["id", "kind", "host", "size_key", "standard_ref", "flags"] {
            if self.fields.contains_key(k) {
                return bad(&format!("`fields` may not set `{k}`"));
            }
        }
        Ok(())
    }

    pub fn size_key(&self) -> String {
        format!("{}/{}", self.kind, self.size_key)
    }
}

/// The outside diameter of an `OD<mm>` key (`"OD1600"`, `"od 1600"`).
pub fn od_key(key: &str) -> Option<f64> {
    let k = key.trim();
    let rest = k.get(..2).filter(|p| p.eq_ignore_ascii_case("od")).and_then(|_| k.get(2..))?;
    rest.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0)
}
