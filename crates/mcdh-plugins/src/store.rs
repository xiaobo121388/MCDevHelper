use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use fs2::FileExt;
use mcdh_core::LocalIndex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tempfile::TempDir;

use crate::{
    PluginError, PluginManifest, Result, archive,
    error::invalid,
    github::{Cancellation, DownloadSource, PluginProgress, Repository},
    origin::{ManagedOrigin, PluginOrigin},
};

const REGISTRY: &str = "plugins.registry.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub repository: PluginOrigin,
    pub source: DownloadSource,
    pub fingerprint: String,
    pub files: BTreeMap<String, String>,
    pub enabled: bool,
    pub trusted_fingerprint: Option<String>,
    pub mcp_fingerprint: Option<String>,
    pub settings: Value,
    pub last_error: Option<PluginError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginSummary {
    pub manifest: PluginManifest,
    pub repository: String,
    pub source: DownloadSource,
    pub fingerprint: String,
    pub enabled: bool,
    pub trusted: bool,
    pub allow_mcp: bool,
    pub compatible: bool,
    pub settings: Value,
    pub last_error: Option<PluginError>,
}

impl From<InstalledPlugin> for PluginSummary {
    fn from(plugin: InstalledPlugin) -> Self {
        Self {
            compatible: plugin.manifest.compatible(),
            trusted: plugin.trusted_fingerprint.as_ref() == Some(&plugin.fingerprint),
            allow_mcp: plugin.mcp_fingerprint.as_ref() == Some(&plugin.fingerprint),
            manifest: plugin.manifest,
            repository: plugin.repository.to_string(),
            source: plugin.source,
            fingerprint: plugin.fingerprint,
            enabled: plugin.enabled,
            settings: plugin.settings,
            last_error: plugin.last_error,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginCandidate {
    pub token: String,
    pub manifest: PluginManifest,
    pub repository: String,
    pub source: DownloadSource,
    pub fingerprint: String,
    pub compatible: bool,
    pub already_installed: bool,
}

struct PreparedPlugin {
    candidate: PluginCandidate,
    record: InstalledPlugin,
    _temporary: TempDir,
    root: PathBuf,
    created: Instant,
}

#[derive(Serialize, Deserialize)]
struct RetainedSettings {
    repository: PluginOrigin,
    settings: Value,
}

#[derive(Clone)]
pub struct PluginService {
    pub(crate) index: LocalIndex,
    pub(crate) root: PathBuf,
    prepared: Arc<Mutex<HashMap<String, PreparedPlugin>>>,
}

pub(crate) struct Lease(File);
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.0);
    }
}

impl PluginService {
    pub fn new(index: LocalIndex) -> Result<Self> {
        let root = index
            .path()
            .parent()
            .ok_or_else(|| invalid("Missing application data directory"))?
            .join("plugins");
        fs::create_dir_all(&root)?;
        Ok(Self {
            index,
            root: mcdh_core::canonicalize_path(&root)?,
            prepared: Arc::default(),
        })
    }

    pub(crate) fn registry(&self) -> Result<BTreeMap<String, InstalledPlugin>> {
        self.index
            .setting(REGISTRY)?
            .map(|text| serde_json::from_str(&text).map_err(Into::into))
            .unwrap_or_else(|| Ok(BTreeMap::new()))
    }

    pub(crate) fn save_registry(&self, records: &BTreeMap<String, InstalledPlugin>) -> Result<()> {
        self.index
            .set_setting(REGISTRY, &serde_json::to_string(records)?)?;
        Ok(())
    }

    pub fn revision(&self) -> Result<String> {
        Ok(self.index.setting(REGISTRY)?.unwrap_or_default())
    }

    pub fn list(&self) -> Result<Vec<PluginSummary>> {
        Ok(self.registry()?.into_values().map(Into::into).collect())
    }

    pub(crate) fn lease(&self, id: &str, exclusive: bool) -> Result<Lease> {
        if id != "registry" && !crate::manifest::identifier(id) {
            return Err(invalid("Invalid plugin id"));
        }
        let directory = self.root.join("locks");
        fs::create_dir_all(&directory)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join(format!("{id}.lock")))?;
        let result = if exclusive {
            FileExt::try_lock_exclusive(&file)
        } else {
            FileExt::try_lock_shared(&file)
        };
        result.map_err(|_| {
            PluginError::new(
                "plugin_busy",
                "Plugin is executing or being modified; finish or cancel its tasks first",
            )
        })?;
        Ok(Lease(file))
    }

    pub async fn preview(
        &self,
        source: &str,
        cancel: &Cancellation,
        progress: impl Fn(PluginProgress) + Send + Sync,
    ) -> Result<PluginCandidate> {
        let repository = Repository::parse(source)?;
        let temporary = tempfile::Builder::new()
            .prefix("preview-")
            .tempdir_in(&self.root)?;
        let archive_path = temporary.path().join("download.zip");
        let origin = crate::github::download(&repository, &archive_path, cancel, &progress).await?;
        cancel.check()?;
        self.prepare(repository, origin, &archive_path, temporary)
    }

    pub fn preview_archive(&self, source: &str, path: &Path) -> Result<PluginCandidate> {
        if fs::metadata(path)?.len() > archive::MAX_DOWNLOAD {
            return Err(invalid("Plugin download exceeds 128 MiB"));
        }
        let repository = Repository::parse(source)?;
        let temporary = tempfile::Builder::new()
            .prefix("preview-")
            .tempdir_in(&self.root)?;
        let origin = DownloadSource {
            kind: "local".into(),
            reference: "local archive".into(),
            archive_sha256: archive::hash_file(path)?,
        };
        self.prepare(repository, origin, path, temporary)
    }

    pub(crate) fn prepare(
        &self,
        repository: impl Into<PluginOrigin>,
        source: DownloadSource,
        path: &Path,
        temporary: TempDir,
    ) -> Result<PluginCandidate> {
        let extracted = temporary.path().join("extracted");
        archive::extract(path, &extracted)?;
        let root = if source.kind == "branch" {
            let children = fs::read_dir(&extracted)?.collect::<std::io::Result<Vec<_>>>()?;
            if children.len() != 1 || !children[0].file_type()?.is_dir() {
                return Err(invalid("Invalid GitHub repository archive"));
            }
            children[0].path()
        } else {
            extracted
        };
        self.prepare_root(repository.into(), source, root, temporary)
    }

    pub fn preview_local(&self, path: &Path, cancel: &Cancellation) -> Result<PluginCandidate> {
        cancel.check()?;
        archive::regular_metadata(path)?;
        let path = mcdh_core::canonicalize_path(path)?;
        let temporary = tempfile::Builder::new()
            .prefix("preview-")
            .tempdir_in(&self.root)?;
        let root = temporary.path().join("extracted");
        let (kind, digest) = if path.is_dir() {
            if self.root.starts_with(&path) || path.starts_with(&self.root) {
                return Err(invalid(
                    "Select a plugin source directory outside MCDH plugin storage",
                ));
            }
            archive::snapshot(&path, &root, cancel)?;
            (
                "local_directory",
                archive::fingerprint(&archive::inventory(&root)?),
            )
        } else {
            let snapshot = temporary.path().join("import.zip");
            archive::copy_bounded(&path, &snapshot, archive::MAX_DOWNLOAD, cancel)?;
            archive::extract_cancelled(&snapshot, &root, cancel)?;
            ("local_zip", archive::hash_file(&snapshot)?)
        };
        cancel.check()?;
        let candidate = self.prepare_root(
            PluginOrigin::Managed {
                kind: ManagedOrigin::Local,
            },
            DownloadSource {
                kind: kind.into(),
                reference: path.to_string_lossy().into_owned(),
                archive_sha256: digest,
            },
            root,
            temporary,
        )?;
        if let Err(error) = cancel.check() {
            self.discard_preview(&candidate.token)?;
            return Err(error);
        }
        Ok(candidate)
    }

    fn prepare_root(
        &self,
        repository: PluginOrigin,
        source: DownloadSource,
        root: PathBuf,
        temporary: TempDir,
    ) -> Result<PluginCandidate> {
        let manifest_path = root.join(crate::MANIFEST_FILE);
        if !manifest_path.is_file() {
            return Err(invalid(
                "Not an MCDH plugin: missing root mcdh-plugin.json or prebuilt files",
            ));
        }
        if fs::metadata(&manifest_path)?.len() > 2 * 1024 * 1024 {
            return Err(invalid("Plugin manifest exceeds 2 MiB"));
        }
        let manifest: PluginManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
        manifest.validate()?;
        if manifest.id == crate::MCDE_PLUGIN_ID {
            return Err(invalid("This id is reserved for the built-in MCDE plugin"));
        }
        let files = archive::inventory(&root)?;
        for entry in
            std::iter::once(&manifest.entry).chain(manifest.panels.iter().map(|panel| &panel.entry))
        {
            if !files.contains_key(entry) {
                return Err(invalid(format!("Missing prebuilt entry: {entry}")));
            }
        }
        if source.kind == "release" && source.reference.trim_start_matches('v') != manifest.version
        {
            return Err(invalid("Release tag and plugin version do not match"));
        }
        let fingerprint = archive::fingerprint(&files);
        let records = self.registry()?;
        if records
            .get(&manifest.id)
            .is_some_and(|record| record.repository != repository)
        {
            return Err(invalid(
                "Plugin id is already owned by a different source; uninstall it explicitly before switching sources",
            ));
        }
        let candidate = PluginCandidate {
            token: uuid::Uuid::new_v4().to_string(),
            compatible: manifest.compatible(),
            already_installed: records
                .get(&manifest.id)
                .is_some_and(|p| p.fingerprint == fingerprint),
            manifest: manifest.clone(),
            repository: repository.to_string(),
            source: source.clone(),
            fingerprint: fingerprint.clone(),
        };
        let settings = manifest.default_settings();
        let record = InstalledPlugin {
            manifest,
            repository,
            source,
            fingerprint,
            files,
            enabled: false,
            trusted_fingerprint: None,
            mcp_fingerprint: None,
            settings,
            last_error: None,
        };
        let mut prepared = self
            .prepared
            .lock()
            .map_err(|_| invalid("Preview registry poisoned"))?;
        prepared.retain(|_, preview| preview.created.elapsed() < Duration::from_secs(900));
        if prepared.len() >= 4 {
            return Err(PluginError::new(
                "too_many_previews",
                "Close an existing plugin preview first",
            ));
        }
        prepared.insert(
            candidate.token.clone(),
            PreparedPlugin {
                candidate: candidate.clone(),
                record,
                _temporary: temporary,
                root,
                created: Instant::now(),
            },
        );
        Ok(candidate)
    }

    pub fn discard_preview(&self, token: &str) -> Result<()> {
        self.prepared
            .lock()
            .map_err(|_| invalid("Preview registry poisoned"))?
            .remove(token);
        Ok(())
    }

    pub fn install(&self, token: &str, fingerprint: &str) -> Result<PluginSummary> {
        let mut prepared = self
            .prepared
            .lock()
            .map_err(|_| invalid("Preview registry poisoned"))?;
        let preview = prepared
            .get(token)
            .ok_or_else(|| invalid("Preview expired; preview the repository again"))?;
        if preview.created.elapsed() >= Duration::from_secs(900)
            || preview.candidate.fingerprint != fingerprint
        {
            return Err(invalid("Preview expired or changed"));
        }
        if !preview.record.manifest.compatible() {
            return Err(PluginError::new(
                "incompatible_plugin",
                "Plugin does not support this MCDH version",
            ));
        }
        let _registry = self.lease("registry", true)?;
        let _plugin = self.lease(&preview.record.manifest.id, true)?;
        let mut records = self.registry()?;
        let mut record = preview.record.clone();
        let retained = self
            .root
            .join("data")
            .join(&record.manifest.id)
            .join("settings.retained.json");
        if !records.contains_key(&record.manifest.id) && retained.is_file() {
            let settings: RetainedSettings =
                serde_json::from_slice(&crate::sdk::read_bounded(&retained)?)?;
            if settings.repository == record.repository
                && crate::manifest::validate_value(
                    &record.manifest.settings_schema,
                    &settings.settings,
                )
                .is_ok()
            {
                record.settings = settings.settings;
            }
        }
        if let Some(previous) = records.get(&record.manifest.id) {
            if previous.repository != record.repository {
                return Err(invalid("Plugin id belongs to another source"));
            }
            if crate::manifest::validate_value(&record.manifest.settings_schema, &previous.settings)
                .is_ok()
            {
                record.settings = previous.settings.clone();
            }
        }
        if archive::inventory(&preview.root)? != record.files {
            return Err(invalid("Preview files have changed"));
        }
        let target = self.version_directory(&record)?;
        fs::create_dir_all(
            target
                .parent()
                .ok_or_else(|| invalid("Missing version parent"))?,
        )?;
        let replaced = if target.exists()
            && !archive::inventory(&target).is_ok_and(|files| files == record.files)
        {
            let backup = self.root.join(format!("replaced-{}", uuid::Uuid::new_v4()));
            fs::rename(&target, &backup)?;
            Some(backup)
        } else {
            None
        };
        let published = !target.exists();
        if published && let Err(error) = fs::rename(&preview.root, &target) {
            if let Some(backup) = &replaced {
                fs::rename(backup, &target)?;
            }
            return Err(error.into());
        }
        record.enabled = true;
        record.trusted_fingerprint = Some(record.fingerprint.clone());
        record.mcp_fingerprint = records
            .get(&record.manifest.id)
            .filter(|old| replaced.is_none() && old.fingerprint == record.fingerprint)
            .and_then(|old| old.mcp_fingerprint.clone());
        records.insert(record.manifest.id.clone(), record.clone());
        let commit = (|| {
            if archive::inventory(&target)? != record.files {
                return Err(invalid("Installed version integrity check failed"));
            }
            self.save_registry(&records)
        })();
        if let Err(error) = commit {
            if published {
                fs::rename(&target, &preview.root).map_err(|restore| {
                    PluginError::new(
                        "rollback_failed",
                        format!(
                            "{error}; cannot restore preview {}: {restore}",
                            preview.root.display()
                        ),
                    )
                })?;
            }
            if let Some(backup) = &replaced {
                fs::rename(backup, &target).map_err(|restore| {
                    PluginError::new(
                        "rollback_failed",
                        format!("{error}; cannot restore {}: {restore}", target.display()),
                    )
                })?;
            }
            return Err(error);
        }
        if let Some(backup) = &replaced
            && let Err(error) = self.remove_owned(backup)
        {
            record.last_error = Some(PluginError::new(
                "cleanup_pending",
                format!(
                    "Installed successfully; old files remain at {}: {error}",
                    backup.display()
                ),
            ));
            records.insert(record.manifest.id.clone(), record.clone());
            if let Err(error) = self.save_registry(&records) {
                eprintln!("Plugin cleanup warning could not be saved: {error}");
            }
        }
        prepared.remove(token);
        Ok(record.into())
    }

    pub fn configure(
        &self,
        id: &str,
        fingerprint: &str,
        enabled: bool,
        allow_mcp: bool,
        settings: Value,
    ) -> Result<PluginSummary> {
        let _registry = self.lease("registry", true)?;
        let _plugin = self.lease(id, true)?;
        let mut records = self.registry()?;
        let record = records
            .get_mut(id)
            .ok_or_else(|| invalid("Plugin is not installed"))?;
        if record.fingerprint != fingerprint {
            return Err(invalid(
                "Plugin version changed; reload before granting trust",
            ));
        }
        if (enabled || allow_mcp)
            && let Err(error) = self.verify(record)
        {
            if error.code == "integrity_error" {
                record.enabled = false;
                record.trusted_fingerprint = None;
                record.mcp_fingerprint = None;
                record.last_error = Some(error.clone());
                self.save_registry(&records)?;
            }
            return Err(error);
        }
        if allow_mcp && !enabled {
            return Err(invalid("Disabled plugins cannot be authorized for MCP"));
        }
        if id == crate::MCDE_PLUGIN_ID {
            if allow_mcp {
                return Err(invalid(
                    "Native MCDE is desktop-only and does not expose MCP tools",
                ));
            }
            self.ensure_mcde_idle()?;
        }
        crate::manifest::validate_value(&record.manifest.settings_schema, &settings)?;
        record.enabled = enabled;
        record.mcp_fingerprint = allow_mcp.then(|| fingerprint.to_string());
        record.settings = settings;
        record.last_error = None;
        let summary = record.clone().into();
        self.save_registry(&records)?;
        Ok(summary)
    }

    pub fn uninstall(&self, id: &str, clear_data: bool) -> Result<()> {
        let _registry = self.lease("registry", true)?;
        let _plugin = self.lease(id, true)?;
        if id == crate::MCDE_PLUGIN_ID {
            self.ensure_mcde_idle()?;
        }
        let mut records = self.registry()?;
        let record = records
            .remove(id)
            .ok_or_else(|| invalid("Plugin is not installed"))?;
        if !clear_data {
            let data = self.root.join("data").join(id);
            fs::create_dir_all(&data)?;
            crate::sdk::write_atomic(
                &data.join("settings.retained.json"),
                &serde_json::to_vec(&RetainedSettings {
                    repository: record.repository,
                    settings: record.settings,
                })?,
            )?;
        }
        // Detach executable code first. A failed database write restores its original path.
        let directory = self.root.join("installed").join(id);
        let tombstone = self.root.join(format!("removed-{}", uuid::Uuid::new_v4()));
        if directory.exists() {
            fs::rename(&directory, &tombstone)?;
        }
        if let Err(error) = self.save_registry(&records) {
            if tombstone.exists() {
                fs::rename(&tombstone, &directory)?;
            }
            return Err(error);
        }
        if tombstone.exists() {
            self.remove_owned(&tombstone)?;
        }
        let data = self.root.join("data").join(id);
        if clear_data && data.exists() {
            self.remove_owned(&data)?;
        }
        Ok(())
    }

    fn remove_owned(&self, path: &Path) -> Result<()> {
        let canonical = mcdh_core::canonicalize_path(path)?;
        if !canonical.starts_with(&self.root) || canonical == self.root {
            return Err(invalid("Refusing to remove a path outside plugin data"));
        }
        fs::remove_dir_all(path)?;
        Ok(())
    }

    pub(crate) fn version_directory(&self, record: &InstalledPlugin) -> Result<PathBuf> {
        if !crate::manifest::identifier(&record.manifest.id)
            || record.fingerprint.len() != 64
            || !record.fingerprint.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(invalid("Invalid stored plugin identity"));
        }
        Ok(self
            .root
            .join("installed")
            .join(&record.manifest.id)
            .join(&record.fingerprint))
    }

    pub(crate) fn verify(&self, record: &InstalledPlugin) -> Result<()> {
        record.manifest.validate()?;
        if !record.manifest.compatible() {
            return Err(PluginError::new(
                "incompatible_plugin",
                "Plugin requires a different MCDH version",
            ));
        }
        if record.trusted_fingerprint.as_ref() != Some(&record.fingerprint) {
            return Err(PluginError::new(
                "untrusted_plugin",
                "Plugin has not been explicitly trusted",
            ));
        }
        if record.manifest.id == crate::MCDE_PLUGIN_ID {
            return crate::builtin::verify_mcde(record);
        }
        let root = self.version_directory(record)?;
        let files = archive::inventory(&root).map_err(|error| {
            PluginError::new(
                "integrity_error",
                format!("Cannot verify plugin code; reinstall and confirm trust again: {error}"),
            )
        })?;
        if files != record.files || archive::fingerprint(&record.files) != record.fingerprint {
            return Err(PluginError::new(
                "integrity_error",
                "Plugin code changed; reinstall and confirm trust again",
            ));
        }
        Ok(())
    }

    pub fn asset(&self, id: &str, fingerprint: &str, path: &str) -> Result<Vec<u8>> {
        let _lease = self.lease(id, false)?;
        let record = self
            .registry()?
            .remove(id)
            .ok_or_else(|| invalid("Plugin is not installed"))?;
        if !record.enabled || record.fingerprint != fingerprint {
            return Err(invalid("Plugin is disabled or its version changed"));
        }
        self.verify(&record).inspect_err(|error| {
            let _ = self.record_error(id, error);
        })?;
        let relative = archive::relative_path(path)?;
        if !record.files.contains_key(path) {
            return Err(invalid("Asset is not in the trusted plugin inventory"));
        }
        Ok(fs::read(self.version_directory(&record)?.join(relative))?)
    }
}
