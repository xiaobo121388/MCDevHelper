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
        ["COM\u{b9}.txt", "safe"],
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

#[test]
fn retained_settings_restore_only_for_the_same_repository() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(temp.path());
    let mut definition = manifest();
    definition["settings_schema"] = json!({"type":"object","properties":{"label":{"type":"string","default":"initial"}},"required":["label"],"additionalProperties":false});
    let path = package(temp.path(), &definition, "");
    let preview = service.preview_archive("owner/repo", &path).unwrap();
    let plugin = service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    service
        .configure(
            &plugin.manifest.id,
            &plugin.fingerprint,
            true,
            false,
            json!({"label":"custom"}),
        )
        .unwrap();
    service.uninstall(&plugin.manifest.id, false).unwrap();
    let preview = service.preview_archive("owner/repo", &path).unwrap();
    let restored = service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    assert_eq!(restored.settings, json!({"label":"custom"}));
    service.uninstall(&plugin.manifest.id, false).unwrap();
    let preview = service.preview_archive("other/repo", &path).unwrap();
    assert_eq!(
        service
            .install(&preview.token, &preview.fingerprint)
            .unwrap()
            .settings,
        json!({"label":"initial"})
    );
    service.uninstall(&plugin.manifest.id, true).unwrap();
    assert!(!service.root.join("data").join(&plugin.manifest.id).exists());
}

#[test]
fn installs_commit_pinned_repository_archives_without_running_code() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(temp.path());
    let archive = temp.path().join("branch.zip");
    let mut zip = ZipWriter::new(fs::File::create(&archive).unwrap());
    for (name, bytes) in [
        ("mcdh-plugin.json", manifest().to_string()),
        ("main.js", "throw new Error('do not run');".into()),
        ("panel.js", "export function mount() {}".into()),
    ] {
        zip.start_file(format!("repo-commit/{name}"), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let staging = tempfile::tempdir_in(&service.root).unwrap();
    let source = crate::github::DownloadSource {
        kind: "branch".into(),
        reference: "b".repeat(40),
        archive_sha256: crate::archive::hash_file(&archive).unwrap(),
    };
    let preview = service
        .prepare(
            crate::github::Repository::parse("owner/repo").unwrap(),
            source,
            &archive,
            staging,
        )
        .unwrap();
    let plugin = service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    assert_eq!(plugin.source.reference, "b".repeat(40));
    assert!(plugin.enabled && plugin.trusted && !plugin.allow_mcp);
}

#[test]
fn rejects_link_entries_and_declared_zip_bombs_before_expansion() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("link.zip");
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    zip.add_symlink("link", "../../escape", SimpleFileOptions::default())
        .unwrap();
    zip.finish().unwrap();
    assert!(crate::archive::extract(&path, &temp.path().join("link-output")).is_err());
    let path = temp.path().join("bomb.zip");
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    zip.start_file("large.bin", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"small").unwrap();
    zip.finish().unwrap();
    let mut bytes = fs::read(&path).unwrap();
    let central = bytes
        .windows(4)
        .position(|part| part == b"PK\x01\x02")
        .unwrap();
    let size = ((crate::archive::MAX_EXPANDED + 1) as u32).to_le_bytes();
    bytes[22..26].copy_from_slice(&size);
    bytes[central + 24..central + 28].copy_from_slice(&size);
    fs::write(&path, bytes).unwrap();
    assert!(
        crate::archive::extract(&path, &temp.path().join("bomb-output"))
            .unwrap_err()
            .message
            .contains("512 MiB")
    );
    let path = temp.path().join("entries.zip");
    let mut zip = ZipWriter::new(fs::File::create(&path).unwrap());
    for index in 0..=crate::archive::MAX_ENTRIES {
        zip.start_file(format!("file-{index}"), SimpleFileOptions::default())
            .unwrap();
    }
    zip.finish().unwrap();
    assert!(
        crate::archive::extract(&path, &temp.path().join("entries-output"))
            .unwrap_err()
            .message
            .contains("Too many")
    );
}

#[test]
fn reserves_namespace_delimiters_and_rejects_oversized_downloads() {
    for id in ["nul", "con", "lpt1", "trailing."] {
        let mut definition = manifest();
        definition["id"] = json!(id);
        assert!(
            serde_json::from_value::<PluginManifest>(definition)
                .unwrap()
                .validate()
                .is_err(),
            "{id}"
        );
    }
    let mut definition = manifest();
    definition["id"] = json!("a__b");
    assert!(
        serde_json::from_value::<PluginManifest>(definition)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut definition = manifest();
    definition["id"] = json!("registry");
    assert!(
        serde_json::from_value::<PluginManifest>(definition)
            .unwrap()
            .validate()
            .is_err()
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.zip");
    fs::File::create(&path)
        .unwrap()
        .set_len(crate::archive::MAX_DOWNLOAD + 1)
        .unwrap();
    assert!(
        service(temp.path())
            .preview_archive("owner/repo", &path)
            .unwrap_err()
            .message
            .contains("128 MiB")
    );
}

#[test]
fn explicitly_reinstalling_repairs_a_tampered_identical_version() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(temp.path());
    let path = package(temp.path(), &manifest(), "// original");
    let preview = service.preview_archive("owner/repo", &path).unwrap();
    let plugin = service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    service
        .configure(
            &plugin.manifest.id,
            &plugin.fingerprint,
            true,
            true,
            json!({}),
        )
        .unwrap();
    let record = service
        .registry()
        .unwrap()
        .remove(&plugin.manifest.id)
        .unwrap();
    fs::write(
        service.version_directory(&record).unwrap().join("main.js"),
        "// tampered",
    )
    .unwrap();
    let preview = service.preview_archive("owner/repo", &path).unwrap();
    let repaired = service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    assert!(repaired.enabled && repaired.trusted && !repaired.allow_mcp);
    assert_eq!(
        service
            .asset(&plugin.manifest.id, &plugin.fingerprint, "main.js")
            .unwrap(),
        b"// original"
    );
}

#[tokio::test]
async fn interrupted_update_retains_the_previous_trusted_files() {
    let temp = tempfile::tempdir().unwrap();
    let service = service(temp.path());
    let preview = service
        .preview_archive(
            "owner/repo",
            &package(temp.path(), &manifest(), "// original"),
        )
        .unwrap();
    let plugin = service
        .install(&preview.token, &preview.fingerprint)
        .unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    assert_eq!(
        service
            .preview("owner/repo", &cancellation, |_| {})
            .await
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert_eq!(
        service
            .asset(&plugin.manifest.id, &plugin.fingerprint, "main.js")
            .unwrap(),
        b"// original"
    );
    assert!(service.list().unwrap()[0].trusted);
}

#[test]
fn published_manifest_schema_validates_the_example_and_versions() {
    let schema: Value =
        serde_json::from_str(include_str!("../../../docs/plugin-manifest.schema.json")).unwrap();
    let example: Value = serde_json::from_str(include_str!(
        "../../../examples/plugins/hello/mcdh-plugin.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&example));
    serde_json::from_value::<PluginManifest>(example.clone())
        .unwrap()
        .validate()
        .unwrap();
    let mut invalid = example;
    invalid["api_version"] = json!(99);
    assert!(!validator.is_valid(&invalid));
}

#[test]
fn missing_code_revokes_authorization_for_panel_calls_and_configuration() {
    for through_panel in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let service = service(temp.path());
        let preview = service
            .preview_archive(
                "owner/repo",
                &package(temp.path(), &manifest(), "// original"),
            )
            .unwrap();
        let plugin = service
            .install(&preview.token, &preview.fingerprint)
            .unwrap();
        service
            .configure(
                &plugin.manifest.id,
                &plugin.fingerprint,
                true,
                true,
                json!({}),
            )
            .unwrap();
        let record = service
            .registry()
            .unwrap()
            .remove(&plugin.manifest.id)
            .unwrap();
        let root = service.version_directory(&record).unwrap();
        if through_panel {
            fs::rename(&root, temp.path().join("detached-code")).unwrap();
        } else {
            fs::remove_file(root.join("main.js")).unwrap();
        }
        let error = if through_panel {
            service
                .panel_info(&plugin.manifest.id, &plugin.fingerprint, "main", None)
                .unwrap_err()
        } else {
            service
                .configure(
                    &plugin.manifest.id,
                    &plugin.fingerprint,
                    true,
                    true,
                    json!({}),
                )
                .unwrap_err()
        };
        assert_eq!(error.code, "integrity_error");
        let current = service.list().unwrap().remove(0);
        assert!(!current.trusted && !current.enabled && !current.allow_mcp);
    }
}
