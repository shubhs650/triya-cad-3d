// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The attachment panel (PLAN T5.3): nozzles, supports and details with their host, axial
//! reference (from the datum, or from the host's start), azimuth with snapping (22.5° on a shell,
//! 5° on a head), frame, the nozzle's pipe, flange, projection and pad, the `standard_ref` and
//! `size_key` of a preset, and the generator's flags. Picking a vessel body in the 3D view picks
//! its element or attachment here. Every edit is one [`edits`] action, so adding a nozzle here
//! sends exactly the JSON an MCP client would.

use egui::RichText;
use serde_json::{Value, json};
use solidcraft_ui_egui::SolveApp;
use solidcraft_ui_egui::theme::Tokens;
use triya_vessel::edit::ATTACHMENT_KINDS;
use triya_vessel::model::{Attachment, AttachmentKind, AxialRef, Frame, VesselDoc};

use super::VesselPanel;
use super::edits::{self, Action, fmt_num};

const FIELD_W: f32 = 64.0;

/// The element or attachment id a vessel body belongs to: `Vessel E2`, `Vessel N1 flange`,
/// `Vessel SD1 base plate` → `E2`, `N1`, `SD1`.
pub fn owner_of(body: &str) -> Option<&str> {
    body.strip_prefix("Vessel ")?.split(' ').next().filter(|s| !s.is_empty())
}

/// The body the 3D selection names first, if any.
fn selected_body(app: &SolveApp) -> Option<String> {
    use solidcraft_engine::Sel;
    app.session.selection.iter().find_map(|s| match s {
        Sel::Body { name } => Some(name.clone()),
        Sel::Face { body, .. } | Sel::Edge { body, .. } => Some(body.clone()),
        _ => None,
    })
}

/// Follow the 3D selection: a picked vessel body selects its element (the host for the next
/// attachment) or its attachment.
pub fn follow_selection(p: &mut VesselPanel, app: &SolveApp) {
    let body = selected_body(app);
    if body == p.last_selection {
        return;
    }
    p.last_selection.clone_from(&body);
    if p.no_follow {
        return;
    }
    let (Some(doc), Some(body)) = (&p.doc, body) else { return };
    let Some(id) = owner_of(&body) else { return };
    if doc.element(id).is_some() {
        p.selected_element = Some(id.to_string());
    } else if doc.attachments.iter().any(|a| a.id == id) {
        p.selected_attachment = Some(id.to_string());
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "nozzle" => "Nozzle",
        "saddle" => "Saddle",
        "leg" => "Legs",
        "bracket" => "Brackets",
        "anchor_chair" => "Anchor chairs",
        "stiffening_ring" => "Stiffening ring",
        "limpet_coil" => "Limpet coil",
        "lifting_lug" => "Lifting lug",
        "insulation_ring" => "Insulation ring",
        "clip" => "Clip",
        _ => "?",
    }
}

fn axial_text(a: &Attachment) -> String {
    match a.axial.reference {
        AxialRef::Datum => format!("@{}", fmt_num(a.axial.mm)),
        AxialRef::HostFrom => format!("{}+{}", a.host, fmt_num(a.axial.mm)),
    }
}

fn frame_name(f: &Frame) -> &'static str {
    match f {
        Frame::Radial => "radial",
        Frame::Hillside { .. } => "hillside",
        Frame::Tangential { .. } => "tangential",
        Frame::Tilted { .. } => "tilted",
        Frame::ParallelToAxis { .. } => "parallel_to_axis",
        Frame::HeadRadialAtR { .. } => "head_radial_at_r",
    }
}

fn frame_label(name: &str) -> &'static str {
    match name {
        "radial" => "Radial",
        "hillside" => "Hillside",
        "tangential" => "Tangential",
        "tilted" => "Tilted",
        "parallel_to_axis" => "Parallel to axis (head)",
        "head_radial_at_r" => "Normal to head at R",
        _ => "?",
    }
}

const FRAMES: [&str; 6] = ["radial", "hillside", "tangential", "tilted", "parallel_to_axis", "head_radial_at_r"];

/// A frame of type `name` with its parameter at a starting value.
fn default_frame(name: &str) -> Value {
    match name {
        "hillside" => json!({"type": "hillside", "offset_mm": 100.0}),
        "tangential" => json!({"type": "tangential"}),
        "tilted" => json!({"type": "tilted", "tilt_deg": 15.0}),
        "parallel_to_axis" => json!({"type": "parallel_to_axis", "radius_mm": 0.0}),
        "head_radial_at_r" => json!({"type": "head_radial_at_r", "radius_mm": 0.0}),
        _ => json!({"type": "radial"}),
    }
}

/// A combo box over `options`; returns the pick when it differs from `current`.
fn pick(ui: &mut egui::Ui, salt: &str, current: &str, options: &[(&str, &str)]) -> Option<String> {
    let mut x = current.to_string();
    let shown = options.iter().find(|(v, _)| *v == current).map_or(current, |(_, l)| *l);
    egui::ComboBox::from_id_salt(salt).selected_text(shown).show_ui(ui, |ui| {
        for (v, l) in options {
            ui.selectable_value(&mut x, v.to_string(), *l);
        }
    });
    (x != current).then_some(x)
}

pub fn show(p: &mut VesselPanel, app: &SolveApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    if p.pipe_sizes.is_empty() {
        p.pipe_sizes = pipe_sizes(app);
    }
    let Some(doc) = p.doc.clone() else {
        ui.label("The design has no vessel. New makes a one-cylinder vessel to start from.");
        return;
    };
    let mut actions: Vec<Option<Action>> = Vec::new();
    ui.horizontal(|ui| {
        let mut follow = !p.no_follow;
        ui.checkbox(&mut follow, "Follow 3D selection").on_hover_text("Picking a vessel body in the view selects its element or attachment here");
        p.no_follow = !follow;
        let mut snap = !p.no_snap;
        ui.checkbox(&mut snap, "Snap angles").on_hover_text("22.5° on shells, 5° on heads");
        p.no_snap = !snap;
    });
    egui::ScrollArea::vertical().id_salt("triya_attach_rows").max_height(280.0).show(ui, |ui| {
        egui::Grid::new("triya_attach_grid").striped(true).spacing([6.0, 3.0]).show(ui, |ui| {
            for h in ["id", "kind", "host", "axial", "angle", "flags"] {
                ui.label(RichText::new(h).color(t.text_dim).size(11.0));
            }
            ui.end_row();
            for a in &doc.attachments {
                let selected = p.selected_attachment.as_deref() == Some(a.id.as_str());
                if ui.selectable_label(selected, RichText::new(&a.id).strong()).on_hover_text("Show its fields below").clicked() {
                    p.selected_attachment = Some(a.id.clone());
                }
                ui.label(kind_label(a.kind.name()));
                ui.label(&a.host);
                ui.label(axial_text(a));
                ui.label(format!("{}°", fmt_num(a.angle_deg)));
                ui.horizontal(|ui| {
                    if a.flags.boolean_failed {
                        ui.label(RichText::new("separate").color(t.warning))
                            .on_hover_text("The kernel's join failed: built as a separate body (see the BOM)");
                    }
                    if a.flags.unverified_table {
                        ui.label(RichText::new("unverified").color(t.warning)).on_hover_text("A dimension came from a table row not marked verified");
                    }
                });
                ui.end_row();
            }
        });
    });
    // Add on the selected element (or the command's default host).
    ui.horizontal(|ui| {
        let key = egui::Id::new("triya_attach_new_kind");
        let mut kind: String = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| "nozzle".into());
        egui::ComboBox::from_id_salt("triya_attach_kind").selected_text(kind_label(&kind)).show_ui(ui, |ui| {
            for k in ATTACHMENT_KINDS {
                ui.selectable_value(&mut kind, k.to_string(), kind_label(k));
            }
        });
        ui.data_mut(|d| d.insert_temp(key, kind.clone()));
        let host = p.selected_element.clone().filter(|h| doc.element(h).is_some());
        let label = match &host {
            Some(h) => format!("Add on {h}"),
            None => "Add".into(),
        };
        if ui.button(label).on_hover_text("vessel.attachment.add: defaults from the host and the presets").clicked() {
            actions.push(Some(edits::add_attachment(&kind, host.as_deref())));
        }
    });
    if let Some(id) = p.selected_attachment.clone()
        && let Some(a) = doc.attachments.iter().find(|a| a.id == id).cloned()
    {
        ui.separator();
        detail(p, app, ui, &doc, &a, &mut actions);
    }
    for a in actions {
        p.queue(a);
    }
}

/// The selected attachment and where its edits go.
struct Ed<'a> {
    doc: &'a VesselDoc,
    a: &'a Attachment,
    actions: &'a mut Vec<Option<Action>>,
}

impl Ed<'_> {
    fn set(&mut self, field: &[&str], v: Option<Value>) {
        self.actions.push(edits::attachment_field(self.doc, &self.a.id, field, v));
    }
}

fn text(p: &mut VesselPanel, ui: &mut egui::Ui, ed: &mut Ed<'_>, label: &str, field: &[&str], value: Option<&str>, width: f32) {
    ui.label(label);
    let key = format!("{}/{}", ed.a.id, field.join("/"));
    if let Some(v) = p.text_field(ui, &key, value.unwrap_or(""), "", width) {
        let v = v.trim().to_string();
        ed.set(field, (!v.is_empty()).then(|| json!(v)));
    }
}

fn detail(p: &mut VesselPanel, app: &SolveApp, ui: &mut egui::Ui, doc: &VesselDoc, a: &Attachment, actions: &mut Vec<Option<Action>>) {
    let t = Tokens::get();
    let resolved = p.resolved().cloned();
    let step = if p.no_snap { 0.0 } else { edits::snap_step(doc, &a.host) };
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("{} · {}", a.id, kind_label(a.kind.name()))).strong());
        if ui.small_button("Delete").clicked() {
            actions.push(edits::delete_attachment(doc, &a.id));
            p.selected_attachment = None;
        }
    });
    if a.flags.boolean_failed {
        ui.label(RichText::new("The last regenerate could not join this into its host: it is a separate body.").color(t.warning));
    }
    if a.flags.unverified_table {
        ui.label(RichText::new("A dimension came from a table row not marked verified.").color(t.warning));
    }
    let mut ed = Ed { doc, a, actions };
    egui::Grid::new("triya_attach_detail").spacing([6.0, 3.0]).show(ui, |ui| {
        // Host, synced with the 3D selection.
        ui.label("Host");
        ui.horizontal(|ui| {
            let hosts: Vec<(&str, &str)> = doc.elements.iter().map(|e| (e.id.as_str(), e.id.as_str())).collect();
            if let (Some(h), Some(r)) = (pick(ui, "triya_attach_host", &a.host, &hosts), resolved.as_ref()) {
                ed.actions.push(edits::rehost(doc, r, &a.id, &h));
            }
            let picked = selected_body(app).and_then(|b| owner_of(&b).map(str::to_string)).filter(|id| doc.element(id).is_some() && *id != a.host);
            if let (Some(h), Some(r)) = (picked, resolved.as_ref())
                && ui.small_button(format!("Use {h} (3D selection)")).clicked()
            {
                ed.actions.push(edits::rehost(doc, r, &a.id, &h));
            }
        });
        ui.end_row();
        ui.label("Axial");
        ui.horizontal(|ui| {
            let current = if a.axial.reference == AxialRef::Datum { "datum" } else { "host_from" };
            if let (Some(to), Some(r)) =
                (pick(ui, "triya_attach_ref", current, &[("datum", "from the datum"), ("host_from", "from the host's start")]), resolved.as_ref())
            {
                let to = if to == "datum" { AxialRef::Datum } else { AxialRef::HostFrom };
                ed.actions.push(edits::axial_ref(doc, r, &a.id, to));
            }
            let key = format!("{}/axial/mm", a.id);
            if let Some(Some(x)) = p.num_field(ui, &key, Some(a.axial.mm), "mm", FIELD_W) {
                ed.set(&["axial", "mm"], Some(json!(x)));
            }
        });
        ui.end_row();
        ui.label("Angle");
        ui.horizontal(|ui| {
            let key = format!("{}/angle_deg", a.id);
            if let Some(Some(x)) = p.num_field(ui, &key, Some(a.angle_deg), "deg", 52.0) {
                ed.set(&["angle_deg"], Some(json!(edits::snap(x, step))));
            }
            let by = if step > 0.0 { step } else { 1.0 };
            if ui.small_button("<").on_hover_text(format!("−{}°", fmt_num(by))).clicked() {
                ed.set(&["angle_deg"], Some(json!(edits::snap(a.angle_deg - by, step))));
            }
            if ui.small_button(">").on_hover_text(format!("+{}°", fmt_num(by))).clicked() {
                ed.set(&["angle_deg"], Some(json!(edits::snap(a.angle_deg + by, step))));
            }
            if step > 0.0 {
                ui.label(RichText::new(format!("snaps to {}°", fmt_num(step))).color(t.text_dim).size(11.0));
            }
        });
        ui.end_row();
        ui.label("Frame");
        ui.horizontal(|ui| {
            let frames: Vec<(&str, &str)> = FRAMES.iter().map(|f| (*f, frame_label(f))).collect();
            if let Some(f) = pick(ui, "triya_attach_frame", frame_name(&a.frame), &frames) {
                ed.set(&["frame"], Some(default_frame(&f)));
            }
            let (field, value, clearable) = match a.frame {
                Frame::Hillside { offset_mm } => ("offset_mm", Some(offset_mm), false),
                Frame::Tangential { offset_mm } => ("offset_mm", offset_mm, true),
                Frame::Tilted { tilt_deg } => ("tilt_deg", Some(tilt_deg), false),
                Frame::ParallelToAxis { radius_mm } | Frame::HeadRadialAtR { radius_mm } => ("radius_mm", Some(radius_mm), false),
                Frame::Radial => ("", None, false),
            };
            if !field.is_empty() {
                let key = format!("{}/frame/{field}", a.id);
                match p.num_field(ui, &key, value, if clearable { "auto" } else { "" }, FIELD_W) {
                    Some(Some(x)) => ed.set(&["frame", field], Some(json!(x))),
                    Some(None) if clearable => ed.set(&["frame", field], None),
                    _ => {}
                }
            }
        });
        ui.end_row();
        if let AttachmentKind::Nozzle { .. } = &a.kind {
            nozzle(p, ui, &mut ed);
        } else {
            preset(p, app, ui, &mut ed);
        }
        text(p, ui, &mut ed, "Material", &["material"], a.material.as_deref(), 140.0);
        ui.end_row();
        text(p, ui, &mut ed, "Description", &["description"], a.description.as_deref(), 180.0);
        ui.end_row();
    });
    // Everything else (a saddle's plates, a coil's pitch, …) as the attachment's own JSON.
    egui::CollapsingHeader::new("All fields (JSON)").id_salt(("triya_attach_json", &a.id)).show(ui, |ui| {
        let key = format!("{}/json", a.id);
        let current = serde_json::to_string_pretty(a).unwrap_or_default();
        let mut buf = p.editing.get(&key).cloned().unwrap_or_else(|| current.clone());
        if ui.add(egui::TextEdit::multiline(&mut buf).code_editor().desired_width(f32::INFINITY).desired_rows(8)).changed() {
            p.editing.insert(key.clone(), buf.clone());
        }
        if ui.button("Apply").on_hover_text("vessel.set: replace this attachment with the JSON above").clicked() {
            match serde_json::from_str::<Value>(&buf) {
                Ok(v) => {
                    if let Some(j) = doc.attachments.iter().position(|x| x.id == a.id) {
                        ed.actions.push(Some(Action {
                            id: "vessel.set",
                            params: json!({"patch": [{"op": "replace", "path": edits::pointer(&["attachments", &j.to_string()]), "value": v}]}),
                        }));
                    }
                    p.editing.remove(&key);
                }
                Err(e) => p.error = Some(format!("JSON: {e}")),
            }
        }
    });
}

/// NPS sizes of the shipped pipe tables (smallest first) and each size's schedules.
fn pipe_sizes(app: &SolveApp) -> Vec<(String, Vec<String>)> {
    let Ok(tables) = triya_vessel::presets::tables(&app.session) else { return Vec::new() };
    let mut sizes: Vec<(String, f64, Vec<String>)> = Vec::new();
    for r in tables.pipe_rows() {
        match sizes.iter_mut().find(|(n, ..)| *n == r.nps) {
            Some((_, _, s)) => {
                if !s.contains(&r.schedule) {
                    s.push(r.schedule.clone());
                }
            }
            None => sizes.push((r.nps.clone(), r.od, vec![r.schedule.clone()])),
        }
    }
    sizes.sort_by(|a, b| a.1.total_cmp(&b.1));
    sizes.into_iter().map(|(n, _, s)| (n, s)).collect()
}

fn nozzle(p: &mut VesselPanel, ui: &mut egui::Ui, ed: &mut Ed<'_>) {
    let AttachmentKind::Nozzle { nps, schedule, neck_thk_mm, flange, projection, inside_projection_mm, attach, pad, manway, service, .. } =
        &ed.a.kind
    else {
        return;
    };
    ui.label("Pipe");
    ui.horizontal(|ui| {
        let sizes: Vec<(String, String)> = p.pipe_sizes.iter().map(|(n, _)| (n.clone(), format!("NPS {n}"))).collect();
        let opts: Vec<(&str, &str)> = sizes.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        if let Some(n) = pick(ui, "triya_nozzle_nps", nps, &opts) {
            ed.set(&["nps"], Some(json!(n)));
        }
        let schedules: Vec<String> = p.pipe_sizes.iter().find(|(n, _)| n == nps).map(|(_, s)| s.clone()).unwrap_or_default();
        let sched_opts: Vec<(&str, &str)> = schedules.iter().map(|s| (s.as_str(), s.as_str())).collect();
        let current = schedule.as_deref().unwrap_or("");
        if let Some(s) = pick(ui, "triya_nozzle_sched", current, &sched_opts) {
            ed.set(&["schedule"], Some(json!(s)));
        }
        ui.label("or wall");
        let key = format!("{}/neck_thk_mm", ed.a.id);
        match p.num_field(ui, &key, *neck_thk_mm, "sch", 44.0) {
            Some(Some(x)) => ed.set(&["neck_thk_mm"], Some(json!(x))),
            Some(None) => ed.set(&["neck_thk_mm"], None),
            None => {}
        }
    });
    ui.end_row();
    ui.label("Flange");
    ui.horizontal(|ui| match flange {
        Some(f) => {
            let std = serde_json::to_value(f.standard).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            if let Some(s) = pick(ui, "triya_flange_std", &std, &[("B16.5", "B16.5"), ("B16.47A", "B16.47 A"), ("B16.47B", "B16.47 B")]) {
                ed.set(&["flange", "standard"], Some(json!(s)));
            }
            let class = f.class.to_string();
            let classes = [
                ("150", "Cl 150"),
                ("300", "Cl 300"),
                ("400", "Cl 400"),
                ("600", "Cl 600"),
                ("900", "Cl 900"),
                ("1500", "Cl 1500"),
                ("2500", "Cl 2500"),
            ];
            if let Some(c) = pick(ui, "triya_flange_class", &class, &classes)
                && let Ok(c) = c.parse::<u32>()
            {
                ed.set(&["flange", "class"], Some(json!(c)));
            }
            let kind = serde_json::to_value(f.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            let kinds = [("wn", "WN"), ("so", "SO"), ("lwn", "LWN"), ("blind", "Blind"), ("lj", "LJ"), ("sw", "SW")];
            if let Some(k) = pick(ui, "triya_flange_type", &kind, &kinds) {
                ed.set(&["flange", "type"], Some(json!(k)));
            }
            let facing = serde_json::to_value(f.facing).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
            if let Some(x) = pick(ui, "triya_flange_facing", &facing, &[("rf", "RF"), ("ff", "FF"), ("rtj", "RTJ")]) {
                ed.set(&["flange", "facing"], Some(json!(x)));
            }
            if ui.small_button("None").on_hover_text("A plain pipe end").clicked() {
                ed.set(&["flange"], None);
            }
        }
        None => {
            if ui.small_button("Add flange").clicked() {
                ed.set(&["flange"], Some(json!({"standard": "B16.5", "class": 150, "type": "wn", "facing": "rf"})));
            }
        }
    });
    ui.end_row();
    ui.label("Projection");
    ui.horizontal(|ui| {
        let from = serde_json::to_value(projection.from).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        if let Some(f) =
            pick(ui, "triya_proj_from", &from, &[("cl", "from the vessel CL"), ("od", "from the shell OD"), ("insulation", "from the insulation")])
        {
            ed.set(&["projection", "from"], Some(json!(f)));
        }
        let key = format!("{}/projection/mm", ed.a.id);
        if let Some(Some(x)) = p.num_field(ui, &key, Some(projection.mm), "mm", FIELD_W) {
            ed.set(&["projection", "mm"], Some(json!(x)));
        }
        ui.label("inside");
        let key = format!("{}/inside_projection_mm", ed.a.id);
        if let Some(Some(x)) = p.num_field(ui, &key, Some(*inside_projection_mm), "mm", 44.0) {
            ed.set(&["inside_projection_mm"], Some(json!(x)));
        }
    });
    ui.end_row();
    ui.label("Attach");
    ui.horizontal(|ui| {
        let current = serde_json::to_value(attach).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        if let Some(x) = pick(ui, "triya_nozzle_attach", &current, &[("set_in", "Set in"), ("set_on", "Set on")]) {
            ed.set(&["attach"], Some(json!(x)));
        }
        let mut has_pad = pad.is_some();
        if ui.checkbox(&mut has_pad, "Pad").changed() {
            ed.set(&["pad"], has_pad.then(|| json!({"width_mm": 100.0, "thickness_mm": 10.0})));
        }
        if let Some(pd) = pad {
            let key = format!("{}/pad/width_mm", ed.a.id);
            if let Some(Some(x)) = p.num_field(ui, &key, Some(pd.width_mm), "width", 44.0) {
                ed.set(&["pad", "width_mm"], Some(json!(x)));
            }
            let key = format!("{}/pad/thickness_mm", ed.a.id);
            if let Some(Some(x)) = p.num_field(ui, &key, Some(pd.thickness_mm), "t", 36.0) {
                ed.set(&["pad", "thickness_mm"], Some(json!(x)));
            }
        }
        let mut is_manway = manway.is_some();
        if ui.checkbox(&mut is_manway, "Manway").changed() {
            ed.set(&["manway"], is_manway.then(|| json!({"davit": false})));
        }
        if let Some(m) = manway {
            let mut davit = m.davit;
            if ui.checkbox(&mut davit, "davit").changed() {
                ed.set(&["manway", "davit"], Some(json!(davit)));
            }
        }
    });
    ui.end_row();
    text(p, ui, ed, "Service", &["service"], service.as_deref(), 140.0);
    ui.end_row();
}

/// `standard_ref` + `size_key` of a support or detail, with the preset keys the tables have.
fn preset(p: &mut VesselPanel, app: &SolveApp, ui: &mut egui::Ui, ed: &mut Ed<'_>) {
    let kind = ed.a.kind.name();
    ui.label("Size key");
    ui.horizontal(|ui| {
        let key = format!("{}/size_key", ed.a.id);
        if let Some(v) = p.text_field(ui, &key, ed.a.size_key.as_deref().unwrap_or(""), "OD1600", 80.0) {
            let v = v.trim().to_string();
            ed.set(&["size_key"], (!v.is_empty()).then(|| json!(v)));
        }
        let keys: Vec<String> = triya_vessel::presets::tables(&app.session)
            .map(|t| t.supports.iter().flat_map(|tb| tb.rows.iter()).filter(|r| r.kind == kind).map(|r| r.size_key.clone()).collect())
            .unwrap_or_default();
        if !keys.is_empty() {
            egui::ComboBox::from_id_salt("triya_size_keys").selected_text("presets").show_ui(ui, |ui| {
                for k in keys.iter().take(200) {
                    if ui.selectable_label(ed.a.size_key.as_deref() == Some(k.as_str()), k).clicked() {
                        ed.set(&["size_key"], Some(json!(k)));
                    }
                }
            });
        }
    });
    ui.end_row();
    ui.label("Standard");
    ui.horizontal(|ui| {
        let r = ed.a.standard_ref.as_ref().map(|r| serde_json::to_value(r).unwrap_or_default()).unwrap_or_default();
        for (field, hint, w) in [("org", "org", 52.0), ("number", "number", 80.0), ("rev", "rev", 30.0)] {
            let key = format!("{}/standard_ref/{field}", ed.a.id);
            let current = r.get(field).and_then(Value::as_str).unwrap_or("");
            if let Some(v) = p.text_field(ui, &key, current, hint, w) {
                let mut next = if r.is_object() { r.clone() } else { json!({"org": "", "number": "", "rev": ""}) };
                if let Some(o) = next.as_object_mut() {
                    o.insert(field.to_string(), json!(v.trim()));
                }
                let empty = ["org", "number", "rev"].iter().all(|k| next.get(*k).and_then(Value::as_str).is_none_or(str::is_empty));
                ed.set(&["standard_ref"], (!empty).then_some(next));
            }
        }
    });
    ui.end_row();
}

#[cfg(test)]
mod tests {
    #[test]
    fn bodies_name_their_owner() {
        assert_eq!(super::owner_of("Vessel E2"), Some("E2"));
        assert_eq!(super::owner_of("Vessel N1 flange"), Some("N1"));
        assert_eq!(super::owner_of("Vessel SD1 base plate"), Some("SD1"));
        assert_eq!(super::owner_of("Body1"), None);
    }
}
