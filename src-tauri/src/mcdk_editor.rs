use crate::{AppState, CommandResult, mcdk_manager::blocking};
use mcdh_core::{
    mcdk_config::{self, ConfigEdit, ConfigSnapshot},
    skins::{self, Catalog, CustomSkin},
};
use tauri::State;

#[tauri::command]
pub async fn get_mcdk_game_installations(
    state: State<'_, AppState>,
    component_id: String,
    game_executable_path: String,
) -> CommandResult<mcdh_core::game_installations::GameCatalog> {
    let index = state.index.clone();
    blocking(move || {
        mcdh_core::game_installations::discover(&index, &component_id, &game_executable_path)
    })
    .await
    .map_err(|e| e.payload())
}

#[tauri::command]
pub async fn get_mcdk_config(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<ConfigSnapshot> {
    let index = state.index.clone();
    blocking(move || mcdk_config::load(&index, &component_id))
        .await
        .map_err(|e| e.payload())
}

#[tauri::command]
pub async fn save_mcdk_config(
    state: State<'_, AppState>,
    component_id: String,
    revision: Option<String>,
    edits: Vec<ConfigEdit>,
) -> CommandResult<ConfigSnapshot> {
    let index = state.index.clone();
    blocking(move || mcdk_config::save(&index, &component_id, revision.as_deref(), &edits))
        .await
        .map_err(|e| e.payload())
}

#[tauri::command]
pub async fn get_mcdk_skin_catalog(game_executable_path: String) -> CommandResult<Catalog> {
    blocking(move || skins::catalog(std::path::Path::new(&game_executable_path)))
        .await
        .map_err(|e| e.payload())
}

#[tauri::command]
pub async fn get_mcdk_skin_preview(
    state: State<'_, AppState>,
    component_id: String,
    path: String,
) -> CommandResult<String> {
    let index = state.index.clone();
    blocking(move || {
        let root = mcdh_core::mcdk::McdkStore::new(index)
            .launch_target(&component_id)?
            .path;
        skins::preview(&root.join(path))
    })
    .await
    .map_err(|e| e.payload())
}

#[tauri::command]
pub async fn get_mcdk_skin_library(
    state: State<'_, AppState>,
    component_id: String,
) -> CommandResult<Vec<CustomSkin>> {
    let index = state.index.clone();
    blocking(move || skins::library(&index, &component_id))
        .await
        .map_err(|e| e.payload())
}

#[tauri::command]
pub async fn update_mcdk_skin_library(
    state: State<'_, AppState>,
    component_id: String,
    path: String,
    slim: bool,
    remove: bool,
) -> CommandResult<Vec<CustomSkin>> {
    let index = state.index.clone();
    blocking(move || skins::update_library(&index, &component_id, &path, slim, remove))
        .await
        .map_err(|e| e.payload())
}
