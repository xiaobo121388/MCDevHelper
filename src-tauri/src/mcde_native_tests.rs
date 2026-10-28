//! Opt-in real-game test for the production backend. Does not simulate a desktop click.

use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

fn children(parent: u32) -> Vec<(u32, String)> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::*,
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        assert_ne!(snapshot, INVALID_HANDLE_VALUE);
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Vec::new();
        let mut next = Process32FirstW(snapshot, &mut entry);
        while next != 0 {
            if entry.th32ParentProcessID == parent {
                let length = entry
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(entry.szExeFile.len());
                found.push((
                    entry.th32ProcessID,
                    String::from_utf16_lossy(&entry.szExeFile[..length]),
                ));
            }
            next = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        found
    }
}

fn controller(path: &Path, operation: &str, pid: u32) -> Option<Value> {
    let result = Command::new(path)
        .arg(operation)
        .arg(pid.to_string())
        .output()
        .ok()?;
    result
        .status
        .success()
        .then(|| serde_json::from_slice(&result.stdout).ok())
        .flatten()
}

#[test]
#[ignore = "starts an owned real 3.10 game; requires MCDH_MCDE_QA_SETUP and the isolated QA environment"]
fn mcdk_test_mcde_native_backend() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let setup_path =
        PathBuf::from(std::env::var_os("MCDH_MCDE_QA_SETUP").expect("Explicit QA setup required"));
    let setup_path = fs::canonicalize(setup_path).unwrap();
    assert!(
        setup_path.starts_with(fs::canonicalize(workspace.join("target/native-mcde-qa")).unwrap())
    );
    let setup: Value = serde_json::from_slice(&fs::read(&setup_path).unwrap()).unwrap();
    assert_eq!(setup["case"], "mcdh_mcde_native_setup");
    let root = PathBuf::from(setup["root"].as_str().unwrap());
    let project = PathBuf::from(setup["project"].as_str().unwrap());
    let data = PathBuf::from(setup["data"].as_str().unwrap());
    for (name, expected) in [
        ("APPDATA", setup["appdata"].as_str().unwrap()),
        (
            "MCDE_QA_WORKSPACE_ROOT",
            setup["workspace"].as_str().unwrap(),
        ),
        (
            "MCDE_QA_ROAMING_APPDATA",
            setup["appdata"].as_str().unwrap(),
        ),
        (
            "MCDE_QA_LOCAL_APPDATA",
            setup["localappdata"].as_str().unwrap(),
        ),
    ] {
        assert_eq!(std::env::var(name).unwrap(), expected, "{name}");
    }
    let index = mcdh_core::LocalIndex::open(data.join("mcdh.db")).unwrap();
    let binary = Path::new(env!("CARGO_MANIFEST_DIR")).join("release-resources/mcdk/mcdk.exe");
    let manager = McdkManager::new(index.clone(), binary);
    mcdh_plugins::PluginService::new(index.clone())
        .unwrap()
        .install_builtin(mcdh_plugins::MCDE_PLUGIN_ID)
        .unwrap();
    assert!(manager.status().unwrap().mcde.unwrap().available);
    let source = fs::read(project.join(".mcdev.json")).unwrap();
    let source_digest = format!("{:x}", Sha256::digest(&source));
    let id = setup["component_id"].as_str().unwrap();
    let started = start_session(&manager, id, SessionMode::Editor).unwrap();
    assert_eq!(started.session.mode, SessionMode::Editor);
    assert_eq!(
        fs::canonicalize(&started.session.component_path).unwrap(),
        fs::canonicalize(&project).unwrap()
    );
    assert_eq!(
        fs::canonicalize(&started.session.executable).unwrap(),
        fs::canonicalize(manager.mcde_host.join("mcdk.exe")).unwrap()
    );
    assert!(start_session(&manager, id, SessionMode::Editor).is_err());
    assert!(start_session(&manager, id, SessionMode::Game).is_err());
    assert!(mcdh_core::mcdk_session::assert_component_idle(&index, &project).is_err());
    let host_pid = started.session.pid;
    let controller_path = workspace
        .parent()
        .unwrap()
        .join("MCDevEditor/build/completion/bin/Release/workbenchctl.exe");
    assert!(controller_path.is_file());
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut first = None;
    while Instant::now() < deadline {
        for (pid, name) in children(host_pid) {
            if name.eq_ignore_ascii_case("Minecraft.Windows.exe")
                && let Some(status) = controller(&controller_path, "status", pid)
                && status["hookReady"] == true
                && status["viewerReady"] == true
                && status["frames"].as_u64().unwrap_or(0) > 0
            {
                first = Some(status);
                break;
            }
        }
        if first.is_some() {
            break;
        }
        if mcdh_core::mcdk_session::inspect_process(host_pid)
            .unwrap()
            .is_none_or(|p| p.exit_code.is_some())
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let first = first.unwrap_or_else(|| {
        panic!(
            "Native frames unavailable; owned session retained for diagnosis: {:?}",
            started.session
        )
    });
    let game_pid = first["gamePid"].as_u64().unwrap() as u32;
    assert_eq!(first["hostPid"], host_pid);
    std::thread::sleep(Duration::from_secs(1));
    let second = controller(&controller_path, "status", game_pid).unwrap();
    assert!(second["frames"].as_u64().unwrap() > first["frames"].as_u64().unwrap());
    assert!(second["copiedFrames"].as_u64().unwrap() > 0);
    assert_eq!(fs::read(project.join(".mcdev.json")).unwrap(), source);
    let reopened = McdkManager::new(
        mcdh_core::LocalIndex::open(data.join("mcdh.db")).unwrap(),
        manager.bundled_binary.clone(),
    );
    assert_eq!(
        refresh_session(&reopened.store).unwrap().session,
        Some(started.session.clone())
    );
    assert!(controller(&controller_path, "snapshot", game_pid).is_some());
    let report = json!({
        "case": "mcdh_mcde_native_backend", "ok": true, "stage": "shared_frames_and_recovered_session",
        "transport": "production_start_session_from_opt_in_test_not_desktop_click",
        "session": started.session, "game_pid": game_pid, "first": first, "second": second,
        "configuration_sha256_before": source_digest,
        "configuration_sha256_after": format!("{:x}", Sha256::digest(fs::read(project.join(".mcdev.json")).unwrap())),
        "duplicate_modes_blocked": true, "original_component_protected": true,
        "process_handle_release_does_not_close_game": true,
        "cleanup": "pending_normal_editor_close", "snapshot_requested": true
    });
    fs::write(
        root.join("native-backend-report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("[MCDK_TEST] {}", serde_json::to_string(&report).unwrap());
    // Keep the owned session for visual inspection and normal close. Never terminate an active game.
    drop(started.process);
    assert!(
        mcdh_core::mcdk_session::inspect_process(host_pid)
            .unwrap()
            .unwrap()
            .exit_code
            .is_none()
    );
}
