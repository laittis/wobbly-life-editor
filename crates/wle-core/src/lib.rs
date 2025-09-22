//! wle-core: Core data models, parser, JSON export, and editing utilities
//!
//! This crate focuses on a small, well-factored surface:
//! - BinaryFormatter reader (dynamic graph) used by all features
//! - Minimal typed helpers for data we care about (SlotInfo convenience)
//! - JSON dump for any .sav for CLI use
//! - Generic JSON edit API (JSON Pointer), and slot zip backup
//!
pub mod binfmt;
pub mod model;
pub mod json;
pub mod saves;
pub mod editor;
pub mod edit;
pub mod binfmt_write;

// Re-export generic JSON edit API
pub use edit::{
    JsonEditValue, JsonKind, ChildInfo,
    parse_file_to_json_value, document_to_json_value,
    get_by_pointer, set_by_pointer, set_raw_by_pointer,
    list_object_primitives_at, list_children, apply_object_primitive_updates,
    add_key, remove_at_pointer, array_insert, array_remove,
    write_json_to_file,
};
pub use binfmt_write::{write_binfmt_from_json, write_binfmt_file_from_json};
