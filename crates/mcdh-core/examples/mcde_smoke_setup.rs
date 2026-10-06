//! Explicit native-test setup only. Copies the owned demo and seeds an isolated MCDH index.

use mcdh_core::{DiscoveryService, LocalIndex, SourceKind, mcdk::McdkStore};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: mcde_smoke_setup <MCDevEditor root> <3.10.0.420447 game exe>".into());
    }
    let editor = fs::canonicalize(&args[0])?;
    let game = PathBuf::from(&args[1]);
    if !game.is_file()
        || game.parent().and_then(Path::file_name) != Some(std::ffi::OsStr::new("3.10.0.420447"))
    {
        return Err("Native smoke tests require the reviewed 3.10.0.420447 installation".into());
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let root = workspace
        .join("target/native-mcde-qa")
        .join(uuid::Uuid::new_v4().to_string());
    let project = root.join("project");
    let data = root.join("data");
    fs::create_dir_all(&project)?;
    for pack in ["RP", "BP"] {
        let source = editor.join("examples/demo").join(pack);
        for entry in walkdir::WalkDir::new(&source).follow_links(false) {
            let entry = entry?;
            if entry.file_type().is_symlink() {
                return Err("Smoke source must not contain links".into());
            }
            let target = project.join(pack).join(entry.path().strip_prefix(&source)?);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&target)?;
            } else if entry.file_type().is_file() {
                fs::copy(entry.path(), &target)?;
            }
        }
    }
    let config = json!({
        "game_executable_path": game, "auto_join_game": true, "include_debug_mod": true,
        "auto_hot_reload_mods": true, "auto_hot_reload_ui": false, "enable_cheats": true,
        "game_mode": 1, "reset_world": false, "world_name": "MCDH MCDE Integration QA",
        "world_folder_name": "MCDH_MCDE_QA_310", "included_mod_dirs": ["RP", "BP"],
        "mcp_server_config": {"enabled": true, "server_ip": "localhost", "server_port": 19147}
    });
    fs::write(
        project.join(".mcdev.json"),
        serde_json::to_vec_pretty(&config)?,
    )?;
    let index = LocalIndex::open(data.join("mcdh.db"))?;
    index.set_setting("mcs_initial_scan_done", "true")?;
    index.add_source(SourceKind::Single, &project)?;
    McdkStore::new(index.clone()).set_auto_update(false)?;
    let discovery = DiscoveryService::new(index).refresh()?;
    if discovery.components.len() != 1 {
        return Err("Smoke project was not uniquely discovered".into());
    }
    for name in ["appdata", "localappdata"] {
        fs::create_dir_all(root.join(name))?;
    }
    let result = json!({
        "case": "mcdh_mcde_native_setup", "ok": true, "stage": "fixture_only_game_not_started",
        "root": root, "data": data, "project": project, "component_id": discovery.components[0].id,
        "workspace": workspace, "appdata": root.join("appdata"), "localappdata": root.join("localappdata"),
        "mcp_port": 19147
    });
    fs::write(root.join("setup.json"), serde_json::to_vec_pretty(&result)?)?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
