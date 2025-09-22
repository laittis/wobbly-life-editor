// Port of the dynamic BinaryFormatter reader
use std::collections::HashMap;
use std::fmt::{self, Write as _};

#[derive(Debug)]
pub struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    ctx: Context<'a>,
    root_id: Option<i32>,
}

#[derive(Debug, Default, Clone)]
struct Context<'a> {
    libraries: HashMap<i32, &'a str>,
    strings: HashMap<i32, &'a str>,
    objects: HashMap<i32, Value<'a>>,        // objectId -> value
    class_meta: HashMap<i32, ClassMeta<'a>>, // metadataId -> class info
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RecordType {
    SerializedStreamHeader = 0,
    ClassWithId = 1,
    SystemClassWithMembers = 2,
    ClassWithMembers = 3,
    SystemClassWithMembersAndTypes = 4,
    ClassWithMembersAndTypes = 5,
    BinaryObjectString = 6,
    BinaryArray = 7,
    MemberPrimitiveTyped = 8,
    MemberReference = 9,
    ObjectNull = 10,
    MessageEnd = 11,
    BinaryLibrary = 12,
    ObjectNullMultiple256 = 13,
    ObjectNullMultiple = 14,
    ArraySinglePrimitive = 15,
    ArraySingleObject = 16,
    ArraySingleString = 17,
}

#[derive(Debug, Clone)]
pub struct Document<'a> {
    root_id: Option<i32>,
    root: Option<Value<'a>>, // convenience snapshot of root
    ctx: Context<'a>,
}

impl<'a> Document<'a> {
    pub fn root_class_name(&self) -> Option<&'a str> {
        match &self.root {
            Some(Value::Object(obj)) => Some(obj.class_name),
            _ => None,
        }
    }

    pub fn pretty(&self) -> String {
        let mut out = String::new();
        if let Some(root) = &self.root {
            self.fmt_value(root, 0, &mut out).ok();
        } else {
            out.push_str("<no root>\n");
        }
        out
    }

    pub fn root_value(&self) -> Option<&Value<'a>> {
        self.root.as_ref()
    }
    pub fn get_object(&self, id: i32) -> Option<&Value<'a>> {
        self.ctx.objects.get(&id)
    }
    pub fn root_object_id(&self) -> Option<i32> {
        self.root_id
    }

    fn fmt_value(&self, v: &Value<'a>, indent: usize, out: &mut String) -> fmt::Result {
        let pad = |n: usize| -> String { " ".repeat(n) };
        match v {
            Value::Null => writeln!(out, "null"),
            Value::Bool(b) => writeln!(out, "{}", b),
            Value::I32(x) => writeln!(out, "{}", x),
            Value::I64(x) => writeln!(out, "{}", x),
            Value::U32(x) => writeln!(out, "{}", x),
            Value::U64(x) => writeln!(out, "{}", x),
            Value::F32(x) => writeln!(out, "{}", x),
            Value::F64(x) => writeln!(out, "{}", x),
            Value::U8(x) => writeln!(out, "{}", x),
            Value::Str(s) => writeln!(out, "\"{}\"", s),
            Value::Bytes(b) => writeln!(out, "<bytes {}>", b.len()),
            Value::Array(items) => {
                writeln!(out, "[")?;
                for it in items {
                    write!(out, "{}", pad(indent + 2))?;
                    self.fmt_value(it, indent + 2, out)?;
                }
                write!(out, "{}]", pad(indent))?;
                writeln!(out)
            }
            Value::Object(obj) => {
                writeln!(out, "{} {{", obj.class_name)?;
                for (name, val) in &obj.members {
                    write!(out, "{}{}: ", pad(indent + 2), name)?;
                    self.fmt_value(val, indent + 2, out)?;
                }
                write!(out, "{}}}", pad(indent))?;
                writeln!(out)
            }
            Value::Ref(id) => {
                if let Some(v2) = self.ctx.objects.get(id) {
                    write!(out, "&{} -> ", id)?;
                    self.fmt_value(v2, indent, out)
                } else {
                    writeln!(out, "&{} (unresolved)", id)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value<'a> {
    Null,
    Bool(bool),
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    U8(u8),
    Str(&'a str),
    Bytes(&'a [u8]),
    Array(Vec<Value<'a>>),
    Object(DynObject<'a>),
    Ref(i32),
}

#[derive(Debug, Clone)]
pub struct DynObject<'a> {
    pub class_name: &'a str,
    #[allow(dead_code)]
    pub library_id: i32,
    pub members: Vec<(&'a str, Value<'a>)>,
}

#[derive(Debug, Clone)]
struct ClassMeta<'a> {
    class_name: &'a str,
    library_id: i32,
    member_names: Vec<&'a str>,
    member_types: Option<Vec<BinaryType>>,
}

#[derive(Debug, Clone, Copy)]
enum BinaryType {
    Primitive(PrimitiveType),
    String,
    Object,
    SystemClass, // not used yet
    Class,       // not used yet
    ObjectArray,
    StringArray,
    PrimitiveArray(PrimitiveType),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimitiveType {
    Boolean,
    Byte,
    Char,
    Decimal,
    Double,
    Int16,
    Int32,
    Int64,
    SByte,
    Single,
    TimeSpan,
    DateTime,
    UInt16,
    UInt32,
    UInt64,
    Null,
    String,
}

impl<'a> Parser<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            ctx: Context::default(),
            root_id: None,
        }
    }
    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn parse_stream(&mut self) -> Result<Document<'a>, String> {
        let rec = self.read_u8()?;
        if rec != RecordType::SerializedStreamHeader as u8 {
            return Err(format!(
                "expected SerializedStreamHeader (0), found {rec:#x} at {:#x}",
                self.pos - 1
            ));
        }
        let _root_id = self.read_i32()?;
        let _header_id = self.read_i32()?;
        let _major = self.read_i32()?;
        let _minor = self.read_i32()?;
        let mut root_snapshot: Option<Value<'a>> = None;
        loop {
            let rec = self.read_u8()?;
            match rec {
                x if x == RecordType::BinaryLibrary as u8 => {
                    let lib_id = self.read_i32()?;
                    let name = self.read_lp_string()?;
                    self.ctx.libraries.insert(lib_id, name);
                }
                x if x == RecordType::ClassWithMembersAndTypes as u8 => {
                    let (obj_id, obj) = self.read_class_with_members_and_types()?;
                    if self.root_id.is_none() {
                        self.root_id = Some(obj_id);
                    }
                    if root_snapshot.is_none() {
                        root_snapshot = Some(Value::Object(obj.clone()));
                    }
                    self.ctx.objects.insert(obj_id, Value::Object(obj));
                }
                x if x == RecordType::SystemClassWithMembersAndTypes as u8 => {
                    let (obj_id, obj) = self.read_system_class_with_members_and_types()?;
                    if self.root_id.is_none() {
                        self.root_id = Some(obj_id);
                    }
                    if root_snapshot.is_none() {
                        root_snapshot = Some(Value::Object(obj.clone()));
                    }
                    self.ctx.objects.insert(obj_id, Value::Object(obj));
                }
                x if x == RecordType::ClassWithMembers as u8 => {
                    let (obj_id, obj) = self.read_class_with_members()?;
                    if self.root_id.is_none() {
                        self.root_id = Some(obj_id);
                    }
                    if root_snapshot.is_none() {
                        root_snapshot = Some(Value::Object(obj.clone()));
                    }
                    self.ctx.objects.insert(obj_id, Value::Object(obj));
                }
                x if x == RecordType::SystemClassWithMembers as u8 => {
                    let (obj_id, obj) = self.read_system_class_with_members()?;
                    if self.root_id.is_none() {
                        self.root_id = Some(obj_id);
                    }
                    if root_snapshot.is_none() {
                        root_snapshot = Some(Value::Object(obj.clone()));
                    }
                    self.ctx.objects.insert(obj_id, Value::Object(obj));
                }
                x if x == RecordType::BinaryObjectString as u8 => {
                    let id = self.read_i32()?;
                    let s = self.read_lp_string()?;
                    self.ctx.strings.insert(id, s);
                    self.ctx.objects.insert(id, Value::Str(s));
                }
                x if x == RecordType::MemberPrimitiveTyped as u8 => {
                    let prim = self.read_primitive_type()?;
                    let v = self.read_inline_primitive(prim)?;
                    let _ = v;
                }
                x if x == RecordType::MemberReference as u8 => {
                    let _ = self.read_i32()?;
                }
                x if x == RecordType::ObjectNull as u8 => {}
                x if x == RecordType::ArraySinglePrimitive as u8 => {
                    let (id, v) = self.read_array_single_primitive()?;
                    self.ctx.objects.insert(id, v);
                }
                x if x == RecordType::ArraySingleString as u8 => {
                    let (id, v) = self.read_array_single_string()?;
                    self.ctx.objects.insert(id, v);
                }
                x if x == RecordType::ArraySingleObject as u8 => {
                    let (id, v) = self.read_array_single_object()?;
                    self.ctx.objects.insert(id, v);
                }
                x if x == RecordType::ClassWithId as u8 => {
                    let (obj_id, obj) = self.read_class_with_id()?;
                    if self.root_id.is_none() {
                        self.root_id = Some(obj_id);
                    }
                    if root_snapshot.is_none() {
                        root_snapshot = Some(Value::Object(obj.clone()));
                    }
                    self.ctx.objects.insert(obj_id, Value::Object(obj));
                }
                x if x == RecordType::BinaryArray as u8 => {
                    let (id, v) = self.read_binary_array()?;
                    self.ctx.objects.insert(id, v);
                }
                x if x == RecordType::MessageEnd as u8 => {
                    break;
                }
                other => {
                    return Err(format!(
                        "unknown/unsupported record {other:#x} at {:#x}",
                        self.pos - 1
                    ));
                }
            }
        }
        Ok(Document {
            root_id: self.root_id,
            root: root_snapshot,
            ctx: std::mem::take(&mut self.ctx),
        })
    }

    fn read_class_with_members_and_types(&mut self) -> Result<(i32, DynObject<'a>), String> {
        let object_id = self.read_i32()?;
        let class_name = self.read_lp_string()?;
        let member_count = self.read_i32()? as usize;
        let mut member_names = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            member_names.push(self.read_lp_string()?);
        }
        let mut bin_types_raw = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            bin_types_raw.push(self.read_u8()?);
        }
        let mut bin_types: Vec<BinaryType> = Vec::with_capacity(member_count);
        for t in bin_types_raw {
            let bt = match t {
                0 => {
                    let p = self.read_primitive_type()?;
                    BinaryType::Primitive(p)
                }
                1 => BinaryType::String,
                2 => BinaryType::Object,
                3 => {
                    let _ = self.read_lp_string()?;
                    BinaryType::SystemClass
                }
                4 => {
                    let _ = self.read_lp_string()?;
                    let _ = self.read_i32()?;
                    BinaryType::Class
                }
                5 => BinaryType::ObjectArray,
                6 => BinaryType::StringArray,
                7 => {
                    let p = self.read_primitive_type()?;
                    BinaryType::PrimitiveArray(p)
                }
                other => return Err(format!("unknown BinaryType {other} at {:#x}", self.pos - 1)),
            };
            bin_types.push(bt);
        }
        let library_id = self.read_i32()?;

        let mut members: Vec<(&'a str, Value<'a>)> = Vec::with_capacity(member_count);
        for (i, bt) in bin_types.iter().enumerate() {
            let name = member_names[i];
            let val = match bt {
                BinaryType::Primitive(p) => self.read_inline_primitive(*p)?,
                BinaryType::String => self.read_next_string_like()?,
                BinaryType::PrimitiveArray(p) => self.read_next_primitive_array(*p)?,
                BinaryType::Object => self.read_next_object_like()?,
                BinaryType::ObjectArray => self.read_next_object_array()?,
                BinaryType::StringArray => self.read_next_string_array()?,
                BinaryType::SystemClass | BinaryType::Class => self.read_next_object_like()?,
            };
            members.push((name, val));
        }
        // Register metadata keyed by object_id as well
        self.ctx.class_meta.insert(
            object_id,
            ClassMeta {
                class_name,
                library_id,
                member_names: member_names.clone(),
                member_types: Some(bin_types.clone()),
            },
        );
        Ok((
            object_id,
            DynObject {
                class_name,
                library_id,
                members,
            },
        ))
    }

    fn read_class_with_members(&mut self) -> Result<(i32, DynObject<'a>), String> {
        let object_id = self.read_i32()?;
        let class_name = self.read_lp_string()?;
        let member_count = self.read_i32()? as usize;
        let mut member_names = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            member_names.push(self.read_lp_string()?);
        }
        let library_id = self.read_i32()?;
        self.ctx.class_meta.insert(
            object_id,
            ClassMeta {
                class_name,
                library_id,
                member_names: member_names.clone(),
                member_types: None,
            },
        );
        let mut members: Vec<(&'a str, Value<'a>)> = Vec::with_capacity(member_count);
        for &name in member_names.iter().take(member_count) {
            let val = self.read_next_any_value()?;
            members.push((name, val));
        }
        Ok((
            object_id,
            DynObject {
                class_name,
                library_id,
                members,
            },
        ))
    }

    fn read_system_class_with_members(&mut self) -> Result<(i32, DynObject<'a>), String> {
        let object_id = self.read_i32()?;
        let class_name = self.read_lp_string()?;
        let member_count = self.read_i32()? as usize;
        let mut member_names = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            member_names.push(self.read_lp_string()?);
        }
        self.ctx.class_meta.insert(
            object_id,
            ClassMeta {
                class_name,
                library_id: 0,
                member_names: member_names.clone(),
                member_types: None,
            },
        );
        let mut members: Vec<(&'a str, Value<'a>)> = Vec::with_capacity(member_count);
        for &name in member_names.iter().take(member_count) {
            let val = self.read_next_any_value()?;
            members.push((name, val));
        }
        Ok((
            object_id,
            DynObject {
                class_name,
                library_id: 0,
                members,
            },
        ))
    }

    fn read_class_with_id(&mut self) -> Result<(i32, DynObject<'a>), String> {
        let object_id = self.read_i32()?;
        let metadata_id = self.read_i32()?;
        let meta = self
            .ctx
            .class_meta
            .get(&metadata_id)
            .ok_or_else(|| format!("unknown metadataId {} at {:#x}", metadata_id, self.pos - 4))?
            .clone();
        let mut members: Vec<(&'a str, Value<'a>)> = Vec::with_capacity(meta.member_names.len());
        if let Some(types) = meta.member_types {
            for (i, bt) in types.iter().enumerate() {
                let name = meta.member_names[i];
                let val = match bt {
                    BinaryType::Primitive(p) => self.read_inline_primitive(*p)?,
                    BinaryType::String => self.read_next_string_like()?,
                    BinaryType::PrimitiveArray(p) => self.read_next_primitive_array(*p)?,
                    BinaryType::Object => self.read_next_object_like()?,
                    BinaryType::ObjectArray => self.read_next_object_array()?,
                    BinaryType::StringArray => self.read_next_string_array()?,
                    BinaryType::SystemClass | BinaryType::Class => self.read_next_object_like()?,
                };
                members.push((name, val));
            }
        } else {
            for name in meta.member_names.iter().copied() {
                let val = self.read_next_any_value()?;
                members.push((name, val));
            }
        }
        Ok((
            object_id,
            DynObject {
                class_name: meta.class_name,
                library_id: meta.library_id,
                members,
            },
        ))
    }

    fn read_next_any_value(&mut self) -> Result<Value<'a>, String> {
        let rec = self.peek_u8()?;
        match rec {
            x if x == RecordType::MemberPrimitiveTyped as u8 => {
                let _ = self.read_u8()?;
                let prim = self.read_primitive_type()?;
                self.read_inline_primitive(prim)
            }
            x if x == RecordType::BinaryObjectString as u8 => self.read_next_string_like(),
            x if x == RecordType::ClassWithMembersAndTypes as u8
                || x == RecordType::SystemClassWithMembersAndTypes as u8
                || x == RecordType::ClassWithMembers as u8
                || x == RecordType::SystemClassWithMembers as u8
                || x == RecordType::ClassWithId as u8 =>
            {
                self.read_next_object_like()
            }
            x if x == RecordType::ArraySinglePrimitive as u8 => {
                let _ = self.read_u8()?;
                let (id, v) = self.read_array_single_primitive()?;
                self.ctx.objects.insert(id, v.clone());
                Ok(v)
            }
            x if x == RecordType::ArraySingleString as u8 => {
                let _ = self.read_u8()?;
                let (id, v) = self.read_array_single_string()?;
                self.ctx.objects.insert(id, v.clone());
                Ok(v)
            }
            x if x == RecordType::ArraySingleObject as u8 => {
                let _ = self.read_u8()?;
                let (id, v) = self.read_array_single_object()?;
                self.ctx.objects.insert(id, v.clone());
                Ok(v)
            }
            x if x == RecordType::MemberReference as u8 => {
                let _ = self.read_u8()?;
                let idref = self.read_i32()?;
                Ok(Value::Ref(idref))
            }
            x if x == RecordType::ObjectNull as u8 => {
                let _ = self.read_u8()?;
                Ok(Value::Null)
            }
            x if x == RecordType::BinaryArray as u8 => {
                let _ = self.read_u8()?;
                let (_id, v) = self.read_binary_array()?;
                Ok(v)
            }
            other => Err(format!("unexpected record {other:#x} at {:#x}", self.pos)),
        }
    }

    fn read_next_string_like(&mut self) -> Result<Value<'a>, String> {
        let rec = self.peek_u8()?;
        if rec == RecordType::BinaryObjectString as u8 {
            let _ = self.read_u8()?;
            let id = self.read_i32()?;
            let s = self.read_lp_string()?;
            self.ctx.strings.insert(id, s);
            Ok(Value::Str(s))
        } else if rec == RecordType::MemberReference as u8 {
            let _ = self.read_u8()?;
            let idref = self.read_i32()?;
            if let Some(Value::Str(s)) = self.ctx.objects.get(&idref).cloned() {
                Ok(Value::Str(s))
            } else {
                Ok(Value::Ref(idref))
            }
        } else if rec == RecordType::ObjectNull as u8 {
            let _ = self.read_u8()?;
            Ok(Value::Null)
        } else {
            Err(format!(
                "expected string-like record (6/9/10), found {rec:#x} at {:#x}",
                self.pos
            ))
        }
    }

    fn read_next_object_like(&mut self) -> Result<Value<'a>, String> {
        let rec = self.peek_u8()?;
        match rec {
            x if x == RecordType::ClassWithMembersAndTypes as u8 => {
                let _ = self.read_u8()?;
                let (id, obj) = self.read_class_with_members_and_types()?;
                self.ctx.objects.insert(id, Value::Object(obj.clone()));
                Ok(Value::Object(obj))
            }
            x if x == RecordType::SystemClassWithMembersAndTypes as u8 => {
                let _ = self.read_u8()?;
                let (id, obj) = self.read_system_class_with_members_and_types()?;
                self.ctx.objects.insert(id, Value::Object(obj.clone()));
                Ok(Value::Object(obj))
            }
            x if x == RecordType::ClassWithMembers as u8 => {
                let _ = self.read_u8()?;
                let (id, obj) = self.read_class_with_members()?;
                self.ctx.objects.insert(id, Value::Object(obj.clone()));
                Ok(Value::Object(obj))
            }
            x if x == RecordType::SystemClassWithMembers as u8 => {
                let _ = self.read_u8()?;
                let (id, obj) = self.read_system_class_with_members()?;
                self.ctx.objects.insert(id, Value::Object(obj.clone()));
                Ok(Value::Object(obj))
            }
            x if x == RecordType::ClassWithId as u8 => {
                let _ = self.read_u8()?;
                let (id, obj) = self.read_class_with_id()?;
                self.ctx.objects.insert(id, Value::Object(obj.clone()));
                Ok(Value::Object(obj))
            }
            x if x == RecordType::MemberReference as u8 => {
                let _ = self.read_u8()?;
                let idref = self.read_i32()?;
                Ok(Value::Ref(idref))
            }
            x if x == RecordType::ObjectNull as u8 => {
                let _ = self.read_u8()?;
                Ok(Value::Null)
            }
            other => Err(format!(
                "unexpected record for object-like member {other:#x} at {:#x}",
                self.pos
            )),
        }
    }

    fn read_system_class_with_members_and_types(&mut self) -> Result<(i32, DynObject<'a>), String> {
        let object_id = self.read_i32()?;
        let class_name = self.read_lp_string()?; // e.g., System.Guid
        let member_count = self.read_i32()? as usize;
        let mut member_names = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            member_names.push(self.read_lp_string()?);
        }
        let mut bin_types_raw = Vec::with_capacity(member_count);
        for _ in 0..member_count {
            bin_types_raw.push(self.read_u8()?);
        }
        let mut bin_types: Vec<BinaryType> = Vec::with_capacity(member_count);
        for t in bin_types_raw {
            let bt = match t {
                0 => BinaryType::Primitive(self.read_primitive_type()?),
                1 => BinaryType::String,
                2 => BinaryType::Object,
                3 => {
                    let _ = self.read_lp_string()?;
                    BinaryType::SystemClass
                }
                4 => {
                    let _ = self.read_lp_string()?;
                    let _ = self.read_i32()?;
                    BinaryType::Class
                }
                5 => BinaryType::ObjectArray,
                6 => BinaryType::StringArray,
                7 => BinaryType::PrimitiveArray(self.read_primitive_type()?),
                other => return Err(format!("unknown BinaryType {other} at {:#x}", self.pos - 1)),
            };
            bin_types.push(bt);
        }
        let mut members: Vec<(&'a str, Value<'a>)> = Vec::with_capacity(member_count);
        for (i, bt) in bin_types.iter().enumerate() {
            let name = member_names[i];
            let val = match bt {
                BinaryType::Primitive(p) => self.read_inline_primitive(*p)?,
                BinaryType::String => self.read_next_string_like()?,
                BinaryType::PrimitiveArray(p) => self.read_next_primitive_array(*p)?,
                BinaryType::Object => self.read_next_object_like()?,
                BinaryType::ObjectArray => self.read_next_object_array()?,
                BinaryType::StringArray => self.read_next_string_array()?,
                BinaryType::SystemClass | BinaryType::Class => self.read_next_object_like()?,
            };
            members.push((name, val));
        }
        self.ctx.class_meta.insert(
            object_id,
            ClassMeta {
                class_name,
                library_id: 0,
                member_names: member_names.clone(),
                member_types: Some(bin_types.clone()),
            },
        );
        Ok((
            object_id,
            DynObject {
                class_name,
                library_id: 0,
                members,
            },
        ))
    }

    // Array helpers for typed consumption
    fn read_next_primitive_array(&mut self, _p: PrimitiveType) -> Result<Value<'a>, String> {
        let rec = self.peek_u8()?;
        if rec == RecordType::ArraySinglePrimitive as u8 {
            let _ = self.read_u8()?;
            let (id, v) = self.read_array_single_primitive()?;
            self.ctx.objects.insert(id, v.clone());
            Ok(v)
        } else if rec == RecordType::BinaryArray as u8 {
            let _ = self.read_u8()?;
            let (id, v) = self.read_binary_array()?;
            self.ctx.objects.insert(id, v.clone());
            Ok(v)
        } else if rec == RecordType::MemberReference as u8 {
            let _ = self.read_u8()?;
            let idref = self.read_i32()?;
            Ok(Value::Ref(idref))
        } else if rec == RecordType::ObjectNull as u8 {
            let _ = self.read_u8()?;
            Ok(Value::Null)
        } else {
            Err(format!(
                "expected primitive array next (15/7/9/10), found {rec:#x} at {:#x}",
                self.pos
            ))
        }
    }

    fn read_next_string_array(&mut self) -> Result<Value<'a>, String> {
        let rec = self.peek_u8()?;
        if rec == RecordType::ArraySingleString as u8 {
            let _ = self.read_u8()?;
            let (id, v) = self.read_array_single_string()?;
            self.ctx.objects.insert(id, v.clone());
            Ok(v)
        } else if rec == RecordType::BinaryArray as u8 {
            let _ = self.read_u8()?;
            let (id, v) = self.read_binary_array()?;
            self.ctx.objects.insert(id, v.clone());
            Ok(v)
        } else {
            Err(format!(
                "expected string array next (17/7), found {rec:#x} at {:#x}",
                self.pos
            ))
        }
    }

    fn read_next_object_array(&mut self) -> Result<Value<'a>, String> {
        let rec = self.peek_u8()?;
        if rec == RecordType::ArraySingleObject as u8 {
            let _ = self.read_u8()?;
            let (id, v) = self.read_array_single_object()?;
            self.ctx.objects.insert(id, v.clone());
            Ok(v)
        } else if rec == RecordType::BinaryArray as u8 {
            let _ = self.read_u8()?;
            let (id, v) = self.read_binary_array()?;
            self.ctx.objects.insert(id, v.clone());
            Ok(v)
        } else {
            Err(format!(
                "expected object array next (16/7), found {rec:#x} at {:#x}",
                self.pos
            ))
        }
    }

    fn read_array_single_primitive(&mut self) -> Result<(i32, Value<'a>), String> {
        let object_id = self.read_i32()?;
        let len = self.read_i32()? as usize;
        let prim = self.read_primitive_type()?;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(self.read_inline_primitive(prim)?);
        }
        Ok((object_id, Value::Array(out)))
    }
    fn read_array_single_string(&mut self) -> Result<(i32, Value<'a>), String> {
        let object_id = self.read_i32()?;
        let len = self.read_i32()? as usize;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(self.read_next_string_like()?);
        }
        Ok((object_id, Value::Array(out)))
    }
    fn read_array_single_object(&mut self) -> Result<(i32, Value<'a>), String> {
        let object_id = self.read_i32()?;
        let len = self.read_i32()? as usize;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            out.push(self.read_next_object_like()?);
        }
        Ok((object_id, Value::Array(out)))
    }

    fn read_binary_array(&mut self) -> Result<(i32, Value<'a>), String> {
        let object_id = self.read_i32()?;
        let array_type = self.read_u8()?; // 0: Single, 3: SingleOffset (others ignored)
        let rank = self.read_i32()? as usize;
        if rank != 1 {
            return Err("only rank-1 arrays supported".to_string());
        }
        let len = self.read_i32()? as usize; // length for 1D
        if matches!(array_type, 3..=5) {
            let _lb = self.read_i32()?;
            let _ = _lb;
        }
        let elem_code = self.read_u8()?;
        let elem_type = match elem_code {
            0 => BinaryType::Primitive(self.read_primitive_type()?),
            1 => BinaryType::String,
            2 => BinaryType::Object,
            3 => {
                let _ = self.read_lp_string()?;
                BinaryType::SystemClass
            }
            4 => {
                let _ = self.read_lp_string()?;
                let _ = self.read_i32()?;
                BinaryType::Class
            }
            5 => BinaryType::ObjectArray,
            6 => BinaryType::StringArray,
            7 => BinaryType::PrimitiveArray(self.read_primitive_type()?),
            other => {
                return Err(format!(
                    "unknown BinaryType in BinaryArray: {} at {:#x}",
                    other,
                    self.pos - 1
                ));
            }
        };
        let mut out = Vec::with_capacity(len);
        match elem_type {
            BinaryType::Primitive(p) => {
                for _ in 0..len {
                    out.push(self.read_inline_primitive(p)?);
                }
            }
            BinaryType::String => {
                for _ in 0..len {
                    out.push(self.read_next_string_like()?);
                }
            }
            BinaryType::Object | BinaryType::SystemClass | BinaryType::Class => {
                while out.len() < len {
                    let rec = self.peek_u8()?;
                    if rec == RecordType::ObjectNull as u8 {
                        let _ = self.read_u8()?;
                        out.push(Value::Null);
                    } else if rec == RecordType::ObjectNullMultiple256 as u8 {
                        let _ = self.read_u8()?;
                        let cnt = self.read_u8()? as usize;
                        out.extend(std::iter::repeat_n(Value::Null, cnt));
                    } else if rec == RecordType::ObjectNullMultiple as u8 {
                        let _ = self.read_u8()?;
                        let cnt = self.read_i32()? as usize;
                        out.extend(std::iter::repeat_n(Value::Null, cnt));
                    } else if rec == RecordType::MemberReference as u8 {
                        let _ = self.read_u8()?;
                        let idref = self.read_i32()?;
                        out.push(Value::Ref(idref));
                    } else if rec == RecordType::BinaryObjectString as u8 {
                        out.push(self.read_next_string_like()?);
                    } else {
                        out.push(self.read_next_object_like()?);
                    }
                }
            }
            BinaryType::ObjectArray | BinaryType::StringArray | BinaryType::PrimitiveArray(_) => {
                return Err(
                    "nested array element types in BinaryArray not supported yet".to_string(),
                );
            }
        }
        Ok((object_id, Value::Array(out)))
    }

    fn read_primitive_type(&mut self) -> Result<PrimitiveType, String> {
        let code = self.read_u8()?;
        let p = match code {
            1 => PrimitiveType::Boolean,
            2 => PrimitiveType::Byte,
            3 => PrimitiveType::Char,
            5 => PrimitiveType::Decimal,
            6 => PrimitiveType::Double,
            7 => PrimitiveType::Int16,
            8 => PrimitiveType::Int32,
            9 => PrimitiveType::Int64,
            10 => PrimitiveType::SByte,
            11 => PrimitiveType::Single,
            12 => PrimitiveType::TimeSpan,
            13 => PrimitiveType::DateTime,
            14 => PrimitiveType::UInt16,
            15 => PrimitiveType::UInt32,
            16 => PrimitiveType::UInt64,
            17 => PrimitiveType::Null,
            18 => PrimitiveType::String,
            _ => {
                return Err(format!(
                    "unknown PrimitiveType code {} at {:#x}",
                    code,
                    self.pos - 1
                ));
            }
        };
        Ok(p)
    }

    fn read_inline_primitive(&mut self, p: PrimitiveType) -> Result<Value<'a>, String> {
        let v = match p {
            PrimitiveType::Boolean => Value::Bool(self.read_u8()? != 0),
            PrimitiveType::Byte => Value::U8(self.read_u8()?),
            PrimitiveType::SByte => Value::I32(self.read_i8()? as i32),
            PrimitiveType::Char => Value::U32(self.read_u16()? as u32),
            PrimitiveType::Int16 => Value::I32(self.read_i16()? as i32),
            PrimitiveType::UInt16 => Value::U32(self.read_u16()? as u32),
            PrimitiveType::Int32 => Value::I32(self.read_i32()?),
            PrimitiveType::UInt32 => Value::U32(self.read_u32()?),
            PrimitiveType::Int64 => Value::I64(self.read_i64()?),
            PrimitiveType::UInt64 => Value::U64(self.read_u64()?),
            PrimitiveType::Single => Value::F32(self.read_f32()?),
            PrimitiveType::Double => Value::F64(self.read_f64()?),
            PrimitiveType::TimeSpan | PrimitiveType::DateTime => Value::I64(self.read_i64()?),
            PrimitiveType::Null => Value::Null,
            PrimitiveType::Decimal => {
                let s = self.read_slice(16)?;
                Value::Bytes(s)
            }
            PrimitiveType::String => {
                let s = self.read_lp_string()?;
                Value::Str(s)
            }
        };
        Ok(v)
    }

    // Low-level utilities
    pub fn peek_u8(&self) -> Result<u8, String> {
        self.data
            .get(self.pos)
            .copied()
            .ok_or_else(|| "eof".to_string())
    }
    pub fn read_u8(&mut self) -> Result<u8, String> {
        if self.pos >= self.data.len() {
            return Err("eof".into());
        }
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }
    pub fn read_i8(&mut self) -> Result<i8, String> {
        Ok(self.read_u8()? as i8)
    }
    pub fn read_u16(&mut self) -> Result<u16, String> {
        if self.pos + 2 > self.data.len() {
            return Err("eof".into());
        }
        let b = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(b)
    }
    pub fn read_i16(&mut self) -> Result<i16, String> {
        Ok(self.read_u16()? as i16)
    }
    pub fn read_u32(&mut self) -> Result<u32, String> {
        if self.pos + 4 > self.data.len() {
            return Err("eof".into());
        }
        let b = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(b)
    }
    pub fn read_i32(&mut self) -> Result<i32, String> {
        Ok(self.read_u32()? as i32)
    }
    pub fn read_u64(&mut self) -> Result<u64, String> {
        if self.pos + 8 > self.data.len() {
            return Err("eof".into());
        }
        let b = u64::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
            self.data[self.pos + 4],
            self.data[self.pos + 5],
            self.data[self.pos + 6],
            self.data[self.pos + 7],
        ]);
        self.pos += 8;
        Ok(b)
    }
    pub fn read_i64(&mut self) -> Result<i64, String> {
        Ok(self.read_u64()? as i64)
    }
    pub fn read_f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_bits(self.read_u32()?))
    }
    pub fn read_f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_bits(self.read_u64()?))
    }
    pub fn read_lp_string(&mut self) -> Result<&'a str, String> {
        let len = self.read_7bit_len()?;
        let s = self.read_slice(len)?;
        std::str::from_utf8(s).map_err(|_| "invalid utf8 in string".to_string())
    }
    pub fn read_slice(&mut self, len: usize) -> Result<&'a [u8], String> {
        if self.pos + len > self.data.len() {
            return Err("eof".into());
        }
        let s = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(s)
    }
    pub fn read_7bit_len(&mut self) -> Result<usize, String> {
        let mut result: usize = 0;
        let mut shift = 0u32;
        loop {
            let b = self.read_u8()? as usize;
            result |= (b & 0x7F) << shift;
            if (b & 0x80) == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return Err("7bit length too large".into());
            }
        }
        Ok(result)
    }
}
