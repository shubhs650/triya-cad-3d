// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! T7.1: the vessel JSON fuzzed through the hostile harness. Each case mutates a reference vessel
//! (random and degenerate values, wrong types, removed keys, duplicated or reordered array items)
//! and sends it through everything that reads a vessel without the kernel: `vessel.new`,
//! `vessel.set` (whole document and JSON patch), `vessel.inspect`, `vessel.bom`,
//! `drawing.generate_ga` and `drawing.export` (svg, dxf, pdf), plus the library calls the UI makes
//! directly (parse, validate, resolve, plan). An `EngineError::Internal` is a panic the engine
//! caught; a library panic is caught here. Both fail the test. `vessel.regenerate` is left out: it
//! drives the kernel (minutes per vessel) and its inputs are the plans checked here.
//!
//! `TRIYA_FUZZ_CASES` overrides the number of cases (default 10 000), `TRIYA_FUZZ_SEED` the seed.

use serde_json::{Map, Value, json};
use solidcraft_engine::{EngineError, Session};
use std::panic::{AssertUnwindSafe, catch_unwind};
use triya_vessel::model::{MAX_ATTACHMENTS, MAX_ELEMENTS, MAX_ID_LEN, MAX_LENGTH_MM, VesselDoc};

fn registered() {
    solidcraft_engine::cmd::triya_extra::register(triya_vessel::COMMANDS).unwrap();
    solidcraft_engine::cmd::triya_extra::register(triya_drawing::COMMANDS).unwrap();
}

fn references() -> Vec<Value> {
    [
        include_str!("../../triya-vessel/tests/reference/vessel-a.json"),
        include_str!("../../triya-vessel/tests/reference/vessel-b.json"),
        include_str!("../../triya-vessel/tests/reference/vessel-c.json"),
        include_str!("../../triya-vessel/tests/reference/every-kind.json"),
    ]
    .iter()
    .map(|s| serde_json::from_str(s).unwrap())
    .collect()
}

/// SplitMix64: small, seedable, good enough to pick mutations (no dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Half the time a plausible change of a number (which the validator lets through, so it reaches
/// the geometry: odd but legal sizes, angles, positions); otherwise a hostile value.
fn value(rng: &mut Rng, old: &Value) -> Value {
    match old.as_f64() {
        Some(x) if rng.below(2) == 0 => match rng.below(4) {
            0 => json!(rng.unit() * 360.0),
            1 => json!((x * (0.05 + 3.0 * rng.unit())).round()),
            2 => json!(x + (rng.unit() - 0.5) * 2000.0),
            _ => json!(x * (0.9 + 0.2 * rng.unit())),
        },
        _ => hostile(rng, old),
    }
}

/// Degenerate values, and some near the documented caps.
fn hostile(rng: &mut Rng, old: &Value) -> Value {
    let scaled = |f: f64| old.as_f64().map_or(json!(f), |x| json!(x * f));
    match rng.below(24) {
        0 => Value::Null,
        1 => json!(true),
        2 => json!(0),
        3 => json!(-1),
        4 => json!(1e-12),
        5 => json!(1e308),
        6 => json!(-1e308),
        7 => json!(MAX_LENGTH_MM),
        8 => json!(MAX_LENGTH_MM * 1.0001),
        9 => json!(u64::MAX),
        10 => json!(""),
        11 => json!("x".repeat(MAX_ID_LEN + 1)),
        12 => json!("nope"),
        13 => json!([]),
        14 => json!({}),
        15 => json!([1e308, -1e308]),
        16 => scaled(0.0),
        17 => scaled(-1.0),
        18 => scaled(1e-6),
        19 => scaled(1e6),
        20 => scaled(0.5 + rng.unit()),
        21 => scaled(10.0 * rng.unit()),
        22 => json!(rng.unit() * 720.0 - 360.0),
        _ => json!("$ref:0.sketch"),
    }
}

/// Every path in the tree (pointer segments), the root excluded.
fn paths(v: &Value, at: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match v {
        Value::Object(m) => {
            for (k, c) in m {
                at.push(k.clone());
                out.push(at.clone());
                paths(c, at, out);
                at.pop();
            }
        }
        Value::Array(a) => {
            for (i, c) in a.iter().enumerate() {
                at.push(i.to_string());
                out.push(at.clone());
                paths(c, at, out);
                at.pop();
            }
        }
        _ => {}
    }
}

fn get_mut<'a>(v: &'a mut Value, path: &[String]) -> Option<&'a mut Value> {
    path.iter().try_fold(v, |v, seg| match v {
        Value::Object(m) => m.get_mut(seg),
        Value::Array(a) => seg.parse::<usize>().ok().and_then(|i| a.get_mut(i)),
        _ => None,
    })
}

fn pointer(path: &[String]) -> String {
    path.iter().map(|s| format!("/{}", s.replace('~', "~0").replace('/', "~1"))).collect()
}

/// One mutation of `doc` in place; also returns it as an RFC 6902 operation on the original.
fn mutate(rng: &mut Rng, doc: &mut Value) -> Value {
    let mut all = Vec::new();
    paths(doc, &mut Vec::new(), &mut all);
    // Numbers are what reach the geometry: three tries to land on one.
    let mut pick = all.get(rng.below(all.len())).cloned();
    for _ in 0..3 {
        if pick.as_ref().and_then(|p| get_mut(doc, p)).is_some_and(|v| v.is_number()) {
            break;
        }
        pick = all.get(rng.below(all.len())).cloned();
    }
    let Some(path) = pick else { return json!({"op": "test", "path": "", "value": {}}) };
    let Some((last, parent_path)) = path.split_last() else { return json!({"op": "test", "path": "", "value": {}}) };
    let op = rng.below(10);
    let Some(parent) = get_mut(doc, parent_path) else { return json!({"op": "test", "path": "", "value": {}}) };
    match (op, parent) {
        // Remove the key or the item.
        (0, Value::Object(m)) => {
            m.remove(last);
            json!({"op": "remove", "path": pointer(&path)})
        }
        (0, Value::Array(a)) => {
            if let Some(i) = last.parse::<usize>().ok().filter(|i| *i < a.len()) {
                a.remove(i);
            }
            json!({"op": "remove", "path": pointer(&path)})
        }
        // Duplicate an array item (duplicate ids, two hosts with the same name, …).
        (1, Value::Array(a)) => {
            let i = last.parse::<usize>().unwrap_or(0).min(a.len().saturating_sub(1));
            let item = a.get(i).cloned().unwrap_or(Value::Null);
            let at = rng.below(a.len() + 1);
            a.insert(at, item.clone());
            json!({"op": "add", "path": pointer(&[parent_path, &[at.to_string()]].concat()), "value": item})
        }
        // Swap two array items (stack order, attachments before their host).
        (2, Value::Array(a)) if a.len() > 1 => {
            let i = last.parse::<usize>().unwrap_or(0).min(a.len() - 1);
            let j = rng.below(a.len());
            a.swap(i, j);
            json!({"op": "move", "from": pointer(&path), "path": pointer(&[parent_path, &[j.to_string()]].concat())})
        }
        // Add an unknown key next to it.
        (3, Value::Object(m)) => {
            let v = hostile(rng, &Value::Null);
            m.insert("fuzz".into(), v.clone());
            json!({"op": "add", "path": pointer(&[parent_path, &["fuzz".to_string()]].concat()), "value": v})
        }
        // Replace the value (most cases: numbers become degenerate, types change).
        (_, parent) => {
            let slot = match parent {
                Value::Object(m) => m.get_mut(last),
                Value::Array(a) => last.parse::<usize>().ok().and_then(|i| a.get_mut(i)),
                _ => None,
            };
            let Some(slot) = slot else { return json!({"op": "test", "path": "", "value": {}}) };
            let v = value(rng, slot);
            *slot = v.clone();
            json!({"op": "replace", "path": pointer(&path), "value": v})
        }
    }
}

#[derive(Default)]
struct Tally {
    cases: usize,
    accepted: usize,
    planned: usize,
    drawn: usize,
    patched: usize,
    panics: Vec<String>,
}

impl Tally {
    fn check(&mut self, case: usize, what: &str, r: &Result<Value, EngineError>) -> bool {
        match r {
            Err(EngineError::Internal(id, msg)) => {
                self.panics.push(format!("case {case} {what}: {id}: {msg}"));
                false
            }
            Err(_) => false,
            Ok(_) => true,
        }
    }
}

/// The library path the UI uses without the engine: parse → validate → resolve → plan.
fn library(doc: &Value) -> Result<bool, String> {
    catch_unwind(AssertUnwindSafe(|| {
        let Ok(d) = VesselDoc::from_value(doc) else { return false };
        if d.validate().is_err() {
            return false;
        }
        let Ok(r) = triya_vessel::resolve(&d) else { return false };
        triya_vessel::plan(&r).is_ok()
    }))
    .map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default())
}

#[test]
fn fuzzed_vessel_json_never_panics() {
    registered();
    let cases = std::env::var("TRIYA_FUZZ_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(10_000usize);
    let seed = std::env::var("TRIYA_FUZZ_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x7121_7A0D_u64);
    let refs = references();
    let mut rng = Rng(seed);
    let mut t = Tally::default();
    let started = std::time::Instant::now();
    for case in 0..cases {
        t.cases += 1;
        let base = refs.get(case % refs.len()).cloned().unwrap_or(Value::Null);
        let mut doc = base.clone();
        let n = 1 + rng.below(3);
        let ops: Vec<Value> = (0..n).map(|_| mutate(&mut rng, &mut doc)).collect();

        match library(&doc) {
            Err(msg) => t.panics.push(format!("case {case} library: {msg}")),
            Ok(true) => t.planned += 1,
            Ok(false) => {}
        }

        // The same change as a JSON patch on a good vessel (what the panels send).
        let mut p = Session::default();
        if p.execute("vessel.new", &json!({"doc": base})).is_ok() {
            let r = p.execute("vessel.set", &json!({"patch": ops}));
            if t.check(case, "vessel.set patch", &r) {
                t.patched += 1;
            }
            let r = p.execute("vessel.inspect", &json!({}));
            t.check(case, "vessel.inspect after patch", &r);
        }

        let mut s = Session::default();
        let new = s.execute("vessel.new", &json!({"doc": doc}));
        if !t.check(case, "vessel.new", &new) {
            continue;
        }
        t.accepted += 1;
        for (id, p) in [("vessel.inspect", json!({})), ("vessel.bom", json!({})), ("vessel.set", json!({"doc": doc}))] {
            let r = s.execute(id, &p);
            t.check(case, id, &r);
        }
        let g = s.execute("drawing.generate_ga", &json!({}));
        if t.check(case, "drawing.generate_ga", &g) {
            t.drawn += 1;
            // Exporting every case to PDF too costs little (the writer is ours, no fonts).
            let format = ["svg", "dxf", "pdf"].get(case % 3).copied().unwrap_or("svg");
            let r = s.execute("drawing.export", &json!({"format": format}));
            t.check(case, "drawing.export", &r);
        }
    }
    eprintln!(
        "fuzz: {} cases in {:.1} s (seed {seed}): {} accepted by vessel.new, {} planned, {} drawn, {} patches applied, {} panics",
        t.cases,
        started.elapsed().as_secs_f64(),
        t.accepted,
        t.planned,
        t.drawn,
        t.patched,
        t.panics.len()
    );
    assert!(t.panics.is_empty(), "{} panics:\n{}", t.panics.len(), t.panics.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    // The fuzz must reach the deep code, not stop at the parser.
    if cases >= 1000 {
        assert!(t.accepted * 10 >= cases, "only {} of {cases} mutated vessels were accepted", t.accepted);
        assert!(t.drawn * 20 >= cases, "only {} of {cases} mutated vessels were drawn", t.drawn);
    }
}

/// The documented caps are enforced (each one alone, on an otherwise good vessel).
#[test]
fn caps_are_enforced() {
    registered();
    let a = references().into_iter().next().unwrap_or(Value::Null);
    let with = |f: &dyn Fn(&mut Map<String, Value>)| {
        let mut d = a.clone();
        if let Some(m) = d.as_object_mut() {
            f(m);
        }
        d
    };
    let one_element = a["elements"][2].clone();
    let one_attachment = a["attachments"][0].clone();
    let too_many_elements = with(&|m| {
        let items = (0..=MAX_ELEMENTS).map(|i| {
            let mut e = one_element.clone();
            e["id"] = json!(format!("E{i}"));
            e
        });
        m.insert("elements".into(), Value::Array(items.collect()));
    });
    let too_many_attachments = with(&|m| {
        m.insert("attachments".into(), Value::Array(vec![one_attachment.clone(); MAX_ATTACHMENTS + 1]));
    });
    let long_id = with(&|m| {
        m.insert("id".into(), json!("V".repeat(MAX_ID_LEN + 1)));
    });
    let long_course = with(&|m| {
        if let Some(e) = m.get_mut("elements").and_then(|e| e.get_mut(2)).and_then(Value::as_object_mut) {
            e.insert("length_mm".into(), json!(MAX_LENGTH_MM * 2.0));
        }
    });
    for (what, doc) in [("elements", too_many_elements), ("attachments", too_many_attachments), ("id length", long_id), ("length", long_course)] {
        let mut s = Session::default();
        let r = s.execute("vessel.new", &json!({"doc": doc}));
        assert!(matches!(r, Err(EngineError::Other(_)) | Err(EngineError::BadParams { .. })), "{what}: {r:?}");
    }
}
