use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use mcdh_core::{
    AppSettings, BumpManifestVersionRequest, ComponentService, ComponentSummary,
    CopyComponentRequest, CreateComponentRequest, DiscoveryResult, DiscoveryService, ErrorPayload,
    ExportComponentRequest, ImportComponentRequest, LocalIndex, MoveComponentRequest,
    OperationResult, SetComponentTagsRequest, SourceKind, SourceRecord, VsCodeStatus,
};
use serde::{Deserialize, Serialize};
use tauri::State;

const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/xiaobo121388/MCDevHelper/releases/latest";
const GITHUB_API_VERSION: &str = "2026-03-10";

type CommandResult<T> = std::result::Result<T, ErrorPayload>;

struct AppState {
    index: LocalIndex,
}

impl AppState {
    fn open() -> mcdh_core::Result<Self> {
        Ok(Self {
            index: LocalIndex::open_default()?,
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
}

#[derive(Debug, Clone, Serialize)]
struct UpdateCheckResult {
    current_version: String,
    latest_version: Option<String>,
    release_name: Option<String>,
    release_url: Option<String>,
    published_at: Option<String>,
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
            update_available: false,
            no_release: true,
        }
    }

    fn from_release(release: GitHubRelease) -> Self {
        Self {
            current_version: mcdh_core::VERSION.into(),
            update_available: release_is_newer(mcdh_core::VERSION, &release.tag_name),
            latest_version: Some(release.tag_name),
            release_name: release.name,
            release_url: Some(release.html_url),
            published_at: release.published_at,
            no_release: false,
        }
    }
}

#[tauri::command]
async fn check_for_updates() -> CommandResult<UpdateCheckResult> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|error| update_error(format!("无法初始化更新检查：{error}")))?;
    let response = client
        .get(LATEST_RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .header(
            reqwest::header::USER_AGENT,
            format!("MCDH/{}", mcdh_core::VERSION),
        )
        .send()
        .await
        .map_err(|error| update_error(format!("无法连接 GitHub：{error}")))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateCheckResult::no_release());
    }
    if !response.status().is_success() {
        return Err(update_error(format!(
            "GitHub 返回 HTTP {}，请稍后重试",
            response.status().as_u16()
        )));
    }
    let release = response
        .json::<GitHubRelease>()
        .await
        .map_err(|error| update_error(format!("GitHub Release 数据无法读取：{error}")))?;
    Ok(UpdateCheckResult::from_release(release))
}

fn release_is_newer(current: &str, candidate: &str) -> bool {
    let current = current.trim().trim_start_matches(['v', 'V']);
    let candidate = candidate.trim().trim_start_matches(['v', 'V']);
    match (
        semver::Version::parse(current),
        semver::Version::parse(candidate),
    ) {
        (Ok(current), Ok(candidate)) => candidate > current,
        _ => candidate != current,
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
fn get_component(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<ComponentSummary> {
    core_result(state.service().get_component(&component_id))
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
fn create_component(
    state: State<'_, AppState>,
    request: CreateComponentRequest,
) -> CommandResult<OperationResult> {
    core_result(state.service().create_component(&request))
}

#[tauri::command]
fn import_component(
    state: State<'_, AppState>,
    request: ImportComponentRequest,
) -> CommandResult<OperationResult> {
    core_result(state.service().import_component(&request))
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
fn regenerate_manifest_uuids(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<OperationResult> {
    core_result(state.service().regenerate_manifest_uuids(&component_id))
}

#[tauri::command]
fn bump_manifest_version(
    state: State<'_, AppState>,
    request: BumpManifestVersionRequest,
) -> CommandResult<OperationResult> {
    core_result(state.service().bump_manifest_version(&request))
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            app_version,
            check_for_updates,
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
            delete_component,
            set_component_tags,
            regenerate_manifest_uuids,
            bump_manifest_version,
            open_component_directory,
            open_warning_directory,
            open_component_in_vscode,
            vscode_status,
            set_vscode_path,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run MCDH");
}

#[cfg(test)]
mod tests {
    use super::{GitHubRelease, UpdateCheckResult, nearest_existing_directory, release_is_newer};

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
    }

    #[test]
    fn release_payload_preserves_the_official_download_page() {
        let result = UpdateCheckResult::from_release(GitHubRelease {
            tag_name: "v0.2.0".into(),
            name: Some("MCDH 0.2.0".into()),
            html_url: "https://github.com/xiaobo121388/MCDevHelper/releases/tag/v0.2.0".into(),
            published_at: Some("2026-08-10T12:00:00Z".into()),
        });
        assert!(result.update_available);
        assert_eq!(result.latest_version.as_deref(), Some("v0.2.0"));
        assert_eq!(
            result.release_url.as_deref(),
            Some("https://github.com/xiaobo121388/MCDevHelper/releases/tag/v0.2.0")
        );
    }
}
