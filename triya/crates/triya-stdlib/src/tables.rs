// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! JSON tables and the set the modeller looks rows up in.
//!
//! Every table file is `{"source": …, "note"?, "standard"?: StandardRef, "rows": […]}`; every row
//! carries its own `source` list and `verified` grade. The set holds any number of flange and
//! pipe tables (one per standard: `asme-b16.5.json`, `asme-b36.10.json`, …); a lookup can name
//! the standard (`"B16.5"`) or take the first table that has the size. A private table loaded
//! with [`TableSet::with_private`] is searched first, so it overrides the shipped rows.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{FlangeKind, FlangeRow, HeadPreset, PipeScheduleRow, Result, StandardRef, StdlibError, Verified, canonical_nps};

/// The marker a placeholder table carries in `source`.
pub const ILLUSTRATIVE: &str = "ILLUSTRATIVE";

/// Longest table text the loader accepts (input files are hostile).
pub const MAX_TABLE_BYTES: usize = 8 << 20;
/// Most rows one table may hold.
pub const MAX_ROWS: usize = 100_000;
/// Most tables of one kind a set may hold.
pub const MAX_TABLES: usize = 64;

const B16_5_JSON: &str = include_str!("../data/asme-b16.5.json");
const B16_47A_JSON: &str = include_str!("../data/asme-b16.47a.json");
const B16_47B_JSON: &str = include_str!("../data/asme-b16.47b.json");
const B36_10_JSON: &str = include_str!("../data/asme-b36.10.json");
const HEAD_PRESETS_JSON: &str = include_str!("../data/head_presets.json");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table<R> {
    /// Where the values come from, for the whole table.
    pub source: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub standard: Option<StandardRef>,
    pub rows: Vec<R>,
}

impl<R> Table<R> {
    /// The table's values are placeholders, not standard data.
    pub fn is_illustrative(&self) -> bool {
        self.source.contains(ILLUSTRATIVE)
    }

    /// The table is for `standard` (`"B16.5"`, `"ASME B16.5"`, `"B16.47A"`; case and spaces ignored).
    pub fn is_for(&self, standard: &str) -> bool {
        let want = squash(standard);
        let Some(s) = &self.standard else { return false };
        let number = squash(&s.number);
        !want.is_empty() && (want == number || want == format!("{}{}", squash(&s.org), number))
    }
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '-' && *c != '_').flat_map(char::to_lowercase).collect()
}

/// A named head preset with the standard it comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeadPresetRow {
    pub name: String,
    #[serde(default)]
    pub standard: Option<StandardRef>,
    pub preset: HeadPreset,
}

/// A row's grade and sources are complete: at least one source, and two distinct ones for
/// `two-source`; placeholders are refused.
fn check_provenance(source: &[String], verified: Verified) -> std::result::Result<(), String> {
    if source.iter().all(|s| s.trim().is_empty()) {
        return Err("has no `source`".into());
    }
    if verified == Verified::Illustrative {
        return Err("is a placeholder (`illustrative`)".into());
    }
    if verified == Verified::TwoSource {
        let mut distinct: Vec<&str> = source.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() < 2 {
            return Err("is marked `two-source` but lists fewer than two distinct sources".into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableSet {
    pub flanges: Vec<Table<FlangeRow>>,
    pub pipe_schedules: Vec<Table<PipeScheduleRow>>,
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

fn table_name<R>(t: &Table<R>, fallback: &str) -> String {
    t.standard.as_ref().map(|s| s.to_string()).unwrap_or_else(|| fallback.to_string())
}

/// A flange table, every row validated; `strict` also demands complete provenance (shipped tables).
pub fn parse_flange_table(text: &str, strict: bool) -> Result<Table<FlangeRow>> {
    let mut t: Table<FlangeRow> = parse_table("flanges", text)?;
    let name = table_name(&t, "flanges");
    for (i, r) in t.rows.iter_mut().enumerate() {
        r.nps = canonical_nps(&r.nps);
        let at = |msg: String| StdlibError::Table { table: name.clone(), msg: format!("row {i} ({}): {msg}", r.size_key()) };
        r.validate().map_err(|e| at(e.to_string()))?;
        if strict {
            check_provenance(&r.source, r.verified).map_err(at)?;
        }
    }
    Ok(t)
}

/// A pipe-schedule table, every row validated; `strict` also demands complete provenance.
pub fn parse_pipe_table(text: &str, strict: bool) -> Result<Table<PipeScheduleRow>> {
    let mut t: Table<PipeScheduleRow> = parse_table("pipe_schedules", text)?;
    let name = table_name(&t, "pipe_schedules");
    for (i, r) in t.rows.iter_mut().enumerate() {
        r.nps = canonical_nps(&r.nps);
        let at = |msg: String| StdlibError::Table { table: name.clone(), msg: format!("row {i} ({}): {msg}", r.size_key()) };
        r.validate().map_err(|e| at(e.to_string()))?;
        if strict {
            check_provenance(&r.source, r.verified).map_err(at)?;
        }
    }
    Ok(t)
}

impl TableSet {
    /// The tables compiled into the crate (ASME B16.5, B16.47 Series A and B, B36.10M, head presets).
    pub fn embedded() -> Result<TableSet> {
        Ok(TableSet {
            flanges: vec![parse_flange_table(B16_5_JSON, true)?, parse_flange_table(B16_47A_JSON, true)?, parse_flange_table(B16_47B_JSON, true)?],
            pipe_schedules: vec![parse_pipe_table(B36_10_JSON, true)?],
            head_presets: parse_table("head_presets", HEAD_PRESETS_JSON)?,
        })
    }

    /// Tables from their JSON texts; every row is validated and must carry its provenance.
    pub fn from_json(flanges: &[&str], pipe_schedules: &[&str], head_presets: &str) -> Result<TableSet> {
        if flanges.len() > MAX_TABLES || pipe_schedules.len() > MAX_TABLES {
            return Err(StdlibError::Invalid(format!("more than {MAX_TABLES} tables of one kind")));
        }
        Ok(TableSet {
            flanges: flanges.iter().map(|t| parse_flange_table(t, true)).collect::<Result<_>>()?,
            pipe_schedules: pipe_schedules.iter().map(|t| parse_pipe_table(t, true)).collect::<Result<_>>()?,
            head_presets: parse_table("head_presets", head_presets)?,
        })
    }

    /// This set with a user's private flange table searched before the shipped ones. Rows that
    /// do not say how they were verified count as `unverified` (the UI flags them).
    pub fn with_private(mut self, flanges: &str) -> Result<TableSet> {
        if self.flanges.len() >= MAX_TABLES {
            return Err(StdlibError::Invalid(format!("more than {MAX_TABLES} flange tables")));
        }
        self.flanges.insert(0, parse_flange_table(flanges, false)?);
        Ok(self)
    }

    /// Any table holds placeholder values.
    pub fn is_illustrative(&self) -> bool {
        self.flanges.iter().any(Table::is_illustrative)
            || self.pipe_schedules.iter().any(Table::is_illustrative)
            || self.head_presets.is_illustrative()
    }

    /// Every flange row, in search order.
    pub fn flange_rows(&self) -> impl Iterator<Item = &FlangeRow> {
        self.flanges.iter().flat_map(|t| t.rows.iter())
    }

    /// Every pipe row, in search order.
    pub fn pipe_rows(&self) -> impl Iterator<Item = &PipeScheduleRow> {
        self.pipe_schedules.iter().flat_map(|t| t.rows.iter())
    }

    /// The first flange of that size in any table.
    pub fn flange(&self, nps: &str, class: &str, kind: FlangeKind) -> Option<&FlangeRow> {
        let nps = canonical_nps(nps);
        self.flange_rows().find(|r| r.kind == kind && r.nps == nps && r.class.eq_ignore_ascii_case(class.trim()))
    }

    /// A flange from the tables for `standard` (`"B16.5"`); an empty standard searches all.
    pub fn flange_in(&self, standard: &str, nps: &str, class: &str, kind: FlangeKind) -> Option<&FlangeRow> {
        if standard.trim().is_empty() {
            return self.flange(nps, class, kind);
        }
        let nps = canonical_nps(nps);
        self.flanges
            .iter()
            .filter(|t| t.is_for(standard))
            .flat_map(|t| t.rows.iter())
            .find(|r| r.kind == kind && r.nps == nps && r.class.eq_ignore_ascii_case(class.trim()))
    }

    /// A flange by its `<nps>/<class>/<kind>` key.
    pub fn flange_by_key(&self, key: &str) -> Option<&FlangeRow> {
        let mut parts = key.split('/');
        let (nps, class, kind) = (parts.next()?, parts.next()?, parts.next()?);
        // NPS fractions contain a slash themselves: "1-1/4/150/wn".
        let (nps, class, kind) = match parts.next() {
            None => (nps.to_string(), class, kind),
            Some(k) if parts.next().is_none() => (format!("{nps}/{class}"), kind, k),
            Some(_) => return None,
        };
        self.flange(&nps, class, FlangeKind::parse(kind)?)
    }

    pub fn pipe(&self, nps: &str, schedule: &str) -> Option<&PipeScheduleRow> {
        let nps = canonical_nps(nps);
        let schedule = schedule.trim();
        let schedule = match schedule.get(..3) {
            Some(p) if p.eq_ignore_ascii_case("sch") => schedule.get(3..).unwrap_or_default().trim(),
            _ => schedule,
        };
        self.pipe_rows().find(|r| r.nps == nps && r.schedule.eq_ignore_ascii_case(schedule))
    }

    pub fn head_preset(&self, name: &str) -> Option<&HeadPresetRow> {
        self.head_presets.rows.iter().find(|r| r.name.eq_ignore_ascii_case(name.trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups_find_rows_by_key_and_spelling() {
        let t = TableSet::embedded().unwrap();
        let f = t.flange("2", "150", FlangeKind::Wn).unwrap();
        assert_eq!(f.size_key(), "2/150/wn");
        assert_eq!(t.flange_by_key("2/150/blind").map(|r| r.kind), Some(FlangeKind::Blind));
        assert_eq!(t.flange_by_key("1-1/4/300/so").map(|r| r.od), Some(135.0));
        assert_eq!(t.flange("1 1/4", "300", FlangeKind::So).map(|r| r.od), Some(135.0));
        assert_eq!(t.flange("1.1/4", "300", FlangeKind::So).map(|r| r.od), Some(135.0));
        assert!(t.flange_by_key("2/150").is_none());
        assert!(t.flange_by_key("1-1/4/150/wn/extra").is_none());
        assert!(t.flange_by_key("2/150/nope").is_none());
        assert!(t.flange_in("B16.5", "2", "150", FlangeKind::Wn).is_some());
        assert!(t.flange_in("ASME B16.5", "2", "150", FlangeKind::Wn).is_some());
        assert!(t.flange_in("EN 1092-1", "2", "150", FlangeKind::Wn).is_none());
        let p = t.pipe("2", "std").unwrap();
        assert!((p.id() - (60.3 - 2.0 * 3.91)).abs() < 1e-9, "{p:?}");
        assert_eq!(t.pipe("2", "Sch 40").map(|r| r.wall), Some(3.91));
        assert_eq!(t.head_preset("Klopper").map(|r| r.preset), Some(HeadPreset::KLOPPER));
        assert_eq!(t.head_preset("asme_fd").map(|r| r.preset), Some(HeadPreset::ASME_FD));
    }

    #[test]
    fn bad_tables_are_errors() {
        assert!(TableSet::from_json(&["nope"], &[], "{}").is_err());
        let bad_row = r#"{"source":"x","rows":[{"nps":"2","dn":50,"od":60,"schedule":"STD","wall":40,"source":["a"],"verified":"single-source"}]}"#;
        assert!(TableSet::from_json(&[B16_5_JSON], &[bad_row], HEAD_PRESETS_JSON).is_err());
        let empty = r#"{"source":"x","rows":[]}"#;
        assert!(TableSet::from_json(&[empty], &[empty], empty).is_ok());
        // Provenance: no source, a placeholder, and a one-source "two-source" row are refused.
        let row = |extra: &str| format!(r#"{{"source":"x","rows":[{{"nps":"2","od":60.3,"schedule":"40","wall":3.91{extra}}}]}}"#);
        for bad in ["", r#","source":["a"],"verified":"illustrative""#, r#","source":["a","a"],"verified":"two-source""#] {
            assert!(TableSet::from_json(&[], &[&row(bad)], empty).is_err(), "{bad}");
        }
        assert!(TableSet::from_json(&[], &[&row(r#","source":["a","b"],"verified":"two-source""#)], empty).is_ok());
        let many = vec![empty; MAX_TABLES + 1];
        assert!(TableSet::from_json(&many, &[], empty).is_err());
    }

    #[test]
    fn a_private_table_overrides_the_shipped_row() {
        let private = r#"{"source":"my company","rows":[{"nps":"2","class":"150","kind":"wn","od":152,"bolt_circle":121,"bolt_count":4,"bolt_size":"M16","thickness":18,"hub_od":80,"length_through_hub":62}]}"#;
        let t = TableSet::embedded().unwrap().with_private(private).unwrap();
        let r = t.flange("2", "150", FlangeKind::Wn).unwrap();
        assert_eq!((r.od, r.verified), (152.0, Verified::Unverified));
        assert_eq!(r.bolt_hole_mm(), Some(18.0));
        assert!(TableSet::embedded().unwrap().with_private("[]").is_err());
    }
}
