use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;
use uuid::Uuid;

use crate::path_utils::canonicalize;
use crate::{
    ComponentKind, ComponentOrigin, ComponentSummary, CoreError, DiscoveryResult, DiscoveryWarning,
    LocalIndex, ManifestSummary, McsInfo, Result, SourceKind, SourceRecord,
};

const MCS_CATEGORIES: [(&str, ComponentKind, i64); 4] = [
    ("AddOn", ComponentKind::Addon, 7),
    ("Map", ComponentKind::Map, 1),
    ("Material", ComponentKind::Material, 3),
    ("Light", ComponentKind::Material, 4),
];

#[derive(Debug, Clone)]
pub struct DiscoveryService {
    index: LocalIndex,
}

impl DiscoveryService {
    pub fn new(index: LocalIndex) -> Self {
        Self { index }
    }

    pub fn refresh(&self) -> Result<DiscoveryResult> {
        self.refresh_with_mcs_work_roots(&automatic_mcs_work_roots())
    }

    pub fn refresh_with_mcs_work_roots(&self, work_roots: &[PathBuf]) -> Result<DiscoveryResult> {
        let mut components = Vec::new();
        let mut sources = Vec::new();
        let mut warnings = Vec::new();
        let mut visited = HashSet::new();

        self.scan_mcs(
            work_roots,
            &mut components,
            &mut sources,
            &mut warnings,
            &mut visited,
        );

        for source in self.index.list_sources()? {
            sources.push(source.clone());
            match source.kind {
                SourceKind::Single => self.inspect_and_push(
                    &source.path,
                    ComponentOrigin::Single {
                        source_id: source.id.clone(),
                    },
                    None,
                    &mut components,
                    &mut warnings,
                    &mut visited,
                ),
                SourceKind::Library => match child_directories(&source.path) {
                    Ok(children) => {
                        for child in children {
                            self.inspect_and_push(
                                &child,
                                ComponentOrigin::Library {
                                    source_id: source.id.clone(),
                                },
                                None,
                                &mut components,
                                &mut warnings,
                                &mut visited,
                            );
                        }
                    }
                    Err(error) => warnings.push(warning_from_error(error)),
                },
                SourceKind::McsAuto => {}
            }
        }

        components.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| path_key(&left.path).cmp(&path_key(&right.path)))
        });
        sources.sort_by_key(|source| path_key(&source.path));

        Ok(DiscoveryResult {
            components,
            sources,
            warnings,
        })
    }

    pub fn get(&self, id: &str) -> Result<ComponentSummary> {
        self.refresh()?
            .components
            .into_iter()
            .find(|component| component.id == id)
            .ok_or_else(|| CoreError::InvalidInput(format!("找不到组件 ID：{id}")))
    }

    fn scan_mcs(
        &self,
        work_roots: &[PathBuf],
        components: &mut Vec<ComponentSummary>,
        sources: &mut Vec<SourceRecord>,
        warnings: &mut Vec<DiscoveryWarning>,
        visited: &mut HashSet<String>,
    ) {
        for work_root in work_roots {
            let accounts = match child_directories(work_root) {
                Ok(accounts) => accounts,
                Err(CoreError::NotFound(_)) => continue,
                Err(error) => {
                    warnings.push(warning_from_error(error));
                    continue;
                }
            };
            for account_path in accounts {
                let account = account_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned());
                let cpp_root = account_path.join("Cpp");
                for (category, kind, component_type) in MCS_CATEGORIES {
                    let category_path = cpp_root.join(category);
                    if !category_path.is_dir() {
                        continue;
                    }
                    let source_id = mcs_source_id(&category_path);
                    sources.push(SourceRecord {
                        id: source_id,
                        kind: SourceKind::McsAuto,
                        path: category_path.clone(),
                    });
                    let children = match child_directories(&category_path) {
                        Ok(children) => children,
                        Err(error) => {
                            warnings.push(warning_from_error(error));
                            continue;
                        }
                    };
                    for child in children {
                        let context = McsContext {
                            account: account.clone(),
                            category: category.to_owned(),
                            default_kind: kind,
                            default_type: component_type,
                        };
                        self.inspect_and_push(
                            &child,
                            ComponentOrigin::Mcs {
                                source_path: category_path.clone(),
                            },
                            Some(context),
                            components,
                            warnings,
                            visited,
                        );
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn inspect_and_push(
        &self,
        path: &Path,
        origin: ComponentOrigin,
        mcs_context: Option<McsContext>,
        components: &mut Vec<ComponentSummary>,
        warnings: &mut Vec<DiscoveryWarning>,
        visited: &mut HashSet<String>,
    ) {
        let canonical = match canonicalize(path) {
            Ok(path) => path,
            Err(error) => {
                warnings.push(DiscoveryWarning {
                    path: path.to_path_buf(),
                    message: error.to_string(),
                });
                return;
            }
        };
        if !visited.insert(path_key(&canonical)) {
            return;
        }
        let is_single = matches!(origin_kind(&origin), SourceKind::Single);
        match self.inspect(&canonical, origin, mcs_context) {
            Ok(component) => components.push(component),
            Err(CoreError::InvalidComponent(_)) => {
                if is_single {
                    warnings.push(DiscoveryWarning {
                        path: canonical,
                        message: "所选目录不是受支持的组件".into(),
                    });
                }
            }
            Err(error) => warnings.push(warning_from_error(error)),
        }
    }

    fn inspect(
        &self,
        path: &Path,
        origin: ComponentOrigin,
        mcs_context: Option<McsContext>,
    ) -> Result<ComponentSummary> {
        let work_config = read_optional_json(&path.join("work.mcscfg"))?;
        let manifest_paths = find_manifest_paths(path)?;
        let mut manifests = manifest_paths
            .iter()
            .map(|manifest_path| read_manifest(manifest_path))
            .collect::<Result<Vec<_>>>()?;
        manifests.sort_by_key(|manifest| path_key(&manifest.path));

        let configured_type = work_config
            .as_ref()
            .and_then(|config| config.get("Type"))
            .and_then(Value::as_i64);
        let kind = if let Some(component_type) = configured_type {
            kind_from_mcs_type(component_type)
                .ok_or_else(|| CoreError::InvalidComponent(path.to_path_buf()))?
        } else if let Some(context) = &mcs_context {
            context.default_kind
        } else {
            infer_kind(path, &manifests)
                .ok_or_else(|| CoreError::InvalidComponent(path.to_path_buf()))?
        };

        let mcs = mcs_context.as_ref().map(|context| McsInfo {
            uid: work_config
                .as_ref()
                .and_then(|config| config.get("UID"))
                .and_then(Value::as_str)
                .filter(|uid| !uid.is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| file_name(path)),
            component_type: configured_type.unwrap_or(context.default_type),
            account: context.account.clone(),
            category: context.category.clone(),
        });

        let name = work_config
            .as_ref()
            .and_then(|config| config.get("Name"))
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| manifests.iter().find_map(|manifest| manifest.name.clone()))
            .unwrap_or_else(|| file_name(path));
        let version = manifests.iter().find_map(|manifest| manifest.version);
        let icon_path = find_icon(path, &manifests);
        let modified_at = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .map(DateTime::<Utc>::from);
        let id = self.index.component_id(path)?;
        let tags = self.index.tags(path)?;

        Ok(ComponentSummary {
            id,
            name,
            kind,
            path: path.to_path_buf(),
            origin,
            mcs,
            manifests,
            version,
            tags,
            icon_path,
            modified_at,
        })
    }
}

#[derive(Debug, Clone)]
struct McsContext {
    account: Option<String>,
    category: String,
    default_kind: ComponentKind,
    default_type: i64,
}

fn child_directories(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        return Err(CoreError::NotFound(path.to_path_buf()));
    }
    let mut children = fs::read_dir(path)
        .map_err(|error| CoreError::io(path, error))?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|file_type| file_type.is_dir() && !file_type.is_symlink())
                .map(|_| entry.path())
        })
        .collect::<Vec<_>>();
    children.sort_by_key(|child| path_key(child));
    Ok(children)
}

fn read_optional_json(path: &Path) -> Result<Option<Value>> {
    if !path.is_file() {
        return Ok(None);
    }
    read_json(path).map(Some)
}

fn read_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).map_err(|error| CoreError::io(path, error))?;
    serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|error| CoreError::json(path, error))
}

fn find_manifest_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    push_file(&mut paths, root.join("manifest.json"));
    for child in child_directories(root)? {
        push_file(&mut paths, child.join("manifest.json"));
    }
    for pack_root in [root.join("behavior_packs"), root.join("resource_packs")] {
        if !pack_root.is_dir() {
            continue;
        }
        for child in child_directories(&pack_root)? {
            push_file(&mut paths, child.join("manifest.json"));
        }
    }
    paths.sort_by_key(|path| path_key(path));
    paths.dedup_by(|left, right| path_key(left) == path_key(right));
    Ok(paths)
}

fn push_file(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_file() {
        paths.push(path);
    }
}

fn read_manifest(path: &Path) -> Result<ManifestSummary> {
    let document = read_json(path)?;
    let header = document.get("header");
    let version = header
        .and_then(|header| header.get("version"))
        .and_then(version_array);
    let mut module_types = document
        .get("modules")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|module| module.get("type").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    module_types.sort();
    module_types.dedup();
    Ok(ManifestSummary {
        path: path.to_path_buf(),
        name: header
            .and_then(|header| header.get("name"))
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .map(ToOwned::to_owned),
        header_uuid: header
            .and_then(|header| header.get("uuid"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        version,
        module_types,
    })
}

fn version_array(value: &Value) -> Option<[u64; 3]> {
    let values = value.as_array()?;
    if values.len() != 3 {
        return None;
    }
    Some([
        values[0].as_u64()?,
        values[1].as_u64()?,
        values[2].as_u64()?,
    ])
}

fn kind_from_mcs_type(component_type: i64) -> Option<ComponentKind> {
    match component_type {
        1 => Some(ComponentKind::Map),
        3 | 4 => Some(ComponentKind::Material),
        7 => Some(ComponentKind::Addon),
        _ => None,
    }
}

fn infer_kind(root: &Path, manifests: &[ManifestSummary]) -> Option<ComponentKind> {
    if root.join("db").is_dir()
        || root.join("level.dat").is_file()
        || root.join("behavior_packs").is_dir()
        || root.join("resource_packs").is_dir()
    {
        return Some(ComponentKind::Map);
    }
    let root_manifest = manifests
        .iter()
        .find(|manifest| manifest.path.parent() == Some(root));
    if let Some(manifest) = root_manifest {
        if manifest
            .module_types
            .iter()
            .any(|module_type| module_type == "data" || module_type == "script")
        {
            return Some(ComponentKind::Addon);
        }
        if manifest
            .module_types
            .iter()
            .any(|module_type| module_type == "resources")
        {
            return Some(ComponentKind::Material);
        }
    }
    (!manifests.is_empty()).then_some(ComponentKind::Addon)
}

fn find_icon(root: &Path, manifests: &[ManifestSummary]) -> Option<PathBuf> {
    for name in [
        "pack_icon.png",
        "world_icon.jpeg",
        "world_icon.jpg",
        "world_icon.png",
    ] {
        let path = root.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    manifests.iter().find_map(|manifest| {
        let path = manifest.path.parent()?.join("pack_icon.png");
        path.is_file().then_some(path)
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

fn origin_kind(origin: &ComponentOrigin) -> SourceKind {
    match origin {
        ComponentOrigin::Mcs { .. } => SourceKind::McsAuto,
        ComponentOrigin::Single { .. } => SourceKind::Single,
        ComponentOrigin::Library { .. } => SourceKind::Library,
    }
}

fn mcs_source_id(path: &Path) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, path_key(path).as_bytes()).to_string()
}

fn warning_from_error(error: CoreError) -> DiscoveryWarning {
    DiscoveryWarning {
        path: error.path().unwrap_or_else(|| Path::new("")).to_path_buf(),
        message: error.to_string(),
    }
}

#[cfg(windows)]
fn automatic_mcs_work_roots() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;

    if std::env::var_os("MCDH_DISABLE_MCS_SCAN").is_some() {
        return Vec::new();
    }
    let mask = unsafe { GetLogicalDrives() };
    (0..26)
        .filter(|index| mask & (1 << index) != 0)
        .map(|index| {
            let drive = char::from(b'A' + index as u8);
            PathBuf::from(format!("{drive}:\\MCStudioDownload\\work"))
        })
        .filter(|path| path.is_dir())
        .collect()
}

#[cfg(not(windows))]
fn automatic_mcs_work_roots() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_json(path: &Path, value: Value) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }

    fn manifest(name: &str, module_type: &str) -> Value {
        serde_json::json!({
            "format_version": 2,
            "header": {
                "name": name,
                "uuid": Uuid::new_v4(),
                "version": [1, 2, 3]
            },
            "modules": [{
                "type": module_type,
                "uuid": Uuid::new_v4(),
                "version": [1, 2, 3]
            }]
        })
    }

    #[test]
    fn discovers_mcs_and_generic_components_without_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().join("state/mcdh.db");
        let index = LocalIndex::open(db).unwrap();
        let work = temp.path().join("MCStudioDownload/work");

        let addon = work.join("dev@example/Cpp/AddOn/addon-id");
        write_json(
            &addon.join("work.mcscfg"),
            serde_json::json!({"UID": "addon-id", "Type": 7, "Name": "测试模组"}),
        );
        write_json(
            &addon.join("behavior_pack_demo/manifest.json"),
            manifest("behavior", "data"),
        );
        write_json(
            &addon.join("resource_pack_demo/manifest.json"),
            manifest("resource", "resources"),
        );

        let material = temp.path().join("library/材质包");
        write_json(
            &material.join("manifest.json"),
            manifest("漂亮材质", "resources"),
        );
        index
            .add_source(SourceKind::Library, temp.path().join("library"))
            .unwrap();
        index.add_source(SourceKind::Single, &material).unwrap();

        let result = DiscoveryService::new(index)
            .refresh_with_mcs_work_roots(&[work])
            .unwrap();
        assert_eq!(result.components.len(), 2);
        assert_eq!(result.sources.len(), 3);
        let addon = result
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Addon)
            .unwrap();
        assert_eq!(addon.name, "测试模组");
        assert_eq!(
            addon.mcs.as_ref().unwrap().account.as_deref(),
            Some("dev@example")
        );
        let material = result
            .components
            .iter()
            .find(|component| component.kind == ComponentKind::Material)
            .unwrap();
        assert_eq!(material.name, "漂亮材质");
        assert_eq!(material.version, Some([1, 2, 3]));
    }

    #[test]
    fn recognizes_blank_mcs_maps_and_ignores_skin() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("mcdh.db")).unwrap();
        let work = temp.path().join("work");
        let map = work.join("account/Cpp/Map/map-id");
        write_json(
            &map.join("work.mcscfg"),
            serde_json::json!({"UID": "map-id", "Type": 1, "Name": "空白地图"}),
        );
        let skin = work.join("account/Cpp/Skin/skin-id");
        write_json(
            &skin.join("work.mcscfg"),
            serde_json::json!({"UID": "skin-id", "Type": 5, "Name": "皮肤"}),
        );

        let result = DiscoveryService::new(index)
            .refresh_with_mcs_work_roots(&[work])
            .unwrap();
        assert_eq!(result.components.len(), 1);
        assert_eq!(result.components[0].kind, ComponentKind::Map);
        assert_eq!(result.components[0].name, "空白地图");
    }

    #[test]
    fn recognizes_all_supported_mcs_and_generic_kinds_and_reports_bad_json() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("mcdh.db")).unwrap();
        let work = temp.path().join("work");
        for (category, uid, component_type) in [
            ("Map", "map-id", 1),
            ("Material", "material-id", 3),
            ("Light", "light-id", 4),
            ("AddOn", "addon-id", 7),
        ] {
            write_json(
                &work.join(format!("account/Cpp/{category}/{uid}/work.mcscfg")),
                serde_json::json!({"UID": uid, "Type": component_type, "Name": category}),
            );
        }

        let library = temp.path().join("中文组件库");
        write_json(
            &library.join("普通模组/behavior/manifest.json"),
            manifest("普通模组", "data"),
        );
        write_json(
            &library.join("普通材质/manifest.json"),
            manifest("普通材质", "resources"),
        );
        fs::create_dir_all(library.join("普通地图/db")).unwrap();
        fs::create_dir_all(library.join("损坏组件")).unwrap();
        fs::write(library.join("损坏组件/manifest.json"), b"{broken json").unwrap();
        index.add_source(SourceKind::Library, &library).unwrap();

        let result = DiscoveryService::new(index)
            .refresh_with_mcs_work_roots(&[work])
            .unwrap();
        assert_eq!(result.components.len(), 7);
        assert_eq!(
            result
                .components
                .iter()
                .filter(|component| component.kind == ComponentKind::Addon)
                .count(),
            2
        );
        assert_eq!(
            result
                .components
                .iter()
                .filter(|component| component.kind == ComponentKind::Material)
                .count(),
            3
        );
        assert_eq!(
            result
                .components
                .iter()
                .filter(|component| component.kind == ComponentKind::Map)
                .count(),
            2
        );
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("JSON"));
    }
}
