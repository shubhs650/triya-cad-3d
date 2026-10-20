// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The stack editor (PLAN T5.2): the vessel's elements in stack order with their stations, add /
//! insert / delete / move, the fields of the selected element, and the vessel-wide settings
//! (orientation, datum, diameter basis). Only type, diameter, thickness and length are needed:
//! an empty diameter inherits the previous element's (stopping at a cone, which states both
//! ends). Every edit is one [`edits`] action.

use egui::RichText;
use serde_json::{Value, json};
use solidcraft_ui_egui::theme::Tokens;
use triya_vessel::edit::ELEMENT_KINDS;
use triya_vessel::model::{Datum, DiameterBasis, Element, ElementKind, HeadPresetId, Orientation};

use super::VesselPanel;
use super::edits::{self, Action, fmt_num};

const FIELD_W: f32 = 64.0;

fn datum_label(d: Datum) -> &'static str {
    match d {
        Datum::BottomTl => "Bottom TL",
        Datum::BottomSeam => "Bottom seam",
        Datum::SkirtBase => "Skirt base",
        Datum::PlantElevation => "Plant elevation",
        Datum::LeftTl => "Left TL",
    }
}

const DATUMS: [Datum; 5] = [Datum::BottomTl, Datum::BottomSeam, Datum::SkirtBase, Datum::PlantElevation, Datum::LeftTl];

fn preset_label(p: HeadPresetId) -> &'static str {
    match p {
        HeadPresetId::AsmeFd => "ASME F&D",
        HeadPresetId::Ellipsoidal21 => "2:1 ellipsoidal",
        HeadPresetId::Ellipsoidal21Ug32Approx => "2:1 (0.9D/0.17D)",
        HeadPresetId::KloepperDin28011 => "Klöpper (DIN 28011)",
        HeadPresetId::KorbbogenDin28013 => "Korbbogen (DIN 28013)",
        HeadPresetId::Torispherical8010 => "Torispherical 80:10",
        HeadPresetId::Hemispherical => "Hemispherical",
        HeadPresetId::CustomTorispherical => "Custom torispherical",
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "cylinder" => "Cylinder",
        "head" => "Head",
        "cone" => "Cone",
        "skirt" => "Skirt",
        "flat_cover" => "Flat cover",
        "body_flange" => "Body flange",
        _ => "?",
    }
}

/// The element's "length" column: what sets its axial size.
fn length_of(e: &Element) -> Option<(&'static str, f64)> {
    match &e.kind {
        ElementKind::Cylinder { length_mm } => Some(("length_mm", *length_mm)),
        ElementKind::Cone { length_mm, .. } => Some(("length_mm", *length_mm)),
        ElementKind::Skirt { height_mm, .. } => Some(("height_mm", *height_mm)),
        _ => None,
    }
}

pub fn show(p: &mut VesselPanel, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let Some(doc) = p.doc.clone() else {
        ui.label("The design has no vessel. New makes a one-cylinder vessel to start from.");
        return;
    };
    settings(p, ui, &doc);
    // Live station readout.
    if let Some(r) = p.resolved() {
        let mut parts = Vec::new();
        if let Some(l) = r.tl_to_tl() {
            parts.push(format!("TL–TL {}", fmt_num(l)));
        }
        parts.push(format!("overall {}", fmt_num(r.z_max - r.z_min)));
        parts.push(format!("from {} to {} ({} = 0)", fmt_num(r.z_min), fmt_num(r.z_max), datum_label(doc.global.datum)));
        ui.label(RichText::new(parts.join(" · ")).color(t.text_dim));
    }
    let along = if doc.global.orientation == Orientation::Horizontal { "left → right" } else { "bottom → top" };
    ui.label(RichText::new(format!("Elements, {along}")).strong());
    let mut actions: Vec<Option<Action>> = Vec::new();
    let n = doc.elements.len();
    egui::ScrollArea::vertical().id_salt("triya_stack_rows").max_height(260.0).show(ui, |ui| {
        egui::Grid::new("triya_stack_grid").striped(true).spacing([6.0, 3.0]).show(ui, |ui| {
            for h in ["id", "kind", "D", "t", "L", "z from", "z to", ""] {
                ui.label(RichText::new(h).color(t.text_dim).size(11.0));
            }
            ui.end_row();
            for (i, e) in doc.elements.iter().enumerate() {
                let st = p.resolved().and_then(|r| r.station(&e.id)).cloned();
                let selected = p.selected_element.as_deref() == Some(e.id.as_str());
                if ui.selectable_label(selected, RichText::new(&e.id).strong()).on_hover_text("Show its fields below").clicked() {
                    p.selected_element = Some(e.id.clone());
                }
                ui.label(kind_label(e.kind.name()));
                let inherited = st.as_ref().map(|s| fmt_num(s.diameter_mm)).unwrap_or_default();
                if let Some(v) = p.num_field(ui, &format!("{}/d", e.id), e.diameter_mm, &inherited, FIELD_W) {
                    actions.push(edits::element_field(&doc, &e.id, &["diameter_mm"], v.map(|x| json!(x))));
                }
                if let Some(Some(v)) = p.num_field(ui, &format!("{}/t", e.id), Some(e.thickness_mm), "", 44.0) {
                    actions.push(edits::element_field(&doc, &e.id, &["thickness_mm"], Some(json!(v))));
                }
                match length_of(e) {
                    Some((field, l)) => {
                        if let Some(Some(v)) = p.num_field(ui, &format!("{}/l", e.id), Some(l), "", FIELD_W) {
                            actions.push(edits::element_field(&doc, &e.id, &[field], Some(json!(v))));
                        }
                    }
                    None => {
                        ui.label(RichText::new("—").color(t.text_dim));
                    }
                }
                match &st {
                    Some(s) => {
                        let tl = s.tl.map(|z| format!("TL {}", fmt_num(z))).unwrap_or_default();
                        ui.label(fmt_num(s.z_from)).on_hover_text(format!("inside radius {} → {} {tl}", fmt_num(s.r_from), fmt_num(s.r_to)));
                        ui.label(fmt_num(s.z_to));
                    }
                    None => {
                        ui.label("?");
                        ui.label("?");
                    }
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(i > 0, egui::Button::new("↑").small()).on_hover_text("Move toward the stack's start").clicked() {
                        actions.push(edits::move_element(&doc, p.resolved(), &e.id, -1));
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("↓").small()).on_hover_text("Move toward the stack's end").clicked() {
                        actions.push(edits::move_element(&doc, p.resolved(), &e.id, 1));
                    }
                    if ui.add_enabled(n > 1, egui::Button::new("x").small()).on_hover_text("Delete (with the attachments on it)").clicked() {
                        actions.push(edits::delete_element(&doc, p.resolved(), &e.id));
                    }
                });
                ui.end_row();
            }
        });
    });
    // Add / insert.
    ui.horizontal(|ui| {
        let key = egui::Id::new("triya_stack_new_kind");
        let mut kind: String = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| "cylinder".into());
        egui::ComboBox::from_id_salt("triya_stack_kind").selected_text(kind_label(&kind)).show_ui(ui, |ui| {
            for k in ELEMENT_KINDS {
                ui.selectable_value(&mut kind, k.to_string(), kind_label(k));
            }
        });
        ui.data_mut(|d| d.insert_temp(key, kind.clone()));
        let after = p.selected_element.clone();
        let label = if after.is_some() { "Insert after selected" } else { "Add" };
        if ui.button(label).on_hover_text("vessel.element.add: defaults from the neighbour (thickness) and the stack (diameter)").clicked() {
            actions.push(Some(edits::insert_element(&kind, after.as_deref())));
        }
    });
    if let Some(sel) = p.selected_element.clone()
        && let Some(e) = doc.element(&sel).cloned()
    {
        ui.separator();
        detail(p, ui, &doc, &e, &mut actions);
    }
    for a in actions {
        p.queue(a);
    }
}

fn settings(p: &mut VesselPanel, ui: &mut egui::Ui, doc: &triya_vessel::VesselDoc) {
    let g = &doc.global;
    ui.horizontal_wrapped(|ui| {
        let mut o = g.orientation;
        egui::ComboBox::from_id_salt("triya_orientation").selected_text(if o == Orientation::Vertical { "Vertical" } else { "Horizontal" }).show_ui(
            ui,
            |ui| {
                ui.selectable_value(&mut o, Orientation::Vertical, "Vertical");
                ui.selectable_value(&mut o, Orientation::Horizontal, "Horizontal");
            },
        );
        if o != g.orientation {
            // A horizontal vessel measures from its left TL; a vertical one from its bottom TL.
            let datum = match (o, g.datum) {
                (Orientation::Horizontal, _) => Datum::LeftTl,
                (Orientation::Vertical, Datum::LeftTl) => Datum::BottomTl,
                (_, d) => d,
            };
            p.queue(Some(Action {
                id: "vessel.set",
                params: json!({"patch": [
                    {"op": "add", "path": "/global/orientation", "value": o},
                    {"op": "add", "path": "/global/datum", "value": datum},
                ]}),
            }));
        }
        ui.label("Datum");
        let mut d = g.datum;
        egui::ComboBox::from_id_salt("triya_datum").selected_text(datum_label(d)).show_ui(ui, |ui| {
            for x in DATUMS {
                let fits = (x == Datum::LeftTl) == (g.orientation == Orientation::Horizontal);
                if fits {
                    ui.selectable_value(&mut d, x, datum_label(x));
                }
            }
        });
        if d != g.datum {
            p.queue(Some(Action { id: "vessel.datum", params: json!({"datum": d}) }));
        }
        if g.datum == Datum::PlantElevation {
            ui.label("at");
            if let Some(Some(v)) = p.num_field(ui, "global/plant", Some(g.plant_elevation_mm), "mm", FIELD_W) {
                p.queue(Some(Action { id: "vessel.datum", params: json!({"plant_elevation_mm": v}) }));
            }
        }
        let mut b = g.diameter_basis;
        egui::ComboBox::from_id_salt("triya_basis").selected_text(if b == DiameterBasis::Inside { "ID basis" } else { "OD basis" }).show_ui(
            ui,
            |ui| {
                ui.selectable_value(&mut b, DiameterBasis::Inside, "ID basis");
                ui.selectable_value(&mut b, DiameterBasis::Outside, "OD basis");
            },
        );
        if b != g.diameter_basis {
            p.queue(Some(edits::global_field("diameter_basis", json!(b))));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Material");
        if let Some(m) = p.text_field(ui, "global/material", &g.default_material, "", 110.0) {
            p.queue(Some(edits::global_field("default_material", json!(m))));
        }
        ui.label("CA");
        if let Some(Some(v)) = p.num_field(ui, "global/ca", Some(g.corrosion_allowance_mm), "mm", 40.0) {
            p.queue(Some(edits::global_field("corrosion_allowance_mm", json!(v))));
        }
        ui.label("Insulation");
        if let Some(Some(v)) = p.num_field(ui, "global/ins", Some(g.insulation_thickness_mm), "mm", 40.0) {
            p.queue(Some(edits::global_field("insulation_thickness_mm", json!(v))));
        }
    });
}

/// The selected element and where its edits go.
struct Ed<'a> {
    doc: &'a triya_vessel::VesselDoc,
    e: &'a Element,
    actions: &'a mut Vec<Option<Action>>,
}

/// An optional numeric field of the selected element (`None` clears it), labelled.
fn opt_num(p: &mut VesselPanel, ui: &mut egui::Ui, ed: &mut Ed<'_>, label: &str, field: &[&str], value: Option<f64>, hint: &str) {
    ui.label(label);
    let key = format!("{}/{}", ed.e.id, field.join("/"));
    if let Some(v) = p.num_field(ui, &key, value, hint, FIELD_W) {
        ed.actions.push(edits::element_field(ed.doc, &ed.e.id, field, v.map(|x| json!(x))));
    }
}

fn detail(p: &mut VesselPanel, ui: &mut egui::Ui, doc: &triya_vessel::VesselDoc, e: &Element, actions: &mut Vec<Option<Action>>) {
    let t = Tokens::get();
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} ·", e.id)).strong());
        let mut kind = e.kind.name().to_string();
        egui::ComboBox::from_id_salt("triya_detail_kind").selected_text(kind_label(&kind)).show_ui(ui, |ui| {
            for k in ELEMENT_KINDS {
                ui.selectable_value(&mut kind, k.to_string(), kind_label(k));
            }
        });
        if kind != e.kind.name() {
            actions.push(edits::change_kind(doc, p.resolved(), &e.id, &kind));
        }
    });
    let mut ed = Ed { doc, e, actions };
    egui::Grid::new("triya_detail_grid").spacing([6.0, 3.0]).show(ui, |ui| {
        match &e.kind {
            ElementKind::Head { preset, straight_flange_mm, crown_radius_mm, knuckle_radius_mm, .. } => {
                ui.label("Preset");
                let mut x = *preset;
                egui::ComboBox::from_id_salt("triya_head_preset").selected_text(preset_label(x)).show_ui(ui, |ui| {
                    for p in HeadPresetId::ALL {
                        ui.selectable_value(&mut x, p, preset_label(p));
                    }
                });
                if x != *preset {
                    ed.actions.push(edits::element_field(doc, &e.id, &["preset"], Some(json!(x))));
                }
                ui.end_row();
                let sf = preset.default_straight_flange(e.thickness_mm);
                opt_num(p, ui, &mut ed, "Straight flange", &["straight_flange_mm"], *straight_flange_mm, &fmt_num(sf));
                ui.end_row();
                if *preset == HeadPresetId::CustomTorispherical || crown_radius_mm.is_some() || knuckle_radius_mm.is_some() {
                    opt_num(p, ui, &mut ed, "Crown radius", &["crown_radius_mm"], *crown_radius_mm, "inside");
                    ui.end_row();
                    opt_num(p, ui, &mut ed, "Knuckle radius", &["knuckle_radius_mm"], *knuckle_radius_mm, "inside");
                    ui.end_row();
                }
            }
            ElementKind::Cone { diameter_small_mm, diameter_large_mm, knuckle_large_mm, knuckle_small_mm, .. } => {
                opt_num(p, ui, &mut ed, "Large end D", &["diameter_large_mm"], Some(*diameter_large_mm), "");
                ui.end_row();
                opt_num(p, ui, &mut ed, "Small end D", &["diameter_small_mm"], Some(*diameter_small_mm), "");
                ui.end_row();
                opt_num(p, ui, &mut ed, "Knuckle (large)", &["knuckle_large_mm"], *knuckle_large_mm, "none");
                ui.end_row();
                opt_num(p, ui, &mut ed, "Knuckle (small)", &["knuckle_small_mm"], *knuckle_small_mm, "none");
                ui.end_row();
            }
            ElementKind::Skirt { base_ring, top_offset_mm, .. } => {
                opt_num(p, ui, &mut ed, "Base ring OD", &["base_ring", "od_mm"], Some(base_ring.od_mm), "");
                ui.end_row();
                opt_num(p, ui, &mut ed, "Base ring ID", &["base_ring", "id_mm"], Some(base_ring.id_mm), "");
                ui.end_row();
                opt_num(p, ui, &mut ed, "Base ring t", &["base_ring", "thickness_mm"], Some(base_ring.thickness_mm), "");
                ui.end_row();
                opt_num(p, ui, &mut ed, "Top below TL", &["top_offset_mm"], *top_offset_mm, "auto");
                ui.end_row();
            }
            ElementKind::FlatCover { bolted } => {
                ui.label("Bolted");
                let mut b = *bolted;
                if ui.checkbox(&mut b, "").changed() {
                    ed.actions.push(edits::element_field(doc, &e.id, &["bolted"], Some(json!(b))));
                }
                ui.end_row();
            }
            ElementKind::BodyFlange { flange_ref, explicit, gasket_gap_mm } => {
                if let Some(f) = flange_ref {
                    ui.label("Table row");
                    ui.label(format!("{} NPS {} Cl {:?} {:?}", f.standard, f.nps_or_dn, f.class, f.kind));
                    ui.end_row();
                    ui.label("NPS / DN");
                    if let Some(v) = p.text_field(ui, &format!("{}/flange_nps", e.id), &f.nps_or_dn, "", FIELD_W) {
                        ed.actions.push(edits::element_field(doc, &e.id, &["flange_ref", "nps_or_dn"], Some(json!(v))));
                    }
                    ui.end_row();
                }
                if let Some(x) = explicit {
                    opt_num(p, ui, &mut ed, "Flange OD", &["explicit", "od_mm"], Some(x.od_mm), "");
                    ui.end_row();
                    opt_num(p, ui, &mut ed, "Bolt circle", &["explicit", "bolt_circle_mm"], Some(x.bolt_circle_mm), "");
                    ui.end_row();
                    opt_num(p, ui, &mut ed, "Flange t", &["explicit", "thickness_mm"], Some(x.thickness_mm), "");
                    ui.end_row();
                }
                opt_num(p, ui, &mut ed, "Gasket gap", &["gasket_gap_mm"], Some(*gasket_gap_mm), "");
                ui.end_row();
            }
            ElementKind::Cylinder { .. } => {}
        }
        ui.label("Material");
        let current = e.material.clone().unwrap_or_default();
        if let Some(m) = p.text_field(ui, &format!("{}/material", e.id), &current, &doc.global.default_material, 140.0) {
            let v = if m.trim().is_empty() { None } else { Some(Value::from(m)) };
            ed.actions.push(edits::element_field(doc, &e.id, &["material"], v));
        }
        ui.end_row();
        ui.label("Description");
        let current = e.description.clone().unwrap_or_default();
        if let Some(m) = p.text_field(ui, &format!("{}/description", e.id), &current, "", 180.0) {
            let v = if m.trim().is_empty() { None } else { Some(Value::from(m)) };
            ed.actions.push(edits::element_field(doc, &e.id, &["description"], v));
        }
        ui.end_row();
    });
    ui.label(RichText::new("Empty D inherits the previous element's; every change is one undo step.").color(t.text_dim).size(11.0));
}
