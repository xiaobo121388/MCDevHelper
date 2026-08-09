use std::path::{Path, PathBuf};
use std::process::Command;

use mcdh_core::{
    BumpManifestVersionRequest, ComponentService, ComponentSummary, CopyComponentRequest,
    CreateComponentRequest, DiscoveryResult, DiscoveryService, ErrorPayload,
    ExportComponentRequest, ImportComponentRequest, LocalIndex, MoveComponentRequest,
    OperationResult, SetComponentTagsRequest, SourceKind, SourceRecord,
};
use serde::Serialize;
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

#[derive(Debug, Serialize)]
struct VsCodeStatus {
    available: bool,
    path: Option<PathBuf>,
    custom: bool,
}

#[tauri::command]
fn app_version() -> &'static str {
    mcdh_core::VERSION
}

#[tauri::command]
fn refresh_components(state: State<'_, AppState>) -> CommandResult<DiscoveryResult> {
    core_result(DiscoveryService::new(state.index.clone()).refresh())
}

#[tauri::command]
fn get_component(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<ComponentSummary> {
    let result = DiscoveryService::new(state.index.clone())
        .refresh()
        .and_then(|result| {
            result
                .components
                .into_iter()
                .find(|component| component.id == component_id)
                .ok_or_else(|| mcdh_core::CoreError::InvalidInput("找不到组件 ID".into()))
        });
    core_result(result)
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
    let component = find_component(&state.index, &component_id)?;
    Command::new("explorer.exe")
        .arg(&component.path)
        .spawn()
        .map_err(|error| mcdh_core::CoreError::io(&component.path, error).payload())?;
    Ok(())
}

#[tauri::command]
fn vscode_status(state: State<'_, AppState>) -> CommandResult<VsCodeStatus> {
    let custom = core_result(state.index.setting("vscode_path"))?
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let detected = custom
        .as_ref()
        .filter(|path| path.is_file())
        .cloned()
        .or_else(detect_vscode);
    Ok(VsCodeStatus {
        available: detected.is_some(),
        path: detected,
        custom: custom.is_some(),
    })
}

#[tauri::command]
fn set_vscode_path(state: State<'_, AppState>, path: Option<PathBuf>) -> CommandResult<()> {
    let value = match path {
        Some(path) => {
            if !path.is_file() {
                return Err(mcdh_core::CoreError::NotFound(path).payload());
            }
            path.to_string_lossy().into_owned()
        }
        None => String::new(),
    };
    core_result(state.index.set_setting("vscode_path", &value))
}

#[tauri::command]
fn open_component_in_vscode(state: State<'_, AppState>, component_id: String) -> CommandResult<()> {
    let component = find_component(&state.index, &component_id)?;
    let configured = core_result(state.index.setting("vscode_path"))?
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let executable = configured
        .filter(|path| path.is_file())
        .or_else(detect_vscode)
        .ok_or_else(|| {
            mcdh_core::CoreError::NotFound(PathBuf::from("Visual Studio Code")).payload()
        })?;
    let target = preferred_workspace(&component.path)
        .map_err(|error| mcdh_core::CoreError::io(&component.path, error).payload())?;
    launch_vscode(&executable, &target)
        .map_err(|error| mcdh_core::CoreError::io(&executable, error).payload())?;
    Ok(())
}

fn find_component(index: &LocalIndex, component_id: &str) -> CommandResult<ComponentSummary> {
    core_result(
        DiscoveryService::new(index.clone())
            .refresh()
            .and_then(|result| {
                result
                    .components
                    .into_iter()
                    .find(|component| component.id == component_id)
                    .ok_or_else(|| mcdh_core::CoreError::InvalidInput("找不到组件 ID".into()))
            }),
    )
}

fn preferred_workspace(root: &Path) -> std::io::Result<PathBuf> {
    let workspaces = std::fs::read_dir(root)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("code-workspace"))
        })
        .collect::<Vec<_>>();
    Ok(if workspaces.len() == 1 {
        workspaces[0].clone()
    } else {
        root.to_path_buf()
    })
}

fn detect_vscode() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join("Programs/Microsoft VS Code/Code.exe"));
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(program_files) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(program_files).join("Microsoft VS Code/Code.exe"));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn launch_vscode(executable: &Path, target: &Path) -> std::io::Result<()> {
    Command::new(executable).arg(target).spawn().map(|_| ())
}

fn core_result<T>(result: mcdh_core::Result<T>) -> CommandResult<T> {
    result.map_err(|error| error.payload())
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
            refresh_components,
            get_component,
            list_sources,
            add_single_component,
            add_library,
            remove_source,
            create_component,
            import_component,
            copy_component,
            move_component,
            export_component,
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
