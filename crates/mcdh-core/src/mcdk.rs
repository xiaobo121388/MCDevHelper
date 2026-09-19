//! Versioned MCDK storage. Hold the state lock when changing active versions.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

use fs2::FileExt;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CoreError, LocalIndex, Result};

pub const MAX_BINARY_SIZE: u64 = 64 * 1024 * 1024;
pub const RELEASE_API: &str =
    "https://api.github.com/repos/GitHub-Zero123/MCDevTool/releases/latest";
pub const DOWNLOAD_ROOT: &str = "https://github.com/GitHub-Zero123/MCDevTool/releases/download/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McdkVersion {
    pub version: String,
    pub tag: String,
    pub asset_name: String,
    pub size: u64,
    pub sha256: String,
}

impl McdkVersion {
    pub fn bundled() -> Self {
        serde_json::from_str(include_str!("../../../assets/mcdk/bundled.json"))
            .expect("invalid bundled MCDK manifest")
    }

    pub fn validate(&self) -> Result<Version> {
        let parsed = Version::parse(&self.version).map_err(|_| invalid("Invalid MCDK version"))?;
        if !parsed.pre.is_empty()
            || !parsed.build.is_empty()
            || self.version != parsed.to_string()
            || !(self.tag == self.version || self.tag == format!("v{}", self.version))
            || self.asset_name != "mcdk.exe"
            || self.size < 256
            || self.size > MAX_BINARY_SIZE
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("Invalid MCDK release manifest"));
        }
        Ok(parsed)
    }

    pub fn download_url(&self) -> Result<String> {
        self.validate()?;
        Ok(format!("{DOWNLOAD_ROOT}{}/mcdk.exe", self.tag))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McdkPreferences {
    pub auto_update: bool,
    pub generation: u64,
}

impl Default for McdkPreferences {
    fn default() -> Self {
        Self {
            auto_update: true,
            generation: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledVersions {
    pub current: Option<McdkVersion>,
    pub previous: Option<McdkVersion>,
}

#[derive(Clone)]
pub struct McdkStore {
    index: LocalIndex,
}

impl McdkStore {
    pub fn new(index: LocalIndex) -> Self {
        Self { index }
    }

    pub fn root(&self) -> PathBuf {
        self.index
            .path()
            .parent()
            .expect("database parent")
            .join("tools/mcdk")
    }

    pub fn lock(&self, name: &str) -> Result<File> {
        if !matches!(name, "state" | "update") {
            return Err(invalid("Invalid MCDK lock"));
        }
        let root = self.root();
        fs::create_dir_all(&root).map_err(|e| CoreError::io(&root, e))?;
        let path = root.join(format!("{name}.lock"));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| CoreError::io(&path, e))?;
        file.try_lock_exclusive().map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                CoreError::Busy
            } else {
                CoreError::io(&path, e)
            }
        })?;
        Ok(file)
    }

    pub fn read<T: serde::de::DeserializeOwned + Default>(&self, key: &str) -> Result<T> {
        match self.index.setting(key)? {
            Some(value) => serde_json::from_str(&value).map_err(|e| CoreError::json(key, e)),
            None => Ok(T::default()),
        }
    }

    pub fn write<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let json = serde_json::to_string(value).map_err(|e| CoreError::json(key, e))?;
        self.index.set_setting(key, &json)
    }

    pub fn preferences(&self) -> Result<McdkPreferences> {
        self.read("mcdk.preferences")
    }

    pub fn set_auto_update(&self, enabled: bool) -> Result<()> {
        let _lock = self.lock("state")?;
        let old = self.preferences()?;
        self.write(
            "mcdk.preferences",
            &McdkPreferences {
                auto_update: enabled,
                generation: old.generation.saturating_add(1),
            },
        )
    }

    pub fn automatic_allowed(&self, generation: u64) -> Result<bool> {
        let preferences = self.preferences()?;
        Ok(preferences.auto_update && preferences.generation == generation)
    }

    pub fn installed(&self) -> Result<InstalledVersions> {
        self.read("mcdk.installed")
    }

    pub fn executable(&self, version: &McdkVersion) -> Result<PathBuf> {
        version.validate()?;
        Ok(self
            .root()
            .join("versions")
            .join(&version.version)
            .join("mcdk.exe"))
    }

    // A failed copy never changes the active executable or its database pointer.
    pub fn stage(&self, version: &McdkVersion, source: &Path) -> Result<()> {
        verify_binary(source, version)?;
        let destination = self.executable(version)?;
        if destination.exists() {
            return verify_binary(&destination, version);
        }
        let versions = self.root().join("versions");
        fs::create_dir_all(&versions).map_err(|e| CoreError::io(&versions, e))?;
        let temp = tempfile::Builder::new()
            .prefix(".install-")
            .tempdir_in(&versions)
            .map_err(|e| CoreError::io(&versions, e))?;
        let binary = temp.path().join("mcdk.exe");
        fs::copy(source, &binary).map_err(|e| CoreError::io(&binary, e))?;
        verify_binary(&binary, version)?;
        OpenOptions::new()
            .write(true)
            .open(&binary)
            .and_then(|f| f.sync_all())
            .map_err(|e| CoreError::io(&binary, e))?;
        fs::rename(temp.path(), destination.parent().unwrap())
            .map_err(|e| CoreError::io(&destination, e))?;
        Ok(())
    }

    pub fn activate(&self, version: &McdkVersion) -> Result<()> {
        verify_binary(&self.executable(version)?, version)?;
        let mut installed = self.installed()?;
        if let Some(current) = &installed.current {
            if current.validate()? >= version.validate()? {
                return Ok(());
            }
        }
        installed.previous = installed.current.take();
        installed.current = Some(version.clone());
        self.write("mcdk.installed", &installed)
    }

    pub fn ensure_ready(&self, bundled_binary: &Path) -> Result<McdkVersion> {
        let mut installed = self.installed()?;
        for version in [installed.current.clone(), installed.previous.clone()]
            .into_iter()
            .flatten()
        {
            if self
                .executable(&version)
                .and_then(|p| verify_binary(&p, &version))
                .is_ok()
            {
                let version = version.clone();
                if installed.current.as_ref() != Some(&version) {
                    installed.current = Some(version.clone());
                    installed.previous = None;
                    self.write("mcdk.installed", &installed)?;
                }
                return Ok(version);
            }
        }
        let bundled = McdkVersion::bundled();
        // A damaged managed copy must not prevent using the read-only bundled fallback.
        if self.stage(&bundled, bundled_binary).is_err() {
            verify_binary(bundled_binary, &bundled)?;
            self.write(
                "mcdk.installed",
                &InstalledVersions {
                    current: Some(bundled.clone()),
                    previous: None,
                },
            )?;
            return Ok(bundled);
        }
        self.write(
            "mcdk.installed",
            &InstalledVersions {
                current: Some(bundled.clone()),
                previous: None,
            },
        )?;
        Ok(bundled)
    }

    pub fn ready_executable(&self, bundled_binary: &Path) -> Result<(McdkVersion, PathBuf)> {
        let version = self.ensure_ready(bundled_binary)?;
        let managed = self.executable(&version)?;
        if verify_binary(&managed, &version).is_ok() {
            return Ok((version, managed));
        }
        verify_binary(bundled_binary, &version)?;
        Ok((version, bundled_binary.to_path_buf()))
    }
}

pub fn verify_binary(path: &Path, version: &McdkVersion) -> Result<()> {
    version.validate()?;
    let file = File::open(path).map_err(|e| CoreError::io(path, e))?;
    if file.metadata().map_err(|e| CoreError::io(path, e))?.len() != version.size {
        return Err(invalid("MCDK binary size mismatch"));
    }
    let mut bytes = Vec::with_capacity(version.size as usize);
    file.take(MAX_BINARY_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| CoreError::io(path, e))?;
    if bytes.len() as u64 != version.size
        || format!("{:x}", Sha256::digest(&bytes)) != version.sha256.to_ascii_lowercase()
    {
        return Err(invalid("MCDK SHA-256 verification failed"));
    }
    verify_pe(&bytes)
}

fn verify_pe(bytes: &[u8]) -> Result<()> {
    let offset = bytes
        .get(0x3c..0x40)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    let valid = offset
        .and_then(|p| p.checked_add(26).map(|end| (p, end)))
        .and_then(|(p, end)| bytes.get(p..end))
        .is_some_and(|pe| {
            pe[0..4] == *b"PE\0\0"
                && pe[4..6] == [0x64, 0x86]
                && pe[24..26] == [0x0b, 0x02]
                && pe[22] & 2 != 0
                && pe[23] & 0x20 == 0
        });
    if bytes.get(..2) != Some(b"MZ") || !valid {
        return Err(invalid("MCDK must be a Windows x64 executable"));
    }
    Ok(())
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidInput(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path, version: &str) -> (McdkVersion, PathBuf) {
        let mut bytes = vec![0u8; 256];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c] = 64;
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&[0x64, 0x86]);
        bytes[86] = 2;
        bytes[88..90].copy_from_slice(&[0x0b, 0x02]);
        let path = root.join(format!("{version}.exe"));
        fs::write(&path, &bytes).unwrap();
        (
            McdkVersion {
                version: version.into(),
                tag: format!("v{version}"),
                asset_name: "mcdk.exe".into(),
                size: 256,
                sha256: format!("{:x}", Sha256::digest(bytes)),
            },
            path,
        )
    }

    #[test]
    fn validates_release_identity_and_architecture() {
        McdkVersion::bundled().validate().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let (mut version, path) = fixture(temp.path(), "1.0.0");
        verify_binary(&path, &version).unwrap();
        version.tag = "../bad".into();
        assert!(version.download_url().is_err());
        version.tag = "v1.0.0".into();
        version.sha256 = "0".repeat(64);
        assert!(verify_binary(&path, &version).is_err());
        assert!(verify_pe(&[0u8; 256]).is_err());
    }

    #[test]
    fn stages_atomically_preserves_previous_and_recovers_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let store = McdkStore::new(LocalIndex::open(temp.path().join("db")).unwrap());
        let (first, first_path) = fixture(temp.path(), "1.0.0");
        let (second, second_path) = fixture(temp.path(), "2.0.0");
        let _lock = store.lock("state").unwrap();
        assert!(store.lock("state").is_err());
        store.stage(&first, &first_path).unwrap();
        store.activate(&first).unwrap();
        store.stage(&second, &second_path).unwrap();
        assert_eq!(store.installed().unwrap().current, Some(first.clone()));
        store.activate(&second).unwrap();
        store.activate(&first).unwrap();
        assert_eq!(store.installed().unwrap().current, Some(second.clone()));
        fs::write(store.executable(&second).unwrap(), b"broken").unwrap();
        assert_eq!(store.ensure_ready(Path::new("missing")).unwrap(), first);
    }

    #[test]
    fn preferences_survive_old_clients_and_cancel_old_generations() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("db")).unwrap();
        let store = McdkStore::new(index.clone());
        assert!(store.automatic_allowed(0).unwrap());
        store.set_auto_update(false).unwrap();
        index
            .set_app_settings(&crate::AppSettings::default())
            .unwrap();
        assert!(!store.preferences().unwrap().auto_update);
        store.set_auto_update(true).unwrap();
        assert!(!store.automatic_allowed(0).unwrap());
        assert!(store.automatic_allowed(2).unwrap());
    }
}
