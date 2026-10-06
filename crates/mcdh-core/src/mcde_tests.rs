use super::*;
use crate::LocalIndex;

fn image(dll: bool) -> Vec<u8> {
    let mut bytes = vec![0u8; 256];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c] = 64;
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&[0x64, 0x86]);
    bytes[86] = 2;
    bytes[87] = if dll { 0x20 } else { 0 };
    bytes[88..90].copy_from_slice(&[0x0b, 0x02]);
    bytes
}

struct Fixture {
    _temp: tempfile::TempDir,
    store: McdeStore,
    project: PathBuf,
    editor: PathBuf,
    host: PathBuf,
    game: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let project = root.join("project");
        let editor = root.join("editor");
        let host = root.join("host");
        let game = root.join("3.10.0.420447/Minecraft.Windows.exe");
        for dir in [&project, &editor, &host, game.parent().unwrap()] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(&game, image(false)).unwrap();
        for name in REQUIRED_IMAGES {
            let path = editor.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, image(name.ends_with(".dll"))).unwrap();
        }
        for name in ["assets", "signatures", "licenses"] {
            fs::create_dir(editor.join(name)).unwrap();
        }
        for name in [
            "assets/native_python.py",
            "assets/native_ui.py",
            "signatures/cpython-native.json",
        ] {
            fs::write(editor.join(name), "offline fixture only").unwrap();
        }
        write_json(&editor.join("plugin.json"), &json!({
            "schema": 1, "id": PLUGIN_ID, "version": "0.1.0", "abi": {"major": 1, "minor": 0},
            "entry_symbol": "mcdk_plugin_entry", "libraries": {"windows.x86_64": "mcdeveditor-plugin.dll"}
        })).unwrap();
        let bytes = image(false);
        fs::write(host.join("mcdk.exe"), &bytes).unwrap();
        fs::write(host.join("mcdev-tracy-bridge.dll"), image(true)).unwrap();
        write_json(&host.join("manifest.json"), &json!({
            "schema": 1, "repository": "https://github.com/GitHub-Zero123/MCDevTool",
            "branch": HOST_BRANCH, "revision": HOST_REVISION, "plugins_enabled": true,
            "abi": {"major": 1, "minor": 0}, "capabilities": ["mcdk.game.process.create"],
            "game_versions": ["3.10.0.420447"], "binary": {"size": bytes.len(), "sha256": format!("{:x}", Sha256::digest(&bytes))}
        })).unwrap();
        write_json(
            &project.join(".mcdev.json"),
            &json!({"game_executable_path": game}),
        )
        .unwrap();
        let store = McdeStore::new(McdkStore::new(
            LocalIndex::open(root.join("data/index.db")).unwrap(),
        ));
        Self {
            _temp: temp,
            store,
            project,
            editor,
            host,
            game,
        }
    }

    fn config(&self) -> Value {
        serde_json::from_slice(&fs::read(self.project.join(".mcdev.json")).unwrap()).unwrap()
    }
    fn prepare(&self) -> Result<PreparedEditorLaunch> {
        self.store
            .prepare_launch(&self.project, &self.editor, &self.host)
    }
}

#[test]
fn validates_host_branch_abi_capability_hash_and_editor_images() {
    let f = Fixture::new();
    assert!(f.store.status(&f.editor, &f.host).unwrap().available);
    let original: Value =
        serde_json::from_slice(&fs::read(f.host.join("manifest.json")).unwrap()).unwrap();
    for (key, value) in [
        ("branch", json!("main")),
        ("revision", json!("bad")),
        ("plugins_enabled", json!(false)),
        ("capabilities", json!([])),
        ("abi", json!({"major": 2, "minor": 0})),
        ("game_versions", json!(["3.9.0.401155"])),
    ] {
        let mut manifest = original.clone();
        manifest[key] = value;
        write_json(&f.host.join("manifest.json"), &manifest).unwrap();
        assert!(verify_host(&f.host).is_err(), "{key}");
        assert!(!f.store.status(&f.editor, &f.host).unwrap().available);
    }
    write_json(&f.host.join("manifest.json"), &original).unwrap();
    fs::write(f.host.join("mcdk.exe"), b"corrupt").unwrap();
    assert!(verify_host(&f.host).is_err());
    fs::write(f.editor.join("mcdeveditor-hook.dll"), image(false)).unwrap();
    assert!(verify_editor(&f.editor).is_err());
    fs::write(f.editor.join("mcdeveditor-hook.dll"), image(true)).unwrap();
    fs::remove_file(f.editor.join("platforms/qwindows.dll")).unwrap();
    assert!(verify_editor(&f.editor).is_err());
}

#[test]
fn persists_directory_without_affecting_release_preferences_and_rejects_invalid_choice() {
    let f = Fixture::new();
    f.store.store.set_auto_update(false).unwrap();
    f.store.set_directory(Some(&f.editor)).unwrap();
    let other = McdeStore::new(f.store.store.clone());
    assert!(
        other
            .status(Path::new("missing"), &f.host)
            .unwrap()
            .configured
    );
    assert!(
        other
            .status(Path::new("missing"), &f.host)
            .unwrap()
            .available
    );
    assert!(f.store.set_directory(Some(&f.project)).is_err());
    assert_eq!(
        other
            .status(Path::new("missing"), &f.host)
            .unwrap()
            .directory,
        Some(f.editor.clone())
    );
    f.store.set_directory(None).unwrap();
    assert!(!other.status(&f.editor, &f.host).unwrap().configured);
    assert!(!f.store.store.preferences().unwrap().auto_update);
    assert!(
        !other
            .status(Path::new("missing"), &f.host)
            .unwrap()
            .available
    );
}

#[test]
fn rejects_missing_or_empty_native_bridge_resources() {
    let f = Fixture::new();
    for name in [
        "assets/native_python.py",
        "assets/native_ui.py",
        "signatures/cpython-native.json",
    ] {
        fs::remove_file(f.editor.join(name)).unwrap();
        assert!(verify_editor(&f.editor).is_err(), "{name}");
        fs::write(f.editor.join(name), "").unwrap();
        assert!(verify_editor(&f.editor).is_err(), "{name}");
        fs::write(f.editor.join(name), "offline fixture only").unwrap();
        assert!(verify_editor(&f.editor).is_ok());
    }
    fs::remove_file(f.host.join("mcdev-tracy-bridge.dll")).unwrap();
    assert!(verify_host(&f.host).is_err());
}

#[test]
fn preserves_source_jsonc_and_resolves_all_host_paths_in_owned_profile() {
    let f = Fixture::new();
    let mut config = f.config();
    config["included_mod_dirs"] =
        json!(["RP", {"path": "BP", "enabled": false, "hot_reload": false, "extension": 1}]);
    config["world_source_path"] = json!("world");
    config["skin_info"] = json!({"skin": "skin.png", "slim": true});
    config["future_setting"] = json!({"a": [1, "unknown", true]});
    config["plugins"] = json!([{ "id": "other.disabled", "path": "plugins/other", "enable": false, "config": {"unknown": true}},
        {"id": PLUGIN_ID, "path": "old", "enable": false}]);
    let source = format!(
        "\u{feff}// keep source comments\n{}",
        serde_json::to_string_pretty(&config).unwrap()
    );
    fs::write(f.project.join(".mcdev.json"), source.as_bytes()).unwrap();
    let launch = f.prepare().unwrap();
    let generated: Value =
        serde_json::from_slice(&fs::read(launch.directory().join(".mcdev.json")).unwrap()).unwrap();
    assert_eq!(
        fs::read(f.project.join(".mcdev.json")).unwrap(),
        source.as_bytes()
    );
    assert_eq!(generated["future_setting"], config["future_setting"]);
    assert_eq!(
        generated["included_mod_dirs"][0],
        json!(f.project.join("RP"))
    );
    assert_eq!(
        generated["included_mod_dirs"][1]["path"],
        json!(f.project.join("BP"))
    );
    assert_eq!(generated["included_mod_dirs"][1]["hot_reload"], false);
    assert_eq!(generated["included_mod_dirs"][1]["extension"], 1);
    assert_eq!(
        generated["world_source_path"],
        json!(f.project.join("world"))
    );
    assert_eq!(
        generated["skin_info"]["skin"],
        json!(f.project.join("skin.png"))
    );
    assert_eq!(
        generated["plugins"][0]["path"],
        json!(f.project.join("plugins/other"))
    );
    assert_eq!(
        generated["plugins"][0]["config"],
        config["plugins"][0]["config"]
    );
    assert_eq!(generated["plugins"][1]["path"], json!(f.editor));
    assert_eq!(
        generated["plugins"][1]["config"]["projectRoot"],
        json!(f.project)
    );
    assert_eq!(generated["plugins"][1]["config"]["closeGameOnClose"], true);
    assert!(
        launch
            .directory()
            .starts_with(f.store.root().join("sessions"))
    );
    let path = launch.directory().to_path_buf();
    drop(launch);
    assert!(!path.exists());
}

#[test]
fn keeps_profile_after_launch_and_resolves_automatic_world_from_original_project() {
    let f = Fixture::new();
    fs::write(f.project.join("level.dat"), "owned fixture").unwrap();
    let launch = f.prepare().unwrap();
    let directory = launch.persist();
    let config: Value =
        serde_json::from_slice(&fs::read(directory.join(".mcdev.json")).unwrap()).unwrap();
    assert_eq!(config["world_source_path"], json!(f.project));
    assert_eq!(config["included_mod_dirs"][0], json!(f.project.join("./")));
    assert!(directory.is_dir());
}

#[test]
fn rejects_invalid_config_unreviewed_game_reset_duplicates_and_other_enabled_plugins() {
    let f = Fixture::new();
    for (key, value) in [
        ("plugins", json!({})),
        ("included_mod_dirs", json!([3])),
        ("world_source_path", json!(3)),
        ("reset_world", json!(true)),
        ("game_executable_path", json!("missing.exe")),
        (
            "plugins",
            json!([{"id": "other", "path": "other", "enable": true}]),
        ),
        ("plugins", json!([{"id": PLUGIN_ID}, {"id": PLUGIN_ID}])),
    ] {
        let mut config = json!({"game_executable_path": f.game});
        config[key] = value;
        write_json(&f.project.join(".mcdev.json"), &config).unwrap();
        assert!(f.prepare().is_err(), "{key}");
        assert_eq!(f.config(), config);
    }
    fs::write(f.project.join(".mcdev.json"), b"{invalid}").unwrap();
    assert!(f.prepare().is_err());
    fs::remove_file(f.project.join(".mcdev.json")).unwrap();
    assert!(f.prepare().unwrap_err().to_string().contains("普通启动"));
}

#[test]
fn old_sessions_deserialize_and_editor_sessions_round_trip() {
    use crate::mcdk_session::{McdkSession, SessionMode};
    let old = json!({"component_id": "id", "component_path": "D:/original", "executable": "D:/mcdk.exe",
        "version": "1.6.1", "pid": 10, "created_at": "100"});
    let mut session: McdkSession = serde_json::from_value(old).unwrap();
    assert_eq!(session.mode, SessionMode::Game);
    assert_eq!(session.launch_directory, None);
    session.mode = SessionMode::Editor;
    session.launch_directory = Some("D:/owned-profile".into());
    assert_eq!(
        serde_json::from_value::<McdkSession>(serde_json::to_value(&session).unwrap()).unwrap(),
        session
    );
}

#[test]
#[cfg(windows)]
fn editor_sessions_protect_original_project_across_mcdh_instances() {
    use crate::mcdk_session::{
        McdkSession, SESSION_KEY, SessionMode, SessionState, assert_component_idle, inspect_process,
    };
    let f = Fixture::new();
    let process = inspect_process(std::process::id()).unwrap().unwrap();
    let launch = f.prepare().unwrap();
    f.store
        .store
        .write(
            SESSION_KEY,
            &SessionState {
                session: Some(McdkSession {
                    component_id: "owned-editor".into(),
                    component_path: f.project.clone(),
                    executable: process.executable,
                    version: "plugin-da038f8".into(),
                    pid: std::process::id(),
                    created_at: process.created_at,
                    mode: SessionMode::Editor,
                    launch_directory: Some(launch.directory().to_path_buf()),
                }),
                last_exit: None,
            },
        )
        .unwrap();
    let other = LocalIndex::open(
        f.store
            .store
            .root()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("index.db"),
    )
    .unwrap();
    assert!(assert_component_idle(&other, &f.project).is_err());
    assert!(assert_component_idle(&other, launch.directory()).is_ok());
    assert!(
        crate::mcdk_session::refresh_session(&McdkStore::new(other))
            .unwrap()
            .session
            .is_some()
    );
}
