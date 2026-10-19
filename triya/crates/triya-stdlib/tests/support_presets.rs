// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! PLAN T4.4: the shipped support presets (provenance, lookup by exact key and nearest smaller
//! OD) and private support tables searched first.

use triya_stdlib::{TableSet, Verified};

#[test]
fn shipped_presets_cover_saddles_legs_and_lugs_with_provenance() {
    let t = TableSet::embedded().unwrap();
    let rows: Vec<_> = t.supports.iter().flat_map(|t| t.rows.iter()).collect();
    for kind in ["saddle", "leg", "lifting_lug"] {
        assert!(rows.iter().filter(|r| r.kind == kind).count() >= 4, "{kind}");
    }
    for r in &rows {
        assert_eq!(r.verified, Verified::Derived, "{}", r.size_key());
        assert!(!r.source.is_empty());
    }
    assert_eq!(t.support("saddle", "OD1600").unwrap().size_key, "OD1600");
    assert_eq!(t.support("saddle", "od 1700").unwrap().size_key, "OD1600");
    assert_eq!(t.support("leg", "OD350").unwrap().size_key, "OD300");
    assert!(t.support("saddle", "OD100").is_none());
    assert!(t.support("saddle", "LL-25").is_none());
    assert!(t.support("bracket", "OD1600").is_none());
}

#[test]
fn a_private_table_comes_first_and_bad_rows_are_refused() {
    let private = r#"{"source": "fictional", "rows": [
        {"kind": "saddle", "size_key": "OD1600", "fields": {"height_mm": 999}},
        {"kind": "lifting_lug", "size_key": "XL-9", "fields": {"hole_dia_mm": 77}}]}"#;
    let t = TableSet::embedded().unwrap().with_private_supports(private).unwrap();
    let r = t.support("saddle", "OD1600").unwrap();
    assert_eq!(r.fields["height_mm"], 999);
    assert_eq!(r.verified, Verified::Unverified);
    assert_eq!(t.support("lifting_lug", "xl-9").unwrap().fields["hole_dia_mm"], 77);
    for bad in [
        r#"{"source": "x", "rows": [{"kind": "saddle", "size_key": "A", "fields": {}}]}"#,
        r#"{"source": "x", "rows": [{"kind": "saddle", "size_key": "A", "fields": {"host": "E1"}}]}"#,
        r#"{"source": "x", "rows": [{"kind": "", "size_key": "A", "fields": {"a": 1}}]}"#,
        r#"{"source": "x", "rows": [{"kind": "saddle", "size_key": " ", "fields": {"a": 1}}]}"#,
        r#"{"rows": []}"#,
    ] {
        assert!(TableSet::embedded().unwrap().with_private_supports(bad).is_err(), "{bad}");
    }
}

#[test]
fn stainless_pipe_schedules_from_b36_19() {
    let t = TableSet::embedded().unwrap();
    let r = t.pipe("3", "10S").unwrap();
    assert_eq!((r.od, r.wall, r.verified), (88.9, 3.05, Verified::TwoSource));
    assert_eq!(t.pipe("2", "sch 40s").unwrap().wall, 3.91);
    assert_eq!(t.pipe("4", "80S").unwrap().wall, 8.56);
    // Carbon-steel schedules still come from B36.10.
    assert_eq!(t.pipe("3", "40").unwrap().wall, 5.49);
    assert!(t.pipe("1/8", "5S").is_none());
}
