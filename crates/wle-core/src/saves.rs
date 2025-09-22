use std::fs;
use std::path::{Path, PathBuf};

pub fn is_save_root(p: &Path) -> bool { p.is_dir() && p.join("SaveInfo.sav").exists() }

pub fn list_slots(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() && p.file_name().and_then(|s| s.to_str()).map(|n| n.starts_with("SaveSlot_")) == Some(true) { out.push(p); }
        }
    }
    out.sort(); out
}

