use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::json::parse_jsonc;
use crate::{ComponentKind, CoreError, McsTemplateIdentity, Result};

const BEHAVIOR_MANIFEST: &str = include_str!("../templates/behavior_manifest.json");
const RESOURCE_MANIFEST: &str = include_str!("../templates/resource_manifest.json");
const STUDIO_CONFIG: &str = include_str!("../templates/studio.json");
const WORK_CONFIG: &str = include_str!("../templates/work.mcscfg");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateRequest {
    pub name: String,
    pub kind: ComponentKind,
    pub destination: PathBuf,
    pub mcs_compatible: bool,
    pub component_uid: Option<String>,
    pub mcs_identity: McsTemplateIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedFile {
    pub relative_path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedTemplate {
    pub component_uid: String,
    pub behavior_pack_directory: Option<String>,
    pub resource_pack_directory: Option<String>,
    pub directories: Vec<PathBuf>,
    pub files: Vec<RenderedFile>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TemplateService;

impl TemplateService {
    pub fn render(&self, request: &TemplateRequest) -> Result<RenderedTemplate> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(CoreError::InvalidInput("组件名称不能为空".into()));
        }
        if !request.destination.is_absolute() {
            return Err(CoreError::InvalidInput("目标路径必须是绝对路径".into()));
        }

        let component_uid = request
            .component_uid
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
        let suffix = &Uuid::new_v4().simple().to_string()[..8];
        let behavior_directory = format!("behavior_pack_{suffix}");
        let resource_directory = format!("resource_pack_{suffix}");
        let behavior_manifest = render_manifest(BEHAVIOR_MANIFEST, name, true)?;
        let resource_manifest = render_manifest(RESOURCE_MANIFEST, name, false)?;

        let (behavior_pack_directory, resource_pack_directory, mut directories, mut files) =
            match request.kind {
                ComponentKind::Addon => {
                    let directories = vec![
                        PathBuf::from(&behavior_directory).join("entities"),
                        PathBuf::from(&resource_directory).join("textures"),
                    ];
                    let files = vec![
                        RenderedFile {
                            relative_path: PathBuf::from(&behavior_directory).join("manifest.json"),
                            content: behavior_manifest,
                        },
                        RenderedFile {
                            relative_path: PathBuf::from(&resource_directory).join("manifest.json"),
                            content: resource_manifest,
                        },
                    ];
                    (
                        Some(behavior_directory),
                        Some(resource_directory),
                        directories,
                        files,
                    )
                }
                ComponentKind::Map => {
                    let behavior_directory =
                        PathBuf::from("behavior_packs").join(&behavior_directory);
                    let resource_directory =
                        PathBuf::from("resource_packs").join(&resource_directory);
                    let directories = vec![
                        behavior_directory.join("entities"),
                        resource_directory.join("textures"),
                    ];
                    let files = vec![
                        RenderedFile {
                            relative_path: behavior_directory.join("manifest.json"),
                            content: behavior_manifest,
                        },
                        RenderedFile {
                            relative_path: resource_directory.join("manifest.json"),
                            content: resource_manifest,
                        },
                    ];
                    (
                        Some(path_text(&behavior_directory)),
                        Some(path_text(&resource_directory)),
                        directories,
                        files,
                    )
                }
                ComponentKind::Material => (
                    None,
                    None,
                    vec![PathBuf::from("textures")],
                    vec![RenderedFile {
                        relative_path: PathBuf::from("manifest.json"),
                        content: resource_manifest,
                    }],
                ),
            };

        if request.mcs_compatible {
            validate_namespace(&request.mcs_identity.namespace)?;
            let target = request.destination.join(&component_uid);
            let (mcs_type, edit_type, is_map, is_pc) = match request.kind {
                ComponentKind::Map => (1, 1, true, true),
                ComponentKind::Material => (3, 0, false, false),
                ComponentKind::Addon => (7, 2, false, true),
            };
            let mut replacements = HashMap::new();
            replacements.insert("{{mcs_uid}}", Value::String(component_uid.clone()));
            replacements.insert("{{component_name}}", Value::String(name.to_owned()));
            replacements.insert(
                "{{developer_account}}",
                Value::String(request.mcs_identity.developer_account.clone()),
            );
            replacements.insert(
                "{{developer_nickname}}",
                Value::String(request.mcs_identity.developer_nickname.clone()),
            );
            replacements.insert(
                "{{developer_user_id}}",
                Value::String(request.mcs_identity.developer_user_id.clone()),
            );
            replacements.insert(
                "{{namespace}}",
                Value::String(request.mcs_identity.namespace.clone()),
            );
            replacements.insert("{{mcs_type}}", Value::from(mcs_type));
            replacements.insert("{{edit_type}}", Value::from(edit_type));
            replacements.insert("{{is_map}}", Value::from(is_map));
            replacements.insert("{{is_pc}}", Value::from(is_pc));
            replacements.insert("{{update_time}}", Value::String(Local::now().to_rfc3339()));
            replacements.insert(
                "{{save_back_map_path}}",
                if request.kind == ComponentKind::Map {
                    Value::String(path_text(&target))
                } else {
                    Value::Null
                },
            );
            replacements.insert(
                "{{save_back_addon_path}}",
                if request.kind == ComponentKind::Addon {
                    Value::String(path_text(&target))
                } else {
                    Value::Null
                },
            );
            files.push(RenderedFile {
                relative_path: PathBuf::from("studio.json"),
                content: render_json(STUDIO_CONFIG, &replacements)?,
            });
            files.push(RenderedFile {
                relative_path: PathBuf::from("work.mcscfg"),
                content: render_json(WORK_CONFIG, &replacements)?,
            });
        }

        directories.sort();
        files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Ok(RenderedTemplate {
            component_uid,
            behavior_pack_directory,
            resource_pack_directory,
            directories,
            files,
        })
    }
}

fn validate_namespace(namespace: &str) -> Result<()> {
    let valid = !namespace.is_empty()
        && namespace.len() <= 64
        && namespace.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && namespace
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase());
    if valid {
        Ok(())
    } else {
        Err(CoreError::InvalidInput(
            "命名空间必须以小写字母开头，且只能包含小写字母、数字和下划线（最多 64 个字符）".into(),
        ))
    }
}

fn render_manifest(template: &str, component_name: &str, behavior: bool) -> Result<String> {
    let mut replacements = HashMap::new();
    if behavior {
        replacements.insert(
            "{{behavior_name}}",
            Value::String(format!("{component_name} 行为包")),
        );
        replacements.insert(
            "{{behavior_header_uuid}}",
            Value::String(Uuid::new_v4().to_string()),
        );
        replacements.insert(
            "{{behavior_module_uuid}}",
            Value::String(Uuid::new_v4().to_string()),
        );
    } else {
        replacements.insert(
            "{{resource_name}}",
            Value::String(format!("{component_name} 资源包")),
        );
        replacements.insert(
            "{{resource_header_uuid}}",
            Value::String(Uuid::new_v4().to_string()),
        );
        replacements.insert(
            "{{resource_module_uuid}}",
            Value::String(Uuid::new_v4().to_string()),
        );
    }
    render_json(template, &replacements)
}

fn render_json(template: &str, replacements: &HashMap<&str, Value>) -> Result<String> {
    let mut document: Value = parse_jsonc(template, "embedded-template")?;
    replace_value(&mut document, replacements);
    serde_json::to_string_pretty(&document)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| CoreError::json(Path::new("rendered-template"), error))
}

fn replace_value(value: &mut Value, replacements: &HashMap<&str, Value>) {
    match value {
        Value::String(text) => {
            if let Some(replacement) = replacements.get(text.as_str()) {
                *value = (*replacement).clone();
                return;
            }
            for (placeholder, replacement) in replacements {
                if let Some(replacement) = replacement.as_str() {
                    *text = text.replace(placeholder, replacement);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_value(value, replacements);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                replace_value(value, replacements);
            }
        }
        _ => {}
    }
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const TEMPLATE_SOURCES: [&str; 4] = [
        BEHAVIOR_MANIFEST,
        RESOURCE_MANIFEST,
        STUDIO_CONFIG,
        WORK_CONFIG,
    ];

    #[test]
    fn embedded_templates_do_not_contain_personal_data() {
        for template in TEMPLATE_SOURCES {
            assert!(!template.contains("467359395"));
            assert!(!template.contains("小波"));
            assert!(!template.contains(":\\MCStudioDownload"));
            assert!(!template.contains(":\\mcStudio"));
            serde_json::from_str::<Value>(template).unwrap();
        }
    }

    #[test]
    fn renders_all_component_kinds_with_unique_identifiers() {
        let service = TemplateService;
        let destination = PathBuf::from(r"D:\MCS\work\account\Cpp\AddOn");
        let mut component_uids = HashSet::new();
        let mut manifest_uuids = HashSet::new();

        for kind in [
            ComponentKind::Addon,
            ComponentKind::Map,
            ComponentKind::Material,
        ] {
            let rendered = service
                .render(&TemplateRequest {
                    name: "测试作品".into(),
                    kind,
                    destination: destination.clone(),
                    mcs_compatible: true,
                    component_uid: None,
                    mcs_identity: McsTemplateIdentity::default(),
                })
                .unwrap();
            assert!(component_uids.insert(rendered.component_uid.clone()));
            assert!(
                rendered
                    .files
                    .iter()
                    .any(|file| file.relative_path == Path::new("studio.json"))
            );
            assert!(
                rendered
                    .files
                    .iter()
                    .any(|file| file.relative_path == Path::new("work.mcscfg"))
            );
            for file in rendered
                .files
                .iter()
                .filter(|file| file.relative_path.ends_with("manifest.json"))
            {
                let document: Value = serde_json::from_str(&file.content).unwrap();
                let uuid = document["header"]["uuid"].as_str().unwrap().to_owned();
                assert!(manifest_uuids.insert(uuid));
            }
        }
    }

    #[test]
    fn ordinary_templates_omit_mcs_configuration() {
        let rendered = TemplateService
            .render(&TemplateRequest {
                name: "普通材质".into(),
                kind: ComponentKind::Material,
                destination: PathBuf::from(r"D:\Projects"),
                mcs_compatible: false,
                component_uid: None,
                mcs_identity: McsTemplateIdentity::default(),
            })
            .unwrap();
        assert_eq!(rendered.files.len(), 1);
        assert_eq!(rendered.files[0].relative_path, Path::new("manifest.json"));
    }

    #[test]
    fn renders_configured_developer_identity_and_namespace() {
        let rendered = TemplateService
            .render(&TemplateRequest {
                name: "身份测试".into(),
                kind: ComponentKind::Addon,
                destination: PathBuf::from(r"D:\MCS\AddOn"),
                mcs_compatible: true,
                component_uid: Some("testuid".into()),
                mcs_identity: McsTemplateIdentity {
                    developer_nickname: "本地开发者".into(),
                    developer_account: "local@example.invalid".into(),
                    developer_user_id: "123".into(),
                    namespace: "local_dev".into(),
                },
            })
            .unwrap();
        let studio = rendered
            .files
            .iter()
            .find(|file| file.relative_path == Path::new("studio.json"))
            .unwrap();
        let studio: Value = serde_json::from_str(&studio.content).unwrap();
        assert_eq!(studio["Account"], "local@example.invalid");
        assert_eq!(studio["UserName"], "本地开发者");
        assert_eq!(studio["UserId"], "123");
        assert_eq!(studio["NameSpace"], "local_dev");
    }
}
