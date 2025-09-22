use crate::binfmt::{Document, Value};
use crate::json::JsonOpts;
use serde_json::json;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum JsonEditValue {
    Int(i64),
    Bool(bool),
    Str(String),
    Float(f64),
    Null,
}

impl From<&JsonEditValue> for serde_json::Value {
    fn from(v: &JsonEditValue) -> Self {
        match v {
            JsonEditValue::Int(n) => serde_json::Value::Number((*n).into()),
            JsonEditValue::Bool(b) => serde_json::Value::Bool(*b),
            JsonEditValue::Str(s) => serde_json::Value::String(s.clone()),
            JsonEditValue::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            JsonEditValue::Null => serde_json::Value::Null,
        }
    }
}

pub fn parse_file_to_json_value(path: &Path, opts: JsonOpts) -> Result<serde_json::Value, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    match data.iter().copied().find(|b| !b.is_ascii_whitespace()) {
        Some(b'{') => serde_json::from_slice::<serde_json::Value>(&data).map_err(|e| e.to_string()),
        Some(_) => {
            let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
            let mut parser = crate::binfmt::Parser::new(leaked);
            match parser.parse_stream() {
                Ok(doc) => Ok(document_to_json_value(&doc, opts)),
                Err(e) => Err(e),
            }
        }
        None => Err("empty file".to_string()),
    }
}

pub fn document_to_json_value(doc: &Document<'_>, opts: JsonOpts) -> serde_json::Value {
    fn write_value(doc: &Document<'_>, v: &Value<'_>, depth: usize, opts: &JsonOpts) -> serde_json::Value {
        match v {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::I32(x) => json!(*x),
            Value::I64(x) => json!(*x),
            Value::U32(x) => json!(*x),
            Value::U64(x) => json!(*x),
            Value::F32(x) => json!(*x),
            Value::F64(x) => json!(*x),
            Value::U8(x) => json!(*x),
            Value::Str(s) => json!(*s),
            Value::Bytes(b) => if opts.bytes_summary { json!({"$type":"bytes","len": b.len()}) } else { serde_json::Value::Null },
            Value::Array(items) => {
                let max = opts.max_array_elems.min(items.len());
                let mut arr = Vec::with_capacity(max + 1);
                for it in items.iter().take(max) {
                    if depth >= opts.max_depth { arr.push(serde_json::Value::Null); }
                    else { arr.push(write_value(doc, it, depth + 1, opts)); }
                }
                if items.len() > max {
                    arr.push(json!({"$truncated": true, "$omitted": items.len() - max }));
                }
                serde_json::Value::Array(arr)
            }
            Value::Object(obj) => {
                let mut map = serde_json::Map::with_capacity(obj.members.len() + 1);
                map.insert("$class".to_string(), json!(obj.class_name));
                for (name, val) in obj.members.iter() {
                    let vv = if depth >= opts.max_depth { serde_json::Value::Null } else { write_value(doc, val, depth + 1, opts) };
                    map.insert((*name).to_string(), vv);
                }
                serde_json::Value::Object(map)
            }
        Value::Ref(id) => {
            if let Some(Value::Object(obj)) = doc.get_object(*id)
                && obj.class_name.starts_with("System.Collections.Generic.List`1")
                    && let Some((_, Value::Ref(items_id))) = obj.members.iter().find(|(name, _)| *name == "_items")
                        && let Some(items) = doc.get_object(*items_id) {
                            return write_value(doc, items, depth, opts);
                        }
            let mut map = serde_json::Map::new();
            map.insert("$ref".to_string(), json!(*id));
            if depth < opts.max_depth && let Some(v2) = doc.get_object(*id) {
                map.insert("$value".to_string(), write_value(doc, v2, depth + 1, opts));
            }
            serde_json::Value::Object(map)
        }
        }
    }

    let mut root = serde_json::Map::new();
    root.insert("$rootClass".to_string(), json!(doc.root_class_name().unwrap_or("<unknown>")));
    if let Some(v) = doc.root_value() {
        root.insert("root".to_string(), write_value(doc, v, 1, &opts));
    } else {
        root.insert("root".to_string(), serde_json::Value::Null);
    }
    serde_json::Value::Object(root)
}

pub fn set_by_pointer(value: &mut serde_json::Value, pointer: &str, new_value: JsonEditValue) -> Result<(), String> {
    match value.pointer_mut(pointer) {
        Some(slot) => { *slot = (&new_value).into(); Ok(()) }
        None => Err(format!("json pointer not found: {}", pointer)),
    }
}

pub fn get_by_pointer(value: &serde_json::Value, pointer: &str) -> Option<serde_json::Value> {
    value.pointer(pointer).cloned()
}

pub fn list_object_primitives_at(value: &serde_json::Value, pointer: &str) -> Result<Vec<(String, JsonEditValue)>, String> {
    let node = value.pointer(pointer).ok_or_else(|| format!("json pointer not found: {}", pointer))?;
    let obj = node.as_object().ok_or_else(|| "target is not an object".to_string())?;
    let mut out = Vec::new();
    for (k, v) in obj {
        let jv = match v {
            serde_json::Value::Bool(b) => Some(JsonEditValue::Bool(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() { Some(JsonEditValue::Int(i)) }
                else { n.as_f64().map(JsonEditValue::Float) }
            }
            serde_json::Value::String(s) => Some(JsonEditValue::Str(s.clone())),
            _ => None,
        };
        if let Some(v) = jv { out.push((k.clone(), v)); }
    }
    Ok(out)
}

pub fn apply_object_primitive_updates(value: &mut serde_json::Value, pointer: &str, updates: &[(String, JsonEditValue)]) -> Result<(), String> {
    let node = value.pointer_mut(pointer).ok_or_else(|| format!("json pointer not found: {}", pointer))?;
    let obj = node.as_object_mut().ok_or_else(|| "target is not an object".to_string())?;
    for (k, v) in updates {
        if let Some(slot) = obj.get_mut(k) {
            *slot = v.into();
        }
    }
    Ok(())
}

pub fn write_json_to_file(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let s = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, s).map_err(|e| e.to_string())
}

// -------- Extra generic helpers for tree browsing and editing --------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonKind { Null, Bool, Number, String, Object, Array }

#[derive(Debug, Clone)]
pub struct ChildInfo { pub key_or_index: String, pub kind: JsonKind, pub len: Option<usize> }

fn kind_of(v: &serde_json::Value) -> JsonKind {
    match v {
        serde_json::Value::Null => JsonKind::Null,
        serde_json::Value::Bool(_) => JsonKind::Bool,
        serde_json::Value::Number(_) => JsonKind::Number,
        serde_json::Value::String(_) => JsonKind::String,
        serde_json::Value::Object(_) => JsonKind::Object,
        serde_json::Value::Array(_) => JsonKind::Array,
    }
}

pub fn list_children(value: &serde_json::Value, pointer: &str) -> Result<Vec<ChildInfo>, String> {
    let node = value.pointer(pointer).ok_or_else(|| format!("json pointer not found: {}", pointer))?;
    let mut out = Vec::new();
    match node {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter() {
                let len = match v { serde_json::Value::Array(a) => Some(a.len()), serde_json::Value::Object(m) => Some(m.len()), _ => None };
                out.push(ChildInfo { key_or_index: k.clone(), kind: kind_of(v), len });
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let len = match v { serde_json::Value::Array(a) => Some(a.len()), serde_json::Value::Object(m) => Some(m.len()), _ => None };
                out.push(ChildInfo { key_or_index: i.to_string(), kind: kind_of(v), len });
            }
        }
        _ => {}
    }
    Ok(out)
}

fn unescape_token(tok: &str) -> String {
    let s = tok.replace("~1", "/");
    s.replace("~0", "~")
}

fn parent_pointer(ptr: &str) -> Option<(&str, &str)> {
    if ptr.is_empty() || ptr == "/" { return None; }
    if let Some(pos) = ptr.rfind('/') { if pos == 0 { return Some(("", &ptr[1..])); } else { return Some((&ptr[..pos], &ptr[pos+1..])); } }
    None
}

pub fn set_raw_by_pointer(root: &mut serde_json::Value, pointer: &str, new_value: serde_json::Value) -> Result<(), String> {
    match root.pointer_mut(pointer) { Some(slot) => { *slot = new_value; Ok(()) } None => Err(format!("json pointer not found: {}", pointer)) }
}

pub fn add_key(root: &mut serde_json::Value, obj_pointer: &str, key: &str, value: serde_json::Value) -> Result<(), String> {
    let node = root.pointer_mut(obj_pointer).ok_or_else(|| format!("json pointer not found: {}", obj_pointer))?;
    let obj = node.as_object_mut().ok_or_else(|| "target is not an object".to_string())?;
    obj.insert(key.to_string(), value);
    Ok(())
}

pub fn remove_at_pointer(root: &mut serde_json::Value, pointer: &str) -> Result<(), String> {
    let (parent_ptr, last) = parent_pointer(pointer).ok_or_else(|| "cannot remove at root".to_string())?;
    let last = unescape_token(last);
    let parent = root.pointer_mut(parent_ptr).ok_or_else(|| format!("json pointer not found: {}", parent_ptr))?;
    match parent {
        serde_json::Value::Object(map) => {
            if map.remove(&last).is_some() { Ok(()) } else { Err("key not found".into()) }
        }
        serde_json::Value::Array(arr) => {
            let idx: usize = last.parse().map_err(|_| "array index invalid".to_string())?;
            if idx >= arr.len() { return Err("array index out of bounds".into()); }
            arr.remove(idx);
            Ok(())
        }
        _ => Err("parent is neither object nor array".into()),
    }
}

pub fn array_insert(root: &mut serde_json::Value, arr_pointer: &str, index: usize, value: serde_json::Value) -> Result<(), String> {
    let node = root.pointer_mut(arr_pointer).ok_or_else(|| format!("json pointer not found: {}", arr_pointer))?;
    let arr = node.as_array_mut().ok_or_else(|| "target is not an array".to_string())?;
    if index > arr.len() { return Err("array index out of bounds".into()); }
    arr.insert(index, value);
    Ok(())
}

pub fn array_remove(root: &mut serde_json::Value, arr_pointer: &str, index: usize) -> Result<(), String> {
    let node = root.pointer_mut(arr_pointer).ok_or_else(|| format!("json pointer not found: {}", arr_pointer))?;
    let arr = node.as_array_mut().ok_or_else(|| "target is not an array".to_string())?;
    if index >= arr.len() { return Err("array index out of bounds".into()); }
    arr.remove(index);
    Ok(())
}
// Generic JSON-pointer editing utilities over serde_json::Value.
// Highlights:
// - RFC 6901 JSON Pointer addressing (`/root/a/b/0`).
// - Inspect: `get_by_pointer`, `list_children`, `list_object_primitives_at`.
// - Modify: `set_by_pointer`, `set_raw_by_pointer`, `add_key`, `remove_at_pointer`,
//   `array_insert`, `array_remove`.
// - `JsonEditValue` covers common scalars; use `set_raw_by_pointer` for full JSON.
// Intended to be UI-friendly and generic — no domain-specific keys.
