use chrono::{DateTime, Local};
use eframe::{App, egui};
use egui::{ColorImage, TextureHandle};
use std::path::{Path, PathBuf};

#[derive(Default)]
struct State {
    root_dir: Option<PathBuf>,
    slots: Vec<PathBuf>,
    selected_slot: Option<usize>,
    player: i32,
    image: Option<TextureHandle>,
    backup_on_save: bool,
    status: String,
    json: Option<serde_json::Value>,
    ptr: String,
    primitive_entries: Vec<(String, wle_core::JsonEditValue)>,
    // UX helpers
    child_filter: String,
    new_key: String,
    new_value_json: String,
    array_index: usize,
    array_value_json: String,
    // Confirmation flags
    confirm_save: bool,
    confirm_remove: Option<String>,
    // Search
    doc: DocKind,
    last_backup_time: Option<DateTime<Local>>,
}

impl State {
    fn clear_slot_cache(&mut self) {
        self.image = None;
        self.json = None;
        self.primitive_entries.clear();
        self.ptr = "/root".into();
    }
    fn selected_slot_path(&self) -> Option<&Path> {
        self.selected_slot
            .and_then(|i| self.slots.get(i))
            .map(|p| p.as_path())
    }
}

struct AppGui {
    state: State,
}

impl AppGui {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            state: State {
                player: 1,
                backup_on_save: true,
                ptr: "/root".into(),
                child_filter: String::new(),
                new_key: String::new(),
                new_value_json: String::new(),
                array_index: 0,
                array_value_json: String::new(),
                confirm_save: false,
                doc: DocKind::Player,
                ..Default::default()
            },
        }
    }
    fn refresh_slots(&mut self) {
        if let Some(root) = &self.state.root_dir {
            self.state.slots = wle_core::saves::list_slots(root);
            if self.state.slots.is_empty() {
                self.state.selected_slot = None;
                self.state.status = "No SaveSlot_* found".into();
            } else {
                if self.state.selected_slot.is_none() {
                    self.state.selected_slot = Some(0);
                }
                self.state.status = format!("Found {} slot(s)", self.state.slots.len());
            }
            self.state.clear_slot_cache();
        }
    }
    fn pick_root_dir(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_directory(".").pick_folder() {
            self.state.root_dir = Some(dir);
            self.refresh_slots();
        }
    }
    fn ensure_loaded(&mut self, ctx: &egui::Context) {
        let Some(slot) = self.state.selected_slot_path().map(|p| p.to_path_buf()) else {
            return;
        };
        // Load SlotInfo small image
        if self.state.image.is_none() {
            let fp = slot.join("SlotInfo.sav");
            if fp.exists()
                && let Ok(doc) = wle_core::json::parse_binary(&fp)
                && let Some(info) = doc.as_save_slot_info()
            {
                let bytes = &info.small_image_data;
                let len = bytes.len();
                let mut dims: Option<(usize, usize, usize)> = None;
                for ch in [3usize, 4usize] {
                    if len % ch == 0 {
                        let side = ((len / ch) as f32).sqrt().floor() as usize;
                        if side > 0 && side * side * ch == len {
                            dims = Some((side, side, ch));
                            break;
                        }
                    }
                }
                if dims.is_none() {
                    if len == 256 * 256 * 3 {
                        dims = Some((256, 256, 3));
                    } else if len == 256 * 256 * 4 {
                        dims = Some((256, 256, 4));
                    }
                }
                if let Some((w, h, ch)) = dims {
                    let mut img = ColorImage::new([w, h], egui::Color32::BLACK);
                    if ch == 3 {
                        for y in 0..h {
                            for x in 0..w {
                                let sy = h - 1 - y;
                                let idx = (sy * w + x) * 3;
                                let r = bytes[idx];
                                let g = bytes[idx + 1];
                                let b = bytes[idx + 2];
                                img.pixels[y * w + x] = egui::Color32::from_rgb(r, g, b);
                            }
                        }
                    } else {
                        for y in 0..h {
                            for x in 0..w {
                                let sy = h - 1 - y;
                                let idx = (sy * w + x) * 4;
                                let r = bytes[idx];
                                let g = bytes[idx + 1];
                                let b = bytes[idx + 2];
                                let a = bytes[idx + 3];
                                img.pixels[y * w + x] =
                                    egui::Color32::from_rgba_unmultiplied(r, g, b, a);
                            }
                        }
                    }
                    let tex = ctx.load_texture("slot_image", img, egui::TextureOptions::LINEAR);
                    self.state.image = Some(tex);
                }
            }
        }
        // Load selected document JSON once
        if self.state.json.is_none() {
            let path = match self.state.doc {
                DocKind::Player => slot.join(format!("PlayerData_{}.sav", self.state.player)),
                DocKind::Mission => slot.join("MissionData.sav"),
                DocKind::Stats => slot.join("StatsData.sav"),
                DocKind::World => slot.join("WorldData.sav"),
            };
            if path.exists() {
                let opts = wle_core::json::JsonOpts::default();
                match wle_core::parse_file_to_json_value(&path, opts) {
                    Ok(v) => {
                        self.state.json = Some(v);
                        self.state.ptr = "/root".into();
                        self.refresh_primitive_entries();
                    }
                    Err(e) => {
                        self.state.status = format!("Load error: {}", e);
                    }
                }
            }
        }
    }
    fn refresh_primitive_entries(&mut self) {
        if let Some(v) = &self.state.json {
            let eff = browse_effective_ptr(v, &self.state.ptr);
            match wle_core::list_object_primitives_at(v, &eff) {
                Ok(kvs) => self.state.primitive_entries = kvs,
                Err(_) => self.state.primitive_entries.clear(),
            }
        }
    }
}

impl App for AppGui {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open GameSave Folder").clicked() {
                    self.pick_root_dir();
                }
                ui.separator();
                if ui.button("Create Backup Now").clicked()
                    && let Some(slot) = self.state.selected_slot_path().map(|p| p.to_path_buf())
                {
                    match wle_core::editor::zip_backup_slot(&slot) {
                        Ok(_) => {
                            self.state.status = "Backup created".into();
                            self.state.last_backup_time = Some(Local::now());
                        }
                        Err(e) => self.state.status = format!("Backup error: {}", e),
                    }
                }
                ui.checkbox(&mut self.state.backup_on_save, "Zip backup on save");
                if let Some(time) = self.state.last_backup_time {
                    ui.label(format!("Last backup: {}", time.format("%Y-%m-%d %H:%M:%S")));
                }
                ui.label(&self.state.status);
            });
        });

        egui::SidePanel::left("left").show(ctx, |ui| {
            ui.heading("Slots");
            if let Some(root) = &self.state.root_dir {
                ui.label(format!("Root: {}", root.display()));
            }
            let mut clicked_index: Option<usize> = None;
            for (i, p) in self.state.slots.iter().enumerate() {
                let sel = Some(i) == self.state.selected_slot;
                if ui
                    .selectable_label(sel, p.file_name().unwrap().to_string_lossy())
                    .clicked()
                {
                    clicked_index = Some(i);
                }
            }
            if let Some(i) = clicked_index {
                self.state.selected_slot = Some(i);
                self.state.clear_slot_cache();
            }
            ui.separator();
            ui.label("Player");
            for i in 1..=4 {
                if ui
                    .radio_value(&mut self.state.player, i, format!("Player {}", i))
                    .clicked()
                {
                    self.state.clear_slot_cache();
                }
            }
            ui.separator();
            ui.label("Document");
            if ui
                .radio_value(&mut self.state.doc, DocKind::Player, "Player Data")
                .clicked()
            {
                self.state.clear_slot_cache();
            }
            if ui
                .radio_value(&mut self.state.doc, DocKind::Mission, "Mission Data")
                .clicked()
            {
                self.state.clear_slot_cache();
            }
            if ui
                .radio_value(&mut self.state.doc, DocKind::Stats, "Stats Data")
                .clicked()
            {
                self.state.clear_slot_cache();
            }
            if ui
                .radio_value(&mut self.state.doc, DocKind::World, "World Data")
                .clicked()
            {
                self.state.clear_slot_cache();
            }
        });

        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(true)
            .default_height(250.0)
            .show(ctx, |ui| {
                ui.heading("Edit primitives at pointer");
                egui::ScrollArea::vertical()
                    .id_source("primitives_scroll")
                    .show(ui, |ui| {
                        for (key, val) in &mut self.state.primitive_entries {
                            ui.horizontal(|ui| {
                                ui.label(&*key);
                                match val {
                                    wle_core::JsonEditValue::Bool(b) => {
                                        ui.checkbox(b, "");
                                    }
                                    wle_core::JsonEditValue::Int(n) => {
                                        let mut v = *n;
                                        let resp = ui.add(egui::DragValue::new(&mut v).speed(1));
                                        if resp.changed() {
                                            *n = v;
                                        }
                                    }
                                    wle_core::JsonEditValue::Float(f) => {
                                        let mut v = *f;
                                        let resp = ui.add(egui::DragValue::new(&mut v).speed(0.5));
                                        if resp.changed() {
                                            *f = v;
                                        }
                                    }
                                    wle_core::JsonEditValue::Str(s) => {
                                        ui.text_edit_singleline(s);
                                    }
                                    wle_core::JsonEditValue::Null => {
                                        ui.label("null");
                                    }
                                }
                            });
                        }
                    });
                ui.separator();
                // Object/Array operations
                if let Some(j) = &self.state.json {
                    let eff = browse_effective_ptr(j, &self.state.ptr);
                    if let Some(node) = j.pointer(&eff) {
                        if node.is_object() {
                            ui.collapsing("Object ops", |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("New key:");
                                    ui.text_edit_singleline(&mut self.state.new_key);
                                    ui.label("Value (JSON):");
                                    ui.text_edit_singleline(&mut self.state.new_value_json);
                                    if ui.button("Add key").clicked() {
                                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(
                                            &self.state.new_value_json,
                                        ) {
                                            if let Some(j2) = &mut self.state.json {
                                                let _ = wle_core::add_key(
                                                    j2,
                                                    &eff,
                                                    &self.state.new_key,
                                                    val,
                                                );
                                                self.refresh_primitive_entries();
                                            }
                                        } else {
                                            self.state.status = "Invalid JSON for value".into();
                                        }
                                    }
                                    if eff != "/root"
                                        && !eff.is_empty()
                                        && ui.button("Remove this node").clicked()
                                    {
                                        self.state.confirm_remove = Some(eff.clone());
                                    }
                                });
                            });
                        } else if node.is_array() {
                            ui.collapsing("Array ops", |ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Index:");
                                    let _ = ui.add(
                                        egui::DragValue::new(&mut self.state.array_index).speed(1),
                                    );
                                    ui.label("Value (JSON):");
                                    ui.text_edit_singleline(&mut self.state.array_value_json);
                                    if ui.button("Insert").clicked() {
                                        match serde_json::from_str::<serde_json::Value>(
                                            &self.state.array_value_json,
                                        ) {
                                            Ok(val) => {
                                                if let Some(j2) = &mut self.state.json {
                                                    let _ = wle_core::array_insert(
                                                        j2,
                                                        &eff,
                                                        self.state.array_index,
                                                        val,
                                                    );
                                                    self.refresh_primitive_entries();
                                                }
                                            }
                                            Err(_) => {
                                                self.state.status = "Invalid JSON for value".into();
                                            }
                                        }
                                    }
                                    if ui.button("Remove").clicked()
                                        && let Some(j2) = &mut self.state.json
                                    {
                                        let _ = wle_core::array_remove(
                                            j2,
                                            &eff,
                                            self.state.array_index,
                                        );
                                        self.refresh_primitive_entries();
                                    }
                                });
                            });
                        }
                    }
                }
                if let Some(ptr_to_remove) = self.state.confirm_remove.clone() {
                    ui.horizontal(|ui| {
                        ui.label(format!("Confirm removal of {}?", ptr_to_remove));
                        if ui.button("Confirm").clicked() {
                            if let Some(j2) = &mut self.state.json {
                                let _ = wle_core::remove_at_pointer(j2, &ptr_to_remove);
                                self.state.ptr = parent_pointer(&ptr_to_remove)
                                    .unwrap_or("/root")
                                    .to_string();
                                self.refresh_primitive_entries();
                            }
                            self.state.confirm_remove = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.state.confirm_remove = None;
                        }
                    });
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Save to .sav").clicked() {
                        self.state.confirm_save = true;
                    }
                });

                if self.state.confirm_save {
                    ui.horizontal(|ui| {
                        ui.label("Confirm save to .sav?");
                        if ui.button("Confirm").clicked() {
                            let selected_slot_path =
                                self.state.selected_slot_path().map(|p| p.to_path_buf());
                            if let Some(j) = &mut self.state.json {
                                let eff = browse_effective_ptr(j, &self.state.ptr);
                                match wle_core::apply_object_primitive_updates(
                                    j,
                                    &eff,
                                    &self.state.primitive_entries,
                                ) {
                                    Ok(_) => {
                                        if let Some(slot) = selected_slot_path {
                                            let path = match self.state.doc {
                                                DocKind::Player => slot.join(format!(
                                                    "PlayerData_{}.sav",
                                                    self.state.player
                                                )),
                                                DocKind::Mission => slot.join("MissionData.sav"),
                                                DocKind::Stats => slot.join("StatsData.sav"),
                                                DocKind::World => slot.join("WorldData.sav"),
                                            };
                                            if path.exists() {
                                                if self.state.backup_on_save {
                                                    let _ =
                                                        wle_core::editor::zip_backup_slot(&slot);
                                                    self.state.last_backup_time =
                                                        Some(Local::now());
                                                }
                                                match wle_core::write_binfmt_file_from_json(
                                                    &path, j,
                                                ) {
                                                    Ok(_) => self.state.status = "Saved".into(),
                                                    Err(e) => {
                                                        self.state.status =
                                                            format!("Save error: {}", e)
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => self.state.status = format!("Update error: {}", e),
                                }
                            }
                            self.state.confirm_save = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.state.confirm_save = false;
                        }
                    });
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.ensure_loaded(ctx);
            if let Some(tex) = &self.state.image {
                ui.image((tex.id(), tex.size_vec2()));
            }
            ui.separator();
            ui.collapsing("JSON Browser Controls", |ui| {
                ui.heading("JSON Browser");
                // Breadcrumbs
                if !self.state.ptr.is_empty() {
                    let ptr_snapshot = self.state.ptr.clone();
                    let parts: Vec<&str> = ptr_snapshot.split('/').skip(1).collect();
                    let mut accum = String::new();
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Path:");
                        for (i, part) in parts.iter().enumerate() {
                            accum.push('/');
                            accum.push_str(part);
                            if ui.link((*part).to_string()).clicked() {
                                self.state.ptr = accum.clone();
                                self.refresh_primitive_entries();
                            }
                            if i + 1 < parts.len() {
                                ui.label("/");
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("Pointer:");
                    ui.text_edit_singleline(&mut self.state.ptr);
                    if ui.button("Up").clicked()
                        && let Some(p) = parent_pointer(&self.state.ptr)
                    {
                        self.state.ptr = p.to_string();
                        self.refresh_primitive_entries();
                    }
                    if ui.button("Refresh").clicked() {
                        self.refresh_primitive_entries();
                    }
                });
            });

            // Handle search/filter UI
            let mut pending_ptr_change: Option<String> = None;

            ui.horizontal(|ui| {
                ui.label("Search / Filter:");
                let search_response = ui.text_edit_singleline(&mut self.state.child_filter);
                if ui.button("Clear").clicked() {
                    self.state.child_filter.clear();
                }
                if !self.state.child_filter.is_empty() {
                    ui.label("(searches keys & values, Enter to jump to first result)");
                }

                // Handle Enter key to navigate to first result
                if search_response.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !self.state.child_filter.is_empty()
                    && let Some(json) = &self.state.json
                {
                    let search_results =
                        find_key_or_value_paths(Some(json), &self.state.child_filter, 64);
                    if let Some(first_result) = search_results.first() {
                        pending_ptr_change = Some(first_result.clone());
                    }
                }
            });

            if let Some(v) = &self.state.json {
                let eff = browse_effective_ptr(v, &self.state.ptr);

                if !self.state.child_filter.is_empty() {
                    // If we have a filter, show search results across the entire tree
                    let search_results =
                        find_key_or_value_paths(Some(v), &self.state.child_filter, 64);
                    ui.label(format!(
                        "Search Results ({}) - Click path to navigate or edit values directly:",
                        search_results.len()
                    ));
                    egui::ScrollArea::vertical()
                        .id_source("search_scroll")
                        .show(ui, |ui| {
                            for p in &search_results {
                                ui.horizontal(|ui| {
                                    // Show the path as a clickable link
                                    if ui.link(p.as_str()).clicked() {
                                        pending_ptr_change = Some(p.clone());
                                    }

                                    // Show value and allow direct editing for primitive types
                                    if let Some(node) = v.pointer(p) {
                                        if node.is_object() {
                                            ui.label("(object)");
                                        } else if node.is_array() {
                                            ui.label("(array)");
                                        } else {
                                            ui.label("(value)");
                                            // Show the current value in a compact way
                                            let value_str = match node {
                                                serde_json::Value::String(s) => {
                                                    format!("\"{}\"", s)
                                                }
                                                serde_json::Value::Number(n) => n.to_string(),
                                                serde_json::Value::Bool(b) => b.to_string(),
                                                serde_json::Value::Null => "null".to_string(),
                                                _ => "...".to_string(),
                                            };
                                            ui.label(format!("= {}", value_str));

                                            // Show edit button for primitive values
                                            if ui.small_button("Edit").clicked() {
                                                pending_ptr_change = Some(p.clone());
                                            }
                                        }
                                    }
                                });
                            }
                        });
                } else if let Ok(children) = wle_core::list_children(v, &eff) {
                    // Display children of current pointer (original logic)
                    ui.label("Children:");
                    egui::ScrollArea::vertical()
                        .id_source("children_scroll")
                        .show(ui, |ui| {
                            for c in &children {
                                let label = format!(
                                    "{} ({:?}{})",
                                    c.key_or_index,
                                    c.kind,
                                    c.len.map(|n| format!(", {}", n)).unwrap_or_default()
                                );
                                if ui.selectable_label(false, label).clicked() {
                                    let tok = escape_token(&c.key_or_index);
                                    let base = if eff.ends_with("/$value") {
                                        eff.trim_end_matches("/$value")
                                    } else {
                                        eff.as_str()
                                    };
                                    pending_ptr_change = Some(if base == "/" || base.is_empty() {
                                        format!("/{}", tok)
                                    } else {
                                        format!("{}/{}", base.trim_end_matches('/'), tok)
                                    });
                                }
                            }
                        });
                }
            }

            // Apply any pending pointer changes
            if let Some(new_ptr) = pending_ptr_change {
                self.state.ptr = new_ptr;
                self.refresh_primitive_entries();
            }
            ui.separator();
            ui.label(&self.state.status);
        });
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::viewport::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Wobbly Life Editor",
        native_options,
        Box::new(|cc| Ok(Box::new(AppGui::new(cc)))),
    )
}

fn parent_pointer(ptr: &str) -> Option<&str> {
    if ptr.is_empty() || ptr == "/" {
        return None;
    }
    if let Some(pos) = ptr.rfind('/') {
        if pos == 0 {
            return Some("/");
        } else {
            return Some(&ptr[..pos]);
        }
    }
    None
}

fn escape_token(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

fn browse_effective_ptr(root: &serde_json::Value, ptr: &str) -> String {
    if let Some(node) = root.pointer(ptr)
        && let Some(obj) = node.as_object()
        && obj.contains_key("$value")
    {
        return format!("{}/$value", ptr.trim_end_matches('/'));
    }
    ptr.to_string()
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum DocKind {
    #[default]
    Player,
    Mission,
    Stats,
    World,
}

fn find_key_paths(root: Option<&serde_json::Value>, query: &str, limit: usize) -> Vec<String> {
    use std::collections::HashSet;
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if query.trim().is_empty() {
        return out;
    }
    let q = query.to_lowercase();
    // start from /root subtree if possible
    if let Some(val) = root {
        if let Some(sub) = val.pointer("/root") {
            dfs_collect_parent_hits("/root", sub, &q, &mut out, &mut seen, limit);
        } else {
            dfs_collect_parent_hits("/root", val, &q, &mut out, &mut seen, limit);
        }
    }
    out
}

fn dfs_collect_parent_hits(
    base: &str,
    val: &serde_json::Value,
    q: &str,
    out: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    match val {
        serde_json::Value::Object(map) => {
            let mut hit = false;
            for (k, v) in map.iter() {
                if k.to_lowercase().contains(q) {
                    hit = true;
                }
                let next = format!("{}/{}", base.trim_end_matches('/'), escape_token(k));
                dfs_collect_parent_hits(&next, v, q, out, seen, limit);
                if out.len() >= limit {
                    return;
                }
            }
            if hit {
                let key = base.to_string();
                if seen.insert(key.clone()) {
                    out.push(key);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let next = format!("{}/{}", base.trim_end_matches('/'), i);
                dfs_collect_parent_hits(&next, v, q, out, seen, limit);
                if out.len() >= limit {
                    return;
                }
            }
        }
        _ => {}
    }
}

fn find_value_paths(root: Option<&serde_json::Value>, query: &str, limit: usize) -> Vec<String> {
    use std::collections::HashSet;
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    if query.trim().is_empty() {
        return out;
    }
    let q = query.trim();
    let kind = if q.eq_ignore_ascii_case("true") {
        Some(ValueMatch::Bool(true))
    } else if q.eq_ignore_ascii_case("false") {
        Some(ValueMatch::Bool(false))
    } else if let Ok(i) = q.parse::<i64>() {
        Some(ValueMatch::I64(i))
    } else if let Ok(f) = q.parse::<f64>() {
        Some(ValueMatch::F64(f))
    } else {
        Some(ValueMatch::Str(q.to_lowercase()))
    };
    if let (Some(val), Some(k)) = (root, kind) {
        if let Some(sub) = val.pointer("/root") {
            dfs_collect_value_hits("/root", sub, &k, &mut out, &mut seen, limit);
        } else {
            dfs_collect_value_hits("/root", val, &k, &mut out, &mut seen, limit);
        }
    }
    out
}

enum ValueMatch {
    Bool(bool),
    I64(i64),
    F64(f64),
    Str(String),
}

fn value_equals(v: &serde_json::Value, k: &ValueMatch) -> bool {
    match k {
        ValueMatch::Bool(b) => v.as_bool() == Some(*b),
        ValueMatch::I64(i) => v.as_i64() == Some(*i),
        ValueMatch::F64(f) => v.as_f64() == Some(*f),
        ValueMatch::Str(s) => v
            .as_str()
            .map(|x| x.to_lowercase().contains(s))
            .unwrap_or(false),
    }
}

fn dfs_collect_value_hits(
    base: &str,
    val: &serde_json::Value,
    k: &ValueMatch,
    out: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
    limit: usize,
) -> bool {
    if out.len() >= limit {
        return false;
    }
    match val {
        serde_json::Value::Object(map) => {
            let mut hit = false;
            for (ck, cv) in map.iter() {
                if value_equals(cv, k) {
                    hit = true;
                }
                let next = format!("{}/{}", base.trim_end_matches('/'), escape_token(ck));
                if dfs_collect_value_hits(&next, cv, k, out, seen, limit) {
                    hit = true;
                }
                if out.len() >= limit {
                    break;
                }
            }
            if hit && seen.insert(base.to_string()) {
                out.push(base.to_string());
            }
            hit
        }
        serde_json::Value::Array(arr) => {
            let mut hit_any = false;
            for (i, el) in arr.iter().enumerate() {
                let next = format!("{}/{}", base.trim_end_matches('/'), i);
                let mut elem_hit = false;
                if value_equals(el, k) {
                    elem_hit = true;
                }
                if dfs_collect_value_hits(&next, el, k, out, seen, limit) {
                    elem_hit = true;
                }
                if elem_hit {
                    if seen.insert(next.clone()) {
                        out.push(next);
                    }
                    hit_any = true;
                }
                if out.len() >= limit {
                    break;
                }
            }
            hit_any
        }
        _ => value_equals(val, k),
    }
}

fn find_key_or_value_paths(
    root: Option<&serde_json::Value>,
    query: &str,
    limit: usize,
) -> Vec<String> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let key_paths = find_key_paths(root, query, limit);
    let value_paths = find_value_paths(root, query, limit);

    let mut combined = key_paths;
    combined.extend(value_paths);

    use std::collections::HashSet;
    let set: HashSet<String> = combined.into_iter().collect();
    set.into_iter().collect()
}
