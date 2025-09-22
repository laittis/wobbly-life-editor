use crate::binfmt::PrimitiveType;
use serde_json::Value as J;

pub fn write_binfmt_from_json(root: &J) -> Result<Vec<u8>, String> {
    // Expect wrapper: { "$rootClass": string, "root": object-or-array }
    let obj = root
        .as_object()
        .ok_or_else(|| "root must be JSON object".to_string())?;
    let root_class = obj
        .get("$rootClass")
        .and_then(|v| v.as_str())
        .unwrap_or("Root");
    let root_val = obj
        .get("root")
        .ok_or_else(|| "missing 'root' field".to_string())?;
    let mut w = Writer::new();
    w.header();
    w.binary_library(
        2,
        "Game, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null",
    );
    w.binary_library(
        3,
        "mscorlib, Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089",
    );
    w.write_root(root_class, root_val)?;
    w.message_end();
    Ok(w.out)
}

pub fn write_binfmt_file_from_json(path: &std::path::Path, root: &J) -> Result<(), String> {
    let data = write_binfmt_from_json(root)?;
    std::fs::write(path, data).map_err(|e| e.to_string())
}

struct Writer {
    out: Vec<u8>,
    next_id: i32,
    next_str_id: i32,
}
impl Writer {
    fn new() -> Self {
        Self {
            out: Vec::with_capacity(1024),
            next_id: 1,
            next_str_id: 100,
        }
    }
    fn push(&mut self, b: u8) {
        self.out.push(b);
    }
    fn write_i32(&mut self, v: i32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn write_i64(&mut self, v: i64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn write_u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    fn write_f64(&mut self, v: f64) {
        self.out.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    fn write_7(&mut self, mut v: usize) {
        while v >= 0x80 {
            self.push(((v as u8) & 0x7F) | 0x80);
            v >>= 7;
        }
        self.push(v as u8);
    }
    fn write_lp_str(&mut self, s: &str) {
        self.write_7(s.len());
        self.out.extend_from_slice(s.as_bytes());
    }

    fn header(&mut self) {
        self.push(0); // SerializedStreamHeader
        self.write_i32(1); // rootId
        self.write_i32(-1); // headerId
        self.write_i32(1); // major
        self.write_i32(0); // minor
    }
    fn binary_library(&mut self, id: i32, name: &str) {
        self.push(12); // BinaryLibrary
        self.write_i32(id);
        self.write_lp_str(name);
    }
    fn message_end(&mut self) {
        self.push(11);
    }

    fn write_root(&mut self, class_name: &str, v: &J) -> Result<(), String> {
        // Encode as ClassWithMembersAndTypes for the root
        let obj_id = self.alloc_obj_id();
        self.push(5); // ClassWithMembersAndTypes
        self.write_i32(obj_id);
        self.write_lp_str(class_name);
        match v {
            J::Object(map) => {
                // Members exclude any special $class key
                let mut pairs: Vec<(&str, &J)> = map
                    .iter()
                    .filter_map(|(k, vv)| {
                        if k == "$class" {
                            None
                        } else {
                            Some((k.as_str(), vv))
                        }
                    })
                    .collect();
                // stable order
                pairs.sort_by(|a, b| a.0.cmp(b.0));
                self.write_i32(pairs.len() as i32);
                for (k, _) in &pairs {
                    self.write_lp_str(k);
                }
                // Types
                for (_, vv) in &pairs {
                    self.push(self.bin_type_code(vv));
                }
                // Extra primitive type info for those that need it
                for (_, vv) in &pairs {
                    if let Some(pt) = self.maybe_prim_type(vv) {
                        self.write_prim_type(pt);
                    }
                }
                // library id for class
                self.write_i32(2);
                // Values
                for (_, vv) in &pairs {
                    self.write_member_value(vv)?;
                }
                Ok(())
            }
            J::Array(arr) => {
                // Represent root as single member "items"
                self.write_i32(1);
                self.write_lp_str("items");
                // Choose array binary type
                if self.is_string_array(arr) {
                    self.push(6);
                }
                // StringArray
                else if let Some(pt) = self.infer_primitive_array_type(arr) {
                    self.push(7);
                    self.write_prim_type(pt);
                } else {
                    self.push(5);
                } // ObjectArray
                self.write_i32(2); // library id
                self.write_array(arr)
            }
            _ => {
                // Represent root as single member "value"
                self.write_i32(1);
                self.write_lp_str("value");
                self.push(self.bin_type_code(v));
                if let Some(pt) = self.maybe_prim_type(v) {
                    self.write_prim_type(pt);
                }
                self.write_i32(2);
                self.write_member_value(v)
            }
        }
    }

    fn bin_type_code(&self, v: &J) -> u8 {
        match v {
            J::Null | J::Bool(_) | J::Number(_) => 0, // Primitive
            J::String(_) => 1,                        // String
            J::Array(a) => {
                if self.is_string_array(a) {
                    6
                } else if self.infer_primitive_array_type(a).is_some() {
                    7
                } else {
                    5
                }
            }
            J::Object(map) => {
                if map.get("$type").and_then(|x| x.as_str()) == Some("bytes") {
                    7
                } else {
                    2
                }
            }
        }
    }
    fn maybe_prim_type(&self, v: &J) -> Option<PrimitiveType> {
        match v {
            J::Null => Some(PrimitiveType::Null),
            J::Bool(_) => Some(PrimitiveType::Boolean),
            J::Number(n) => {
                if n.is_i64() {
                    Some(PrimitiveType::Int64)
                } else if n.is_u64() {
                    Some(PrimitiveType::UInt64)
                } else {
                    Some(PrimitiveType::Double)
                }
            }
            J::String(_) => None,
            J::Array(a) => self.infer_primitive_array_type(a),
            J::Object(map) => {
                if map.get("$type").and_then(|x| x.as_str()) == Some("bytes") {
                    Some(PrimitiveType::Byte)
                } else {
                    None
                }
            }
        }
    }

    fn write_prim_type(&mut self, p: PrimitiveType) {
        let code = match p {
            PrimitiveType::Boolean => 1,
            PrimitiveType::Byte => 2,
            PrimitiveType::Char => 3,
            PrimitiveType::Decimal => 5,
            PrimitiveType::Double => 6,
            PrimitiveType::Int16 => 7,
            PrimitiveType::Int32 => 8,
            PrimitiveType::Int64 => 9,
            PrimitiveType::SByte => 10,
            PrimitiveType::Single => 11,
            PrimitiveType::TimeSpan => 12,
            PrimitiveType::DateTime => 13,
            PrimitiveType::UInt16 => 14,
            PrimitiveType::UInt32 => 15,
            PrimitiveType::UInt64 => 16,
            PrimitiveType::Null => 17,
            PrimitiveType::String => 18,
        };
        self.push(code);
    }

    fn write_member_value(&mut self, v: &J) -> Result<(), String> {
        match v {
            J::Null => Ok(()),
            J::Bool(b) => {
                self.push(if *b { 1 } else { 0 });
                Ok(())
            }
            J::Number(n) => {
                if let Some(i) = n.as_i64() {
                    self.write_i64(i);
                } else if let Some(u) = n.as_u64() {
                    self.write_u64(u);
                } else if let Some(f) = n.as_f64() {
                    self.write_f64(f);
                }
                Ok(())
            }
            J::String(s) => {
                self.write_string_obj(s);
                Ok(())
            }
            J::Array(a) => self.write_array(a),
            J::Object(map) => {
                if map.get("$type").and_then(|x| x.as_str()) == Some("bytes") {
                    // synthesize zeroed byte array of given length (we do not have the raw bytes in summary dumps)
                    let len = map.get("len").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    self.write_primitive_array_u8(&vec![0u8; len]);
                    Ok(())
                } else {
                    let class_name = map
                        .get("$class")
                        .and_then(|x| x.as_str())
                        .unwrap_or("Object");
                    self.write_object(map, class_name)
                }
            }
        }
    }

    fn write_string_obj(&mut self, s: &str) {
        self.push(6); // BinaryObjectString
        let id = self.alloc_str_id();
        self.write_i32(id);
        self.write_lp_str(s);
    }

    fn write_array(&mut self, a: &[J]) -> Result<(), String> {
        if self.is_string_array(a) {
            self.push(17); // ArraySingleString
            let id = self.alloc_obj_id();
            self.write_i32(id);
            self.write_i32(a.len() as i32);
            for v in a {
                let s = v
                    .as_str()
                    .ok_or_else(|| "string array element must be string".to_string())?;
                self.write_string_obj(s);
            }
            Ok(())
        } else if let Some(pt) = self.infer_primitive_array_type(a) {
            match pt {
                PrimitiveType::Byte => {
                    // Serialize as ArraySinglePrimitive(Byte) using element u8 casting
                    let mut bytes: Vec<u8> = Vec::with_capacity(a.len());
                    for v in a {
                        match v {
                            J::Number(n) => {
                                let x = n
                                    .as_u64()
                                    .ok_or_else(|| "byte array element must be u64".to_string())?;
                                bytes.push((x & 0xFF) as u8);
                            }
                            _ => return Err("non-number in byte array".into()),
                        }
                    }
                    self.write_primitive_array_u8(&bytes);
                    Ok(())
                }
                PrimitiveType::Int32 => {
                    self.push(15); // ArraySinglePrimitive
                    let id = self.alloc_obj_id();
                    self.write_i32(id);
                    self.write_i32(a.len() as i32);
                    self.write_prim_type(PrimitiveType::Int32);
                    for v in a {
                        let i = v
                            .as_i64()
                            .ok_or_else(|| "int array element must be i64".to_string())?;
                        self.write_i32(i as i32);
                    }
                    Ok(())
                }
                PrimitiveType::Int64 => {
                    self.push(15);
                    let id = self.alloc_obj_id();
                    self.write_i32(id);
                    self.write_i32(a.len() as i32);
                    self.write_prim_type(PrimitiveType::Int64);
                    for v in a {
                        let i = v
                            .as_i64()
                            .ok_or_else(|| "int64 array element must be i64".to_string())?;
                        self.write_i64(i);
                    }
                    Ok(())
                }
                PrimitiveType::Double => {
                    self.push(15);
                    let id = self.alloc_obj_id();
                    self.write_i32(id);
                    self.write_i32(a.len() as i32);
                    self.write_prim_type(PrimitiveType::Double);
                    for v in a {
                        let f = v
                            .as_f64()
                            .ok_or_else(|| "float array element must be f64".to_string())?;
                        self.write_f64(f);
                    }
                    Ok(())
                }
                _ => {
                    // Fallback to object array
                    self.write_object_array(a)
                }
            }
        } else {
            self.write_object_array(a)
        }
    }

    fn write_object_array(&mut self, a: &[J]) -> Result<(), String> {
        self.push(16); // ArraySingleObject
        let id = self.alloc_obj_id();
        self.write_i32(id);
        self.write_i32(a.len() as i32);
        for v in a {
            match v {
                J::Object(map) => {
                    let class_name = map
                        .get("$class")
                        .and_then(|x| x.as_str())
                        .unwrap_or("Object");
                    self.write_object(map, class_name)?;
                }
                _ => return Err("array element is not object".into()),
            }
        }
        Ok(())
    }

    fn write_object(
        &mut self,
        map: &serde_json::Map<String, J>,
        class_name: &str,
    ) -> Result<(), String> {
        let id = self.alloc_obj_id();
        self.push(5); // ClassWithMembersAndTypes
        self.write_i32(id);
        self.write_lp_str(class_name);
        let mut pairs: Vec<(&str, &J)> = map
            .iter()
            .filter_map(|(k, v)| {
                if k == "$class" {
                    None
                } else {
                    Some((k.as_str(), v))
                }
            })
            .collect();
        pairs.sort_by(|a, b| a.0.cmp(b.0));
        self.write_i32(pairs.len() as i32);
        for (k, _) in &pairs {
            self.write_lp_str(k);
        }
        for (_, v) in &pairs {
            self.push(self.bin_type_code(v));
        }
        for (_, v) in &pairs {
            if let Some(pt) = self.maybe_prim_type(v) {
                self.write_prim_type(pt);
            }
        }
        self.write_i32(2); // library id
        for (_, v) in &pairs {
            self.write_member_value(v)?;
        }
        Ok(())
    }

    fn infer_primitive_array_type(&self, a: &[J]) -> Option<PrimitiveType> {
        if a.is_empty() {
            return Some(PrimitiveType::Int32);
        }
        let mut kind: Option<PrimitiveType> = None;
        for v in a {
            let k = match v {
                J::Bool(_) => PrimitiveType::Boolean,
                J::Number(n) => {
                    if n.is_i64() {
                        PrimitiveType::Int64
                    } else if n.is_u64() {
                        PrimitiveType::UInt64
                    } else {
                        PrimitiveType::Double
                    }
                }
                J::String(_) => return None,
                J::Null => PrimitiveType::Null,
                J::Array(_) | J::Object(_) => return None,
            };
            if let Some(prev) = kind {
                if prev as u8 != k as u8 {
                    return None;
                }
            } else {
                kind = Some(k);
            }
        }
        // Prefer Byte if all numeric elements are 0..=255 (works for signed or unsigned JSON numbers)
        let all_byte = a.iter().all(|v| {
            if let Some(u) = v.as_u64() {
                u <= 255
            } else if let Some(i) = v.as_i64() {
                (0..=255).contains(&i)
            } else {
                false
            }
        });
        if all_byte {
            return Some(PrimitiveType::Byte);
        }
        // Collapse int64→int32 if all fit into i32
        if kind == Some(PrimitiveType::Int64)
            && a.iter().all(|v| {
                v.as_i64()
                    .map(|i| i >= i32::MIN as i64 && i <= i32::MAX as i64)
                    .unwrap_or(false)
            })
        {
            return Some(PrimitiveType::Int32);
        }
        kind
    }

    fn write_primitive_array_u8(&mut self, bytes: &[u8]) {
        self.push(15); // ArraySinglePrimitive
        let id = self.alloc_obj_id();
        self.write_i32(id);
        self.write_i32(bytes.len() as i32);
        self.write_prim_type(PrimitiveType::Byte);
        self.out.extend_from_slice(bytes);
    }

    fn is_string_array(&self, a: &[J]) -> bool {
        !a.is_empty() && a.iter().all(|v| v.is_string())
    }

    fn alloc_obj_id(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    fn alloc_str_id(&mut self) -> i32 {
        let id = self.next_str_id;
        self.next_str_id += 1;
        id
    }
}
// Serialize a generic JSON tree (from wle-core JSON dump) back into BinaryFormatter.
// Expected input shape:
// - Wrapper object: `{ "$rootClass": "TypeName", "root": <object|array|primitive> }`
// - Objects may carry `$class` to define their runtime type.
// - Bytes can be represented as:
//   - Summary `{ "$type": "bytes", "len": N }` (writer fills zeros), or
//   - Primitive array of 0..=255 integers (preferred for exact roundtrip).
// This is a pragmatic encoder to enable roundtrips for editing workflows.
// It does not reconstruct shared references or advanced .NET types.
