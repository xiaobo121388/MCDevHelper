use super::*;
use std::path::PathBuf;

struct Fixture {
    _root: tempfile::TempDir,
    index: LocalIndex,
    service: CustomExportService,
    source: PathBuf,
    destination: PathBuf,
    id: String,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(root.path().join("data/db")).unwrap();
        let source = root.path().join("中文 component {output_dir}");
        let destination = root.path().join("export");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::write(source.join("level.dat"), "original").unwrap();
        fs::create_dir(source.join(".empty")).unwrap();
        let id = index.component_id(&source).unwrap();
        let service = CustomExportService::new(index.clone()).unwrap();
        Self {
            _root: root,
            index,
            service,
            source,
            destination,
            id,
        }
    }

    fn profile(&self, test: &str) -> CustomExportProfile {
        let profile = CustomExportProfile {
            name: "Export test".into(),
            executable: std::env::current_exe().unwrap(),
            arguments: vec![
                "--exact".into(),
                format!("custom_export::runtime::tests::{test}"),
                "--nocapture".into(),
            ],
            ..Default::default()
        };
        ProfileStore::new(self.index.clone())
            .save(vec![profile])
            .unwrap()
            .remove(0)
    }

    fn start(&self, profile: &CustomExportProfile) -> CustomExportTask {
        self.service
            .start(
                Caller::Desktop,
                StartCustomExportRequest {
                    component_id: self.id.clone(),
                    profile_id: profile.id.clone(),
                    destination: self.destination.clone(),
                    conflict_policy: ExportConflictPolicy::Error,
                },
            )
            .unwrap()
    }

    fn wait(&self, id: &str, target: impl Fn(TaskStatus) -> bool) -> CustomExportTask {
        let begin = Instant::now();
        loop {
            let task = self.service.get(Caller::Desktop, id, 0).unwrap();
            if target(task.status) {
                return task;
            }
            assert!(
                begin.elapsed() < Duration::from_secs(20),
                "timed out: {task:?}"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn substitution_is_single_pass() {
    let values = [
        ("input_dir", "D:/a {output_dir}".into()),
        ("output_dir", "D:/out".into()),
    ];
    assert_eq!(
        substitute("--input={input_dir}:{output_dir}:{unknown}", &values),
        "--input=D:/a {output_dir}:D:/out:{unknown}"
    );
}

#[test]
#[cfg(windows)]
fn snapshot_exports_and_conflict_resolution_does_not_rerun() {
    let fixture = Fixture::new();
    let profile = fixture.profile("fixture_success");
    fs::write(fixture.destination.join("中文 artifact.custom"), "old").unwrap();
    let task = fixture.start(&profile);
    let waiting = fixture.wait(&task.id, |s| {
        s == TaskStatus::AwaitingConflict || s.terminal()
    });
    assert_eq!(waiting.status, TaskStatus::AwaitingConflict, "{waiting:?}");
    assert!(
        waiting
            .logs
            .iter()
            .any(|l| l.text.contains("fixture success"))
    );
    assert!(fixture.index.try_lock_mutations().is_ok());
    fixture
        .service
        .resolve_conflict(Caller::Desktop, &task.id, ExportConflictPolicy::Rename)
        .unwrap();
    let finished = fixture.wait(&task.id, TaskStatus::terminal);
    assert_eq!(finished.status, TaskStatus::Succeeded, "{finished:?}");
    assert_eq!(
        fs::read_to_string(fixture.source.join("level.dat")).unwrap(),
        "original"
    );
    assert_eq!(
        fs::read_to_string(fixture.destination.join("中文 artifact.custom")).unwrap(),
        "old"
    );
    assert!(
        finished
            .result
            .unwrap()
            .actual_path
            .ends_with("中文 artifact (2).custom")
    );
    let logs = fixture
        .service
        .get(Caller::Desktop, &task.id, finished.next_cursor)
        .unwrap();
    assert!(logs.logs.is_empty());
    assert_eq!(
        fixture
            .service
            .cancel(Caller::Desktop, &task.id)
            .unwrap()
            .status,
        TaskStatus::Succeeded
    );
}

#[test]
#[cfg(windows)]
fn authorization_invalid_artifact_and_failure_leave_existing_output() {
    let fixture = Fixture::new();
    let mut profile = fixture.profile("fixture_invalid");
    let request = StartCustomExportRequest {
        component_id: fixture.id.clone(),
        profile_id: profile.id.clone(),
        destination: fixture.destination.clone(),
        conflict_policy: ExportConflictPolicy::Overwrite,
    };
    assert_eq!(
        fixture
            .service
            .start(Caller::Mcp, request.clone())
            .unwrap_err()
            .code(),
        "not_authorized"
    );
    profile.allow_mcp = true;
    ProfileStore::new(fixture.index.clone())
        .save(vec![profile])
        .unwrap();
    let task = fixture.service.start(Caller::Mcp, request).unwrap();
    assert!(fixture.service.get(Caller::Desktop, &task.id, 0).is_err());
    fixture.service.shutdown();
    let profile = fixture.profile("fixture_invalid");
    let task = fixture.start(&profile);
    let finished = fixture.wait(&task.id, TaskStatus::terminal);
    assert_eq!(finished.error.unwrap().code, "invalid_artifact");
    let profile = fixture.profile("fixture_failure");
    let task = fixture.start(&profile);
    let finished = fixture.wait(&task.id, TaskStatus::terminal);
    assert_eq!(finished.error.unwrap().exit_code, Some(7));
    assert!(fs::read_dir(&fixture.destination).unwrap().next().is_none());
}

#[test]
#[cfg(windows)]
fn cancellation_timeout_and_source_lock() {
    let fixture = Fixture::new();
    let mut profile = fixture.profile("fixture_long");
    profile.input_mode = InputMode::Source;
    profile = ProfileStore::new(fixture.index.clone())
        .save(vec![profile])
        .unwrap()
        .remove(0);
    let task = fixture.start(&profile);
    fixture.wait(&task.id, |s| s == TaskStatus::Running || s.terminal());
    assert!(matches!(
        fixture.index.try_lock_mutations(),
        Err(CoreError::Busy)
    ));
    fixture.service.cancel(Caller::Desktop, &task.id).unwrap();
    fixture.service.cancel(Caller::Desktop, &task.id).unwrap();
    assert_eq!(
        fixture.wait(&task.id, TaskStatus::terminal).status,
        TaskStatus::Cancelled
    );
    assert!(fixture.index.try_lock_mutations().is_ok());
    profile.timeout_seconds = 1;
    profile = ProfileStore::new(fixture.index.clone())
        .save(vec![profile])
        .unwrap()
        .remove(0);
    let task = fixture.start(&profile);
    assert_eq!(
        fixture
            .wait(&task.id, TaskStatus::terminal)
            .error
            .unwrap()
            .code,
        "timed_out"
    );
}

#[test]
#[cfg(windows)]
fn refuses_output_inside_source_and_working_directory_errors() {
    let fixture = Fixture::new();
    let profile = fixture.profile("fixture_success");
    let request = StartCustomExportRequest {
        component_id: fixture.id.clone(),
        profile_id: profile.id,
        destination: fixture.source.clone(),
        conflict_policy: ExportConflictPolicy::Error,
    };
    assert!(fixture.service.start(Caller::Desktop, request).is_err());
}

#[test]
#[cfg(windows)]
fn cancels_descendants_and_shutdown_cleans_tasks() {
    let fixture = Fixture::new();
    let mut profile = fixture.profile("fixture_parent");
    profile.input_mode = InputMode::Source;
    profile = ProfileStore::new(fixture.index.clone())
        .save(vec![profile])
        .unwrap()
        .remove(0);
    let task = fixture.start(&profile);
    let pid_path = fixture.source.join("child.pid");
    let begin = Instant::now();
    while !pid_path.exists() {
        assert!(begin.elapsed() < Duration::from_secs(10));
        thread::sleep(Duration::from_millis(20));
    }
    let pid: u32 = fs::read_to_string(&pid_path).unwrap().parse().unwrap();
    fixture.service.shutdown();
    assert_eq!(
        fixture
            .service
            .get(Caller::Desktop, &task.id, 0)
            .unwrap()
            .status,
        TaskStatus::Cancelled
    );
    assert!(
        crate::mcdk_session::inspect_process(pid)
            .unwrap()
            .is_none_or(|process| process.exit_code.is_some())
    );
    assert!(
        fs::read_dir(fixture.service.0.workspace.root.path())
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("task-"))
    );
}

#[test]
#[cfg(windows)]
fn handles_overwrite_log_limit_and_gb18030() {
    let fixture = Fixture::new();
    let mut profile = fixture.profile("fixture_logs");
    profile.log_encoding = LogEncoding::Gb18030;
    profile = ProfileStore::new(fixture.index.clone())
        .save(vec![profile])
        .unwrap()
        .remove(0);
    let task = fixture.start(&profile);
    let result = fixture.wait(&task.id, TaskStatus::terminal);
    assert_eq!(result.status, TaskStatus::Succeeded, "{result:?}");
    assert!(result.logs_truncated);
    assert!(result.logs.iter().map(|log| log.text.len()).sum::<usize>() <= LOG_LIMIT);
    assert!(result.logs.iter().any(|log| log.text.contains("中文日志")));
    let profile = fixture.profile("fixture_success");
    fs::write(fixture.destination.join("中文 artifact.custom"), "old").unwrap();
    let task = fixture.start(&profile);
    fixture.wait(&task.id, |s| s == TaskStatus::AwaitingConflict);
    fixture
        .service
        .resolve_conflict(Caller::Desktop, &task.id, ExportConflictPolicy::Overwrite)
        .unwrap();
    assert_eq!(
        fixture.wait(&task.id, TaskStatus::terminal).status,
        TaskStatus::Succeeded
    );
    assert_eq!(
        fs::read_to_string(fixture.destination.join("中文 artifact.custom")).unwrap(),
        "artifact"
    );
}

#[test]
fn preparation_cancellation_and_expired_record_recovery() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("source"), "data").unwrap();
    let output = temp.path().join("out");
    let error = files::copy_snapshot(temp.path(), &output, &|| {
        Err(failure("cancelled", "cancel"))
    })
    .unwrap_err();
    assert_eq!(error.code(), "cancelled");
    assert!(!output.exists());
    let index = LocalIndex::open(temp.path().join("db")).unwrap();
    let first = HostWorkspace::create(&index).unwrap();
    let first_path = first.root.path().to_path_buf();
    let second = HostWorkspace::create(&index).unwrap();
    assert!(first_path.exists());
    drop(second);
    drop(first);
    assert!(!first_path.exists());
    let orphan = temp.path().join("custom-export-jobs/host-orphan");
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join("owner.lock"), "").unwrap();
    let _workspace = HostWorkspace::create(&index).unwrap();
    assert!(!orphan.exists());
}

#[test]
fn fixture_parent() {
    if std::env::var_os("MCDH_OUTPUT_DIR").is_none() {
        return;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "custom_export::runtime::tests::fixture_long",
            "--nocapture",
        ])
        .spawn()
        .unwrap();
    fs::write(
        Path::new(&std::env::var("MCDH_INPUT_DIR").unwrap()).join("child.pid"),
        child.id().to_string(),
    )
    .unwrap();
    child.wait().unwrap();
}

#[test]
fn fixture_logs() {
    use std::io::Write;
    let Ok(output) = std::env::var("MCDH_OUTPUT_DIR") else {
        return;
    };
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&vec![b'x'; LOG_LIMIT + 65536]).unwrap();
    let (bytes, _, _) = GB18030.encode("中文日志");
    stdout.write_all(&bytes).unwrap();
    stdout.flush().unwrap();
    fs::write(Path::new(&output).join("logs.bin"), "ok").unwrap();
}

#[test]
fn fixture_success() {
    let Ok(output) = std::env::var("MCDH_OUTPUT_DIR") else {
        return;
    };
    let input = PathBuf::from(std::env::var("MCDH_INPUT_DIR").unwrap());
    assert!(input.join(".empty").is_dir());
    fs::write(input.join("level.dat"), "changed in snapshot").unwrap();
    fs::write(Path::new(&output).join("中文 artifact.custom"), "artifact").unwrap();
    println!("fixture success");
    eprintln!("stderr message");
}

#[test]
fn fixture_invalid() {
    let Ok(output) = std::env::var("MCDH_OUTPUT_DIR") else {
        return;
    };
    fs::write(Path::new(&output).join("one"), "x").unwrap();
    fs::write(Path::new(&output).join("two"), "x").unwrap();
}

#[test]
fn fixture_failure() {
    if std::env::var_os("MCDH_OUTPUT_DIR").is_some() {
        std::process::exit(7);
    }
}

#[test]
fn fixture_long() {
    if std::env::var_os("MCDH_OUTPUT_DIR").is_some() {
        thread::sleep(Duration::from_secs(60));
    }
}
