// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! Golden-file tests (`docs/drawing-model.md` §7): each case's SVG (and, for the export cases,
//! DXF and PDF) is compared byte for byte with `tests/golden/<case>.<ext>`. `TRIYA_UPDATE_GOLDEN=1 cargo test -p triya-drawing --test golden`
//! rewrites them (look at the diff before committing).

use std::path::PathBuf;

use serde_json::{Value, json};
use solidcraft_engine::Session;
use triya_drawing::ga::{self, GaOptions};
use triya_drawing::model::{Dimension, Layer, P2, Sheet, SheetSize, Table, TableKind, View};
use triya_drawing::vessel_views::{self, VesselModel};
use triya_drawing::{dims, to_dxf, to_pdf, to_svg};
use triya_vessel::VesselDoc;

fn golden(case: &str, text: &str) {
    golden_bytes(case, "svg", text.as_bytes());
}

fn golden_bytes(case: &str, ext: &str, bytes: &[u8]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(format!("{case}.{ext}"));
    if std::env::var_os("TRIYA_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, bytes).unwrap();
        return;
    }
    let want = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e} (run with TRIYA_UPDATE_GOLDEN=1)", path.display()));
    if want != bytes {
        let out = std::env::temp_dir().join(format!("{case}.actual.{ext}"));
        let _ = std::fs::write(&out, bytes);
        panic!("{case}: the {ext} differs from {} (actual output in {})", path.display(), out.display());
    }
}

pub fn reference(name: &str) -> Value {
    serde_json::from_str(match name {
        "a" => include_str!("../../triya-vessel/tests/reference/vessel-a.json"),
        "b" => include_str!("../../triya-vessel/tests/reference/vessel-b.json"),
        _ => include_str!("../../triya-vessel/tests/reference/vessel-c.json"),
    })
    .unwrap()
}

fn model(name: &str) -> VesselModel {
    let doc = VesselDoc::from_value(&reference(name)).unwrap();
    VesselModel::embedded(&doc).unwrap()
}

/// `vessel.bom` of the reference vessel, not regenerated (handbook weights).
pub fn bom(name: &str) -> Value {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    let mut s = Session::default();
    s.execute("vessel.new", &json!({"doc": reference(name)})).unwrap();
    s.execute("vessel.bom", &json!({})).unwrap()
}

fn view_sheet(v: View) -> String {
    let mut sheet = Sheet::new(SheetSize::A1);
    sheet.views.push(v);
    let (lo, _) = triya_drawing::plot::view_sheet_bounds(&sheet.views[0]).unwrap();
    let v = &mut sheet.views[0];
    v.origin = P2::new(v.origin.x + 60.0 - lo.x, v.origin.y + 80.0 - lo.y);
    to_svg(&sheet).unwrap()
}

#[test]
fn rect_dim_table() {
    let mut sheet = Sheet::new(SheetSize::A3);
    let mut v = View::new("front", P2::new(60.0, 120.0), 0.1);
    v.title = "FRONT".into();
    let r = [P2::new(0.0, 0.0), P2::new(1000.0, 0.0), P2::new(1000.0, 500.0), P2::new(0.0, 500.0)];
    for i in 0..4 {
        v.line(r[i], r[(i + 1) % 4], Layer::Visible);
    }
    v.dimensions.push(Dimension::linear(r[0], r[1], P2::new(0.0, -100.0), P2::new(1.0, 0.0), "1000", "E1.length_mm"));
    v.dimensions.push(Dimension::linear(r[1], r[2], P2::new(1100.0, 0.0), P2::new(0.0, 1.0), "500", "E1.diameter_mm"));
    sheet.views.push(v);
    let mut t = Table::new(TableKind::Other, "PARTS", &["ITEM", "DESCRIPTION", "QTY"], &[12.0, 50.0, 12.0]);
    t.at = P2::new(230.0, 250.0);
    t.rows.push(vec!["1".into(), "PLATE 1000 x 500 x 10".into(), "1".into()]);
    t.rows.push(vec!["2".into(), "a description far too long for its column".into(), "2".into()]);
    sheet.tables.push(t);
    sheet.title_block.title = "RECTANGLE".into();
    sheet.title_block.drawing_number = "T-001".into();
    sheet.title_block.scale = "1:10".into();
    golden("rect_dim_table", &to_svg(&sheet).unwrap());
    golden_bytes("rect_dim_table", "dxf", to_dxf(&sheet).unwrap().as_bytes());
    golden_bytes("rect_dim_table", "pdf", &to_pdf(&sheet).unwrap());
}

#[test]
fn vessel_a_elevation() {
    let m = model("a");
    let mut v = vessel_views::elevation(&m, 1.0 / 25.0).unwrap();
    dims::elevation(&m, &mut v);
    golden("vessel_a_elevation", &view_sheet(v));
}

#[test]
fn vessel_a_plan() {
    let m = model("a");
    let mut v = vessel_views::plan(&m, 1.0 / 25.0).unwrap();
    dims::plan(&m, &mut v);
    golden("vessel_a_plan", &view_sheet(v));
}

#[test]
fn vessel_a_section() {
    let m = model("a");
    golden("vessel_a_section", &view_sheet(vessel_views::section(&m, 1.0 / 25.0).unwrap()));
}

#[test]
fn vessel_b_horizontal() {
    let m = model("b");
    let mut v = vessel_views::elevation(&m, 1.0 / 20.0).unwrap();
    dims::elevation(&m, &mut v);
    golden("vessel_b_horizontal", &view_sheet(v));
}

#[test]
fn vessel_c_legs() {
    let m = model("c");
    let mut v = vessel_views::elevation(&m, 1.0 / 15.0).unwrap();
    dims::elevation(&m, &mut v);
    golden("vessel_c_legs", &view_sheet(v));
}

#[test]
fn ga_sheets_of_the_reference_vessels() {
    for name in ["a", "b", "c"] {
        let m = model(name);
        let b = bom(name);
        let sheet = ga::general_arrangement(&m, Some(&b), &GaOptions::default()).unwrap();
        golden(&format!("vessel_{name}_ga"), &to_svg(&sheet).unwrap());
        if name == "a" {
            golden_bytes("vessel_a_ga", "dxf", to_dxf(&sheet).unwrap().as_bytes());
            golden_bytes("vessel_a_ga", "pdf", &to_pdf(&sheet).unwrap());
        }
    }
}
