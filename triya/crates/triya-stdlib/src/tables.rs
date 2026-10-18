// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! JSON tables and the set the modeller looks rows up in.
//!
//! Every table file is `{"source": …, "note"?, "standard"?: StandardRef, "rows": […]}`. The
//! embedded files under `data/` are illustrative (their `source` says so); a real deployment
//! loads verified tables with [`TableSet::from_json`] and the embedded ones are only the
//! fallback that keeps the loader and the callers testable.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{FlangeKind, FlangeRow, HeadPreset, PipeScheduleRow, Result, StandardRef, StdlibError};

/// The marker every illustrative table carries in `source`.
pub const ILLUSTRATIVE: &str = "ILLUSTRATIVE";

/// Longest table text the loader accepts (input files are hostile).
pub const MAX_TABLE_BYTES: usize = 8 << 20;
/// Most rows one table may hold.
pub const MAX_ROWS: usize = 100_000;

const FLANGES_JSON: &str = include_str!("../data/flanges.json");
const PIPE_SCHEDULES_JSON: &str = include_str!("../data/pipe_schedules.json");
const HEAD_PRESETS_JSON: &str = include_str!("../data/head_presets.json");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table<R> {
    /// Where the values come from. Illustrative tables say `ILLUSTRATIVE — replace with verified values`.
    pub source: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub standard: Option<StandardRef>,
    pub rows: Vec<R>,
}

impl<R> Table<R> {
    /// The table's values are placeholders, not verified standard data.
    pub fn is_illustrative(&self) -> bool {
        self.source.contains(ILLUSTRATIVE)
    }
}

/// A named head preset with the standard it comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeadPresetRow {
    pub name: String,
    #[serde(default)]
    pub standard: Option<StandardRef>,
    pub preset: HeadPreset,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableSet {
    pub flanges: Table<FlangeRow>,
    pub pipe_schedules: Table<PipeScheduleRow>,
    pub head_presets: Table<HeadPresetRow>,
}

fn parse_table<R: DeserializeOwned>(name: &str, text: &str) -> Result<Table<R>> {
    if text.len() > MAX_TABLE_BYTES {
        return Err(StdlibError::Table { table: name.into(), msg: format!("larger than {} MiB", MAX_TABLE_BYTES >> 20) });
    }
    let t: Table<R> = serde_json::from_str(text).map_err(|e| StdlibError::Table { table: name.into(), msg: e.to_string() })?;
    if t.rows.len() > MAX_ROWS {
        return Err(StdlibError::Table { table: name.into(), msg: format!("more than {MAX_ROWS} rows") });
    }
    Ok(t)
}

impl TableSet {
    /// The illustrative tables compiled into the crate.
    pub fn embedded() -> Result<TableSet> {
        TableSet::from_json(FLANGES_JSON, PIPE_SCHEDULES_JSON, HEAD_PRESETS_JSON)
    }

    /// Tables from their JSON texts; every row is validated.
    pub fn from_json(flanges: &str, pipe_schedules: &str, head_presets: &str) -> Result<TableSet> {
        let set = TableSet {
            flanges: parse_table("flanges", flanges)?,
            pipe_schedules: parse_table("pipe_schedules", pipe_schedules)?,
            head_presets: parse_table("head_presets", head_presets)?,
        };
        for (i, r) in set.flanges.rows.iter().enumerate() {
            r.validate().map_err(|e| StdlibError::Table { table: "flanges".into(), msg: format!("row {i}: {e}") })?;
        }
        for (i, r) in set.pipe_schedules.rows.iter().enumerate() {
            r.validate().map_err(|e| StdlibError::Table { table: "pipe_schedules".into(), msg: format!("row {i}: {e}") })?;
        }
        Ok(set)
    }

    /// Any table holds placeholder values.
    pub fn is_illustrative(&self) -> bool {
        self.flanges.is_illustrative() || self.pipe_schedules.is_illustrative() || self.head_presets.is_illustrative()
    }

    pub fn flange(&self, nps: &str, class: &str, kind: FlangeKind) -> Option<&FlangeRow> {
        self.flanges.rows.iter().find(|r| r.kind == kind && r.nps.eq_ignore_ascii_case(nps.trim()) && r.class.eq_ignore_ascii_case(class.trim()))
    }

    /// A flange by its `<nps>/<class>/<kind>` key.
    pub fn flange_by_key(&self, key: &str) -> Option<&FlangeRow> {
        let mut parts = key.split('/');
        let (nps, class, kind) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() {
            return None;
        }
        self.flange(nps, class, FlangeKind::parse(kind)?)
    }

    pub fn pipe(&self, nps: &str, schedule: &str) -> Option<&PipeScheduleRow> {
        self.pipe_schedules.rows.iter().find(|r| r.nps.eq_ignore_ascii_case(nps.trim()) && r.schedule.eq_ignore_ascii_case(schedule.trim()))
    }

    pub fn head_preset(&self, name: &str) -> Option<&HeadPresetRow> {
        self.head_presets.rows.iter().find(|r| r.name.eq_ignore_ascii_case(name.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_tables_load_and_are_marked_illustrative() {
        let t = TableSet::embedded().unwrap();
        assert!(t.is_illustrative());
        assert!(t.flanges.source.starts_with("ILLUSTRATIVE"));
        assert_eq!(t.flanges.rows.len(), 3);
        assert_eq!(t.pipe_schedules.rows.len(), 2);
        assert_eq!(t.head_presets.rows.len(), 5);
    }

    #[test]
    fn lookups_find_rows_by_key() {
        let t = TableSet::embedded().unwrap();
        let f = t.flange("2", "150", FlangeKind::Wn).unwrap();
        assert_eq!(f.size_key(), "2/150/wn");
        assert_eq!(t.flange_by_key("2/150/blind").map(|r| r.kind), Some(FlangeKind::Blind));
        assert!(t.flange_by_key("2/150").is_none());
        assert!(t.flange_by_key("2/150/wn/extra").is_none());
        assert!(t.flange_by_key("2/150/nope").is_none());
        let p = t.pipe("2", "std").unwrap();
        assert!((p.id() - (60.0 - 7.8)).abs() < 1e-9);
        assert_eq!(t.head_preset("Klopper").map(|r| r.preset), Some(HeadPreset::KLOPPER));
        assert_eq!(t.head_preset("asme_fd").map(|r| r.preset), Some(HeadPreset::ASME_FD));
    }

    #[test]
    fn bad_tables_are_errors() {
        assert!(TableSet::from_json("nope", "{}", "{}").is_err());
        let bad_row = r#"{"source":"x","rows":[{"nps":"2","dn":50,"od":60,"schedule":"STD","wall":40}]}"#;
        assert!(TableSet::from_json(FLANGES_JSON, bad_row, HEAD_PRESETS_JSON).is_err());
        let nan_free = r#"{"source":"x","rows":[]}"#;
        assert!(TableSet::from_json(nan_free, nan_free, nan_free).is_ok());
    }
}
