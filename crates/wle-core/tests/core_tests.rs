#[test]
fn parse_and_dump_dynamic_json() {
    // Build a small SlotInfo .sav and verify dynamic JSON dump
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("SlotInfo.sav");
    let img = vec![0u8; 16 * 16 * 3];
    let bytes = wle_core::editor::build_slot_info_bytes(2, "2025-09-22 12:00", &img);
    std::fs::write(&p, bytes).unwrap();
    let doc = wle_core::json::parse_binary(&p).expect("parse");
    let js = wle_core::json::dump_dynamic_json(&doc, wle_core::json::JsonOpts::default());
    assert!(js.contains("\"$rootClass\""));
    assert!(js.starts_with("{"));
}

#[test]
fn decode_slot_info_image_len() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("SlotInfo.sav");
    let img = vec![0u8; 16 * 16 * 3];
    let bytes = wle_core::editor::build_slot_info_bytes(2, "2025-09-22 12:00", &img);
    std::fs::write(&p, bytes).unwrap();
    let doc = wle_core::json::parse_binary(&p).expect("parse");
    let cls = doc.root_class_name().unwrap_or("<none>").to_string();
    let info = doc
        .as_save_slot_info()
        .unwrap_or_else(|| panic!("slotinfo root={}", cls));
    assert!(info.small_image_data.len() > 0);
    assert!(info.last_selected_player_slot >= 0);
}

#[test]
fn generic_json_pointer_edit_ops() {
    use tempfile::tempdir;
    use wle_core::json::JsonOpts;
    use wle_core::{
        JsonEditValue, add_key, array_insert, array_remove, get_by_pointer, list_children,
        parse_file_to_json_value, remove_at_pointer, set_by_pointer, write_json_to_file,
    };

    let dir = tempdir().unwrap();
    let src = dir.path().join("data.json");
    let out = dir.path().join("out.json");
    let content = r#"{"$rootClass":"X","root":{"a":{"b":[1,2,3],"c":true}}}"#;
    std::fs::write(&src, content).unwrap();

    let mut v = parse_file_to_json_value(&src, JsonOpts::default()).expect("load json");
    // get
    let n = get_by_pointer(&v, "/root/a/b/1").unwrap();
    assert_eq!(n, serde_json::json!(2));
    // list
    let kids = list_children(&v, "/root/a").unwrap();
    assert!(kids.iter().any(|c| c.key_or_index == "b"));
    assert!(kids.iter().any(|c| c.key_or_index == "c"));
    // set bool
    set_by_pointer(&mut v, "/root/a/c", JsonEditValue::Bool(false)).unwrap();
    assert_eq!(
        get_by_pointer(&v, "/root/a/c").unwrap(),
        serde_json::json!(false)
    );
    // array insert/remove
    array_insert(&mut v, "/root/a/b", 1, serde_json::json!(42)).unwrap();
    assert_eq!(
        get_by_pointer(&v, "/root/a/b/1").unwrap(),
        serde_json::json!(42)
    );
    array_remove(&mut v, "/root/a/b", 3).unwrap(); // remove former last element 3
    // add/remove key
    add_key(&mut v, "/root/a", "d", serde_json::json!("hi")).unwrap();
    assert_eq!(
        get_by_pointer(&v, "/root/a/d").unwrap(),
        serde_json::json!("hi")
    );
    remove_at_pointer(&mut v, "/root/a/d").unwrap();
    assert!(get_by_pointer(&v, "/root/a/d").is_none());
    // write
    write_json_to_file(&out, &v).unwrap();
    let s = std::fs::read_to_string(&out).unwrap();
    assert!(s.contains("\"root\""));
}

#[test]
fn zip_backup_arbitrary_dir() {
    use std::fs;
    use std::io::Write as _;
    use tempfile::tempdir;
    let d = tempdir().unwrap();
    // Create small tree
    fs::create_dir_all(d.path().join("a/b")).unwrap();
    let mut f = fs::File::create(d.path().join("a/b/x.txt")).unwrap();
    writeln!(&mut f, "hello").unwrap();
    let zip = wle_core::editor::zip_backup_slot(d.path()).unwrap();
    assert!(zip.exists());
}

#[test]
fn write_generic_binfmt_roundtrip() {
    use tempfile::tempdir;
    use wle_core::{json, write_binfmt_file_from_json};
    let dir = tempdir().unwrap();
    let p = dir.path().join("gen.sav");
    let root = serde_json::json!({
        "$rootClass": "TestRoot",
        "root": {
            "$class": "TestRoot",
            "a": 123,
            "b": "hello",
            "c": true,
            "d": [1,2,3],
            "child": { "$class": "Child", "x": 1, "y": 2 }
        }
    });
    write_binfmt_file_from_json(&p, &root).expect("write");
    let doc = wle_core::json::parse_binary(&p).expect("parse");
    // Validate class and some members via dynamic JSON
    let dumped = wle_core::json::dump_dynamic_json(&doc, json::JsonOpts::default());
    assert!(dumped.contains("\"$rootClass\": \"TestRoot\""));
    assert!(dumped.contains("\"a\":123"));
    assert!(dumped.contains("\"b\":\"hello\""));
    assert!(dumped.contains("\"c\":true"));
    assert!(dumped.contains("\"d\":[1,2,3]"));
    assert!(dumped.contains("\"$class\":\"Child\""));
}
