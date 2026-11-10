use super::*;
use crate::SourceKind;
use serde_json::json;

fn fixture() -> (tempfile::TempDir, LocalIndex, String, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("component");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("manifest.json"), serde_json::to_vec(&json!({
        "format_version": 2,
        "header": { "name": "Editor test", "description": "Test", "uuid": uuid::Uuid::new_v4(), "version": [1,0,0] },
        "modules": [{"type": "data", "uuid": uuid::Uuid::new_v4(), "version": [1,0,0]}]
    })).unwrap()).unwrap();
    let index = LocalIndex::open(temp.path().join("index/db.sqlite")).unwrap();
    index.add_source(SourceKind::Single, &root).unwrap();
    let id = index.component_id(&root).unwrap();
    (temp, index, id, root.join(".mcdev.json"))
}

fn edit(key: &str, value: Value) -> ConfigEdit {
    ConfigEdit {
        path: key.split('.').map(str::to_owned).collect(),
        value,
    }
}

#[test]
fn missing_config_is_read_only_until_explicit_save() {
    let (_temp, index, id, path) = fixture();
    let before = load(&index, &id).unwrap();
    assert!(before.revision.is_none());
    assert!(!path.exists());
    let saved = save(&index, &id, None, &[edit("world_name", json!("World"))]).unwrap();
    assert_eq!(saved.data["world_name"], "World");
    assert!(saved.revision.is_some());
}

#[test]
fn patch_preserves_bom_crlf_comments_unknown_fields_and_skin_metadata() {
    let (_temp, index, id, path) = fixture();
    let original = "\u{feff}{\r\n  // retained\r\n  \"skin_info\": {\"skin\":\"old.png\", /* keep */ \"slim\":false, \"custom\":7},\r\n  \"unknown\": {\"nested\":[1,2]},\r\n}\r\n";
    fs::write(&path, original).unwrap();
    let before = load(&index, &id).unwrap();
    let saved = save(
        &index,
        &id,
        before.revision.as_deref(),
        &[
            edit("skin_info.skin", json!("new.png")),
            edit("skin_info.slim", json!(true)),
        ],
    )
    .unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.starts_with('\u{feff}'));
    assert!(content.contains("// retained"));
    assert!(content.contains("/* keep */"));
    assert!(!content.replace("\r\n", "").contains('\n'));
    assert_eq!(saved.data["skin_info"]["custom"], 7);
    assert_eq!(saved.data["unknown"], before.data["unknown"]);
    assert_eq!(saved.data["skin_info"]["skin"], "new.png");
}

#[test]
fn stale_deleted_and_newly_created_configs_cannot_be_overwritten() {
    let (_temp, index, id, path) = fixture();
    fs::write(&path, "{}").unwrap();
    let old = load(&index, &id).unwrap();
    fs::write(&path, "{\"user_name\":\"external\"}").unwrap();
    assert!(
        save(
            &index,
            &id,
            old.revision.as_deref(),
            &[edit("user_name", json!("mine"))]
        )
        .is_err()
    );
    assert!(save(&index, &id, None, &[edit("user_name", json!("mine"))]).is_err());
    assert_eq!(load(&index, &id).unwrap().data["user_name"], "external");
    fs::remove_file(&path).unwrap();
    assert!(save(&index, &id, old.revision.as_deref(), &[]).is_err());
    assert!(!path.exists());
}

#[test]
fn invalid_edits_are_rejected_before_any_write() {
    let (_temp, index, id, path) = fixture();
    for change in [
        edit("unexpected", json!(true)),
        edit("game_mode", json!(8)),
        edit("world_seed", json!(1.5)),
        edit("world_seed", json!(9007199254740992_u64)),
        edit("mcp_server_config.server_port", json!(0)),
        edit("world_folder_name", json!("../other")),
        edit("window_style.opacity", json!(256)),
        edit("included_mod_dirs", json!([{"path":"", "enabled":true}])),
        edit("skin_info.slim", json!("false")),
        ConfigEdit {
            path: vec![],
            value: json!(true),
        },
    ] {
        assert!(save(&index, &id, None, &[change]).is_err());
        assert!(!path.exists());
    }
}

#[test]
fn broken_json_and_non_object_nested_fields_are_not_replaced() {
    let (_temp, index, id, path) = fixture();
    for original in ["{broken", "[]"] {
        fs::write(&path, original).unwrap();
        assert!(load(&index, &id).is_err());
        assert!(save(&index, &id, None, &[edit("world_name", json!("new"))]).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
    fs::write(&path, "{\"skin_info\":null}").unwrap();
    let before = load(&index, &id).unwrap();
    assert!(
        save(
            &index,
            &id,
            before.revision.as_deref(),
            &[edit("skin_info.slim", json!(true))]
        )
        .is_err()
    );
}

#[test]
fn lock_and_unknown_component_are_rejected() {
    let (_temp, index, id, path) = fixture();
    let _lock = index.try_lock_mutations().unwrap();
    assert!(matches!(save(&index, &id, None, &[]), Err(CoreError::Busy)));
    assert!(load(&index, "unregistered").is_err());
    assert!(!path.exists());
}

#[test]
fn duplicate_properties_cannot_report_a_successful_but_ineffective_save() {
    let (_temp, index, id, path) = fixture();
    let original = "{\"world_name\":\"first\",\"world_name\":\"second\"}";
    fs::write(&path, original).unwrap();
    let before = load(&index, &id).unwrap();
    let result = save(
        &index,
        &id,
        before.revision.as_deref(),
        &[edit("world_name", json!("new"))],
    );
    if let Ok(saved) = result {
        assert_eq!(saved.data["world_name"], "new");
    } else {
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }
}

#[test]
#[cfg(windows)]
fn active_managed_game_blocks_config_writes() {
    use crate::mcdk_session::{McdkSession, SESSION_KEY, SessionState, inspect_process};
    let (_temp, index, id, path) = fixture();
    let process = inspect_process(std::process::id()).unwrap().unwrap();
    McdkStore::new(index.clone())
        .write(
            SESSION_KEY,
            &SessionState {
                session: Some(McdkSession {
                    component_id: id.clone(),
                    component_path: path.parent().unwrap().into(),
                    executable: process.executable,
                    created_at: process.created_at,
                    pid: std::process::id(),
                    version: "1.6.1".into(),
                }),
                last_exit: None,
            },
        )
        .unwrap();
    assert!(load(&index, &id).is_ok());
    assert!(save(&index, &id, None, &[edit("world_name", json!("new"))]).is_err());
    assert!(!path.exists());
}

#[test]
fn custom_library_deduplicates_and_never_deletes_skin_files() {
    let (_temp, index, id, path) = fixture();
    let skin = path.with_file_name("test.png");
    let mut bytes = vec![];
    {
        let encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[200])
            .unwrap();
    }
    fs::write(&skin, bytes).unwrap();
    let first = crate::skins::update_library(&index, &id, "test.png", false, false).unwrap();
    let second = crate::skins::update_library(&index, &id, &first[0].path, true, false).unwrap();
    assert_eq!(second.len(), 1);
    assert!(second[0].slim);
    assert!(
        crate::skins::update_library(&index, &id, &first[0].path, false, true)
            .unwrap()
            .is_empty()
    );
    assert!(skin.exists());
    assert!(!path.exists());
}
