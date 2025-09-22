use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::model::Guid;
use walkdir::WalkDir;
use zip::CompressionMethod;
use zip::write::FileOptions;

// Zip backup of a slot directory (non-destructive)
pub fn zip_backup_slot(dir: &Path) -> io::Result<PathBuf> {
    if !dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a directory",
        ));
    }
    let parent = dir.parent().unwrap_or(Path::new("."));
    let name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("slot");
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let zip_name = format!("{}_{}.zip", name, ts);
    let dest = parent.join(zip_name);

    let file = fs::File::create(&dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let base = dir;
    for entry in WalkDir::new(base) {
        let entry = entry.map_err(|e| io::Error::other(e.to_string()))?;
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let name = rel.to_string_lossy().replace('\\', "/");
        if path.is_dir() {
            zip.add_directory(name, options)?;
        } else {
            zip.start_file(name, options)?;
            let data = fs::read(path)?;
            zip.write_all(&data)?;
        }
    }
    zip.finish()?;
    Ok(dest)
}

// TODO: Generic BinaryFormatter write-back (edit → binary) is out of scope for now.
//       Only JSON-value edits and JSON file writes are supported in core.

fn write_i32(w: &mut Vec<u8>, v: i32) {
    w.extend_from_slice(&v.to_le_bytes());
}
fn write_lp_string(w: &mut Vec<u8>, s: &str) {
    write_7bit_len(w, s.len());
    w.extend_from_slice(s.as_bytes());
}
fn write_7bit_len(w: &mut Vec<u8>, mut v: usize) {
    while v >= 0x80 {
        w.push(((v as u8) & 0x7F) | 0x80);
        v >>= 7;
    }
    w.push(v as u8);
}

// Parse canonical hyphenated GUID (8-4-4-4-12) into internal Guid fields
pub fn parse_guid_hyphen(s: &str) -> Result<Guid, String> {
    let parts: Vec<&str> = s.trim().split('-').collect();
    if parts.len() != 5 {
        return Err("invalid guid format".into());
    }
    let p1 = u32::from_str_radix(parts[0], 16).map_err(|_| "bad a")?;
    let p2 = u16::from_str_radix(parts[1], 16).map_err(|_| "bad b")?;
    let p3 = u16::from_str_radix(parts[2], 16).map_err(|_| "bad c")?;
    if parts[3].len() != 4 || parts[4].len() != 12 {
        return Err("bad d/e or tail".into());
    }
    let b0 = u8::from_str_radix(&parts[3][0..2], 16).map_err(|_| "bad d0")?;
    let b1 = u8::from_str_radix(&parts[3][2..4], 16).map_err(|_| "bad d1")?;
    let mut tail = [0u8; 6];
    for (i, slot) in tail.iter_mut().enumerate() {
        let off = i * 2;
        *slot = u8::from_str_radix(&parts[4][off..off + 2], 16).map_err(|_| "bad tail")?;
    }
    let a = p1 as i32;
    let b = (p2 as i16) as i32;
    let c = (p3 as i16) as i32;
    let d_to_k = [b0, b1, tail[0], tail[1], tail[2], tail[3], tail[4], tail[5]];
    Ok(Guid { a, b, c, d_to_k })
}

// Test helper: build a minimal SlotInfo.sav payload (BinaryFormatter)
pub fn build_slot_info_bytes(
    last_selected_player_slot: i32,
    date_time: &str,
    small_image_data: &[u8],
) -> Vec<u8> {
    let mut w = Vec::with_capacity(64 + small_image_data.len());
    // SerializedStreamHeader
    w.push(0); // record 0
    write_i32(&mut w, 1); // rootId
    write_i32(&mut w, -1); // headerId
    write_i32(&mut w, 1); // major
    write_i32(&mut w, 0); // minor

    // BinaryLibrary (id=2)
    w.push(12);
    write_i32(&mut w, 2);
    write_lp_string(
        &mut w,
        "Game, Version=0.0.0.0, Culture=neutral, PublicKeyToken=null",
    );

    // ClassWithMembersAndTypes (5)
    w.push(5);
    write_i32(&mut w, 1); // objectId (root)
    write_lp_string(&mut w, "SaveSlotInfoData");
    write_i32(&mut w, 3); // member count
    write_lp_string(&mut w, "lastSelectedPlayerSlot");
    write_lp_string(&mut w, "dateTime");
    write_lp_string(&mut w, "smallImageData");

    // member types: Primitive(Int32), String, PrimitiveArray(Byte)
    w.push(0); // Primitive
    w.push(1); // String
    w.push(7); // PrimitiveArray
    // Extra type info
    w.push(8); // PrimitiveType::Int32
    // String has no extra
    w.push(2); // PrimitiveType::Byte for array

    // library id for SaveSlotInfoData
    write_i32(&mut w, 2);

    // values:
    write_i32(&mut w, last_selected_player_slot);
    // dateTime as BinaryObjectString (6)
    w.push(6);
    write_i32(&mut w, 3); // string object id
    write_lp_string(&mut w, date_time);
    // smallImageData as ArraySinglePrimitive (15)
    w.push(15);
    write_i32(&mut w, 4); // array object id
    write_i32(&mut w, small_image_data.len() as i32); // length
    w.push(2); // PrimitiveType::Byte
    w.extend_from_slice(small_image_data);
    // MessageEnd (11)
    w.push(11);
    w
}
