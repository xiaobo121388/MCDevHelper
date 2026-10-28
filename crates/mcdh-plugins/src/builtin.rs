use std::collections::BTreeMap;

use mcdh_core::{
    mcdk::McdkStore,
    mcdk_session::{SessionMode, refresh_session},
};
use serde_json::json;

use crate::{
    ExecutionRequest, PluginCaller, PluginError, PluginManifest, PluginService, Result, archive,
    error::invalid,
    github::DownloadSource,
    origin::{ManagedOrigin, PluginOrigin},
    store::{InstalledPlugin, Lease},
};

pub const MCDE_PLUGIN_ID: &str = "mcdh.mcde";

/// Holds the cross-process read lease for a native MCDE launch.
pub struct McdePluginLease {
    _lease: Lease,
}

fn mcde_record() -> Result<InstalledPlugin> {
    let manifest: PluginManifest = serde_json::from_value(json!({
        "schema_version":1, "api_version":1, "id":MCDE_PLUGIN_ID,
        "name":"MCDE", "version":mcdh_core::VERSION, "author":"MCDH",
        "description":"MCDevEditor 原生编辑器", "engines":{"mcdh":format!("={}", mcdh_core::VERSION)},
        "entry":"native/mcde", "commands":[{
            "id":"open-editor", "title":"在 MCDE 中打开", "menus":["component"],
            "description":"使用原生 MCDE 宿主打开当前组件"
        }]
    }))?;
    manifest.validate()?;
    let fingerprint = archive::fingerprint(&BTreeMap::from([
        ("manifest".into(), serde_json::to_string(&manifest)?),
        (
            "host_revision".into(),
            mcdh_core::mcde::HOST_REVISION.into(),
        ),
    ]));
    Ok(InstalledPlugin {
        manifest,
        repository: PluginOrigin::Managed {
            kind: ManagedOrigin::BundledMcde,
        },
        source: DownloadSource {
            kind: "builtin".into(),
            reference: format!("MCDH {}", mcdh_core::VERSION),
            archive_sha256: String::new(),
        },
        trusted_fingerprint: Some(fingerprint.clone()),
        fingerprint,
        files: BTreeMap::new(),
        enabled: true,
        mcp_fingerprint: None,
        settings: json!({}),
        last_error: None,
    })
}

pub(crate) fn verify_mcde(record: &InstalledPlugin) -> Result<()> {
    let expected = mcde_record()?;
    if record.repository != expected.repository
        || record.fingerprint != expected.fingerprint
        || serde_json::to_value(&record.manifest)? != serde_json::to_value(expected.manifest)?
        || !record.files.is_empty()
        || record.mcp_fingerprint.is_some()
    {
        return Err(PluginError::new(
            "integrity_error",
            "Built-in MCDE registration does not match this application",
        ));
    }
    Ok(())
}

impl PluginService {
    pub fn register_builtins(&self) -> Result<()> {
        if self
            .registry()?
            .get(MCDE_PLUGIN_ID)
            .is_some_and(|record| verify_mcde(record).is_ok())
        {
            return Ok(());
        }
        let _registry = self.lease("registry", true)?;
        let _plugin = self.lease(MCDE_PLUGIN_ID, true)?;
        let mut records = self.registry()?;
        let mut record = mcde_record()?;
        if let Some(previous) = records.get(MCDE_PLUGIN_ID) {
            if previous.repository != record.repository {
                return Err(invalid(
                    "The reserved MCDE id belongs to another source; uninstall that plugin first",
                ));
            }
            if verify_mcde(previous).is_ok() {
                return Ok(());
            }
            record.enabled = previous.enabled;
        }
        records.insert(MCDE_PLUGIN_ID.into(), record);
        self.save_registry(&records)
    }

    pub fn mcde_enabled(&self) -> Result<bool> {
        let records = self.registry()?;
        Ok(records
            .get(MCDE_PLUGIN_ID)
            .is_some_and(|record| record.enabled && self.verify(record).is_ok()))
    }

    pub fn mcde_lease(&self, fingerprint: Option<&str>) -> Result<McdePluginLease> {
        let lease = self.lease(MCDE_PLUGIN_ID, false)?;
        let record = self.authorize(MCDE_PLUGIN_ID, PluginCaller::Desktop)?;
        if fingerprint.is_some_and(|expected| expected != record.fingerprint) {
            return Err(PluginError::new(
                "plugin_version_changed",
                "MCDE registration changed; refresh the plugin list",
            ));
        }
        Ok(McdePluginLease { _lease: lease })
    }

    pub fn mcde_command(&self, request: &ExecutionRequest) -> Result<McdePluginLease> {
        if request.plugin_id != MCDE_PLUGIN_ID || request.command != "open-editor" {
            return Err(invalid("Unknown native MCDE command"));
        }
        crate::manifest::validate_value(&crate::manifest::object_schema(), &request.input)?;
        if request.component_id.as_ref().is_none_or(|id| id.is_empty()) {
            return Err(invalid("MCDE requires a component context"));
        }
        self.mcde_lease(request.fingerprint.as_deref())
    }

    pub fn record_mcde_error(&self, error: &PluginError) -> Result<()> {
        self.record_error(MCDE_PLUGIN_ID, error)
    }

    pub(crate) fn ensure_mcde_idle(&self) -> Result<()> {
        let _component = self.index.try_lock_mutations()?;
        let store = McdkStore::new(self.index.clone());
        let _state = store.lock("state")?;
        if refresh_session(&store)?
            .session
            .is_some_and(|session| session.mode == SessionMode::Editor)
        {
            return Err(PluginError::new(
                "plugin_busy",
                "MCDE is running; close its game session before changing the plugin",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_registration_persists_disable_and_protects_identity() {
        let temp = tempfile::tempdir().unwrap();
        let service = crate::tests::service(temp.path());
        service.register_builtins().unwrap();
        let plugin = service.list().unwrap().remove(0);
        assert_eq!(plugin.manifest.id, MCDE_PLUGIN_ID);
        assert_eq!(plugin.source.kind, "builtin");
        assert!(plugin.manifest.mcp_tools.is_empty());
        assert!(service.mcde_enabled().unwrap());
        assert!(
            service
                .configure(MCDE_PLUGIN_ID, &plugin.fingerprint, true, true, json!({}))
                .is_err()
        );
        let guard = service.mcde_lease(Some(&plugin.fingerprint)).unwrap();
        let other = PluginService::new(service.index.clone()).unwrap();
        other.register_builtins().unwrap();
        assert_eq!(
            other
                .configure(MCDE_PLUGIN_ID, &plugin.fingerprint, false, false, json!({}))
                .unwrap_err()
                .code,
            "plugin_busy"
        );
        drop(guard);
        other
            .configure(MCDE_PLUGIN_ID, &plugin.fingerprint, false, false, json!({}))
            .unwrap();
        other.register_builtins().unwrap();
        assert!(!service.mcde_enabled().unwrap());
        assert!(service.mcde_lease(None).is_err());
        assert_eq!(
            service.uninstall(MCDE_PLUGIN_ID, true).unwrap_err().code,
            "builtin_plugin"
        );
        let mut manifest = crate::tests::manifest();
        manifest["id"] = json!(MCDE_PLUGIN_ID);
        let archive = crate::tests::package(temp.path(), &manifest, "");
        assert!(
            service
                .preview_local(&archive, &crate::Cancellation::default())
                .is_err()
        );
        assert!(service.preview_archive("o/r", &archive).is_err());
    }

    #[test]
    fn native_commands_require_context_current_fingerprint_and_enabled_registration() {
        let temp = tempfile::tempdir().unwrap();
        let service = crate::tests::service(temp.path());
        service.register_builtins().unwrap();
        let mut request: ExecutionRequest = serde_json::from_value(
            json!({"plugin_id":MCDE_PLUGIN_ID,"command":"open-editor","component_id":"component"}),
        )
        .unwrap();
        assert!(service.mcde_command(&request).is_ok());
        request.fingerprint = Some("old".into());
        assert!(service.mcde_command(&request).is_err());
        request.fingerprint = None;
        request.input = json!({"unexpected":true});
        assert!(service.mcde_command(&request).is_err());
        request.input = json!({});
        request.component_id = None;
        assert!(service.mcde_command(&request).is_err());
        let mut records = service.registry().unwrap();
        records.get_mut(MCDE_PLUGIN_ID).unwrap().manifest.entry = "malicious.js".into();
        service.save_registry(&records).unwrap();
        assert!(!service.mcde_enabled().unwrap());
        assert!(service.mcde_lease(None).is_err());
    }

    #[test]
    #[cfg(windows)]
    fn live_editor_blocks_disable_across_instances_without_opening_a_game() {
        use mcdh_core::mcdk_session::{McdkSession, SESSION_KEY, SessionState, inspect_process};
        let temp = tempfile::tempdir().unwrap();
        let service = crate::tests::service(temp.path());
        service.register_builtins().unwrap();
        let plugin = service.list().unwrap().remove(0);
        let process = inspect_process(std::process::id()).unwrap().unwrap();
        let store = McdkStore::new(service.index.clone());
        store
            .write(
                SESSION_KEY,
                &SessionState {
                    session: Some(McdkSession {
                        component_id: "test".into(),
                        component_path: temp.path().into(),
                        executable: process.executable,
                        created_at: process.created_at,
                        pid: std::process::id(),
                        version: "test".into(),
                        mode: SessionMode::Editor,
                        launch_directory: None,
                    }),
                    last_exit: None,
                },
            )
            .unwrap();
        let other = PluginService::new(service.index.clone()).unwrap();
        assert_eq!(
            other
                .configure(MCDE_PLUGIN_ID, &plugin.fingerprint, false, false, json!({}))
                .unwrap_err()
                .code,
            "plugin_busy"
        );
        assert!(service.mcde_enabled().unwrap());
        store.write(SESSION_KEY, &SessionState::default()).unwrap();
        other
            .configure(MCDE_PLUGIN_ID, &plugin.fingerprint, false, false, json!({}))
            .unwrap();
        assert!(!service.mcde_enabled().unwrap());
    }
}
