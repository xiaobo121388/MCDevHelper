use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use mcdh_core::{
    AppSettings, BumpManifestVersionRequest, ComponentService, ComponentSummary,
    CopyComponentRequest, CreateComponentRequest, DiscoveryResult, DiscoveryService, ErrorPayload,
    ExportComponentRequest, ExportSourceInfo, ImportComponentRequest, LocalIndex, MoveComponentRequest,
    OperationResult, QuickExportPhase, QuickExportRequest, SetComponentMetadataRequest,
    SetComponentTagsRequest, SourceKind, SourceRecord, VsCodeStatus,
};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use mcdh_core::custom_export::CustomExportService;

mod mcdk_manager;
mod mcdk_release;
mod mcdk_launch;
mod app_update;
mod custom_export;
mod windows;
use custom_export::*;
use app_update::{install_app_update, app_update_error};
use mcdk_launch::launch_component_game;
use mcdk_manager::{mcdk_status, set_mcdk_auto_update, check_mcdk_update, install_mcdk_update};

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/xiaobo121388/MCDevHelper/releases/latest";
const GITHUB_API_VERSION: &str = "2026-03-10";

type CommandResult<T> = std::result::Result<T, ErrorPayload>;

struct AppState {
    index: LocalIndex,
    exports: CustomExportService,
    quick_exports: Arc<AtomicUsize>,
}

struct QuickExportActivity(Arc<AtomicUsize>);

impl QuickExportActivity {
    fn begin(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::AcqRel);
        Self(count.clone())
    }
}

impl Drop for QuickExportActivity {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl AppState {
    fn open() -> mcdh_core::Result<Self> {
        let index = LocalIndex::open_default()?;
        Ok(Self {
            exports: CustomExportService::new(index.clone())?,
            index,
            quick_exports: Arc::new(AtomicUsize::new(0)),
        })
    }

    fn service(&self) -> ComponentService {
        ComponentService::new(self.index.clone())
    }
}

#[tauri::command]
fn app_version() -> &'static str {
    mcdh_core::VERSION
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    published_at: Option<String>,
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<app_update::Asset>,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateCheckResult {
    current_version: String,
    latest_version: Option<String>,
    release_name: Option<String>,
    release_url: Option<String>,
    published_at: Option<String>,
    release_notes: Option<String>,
    update_available: bool,
    no_release: bool,
}

impl UpdateCheckResult {
    fn no_release() -> Self {
        Self {
            current_version: mcdh_core::VERSION.into(),
            latest_version: None,
            release_name: None,
            release_url: None,
            published_at: None,
            release_notes: None,
            update_available: false,
            no_release: true,
        }
    }

    fn from_release(release: GitHubRelease) -> Self {
        Self {
            current_version: mcdh_core::VERSION.into(),
            update_available: !release.draft && !release.prerelease
                && release_is_newer(mcdh_core::VERSION, &release.tag_name),
            latest_version: Some(release.tag_name),
            release_name: release.name,
            release_url: Some(release.html_url),
            published_at: release.published_at,
            release_notes: release.body.filter(|body| !body.trim().is_empty()),
            no_release: false,
        }
    }
}

#[tauri::command]
async fn check_for_updates() -> CommandResult<UpdateCheckResult> {
    Ok(app_update::latest_release().await.map_err(update_error)?
        .map(UpdateCheckResult::from_release).unwrap_or_else(UpdateCheckResult::no_release))
}

fn release_is_newer(current: &str, candidate: &str) -> bool {
    let current = current.trim().trim_start_matches(['v', 'V']);
    let candidate = candidate.trim().trim_start_matches(['v', 'V']);
    match (
        semver::Version::parse(current),
        semver::Version::parse(candidate),
    ) {
        (Ok(current), Ok(candidate)) => candidate.pre.is_empty() && candidate.build.is_empty() && candidate > current,
        _ => false,
    }
}

fn update_error(message: String) -> ErrorPayload {
    mcdh_core::CoreError::InvalidInput(message).payload()
}

#[tauri::command]
fn mcp_client_config() -> CommandResult<String> {
    let executable = std::env::current_exe()
        .map_err(|error| mcdh_core::CoreError::io("mcdh-mcp.exe", error).payload())?
        .with_file_name("mcdh-mcp.exe");
    serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {
            "mcdh": {
                "command": executable
            }
        }
    }))
    .map_err(|error| mcdh_core::CoreError::json("mcp-client-config", error).payload())
}

#[tauri::command]
async fn refresh_components(state: State<'_, AppState>) -> CommandResult<DiscoveryResult> {
    let index = state.index.clone();
    background(move || DiscoveryService::new(index).refresh()).await
}

#[tauri::command]
async fn get_component(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<ComponentSummary> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).get_component(&component_id)).await
}

#[tauri::command]
fn list_sources(state: State<'_, AppState>) -> CommandResult<Vec<SourceRecord>> {
    core_result(state.index.list_sources())
}

#[tauri::command]
fn add_single_component(state: State<'_, AppState>, path: PathBuf) -> CommandResult<SourceRecord> {
    core_result(state.index.add_source(SourceKind::Single, path))
}

#[tauri::command]
fn add_library(state: State<'_, AppState>, path: PathBuf) -> CommandResult<SourceRecord> {
    core_result(state.index.add_source(SourceKind::Library, path))
}

#[tauri::command]
fn remove_source(state: State<'_, AppState>, source_id: String) -> CommandResult<bool> {
    core_result(state.index.remove_source(&source_id))
}

#[tauri::command]
fn add_mcs_path(state: State<'_, AppState>, path: PathBuf) -> CommandResult<Vec<SourceRecord>> {
    core_result(DiscoveryService::new(state.index.clone()).add_mcs_source_path(&path))
}

#[tauri::command]
async fn rescan_mcs_paths(state: State<'_, AppState>) -> CommandResult<Vec<SourceRecord>> {
    let index = state.index.clone();
    background(move || DiscoveryService::new(index).rescan_mcs_sources()).await
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> CommandResult<AppSettings> {
    core_result(state.index.app_settings())
}

#[tauri::command]
fn set_settings(state: State<'_, AppState>, settings: AppSettings) -> CommandResult<AppSettings> {
    core_result(state.index.set_app_settings(&settings))
}

#[tauri::command]
async fn create_component(
    state: State<'_, AppState>,
    request: CreateComponentRequest,
) -> CommandResult<OperationResult> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).create_component(&request)).await
}

#[tauri::command]
async fn import_component(
    state: State<'_, AppState>,
    request: ImportComponentRequest,
) -> CommandResult<OperationResult> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).import_component(&request)).await
}

#[tauri::command]
fn copy_component(
    state: State<'_, AppState>,
    request: CopyComponentRequest,
) -> CommandResult<OperationResult> {
    core_result(state.service().copy_component(&request))
}

#[tauri::command]
fn move_component(
    state: State<'_, AppState>,
    request: MoveComponentRequest,
) -> CommandResult<OperationResult> {
    core_result(state.service().move_component(&request))
}

#[tauri::command]
async fn export_component(
    state: State<'_, AppState>,
    request: ExportComponentRequest,
) -> CommandResult<OperationResult> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).export_component(&request)).await
}

#[tauri::command]
async fn get_component_export_source(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<ExportSourceInfo> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).export_source_info(&component_id)).await
}

#[tauri::command]
async fn set_component_export_source(
    state: State<'_, AppState>,
    component_id: String,
    source: PathBuf,
) -> CommandResult<PathBuf> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).set_export_source(&component_id, &source)).await
}

#[tauri::command]
async fn quick_export_component(
    state: State<'_, AppState>,
    request: QuickExportRequest,
    on_progress: tauri::ipc::Channel<QuickExportPhase>,
) -> CommandResult<OperationResult> {
    if app_update::is_updating() { return Err(mcdh_core::CoreError::Busy.payload()); }
    let index = state.index.clone();
    let activity = QuickExportActivity::begin(&state.quick_exports);
    background(move || {
        let _activity = activity;
        ComponentService::new(index).quick_export_component(&request, |phase| {
            let _ = on_progress.send(phase);
        })
    })
    .await
}

#[tauri::command]
fn set_quick_export_destination(
    state: State<'_, AppState>,
    destination: PathBuf,
) -> CommandResult<AppSettings> {
    let mut settings = core_result(state.index.app_settings())?;
    settings.quick_export.destination = Some(destination);
    core_result(state.index.set_app_settings(&settings))
}

#[tauri::command]
fn delete_component(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<OperationResult> {
    core_result(state.service().delete_component(&component_id))
}

#[tauri::command]
fn set_component_tags(
    state: State<'_, AppState>,
    request: SetComponentTagsRequest,
) -> CommandResult<OperationResult> {
    core_result(state.service().set_component_tags(&request))
}

#[tauri::command]
fn set_component_metadata(
    state: State<'_, AppState>,
    request: SetComponentMetadataRequest,
) -> CommandResult<OperationResult> {
    core_result(state.service().set_component_metadata(&request))
}

#[tauri::command]
async fn regenerate_manifest_uuids(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<OperationResult> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).regenerate_manifest_uuids(&component_id)).await
}

#[tauri::command]
async fn bump_manifest_version(
    state: State<'_, AppState>,
    request: BumpManifestVersionRequest,
) -> CommandResult<OperationResult> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).bump_manifest_version(&request)).await
}

#[tauri::command]
async fn open_component_directory(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<()> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).open_component_directory(&component_id)).await
}

#[tauri::command]
fn open_warning_directory(path: PathBuf) -> CommandResult<()> {
    if !path.is_absolute() {
        return Err(
            mcdh_core::CoreError::InvalidInput("扫描问题路径不是绝对路径".into()).payload(),
        );
    }
    let directory = nearest_existing_directory(&path)
        .ok_or_else(|| mcdh_core::CoreError::NotFound(path.clone()).payload())?;
    Command::new("explorer.exe")
        .arg(&directory)
        .spawn()
        .map_err(|error| mcdh_core::CoreError::io(&directory, error).payload())?;
    Ok(())
}

fn nearest_existing_directory(path: &Path) -> Option<PathBuf> {
    let mut candidate = if path.is_dir() {
        Some(path)
    } else {
        path.parent()
    };
    while let Some(current) = candidate {
        if current.is_dir() {
            return Some(current.to_path_buf());
        }
        candidate = current.parent();
    }
    None
}

#[tauri::command]
fn vscode_status(state: State<'_, AppState>) -> CommandResult<VsCodeStatus> {
    core_result(state.service().vscode_status())
}

#[tauri::command]
fn set_vscode_path(state: State<'_, AppState>, path: Option<PathBuf>) -> CommandResult<()> {
    core_result(state.service().set_vscode_path(path.as_deref()))
}

#[tauri::command]
async fn open_component_in_vscode(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<()> {
    let index = state.index.clone();
    background(move || ComponentService::new(index).open_component_in_vscode(&component_id)).await
}

fn core_result<T>(result: mcdh_core::Result<T>) -> CommandResult<T> {
    result.map_err(|error| error.payload())
}

async fn background<T, F>(operation: F) -> CommandResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> mcdh_core::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| {
            mcdh_core::CoreError::InvalidInput(format!("后台任务失败：{error}")).payload()
        })?
        .map_err(|error| error.payload())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::open().expect("failed to open MCDH local index");
    tauri::Builder::default()
        .manage(state)
        .setup(mcdk_manager::setup)
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let exporting = window.label() == "main"
                    && (window.state::<AppState>().quick_exports.load(Ordering::Acquire) > 0
                        || window.state::<AppState>().exports.has_active_quick_export());
                if app_update::is_updating() || exporting { api.prevent_close(); }
            }
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            windows::open_dialog_window,
            app_version,
            mcdk_status,
            launch_component_game,
            set_mcdk_auto_update,
            check_mcdk_update,
            install_mcdk_update,
            check_for_updates,
            install_app_update,
            app_update_error,
            mcp_client_config,
            refresh_components,
            get_component,
            list_sources,
            add_single_component,
            add_library,
            add_mcs_path,
            remove_source,
            rescan_mcs_paths,
            get_settings,
            set_settings,
            create_component,
            import_component,
            copy_component,
            move_component,
            export_component,
            get_component_export_source,
            set_component_export_source,
            quick_export_component,
            set_quick_export_destination,
            list_custom_export_profiles,
            save_custom_export_profiles,
            start_custom_export,
            start_quick_custom_export,
            get_custom_export_task,
            list_custom_export_tasks,
            cancel_custom_export,
            resolve_custom_export_conflict,
            delete_component,
            set_component_tags,
            set_component_metadata,
            regenerate_manifest_uuids,
            bump_manifest_version,
            open_component_directory,
            open_warning_directory,
            open_component_in_vscode,
            vscode_status,
            set_vscode_path,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build MCDH")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<AppState>().exports.shutdown();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::{GitHubRelease, UpdateCheckResult, nearest_existing_directory, release_is_newer};

    #[test]
    fn quick_export_activity_tracks_all_workers_until_completion() {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let first = super::QuickExportActivity::begin(&count);
        let second = super::QuickExportActivity::begin(&count);
        assert_eq!(count.load(std::sync::atomic::Ordering::Acquire), 2);
        drop(first);
        assert_eq!(count.load(std::sync::atomic::Ordering::Acquire), 1);
        drop(second);
        assert_eq!(count.load(std::sync::atomic::Ordering::Acquire), 0);
    }

    #[test]
    fn warning_paths_fall_back_to_the_nearest_existing_parent() {
        let crate_directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let missing = crate_directory.join("missing/warning/item");
        assert_eq!(
            nearest_existing_directory(&missing).as_deref(),
            Some(crate_directory)
        );
    }

    #[test]
    fn release_comparison_accepts_v_prefix_and_ignores_older_versions() {
        assert!(release_is_newer("0.1.0", "v0.2.0"));
        assert!(!release_is_newer("0.1.0", "v0.1.0"));
        assert!(!release_is_newer("0.1.0", "v0.0.9"));
        assert!(!release_is_newer("0.1.0", "invalid"));
        assert!(!release_is_newer("0.1.0", "v9.0.0-beta.1"));
    }

    #[test]
    fn release_payload_preserves_the_official_download_page() {
        let mut next = semver::Version::parse(mcdh_core::VERSION).unwrap();
        next.patch += 1;
        let tag = format!("v{next}");
        let url = format!("https://github.com/xiaobo121388/MCDevHelper/releases/tag/{tag}");
        let result = UpdateCheckResult::from_release(GitHubRelease {
            tag_name: tag.clone(),
            name: Some(format!("MCDH {next}")),
            html_url: url.clone(),
            published_at: Some("2026-08-10T12:00:00Z".into()),
            body: Some("新增自动更新提示".into()),
            draft: false,
            prerelease: false,
            assets: vec![],
        });
        assert!(result.update_available);
        assert_eq!(result.latest_version.as_deref(), Some(tag.as_str()));
        assert_eq!(result.release_notes.as_deref(), Some("新增自动更新提示"));
        assert_eq!(
            result.release_url.as_deref(),
            Some(url.as_str())
        );
    }
}
