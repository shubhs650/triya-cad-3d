// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! PLAN T3.1 acceptance: the shipped B16.5 Class 150/300 and B36.10M tables (row counts, spot
//! values, provenance on every row) and the validator rules of `docs/standards-library.md` §4.

use std::collections::BTreeMap;

use triya_stdlib::{FlangeKind, TableSet, Verified};

/// NPS as a number, for ordering (`"1-1/4"` → 1.25).
fn nps_value(nps: &str) -> f64 {
    let mut total = 0.0;
    for part in nps.split('-') {
        total += match part.split_once('/') {
            Some((n, d)) => n.parse::<f64>().unwrap() / d.parse::<f64>().unwrap(),
            None => part.parse::<f64>().unwrap(),
        };
    }
    total
}

/// At most `places` decimals (a re-keyed millimetre value, not a converted inch value).
fn decimals_at_most(x: f64, places: i32) -> bool {
    let k = 10f64.powi(places);
    ((x * k).round() - x * k).abs() < 1e-6
}

#[test]
fn b16_5_class_150_and_300_tables_are_complete() {
    let t = TableSet::embedded().unwrap();
    assert!(!t.is_illustrative());
    let b165 = t.flanges.iter().find(|t| t.is_for("B16.5")).unwrap();
    let count = |class: &str, kind: FlangeKind| b165.rows.iter().filter(|r| r.class == class && r.kind == kind).count();
    for class in ["150", "300"] {
        // NPS 1/2 … 24: 21 sizes (incl. 3-1/2 and 22) for WN, SO, blind and LJ; SW to NPS 3.
        for kind in [FlangeKind::Wn, FlangeKind::So, FlangeKind::Blind, FlangeKind::Lj] {
            assert_eq!(count(class, kind), 21, "Class {class} {kind:?}");
        }
        assert_eq!(count(class, FlangeKind::Sw), 8, "Class {class} SW");
    }
    assert_eq!(b165.rows.len(), 2 * (4 * 21 + 8));

    // NPS 4 Class 150 WN: B16.5's metric table gives OD 230 (the inch table's 9.00 in is 228.6),
    // 8 bolts 5/8 in on 190.5 in 3/4 in holes, tf 22.3, hub 135, Y 75, RF 157.2.
    let r = t.flange("4", "150", FlangeKind::Wn).unwrap();
    assert_eq!((r.od, r.bolt_circle, r.bolt_count, r.bolt_size.as_str()), (230.0, 190.5, 8, "5/8"));
    assert_eq!((r.bolt_hole, r.thickness, r.hub_od, r.length_through_hub, r.raised_face_d), (Some(19.1), 22.3, 135.0, 75.0, 157.2));
    assert!((r.od - 228.6).abs() < 1.5, "metric rounding of 9.00 in");
    assert_eq!(r.verified, Verified::TwoSource);
    // NPS 24 Class 300 WN: OD 915, 24 × 1-1/2 on 812.8.
    let r = t.flange("24", "300", FlangeKind::Wn).unwrap();
    assert_eq!((r.od, r.bolt_circle, r.bolt_count, r.bolt_size.as_str()), (915.0, 812.8, 24, "1-1/2"));
    // A slip-on has a bore over the pipe, a socket weld a socket, a blind no hub.
    assert_eq!(t.flange("2", "150", FlangeKind::So).unwrap().bore, Some(61.9));
    assert_eq!(t.flange("1", "300", FlangeKind::Sw).unwrap().socket_depth, Some(13.0));
    assert_eq!(t.flange("6", "300", FlangeKind::Blind).unwrap().hub_od, 0.0);
}

#[test]
fn b36_10_schedules_are_complete() {
    let t = TableSet::embedded().unwrap();
    let p = t.pipe("4", "STD").unwrap();
    assert_eq!((p.od, p.wall, p.dn), (114.3, 6.02, Some(100)));
    assert_eq!(t.pipe("4", "40").unwrap().wall, 6.02);
    assert_eq!(t.pipe("4", "XS").unwrap().wall, 8.56);
    assert_eq!(t.pipe("24", "STD").unwrap().wall, 9.53);
    assert_eq!(t.pipe("1/2", "160").unwrap().wall, 4.78);
    let rows: Vec<_> = t.pipe_rows().collect();
    assert_eq!(rows.len(), 279);
    for nps in ["1/2", "3/4", "1", "1-1/4", "1-1/2", "2", "3", "4", "6", "8", "10", "12", "16", "24"] {
        for sch in ["STD", "XS", "40", "80"] {
            assert!(t.pipe(nps, sch).is_some(), "NPS {nps} {sch}");
        }
    }
}

#[test]
fn every_shipped_row_has_its_provenance() {
    let t = TableSet::embedded().unwrap();
    for r in t.flange_rows() {
        assert!(!r.source.is_empty() && r.verified != Verified::Unverified && r.verified != Verified::Illustrative, "{}", r.size_key());
        if r.verified == Verified::TwoSource {
            assert!(r.source.len() >= 2 && r.source[0] != r.source[1], "{}", r.size_key());
        }
    }
    for r in t.pipe_rows() {
        assert!(!r.source.is_empty() && r.verified != Verified::Unverified && r.verified != Verified::Illustrative, "{}", r.size_key());
    }
    let two = t.flange_rows().filter(|r| r.verified == Verified::TwoSource).count();
    assert!(two * 10 >= t.flange_rows().count() * 9, "{two} two-source flange rows");
}

#[test]
fn shipped_rows_pass_the_sanity_rules() {
    let t = TableSet::embedded().unwrap();
    // OD, bolt circle and bolt count do not decrease with NPS within a standard, class and kind.
    let mut groups: BTreeMap<(String, &str), Vec<_>> = BTreeMap::new();
    for r in t.flange_rows() {
        groups.entry((r.class.clone(), r.kind.code())).or_default().push(r);
        assert!(r.bolt_circle < r.od && (r.raised_face_d == 0.0 || r.raised_face_d < r.bolt_circle), "{}", r.size_key());
        let hole = r.bolt_hole.unwrap();
        assert!(r.hub_od < r.bolt_circle - hole && r.bolt_circle + hole < r.od, "{}: hub/holes/rim", r.size_key());
        for x in [r.od, r.bolt_circle, r.thickness, r.hub_od, r.raised_face_d, r.length_through_hub, hole] {
            assert!(decimals_at_most(x, 1), "{}: {x}", r.size_key());
        }
        assert!(r.bolt_size.starts_with('M') || r.bolt_size.chars().all(|c| c.is_ascii_digit() || c == '/' || c == '-'), "{}", r.bolt_size);
    }
    for ((class, kind), mut rows) in groups {
        rows.sort_by(|a, b| nps_value(&a.nps).total_cmp(&nps_value(&b.nps)));
        for w in rows.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!(b.od >= a.od && b.bolt_circle >= a.bolt_circle && b.bolt_count >= a.bolt_count, "Class {class} {kind}: {} → {}", a.nps, b.nps);
        }
    }
    for r in t.pipe_rows() {
        assert!(decimals_at_most(r.od, 1) && decimals_at_most(r.wall, 2), "{}", r.size_key());
    }
}
