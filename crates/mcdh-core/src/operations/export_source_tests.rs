use super::*;
use crate::SourceKind;
use zip::ZipArchive;

fn fixture() -> (tempfile::TempDir, LocalIndex, PathBuf, String, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let index = LocalIndex::open(temp.path().join("state/db")).unwrap();
    let root = temp.path().join("BOMD");
    let destination = temp.path().join("exports");
    fs::create_dir_all(&destination).unwrap();
    for (directory, kind) in [("BP", "data"), ("RP", "resources")] {
        let pack = root.join("src").join(directory);
        fs::create_dir_all(&pack).unwrap();
        let manifest = serde_json::json!({"format_version":2,"header":{"name":directory,"uuid":Uuid::new_v4().to_string(),"version":[1,0,0]},"modules":[{"type":kind,"uuid":Uuid::new_v4().to_string(),"version":[1,0,0]}]});
        fs::write(
            pack.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(pack.join("content.txt"), directory).unwrap();
    }
    fs::write(root.join("work.mcscfg"), r#"{"Type":7,"Name":"BOMD"}"#).unwrap();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/private"), "must not be in game ZIP").unwrap();
    fs::write(root.join("build.ps1"), "project helper").unwrap();
    fs::write(root.join("src/BP/typing.pyi"), "stub").unwrap();
    index.add_source(SourceKind::Single, &root).unwrap();
    let id = index.component_id(&root).unwrap();
    (temp, index, root, id, destination)
}

fn originals(root: &Path) -> [Vec<u8>; 2] {
    ["src/BP/manifest.json", "src/RP/manifest.json"].map(|file| fs::read(root.join(file)).unwrap())
}

fn entries(path: &Path) -> Vec<String> {
    let mut zip = ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_owned())
        .collect()
}

#[test]
fn location_info_is_read_only_and_reports_missing_and_saved_sources() {
    let (_temp, index, root, id, destination) = fixture();
    let service = ComponentService::new(index.clone());
    let before = originals(&root);
    let unresolved = service.export_source_info(&id).unwrap();
    assert_eq!(unresolved.path, root);
    assert!(!unresolved.configured);
    assert!(!unresolved.valid);
    assert!(unresolved.issue.is_some());
    assert!(index.component_export_source(&root).unwrap().is_none());

    service.set_export_source(&id, &root.join("src")).unwrap();
    let saved = service.export_source_info(&id).unwrap();
    assert_eq!(saved.path, root.join("src"));
    assert!(saved.configured);
    assert!(saved.valid);
    assert!(saved.issue.is_none());

    fs::rename(root.join("src"), root.join("moved")).unwrap();
    let missing = service.export_source_info(&id).unwrap();
    assert_eq!(missing.path, root.join("src"));
    assert!(missing.configured);
    assert!(!missing.valid);
    assert!(missing.issue.is_some());
    let after = ["BP/manifest.json", "RP/manifest.json"]
        .map(|file| fs::read(root.join("moved").join(file)).unwrap());
    assert_eq!(after, before);
    assert!(fs::read_dir(destination).unwrap().next().is_none());
}

#[test]
fn recognizable_project_directory_is_visible_without_a_saved_override() {
    let (_temp, index, root, _id, _destination) = fixture();
    let source = root.join("src");
    index.add_source(SourceKind::Single, &source).unwrap();
    let id = index.component_id(&source).unwrap();
    let service = ComponentService::new(index.clone());
    let info = service.export_source_info(&id).unwrap();
    assert_eq!(info.path, source);
    assert!(info.valid);
    assert!(!info.configured);
    assert!(info.issue.is_none());
    assert!(index.component_export_source(&source).unwrap().is_none());
    service.set_export_source(&id, &source).unwrap();
    assert!(service.export_source_info(&id).unwrap().configured);
}

#[test]
fn unidentified_root_requests_selection_before_any_mutation_or_publication() {
    let (_temp, index, root, id, destination) = fixture();
    let service = ComponentService::new(index.clone());
    let original = originals(&root);
    for mode in [ContentMode::Clean, ContentMode::Full] {
        let error = service
            .export_component(&ExportComponentRequest {
                component_id: id.clone(),
                destination: destination.clone(),
                content_mode: mode,
                conflict_policy: ExportConflictPolicy::Rename,
            })
            .unwrap_err();
        assert_eq!(error.code(), "pack_location_required");
        assert_eq!(error.path(), Some(root.as_path()));
    }
    let mut phases = Vec::new();
    let request = QuickExportRequest {
        component_id: id.clone(),
        destination: destination.clone(),
    };
    assert_eq!(
        service
            .quick_export_component(&request, |phase| phases.push(phase))
            .unwrap_err()
            .code(),
        "pack_location_required"
    );
    assert_eq!(phases, vec![QuickExportPhase::Preparing]);
    assert_eq!(originals(&root), original);
    assert!(fs::read_dir(destination).unwrap().next().is_none());
    assert!(index.component_export_source(&root).unwrap().is_none());
}

#[test]
fn selected_source_persists_and_clean_zip_flattens_only_the_real_packs() {
    let (_temp, index, root, id, destination) = fixture();
    let service = ComponentService::new(index.clone());
    assert_eq!(
        service.set_export_source(&id, &root.join("src")).unwrap(),
        root.join("src")
    );
    let stored = index
        .setting(&format!("component_export_source:{id}"))
        .unwrap()
        .unwrap();
    assert_eq!(stored, "\"src\"");
    let component = service.get_component(&id).unwrap();
    assert_eq!(component.id, id);
    assert_eq!(component.path, root);
    assert_eq!(component.manifests.len(), 2);
    assert_eq!(component.name, "BOMD");
    let result = service
        .quick_export_component(
            &QuickExportRequest {
                component_id: id.clone(),
                destination: destination.clone(),
            },
            |_| {},
        )
        .unwrap();
    assert_eq!(result.component.unwrap().version, Some([1, 0, 1]));
    let names = entries(&result.actual_path);
    assert!(names.contains(&"BP/manifest.json".into()));
    assert!(names.contains(&"RP/manifest.json".into()));
    assert!(!names.iter().any(|name| name.starts_with("src/")
        || name.contains(".git")
        || name.ends_with(".pyi")
        || name == "build.ps1"));
    let full = service
        .export_component(&ExportComponentRequest {
            component_id: id.clone(),
            destination,
            content_mode: ContentMode::Full,
            conflict_policy: ExportConflictPolicy::Rename,
        })
        .unwrap();
    let names = entries(&full.actual_path);
    assert!(names.contains(&"src/BP/manifest.json".into()));
    assert!(names.contains(&"src/RP/manifest.json".into()));
    assert!(names.contains(&".git/private".into()));
    assert!(names.contains(&"build.ps1".into()));
    assert!(names.contains(&"src/BP/typing.pyi".into()));
    let regenerated = service.regenerate_manifest_uuids(&id).unwrap();
    assert_eq!(regenerated.modified_files.len(), 2);
    assert_eq!(regenerated.component.unwrap().path, root);
    let version = service
        .bump_manifest_version(&BumpManifestVersionRequest {
            component_id: id,
            part: VersionPart::Minor,
        })
        .unwrap();
    assert_eq!(version.component.unwrap().version, Some([1, 1, 0]));
}

#[test]
fn selected_source_rolls_back_on_conflict_and_does_not_forget_its_location() {
    let (_temp, index, root, id, destination) = fixture();
    let service = ComponentService::new(index.clone());
    service.set_export_source(&id, &root.join("src")).unwrap();
    let original = originals(&root);
    let mut settings = index.app_settings().unwrap();
    settings.quick_export.conflict_policy = ExportConflictPolicy::Error;
    index.set_app_settings(&settings).unwrap();
    fs::write(destination.join("BOMD.zip"), "previous ZIP").unwrap();
    let error = service
        .quick_export_component(
            &QuickExportRequest {
                component_id: id,
                destination: destination.clone(),
            },
            |_| {},
        )
        .unwrap_err();
    assert_eq!(error.code(), "destination_exists");
    assert_eq!(originals(&root), original);
    assert_eq!(
        index.component_export_source(&root).unwrap(),
        Some(root.join("src"))
    );
    assert_eq!(
        fs::read_to_string(destination.join("BOMD.zip")).unwrap(),
        "previous ZIP"
    );
}

#[test]
fn invalid_or_outside_selections_are_rejected_and_missing_locations_can_be_reselected() {
    let (_temp, index, root, id, destination) = fixture();
    let service = ComponentService::new(index.clone());
    assert!(service.set_export_source(&id, &destination).is_err());
    assert!(service.set_export_source(&id, &root).is_err());
    assert!(index.component_export_source(&root).unwrap().is_none());
    service.set_export_source(&id, &root.join("src")).unwrap();
    fs::rename(root.join("src"), root.join("assets")).unwrap();
    assert_eq!(
        service.export_source(&id).unwrap_err().code(),
        "pack_location_required"
    );
    service
        .set_export_source(&id, &root.join("assets"))
        .unwrap();
    assert_eq!(service.export_source(&id).unwrap(), root.join("assets"));
    assert_eq!(service.get_component(&id).unwrap().manifests.len(), 2);
}

#[test]
#[ignore = "requires MCDH_VERIFY_SOURCE; reads the source without changing its files"]
fn verifies_local_nested_project_without_changing_manifests() {
    let root =
        PathBuf::from(std::env::var_os("MCDH_VERIFY_SOURCE").expect("set MCDH_VERIFY_SOURCE"));
    let temp = tempfile::tempdir().unwrap();
    let index = LocalIndex::open(temp.path().join("state/db")).unwrap();
    index
        .add_source(SourceKind::McsAuto, root.parent().unwrap())
        .unwrap();
    let id = index.component_id(&root).unwrap();
    let service = ComponentService::new(index.clone());
    let original = originals(&root);
    assert_eq!(
        service.export_source(&id).unwrap_err().code(),
        "pack_location_required"
    );
    service.set_export_source(&id, &root.join("src")).unwrap();
    let destination = temp.path().join("export");
    fs::create_dir(&destination).unwrap();
    let mut settings = index.app_settings().unwrap();
    settings.quick_export.regenerate_uuids = false;
    settings.quick_export.bump_version = false;
    index.set_app_settings(&settings).unwrap();
    let result = service
        .quick_export_component(
            &QuickExportRequest {
                component_id: id,
                destination,
            },
            |_| {},
        )
        .unwrap();
    let names = entries(&result.actual_path);
    assert!(names.contains(&"BP/manifest.json".into()));
    assert!(names.contains(&"RP/manifest.json".into()));
    assert!(
        !names
            .iter()
            .any(|name| name.starts_with("src/") || name.starts_with(".git/"))
    );
    assert_eq!(originals(&root), original);
}
