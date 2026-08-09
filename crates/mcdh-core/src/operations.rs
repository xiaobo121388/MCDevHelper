use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::{
    ComponentKind, ComponentSummary, CopyComponentRequest, CoreError, CreateComponentRequest,
    DiscoveryService, IdentityPolicy, LocalIndex, MoveComponentRequest, OperationResult, Result,
    TemplateRequest, TemplateService,
};

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
        let rendered = self.template.render(&TemplateRequest {
            name: request.name.clone(),
            kind: request.kind,
            destination: destination.clone(),
            mcs_compatible: request.mcs_compatible,
            component_uid: None,
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
            )?;
        }
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
                )?;
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

    fn find_component(&self, id: &str) -> Result<ComponentSummary> {
        let result = if let Some(roots) = &self.mcs_work_roots {
            self.discovery.refresh_with_mcs_work_roots(roots)?
        } else {
            self.discovery.refresh()?
        };
        result
            .components
            .into_iter()
            .find(|component| component.id == id)
            .ok_or_else(|| CoreError::InvalidInput(format!("找不到组件 ID：{id}")))
    }
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
            },
        ),
        CopyMode::Clean if component.kind == ComponentKind::Addon => {
            let mut report = CopyReport::default();
            let pack_directories = direct_pack_directories(&component.path)?;
            if pack_directories.is_empty() && component.path.join("manifest.json").is_file() {
                return copy_tree(
                    &component.path,
                    destination,
                    CopyFilter {
                        exclude_mcs: true,
                        exclude_dot: false,
                    },
                );
            }
            for pack in pack_directories {
                let target = destination.join(
                    pack.file_name()
                        .ok_or_else(|| CoreError::InvalidComponent(component.path.clone()))?,
                );
                let child_report = copy_tree(
                    &pack,
                    &target,
                    CopyFilter {
                        exclude_mcs: false,
                        exclude_dot: false,
                    },
                )?;
                let prefix = PathBuf::from(pack.file_name().unwrap());
                report.files.extend(
                    child_report
                        .files
                        .into_iter()
                        .map(|(path, size)| (prefix.join(path), size)),
                );
            }
            Ok(report)
        }
        CopyMode::Clean => copy_tree(
            &component.path,
            destination,
            CopyFilter {
                exclude_mcs: true,
                exclude_dot: true,
            },
        ),
    }
}

#[derive(Debug, Clone, Copy)]
struct CopyFilter {
    exclude_mcs: bool,
    exclude_dot: bool,
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

fn write_mcs_configuration(
    root: &Path,
    destination: &Path,
    uid: &str,
    name: &str,
    kind: ComponentKind,
) -> Result<()> {
    let rendered = TemplateService.render(&TemplateRequest {
        name: name.to_owned(),
        kind,
        destination: destination.to_path_buf(),
        mcs_compatible: true,
        component_uid: Some(uid.to_owned()),
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
        let mut document = read_json(&path)?;
        if let Some(header) = document.get_mut("header").and_then(Value::as_object_mut)
            && let Some(old_uuid) = header.get("uuid").and_then(Value::as_str)
        {
            let new_uuid = Uuid::new_v4().to_string();
            header_map.insert(old_uuid.to_owned(), new_uuid.clone());
            header.insert("uuid".into(), Value::String(new_uuid));
        }
        if let Some(modules) = document.get_mut("modules").and_then(Value::as_array_mut) {
            for module in modules {
                if let Some(module) = module.as_object_mut() {
                    module.insert("uuid".into(), Value::String(Uuid::new_v4().to_string()));
                }
            }
        }
        documents.push((path, document));
    }
    for (_, document) in &mut documents {
        rewrite_uuid_references(document, &header_map);
    }
    let mut modified = Vec::new();
    for (path, document) in documents {
        write_json(&path, &document)?;
        modified.push(path);
    }
    for name in ["world_behavior_packs.json", "world_resource_packs.json"] {
        let path = root.join(name);
        if path.is_file() {
            let mut document = read_json(&path)?;
            rewrite_uuid_references(&mut document, &header_map);
            write_json(&path, &document)?;
            modified.push(path);
        }
    }
    Ok(modified)
}

fn rewrite_uuid_references(value: &mut Value, mapping: &HashMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(replacement) = mapping.get(text) {
                *text = replacement.clone();
            }
        }
        Value::Array(values) => {
            for value in values {
                rewrite_uuid_references(value, mapping);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                rewrite_uuid_references(value, mapping);
            }
        }
        _ => {}
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
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|error| CoreError::json(path, error))
}

fn write_json(path: &Path, document: &Value) -> Result<()> {
    let mut content =
        serde_json::to_string_pretty(document).map_err(|error| CoreError::json(path, error))?;
    content.push('\n');
    fs::write(path, content).map_err(|error| CoreError::io(path, error))
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
    fs::canonicalize(path).map_err(|error| CoreError::io(path, error))
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
            })
            .unwrap();
        assert!(generic.actual_path.ends_with("示例_模组"));
        assert!(!generic.actual_path.join("studio.json").exists());
        assert_eq!(manifest_files(&generic.actual_path).unwrap().len(), 2);

        let mcs = service
            .create_component(&CreateComponentRequest {
                name: "空白地图".into(),
                kind: ComponentKind::Map,
                destination: mcs_root,
                mcs_compatible: true,
            })
            .unwrap();
        let studio = read_json(&mcs.actual_path.join("studio.json")).unwrap();
        assert_eq!(studio["Account"], "mcdh@local.invalid");
        assert_eq!(
            studio["SaveBackMapPath"].as_str(),
            Some(mcs.actual_path.to_string_lossy().as_ref())
        );
        assert!(mcs.actual_path.join("work.mcscfg").is_file());
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
}
