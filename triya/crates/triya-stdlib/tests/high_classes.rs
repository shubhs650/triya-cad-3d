// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! PLAN T3.2 acceptance: B16.5 Class 400–2500 and B16.47 Series A/B weld-neck and blind rows.
//! Every row marked `two-source` records two different sources; the rest say why not.

use triya_stdlib::{FlangeKind, TableSet, Verified};

/// The site a source entry names (`bd_warehouse`, `wermac.org`, `blog.projectmaterials.com`).
fn origin(source: &str) -> &str {
    source.split([' ', '/']).next().unwrap_or_default()
}

#[test]
fn two_source_rows_record_both_sources() {
    let t = TableSet::embedded().unwrap();
    for table in &t.flanges {
        for r in &table.rows {
            match r.verified {
                Verified::TwoSource => {
                    let mut sites: Vec<&str> = r.source.iter().map(|s| origin(s)).collect();
                    sites.sort_unstable();
                    sites.dedup();
                    assert!(sites.len() >= 2, "{:?} {}: {:?}", table.standard, r.size_key(), r.source);
                }
                // A row the sources disagree on, or that only one source has, says so.
                Verified::SingleSource => assert!(r.note.is_some() || r.source.len() == 1, "{:?} {}", table.standard, r.size_key()),
                other => panic!("{:?} {}: {other:?}", table.standard, r.size_key()),
            }
        }
    }
}

#[test]
fn high_class_b16_5_rows_are_there() {
    let t = TableSet::embedded().unwrap();
    for (class, sizes) in [("400", 12), ("600", 20), ("900", 11), ("1500", 18), ("2500", 13)] {
        for kind in [FlangeKind::Wn, FlangeKind::Blind] {
            let n = t.flanges.iter().filter(|x| x.is_for("B16.5")).flat_map(|x| &x.rows).filter(|r| r.class == class && r.kind == kind).count();
            assert_eq!(n, sizes, "Class {class} {kind:?}");
        }
    }
    // NPS 4 Class 600 WN: OD 275, 8 × 7/8 in on 215.9 in 1 in holes, tf 38.1 (7 mm RF on top).
    let r = t.flange_in("B16.5", "4", "600", FlangeKind::Wn).unwrap();
    assert_eq!((r.od, r.bolt_circle, r.bolt_count, r.bolt_size.as_str(), r.bolt_hole), (275.0, 215.9, 8, "7/8", Some(25.4)));
    assert_eq!((r.thickness, r.raised_face_h, r.verified), (38.1, 7.0, Verified::TwoSource));
    // NPS 12 Class 1500 blind.
    let r = t.flange_in("B16.5", "12", "1500", FlangeKind::Blind).unwrap();
    assert_eq!((r.od, r.bolt_count), (675.0, 16)); // 26.5 in on the inch table
}

#[test]
fn b16_47_series_a_and_b_are_separate_tables() {
    let t = TableSet::embedded().unwrap();
    for standard in ["B16.47A", "B16.47B"] {
        for class in ["150", "300", "400", "600"] {
            let n = t
                .flanges
                .iter()
                .filter(|x| x.is_for(standard))
                .flat_map(|x| &x.rows)
                .filter(|r| r.class == class && r.kind == FlangeKind::Wn)
                .count();
            assert_eq!(n, 18, "{standard} Class {class}");
        }
    }
    // NPS 26 Class 150: Series A OD 870, 24 holes 34.9 on 806.4; Series B OD 785 on 744.5.
    let a = t.flange_in("B16.47A", "26", "150", FlangeKind::Wn).unwrap();
    assert_eq!((a.od, a.bolt_circle, a.bolt_count, a.bolt_hole), (870.0, 806.4, 24, Some(34.9)));
    let b = t.flange_in("B16.47B", "26", "150", FlangeKind::Wn).unwrap();
    assert_eq!((b.od, b.bolt_circle), (785.0, 744.5));
    // B16.5 stops at NPS 24: without a standard the NPS 26 lookup finds Series A first.
    assert!(t.flange_in("B16.5", "26", "150", FlangeKind::Wn).is_none());
    assert_eq!(t.flange("26", "150", FlangeKind::Wn).map(|r| r.od), Some(870.0));
}
