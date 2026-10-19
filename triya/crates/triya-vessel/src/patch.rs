// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Triya CAD 3D contributors
//! JSON Patch (RFC 6902) on the vessel document, for `vessel.set {patch}`: the stack editor and
//! the attachment panel send one small patch per edit (`replace /elements/2/length_mm`), so an
//! edit made in the UI is the same JSON a script or an MCP client would send. Pointers follow
//! RFC 6901 (`~1` is `/`, `~0` is `~`, `-` appends to an array). Ops: `add`, `remove`,
//! `replace`, `move`, `copy`, `test`. All or nothing: the result is validated as a whole
//! document before it is stored.

use serde_json::Value;

/// At most this many operations in one patch.
pub const MAX_OPS: usize = 256;
/// At most this many pointer segments.
const MAX_DEPTH: usize = 32;
/// Longest pointer accepted (bytes).
const MAX_POINTER: usize = 1024;

/// The segments of an RFC 6901 pointer (`""` is the whole document).
pub fn segments(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.len() > MAX_POINTER {
        return Err(format!("a pointer is at most {MAX_POINTER} bytes"));
    }
    if pointer.is_empty() {
        return Ok(Vec::new());
    }
    let rest = pointer.strip_prefix('/').ok_or_else(|| format!("pointer `{}` must start with `/`", short(pointer)))?;
    let segs: Vec<String> = rest.split('/').map(|s| s.replace("~1", "/").replace("~0", "~")).collect();
    if segs.len() > MAX_DEPTH {
        return Err(format!("a pointer has at most {MAX_DEPTH} segments"));
    }
    Ok(segs)
}

/// Escape one segment for a pointer.
pub fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn short(s: &str) -> String {
    s.chars().take(60).collect()
}

fn index(seg: &str, len: usize, allow_end: bool) -> Result<usize, String> {
    if allow_end && seg == "-" {
        return Ok(len);
    }
    if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) || (seg.len() > 1 && seg.starts_with('0')) {
        return Err(format!("`{}` is not an array index", short(seg)));
    }
    let i: usize = seg.parse().map_err(|_| format!("`{}` is not an array index", short(seg)))?;
    let max = if allow_end { len } else { len.saturating_sub(1) };
    if i > max || (!allow_end && len == 0) {
        return Err(format!("index {i} is out of range (length {len})"));
    }
    Ok(i)
}

fn get<'a>(doc: &'a Value, segs: &[String]) -> Result<&'a Value, String> {
    let mut at = doc;
    for s in segs {
        at = match at {
            Value::Object(m) => m.get(s).ok_or_else(|| format!("no member `{}`", short(s)))?,
            Value::Array(a) => a.get(index(s, a.len(), false)?).ok_or_else(|| format!("no index `{}`", short(s)))?,
            _ => return Err(format!("`{}` is inside a value that is not an object or array", short(s))),
        };
    }
    Ok(at)
}

fn parent<'a>(doc: &'a mut Value, segs: &[String]) -> Result<(&'a mut Value, String), String> {
    let (last, path) = segs.split_last().ok_or_else(|| "the whole document cannot be the target here".to_string())?;
    let mut at = doc;
    for s in path {
        at = match at {
            Value::Object(m) => m.get_mut(s).ok_or_else(|| format!("no member `{}`", short(s)))?,
            Value::Array(a) => {
                let i = index(s, a.len(), false)?;
                a.get_mut(i).ok_or_else(|| format!("no index `{}`", short(s)))?
            }
            _ => return Err(format!("`{}` is inside a value that is not an object or array", short(s))),
        };
    }
    Ok((at, last.clone()))
}

fn add(doc: &mut Value, segs: &[String], value: Value) -> Result<(), String> {
    if segs.is_empty() {
        *doc = value;
        return Ok(());
    }
    let (p, last) = parent(doc, segs)?;
    match p {
        Value::Object(m) => {
            m.insert(last, value);
        }
        Value::Array(a) => {
            let i = index(&last, a.len(), true)?;
            a.insert(i, value);
        }
        _ => return Err(format!("cannot add `{}` to a value that is not an object or array", short(&last))),
    }
    Ok(())
}

fn remove(doc: &mut Value, segs: &[String]) -> Result<Value, String> {
    let (p, last) = parent(doc, segs)?;
    match p {
        Value::Object(m) => m.remove(&last).ok_or_else(|| format!("no member `{}`", short(&last))),
        Value::Array(a) => {
            let i = index(&last, a.len(), false)?;
            Ok(a.remove(i))
        }
        _ => Err(format!("cannot remove `{}` from a value that is not an object or array", short(&last))),
    }
}

fn pointer_of<'a>(op: &'a Value, key: &str) -> Result<&'a str, String> {
    op.get(key).and_then(Value::as_str).ok_or_else(|| format!("operation needs `{key}` (a JSON pointer)"))
}

/// Apply `patch` (an array of operations) to `doc`, in order; on any error `doc` is unchanged.
pub fn apply(doc: &Value, patch: &Value) -> Result<Value, String> {
    let ops = patch.as_array().ok_or_else(|| "`patch` must be an array of operations".to_string())?;
    if ops.len() > MAX_OPS {
        return Err(format!("a patch has at most {MAX_OPS} operations"));
    }
    let mut out = doc.clone();
    for (n, op) in ops.iter().enumerate() {
        let at = |e: String| format!("patch[{n}]: {e}");
        let name = op.get("op").and_then(Value::as_str).ok_or_else(|| at("needs `op`".into()))?;
        let path = segments(pointer_of(op, "path").map_err(at)?).map_err(at)?;
        let value = || op.get("value").cloned().ok_or_else(|| at("needs `value`".into()));
        match name {
            "add" => add(&mut out, &path, value()?).map_err(at)?,
            "remove" => {
                remove(&mut out, &path).map_err(at)?;
            }
            "replace" => {
                let v = value()?;
                if path.is_empty() {
                    out = v;
                } else {
                    remove(&mut out, &path).map_err(at)?;
                    add(&mut out, &path, v).map_err(at)?;
                }
            }
            "move" | "copy" => {
                let from = segments(pointer_of(op, "from").map_err(at)?).map_err(at)?;
                if name == "move" && path.len() > from.len() && path[..from.len()] == from[..] {
                    return Err(at("cannot move a value into itself".into()));
                }
                let v = if name == "move" { remove(&mut out, &from).map_err(at)? } else { get(&out, &from).map_err(at)?.clone() };
                add(&mut out, &path, v).map_err(at)?;
            }
            "test" => {
                if *get(&out, &path).map_err(at)? != value()? {
                    return Err(at("test failed".into()));
                }
            }
            other => return Err(at(format!("unknown op `{}`", short(other)))),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rfc_6902_operations() {
        let doc = json!({"a": [1, 2, 3], "b": {"c/d": 1, "e~f": 2}});
        let ok = |p: Value| apply(&doc, &p).unwrap();
        assert_eq!(ok(json!([{"op": "replace", "path": "/a/1", "value": 9}]))["a"], json!([1, 9, 3]));
        assert_eq!(ok(json!([{"op": "add", "path": "/a/-", "value": 4}]))["a"], json!([1, 2, 3, 4]));
        assert_eq!(ok(json!([{"op": "add", "path": "/a/0", "value": 0}]))["a"], json!([0, 1, 2, 3]));
        assert_eq!(ok(json!([{"op": "remove", "path": "/a/0"}]))["a"], json!([2, 3]));
        assert_eq!(ok(json!([{"op": "move", "from": "/a/0", "path": "/a/2"}]))["a"], json!([2, 3, 1]));
        assert_eq!(ok(json!([{"op": "copy", "from": "/a/0", "path": "/x"}]))["x"], json!(1));
        assert_eq!(ok(json!([{"op": "replace", "path": "/b/c~1d", "value": 5}]))["b"]["c/d"], json!(5));
        assert_eq!(ok(json!([{"op": "remove", "path": "/b/e~0f"}]))["b"], json!({"c/d": 1}));
        assert_eq!(ok(json!([{"op": "test", "path": "/a/2", "value": 3}])), doc);
        for bad in [
            json!([{"op": "test", "path": "/a/2", "value": 4}]),
            json!([{"op": "replace", "path": "/a/3", "value": 1}]),
            json!([{"op": "replace", "path": "/a/01", "value": 1}]),
            json!([{"op": "remove", "path": "/zz"}]),
            json!([{"op": "replace", "path": "/zz", "value": 1}]),
            json!([{"op": "move", "from": "/b", "path": "/b/x"}]),
            json!([{"op": "add", "path": "a", "value": 1}]),
            json!([{"op": "frob", "path": "/a"}]),
            json!([{"op": "add", "path": "/a/0/x", "value": 1}]),
            json!({"op": "add"}),
            json!(vec![json!({"op": "test", "path": "", "value": 1}); MAX_OPS + 1]),
        ] {
            assert!(apply(&doc, &bad).is_err(), "{bad}");
        }
        // All or nothing.
        assert!(apply(&doc, &json!([{"op": "remove", "path": "/a"}, {"op": "remove", "path": "/a"}])).is_err());
        assert_eq!(escape("c/d~"), "c~1d~0");
    }
}
