use crate::binfmt::{Document, DynObject, Value};
use std::borrow::Cow;

#[derive(Debug, Clone)]
pub struct SaveSlotInfoData<'a> {
    pub last_selected_player_slot: i32,
    pub date_time: &'a str,
    pub small_image_data: Cow<'a, [u8]>,
}

#[derive(Debug, Clone, Copy)]
pub struct Guid {
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d_to_k: [u8; 8],
}

impl core::fmt::Display for Guid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-",
            self.a as u32, self.b as u16, self.c as u16, self.d_to_k[0], self.d_to_k[1]
        )?;
        for i in 2..8 {
            write!(f, "{:02x}", self.d_to_k[i])?;
        }
        Ok(())
    }
}

impl<'a> Document<'a> {
    pub fn as_save_slot_info(&'a self) -> Option<SaveSlotInfoData<'a>> {
        let Value::Object(obj) = self.root_value()? else {
            return None;
        };
        let mut last: Option<i32> = None;
        let mut dt: Option<&'a str> = None;
        let mut img: Option<&'a [u8]> = None;
        for (name, v) in &obj.members {
            match *name {
                "lastSelectedPlayerSlot" => last = self.get_i32(v),
                "dateTime" => dt = self.get_str(v),
                "smallImageData" => img = self.get_bytes(v),
                _ => {}
            }
        }
        // smallImageData might be a byte array; support both bytes and u8[]
        let small = if let Some(b) = img {
            Cow::Borrowed(b)
        } else if let Some(v) = self
            .member_value(obj, "smallImageData")
            .and_then(|vv| self.get_u8_array(vv))
        {
            Cow::Owned(v)
        } else {
            return None;
        };
        Some(SaveSlotInfoData {
            last_selected_player_slot: last?,
            date_time: dt?,
            small_image_data: small,
        })
    }

    // helpers
    pub fn resolve_value(&'a self, v: &'a Value<'a>) -> &'a Value<'a> {
        let mut cur = v;
        let mut guard = 0;
        while let Value::Ref(id) = cur {
            if let Some(next) = self.get_object(*id) {
                cur = next;
            } else {
                break;
            }
            guard += 1;
            if guard > 8 {
                break;
            }
        }
        cur
    }
    pub fn as_object_value(&'a self, v: &'a Value<'a>) -> Option<&'a DynObject<'a>> {
        match self.resolve_value(v) {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }
    pub fn member_value(&'a self, obj: &'a DynObject<'a>, name: &str) -> Option<&'a Value<'a>> {
        obj.members.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }
    fn get_i32(&'a self, v: &Value<'a>) -> Option<i32> {
        match self.resolve_value(v) {
            Value::I32(x) => Some(*x),
            _ => None,
        }
    }
    fn get_str(&'a self, v: &'a Value<'a>) -> Option<&'a str> {
        match self.resolve_value(v) {
            Value::Str(s) => Some(*s),
            _ => None,
        }
    }
    fn get_bytes(&'a self, v: &'a Value<'a>) -> Option<&'a [u8]> {
        match self.resolve_value(v) {
            Value::Bytes(b) => Some(*b),
            _ => None,
        }
    }
    fn get_u8_array(&'a self, v: &Value<'a>) -> Option<Vec<u8>> {
        match self.resolve_value(v) {
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    if let Value::U8(b) = self.resolve_value(it) {
                        out.push(*b);
                    } else {
                        return None;
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }
    // Note: domain-specific helpers removed to keep core generic.
}
