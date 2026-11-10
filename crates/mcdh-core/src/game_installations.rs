//! Local game discovery uses the MCDevTool MCStudio directory convention.
use crate::{LocalIndex, Result, mcdk::McdkStore, mcdk_config::invalid};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
pub struct GameInstallation {
    pub path: PathBuf,
    pub version: Option<String>,
    pub source: &'static str,
}

#[derive(Debug, Default, Serialize)]
pub struct GameCatalog {
    pub installations: Vec<GameInstallation>,
    pub warnings: Vec<String>,
    pub selected_path: Option<PathBuf>,
}

pub fn discover(index: &LocalIndex, component_id: &str, configured: &str) -> Result<GameCatalog> {
    let root = McdkStore::new(index.clone())
        .launch_target(component_id)?
        .path;
    if configured.len() > 32768 || configured.contains('\0') {
        return Err(invalid("游戏路径无效"));
    }
    let selected = (!configured.trim().is_empty()).then(|| root.join(configured.trim()));
    Ok(scan(&automatic_roots(), selected.as_deref()))
}

fn version_parts(value: &str) -> Option<Vec<u64>> {
    let parts = value
        .split('.')
        .map(|v| {
            if v.is_empty() || !v.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            v.parse::<u64>().ok()
        })
        .collect::<Option<Vec<_>>>()?;
    (3..=6).contains(&parts.len()).then_some(parts)
}

fn executable_version(path: &Path) -> Option<String> {
    let name = path.parent()?.file_name()?.to_str()?;
    version_parts(name).map(|_| name.to_owned())
}

fn scan(roots: &[PathBuf], configured: Option<&Path>) -> GameCatalog {
    let mut result = GameCatalog::default();
    let mut roots = roots
        .iter()
        .map(|p| (p.clone(), "mcstudio"))
        .collect::<Vec<_>>();
    if let Some(path) = configured {
        if executable_version(path).is_some() {
            if let Some(parent) = path.parent().and_then(Path::parent) {
                roots.push((parent.into(), "sibling"));
            }
        }
    }
    let mut visited = HashSet::new();
    let mut seen = HashSet::new();
    for (root, source) in roots {
        let Ok(root) = crate::canonicalize_path(&root) else {
            continue;
        };
        if !visited.insert(key(&root)) {
            continue;
        }
        let entries = match fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(_) => {
                result
                    .warnings
                    .push(format!("无法读取游戏目录：{}", root.display()));
                continue;
            }
        };
        for (index, entry) in entries.enumerate() {
            if index >= 1024 {
                result.warnings.push(format!(
                    "游戏目录条目超过 1024，已限制扫描：{}",
                    root.display()
                ));
                break;
            }
            let Ok(entry) = entry else {
                continue;
            };
            if !entry
                .file_type()
                .is_ok_and(|t| t.is_dir() && !t.is_symlink())
            {
                continue;
            }
            if version_parts(&entry.file_name().to_string_lossy()).is_none() {
                continue;
            }
            insert(
                &mut result,
                &mut seen,
                &entry.path().join("Minecraft.Windows.exe"),
                source,
            );
        }
    }
    if let Some(path) = configured {
        insert(&mut result, &mut seen, path, "configured");
        result.selected_path = crate::canonicalize_path(path)
            .ok()
            .filter(|p| seen.contains(&key(p)));
    }
    result.installations.sort_by(|a, b| {
        let parts = |v: &Option<String>| {
            v.as_deref().and_then(version_parts).map(|mut p| {
                p.resize(6, 0);
                p
            })
        };
        parts(&b.version)
            .cmp(&parts(&a.version))
            .then_with(|| key(&a.path).cmp(&key(&b.path)))
    });
    result
}

fn insert(result: &mut GameCatalog, seen: &mut HashSet<String>, path: &Path, source: &'static str) {
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return;
    }
    let Ok(real) = crate::canonicalize_path(path) else {
        return;
    };
    if seen.insert(key(&real)) {
        result.installations.push(GameInstallation {
            version: executable_version(path),
            path: real,
            source,
        });
    }
}

fn key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_lowercase()
}

#[cfg(windows)]
fn automatic_roots() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    if std::env::var_os("MCDH_DISABLE_MCS_SCAN").is_some() {
        return vec![];
    }
    let mask = unsafe { GetLogicalDrives() };
    (0..26)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| {
            let root = format!("{}:\\", char::from(b'A' + i as u8));
            let wide = root.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            // DRIVE_FIXED: do not block discovery on unavailable network/removable volumes.
            (unsafe { GetDriveTypeW(wide.as_ptr()) } == 3)
                .then(|| PathBuf::from(root).join("MCStudioDownload/game/MinecraftPE_Netease"))
        })
        .collect()
}

#[cfg(not(windows))]
fn automatic_roots() -> Vec<PathBuf> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(root: &Path, version: &str) -> PathBuf {
        let file = root.join(version).join("Minecraft.Windows.exe");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"fixture").unwrap();
        file
    }

    #[test]
    fn versions_are_numeric_sorted_deduplicated_and_non_recursive() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let current = game(root, "3.9.0.1");
        game(root, "3.10.0.2");
        game(root, "3.10.0.12");
        game(root, "PCLauncher_x64");
        game(&root.join("nested"), "9.0.0");
        fs::create_dir(root.join("99.0.0")).unwrap();
        let result = scan(&[root.into(), root.into()], Some(&current));
        assert_eq!(
            result.selected_path,
            Some(crate::canonicalize_path(&current).unwrap())
        );
        assert_eq!(
            result
                .installations
                .iter()
                .map(|v| v.version.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("3.10.0.12"), Some("3.10.0.2"), Some("3.9.0.1")]
        );
        assert!(result.installations.iter().all(|v| v.source == "mcstudio"));
    }

    #[test]
    fn custom_paths_and_siblings_are_supported_without_installing_or_running_anything() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let current = game(root, "3.10.0.1");
        game(root, "3.11.0.1");
        assert_eq!(scan(&[], Some(&current)).installations.len(), 2);
        let custom = root.join("custom.exe");
        fs::write(&custom, []).unwrap();
        let result = scan(&[], Some(&custom));
        assert_eq!(result.installations.len(), 1);
        assert_eq!(result.installations[0].source, "configured");
        assert!(result.installations[0].version.is_none());
        assert!(
            scan(&[], Some(&root.join("missing.exe")))
                .installations
                .is_empty()
        );
    }

    #[test]
    fn invalid_versions_are_rejected() {
        for value in [
            "3.10",
            "-3.10.0",
            "3.10.0-beta",
            "3..0.0",
            "18446744073709551616.1.0",
        ] {
            assert!(version_parts(value).is_none());
        }
    }
}
