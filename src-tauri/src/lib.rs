use std::path::PathBuf;

use mcdh_core::{
    AppSettings, BumpManifestVersionRequest, ComponentService, ComponentSummary,
    CopyComponentRequest, CreateComponentRequest, DiscoveryResult, DiscoveryService, ErrorPayload,
    ExportComponentRequest, ImportComponentRequest, LocalIndex, MoveComponentRequest,
    OperationResult, SetComponentTagsRequest, SourceKind, SourceRecord, VsCodeStatus,
};
use tauri::State;

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
fn export_component(
    state: State<'_, AppState>,
    request: ExportComponentRequest,
) -> CommandResult<OperationResult> {
    core_result(state.service().export_component(&request))
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
fn open_component_directory(state: State<'_, AppState>, component_id: String) -> CommandResult<()> {
    core_result(state.service().open_component_directory(&component_id))
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
fn open_component_in_vscode(state: State<'_, AppState>, component_id: String) -> CommandResult<()> {
    core_result(state.service().open_component_in_vscode(&component_id))
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
            open_component_in_vscode,
            vscode_status,
            set_vscode_path,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run MCDH");
}
