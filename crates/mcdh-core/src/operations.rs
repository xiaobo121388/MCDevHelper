use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use jsonc_parser::cst::{CstInputValue, CstNode, CstObject, CstRootNode};
use serde_json::Value;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::archive::{
    expand_nested_mcpacks, extract_archive, is_supported_archive, temporary_zip_path,
    write_addon_zip_roots, write_zip, write_zip_with_extra_file,
};
use crate::json::{parse_jsonc, parse_options};
use crate::metadata::{
    METADATA_FILE_NAME, metadata_bytes, metadata_path, normalized_metadata,
    read_component_metadata, write_component_metadata,
};
use crate::path_utils::canonicalize;
use crate::{
    BumpManifestVersionRequest, ComponentKind, ComponentOrigin, ComponentSummary, ContentMode,
    CopyComponentRequest, CoreError, CreateComponentRequest, DiscoveryService,
    ExportComponentRequest, IdentityPolicy, ImportComponentRequest, LocalIndex,
    McsTemplateIdentity, MoveComponentRequest, OperationResult, Result,
    SetComponentMetadataRequest, SetComponentTagsRequest, TemplateRequest, TemplateService,
    VersionPart, VsCodeStatus,
};

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_MOVE_AFTER_COPY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Debug, Clone)]
pub struct ComponentService {
    index: LocalIndex,
    discovery: DiscoveryService,
    template: TemplateService,
    mcs_work_roots: Option<Vec<PathBuf>>,
}

impl ComponentService {
    pub fn new(index: LocalIndex) -> Self {
        Self {
            discovery: DiscoveryService::new(index.clone()),
            index,
            template: TemplateService,
            mcs_work_roots: None,
        }
    }

    #[cfg(test)]
    fn with_mcs_work_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.mcs_work_roots = Some(roots);
        self
    }

    pub fn create_component(&self, request: &CreateComponentRequest) -> Result<OperationResult> {
        let _guard = self.index.try_lock_mutations()?;
        let destination = existing_directory(&request.destination)?;
        let mcs_identity = self.mcs_identity(request.namespace.as_deref())?;
        let rendered = self.template.render(&TemplateRequest {
            name: request.name.clone(),
            kind: request.kind,
            destination: destination.clone(),
            mcs_compatible: request.mcs_compatible,
            component_uid: None,
            mcs_identity,
        })?;
        let target = if request.mcs_compatible {
            destination.join(&rendered.component_uid)
        } else {
            unique_child(&destination, &sanitize_file_name(&request.name))
        };
        if target.exists() {
            return Err(CoreError::InvalidInput(format!(
                "随机生成的目标目录已存在：{}",
                target.display()
            )));
        }

        let mut staging = StagingDirectory::new(&destination)?;
        for directory in &rendered.directories {
            let path = staging.path().join(directory);
            fs::create_dir_all(&path).map_err(|error| CoreError::io(&path, error))?;
        }
        for file in &rendered.files {
            let path = staging.path().join(&file.relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| CoreError::io(parent, error))?;
            }
            fs::write(&path, file.content.as_bytes())
                .map_err(|error| CoreError::io(&path, error))?;
        }
        write_component_metadata(
            staging.path(),
            &normalized_metadata(&request.name, &[], false)?,
        )?;
        let actual_path = staging.publish(&target)?;
        let modified_files = collect_files(&actual_path)?;
        self.index.component_id(&actual_path)?;
        Ok(OperationResult {
            component: None,
            actual_path,
            modified_files,
            warnings: Vec::new(),
        })
    }

    pub fn copy_component(&self, request: &CopyComponentRequest) -> Result<OperationResult> {
        if request.identity_policy == IdentityPolicy::Error {
            return Err(CoreError::InvalidInput(
                "复制组件时 identity_policy 只能是 preserve 或 regenerate".into(),
            ));
        }
        let _guard = self.index.try_lock_mutations()?;
        let component = self.find_component(&request.component_id)?;
        let destination = existing_directory(&request.destination)?;
        ensure_not_inside(&component.path, &destination)?;
        let mcs_uid = request
            .mcs_compatible
            .then(|| Uuid::new_v4().simple().to_string());
        let target = match &mcs_uid {
            Some(uid) => destination.join(uid),
            None => unique_child(&destination, &sanitize_file_name(&component.name)),
        };
        let mut staging = StagingDirectory::new(&destination)?;
        let mode = copy_mode(&component, request.mcs_compatible, false);
        let report = copy_component_content(&component, staging.path(), mode)?;
        verify_copy(staging.path(), &report)?;
        if request.identity_policy == IdentityPolicy::Regenerate {
            regenerate_manifest_identifiers(staging.path())?;
        }
        if let Some(uid) = &mcs_uid {
            write_mcs_configuration(
                staging.path(),
                &destination,
                uid,
                &component.name,
                component.kind,
                &self.mcs_identity(None)?,
            )?;
        }
        preserve_or_create_metadata(&component, staging.path())?;
        let actual_path = staging.publish(&target)?;
        self.index.component_id(&actual_path)?;
        Ok(OperationResult {
            component: None,
            modified_files: collect_files(&actual_path)?,
            actual_path,
            warnings: Vec::new(),
        })
    }

    pub fn move_component(&self, request: &MoveComponentRequest) -> Result<OperationResult> {
        let _guard = self.index.try_lock_mutations()?;
        let component = self.find_component(&request.component_id)?;
        let destination = existing_directory(&request.destination)?;
        ensure_not_inside(&component.path, &destination)?;
        let source_is_mcs = component.mcs.is_some();
        let target_uid = if request.mcs_compatible {
            component
                .mcs
                .as_ref()
                .map(|mcs| mcs.uid.clone())
                .or_else(|| Some(Uuid::new_v4().simple().to_string()))
        } else {
            None
        };
        let target = match &target_uid {
            Some(uid) => unique_mcs_child(&destination, uid),
            None => unique_child(&destination, &sanitize_file_name(&component.name)),
        };

        let can_rename =
            !source_is_mcs && !request.mcs_compatible && same_volume(&component.path, &destination);
        let actual_path = if can_rename {
            fs::rename(&component.path, &target)
                .map_err(|error| CoreError::io(&component.path, error))?;
            target
        } else {
            let mut staging = StagingDirectory::new(&destination)?;
            let mode = copy_mode(&component, request.mcs_compatible, true);
            let report = copy_component_content(&component, staging.path(), mode)?;
            verify_copy(staging.path(), &report)?;
            if let Some(uid) = &target_uid {
                write_mcs_configuration(
                    staging.path(),
                    &destination,
                    uid,
                    &component.name,
                    component.kind,
                    &self.mcs_identity(None)?,
                )?;
            }
            preserve_metadata_file(&component.path, staging.path())?;
            if fail_move_after_copy() {
                return Err(CoreError::InvalidInput("测试注入的移动发布失败".into()));
            }
            let published = staging.publish(&target)?;
            fs::remove_dir_all(&component.path)
                .map_err(|error| CoreError::io(&component.path, error))?;
            published
        };
        self.index
            .move_component_metadata(&component.path, &actual_path)?;
        Ok(OperationResult {
            component: None,
            modified_files: collect_files(&actual_path)?,
            actual_path,
            warnings: Vec::new(),
        })
    }

    pub fn import_component(&self, request: &ImportComponentRequest) -> Result<OperationResult> {
        let _guard = self.index.try_lock_mutations()?;
        let source = canonicalize(&request.source)?;
        let destination = existing_directory(&request.destination)?;
        if source.is_dir() {
            ensure_not_inside(&source, &destination)?;
        } else if !source.is_file() || !is_supported_archive(&source) {
            return Err(CoreError::InvalidInput(
                "仅支持组件文件夹、ZIP、mcpack 或 mcaddon".into(),
            ));
        }

        let extraction = if source.is_file() {
            let temp = tempfile::Builder::new()
                .prefix("mcdh-import-")
                .tempdir()
                .map_err(|error| CoreError::io(&source, error))?;
            extract_archive(&source, temp.path())?;
            expand_nested_mcpacks(temp.path())?;
            Some(temp)
        } else {
            None
        };
        let unpacked = extraction
            .as_ref()
            .map_or(source.as_path(), tempfile::TempDir::path);
        let import_root = unwrap_single_directory(unpacked)?;
        let (kind, fallback_name) = inspect_import(&import_root, &source)?;
        let mut warnings = Vec::new();
        let imported_metadata = if request.content_mode == ContentMode::Full {
            match read_component_metadata(&import_root) {
                Ok(metadata) => metadata,
                Err(error) => {
                    warnings.push(error.to_string());
                    None
                }
            }
        } else {
            None
        };
        let name = imported_metadata
            .as_ref()
            .map(|metadata| metadata.display_name.clone())
            .unwrap_or(fallback_name);
        let duplicate_uuids = duplicate_manifest_uuids(&import_root, &self.current_components()?)?;
        if !duplicate_uuids.is_empty() && request.identity_policy == IdentityPolicy::Error {
            return Err(CoreError::Conflict(format!(
                "导入包与现有组件重复 UUID：{}",
                duplicate_uuids.join(", ")
            )));
        }

        let target_uid = request
            .mcs_compatible
            .then(|| Uuid::new_v4().simple().to_string());
        let target = match &target_uid {
            Some(uid) => destination.join(uid),
            None => unique_child(&destination, &sanitize_file_name(&name)),
        };
        let mut staging = StagingDirectory::new(&destination)?;
        let report = match request.content_mode {
            ContentMode::Clean => copy_clean_content(&import_root, staging.path(), kind, false)?,
            ContentMode::Full => copy_tree(
                &import_root,
                staging.path(),
                CopyFilter {
                    exclude_mcs: false,
                    exclude_dot: false,
                    exclude_python_artifacts: false,
                    exclude_metadata: false,
                },
            )?,
        };
        verify_copy(staging.path(), &report)?;
        if request.identity_policy == IdentityPolicy::Regenerate {
            regenerate_manifest_identifiers(staging.path())?;
        }
        if let Some(uid) = &target_uid {
            write_mcs_configuration(
                staging.path(),
                &destination,
                uid,
                &name,
                kind,
                &self.mcs_identity(None)?,
            )?;
        }
        if !metadata_path(staging.path()).exists() {
            write_component_metadata(staging.path(), &normalized_metadata(&name, &[], false)?)?;
        }
        let actual_path = staging.publish(&target)?;
        self.index.component_id(&actual_path)?;
        Ok(OperationResult {
            component: None,
            modified_files: collect_files(&actual_path)?,
            actual_path,
            warnings,
        })
    }

    pub fn export_component(&self, request: &ExportComponentRequest) -> Result<OperationResult> {
        let _guard = self.index.try_lock_mutations()?;
        let component_path = self.indexed_component_path(&request.component_id)?;
        let (component_kind, fallback_name) = inspect_export(&component_path)?;
        let mut warnings = Vec::new();
        let component_metadata = match read_component_metadata(&component_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                warnings.push(error.to_string());
                None
            }
        };
        let component_name = component_metadata
            .as_ref()
            .map(|metadata| metadata.display_name.clone())
            .unwrap_or(fallback_name);
        let destination = existing_directory(&request.destination)?;
        let archive_name = match request.content_mode {
            ContentMode::Clean => component_name.clone(),
            ContentMode::Full => format!("{component_name} 完整"),
        };
        let archive_path = unique_archive_path(&destination, &sanitize_file_name(&archive_name));
        let temporary = temporary_zip_path(&destination);
        let write_result = match request.content_mode {
            ContentMode::Clean => {
                write_clean_component_zip(&component_path, component_kind, &destination, &temporary)
            }
            ContentMode::Full => {
                let metadata_is_regular = fs::symlink_metadata(metadata_path(&component_path))
                    .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink());
                if metadata_is_regular {
                    write_zip(&component_path, &temporary)
                } else {
                    let generated = normalized_metadata(
                        &component_name,
                        &self.index.tags(&component_path)?,
                        false,
                    )?;
                    let bytes = metadata_bytes(&generated)?;
                    write_zip_with_extra_file(
                        &component_path,
                        &temporary,
                        METADATA_FILE_NAME,
                        &bytes,
                    )
                }
            }
        };
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        if let Err(error) = fs::rename(&temporary, &archive_path) {
            let _ = fs::remove_file(&temporary);
            return Err(CoreError::io(&archive_path, error));
        }
        Ok(OperationResult {
            component: None,
            actual_path: archive_path.clone(),
            modified_files: vec![archive_path],
            warnings,
        })
    }

    pub fn delete_component(&self, component_id: &str) -> Result<OperationResult> {
        let _guard = self.index.try_lock_mutations()?;
        let component = self.find_component(component_id)?;
        let path = canonicalize(&component.path)?;
        if !path.is_dir() || path.parent().is_none() || path.file_name().is_none() {
            return Err(CoreError::InvalidInput(
                "拒绝删除无效或过于宽泛的组件路径".into(),
            ));
        }
        fs::remove_dir_all(&path).map_err(|error| CoreError::io(&path, error))?;
        self.index.remove_component_metadata(&path)?;
        if let ComponentOrigin::Single { source_id } = &component.origin {
            self.index.remove_source(source_id)?;
        }
        Ok(OperationResult {
            component: Some(component),
            actual_path: path,
            modified_files: Vec::new(),
            warnings: Vec::new(),
        })
    }

    pub fn set_component_tags(&self, request: &SetComponentTagsRequest) -> Result<OperationResult> {
        let component = self.discovery.get_indexed(&request.component_id)?;
        self.set_component_metadata(&SetComponentMetadataRequest {
            component_id: request.component_id.clone(),
            display_name: component.name,
            tags: request.tags.clone(),
            favorite: component.favorite,
        })
    }

    pub fn set_component_metadata(
        &self,
        request: &SetComponentMetadataRequest,
    ) -> Result<OperationResult> {
        let _guard = self.index.try_lock_mutations()?;
        let component = self.discovery.get_indexed(&request.component_id)?;
        let metadata = normalized_metadata(&request.display_name, &request.tags, request.favorite)?;
        read_component_metadata(&component.path)?;
        let mut modified_files = Vec::new();
        let work_config = component.path.join("work.mcscfg");
        if component.mcs.is_some() && work_config.is_file() {
            let tags = metadata
                .tags
                .iter()
                .cloned()
                .map(CstInputValue::from)
                .collect();
            update_jsonc_property(&work_config, "CustomTags", CstInputValue::Array(tags))?;
            modified_files.push(work_config);
        }
        modified_files.push(write_component_metadata(&component.path, &metadata)?);
        self.index.set_tags(&component.path, &metadata.tags)?;
        let updated = self.discovery.get_indexed(&request.component_id)?;
        Ok(OperationResult {
            component: Some(updated),
            actual_path: component.path,
            modified_files,
            warnings: Vec::new(),
        })
    }

    pub fn regenerate_manifest_uuids(&self, component_id: &str) -> Result<OperationResult> {
        let _guard = self.index.try_lock_mutations()?;
        let path = self.indexed_component_path(component_id)?;
        let modified_files = regenerate_manifest_identifiers(&path)?;
        let updated = self.discovery.get_indexed(component_id)?;
        Ok(OperationResult {
            component: Some(updated),
            actual_path: path,
            modified_files,
            warnings: Vec::new(),
        })
    }

    pub fn bump_manifest_version(
        &self,
        request: &BumpManifestVersionRequest,
    ) -> Result<OperationResult> {
        let _guard = self.index.try_lock_mutations()?;
        let path = self.indexed_component_path(&request.component_id)?;
        let modified_files = bump_manifest_versions(&path, request.part)?;
        let updated = self.discovery.get_indexed(&request.component_id)?;
        Ok(OperationResult {
            component: Some(updated),
            actual_path: path,
            modified_files,
            warnings: Vec::new(),
        })
    }

    pub fn get_component(&self, component_id: &str) -> Result<ComponentSummary> {
        self.find_component(component_id)
    }

    fn mcs_identity(&self, namespace: Option<&str>) -> Result<McsTemplateIdentity> {
        let settings = self.index.app_settings()?;
        Ok(McsTemplateIdentity {
            developer_nickname: settings.developer_nickname,
            developer_account: settings.developer_account,
            developer_user_id: settings.developer_user_id,
            namespace: namespace.unwrap_or("mcdh").trim().to_owned(),
        })
    }

    pub fn open_component_directory(&self, component_id: &str) -> Result<()> {
        let path = self.indexed_component_path(component_id)?;
        Command::new("explorer.exe")
            .arg(&path)
            .spawn()
            .map_err(|error| CoreError::io(&path, error))?;
        Ok(())
    }

    pub fn vscode_status(&self) -> Result<VsCodeStatus> {
        let custom = self
            .index
            .setting("vscode_path")?
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

    pub fn set_vscode_path(&self, path: Option<&Path>) -> Result<()> {
        let value = match path {
            Some(path) => {
                if !path.is_file() {
                    return Err(CoreError::NotFound(path.to_path_buf()));
                }
                path.to_string_lossy().into_owned()
            }
            None => String::new(),
        };
        self.index.set_setting("vscode_path", &value)
    }

    pub fn open_component_in_vscode(&self, component_id: &str) -> Result<()> {
        let path = self.indexed_component_path(component_id)?;
        let configured = self
            .index
            .setting("vscode_path")?
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        let executable = configured
            .filter(|path| path.is_file())
            .or_else(detect_vscode)
            .ok_or_else(|| CoreError::NotFound(PathBuf::from("Visual Studio Code")))?;
        let target = preferred_workspace(&path).map_err(|error| CoreError::io(&path, error))?;
        Command::new(&executable)
            .arg(&target)
            .spawn()
            .map_err(|error| CoreError::io(&executable, error))?;
        Ok(())
    }

    fn find_component(&self, id: &str) -> Result<ComponentSummary> {
        self.current_components()?
            .into_iter()
            .find(|component| component.id == id)
            .ok_or_else(|| CoreError::InvalidInput(format!("找不到组件 ID：{id}")))
    }

    fn indexed_component_path(&self, id: &str) -> Result<PathBuf> {
        let path = self
            .index
            .component_path(id)?
            .ok_or_else(|| CoreError::InvalidInput(format!("找不到组件 ID：{id}")))?;
        if !path.is_dir() {
            return Err(CoreError::NotFound(path));
        }
        Ok(path)
    }

    fn current_components(&self) -> Result<Vec<ComponentSummary>> {
        let result = if let Some(roots) = &self.mcs_work_roots {
            self.discovery.refresh_with_mcs_work_roots(roots)?
        } else {
            self.discovery.refresh()?
        };
        Ok(result.components)
    }
}

fn fail_move_after_copy() -> bool {
    #[cfg(test)]
    {
        FAIL_NEXT_MOVE_AFTER_COPY.with(|flag| flag.replace(false))
    }
    #[cfg(not(test))]
    false
}

fn preferred_workspace(root: &Path) -> std::io::Result<PathBuf> {
    let workspaces = fs::read_dir(root)?
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

#[derive(Debug, Clone, Copy)]
enum CopyMode {
    Full { exclude_mcs: bool },
    Clean,
}

fn copy_mode(component: &ComponentSummary, target_is_mcs: bool, moving: bool) -> CopyMode {
    match (component.mcs.is_some(), target_is_mcs, moving) {
        (true, false, _) => CopyMode::Clean,
        (_, true, _) => CopyMode::Full { exclude_mcs: true },
        _ => CopyMode::Full { exclude_mcs: false },
    }
}

#[derive(Debug, Default)]
struct CopyReport {
    files: Vec<(PathBuf, u64)>,
}

fn copy_component_content(
    component: &ComponentSummary,
    destination: &Path,
    mode: CopyMode,
) -> Result<CopyReport> {
    match mode {
        CopyMode::Full { exclude_mcs } => copy_tree(
            &component.path,
            destination,
            CopyFilter {
                exclude_mcs,
                exclude_dot: false,
                exclude_python_artifacts: false,
                exclude_metadata: false,
            },
        ),
        CopyMode::Clean => copy_clean_content(&component.path, destination, component.kind, false),
    }
}

fn preserve_or_create_metadata(component: &ComponentSummary, destination: &Path) -> Result<()> {
    preserve_metadata_file(&component.path, destination)?;
    if !metadata_path(destination).exists() {
        write_component_metadata(
            destination,
            &normalized_metadata(&component.name, &component.tags, component.favorite)?,
        )?;
    }
    Ok(())
}

fn preserve_metadata_file(source: &Path, destination: &Path) -> Result<()> {
    let source = metadata_path(source);
    let target = metadata_path(destination);
    if target.exists() {
        return Ok(());
    }
    let metadata = match fs::symlink_metadata(&source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(CoreError::io(&source, error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(());
    }
    fs::copy(&source, &target).map_err(|error| CoreError::io(&target, error))?;
    Ok(())
}

fn copy_clean_content(
    source: &Path,
    destination: &Path,
    kind: ComponentKind,
    exclude_python_artifacts: bool,
) -> Result<CopyReport> {
    if kind != ComponentKind::Addon {
        return copy_tree(
            source,
            destination,
            CopyFilter {
                exclude_mcs: true,
                exclude_dot: true,
                exclude_python_artifacts: false,
                exclude_metadata: true,
            },
        );
    }

    let mut report = CopyReport::default();
    let pack_directories = direct_pack_directories(source)?;
    if pack_directories.is_empty() && source.join("manifest.json").is_file() {
        return copy_tree(
            source,
            destination,
            CopyFilter {
                exclude_mcs: true,
                exclude_dot: false,
                exclude_python_artifacts,
                exclude_metadata: true,
            },
        );
    }
    for pack in pack_directories {
        let file_name = pack
            .file_name()
            .ok_or_else(|| CoreError::InvalidComponent(source.to_path_buf()))?;
        let target = destination.join(file_name);
        let child_report = copy_tree(
            &pack,
            &target,
            CopyFilter {
                exclude_mcs: false,
                exclude_dot: false,
                exclude_python_artifacts,
                exclude_metadata: true,
            },
        )?;
        let prefix = PathBuf::from(file_name);
        report.files.extend(
            child_report
                .files
                .into_iter()
                .map(|(path, size)| (prefix.join(path), size)),
        );
    }
    if report.files.is_empty() {
        return Err(CoreError::InvalidComponent(source.to_path_buf()));
    }
    Ok(report)
}

fn write_clean_component_zip(
    source: &Path,
    kind: ComponentKind,
    staging_parent: &Path,
    destination: &Path,
) -> Result<()> {
    if kind == ComponentKind::Addon {
        let pack_directories = direct_pack_directories(source)?;
        if !pack_directories.is_empty() {
            return write_addon_zip_roots(&pack_directories, destination);
        }
    }

    let staging = tempfile::Builder::new()
        .prefix(".mcdh-export-")
        .tempdir_in(staging_parent)
        .map_err(|error| CoreError::io(staging_parent, error))?;
    let report = copy_clean_content(source, staging.path(), kind, kind == ComponentKind::Addon)?;
    verify_copy(staging.path(), &report)?;
    write_zip(staging.path(), destination)
}

#[derive(Debug, Clone, Copy)]
struct CopyFilter {
    exclude_mcs: bool,
    exclude_dot: bool,
    exclude_python_artifacts: bool,
    exclude_metadata: bool,
}

fn copy_tree(source: &Path, destination: &Path, filter: CopyFilter) -> Result<CopyReport> {
    fs::create_dir_all(destination).map_err(|error| CoreError::io(destination, error))?;
    let mut report = CopyReport::default();
    let walker = WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| include_entry(source, entry, filter));
    for entry in walker {
        let entry = entry.map_err(|error| {
            let path = error.path().unwrap_or(source);
            CoreError::io(
                path,
                error
                    .io_error()
                    .map(|error| std::io::Error::new(error.kind(), error.to_string()))
                    .unwrap_or_else(|| std::io::Error::other(error.to_string())),
            )
        })?;
        if entry.path() == source || entry.file_type().is_symlink() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| CoreError::InvalidInput("无法计算复制相对路径".into()))?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(|error| CoreError::io(&target, error))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| CoreError::io(parent, error))?;
            }
            fs::copy(entry.path(), &target).map_err(|error| CoreError::io(&target, error))?;
            let size = entry
                .metadata()
                .map_err(|error| CoreError::io(entry.path(), std::io::Error::other(error)))?
                .len();
            report.files.push((relative.to_path_buf(), size));
        }
    }
    Ok(report)
}

fn include_entry(source: &Path, entry: &DirEntry, filter: CopyFilter) -> bool {
    if entry.path() == source {
        return true;
    }
    let relative = match entry.path().strip_prefix(source) {
        Ok(relative) => relative,
        Err(_) => return false,
    };
    if filter.exclude_dot
        && relative
            .components()
            .any(|component| component.as_os_str().to_string_lossy().starts_with('.'))
    {
        return false;
    }
    if filter.exclude_mcs && relative.components().count() == 1 {
        let name = relative.file_name();
        if matches!(name, Some(name) if name == OsStr::new("studio.json") || name == OsStr::new("work.mcscfg"))
        {
            return false;
        }
    }
    if filter.exclude_metadata
        && relative.components().count() == 1
        && relative.file_name() == Some(OsStr::new(METADATA_FILE_NAME))
    {
        return false;
    }
    if filter.exclude_python_artifacts
        && relative
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("pyi") || extension.eq_ignore_ascii_case("pyc")
            })
    {
        return false;
    }
    true
}

fn direct_pack_directories(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(root)
        .map_err(|error| CoreError::io(root, error))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir() && !kind.is_symlink())
                .map(|_| entry.path())
        })
        .filter(|path| path.join("manifest.json").is_file())
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}

fn unwrap_single_directory(root: &Path) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    for _ in 0..4 {
        let mut entries = fs::read_dir(&current)
            .map_err(|error| CoreError::io(&current, error))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| CoreError::io(&current, error))?;
        entries.retain(|entry| entry.file_name() != OsStr::new("__MACOSX"));
        if entries.len() != 1 {
            break;
        }
        let entry = &entries[0];
        let file_type = entry
            .file_type()
            .map_err(|error| CoreError::io(entry.path(), error))?;
        if !file_type.is_dir() || file_type.is_symlink() {
            break;
        }
        current = entry.path();
    }
    Ok(current)
}

fn inspect_import(root: &Path, source: &Path) -> Result<(ComponentKind, String)> {
    let fallback_name = source
        .file_stem()
        .or_else(|| source.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "导入组件".into());
    let work_config = root.join("work.mcscfg");
    if work_config.is_file() {
        let document = read_json(&work_config)?;
        let component_type = document
            .get("Type")
            .and_then(Value::as_i64)
            .ok_or_else(|| CoreError::InvalidComponent(root.to_path_buf()))?;
        let kind = match component_type {
            1 => ComponentKind::Map,
            3 | 4 => ComponentKind::Material,
            7 => ComponentKind::Addon,
            _ => return Err(CoreError::InvalidComponent(root.to_path_buf())),
        };
        let name = document
            .get("Name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&fallback_name)
            .to_owned();
        return Ok((kind, name));
    }

    if root.join("db").is_dir()
        || root.join("level.dat").is_file()
        || root.join("behavior_packs").is_dir()
        || root.join("resource_packs").is_dir()
    {
        return Ok((ComponentKind::Map, fallback_name));
    }

    let root_manifest = root.join("manifest.json");
    if root_manifest.is_file() {
        let document = read_json(&root_manifest)?;
        let module_types = document
            .get("modules")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|module| module.get("type").and_then(Value::as_str))
            .collect::<Vec<_>>();
        let kind = if module_types
            .iter()
            .any(|module_type| matches!(*module_type, "data" | "script"))
        {
            ComponentKind::Addon
        } else if module_types.contains(&"resources") {
            ComponentKind::Material
        } else {
            return Err(CoreError::InvalidComponent(root.to_path_buf()));
        };
        let name = document
            .get("header")
            .and_then(|header| header.get("name"))
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&fallback_name)
            .to_owned();
        return Ok((kind, name));
    }

    if !direct_pack_directories(root)?.is_empty() {
        return Ok((ComponentKind::Addon, fallback_name));
    }
    Err(CoreError::InvalidComponent(root.to_path_buf()))
}

fn inspect_export(root: &Path) -> Result<(ComponentKind, String)> {
    let (kind, mut name) = inspect_import(root, root)?;
    if kind == ComponentKind::Addon
        && !root.join("work.mcscfg").is_file()
        && !root.join("manifest.json").is_file()
    {
        for pack in direct_pack_directories(root)? {
            let manifest = read_json(&pack.join("manifest.json"))?;
            if let Some(manifest_name) = manifest
                .get("header")
                .and_then(|header| header.get("name"))
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
            {
                name = manifest_name.to_owned();
                break;
            }
        }
    }
    Ok((kind, name))
}

fn duplicate_manifest_uuids(
    imported_root: &Path,
    existing_components: &[ComponentSummary],
) -> Result<Vec<String>> {
    let existing = existing_components
        .iter()
        .flat_map(|component| &component.manifests)
        .filter_map(|manifest| manifest.header_uuid.as_deref())
        .map(str::to_ascii_lowercase)
        .collect::<std::collections::HashSet<_>>();
    let mut duplicates = Vec::new();
    for path in manifest_files(imported_root)? {
        let document = read_json(&path)?;
        let Some(uuid) = document
            .get("header")
            .and_then(|header| header.get("uuid"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if existing.contains(&uuid.to_ascii_lowercase()) {
            duplicates.push(uuid.to_owned());
        }
    }
    duplicates.sort();
    duplicates.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    Ok(duplicates)
}

fn unique_archive_path(parent: &Path, base_name: &str) -> PathBuf {
    let first = parent.join(format!("{base_name}.zip"));
    if !first.exists() {
        return first;
    }
    for suffix in 2.. {
        let candidate = parent.join(format!("{base_name} ({suffix}).zip"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn write_mcs_configuration(
    root: &Path,
    destination: &Path,
    uid: &str,
    name: &str,
    kind: ComponentKind,
    mcs_identity: &McsTemplateIdentity,
) -> Result<()> {
    let rendered = TemplateService.render(&TemplateRequest {
        name: name.to_owned(),
        kind,
        destination: destination.to_path_buf(),
        mcs_compatible: true,
        component_uid: Some(uid.to_owned()),
        mcs_identity: mcs_identity.clone(),
    })?;
    for file in rendered.files.into_iter().filter(|file| {
        matches!(
            file.relative_path.file_name().and_then(OsStr::to_str),
            Some("studio.json" | "work.mcscfg")
        )
    }) {
        let path = root.join(file.relative_path);
        fs::write(&path, file.content).map_err(|error| CoreError::io(&path, error))?;
    }
    Ok(())
}

fn regenerate_manifest_identifiers(root: &Path) -> Result<Vec<PathBuf>> {
    let manifests = manifest_files(root)?;
    let mut documents = Vec::new();
    let mut header_map = HashMap::new();
    for path in manifests {
        let document = EditableJsonc::read(&path)?;
        if let Some(root_object) = document.root.object_value() {
            if let Some(header) = root_object.object_value("header")
                && let Some(old_uuid) = object_string(&header, "uuid")
            {
                let new_uuid = Uuid::new_v4().to_string();
                header_map.insert(old_uuid.to_ascii_lowercase(), new_uuid.clone());
                set_string_property(&header, "uuid", new_uuid);
            }
            if let Some(modules) = root_object.array_value("modules") {
                for module in modules.elements() {
                    if let Some(module) = module.as_object() {
                        set_string_property(&module, "uuid", Uuid::new_v4().to_string());
                    }
                }
            }
        }
        documents.push((path, document));
    }
    for (_, document) in &mut documents {
        if let Some(value) = document.root.value() {
            rewrite_uuid_references(&value, &header_map);
        }
    }
    for name in ["world_behavior_packs.json", "world_resource_packs.json"] {
        let path = root.join(name);
        if path.is_file() {
            let document = EditableJsonc::read(&path)?;
            if let Some(value) = document.root.value() {
                rewrite_uuid_references(&value, &header_map);
            }
            documents.push((path, document));
        }
    }
    let mut modified = Vec::new();
    for (path, document) in documents {
        document.write(&path)?;
        modified.push(path);
    }
    Ok(modified)
}

fn bump_manifest_versions(root: &Path, part: VersionPart) -> Result<Vec<PathBuf>> {
    let manifests = manifest_files(root)?;
    if manifests.is_empty() {
        return Err(CoreError::InvalidComponent(root.to_path_buf()));
    }
    let mut documents = Vec::new();
    let mut version_map = HashMap::new();
    for path in manifests {
        let document = EditableJsonc::read(&path)?;
        let Some(root_object) = document.root.object_value() else {
            continue;
        };
        let Some(header) = root_object.object_value("header") else {
            continue;
        };
        let Some(version) = object_version(&header, "version") else {
            continue;
        };
        let new_version = bump_version(version, part)?;
        if let Some(uuid) = object_string(&header, "uuid") {
            version_map.insert(uuid.to_ascii_lowercase(), new_version);
        }
        set_version_property(&header, "version", new_version);
        if let Some(modules) = root_object.array_value("modules") {
            for module in modules.elements() {
                if let Some(module) = module.as_object() {
                    set_version_property(&module, "version", new_version);
                }
            }
        }
        documents.push((path, document));
    }
    if documents.is_empty() {
        return Err(CoreError::InvalidInput(
            "组件中没有可提升的 manifest 版本".into(),
        ));
    }

    for (_, document) in &mut documents {
        if let Some(value) = document.root.value() {
            rewrite_version_references(&value, &version_map);
        }
    }
    for name in ["world_behavior_packs.json", "world_resource_packs.json"] {
        let path = root.join(name);
        if path.is_file() {
            let document = EditableJsonc::read(&path)?;
            if let Some(value) = document.root.value() {
                rewrite_version_references(&value, &version_map);
            }
            documents.push((path, document));
        }
    }

    let mut modified = Vec::new();
    for (path, document) in documents {
        document.write(&path)?;
        modified.push(path);
    }
    Ok(modified)
}

fn bump_version(version: [u64; 3], part: VersionPart) -> Result<[u64; 3]> {
    let overflow = || CoreError::InvalidInput("manifest 版本号已达到上限".into());
    match part {
        VersionPart::Major => Ok([version[0].checked_add(1).ok_or_else(overflow)?, 0, 0]),
        VersionPart::Minor => Ok([
            version[0],
            version[1].checked_add(1).ok_or_else(overflow)?,
            0,
        ]),
        VersionPart::Patch => Ok([
            version[0],
            version[1],
            version[2].checked_add(1).ok_or_else(overflow)?,
        ]),
    }
}

fn rewrite_version_references(node: &CstNode, mapping: &HashMap<String, [u64; 3]>) {
    if let Some(object) = node.as_object() {
        let referenced_uuid = object_string(&object, "uuid")
            .or_else(|| object_string(&object, "pack_id"))
            .map(|uuid| uuid.to_ascii_lowercase());
        if let Some(version) = referenced_uuid.and_then(|uuid| mapping.get(&uuid)) {
            set_version_property(&object, "version", *version);
        }
        for property in object.properties() {
            if let Some(value) = property.value() {
                rewrite_version_references(&value, mapping);
            }
        }
    } else if let Some(array) = node.as_array() {
        for element in array.elements() {
            rewrite_version_references(&element, mapping);
        }
    }
}

fn object_version(object: &CstObject, name: &str) -> Option<[u64; 3]> {
    let values = object.array_value(name)?.elements();
    if values.len() != 3 {
        return None;
    }
    Some([
        values[0].as_number_lit()?.to_string().parse().ok()?,
        values[1].as_number_lit()?.to_string().parse().ok()?,
        values[2].as_number_lit()?.to_string().parse().ok()?,
    ])
}

fn set_version_property(object: &CstObject, name: &str, version: [u64; 3]) {
    if let Some(array) = object.array_value(name) {
        let elements = array.elements();
        if elements.len() == 3
            && elements
                .iter()
                .all(|element| element.as_number_lit().is_some())
        {
            for (element, value) in elements.into_iter().zip(version) {
                element
                    .as_number_lit()
                    .expect("version element was checked as a number")
                    .set_raw_value(value.to_string());
            }
            return;
        }
    }

    let value = version_input(version);
    if let Some(property) = object.get(name) {
        property.set_value(value);
    } else {
        object.append(name, value);
    }
}

fn version_input(version: [u64; 3]) -> CstInputValue {
    CstInputValue::Array(version.into_iter().map(CstInputValue::from).collect())
}

fn object_string(object: &CstObject, name: &str) -> Option<String> {
    object
        .get(name)?
        .value()?
        .as_string_lit()?
        .decoded_value()
        .ok()
}

fn set_string_property(object: &CstObject, name: &str, value: String) {
    if let Some(property) = object.get(name) {
        if let Some(string) = property.value().and_then(|value| value.as_string_lit()) {
            string.set_raw_value(format!("\"{value}\""));
        } else {
            property.set_value(CstInputValue::String(value));
        }
    } else {
        object.append(name, CstInputValue::String(value));
    }
}

fn rewrite_uuid_references(node: &CstNode, mapping: &HashMap<String, String>) {
    if let Some(string) = node.as_string_lit() {
        if let Ok(value) = string.decoded_value()
            && let Some(replacement) = mapping.get(&value.to_ascii_lowercase())
        {
            string.set_raw_value(format!("\"{replacement}\""));
        }
    } else if let Some(array) = node.as_array() {
        for element in array.elements() {
            rewrite_uuid_references(&element, mapping);
        }
    } else if let Some(object) = node.as_object() {
        for property in object.properties() {
            if let Some(value) = property.value() {
                rewrite_uuid_references(&value, mapping);
            }
        }
    }
}

fn manifest_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).max_depth(4).follow_links(false) {
        let entry = entry.map_err(|error| {
            CoreError::io(
                error.path().unwrap_or(root),
                std::io::Error::other(error.to_string()),
            )
        })?;
        if entry.file_type().is_file() && entry.file_name() == OsStr::new("manifest.json") {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

fn read_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).map_err(|error| CoreError::io(path, error))?;
    parse_jsonc(&text, path)
}

struct EditableJsonc {
    root: CstRootNode,
    has_bom: bool,
}

impl EditableJsonc {
    fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|error| CoreError::io(path, error))?;
        let has_bom = text.starts_with('\u{feff}');
        let text = text.trim_start_matches('\u{feff}');
        let root = CstRootNode::parse(text, &parse_options())
            .map_err(|error| CoreError::json(path, error))?;
        Ok(Self { root, has_bom })
    }

    fn write(self, path: &Path) -> Result<()> {
        let content = self.root.to_string();
        if self.has_bom {
            let mut bytes = Vec::with_capacity(3 + content.len());
            bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
            bytes.extend_from_slice(content.as_bytes());
            atomic_write(path, &bytes)
        } else {
            atomic_write(path, content.as_bytes())
        }
    }
}

fn update_jsonc_property(path: &Path, name: &str, value: CstInputValue) -> Result<()> {
    let document = EditableJsonc::read(path)?;
    let object = document.root.object_value().ok_or_else(|| {
        CoreError::InvalidInput(format!("JSON 根节点不是对象：{}", path.display()))
    })?;
    if let Some(property) = object.get(name) {
        property.set_value(value);
    } else {
        object.append(name, value);
    }
    document.write(path)
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::InvalidInput("JSON 文件没有父目录".into()))?;
    let temporary = parent.join(format!(".mcdh-json-{}.tmp", Uuid::new_v4().simple()));
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
        replace_file(&temporary, path).map_err(|error| CoreError::io(path, error))
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

fn verify_copy(root: &Path, report: &CopyReport) -> Result<()> {
    for (relative, expected_size) in &report.files {
        let path = root.join(relative);
        let size = fs::metadata(&path)
            .map_err(|error| CoreError::io(&path, error))?
            .len();
        if size != *expected_size {
            return Err(CoreError::InvalidInput(format!(
                "复制校验失败：{}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| {
            CoreError::io(
                error.path().unwrap_or(root),
                std::io::Error::other(error.to_string()),
            )
        })?;
        if entry.file_type().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

fn existing_directory(path: &Path) -> Result<PathBuf> {
    if !path.is_dir() {
        return Err(CoreError::NotFound(path.to_path_buf()));
    }
    canonicalize(path)
}

fn ensure_not_inside(source: &Path, destination: &Path) -> Result<()> {
    if destination.starts_with(source) {
        return Err(CoreError::InvalidInput(
            "目标目录不能位于组件目录内部".into(),
        ));
    }
    Ok(())
}

fn unique_child(parent: &Path, base_name: &str) -> PathBuf {
    let first = parent.join(base_name);
    if !first.exists() {
        return first;
    }
    for suffix in 2.. {
        let candidate = parent.join(format!("{base_name} ({suffix})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn unique_mcs_child(parent: &Path, uid: &str) -> PathBuf {
    let target = parent.join(uid);
    if !target.exists() {
        return target;
    }
    unique_child(parent, uid)
}

fn sanitize_file_name(name: &str) -> String {
    let mut sanitized = name
        .trim()
        .chars()
        .map(|character| {
            if matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            ) || character.is_control()
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    sanitized = sanitized.trim_end_matches([' ', '.']).to_owned();
    if sanitized.is_empty() {
        sanitized = "未命名组件".into();
    }
    let stem = sanitized
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        sanitized.insert(0, '_');
    }
    sanitized
}

fn same_volume(left: &Path, right: &Path) -> bool {
    fn prefix(path: &Path) -> Option<String> {
        path.components().find_map(|component| match component {
            Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().to_lowercase()),
            Component::RootDir => Some("/".into()),
            _ => None,
        })
    }
    prefix(left) == prefix(right)
}

struct StagingDirectory {
    path: PathBuf,
    published: bool,
}

impl StagingDirectory {
    fn new(parent: &Path) -> Result<Self> {
        for _ in 0..16 {
            let path = parent.join(format!(".mcdh-staging-{}", Uuid::new_v4().simple()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        published: false,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(CoreError::io(&path, error)),
            }
        }
        Err(CoreError::InvalidInput("无法创建操作暂存目录".into()))
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn publish(&mut self, target: &Path) -> Result<PathBuf> {
        fs::rename(&self.path, target).map_err(|error| CoreError::io(target, error))?;
        self.published = true;
        Ok(target.to_path_buf())
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if !self.published && self.path.is_dir() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceKind;
    use std::io::{Cursor, Read, Write};
    use zip::write::SimpleFileOptions;
    use zip::{ZipArchive, ZipWriter};

    fn write_json(path: &Path, value: Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }

    fn manifest(name: &str, module_type: &str, header_uuid: Uuid) -> Value {
        serde_json::json!({
            "format_version": 2,
            "header": {"name": name, "uuid": header_uuid, "version": [0, 0, 1]},
            "modules": [{"type": module_type, "uuid": Uuid::new_v4(), "version": [0, 0, 1]}]
        })
    }

    fn archive_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn write_archive(path: &Path, entries: &[(&str, Vec<u8>)]) {
        fs::write(path, archive_bytes(entries)).unwrap();
    }

    #[test]
    fn creates_generic_and_mcs_components() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let service = ComponentService::new(index);
        let generic_root = temp.path().join("generic");
        let mcs_root = temp.path().join("mcs");
        fs::create_dir_all(&generic_root).unwrap();
        fs::create_dir_all(&mcs_root).unwrap();

        let generic = service
            .create_component(&CreateComponentRequest {
                name: "示例:模组".into(),
                kind: ComponentKind::Addon,
                destination: generic_root,
                mcs_compatible: false,
                namespace: None,
            })
            .unwrap();
        assert!(generic.actual_path.ends_with("示例_模组"));
        assert!(!generic.actual_path.join("studio.json").exists());
        assert!(generic.actual_path.join(METADATA_FILE_NAME).is_file());
        assert_eq!(manifest_files(&generic.actual_path).unwrap().len(), 2);

        let mcs = service
            .create_component(&CreateComponentRequest {
                name: "空白地图".into(),
                kind: ComponentKind::Map,
                destination: mcs_root,
                mcs_compatible: true,
                namespace: Some("mcdh".into()),
            })
            .unwrap();
        let studio = read_json(&mcs.actual_path.join("studio.json")).unwrap();
        assert_eq!(studio["Account"], "mcdh@local.invalid");
        assert_eq!(
            studio["SaveBackMapPath"].as_str(),
            Some(mcs.actual_path.to_string_lossy().as_ref())
        );
        assert!(mcs.actual_path.join("work.mcscfg").is_file());
        assert!(mcs.actual_path.join(METADATA_FILE_NAME).is_file());
    }

    #[test]
    fn resolves_open_targets_from_the_index_without_discovery() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let component = temp.path().join("indexed-component");
        fs::create_dir_all(&component).unwrap();
        let component_id = index.component_id(&component).unwrap();

        let broken_library = temp.path().join("broken-library");
        fs::create_dir_all(broken_library.join("broken-component")).unwrap();
        fs::write(
            broken_library.join("broken-component/manifest.json"),
            b"{broken json",
        )
        .unwrap();
        index
            .add_source(SourceKind::Library, &broken_library)
            .unwrap();

        let service = ComponentService::new(index);
        assert_eq!(
            service.indexed_component_path(&component_id).unwrap(),
            component
        );

        fs::remove_dir_all(&component).unwrap();
        assert!(matches!(
            service.indexed_component_path(&component_id),
            Err(CoreError::NotFound(path)) if path == component
        ));
    }

    #[test]
    fn prefers_only_a_single_workspace_file() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let first = root.join("project.code-workspace");
        fs::write(&first, b"{}").unwrap();
        assert_eq!(preferred_workspace(root).unwrap(), first);

        fs::write(root.join("second.code-workspace"), b"{}").unwrap();
        assert_eq!(preferred_workspace(root).unwrap(), root);
    }

    #[test]
    fn deletes_a_discovered_component_and_its_single_source() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let component_path = temp.path().join("待删除组件");
        write_json(
            &component_path.join("manifest.json"),
            manifest("待删除组件", "resources", Uuid::new_v4()),
        );
        index
            .add_source(SourceKind::Single, &component_path)
            .unwrap();
        let service = ComponentService::new(index.clone()).with_mcs_work_roots(Vec::new());
        let component = service.current_components().unwrap().remove(0);

        let result = service.delete_component(&component.id).unwrap();
        assert_eq!(result.actual_path, component_path);
        assert!(!component_path.exists());
        assert!(index.list_sources().unwrap().is_empty());
    }

    #[test]
    fn copies_with_new_manifest_ids_and_moves_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("library");
        let copies = temp.path().join("copies");
        let moved = temp.path().join("moved");
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(&copies).unwrap();
        fs::create_dir_all(&moved).unwrap();
        let source = library.join("材质");
        let old_uuid = Uuid::new_v4();
        write_json(
            &source.join("manifest.json"),
            manifest("材质", "resources", old_uuid),
        );
        index.add_source(SourceKind::Library, &library).unwrap();
        let service = ComponentService::new(index.clone());
        let component = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(&[])
            .unwrap()
            .components
            .remove(0);
        let original_id = component.id.clone();

        let copied = service
            .copy_component(&CopyComponentRequest {
                component_id: component.id.clone(),
                destination: copies,
                mcs_compatible: false,
                identity_policy: IdentityPolicy::Regenerate,
            })
            .unwrap();
        let copied_manifest = read_json(&copied.actual_path.join("manifest.json")).unwrap();
        assert_ne!(copied_manifest["header"]["uuid"], old_uuid.to_string());
        assert!(copied.actual_path.join(METADATA_FILE_NAME).is_file());

        let moved_result = service
            .move_component(&MoveComponentRequest {
                component_id: component.id,
                destination: moved,
                mcs_compatible: false,
            })
            .unwrap();
        assert!(!source.exists());
        assert!(moved_result.actual_path.join("manifest.json").is_file());
        assert_eq!(
            index.component_id(&moved_result.actual_path).unwrap(),
            original_id
        );
    }

    #[test]
    fn cleans_mcs_addon_when_copying_to_a_normal_directory() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let work = temp.path().join("work");
        let addon = work.join("account/Cpp/AddOn/addon-id");
        write_json(
            &addon.join("work.mcscfg"),
            serde_json::json!({"UID": "addon-id", "Type": 7, "Name": "模组"}),
        );
        write_json(
            &addon.join("studio.json"),
            serde_json::json!({"Account": "private@example.com"}),
        );
        write_json(
            &addon.join("behavior_pack/manifest.json"),
            manifest("BP", "data", Uuid::new_v4()),
        );
        write_json(
            &addon.join("resource_pack/manifest.json"),
            manifest("RP", "resources", Uuid::new_v4()),
        );
        fs::create_dir_all(addon.join(".mcs")).unwrap();
        fs::write(addon.join("notes.txt"), b"not package content").unwrap();
        let output = temp.path().join("output");
        fs::create_dir(&output).unwrap();
        let service = ComponentService::new(index.clone()).with_mcs_work_roots(vec![work]);
        let component = DiscoveryService::new(index)
            .refresh_with_mcs_work_roots(&[temp.path().join("work")])
            .unwrap()
            .components
            .remove(0);

        let copied = service
            .copy_component(&CopyComponentRequest {
                component_id: component.id,
                destination: output,
                mcs_compatible: false,
                identity_policy: IdentityPolicy::Preserve,
            })
            .unwrap();
        assert!(copied.actual_path.join("behavior_pack").is_dir());
        assert!(copied.actual_path.join("resource_pack").is_dir());
        assert!(!copied.actual_path.join("studio.json").exists());
        assert!(!copied.actual_path.join("notes.txt").exists());
        assert!(!copied.actual_path.join(".mcs").exists());
    }

    #[test]
    fn imports_nested_mcaddon_and_exports_a_clean_numbered_zip() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("组件库");
        let exports = temp.path().join("导出");
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(&exports).unwrap();
        index.add_source(SourceKind::Library, &library).unwrap();

        let behavior_manifest =
            serde_json::to_vec(&manifest("Behavior", "data", Uuid::new_v4())).unwrap();
        let resource_manifest =
            serde_json::to_vec(&manifest("Resources", "resources", Uuid::new_v4())).unwrap();
        let behavior = archive_bytes(&[("manifest.json", behavior_manifest)]);
        let resources = archive_bytes(&[("manifest.json", resource_manifest)]);
        let package = temp.path().join("组合包.mcaddon");
        write_archive(
            &package,
            &[
                ("behavior.mcpack", behavior),
                ("resources.mcpack", resources),
                ("studio.json", b"private metadata".to_vec()),
            ],
        );

        let service = ComponentService::new(index.clone()).with_mcs_work_roots(Vec::new());
        let imported = service
            .import_component(&ImportComponentRequest {
                source: package,
                destination: library.clone(),
                mcs_compatible: false,
                identity_policy: IdentityPolicy::Preserve,
                content_mode: ContentMode::Clean,
            })
            .unwrap();
        assert!(
            imported
                .actual_path
                .join("behavior/manifest.json")
                .is_file()
        );
        assert!(
            imported
                .actual_path
                .join("resources/manifest.json")
                .is_file()
        );
        assert!(!imported.actual_path.join("studio.json").exists());

        let component = DiscoveryService::new(index)
            .refresh_with_mcs_work_roots(&[])
            .unwrap()
            .components
            .remove(0);
        fs::write(exports.join("组合包.zip"), b"existing").unwrap();
        let exported = service
            .export_component(&ExportComponentRequest {
                component_id: component.id,
                destination: exports,
                content_mode: ContentMode::Clean,
            })
            .unwrap();
        assert!(exported.actual_path.ends_with("组合包 (2).zip"));

        let file = fs::File::open(exported.actual_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let mut names = Vec::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            names.push(entry.name().to_owned());
        }
        assert!(names.contains(&"behavior/manifest.json".into()));
        assert!(names.contains(&"resources/manifest.json".into()));
        assert!(!names.iter().any(|name| name.ends_with("studio.json")));
    }

    #[test]
    fn exports_and_imports_full_component_backups_without_cleaning_files() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("library");
        let exports = temp.path().join("exports");
        let full_restore = temp.path().join("full-restore");
        let clean_restore = temp.path().join("clean-restore");
        for directory in [&library, &exports, &full_restore, &clean_restore] {
            fs::create_dir_all(directory).unwrap();
        }
        let map = library.join("完整地图");
        fs::create_dir_all(map.join("db")).unwrap();
        fs::write(map.join("db/chunk.bin"), b"world").unwrap();
        fs::write(map.join(".hidden"), b"development").unwrap();
        fs::write(map.join("types.pyi"), b"typing").unwrap();
        write_json(
            &map.join("work.mcscfg"),
            serde_json::json!({"Type": 1, "Name": "完整地图"}),
        );
        write_json(
            &map.join("studio.json"),
            serde_json::json!({"private": true}),
        );
        index.set_tags(&map, &["备份".into()]).unwrap();
        index.add_source(SourceKind::Library, &library).unwrap();
        let component = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(&[])
            .unwrap()
            .components
            .remove(0);
        assert!(!map.join(METADATA_FILE_NAME).exists());
        let service = ComponentService::new(index).with_mcs_work_roots(Vec::new());

        let clean = service
            .export_component(&ExportComponentRequest {
                component_id: component.id.clone(),
                destination: exports.clone(),
                content_mode: ContentMode::Clean,
            })
            .unwrap();
        let full = service
            .export_component(&ExportComponentRequest {
                component_id: component.id,
                destination: exports,
                content_mode: ContentMode::Full,
            })
            .unwrap();
        assert!(!map.join(METADATA_FILE_NAME).exists());

        let archive_names = |path: &Path| {
            let mut archive = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
            (0..archive.len())
                .map(|index| archive.by_index(index).unwrap().name().to_owned())
                .collect::<Vec<_>>()
        };
        let clean_names = archive_names(&clean.actual_path);
        assert!(!clean_names.iter().any(|name| {
            name == METADATA_FILE_NAME
                || name == ".hidden"
                || name == "studio.json"
                || name == "work.mcscfg"
        }));
        let full_names = archive_names(&full.actual_path);
        for expected in [
            METADATA_FILE_NAME,
            ".hidden",
            "types.pyi",
            "studio.json",
            "work.mcscfg",
            "db/chunk.bin",
        ] {
            assert!(
                full_names.contains(&expected.to_owned()),
                "missing {expected}"
            );
        }
        assert!(full.actual_path.ends_with("完整地图 完整.zip"));

        let restored = service
            .import_component(&ImportComponentRequest {
                source: full.actual_path.clone(),
                destination: full_restore,
                mcs_compatible: false,
                identity_policy: IdentityPolicy::Preserve,
                content_mode: ContentMode::Full,
            })
            .unwrap();
        for expected in [
            METADATA_FILE_NAME,
            ".hidden",
            "types.pyi",
            "studio.json",
            "work.mcscfg",
        ] {
            assert!(restored.actual_path.join(expected).exists());
        }
        let restored_metadata = read_component_metadata(&restored.actual_path)
            .unwrap()
            .unwrap();
        assert_eq!(restored_metadata.tags, vec!["备份"]);

        let cleaned = service
            .import_component(&ImportComponentRequest {
                source: full.actual_path,
                destination: clean_restore,
                mcs_compatible: false,
                identity_policy: IdentityPolicy::Preserve,
                content_mode: ContentMode::Clean,
            })
            .unwrap();
        assert!(cleaned.actual_path.join(METADATA_FILE_NAME).is_file());
        assert!(!cleaned.actual_path.join(".hidden").exists());
        assert!(!cleaned.actual_path.join("studio.json").exists());
        assert!(!cleaned.actual_path.join("work.mcscfg").exists());
    }

    #[test]
    fn exports_indexed_mcs_addon_without_rescanning_development_directories() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("library");
        let output = temp.path().join("exports");
        let addon = library.join("9c7e890e089f4d339af868e08d4bcd7f");
        fs::create_dir_all(addon.join(".venv/Lib/site-packages/noise")).unwrap();
        fs::create_dir_all(&output).unwrap();
        fs::write(
            addon.join(".venv/Lib/site-packages/noise/manifest.json"),
            b"development-only",
        )
        .unwrap();
        write_json(
            &addon.join("work.mcscfg"),
            serde_json::json!({
                "UID": "9c7e890e089f4d339af868e08d4bcd7f",
                "Type": 7,
                "Name": "空中厕所2"
            }),
        );
        write_json(
            &addon.join("behavior_pack_Z6mMrsGM/manifest.json"),
            manifest("Behavior", "data", Uuid::new_v4()),
        );
        fs::create_dir_all(addon.join("behavior_pack_Z6mMrsGM/scripts/cache")).unwrap();
        fs::write(
            addon.join("behavior_pack_Z6mMrsGM/scripts/server.py"),
            b"print('kept')",
        )
        .unwrap();
        fs::write(
            addon.join("behavior_pack_Z6mMrsGM/scripts/server.pyi"),
            b"def start() -> None: ...",
        )
        .unwrap();
        fs::write(
            addon.join("behavior_pack_Z6mMrsGM/scripts/cache/server.pyc"),
            b"bytecode",
        )
        .unwrap();
        write_json(
            &addon.join("resource_pack_p69M2JA2/manifest.json"),
            manifest("Resources", "resources", Uuid::new_v4()),
        );
        let source = index.add_source(SourceKind::Library, &library).unwrap();
        let component = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(&[])
            .unwrap()
            .components
            .remove(0);

        index.remove_source(&source.id).unwrap();
        let exported = ComponentService::new(index)
            .with_mcs_work_roots(Vec::new())
            .export_component(&ExportComponentRequest {
                component_id: component.id,
                destination: output,
                content_mode: ContentMode::Clean,
            })
            .unwrap();
        assert!(exported.component.is_none());
        assert!(exported.actual_path.ends_with("空中厕所2.zip"));

        let mut archive = ZipArchive::new(fs::File::open(exported.actual_path).unwrap()).unwrap();
        let names = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_owned())
            .collect::<Vec<_>>();
        assert!(names.contains(&"behavior_pack_Z6mMrsGM/manifest.json".into()));
        assert!(names.contains(&"resource_pack_p69M2JA2/manifest.json".into()));
        assert!(names.contains(&"behavior_pack_Z6mMrsGM/scripts/server.py".into()));
        assert!(!names.iter().any(|name| {
            let lower = name.to_ascii_lowercase();
            lower.ends_with(".pyi") || lower.ends_with(".pyc")
        }));
        assert!(!names.iter().any(|name| name.contains(".venv")));
        assert!(!names.iter().any(|name| name.ends_with("work.mcscfg")));
    }

    #[test]
    fn applies_duplicate_uuid_policy_during_import() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("library");
        let imports = temp.path().join("imports");
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(&imports).unwrap();
        let duplicate_uuid = Uuid::new_v4();
        write_json(
            &library.join("existing/manifest.json"),
            manifest("Existing", "resources", duplicate_uuid),
        );
        index.add_source(SourceKind::Library, &library).unwrap();
        let package = temp.path().join("duplicate.mcpack");
        write_archive(
            &package,
            &[(
                "manifest.json",
                serde_json::to_vec(&manifest("Duplicate", "resources", duplicate_uuid)).unwrap(),
            )],
        );
        let service = ComponentService::new(index).with_mcs_work_roots(Vec::new());

        let error = service
            .import_component(&ImportComponentRequest {
                source: package.clone(),
                destination: imports.clone(),
                mcs_compatible: false,
                identity_policy: IdentityPolicy::Error,
                content_mode: ContentMode::Clean,
            })
            .unwrap_err();
        assert_eq!(error.code(), "conflict");

        let imported = service
            .import_component(&ImportComponentRequest {
                source: package,
                destination: imports,
                mcs_compatible: false,
                identity_policy: IdentityPolicy::Regenerate,
                content_mode: ContentMode::Clean,
            })
            .unwrap();
        let document = read_json(&imported.actual_path.join("manifest.json")).unwrap();
        assert_ne!(document["header"]["uuid"], duplicate_uuid.to_string());
    }

    #[test]
    fn regenerates_manifest_ids_and_rewrites_internal_references() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("library");
        let map_root = library.join("依赖地图");
        fs::create_dir_all(map_root.join("db")).unwrap();
        let behavior_uuid = Uuid::new_v4();
        let resource_uuid = Uuid::new_v4();
        let mut behavior = manifest("Behavior", "data", behavior_uuid);
        behavior["dependencies"] = serde_json::json!([{
            "uuid": resource_uuid,
            "version": [0, 0, 1]
        }]);
        write_json(&map_root.join("behavior_packs/bp/manifest.json"), behavior);
        write_json(
            &map_root.join("resource_packs/rp/manifest.json"),
            manifest("Resources", "resources", resource_uuid),
        );
        write_json(
            &map_root.join("world_behavior_packs.json"),
            serde_json::json!([{"pack_id": behavior_uuid, "version": [0, 0, 1]}]),
        );
        write_json(
            &map_root.join("world_resource_packs.json"),
            serde_json::json!([{"pack_id": resource_uuid, "version": [0, 0, 1]}]),
        );
        index.add_source(SourceKind::Library, &library).unwrap();
        let component = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(&[])
            .unwrap()
            .components
            .remove(0);
        let service = ComponentService::new(index).with_mcs_work_roots(Vec::new());

        let result = service.regenerate_manifest_uuids(&component.id).unwrap();
        assert_eq!(result.modified_files.len(), 4);
        let behavior = read_json(&map_root.join("behavior_packs/bp/manifest.json")).unwrap();
        let resources = read_json(&map_root.join("resource_packs/rp/manifest.json")).unwrap();
        let behavior_new = behavior["header"]["uuid"].as_str().unwrap();
        let resource_new = resources["header"]["uuid"].as_str().unwrap();
        assert_ne!(behavior_new, behavior_uuid.to_string());
        assert_ne!(resource_new, resource_uuid.to_string());
        assert_eq!(behavior["dependencies"][0]["uuid"], resource_new);
        assert_eq!(
            read_json(&map_root.join("world_behavior_packs.json")).unwrap()[0]["pack_id"],
            behavior_new
        );
        assert_eq!(
            read_json(&map_root.join("world_resource_packs.json")).unwrap()[0]["pack_id"],
            resource_new
        );
    }

    #[test]
    fn bumps_header_module_dependency_and_world_versions() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("library");
        let map_root = library.join("版本地图");
        fs::create_dir_all(map_root.join("db")).unwrap();
        let behavior_uuid = Uuid::new_v4();
        let resource_uuid = Uuid::new_v4();
        let mut behavior = manifest("Behavior", "data", behavior_uuid);
        behavior["header"]["version"] = serde_json::json!([1, 2, 3]);
        behavior["modules"][0]["version"] = serde_json::json!([1, 2, 3]);
        behavior["dependencies"] = serde_json::json!([{
            "uuid": resource_uuid,
            "version": [2, 0, 0]
        }]);
        let mut resources = manifest("Resources", "resources", resource_uuid);
        resources["header"]["version"] = serde_json::json!([2, 0, 0]);
        resources["modules"][0]["version"] = serde_json::json!([2, 0, 0]);
        write_json(&map_root.join("behavior_packs/bp/manifest.json"), behavior);
        write_json(&map_root.join("resource_packs/rp/manifest.json"), resources);
        write_json(
            &map_root.join("world_resource_packs.json"),
            serde_json::json!([{"pack_id": resource_uuid, "version": [2, 0, 0]}]),
        );
        index.add_source(SourceKind::Library, &library).unwrap();
        let component = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(&[])
            .unwrap()
            .components
            .remove(0);
        let service = ComponentService::new(index).with_mcs_work_roots(Vec::new());

        service
            .bump_manifest_version(&BumpManifestVersionRequest {
                component_id: component.id,
                part: VersionPart::Patch,
            })
            .unwrap();
        let behavior = read_json(&map_root.join("behavior_packs/bp/manifest.json")).unwrap();
        assert_eq!(behavior["header"]["version"], serde_json::json!([1, 2, 4]));
        assert_eq!(
            behavior["modules"][0]["version"],
            serde_json::json!([1, 2, 4])
        );
        assert_eq!(
            behavior["dependencies"][0]["version"],
            serde_json::json!([2, 0, 1])
        );
        assert_eq!(
            read_json(&map_root.join("world_resource_packs.json")).unwrap()[0]["version"],
            serde_json::json!([2, 0, 1])
        );
    }

    #[test]
    fn discovers_and_updates_jsonc_component_files() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("library");
        let component_root = library.join("JSONC 模组");
        fs::create_dir_all(&component_root).unwrap();
        let header_uuid = Uuid::new_v4();
        let module_uuid = Uuid::new_v4();
        fs::write(
            component_root.join("manifest.json"),
            format!(
                r#"{{
                    // manifest may contain line comments
                    "format_version": 2,
                    "header": {{
                        "name": "JSONC 模组",
                        // header UUID comment must survive
                        "uuid": "{header_uuid}",
                        "version": [
                            // major version comment must survive
                            1, 0, 0,
                        ],
                    }},
                    /* block comments are supported too */
                    "modules": [{{
                        "type": "data",
                        "uuid": "{module_uuid}",
                        "version": [1, 0, 0,], /* module version comment must survive */
                    }}],
                    "dependencies": [{{
                        // dependency reference comment must survive
                        "uuid": "{header_uuid}",
                        "version": [1, 0, 0,],
                    }}],
                }}"#
            ),
        )
        .unwrap();
        fs::write(
            component_root.join("world_behavior_packs.json"),
            format!(
                r#"[
                    {{
                        // world pack comment must survive
                        "pack_id": "{header_uuid}",
                        "version": [1, 0, 0,],
                    }},
                ]"#
            ),
        )
        .unwrap();
        fs::write(
            component_root.join("work.mcscfg"),
            r#"{
                // MCS configuration is JSONC as well
                "Type": 7,
                "Name": "JSONC 模组",
            }"#,
        )
        .unwrap();
        index.add_source(SourceKind::Library, &library).unwrap();
        let component = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(&[])
            .unwrap()
            .components
            .remove(0);
        assert_eq!(component.name, "JSONC 模组");

        let service = ComponentService::new(index).with_mcs_work_roots(Vec::new());
        let regenerated = service.regenerate_manifest_uuids(&component.id).unwrap();
        assert!(regenerated.component.is_some());
        let after_uuid = fs::read_to_string(component_root.join("manifest.json")).unwrap();
        assert!(after_uuid.contains("// manifest may contain line comments"));
        assert!(after_uuid.contains("// header UUID comment must survive"));
        assert!(after_uuid.contains("// major version comment must survive"));
        assert!(after_uuid.contains("// dependency reference comment must survive"));
        assert!(after_uuid.contains("/* block comments are supported too */"));
        assert!(after_uuid.contains("/* module version comment must survive */"));
        assert!(!after_uuid.contains(&header_uuid.to_string()));
        assert!(!after_uuid.contains(&module_uuid.to_string()));
        let world_after_uuid =
            fs::read_to_string(component_root.join("world_behavior_packs.json")).unwrap();
        assert!(world_after_uuid.contains("// world pack comment must survive"));
        assert!(!world_after_uuid.contains(&header_uuid.to_string()));

        service
            .bump_manifest_version(&BumpManifestVersionRequest {
                component_id: component.id,
                part: VersionPart::Patch,
            })
            .unwrap();
        let written = fs::read_to_string(component_root.join("manifest.json")).unwrap();
        assert!(written.contains("// manifest may contain line comments"));
        assert!(written.contains("// header UUID comment must survive"));
        assert!(written.contains("// major version comment must survive"));
        assert!(written.contains("// dependency reference comment must survive"));
        assert!(written.contains("/* block comments are supported too */"));
        assert!(written.contains("/* module version comment must survive */"));
        let written: Value = parse_jsonc(&written, "written-jsonc").unwrap();
        assert_eq!(written["header"]["version"], serde_json::json!([1, 0, 1]));
        assert_eq!(
            written["modules"][0]["version"],
            serde_json::json!([1, 0, 1])
        );
        assert_eq!(
            written["dependencies"][0]["version"],
            serde_json::json!([1, 0, 1])
        );
        let world_written =
            fs::read_to_string(component_root.join("world_behavior_packs.json")).unwrap();
        assert!(world_written.contains("// world pack comment must survive"));
        let world_written: Value = parse_jsonc(&world_written, "written-world-jsonc").unwrap();
        assert_eq!(world_written[0]["version"], serde_json::json!([1, 0, 1]));
    }

    #[test]
    fn stores_normalized_tags_and_syncs_mcs_custom_tags() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let work = temp.path().join("work");
        let addon = work.join("account/Cpp/AddOn/addon-id");
        fs::create_dir_all(&addon).unwrap();
        fs::write(
            addon.join("work.mcscfg"),
            r#"{
                // custom MCS comment must survive tag updates
                "UID": "addon-id",
                "Type": 7,
                "Name": "标签模组",
                "CustomTags": [],
            }"#,
        )
        .unwrap();
        write_json(
            &addon.join("behavior/manifest.json"),
            manifest("Behavior", "data", Uuid::new_v4()),
        );
        let component = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(std::slice::from_ref(&work))
            .unwrap()
            .components
            .remove(0);
        let service = ComponentService::new(index).with_mcs_work_roots(vec![work]);

        let result = service
            .set_component_tags(&SetComponentTagsRequest {
                component_id: component.id,
                tags: vec![" 开发 ".into(), "测试".into(), "开发".into()],
            })
            .unwrap();
        assert_eq!(result.component.unwrap().tags, vec!["开发", "测试"]);
        assert!(
            fs::read_to_string(addon.join("work.mcscfg"))
                .unwrap()
                .contains("// custom MCS comment must survive tag updates")
        );
        assert_eq!(
            read_json(&addon.join("work.mcscfg")).unwrap()["CustomTags"],
            serde_json::json!(["开发", "测试"])
        );
        let metadata = read_component_metadata(&addon).unwrap().unwrap();
        assert_eq!(metadata.display_name, "标签模组");
        assert_eq!(metadata.tags, vec!["开发", "测试"]);
        assert!(!metadata.favorite);
    }

    #[test]
    fn updates_display_metadata_without_rewriting_mcs_names() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let work = temp.path().join("work");
        let addon = work.join("account/Cpp/AddOn/addon-id");
        fs::create_dir_all(&addon).unwrap();
        fs::write(
            addon.join("work.mcscfg"),
            r#"{
                "UID": "addon-id",
                "Type": 7,
                "Name": "MCS 原名称",
                "CustomTags": [],
            }"#,
        )
        .unwrap();
        write_json(
            &addon.join("behavior/manifest.json"),
            manifest("Behavior", "data", Uuid::new_v4()),
        );
        fs::write(
            addon.join(METADATA_FILE_NAME),
            r#"{
                // preserve metadata comments
                "schema_version": 1,
                "display_name": "旧显示名称",
                "tags": [],
                "favorite": false,
                "future": "preserved",
            }"#,
        )
        .unwrap();
        let component = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(std::slice::from_ref(&work))
            .unwrap()
            .components
            .remove(0);
        let service = ComponentService::new(index).with_mcs_work_roots(vec![work]);

        let result = service
            .set_component_metadata(&SetComponentMetadataRequest {
                component_id: component.id,
                display_name: " 新显示名称 ".into(),
                tags: vec![" 收藏 ".into(), "收藏".into()],
                favorite: true,
            })
            .unwrap();
        let updated = result.component.unwrap();
        assert_eq!(updated.name, "新显示名称");
        assert_eq!(updated.tags, vec!["收藏"]);
        assert!(updated.favorite);
        assert_eq!(
            read_json(&addon.join("work.mcscfg")).unwrap()["Name"],
            "MCS 原名称"
        );
        let written = fs::read_to_string(addon.join(METADATA_FILE_NAME)).unwrap();
        assert!(written.contains("// preserve metadata comments"));
        assert!(written.contains("future"));
    }

    #[test]
    fn exports_maps_and_materials_with_clean_content_rules() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("library");
        let output = temp.path().join("exports");
        fs::create_dir_all(&output).unwrap();

        let map = library.join("地图白名单");
        fs::create_dir_all(map.join("db")).unwrap();
        fs::write(map.join("db/chunk.bin"), b"world").unwrap();
        fs::write(map.join("level.dat"), b"level").unwrap();
        fs::write(map.join(".secret"), b"hidden").unwrap();
        write_json(
            &map.join("studio.json"),
            serde_json::json!({"private": true}),
        );
        write_json(&map.join("work.mcscfg"), serde_json::json!({"Type": 1}));

        let material = library.join("材质白名单");
        write_json(
            &material.join("manifest.json"),
            manifest("Material", "resources", Uuid::new_v4()),
        );
        fs::create_dir_all(material.join("textures")).unwrap();
        fs::write(material.join("textures/terrain.png"), b"png").unwrap();
        fs::create_dir_all(material.join(".cache")).unwrap();
        fs::write(material.join(".cache/private.bin"), b"hidden").unwrap();
        write_json(
            &material.join("work.mcscfg"),
            serde_json::json!({"Type": 3}),
        );
        index.add_source(SourceKind::Library, &library).unwrap();
        let components = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(&[])
            .unwrap()
            .components;
        let service = ComponentService::new(index).with_mcs_work_roots(Vec::new());

        for component in components {
            let result = service
                .export_component(&ExportComponentRequest {
                    component_id: component.id,
                    destination: output.clone(),
                    content_mode: ContentMode::Clean,
                })
                .unwrap();
            let file = fs::File::open(result.actual_path).unwrap();
            let mut archive = ZipArchive::new(file).unwrap();
            let names = (0..archive.len())
                .map(|index| archive.by_index(index).unwrap().name().to_owned())
                .collect::<Vec<_>>();
            assert!(!names.iter().any(|name| {
                name.split('/').any(|part| part.starts_with('.'))
                    || name.ends_with("studio.json")
                    || name.ends_with("work.mcscfg")
            }));
            match component.kind {
                ComponentKind::Map => {
                    assert!(names.contains(&"db/chunk.bin".into()));
                    assert!(names.contains(&"level.dat".into()));
                }
                ComponentKind::Material => {
                    assert!(names.contains(&"manifest.json".into()));
                    assert!(names.contains(&"textures/terrain.png".into()));
                }
                ComponentKind::Addon => unreachable!(),
            }
        }
    }

    #[test]
    fn rolls_back_relocation_staging_when_publish_fails_after_copy() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("state/mcdh.db")).unwrap();
        let library = temp.path().join("library");
        let destination = temp.path().join("mcs-target");
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let source = library.join("rollback-addon");
        write_json(
            &source.join("behavior/manifest.json"),
            manifest("Rollback", "data", Uuid::new_v4()),
        );
        index.add_source(SourceKind::Library, &library).unwrap();
        let component = DiscoveryService::new(index.clone())
            .refresh_with_mcs_work_roots(&[])
            .unwrap()
            .components
            .remove(0);
        let service = ComponentService::new(index).with_mcs_work_roots(Vec::new());
        FAIL_NEXT_MOVE_AFTER_COPY.with(|flag| flag.set(true));

        let error = service
            .move_component(&MoveComponentRequest {
                component_id: component.id,
                destination: destination.clone(),
                mcs_compatible: true,
            })
            .unwrap_err();
        assert_eq!(error.code(), "invalid_input");
        assert!(source.join("behavior/manifest.json").is_file());
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    }
}
