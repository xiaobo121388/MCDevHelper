use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use jsonc_parser::cst::{CstInputValue, CstRootNode};
use uuid::Uuid;

use crate::json::{parse_jsonc, parse_options};
use crate::{ComponentMetadata, CoreError, Result};

pub(crate) const METADATA_FILE_NAME: &str = ".mcdh.json";

pub(crate) fn metadata_path(root: &Path) -> PathBuf {
    root.join(METADATA_FILE_NAME)
}

pub(crate) fn read_component_metadata(root: &Path) -> Result<Option<ComponentMetadata>> {
    let path = metadata_path(root);
    let file_metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(CoreError::io(&path, error)),
    };
    if file_metadata.file_type().is_symlink() || !file_metadata.is_file() {
        return Err(CoreError::InvalidInput(format!(
            "组件配置必须是普通文件：{}",
            path.display()
        )));
    }
    let metadata: ComponentMetadata = parse_jsonc(
        &fs::read_to_string(&path).map_err(|error| CoreError::io(&path, error))?,
        &path,
    )?;
    validate_metadata(metadata, &path).map(Some)
}

pub(crate) fn normalized_metadata(
    display_name: impl Into<String>,
    tags: &[String],
    favorite: bool,
) -> Result<ComponentMetadata> {
    validate_metadata(
        ComponentMetadata::new(display_name, tags.to_vec(), favorite),
        Path::new(METADATA_FILE_NAME),
    )
}

pub(crate) fn metadata_bytes(metadata: &ComponentMetadata) -> Result<Vec<u8>> {
    let metadata = validate_metadata(metadata.clone(), Path::new(METADATA_FILE_NAME))?;
    let mut bytes = serde_json::to_vec_pretty(&metadata)
        .map_err(|error| CoreError::json(METADATA_FILE_NAME, error))?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(crate) fn write_component_metadata(
    root: &Path,
    metadata: &ComponentMetadata,
) -> Result<PathBuf> {
    let path = metadata_path(root);
    let metadata = validate_metadata(metadata.clone(), &path)?;
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            read_component_metadata(root)?;
            update_existing_metadata(&path, &metadata)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            atomic_write(&path, &metadata_bytes(&metadata)?)?;
        }
        Err(error) => return Err(CoreError::io(&path, error)),
    }
    Ok(path)
}

fn validate_metadata(mut metadata: ComponentMetadata, path: &Path) -> Result<ComponentMetadata> {
    if metadata.schema_version != 1 {
        return Err(CoreError::InvalidInput(format!(
            "不支持的组件配置版本 {}：{}",
            metadata.schema_version,
            path.display()
        )));
    }
    metadata.display_name = metadata.display_name.trim().to_owned();
    if metadata.display_name.is_empty() {
        return Err(CoreError::InvalidInput(format!(
            "组件显示名称不能为空：{}",
            path.display()
        )));
    }
    metadata.tags = normalize_tags(&metadata.tags);
    Ok(metadata)
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut normalized = tags
        .iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    normalized.sort_by_key(|tag| tag.to_lowercase());
    normalized.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    normalized
}

fn update_existing_metadata(path: &Path, metadata: &ComponentMetadata) -> Result<()> {
    let text = fs::read_to_string(path).map_err(|error| CoreError::io(path, error))?;
    let has_bom = text.starts_with('\u{feff}');
    let root = CstRootNode::parse(text.trim_start_matches('\u{feff}'), &parse_options())
        .map_err(|error| CoreError::json(path, error))?;
    let object = root.object_value().ok_or_else(|| {
        CoreError::InvalidInput(format!("JSON 根节点不是对象：{}", path.display()))
    })?;
    set_property(&object, "schema_version", CstInputValue::from(1));
    set_property(
        &object,
        "display_name",
        CstInputValue::from(metadata.display_name.clone()),
    );
    set_property(
        &object,
        "tags",
        CstInputValue::Array(
            metadata
                .tags
                .iter()
                .cloned()
                .map(CstInputValue::from)
                .collect(),
        ),
    );
    set_property(&object, "favorite", CstInputValue::from(metadata.favorite));

    let content = root.to_string();
    if has_bom {
        let mut bytes = Vec::with_capacity(3 + content.len());
        bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        bytes.extend_from_slice(content.as_bytes());
        atomic_write(path, &bytes)
    } else {
        atomic_write(path, content.as_bytes())
    }
}

fn set_property(object: &jsonc_parser::cst::CstObject, name: &str, value: CstInputValue) {
    if let Some(property) = object.get(name) {
        property.set_value(value);
    } else {
        object.append(name, value);
    }
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::InvalidInput("组件配置没有父目录".into()))?;
    let temporary = parent.join(format!(".mcdh-metadata-{}.tmp", Uuid::new_v4().simple()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| CoreError::io(&temporary, error))?;
        file.write_all(content)
            .map_err(|error| CoreError::io(&temporary, error))?;
        file.sync_all()
            .map_err(|error| CoreError::io(&temporary, error))?;
        drop(file);
        replace_file(&temporary, path).map_err(|error| CoreError::io(path, error))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_normalizes_and_updates_jsonc_without_losing_unknown_fields() {
        let temp = tempfile::tempdir().unwrap();
        let path = metadata_path(temp.path());
        fs::write(
            &path,
            r#"{
                // keep this component note
                "schema_version": 1,
                "display_name": "  可携带配置  ",
                "tags": [" 测试 ", "开发", "测试"],
                "favorite": false,
                "future_field": { "enabled": true },
            }"#,
        )
        .unwrap();

        let metadata = read_component_metadata(temp.path()).unwrap().unwrap();
        assert_eq!(metadata.display_name, "可携带配置");
        assert_eq!(metadata.tags, vec!["开发", "测试"]);

        write_component_metadata(
            temp.path(),
            &ComponentMetadata::new("新名称", vec!["发布".into()], true),
        )
        .unwrap();
        let written = fs::read_to_string(path).unwrap();
        assert!(written.contains("// keep this component note"));
        assert!(written.contains("future_field"));
        let metadata = read_component_metadata(temp.path()).unwrap().unwrap();
        assert_eq!(metadata.display_name, "新名称");
        assert_eq!(metadata.tags, vec!["发布"]);
        assert!(metadata.favorite);
    }

    #[test]
    fn refuses_to_overwrite_invalid_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let path = metadata_path(temp.path());
        fs::write(&path, b"{broken json").unwrap();

        assert!(
            write_component_metadata(
                temp.path(),
                &ComponentMetadata::new("修复", Vec::new(), false),
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"{broken json");
    }
}
