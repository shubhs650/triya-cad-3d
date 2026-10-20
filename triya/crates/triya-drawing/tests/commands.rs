// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! The `drawing.*` commands through the engine (T6.7): generate and export on a reference
//! vessel, one undo step, and the sibling of upstream's `hostile_params_never_panic` (upstream's
//! test cannot see runtime-registered commands).

use serde_json::{Value, json};
use solidcraft_engine::{EngineError, Session};
use triya_drawing::commands::{self, SHEET_KEY};

fn registered() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    solidcraft_engine::cmd::triya_extra::register(triya_drawing::COMMANDS).unwrap();
}

fn reference_a() -> Value {
    serde_json::from_str(include_str!("../../triya-vessel/tests/reference/vessel-a.json")).unwrap()
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, &p).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

#[test]
fn generate_and_export_the_ga_of_vessel_a() {
    registered();
    let mut s = Session::default();
    assert!(s.execute("drawing.generate_ga", &json!({})).is_err(), "no vessel yet");
    run(&mut s, "vessel.new", json!({"doc": reference_a()}));
    let undo = s.undo.len();
    let g = run(&mut s, "drawing.generate_ga", json!({"title_block": {"checked_by": "QA"}}));
    assert_eq!(g["size"], json!("A1"));
    assert_eq!(g["scale"], json!("1:30"));
    assert_eq!(g["views"].as_array().map(Vec::len), Some(3));
    assert_eq!(g["tables"].as_array().map(Vec::len), Some(5));
    assert!(g.get("sheet").is_none());
    assert_eq!(s.doc.metadata.get(SHEET_KEY).and_then(|v| v["options"]["title_block"]["checked_by"].as_str()), Some("QA"));
    assert_eq!(s.undo.len(), undo + 1, "one undo step");

    // Export rebuilds the sheet from the stored options: the checker is on it.
    let svg = run(&mut s, "drawing.export", json!({}));
    assert_eq!(svg["format"], json!("svg"));
    assert_eq!(svg["filename"], json!("V-101-GA-001.svg"));
    let text = svg["text"].as_str().unwrap();
    assert!(text.starts_with("<svg") && text.contains(">QA</text>"));
    let dxf = run(&mut s, "drawing.export", json!({"format": "dxf"}));
    assert!(dxf["text"].as_str().unwrap().contains("\nOUTLINE\n"));
    let pdf = run(&mut s, "drawing.export", json!({"format": "pdf"}));
    assert!(pdf["base64"].as_str().unwrap().starts_with("JVBERi0xLjQ"), "%PDF-1.4");
    let model = run(&mut s, "drawing.export", json!({"format": "json"}));
    assert_eq!(model["sheet"]["size"], json!("a1"));
    assert_eq!(s.undo.len(), undo + 1, "exports are not undo steps");

    // To a file, the format from the extension.
    let path = std::env::temp_dir().join(format!("triya-ga-{}.pdf", std::process::id()));
    let p = path.to_string_lossy().to_string();
    let w = run(&mut s, "drawing.export", json!({"path": p}));
    assert_eq!(w["format"], json!("pdf"));
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4") && bytes.len() == w["bytes"].as_u64().unwrap() as usize);
    let _ = std::fs::remove_file(&path);

    // Options: a smaller sheet without the BOM, a forced scale; bad ones are refused.
    let g = run(&mut s, "drawing.generate_ga", json!({"size": "a2", "tables": ["notes"], "scale": "1:50", "sheet": true}));
    assert_eq!((g["size"].as_str(), g["scale"].as_str()), (Some("A2"), Some("1:50")));
    assert_eq!(g["sheet"]["tables"].as_array().map(Vec::len), Some(1));
    for bad in [json!({"size": "B1"}), json!({"views": []}), json!({"views": ["iso"]}), json!({"scale": 0}), json!({"template": "<svg"}), json!(7)] {
        assert!(s.execute("drawing.generate_ga", &bad).is_err(), "{bad}");
    }
    assert!(s.execute("drawing.export", &json!({"format": "dwg"})).is_err());
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(commands::stored_options(&s).size, triya_drawing::model::SheetSize::A1, "undo restores the previous options");
}

#[test]
fn drawing_commands_are_registered_on_the_inspect_panel() {
    registered();
    assert!(solidcraft_engine::cmd::triya_extra::register(triya_drawing::COMMANDS).is_ok(), "idempotent");
    let all = solidcraft_engine::command_specs();
    for spec in triya_drawing::COMMANDS {
        assert_eq!(all.iter().filter(|c| c.id == spec.id).count(), 1, "{}", spec.id);
        assert_eq!((spec.tab, spec.panel), (triya_vessel::commands::TAB, "INSPECT"));
    }
}

fn deep(n: usize) -> Value {
    (0..n).fold(json!(1), |v, _| json!([v]))
}

/// Every drawing command, every documented key, every hostile value: no panic (an
/// `EngineError::Internal` is a panic the engine caught).
#[test]
fn drawing_commands_never_panic_on_hostile_params() {
    registered();
    let known: Vec<&str> = solidcraft_engine::command_specs().iter().map(|c| c.id).filter(|id| id.starts_with("drawing.")).collect();
    let ours: Vec<&str> = triya_drawing::COMMANDS.iter().map(|c| c.id).collect();
    assert_eq!(known, ours, "a drawing command outside COMMANDS would escape this test");
    let keys = ["size", "views", "tables", "scale", "template", "title_block", "sheet", "format", "path"];
    let hostile = [
        Value::Null,
        json!(true),
        json!(-1),
        json!(0),
        json!(1e308),
        json!(-1e308),
        json!(""),
        json!("nope"),
        json!("1:0"),
        json!("1:1e308"),
        json!("/downloads/../../x.svg"),
        json!([]),
        json!(["elevation", "elevation", "plan"]),
        json!(vec![json!("plan"); 5000]),
        json!({}),
        json!({"title": "x".repeat(70 * 1024)}),
        json!({"extra": {"k": 1}}),
        json!("<svg viewBox=\"0 0 1e308 -5\"><text x=\"nan\" font-size=\"1e308\">{{title}}</text><rect width=\"-1\"/></svg>"),
        json!("x".repeat(70 * 1024)),
        deep(300),
    ];
    // A tiny vessel keeps the many exports fast; also a broken stored sheet entry.
    let tiny = json!({"diameter_mm": 500, "thickness_mm": 5, "length_mm": 800});
    let states: Vec<Box<dyn Fn(&mut Session)>> = vec![
        Box::new(|_: &mut Session| {}),
        Box::new(move |s: &mut Session| {
            let _ = s.execute("vessel.new", &tiny);
        }),
        Box::new(|s: &mut Session| {
            let _ = s.execute("vessel.new", &json!({}));
            let mut m = s.doc.metadata.clone();
            let _ = m.set(SHEET_KEY, json!({"options": {"size": "a9", "views": 7, "scale": -1}}));
            s.doc_mut().metadata = m;
        }),
    ];
    let mut internal = Vec::new();
    for spec in triya_drawing::COMMANDS {
        for v in &hostile {
            for k in keys {
                if k == "path" && v.is_string() {
                    continue; // no writing files named by the hostile strings
                }
                for (label, p) in [("keyed", json!({ k: v.clone() })), ("whole", v.clone())] {
                    for (state, prepare) in states.iter().enumerate() {
                        let mut s = Session::default();
                        prepare(&mut s);
                        if let Err(EngineError::Internal(id, msg)) = s.execute(spec.id, &p) {
                            internal.push(format!("{id} {label} state {state} {p}: {msg}"));
                        }
                    }
                }
            }
        }
    }
    assert!(internal.is_empty(), "commands panicked:\n{}", internal.join("\n"));
}
