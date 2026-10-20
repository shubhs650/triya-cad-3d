// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The sheet's tables (`docs/drawing-model.md` §4): design data, nozzle schedule, bill of
//! materials (from `vessel.bom`), notes and revisions. Every value is read from the vessel
//! document, its resolved geometry or the BOM; nothing is typed into the drawing.

use serde_json::Value;
use triya_vessel::model::{AngleSense, AngleZero, AttachmentKind, Facing, Frame, NeckType, Orientation, ProjectionFrom, Rating};

use crate::fmt::{deg, mm, num};
use crate::model::{Table, TableKind};
use crate::plot::{TABLE_TEXT_H, text_width};
use crate::vessel_views::VesselModel;

/// Most rows a table shows (the rest is summarised in its last row).
pub const MAX_ROWS: usize = 120;

/// Column widths from the contents (header and rows), within `[min, max]` mm.
pub fn fit_columns(t: &mut Table, min: f64, max: f64) {
    t.column_widths = (0..t.columns.len())
        .map(|i| {
            std::iter::once(t.columns.get(i))
                .chain(t.rows.iter().map(|r| r.get(i)))
                .flatten()
                .map(|c| text_width(c, TABLE_TEXT_H) + 3.0)
                .fold(min, f64::max)
                .min(max)
        })
        .collect();
}

fn pressure(p: &Option<triya_vessel::model::Pressure>) -> String {
    p.as_ref().map_or("-".into(), |p| format!("{} kPa(g) @ {} °C", num(p.kpa), num(p.at_c)))
}

fn opt(x: Option<f64>, unit: &str) -> String {
    x.map_or("-".into(), |v| format!("{}{unit}", num(v)))
}

fn opt_s(x: &Option<String>) -> String {
    x.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "-".into())
}

/// The design-data block; weights from the document or, when it has none, the BOM's totals.
pub fn design_data(m: &VesselModel, bom: Option<&Value>) -> Table {
    let d = &m.doc.design_data;
    let totals = bom.and_then(|b| b.get("totals"));
    // Before a regenerate nothing is measured and the totals are 0: weigh the handbook values.
    let handbook: f64 = bom
        .and_then(|b| b.get("items"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|it| it.get("weight_kg").and_then(Value::as_f64).or_else(|| it.get("handbook_kg").and_then(Value::as_f64)))
        .sum();
    let measured = totals.and_then(|t| t.get("empty_kg")).and_then(Value::as_f64).unwrap_or(0.0);
    let shift = if measured > 0.0 { 0.0 } else { handbook };
    let total = |k: &str| {
        let v = totals.and_then(|t| t.get(k)).and_then(Value::as_f64)?;
        let v = if k.ends_with("_kg") { v + shift } else { v };
        Some(crate::bom_round(v))
    };
    let mut t = Table::new(TableKind::DesignData, "DESIGN DATA", &["ITEM", "VALUE"], &[]);
    let mut row = |k: &str, v: String| t.rows.push(vec![k.to_string(), v]);
    row("DESIGN CODE", opt_s(&d.code));
    row("MAWP INTERNAL", pressure(&d.mawp_int));
    row("MAWP EXTERNAL", pressure(&d.mawp_ext));
    row("DESIGN TEMPERATURE", opt(d.design_temp_c, " °C"));
    row("MDMT", opt(d.mdmt_c, " °C"));
    row("HYDROTEST PRESSURE", opt(d.hydrotest_kpa, " kPa(g)"));
    row("CORROSION ALLOWANCE", format!("{} mm", num(m.doc.global.corrosion_allowance_mm)));
    row("JOINT EFFICIENCY", opt(d.joint_eff, ""));
    row("RADIOGRAPHY", opt_s(&d.rt));
    row("PWHT", d.pwht.map_or("-".into(), |p| if p { "YES".into() } else { "NO".into() }));
    row("WIND / SEISMIC", format!("{} / {}", opt_s(&d.wind_code), opt_s(&d.seismic_code)));
    row("CAPACITY", opt(d.capacity_m3.or_else(|| total("water_m3")), " m³"));
    row("MATERIAL (SHELL)", m.doc.global.default_material.clone());
    if m.doc.global.insulation_thickness_mm > 0.0 {
        row("INSULATION", format!("{} mm", num(m.doc.global.insulation_thickness_mm)));
    }
    row("WEIGHT EMPTY", opt(d.empty_weight_kg.or_else(|| total("empty_kg")), " kg"));
    row("WEIGHT OPERATING", opt(d.operating_weight_kg.or_else(|| total("operating_kg")), " kg"));
    row("WEIGHT TEST", opt(d.test_weight_kg.or_else(|| total("test_kg")), " kg"));
    let np = d.nameplate.as_ref();
    row(
        "SERIAL / YEAR",
        format!(
            "{} / {}",
            np.and_then(|n| n.serial.clone()).unwrap_or_else(|| "-".into()),
            np.and_then(|n| n.year).map_or("-".into(), |y| y.to_string())
        ),
    );
    fit_columns(&mut t, 12.0, 80.0);
    t
}

fn rating(r: &Rating) -> String {
    match r {
        Rating::Class(c) => format!("CL {c}"),
        Rating::Named(s) => s.clone(),
    }
}

/// The nozzle schedule, in the document's order.
pub fn nozzle_schedule(m: &VesselModel) -> Table {
    let mut t = Table::new(
        TableKind::NozzleSchedule,
        "NOZZLE SCHEDULE",
        &["MARK", "QTY", "SIZE", "SCH/THK", "RATING", "FACING", "TYPE", "ELEV.", "AZIM.", "PROJ.", "SERVICE", "REMARKS"],
        &[],
    );
    for a in &m.doc.attachments {
        let AttachmentKind::Nozzle { nps, schedule, neck_thk_mm, neck_type, flange, projection, pad, manway, service, attach, .. } = &a.kind else {
            continue;
        };
        let g = m.nozzles.iter().find(|g| g.id == a.id);
        let sch = match (schedule, neck_thk_mm) {
            (_, Some(th)) => format!("{} THK", num(*th)),
            (Some(s), None) => s.clone(),
            _ => "-".into(),
        };
        let (rating, facing, kind) = match flange {
            Some(f) => (
                format!(
                    "{} {}",
                    serde_json::to_value(f.standard).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default(),
                    rating(&Rating::Class(f.class))
                ),
                match f.facing {
                    Facing::Rf => "RF",
                    Facing::Ff => "FF",
                    Facing::Rtj => "RTJ",
                }
                .to_string(),
                f.kind.code().to_ascii_uppercase(),
            ),
            None => ("-".into(), "-".into(), "-".into()),
        };
        let elev = g.and_then(|g| m.nozzle_z(g)).map_or_else(|| format!("ON {}", a.host), mm);
        let from = match projection.from {
            ProjectionFrom::Cl => "CL",
            ProjectionFrom::Od => "OD",
            ProjectionFrom::Insulation => "INS",
        };
        let mut remarks: Vec<String> = Vec::new();
        match a.frame {
            Frame::Radial => {}
            Frame::Hillside { offset_mm } => remarks.push(format!("HILLSIDE {}", mm(offset_mm))),
            Frame::Tangential { offset_mm } => remarks.push(offset_mm.map_or("TANGENTIAL".into(), |o| format!("TANGENTIAL {}", mm(o)))),
            Frame::Tilted { tilt_deg } => remarks.push(format!("TILTED {}", deg(tilt_deg))),
            Frame::ParallelToAxis { radius_mm } => remarks.push(format!("AXIAL AT R {}", mm(radius_mm))),
            Frame::HeadRadialAtR { radius_mm } => remarks.push(format!("NORMAL AT R {}", mm(radius_mm))),
        }
        if let Some(p) = pad {
            remarks.push(format!("PAD {} x {}", mm(p.width_mm), num(p.thickness_mm)));
        }
        if let Some(mw) = manway {
            remarks.push(if mw.davit { "MANWAY, DAVIT".into() } else { "MANWAY".into() });
        }
        if *neck_type != NeckType::Pipe {
            remarks.push(match neck_type {
                NeckType::Lwn => "LWN".into(),
                NeckType::Forged => "FORGED".into(),
                NeckType::Pipe => String::new(),
            });
        }
        if *attach == triya_vessel::model::NozzleAttach::SetOn {
            remarks.push("SET ON".into());
        }
        if a.flags.boolean_failed {
            remarks.push("SEPARATE BODY".into());
        }
        if a.flags.unverified_table || g.is_some_and(|g| g.unverified_table) {
            remarks.push("UNVERIFIED TABLE".into());
        }
        t.rows.push(vec![
            a.id.clone(),
            "1".into(),
            format!("{nps}\""),
            sch,
            rating,
            facing,
            kind,
            elev,
            deg(a.angle_deg.rem_euclid(360.0)),
            format!("{} {from}", mm(projection.mm)),
            opt_s(service),
            remarks.join(", "),
        ]);
    }
    fit_columns(&mut t, 8.0, 60.0);
    t
}

fn standard_ref(m: &VesselModel, id: &str) -> String {
    m.doc
        .attachments
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.standard_ref.as_ref())
        .map(|r| {
            let v = serde_json::to_value(r).unwrap_or(Value::Null);
            let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            let rev = s("rev");
            format!("{} {}{}", s("org"), s("number"), if rev.is_empty() { String::new() } else { format!(" R{rev}") }).trim().to_string()
        })
        .unwrap_or_default()
}

/// The bill of materials from `vessel.bom`'s JSON: identical parts of one item on one row with
/// their count; weights measured where the vessel is built, handbook values otherwise.
pub fn bom(m: &VesselModel, bom: &Value) -> Table {
    let mut t =
        Table::new(TableKind::Bom, "BILL OF MATERIALS", &["ITEM", "QTY", "MARK", "DESCRIPTION", "MATERIAL", "STD / REF", "UNIT kg", "TOTAL kg"], &[]);
    // (mark, description, material) → (qty, unit weight, any handbook-only).
    let mut groups: Vec<(String, String, String, u32, f64, bool)> = Vec::new();
    let mut handbook_only = false;
    for it in bom.get("items").and_then(Value::as_array).into_iter().flatten().take(10_000) {
        let s = |k: &str| it.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let (id, desc, mat) = (s("id"), s("description").to_ascii_uppercase(), s("material"));
        let qty = it.get("qty").and_then(Value::as_u64).unwrap_or(1).min(100_000) as u32;
        let measured = it.get("weight_kg").and_then(Value::as_f64);
        let w = measured.or_else(|| it.get("handbook_kg").and_then(Value::as_f64)).unwrap_or(0.0);
        let hb = measured.is_none();
        handbook_only |= hb;
        match groups.iter_mut().find(|g| g.0 == id && g.1 == desc && g.2 == mat && (g.4 - w).abs() <= 0.005 * w.abs().max(1.0)) {
            Some(g) => {
                g.3 += qty;
                g.5 |= hb;
            }
            None => groups.push((id, desc, mat, qty, w, hb)),
        }
    }
    let mut total = 0.0;
    for (i, (id, desc, mat, qty, w, _)) in groups.iter().enumerate() {
        total += w * f64::from(*qty);
        if i < MAX_ROWS {
            t.rows.push(vec![
                (i + 1).to_string(),
                qty.to_string(),
                id.clone(),
                desc.clone(),
                mat.clone(),
                standard_ref(m, id),
                num(*w),
                num(w * f64::from(*qty)),
            ]);
        }
    }
    if groups.len() > MAX_ROWS {
        t.rows.push(vec![String::new(), String::new(), String::new(), format!("… {} MORE ITEMS (SEE BOM EXPORT)", groups.len() - MAX_ROWS)]);
    }
    t.rows.push(vec![String::new(), String::new(), String::new(), "TOTAL (EMPTY)".into(), String::new(), String::new(), String::new(), num(total)]);
    if handbook_only {
        t.title = "BILL OF MATERIALS (WEIGHTS: HANDBOOK VALUES WHERE NOT REGENERATED)".into();
    }
    fit_columns(&mut t, 8.0, 85.0);
    t
}

/// The general notes: the drawing's defaults, then the document's own notes.
pub fn notes(m: &VesselModel) -> Table {
    let gl = &m.doc.global;
    let zero = match (gl.orientation, gl.angle_zero) {
        (Orientation::Vertical, AngleZero::PlusX) => "0° = +X",
        (Orientation::Vertical, _) => "0° = PLANT NORTH",
        (Orientation::Horizontal, AngleZero::PlusY) => "0° = +Y",
        (Orientation::Horizontal, _) => "0° = TOP",
    };
    let sense = match (gl.orientation, gl.angle_sense) {
        (Orientation::Vertical, AngleSense::ClockwiseFromAbove) => "CLOCKWISE LOOKING DOWN",
        (Orientation::Vertical, AngleSense::CounterclockwiseFromAbove) => "COUNTER-CLOCKWISE LOOKING DOWN",
        (Orientation::Horizontal, AngleSense::ClockwiseFromAbove) => "CLOCKWISE LOOKING FROM THE LEFT END",
        (Orientation::Horizontal, AngleSense::CounterclockwiseFromAbove) => "COUNTER-CLOCKWISE LOOKING FROM THE LEFT END",
    };
    let mut lines: Vec<String> = vec![
        "ALL DIMENSIONS IN mm UNLESS NOTED.".into(),
        format!("ELEVATIONS FROM THE DATUM ({}).", m.datum_name()),
        format!("NOZZLE ORIENTATION: {zero}, {sense}."),
        "WELDS ARE NOT MODELLED; WELD DETAILS PER THE FABRICATION SPECIFICATION.".into(),
    ];
    for n in &m.doc.notes {
        let up = n.trim().to_ascii_uppercase();
        if !up.is_empty() && !lines.iter().any(|l| l.eq_ignore_ascii_case(&up)) {
            lines.push(up);
        }
    }
    let flagged: Vec<&str> = m.doc.attachments.iter().filter(|a| a.flags.boolean_failed).map(|a| a.id.as_str()).collect();
    if !flagged.is_empty() {
        lines.push(format!("NOZZLES {} ARE SEPARATE BODIES IN THE 3D MODEL (KERNEL BOOLEAN FAILED).", flagged.join(", ")));
    }
    for w in m.warnings.iter().take(8) {
        lines.push(format!("CHECK: {}", w.to_ascii_uppercase()));
    }
    let mut t = Table::new(TableKind::Notes, "NOTES", &["", "NOTE"], &[]);
    for (i, l) in lines.into_iter().take(MAX_ROWS).enumerate() {
        t.rows.push(vec![format!("{}.", i + 1), l]);
    }
    fit_columns(&mut t, 6.0, 170.0);
    t
}

/// The revision table, newest last.
pub fn revisions(m: &VesselModel) -> Table {
    let mut t = Table::new(TableKind::Revisions, "REVISIONS", &["REV", "DATE", "BY", "DESCRIPTION"], &[]);
    for r in m.doc.revisions.iter().take(MAX_ROWS) {
        t.rows.push(vec![r.rev.clone(), r.date.clone(), r.by.clone(), r.desc.to_ascii_uppercase()]);
    }
    if t.rows.is_empty() {
        t.rows.push(vec!["-".into(), "-".into(), "-".into(), "-".into()]);
    }
    fit_columns(&mut t, 10.0, 120.0);
    t
}
