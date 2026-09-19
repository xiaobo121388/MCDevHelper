use crate::{AppState, CommandResult, background, core_result};
use mcdh_core::ExportConflictPolicy;
use mcdh_core::custom_export::{
    Caller, CustomExportProfile, CustomExportTask, ProfileStore, StartCustomExportRequest,
};
use tauri::State;

#[tauri::command]
pub(crate) fn list_custom_export_profiles(
    state: State<'_, AppState>,
) -> CommandResult<Vec<CustomExportProfile>> {
    core_result(state.exports.profiles(Caller::Desktop))
}

#[tauri::command]
pub(crate) fn save_custom_export_profiles(
    state: State<'_, AppState>,
    profiles: Vec<CustomExportProfile>,
) -> CommandResult<Vec<CustomExportProfile>> {
    core_result(ProfileStore::new(state.index.clone()).save(profiles))
}

#[tauri::command]
pub(crate) async fn start_custom_export(
    state: State<'_, AppState>,
    request: StartCustomExportRequest,
) -> CommandResult<CustomExportTask> {
    let service = state.exports.clone();
    background(move || service.start(Caller::Desktop, request)).await
}

#[tauri::command]
pub(crate) fn get_custom_export_task(
    state: State<'_, AppState>,
    task_id: String,
    cursor: Option<u64>,
) -> CommandResult<CustomExportTask> {
    core_result(
        state
            .exports
            .get(Caller::Desktop, &task_id, cursor.unwrap_or(0)),
    )
}

#[tauri::command]
pub(crate) fn list_custom_export_tasks(state: State<'_, AppState>) -> Vec<CustomExportTask> {
    state.exports.list_tasks(Caller::Desktop)
}

#[tauri::command]
pub(crate) fn cancel_custom_export(
    state: State<'_, AppState>,
    task_id: String,
) -> CommandResult<CustomExportTask> {
    core_result(state.exports.cancel(Caller::Desktop, &task_id))
}

#[tauri::command]
pub(crate) fn resolve_custom_export_conflict(
    state: State<'_, AppState>,
    task_id: String,
    conflict_policy: ExportConflictPolicy,
) -> CommandResult<CustomExportTask> {
    core_result(
        state
            .exports
            .resolve_conflict(Caller::Desktop, &task_id, conflict_policy),
    )
}
