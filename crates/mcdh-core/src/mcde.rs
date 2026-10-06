//! MCDE installation validation and owned, non-destructive plugin launch profiles.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{CoreError, Result, mcdk::McdkStore};

pub const PLUGIN_ID: &str = "com.xiaobo.mcdeveditor";
pub const HOST_REVISION: &str = "da038f849a1c23211f5bb793a3450b579275308d";
pub const HOST_BRANCH: &str = "feature/plugin-system";
const INSTALLATION_KEY: &str = "mcde.installation";
const MAX_JSON_SIZE: u64 = 2 * 1024 * 1024;
const REQUIRED_IMAGES: &[&str] = &[
    "mcdeveditor.exe",
    "mcdeveditor-hook.dll",
    "mcdeveditor-plugin.dll",
    "Qt6Core.dll",
    "Qt6Gui.dll",
    "Qt6Widgets.dll",
    "Qt6OpenGL.dll",
    "Qt6OpenGLWidgets.dll",
    "Qt6Svg.dll",
    "platforms/qwindows.dll",
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Installation {
    directory: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McdeStatus {
    pub available: bool,
    pub directory: Option<PathBuf>,
    pub configured: bool,
    pub host_available: bool,
    pub host_revision: Option<String>,
    pub plugin_version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PluginHostManifest {
    pub schema: u32,
    pub repository: String,
    pub branch: String,
    pub revision: String,
    pub plugins_enabled: bool,
    pub abi: Abi,
    pub capabilities: Vec<String>,
    pub game_versions: Vec<String>,
    pub binary: BinaryDigest,
}

#[derive(Debug, Deserialize)]
pub struct Abi {
    pub major: u32,
    pub minor: u32,
}

#[derive(Debug, Deserialize)]
pub struct BinaryDigest {
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug)]
pub struct PreparedEditorLaunch {
    pub executable: PathBuf,
    pub version: String,
    profile: tempfile::TempDir,
}

impl PreparedEditorLaunch {
    pub fn directory(&self) -> &Path {
        self.profile.path()
    }

    // The host must keep its configuration even after MCDH exits.
    pub fn persist(self) -> PathBuf {
        self.profile.keep()
    }
}

#[derive(Clone)]
pub struct McdeStore {
    store: McdkStore,
}

impl McdeStore {
    pub fn new(store: McdkStore) -> Self {
        Self { store }
    }

    pub fn root(&self) -> PathBuf {
        self.store.root().with_file_name("mcde")
    }

    fn installation(&self) -> Result<Installation> {
        self.store.read(INSTALLATION_KEY)
    }

    pub fn set_directory(&self, directory: Option<&Path>) -> Result<()> {
        let directory = directory
            .map(|path| -> Result<PathBuf> {
                let path = crate::path_utils::canonicalize(path)?;
                verify_editor(&path)?;
                Ok(path)
            })
            .transpose()?;
        self.store
            .write(INSTALLATION_KEY, &Installation { directory })
    }

    pub fn status(&self, bundled_editor: &Path, host_directory: &Path) -> Result<McdeStatus> {
        let installation = self.installation()?;
        let configured = installation.directory.is_some();
        let directory = installation.directory.or_else(|| {
            bundled_editor
                .is_dir()
                .then(|| bundled_editor.to_path_buf())
        });
        let host = verify_host(host_directory);
        let editor = directory.as_deref().map(verify_editor).unwrap_or_else(|| {
            Err(invalid(
                "MCDE 未配置，请选择包含 plugin.json 的已编译插件目录",
            ))
        });
        let errors: Vec<String> = [
            host.as_ref().err().map(ToString::to_string),
            editor.as_ref().err().map(ToString::to_string),
        ]
        .into_iter()
        .flatten()
        .collect();
        Ok(McdeStatus {
            available: host.is_ok() && editor.is_ok(),
            directory,
            configured,
            host_available: host.is_ok(),
            host_revision: host.ok().map(|manifest| manifest.revision),
            plugin_version: editor.ok(),
            error: (!errors.is_empty()).then(|| errors.join("；")),
        })
    }

    pub fn prepare_launch(
        &self,
        project: &Path,
        bundled_editor: &Path,
        host_directory: &Path,
    ) -> Result<PreparedEditorLaunch> {
        let host = verify_host(host_directory)?;
        let installation = self.installation()?;
        let editor = crate::path_utils::canonicalize(
            installation.directory.as_deref().unwrap_or(bundled_editor),
        )?;
        verify_editor(&editor)?;
        let project = crate::path_utils::canonicalize(project)?;
        let config_path = project.join(".mcdev.json");
        if !config_path.is_file() {
            return Err(invalid(
                "项目缺少 .mcdev.json，请先使用普通启动完成 MCDK 游戏配置",
            ));
        }
        let source = read_bounded(&config_path, MAX_JSON_SIZE)?;
        let text =
            std::str::from_utf8(&source).map_err(|_| invalid(".mcdev.json 必须使用 UTF-8 编码"))?;
        let config: Value = crate::json::parse_jsonc(text, &config_path)?;
        let profiles = self.root().join("sessions");
        fs::create_dir_all(&profiles).map_err(|e| CoreError::io(&profiles, e))?;
        let profile = tempfile::Builder::new()
            .prefix("launch-")
            .tempdir_in(&profiles)
            .map_err(|e| CoreError::io(&profiles, e))?;
        let state_id = uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            project.to_string_lossy().to_lowercase().as_bytes(),
        );
        let state_directory = self.root().join("editor-state").join(state_id.to_string());
        let config = launch_config(
            config,
            &project,
            &editor,
            &state_directory,
            &host.game_versions,
        )?;
        write_json(&profile.path().join(".mcdev.json"), &config)?;
        write_json(
            &profile.path().join("source.json"),
            &json!({
                "project": project, "configuration": config_path,
                "source_sha256": format!("{:x}", Sha256::digest(&source)),
                "host_branch": host.branch, "host_revision": host.revision,
            }),
        )?;
        Ok(PreparedEditorLaunch {
            executable: host_directory.join("mcdk.exe"),
            version: format!("plugin-{}", &HOST_REVISION[..7]),
            profile,
        })
    }
}

pub fn verify_host(directory: &Path) -> Result<PluginHostManifest> {
    let manifest_path = directory.join("manifest.json");
    if !manifest_path.is_file() {
        return Err(invalid("插件版 MCDK 尚未准备，请运行 pnpm mcde:prepare"));
    }
    let manifest: PluginHostManifest =
        serde_json::from_slice(&read_bounded(&manifest_path, MAX_JSON_SIZE)?)
            .map_err(|e| CoreError::json(&manifest_path, e))?;
    if manifest.schema != 1
        || manifest.repository != "https://github.com/GitHub-Zero123/MCDevTool"
        || manifest.branch != HOST_BRANCH
        || manifest.revision != HOST_REVISION
        || !manifest.plugins_enabled
        || manifest.abi.major != 1
        || manifest.abi.minor != 0
        || !manifest
            .capabilities
            .iter()
            .any(|c| c == "mcdk.game.process.create")
        || manifest.game_versions != ["3.10.0.420447"]
        || manifest.binary.size < 256
        || manifest.binary.size > crate::mcdk::MAX_BINARY_SIZE
        || manifest.binary.sha256.len() != 64
        || !manifest
            .binary
            .sha256
            .bytes()
            .all(|c| c.is_ascii_hexdigit())
    {
        return Err(invalid(
            "MCDE 宿主清单不匹配，需要锁定的 feature/plugin-system 插件版 MCDK",
        ));
    }
    let path = directory.join("mcdk.exe");
    let bytes = read_bounded(&path, crate::mcdk::MAX_BINARY_SIZE)?;
    if bytes.len() as u64 != manifest.binary.size
        || format!("{:x}", Sha256::digest(&bytes)) != manifest.binary.sha256.to_ascii_lowercase()
    {
        return Err(invalid(
            "MCDE 插件版 MCDK 文件校验失败，请重新运行 mcde:prepare",
        ));
    }
    crate::mcdk::verify_pe(&bytes)?;
    verify_image(&directory.join("mcdev-tracy-bridge.dll"), true)?;
    Ok(manifest)
}

pub fn verify_editor(directory: &Path) -> Result<String> {
    let path = directory.join("plugin.json");
    let manifest: Value = serde_json::from_slice(&read_bounded(&path, MAX_JSON_SIZE)?)
        .map_err(|e| CoreError::json(&path, e))?;
    if manifest["schema"] != 1
        || manifest["id"] != PLUGIN_ID
        || manifest["abi"]["major"] != 1
        || manifest["abi"]["minor"] != 0
        || manifest["libraries"]["windows.x86_64"] != "mcdeveditor-plugin.dll"
        || manifest["entry_symbol"] != "mcdk_plugin_entry"
    {
        return Err(invalid(
            "选择的目录不是兼容的 MCDE Windows x64 插件（ABI 1.0）",
        ));
    }
    let version = manifest["version"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("MCDE 插件缺少版本号"))?;
    for name in REQUIRED_IMAGES {
        verify_image(&directory.join(name), name.ends_with(".dll"))?;
    }
    for name in ["assets", "signatures", "licenses"] {
        if !directory.join(name).is_dir() {
            return Err(invalid(format!("MCDE 运行资源缺失：{name}")));
        }
    }
    for name in [
        "assets/native_python.py",
        "assets/native_ui.py",
        "signatures/cpython-native.json",
    ] {
        let path = directory.join(name);
        if !fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() > 0) {
            return Err(invalid(format!("MCDE 原生桥接文件缺失或为空：{name}")));
        }
    }
    Ok(version.to_owned())
}

fn verify_image(path: &Path, dll: bool) -> Result<()> {
    let file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    let mut bytes = Vec::with_capacity(4096);
    file.take(4096)
        .read_to_end(&mut bytes)
        .map_err(|e| CoreError::io(path, e))?;
    let offset = bytes
        .get(0x3c..0x40)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let valid = offset
        .and_then(|p| p.checked_add(26).and_then(|end| bytes.get(p..end)))
        .is_some_and(|pe| {
            pe[..4] == *b"PE\0\0"
                && pe[4..6] == [0x64, 0x86]
                && pe[24..26] == [0x0b, 0x02]
                && pe[22] & 2 != 0
                && (pe[23] & 0x20 != 0) == dll
        });
    if bytes.get(..2) != Some(b"MZ") || !valid {
        return Err(invalid(format!(
            "MCDE 文件不是预期的 Windows x64 映像：{}",
            path.display()
        )));
    }
    Ok(())
}

fn launch_config(
    mut config: Value,
    project: &Path,
    editor: &Path,
    state: &Path,
    game_versions: &[String],
) -> Result<Value> {
    let root = config
        .as_object_mut()
        .ok_or_else(|| invalid(".mcdev.json 根节点必须是对象"))?;
    if root
        .get("reset_world")
        .is_some_and(|v| v != false && !v.is_null())
    {
        return Err(invalid(
            "MCDE 内嵌启动拒绝 reset_world，请先关闭世界重置选项",
        ));
    }
    let game = root
        .get("game_executable_path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("请先使用普通启动配置 game_executable_path"))?;
    let game = absolute(project, game)?;
    if !game.is_file()
        || !game.file_name().is_some_and(|s| {
            s.to_string_lossy()
                .eq_ignore_ascii_case("Minecraft.Windows.exe")
        })
        || !game
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|s| game_versions.iter().any(|v| s == v.as_str()))
    {
        return Err(invalid(
            "MCDE 当前仅验证网易游戏 3.10.0.420447，请检查 game_executable_path",
        ));
    }
    root.insert("game_executable_path".into(), json!(game));
    let packs = root
        .entry("included_mod_dirs")
        .or_insert_with(|| json!(["./"]));
    for pack in packs
        .as_array_mut()
        .ok_or_else(|| invalid("included_mod_dirs 必须是数组"))?
    {
        if let Some(path) = pack.as_str() {
            *pack = json!(absolute(project, path)?);
        } else if let Some(object) = pack.as_object_mut() {
            if object.get("path").is_some_and(|v| !v.is_string()) {
                return Err(invalid("资源包 path 必须是字符串"));
            }
            let path = object.get("path").and_then(Value::as_str).unwrap_or("./");
            object.insert("path".into(), json!(absolute(project, path)?));
        } else {
            return Err(invalid("included_mod_dirs 中存在无效条目"));
        }
    }
    let world = root
        .get("world_source_path")
        .cloned()
        .unwrap_or(json!("auto"));
    let world = match world.as_str() {
        Some("auto") => {
            if project.join("level.dat").is_file() {
                json!(project)
            } else {
                Value::Null
            }
        }
        Some("") => Value::Null,
        Some(path) => json!(absolute(project, path)?),
        None if world.is_null() => Value::Null,
        None => return Err(invalid("world_source_path 必须是路径字符串、auto 或 null")),
    };
    root.insert("world_source_path".into(), world);
    if let Some(skin) = root.get_mut("skin_info").and_then(Value::as_object_mut)
        && let Some(path) = skin
            .get("skin")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    {
        skin.insert("skin".into(), json!(absolute(project, path)?));
    }
    let plugins = root
        .entry("plugins")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| invalid("plugins 必须是数组"))?;
    let mut mcde_count = 0;
    for plugin in plugins.iter_mut() {
        let object = plugin
            .as_object_mut()
            .ok_or_else(|| invalid("plugins 中存在无效声明"))?;
        if object.get("id").and_then(Value::as_str) == Some(PLUGIN_ID) {
            mcde_count += 1;
            continue;
        }
        if object.get("enable").is_some_and(|v| v != false) {
            return Err(invalid(
                "MCDE 内嵌模式目前仅支持 MCDE 插件，其他已启用插件请使用普通启动",
            ));
        }
        let path = object
            .get("path")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("插件声明缺少 path"))?;
        object.insert("path".into(), json!(absolute(project, path)?));
    }
    if mcde_count > 1 {
        return Err(invalid("项目存在重复 MCDE 插件声明"));
    }
    plugins.retain(|p| p["id"] != PLUGIN_ID);
    plugins.push(json!({
        "enable": true, "id": PLUGIN_ID, "path": editor,
        "config": {"enabled": true, "width": 1440, "height": 900, "closeGameOnClose": true,
            "projectRoot": project, "stateDirectory": state}
    }));
    Ok(config)
}

fn absolute(project: &Path, path: &str) -> Result<PathBuf> {
    if path.contains(['\0', '"']) || path.is_empty() {
        return Err(invalid("启动配置包含无效路径"));
    }
    let path = Path::new(path);
    if !path.is_absolute()
        && (path.has_root()
            || matches!(
                path.components().next(),
                Some(std::path::Component::Prefix(_))
            ))
    {
        return Err(invalid("启动路径不能使用驱动器相对路径"));
    }
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        project.join(path)
    })
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    if file.metadata().map_err(|e| CoreError::io(path, e))?.len() > maximum {
        return Err(invalid(format!("文件超出读取限制：{}", path.display())));
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| CoreError::io(path, e))?;
    if bytes.len() as u64 > maximum {
        return Err(invalid("文件在读取期间超出限制"));
    }
    Ok(bytes)
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| CoreError::json(path, e))?;
    let mut file = File::create(path).map_err(|e| CoreError::io(path, e))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| CoreError::io(path, e))
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidInput(message.into())
}

#[cfg(test)]
#[path = "mcde_tests.rs"]
mod tests;
