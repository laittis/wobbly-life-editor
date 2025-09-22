use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use crate::binfmt::{Document, Parser, Value};

#[derive(Clone, Copy)]
pub struct JsonOpts {
    pub max_array_elems: usize,
    pub max_depth: usize,
    pub bytes_summary: bool,
}

impl Default for JsonOpts {
    fn default() -> Self {
        Self {
            max_array_elems: 128,
            max_depth: 16,
            bytes_summary: true,
        }
    }
}

pub fn parse_binary(path: &Path) -> Result<Document<'static>, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
    let mut parser = Parser::new(leaked);
    parser.parse_stream()
}

pub fn dump_file_json(path: &Path, opts: JsonOpts) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    match data.iter().copied().find(|b| !b.is_ascii_whitespace()) {
        Some(b'{') => std::str::from_utf8(&data)
            .map(|s| s.to_string())
            .map_err(|_| "non-utf8 text".to_string()),
        Some(_) => {
            let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
            let mut parser = Parser::new(leaked);
            match parser.parse_stream() {
                Ok(doc) => Ok(dump_dynamic_json(&doc, opts)),
                Err(e) => Err(e),
            }
        }
        None => Err("empty file".to_string()),
    }
}

pub fn dump_dynamic_json(doc: &Document<'_>, opts: JsonOpts) -> String {
    let mut out = String::new();
    write!(
        &mut out,
        "{{\n  \"$rootClass\": \"{}\",\n  \"root\": ",
        doc.root_class_name().unwrap_or("<unknown>")
    )
    .ok();
    if let Some(v) = doc.root_value() {
        write_value_json(doc, v, 1, &mut out, &opts).ok();
    } else {
        out.push_str("null");
    }
    out.push_str("\n}\n");
    out
}

fn write_value_json(
    wctx: &Document<'_>,
    v: &Value<'_>,
    depth: usize,
    out: &mut String,
    opts: &JsonOpts,
) -> std::fmt::Result {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => write!(out, "{}", if *b { "true" } else { "false" })?,
        Value::I32(x) => write!(out, "{}", x)?,
        Value::I64(x) => write!(out, "{}", x)?,
        Value::U32(x) => write!(out, "{}", x)?,
        Value::U64(x) => write!(out, "{}", x)?,
        Value::F32(x) => write!(out, "{}", x)?,
        Value::F64(x) => write!(out, "{}", x)?,
        Value::U8(x) => write!(out, "{}", x)?,
        Value::Str(s) => write!(out, "\"{}\"", escape_json(s))?,
        Value::Bytes(b) => {
            if opts.bytes_summary {
                write!(out, "{{\"$type\":\"bytes\",\"len\":{}}}", b.len())?;
            } else {
                out.push('[');
                for (i, by) in b.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write!(out, "{}", *by)?;
                }
                out.push(']');
            }
        }
        Value::Array(items) => {
            out.push('[');
            let max = opts.max_array_elems.min(items.len());
            for (i, it) in items.iter().enumerate().take(max) {
                if i > 0 {
                    out.push(',');
                }
                if depth >= opts.max_depth {
                    out.push_str("null");
                } else {
                    write_value_json(wctx, it, depth + 1, out, opts)?;
                }
            }
            if items.len() > max {
                write!(
                    out,
                    ",{{\"$truncated\":true,\"$omitted\":{}}}",
                    items.len() - max
                )?;
            }
            out.push(']');
        }
        Value::Object(obj) => {
            out.push('{');
            write!(out, "\"$class\":\"{}\"", escape_json(obj.class_name))?;
            for (name, val) in obj.members.iter() {
                out.push(',');
                write!(out, "\"{}\":", escape_json(name))?;
                if depth >= opts.max_depth {
                    out.push_str("null");
                } else {
                    write_value_json(wctx, val, depth + 1, out, opts)?;
                }
            }
            out.push('}');
        }
        Value::Ref(id) => {
            out.push('{');
            write!(out, "\"$ref\":{}", id)?;
            if depth < opts.max_depth
                && let Some(v2) = wctx.get_object(*id)
            {
                out.push_str(",\"$value\":");
                write_value_json(wctx, v2, depth + 1, out, opts)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write;
                write!(&mut out, "\\u{:04x}", c as u32).ok();
            }
            c => out.push(c),
        }
    }
    out
}

// Directory helpers
pub fn find_sav_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("sav") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

pub fn dump_dir_map_json(dir: &Path, opts: JsonOpts) -> Result<String, String> {
    let mut out = String::new();
    out.push_str("{\n");
    let files = find_sav_files(dir);
    for (i, f) in files.iter().enumerate() {
        let name = f.file_name().and_then(|s| s.to_str()).unwrap_or("file");
        if i > 0 {
            out.push_str(",\n");
        }
        write!(&mut out, "  \"{}\": ", name).ok();
        match dump_file_json(f, opts) {
            Ok(s) => out.push_str(&s),
            Err(e) => {
                write!(&mut out, "{{\"$error\":\"{}\"}}", escape_json(&e)).ok();
            }
        }
    }
    out.push_str("\n}\n");
    Ok(out)
}
