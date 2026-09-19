use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ComponentKind, CoreError, LocalIndex, Result};

const SETTINGS_KEY: &str = "custom_export_profiles";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    #[default]
    Snapshot,
    Source,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogEncoding {
    #[default]
    Utf8,
    Gb18030,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CustomExportProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub input_mode: InputMode,
    pub working_directory: Option<PathBuf>,
    pub component_kinds: Vec<ComponentKind>,
    pub timeout_seconds: u64,
    pub log_encoding: LogEncoding,
    pub allow_mcp: bool,
}

impl Default for CustomExportProfile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            executable: PathBuf::new(),
            arguments: Vec::new(),
            input_mode: InputMode::Snapshot,
            working_directory: None,
            component_kinds: vec![
                ComponentKind::Addon,
                ComponentKind::Map,
                ComponentKind::Material,
            ],
            timeout_seconds: 1800,
            log_encoding: LogEncoding::Utf8,
            allow_mcp: false,
        }
    }
}

impl CustomExportProfile {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.chars().count() > 60 {
            return Err(CoreError::InvalidInput(
                "方案名称须为 1 到 60 个字符".into(),
            ));
        }
        if !self.executable.is_absolute()
            || self.executable.to_string_lossy().contains('\0')
            || !self
                .executable
                .extension()
                .is_some_and(|v| v.eq_ignore_ascii_case("exe"))
        {
            return Err(CoreError::InvalidInput(
                "请选择 EXE 绝对路径；脚本须通过解释器运行".into(),
            ));
        }
        if self
            .working_directory
            .as_ref()
            .is_some_and(|path| !path.is_absolute() || path.to_string_lossy().contains('\0'))
        {
            return Err(CoreError::InvalidInput("工作目录必须为绝对路径".into()));
        }
        if self.arguments.iter().any(|arg| arg.contains('\0')) {
            return Err(CoreError::InvalidInput("参数不能包含空字符".into()));
        }
        if self.timeout_seconds == 0 || self.timeout_seconds > u32::MAX as u64 {
            return Err(CoreError::InvalidInput(
                "超时秒数必须为正整数且不超过 4294967295".into(),
            ));
        }
        if self.component_kinds.is_empty() {
            return Err(CoreError::InvalidInput("至少选择一种适用组件类型".into()));
        }
        Ok(())
    }

    fn same_execution(&self, other: &Self) -> bool {
        self.executable == other.executable
            && self.arguments == other.arguments
            && self.input_mode == other.input_mode
            && self.working_directory == other.working_directory
    }
}

#[derive(Serialize, Deserialize)]
struct StoredProfiles {
    schema_version: u32,
    profiles: Vec<CustomExportProfile>,
}

#[derive(Clone)]
pub struct ProfileStore(LocalIndex);

impl ProfileStore {
    pub fn new(index: LocalIndex) -> Self {
        Self(index)
    }

    pub fn list(&self) -> Result<Vec<CustomExportProfile>> {
        let Some(text) = self.0.setting(SETTINGS_KEY)? else {
            return Ok(Vec::new());
        };
        let stored: StoredProfiles = crate::json::parse_jsonc(&text, SETTINGS_KEY)?;
        if stored.schema_version != 1 {
            return Err(CoreError::InvalidInput("不支持的自定义导出配置版本".into()));
        }
        Ok(stored.profiles)
    }

    // Only desktop adapters expose this operation; MCP can never write profiles.
    pub fn save(&self, mut profiles: Vec<CustomExportProfile>) -> Result<Vec<CustomExportProfile>> {
        let _guard = self.0.try_lock_mutations()?;
        let previous = self.list()?;
        let mut ids = HashSet::new();
        for profile in &mut profiles {
            if profile.id.is_empty() {
                profile.id = Uuid::new_v4().to_string();
            }
            if !ids.insert(profile.id.clone()) || Uuid::parse_str(&profile.id).is_err() {
                return Err(CoreError::InvalidInput("方案 ID 无效或重复".into()));
            }
            profile.name = profile.name.trim().to_owned();
            profile.validate()?;
            let old = previous.iter().find(|old| old.id == profile.id);
            if !old.is_some_and(|old| profile.same_execution(old)) {
                profile.allow_mcp = false;
            }
        }
        let text = serde_json::to_string(&StoredProfiles {
            schema_version: 1,
            profiles: profiles.clone(),
        })
        .map_err(|error| CoreError::json(SETTINGS_KEY, error))?;
        self.0.set_setting(SETTINGS_KEY, &text)?;
        Ok(profiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> CustomExportProfile {
        CustomExportProfile {
            name: "Custom".into(),
            executable: std::env::current_exe()
                .unwrap()
                .with_file_name("packer.exe"),
            ..Default::default()
        }
    }

    #[test]
    fn profiles_are_independent_ordered_and_revoke_execution_changes() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("db")).unwrap();
        let store = ProfileStore::new(index.clone());
        assert!(store.list().unwrap().is_empty());
        let mut profiles = store.save(vec![profile(), profile()]).unwrap();
        assert_ne!(profiles[0].id, profiles[1].id);
        profiles[0].allow_mcp = true;
        profiles = store.save(profiles).unwrap();
        assert!(profiles[0].allow_mcp);
        profiles[0].name = "Renamed".into();
        profiles = store.save(profiles).unwrap();
        assert!(profiles[0].allow_mcp);
        profiles[0].arguments.push("--new".into());
        profiles = store.save(profiles).unwrap();
        assert!(!profiles[0].allow_mcp);
        profiles.swap(0, 1);
        store.save(profiles.clone()).unwrap();
        index
            .set_app_settings(&crate::AppSettings::default())
            .unwrap();
        assert_eq!(store.list().unwrap(), profiles);
        store.save(vec![]).unwrap();
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn rejects_invalid_configuration_and_preserves_corrupt_storage() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("db")).unwrap();
        let store = ProfileStore::new(index.clone());
        let mut invalid = profile();
        invalid.executable = PathBuf::from("pack.cmd");
        assert!(store.save(vec![invalid]).is_err());
        invalid = profile();
        invalid.timeout_seconds = 0;
        assert!(invalid.validate().is_err());
        index
            .set_setting(SETTINGS_KEY, r#"{"schema_version":2,"profiles":[]}"#)
            .unwrap();
        assert!(store.save(vec![]).is_err());
        assert!(index.setting(SETTINGS_KEY).unwrap().unwrap().contains(":2"));
    }
}
