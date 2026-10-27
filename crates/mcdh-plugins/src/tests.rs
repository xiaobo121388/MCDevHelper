use crate::*;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use zip::{ZipWriter, write::SimpleFileOptions};

pub(crate) fn manifest() -> Value {
    json!({"schema_version":1,"api_version":1,"id":"example.hello","name":"Hello","version":"1.0.0",
        "engines":{"mcdh":">=1.0.0"},"entry":"main.js","commands":[{"id":"hello","title":"Hello"}],
        "panels":[{"id":"main","title":"Hello panel","entry":"panel.js"}],
        "mcp_tools":[{"name":"hello","description":"Say hello","command":"hello"}]})
}

pub(crate) fn package(directory: &Path, manifest: &Value, script: &str) -> PathBuf {
    let path = directory.join(format!("{}.zip", uuid::Uuid::new_v4()));
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    for (name, bytes) in [
        ("mcdh-plugin.json", manifest.to_string()),
        ("main.js", script.to_string()),
        ("panel.js", "export function mount() {}".into()),
    ] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}

pub(crate) fn service(directory: &Path) -> PluginService {
    PluginService::new(mcdh_core::LocalIndex::open(directory.join("state/mcdh.db")).unwrap())
        .unwrap()
}

#[test]
fn installation_is_explicit_atomic_and_repository_bound() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(temp.path());
    let path = package(
        temp.path(),
        &manifest(),
        "throw new Error('must not execute during install');",
    );
    let preview = service.preview_archive("owner/repo", &path).unwrap();
    assert!(service.list().unwrap().is_empty());
    assert!(service.install(&preview.token, "wrong").is_err());
    let installed = service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    assert!(installed.enabled && installed.trusted && !installed.allow_mcp);
    let duplicate = service.preview_archive("owner/repo", &path).unwrap();
    assert!(duplicate.already_installed);
    assert!(service.preview_archive("other/repo", &path).is_err());
    let mut incompatible = manifest();
    incompatible["engines"]["mcdh"] = json!(">=99.0.0");
    let path = package(temp.path(), &incompatible, "");
    let preview = service.preview_archive("owner/repo", &path).unwrap();
    assert!(!preview.compatible);
    assert!(
        service
            .install(&preview.token, &preview.fingerprint)
            .is_err()
    );
    assert_eq!(
        service.list().unwrap()[0].fingerprint,
        installed.fingerprint
    );
}

#[test]
fn update_revokes_mcp_and_busy_plugins_cannot_be_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(temp.path());
    let first = service
        .preview_archive("o/r", &package(temp.path(), &manifest(), "// v1"))
        .unwrap();
    let installed = service.install(&first.token, &first.fingerprint).unwrap();
    service
        .configure(
            "example.hello",
            &installed.fingerprint,
            true,
            true,
            json!({}),
        )
        .unwrap();
    let second = service
        .preview_archive("o/r", &package(temp.path(), &manifest(), "// v2"))
        .unwrap();
    let lease = service.lease("example.hello", false).unwrap();
    assert_eq!(
        service
            .install(&second.token, &second.fingerprint)
            .unwrap_err()
            .code,
        "plugin_busy"
    );
    assert!(service.uninstall("example.hello", false).is_err());
    drop(lease);
    assert!(
        !service
            .install(&second.token, &second.fingerprint)
            .unwrap()
            .allow_mcp
    );
    assert!(
        service
            .configure(
                "example.hello",
                &installed.fingerprint,
                true,
                true,
                json!({})
            )
            .is_err()
    );
}

#[test]
fn modified_code_and_unlisted_assets_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(temp.path());
    let preview = service
        .preview_archive("o/r", &package(temp.path(), &manifest(), "// trusted"))
        .unwrap();
    service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    assert!(
        service
            .asset("example.hello", &preview.fingerprint, "panel.js")
            .is_ok()
    );
    assert!(
        service
            .asset("example.hello", &preview.fingerprint, "../mcdh.db")
            .is_err()
    );
    let record = service.registry().unwrap().remove("example.hello").unwrap();
    fs::write(
        service.version_directory(&record).unwrap().join("main.js"),
        "// altered",
    )
    .unwrap();
    assert_eq!(
        service
            .asset("example.hello", &preview.fingerprint, "panel.js")
            .unwrap_err()
            .code,
        "integrity_error"
    );
}

#[test]
fn uninstall_retains_data_unless_explicitly_cleared() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(temp.path());
    let path = package(temp.path(), &manifest(), "");
    let preview = service.preview_archive("o/r", &path).unwrap();
    service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    let data = service.root.join("data/example.hello");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("test.json"), "{}").unwrap();
    service.uninstall("example.hello", false).unwrap();
    assert!(data.is_dir());
    let preview = service.preview_archive("o/r", &path).unwrap();
    service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    service.uninstall("example.hello", true).unwrap();
    assert!(!data.exists());
}

#[test]
fn rejects_malicious_archives_and_external_schema_refs() {
    let temp = tempfile::tempdir().unwrap();
    for names in [
        ["../escape", "safe"],
        ["A.js", "a.js"],
        ["CON.txt", "safe"],
        ["a:stream", "safe"],
        ["a", "a/child"],
    ] {
        let path = temp.path().join("malicious.zip");
        let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
        for name in names {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
        let target = tempfile::tempdir_in(temp.path()).unwrap();
        assert!(
            crate::archive::extract(&path, target.path()).is_err(),
            "{names:?}"
        );
    }
    let mut definition = manifest();
    definition["commands"][0]["input_schema"] =
        json!({"type":"object", "$ref":"file:///C:/secret"});
    assert!(
        serde_json::from_value::<PluginManifest>(definition)
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(!temp.path().join("escape").exists());
}

#[test]
fn missing_prebuilt_entries_and_invalid_defaults_are_reported() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(temp.path());
    let mut definition = manifest();
    definition["entry"] = json!("dist/not-built.js");
    assert!(
        service
            .preview_archive("o/r", &package(temp.path(), &definition, ""))
            .is_err()
    );
    definition = manifest();
    definition["settings_schema"] =
        json!({"type":"object", "required":["value"], "properties":{"value":{"type":"string"}}});
    assert!(
        service
            .preview_archive("o/r", &package(temp.path(), &definition, ""))
            .is_err()
    );
}
