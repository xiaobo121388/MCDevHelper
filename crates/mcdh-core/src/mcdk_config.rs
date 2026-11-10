//! Configuration contract adapted from mcdev-tools (MIT); see docs/mcdk-editor.md.
use crate::json::{parse_jsonc, parse_options};
use crate::{CoreError, LocalIndex, Result, mcdk::McdkStore};
use jsonc_parser::cst::{CstInputValue, CstRootNode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_CONFIG_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct ConfigSnapshot {
    pub path: PathBuf,
    pub revision: Option<String>,
    pub data: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigEdit {
    pub path: Vec<String>,
    pub value: Value,
}

pub fn load(index: &LocalIndex, component_id: &str) -> Result<ConfigSnapshot> {
    snapshot(&config_path(index, component_id)?)
}

pub fn save(
    index: &LocalIndex,
    component_id: &str,
    revision: Option<&str>,
    edits: &[ConfigEdit],
) -> Result<ConfigSnapshot> {
    let _guard = index.try_lock_mutations()?;
    let path = config_path(index, component_id)?;
    crate::mcdk_session::assert_component_idle(index, path.parent().unwrap())?;
    let before = snapshot(&path)?;
    if before.revision.as_deref() != revision {
        return Err(invalid("配置已被其他程序修改，请重新加载后再保存。"));
    }
    if edits.len() > 128 {
        return Err(invalid("配置修改数量超过限制"));
    }
    for edit in edits {
        validate_edit(edit)?;
    }
    let text = read_config(&path)?.unwrap_or_else(|| "{\n}\n".into());
    if hash_text(&text, before.revision.is_some()) != before.revision {
        return Err(invalid("配置已改变，请重新加载。"));
    }
    let root = CstRootNode::parse(text.trim_start_matches('\u{feff}'), &parse_options())
        .map_err(|error| CoreError::json(&path, error))?;
    for edit in edits {
        let mut object = root
            .object_value()
            .ok_or_else(|| invalid("配置根节点必须是对象"))?;
        for part in &edit.path[..edit.path.len() - 1] {
            if object.get(part).is_none() {
                object.append(part, CstInputValue::Object(vec![]));
            }
            object = object
                .object_value(part)
                .ok_or_else(|| invalid(&format!("{part} 不是对象，无法安全修改")))?;
        }
        let name = edit.path.last().unwrap();
        let value = cst_value(&edit.value);
        if let Some(property) = object.get(name) {
            property.set_value(value);
        } else {
            object.append(name, value);
        }
    }
    let mut output = root.to_string();
    if text.starts_with('\u{feff}') {
        output.insert(0, '\u{feff}');
    }
    if output.len() > MAX_CONFIG_BYTES {
        return Err(invalid("配置文件超过 1 MiB"));
    }
    let updated: Value = parse_jsonc(&output, &path)?;
    for edit in edits {
        let actual = edit
            .path
            .iter()
            .try_fold(&updated, |value, key| value.get(key));
        if actual != Some(&edit.value) {
            return Err(invalid("配置含有重复或冲突的字段，无法安全保存。"));
        }
    }
    if read_config(&path)? != before.revision.as_ref().map(|_| text.clone()) {
        return Err(invalid("保存前检测到外部修改，请重新加载。"));
    }
    if edits.is_empty() {
        return Ok(before);
    }
    crate::operations::atomic_write(&path, output.as_bytes())?;
    snapshot(&path)
}

fn config_path(index: &LocalIndex, component_id: &str) -> Result<PathBuf> {
    let component = McdkStore::new(index.clone()).launch_target(component_id)?;
    let path = component.path.join(".mcdev.json");
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(invalid(".mcdev.json 必须是普通文件，不能是链接或目录"));
        }
        let real = crate::canonicalize_path(&path)?;
        let parent = crate::canonicalize_path(&component.path)?;
        if real.parent() != Some(parent.as_path()) {
            return Err(invalid("配置路径超出组件目录"));
        }
    }
    Ok(path)
}

fn snapshot(path: &Path) -> Result<ConfigSnapshot> {
    let text = read_config(path)?;
    let data: Value = match &text {
        Some(text) => parse_jsonc(text, path)?,
        None => serde_json::json!({}),
    };
    if !data.is_object() {
        return Err(invalid(".mcdev.json 根节点必须是对象"));
    }
    Ok(ConfigSnapshot {
        path: path.into(),
        revision: text.as_ref().and_then(|text| hash_text(text, true)),
        data,
    })
}

fn read_config(path: &Path) -> Result<Option<String>> {
    match bounded_read(path, MAX_CONFIG_BYTES) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| invalid("配置必须使用 UTF-8 编码")),
        Err(CoreError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let file = fs::File::open(path).map_err(|error| CoreError::io(path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| CoreError::io(path, error))?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(invalid("文件类型或大小不符合限制"));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CoreError::io(path, error))?;
    if bytes.len() > limit {
        return Err(invalid("文件大小超过限制"));
    }
    Ok(bytes)
}

fn hash_text(text: &str, exists: bool) -> Option<String> {
    exists.then(|| format!("{:x}", Sha256::digest(text.as_bytes())))
}

fn cst_value(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(v) => CstInputValue::Bool(*v),
        Value::Number(v) => CstInputValue::Number(v.to_string()),
        Value::String(v) => CstInputValue::String(v.clone()),
        Value::Array(v) => CstInputValue::Array(v.iter().map(cst_value).collect()),
        Value::Object(v) => {
            CstInputValue::Object(v.iter().map(|(k, v)| (k.clone(), cst_value(v))).collect())
        }
    }
}

pub(crate) fn invalid(message: &str) -> CoreError {
    CoreError::InvalidInput(message.into())
}

fn validate_edit(edit: &ConfigEdit) -> Result<()> {
    let key = edit.path.join(".");
    let value = &edit.value;
    let integer = |min: i64, max: i64| value.as_i64().is_some_and(|v| (min..=max).contains(&v));
    let text = || {
        value
            .as_str()
            .is_some_and(|v| v.len() <= 32768 && !v.contains('\0'))
    };
    let valid = match key.as_str() {
        "game_executable_path"
        | "world_name"
        | "user_name"
        | "skin_info.skin"
        | "mcp_server_config.server_ip" => text(),
        "world_folder_name" => {
            text()
                && value.as_str().is_some_and(|v| {
                    !v.is_empty() && v != "." && v != ".." && !v.contains(['/', '\\', ':'])
                })
        }
        "world_seed" => value.is_null() || integer(-9_007_199_254_740_991, 9_007_199_254_740_991),
        "world_type" => integer(0, 2),
        "game_mode" => integer(0, 3),
        "log_protocol" => integer(0, 1),
        "reset_world"
        | "auto_join_game"
        | "include_debug_mod"
        | "enable_cheats"
        | "keep_inventory"
        | "auto_hot_reload_mods"
        | "auto_hot_reload_ui"
        | "auto_hot_reload_shaders"
        | "auto_hot_reload_materials"
        | "auto_hot_reload_particles"
        | "do_daylight_cycle"
        | "do_weather_cycle"
        | "skin_info.slim"
        | "window_style.always_on_top"
        | "window_style.hide_title_bar"
        | "window_style.hide_taskbar_icon"
        | "debug_options.reload_key_global"
        | "mcp_server_config.enabled"
        | "experiment_options.data_driven_biomes"
        | "experiment_options.upcoming_creator_features"
        | "experiment_options.experimental_creator_cameras"
        | "experiment_options.gametest"
        | "experiment_options.deferred_technical_preview" => value.is_boolean(),
        "debug_options.reload_key"
        | "debug_options.reload_world_key"
        | "debug_options.reload_addon_key"
        | "debug_options.reload_shaders_key" => value
            .as_str()
            .is_some_and(|v| v.is_empty() || v.parse::<u16>().is_ok_and(|n| n <= 255)),
        "mcp_server_config.server_port" => integer(1, 65535),
        "window_style.opacity" => value.is_null() || integer(0, 255),
        "window_style.lock_corner" => value.is_null() || integer(1, 4),
        "window_style.title_bar_color" => value.is_null() || vector(value, 3, 0, 255),
        "window_style.fixed_size" => value.is_null() || vector(value, 2, 1, 32768),
        "window_style.fixed_position" => value.is_null() || vector(value, 2, -32768, 32768),
        "included_mod_dirs" => value.as_array().is_some_and(|items| {
            items.len() <= 128
                && items.iter().all(|v| {
                    let path = if v.is_string() {
                        v.as_str()
                    } else {
                        v.get("path").and_then(Value::as_str)
                    };
                    path.is_some_and(|s| {
                        !s.trim().is_empty() && s.len() <= 32768 && !s.contains('\0')
                    }) && ["enabled", "hot_reload"]
                        .iter()
                        .all(|k| v.get(k).is_none_or(Value::is_boolean))
                })
        }),
        _ => false,
    };
    if !valid || edit.path.is_empty() || edit.path.iter().any(|p| p.contains('.')) {
        return Err(invalid(&format!("不支持的配置字段或无效值：{key}")));
    }
    Ok(())
}

fn vector(value: &Value, count: usize, min: i64, max: i64) -> bool {
    value.as_array().is_some_and(|v| {
        v.len() == count
            && v.iter()
                .all(|n| n.as_i64().is_some_and(|n| (min..=max).contains(&n)))
    })
}

#[cfg(test)]
mod tests;
