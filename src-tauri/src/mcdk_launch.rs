use crate::{
    CommandResult,
    mcdk_manager::{McdkManager, blocking},
};
use mcdh_core::{
    CoreError, Result,
    mcdk_session::{McdkSession, SESSION_KEY, SessionMode, SessionState, refresh_session},
};
use tauri::State;

#[tauri::command]
pub async fn launch_component_game(
    app: tauri::AppHandle,
    manager: State<'_, McdkManager>,
    component_id: String,
) -> CommandResult<McdkSession> {
    let manager = manager.inner().clone();
    let session = blocking(move || launch(&app, &manager, &component_id, SessionMode::Game)).await;
    session.map_err(|e| e.payload())
}

#[tauri::command]
pub async fn launch_component_editor(
    app: tauri::AppHandle,
    manager: State<'_, McdkManager>,
    component_id: String,
) -> CommandResult<McdkSession> {
    let manager = manager.inner().clone();
    blocking(move || launch(&app, &manager, &component_id, SessionMode::Editor))
        .await
        .map_err(|e| e.payload())
}

pub(crate) fn launch(
    app: &tauri::AppHandle,
    manager: &McdkManager,
    component_id: &str,
    mode: SessionMode,
) -> Result<McdkSession> {
    let started = start_session(manager, component_id, mode)?;
    manager.emit(app);
    #[cfg(windows)]
    {
        let manager = manager.clone();
        let app = app.clone();
        std::thread::spawn(move || {
            windows::wait(&started.process);
            // Keep the handle until the exit code has been persisted, even if a component operation is busy.
            loop {
                match manager.index.try_lock_mutations() {
                    Ok(_lock) => {
                        let _ = refresh_session(&manager.store);
                        manager.emit(&app);
                        break;
                    }
                    Err(CoreError::Busy) => {
                        std::thread::sleep(std::time::Duration::from_millis(200))
                    }
                    Err(_) => break,
                }
            }
        });
    }
    Ok(started.session)
}

struct StartedSession {
    session: McdkSession,
    #[cfg(windows)]
    process: std::os::windows::io::OwnedHandle,
}

fn start_session(
    manager: &McdkManager,
    component_id: &str,
    mode: SessionMode,
) -> Result<StartedSession> {
    let _plugin = if mode == SessionMode::Editor {
        Some(
            mcdh_plugins::PluginService::new(manager.index.clone())
                .and_then(|service| service.mcde_lease(None))
                .map_err(|error| CoreError::InvalidInput(format!("MCDE 插件不可用：{error}")))?,
        )
    } else {
        None
    };
    let _component_lock = manager.index.try_lock_mutations()?;
    let _state_lock = manager.store.lock("state")?;
    if refresh_session(&manager.store)?.session.is_some() {
        return Err(CoreError::InvalidInput(
            "已有 MCDK 会话正在运行，请先退出游戏".into(),
        ));
    }
    let component = manager.store.launch_target(component_id)?;
    if !component.path.is_dir() {
        return Err(CoreError::NotFound(component.path));
    }
    let editor = if mode == SessionMode::Editor {
        Some(
            mcdh_core::mcde::McdeStore::new(manager.store.clone()).prepare_launch(
                &component.path,
                &manager.bundled_editor,
                &manager.mcde_host,
            )?,
        )
    } else {
        None
    };
    let (version, executable, directory) = if let Some(editor) = &editor {
        (
            editor.version.clone(),
            editor.executable.clone(),
            editor.directory().to_path_buf(),
        )
    } else {
        let (version, executable) = manager.store.ready_executable(&manager.bundled_binary)?;
        (version.version, executable, component.path.clone())
    };
    #[cfg(windows)]
    {
        let pending = windows::PendingProcess::create_with_visibility(
            &executable,
            &directory,
            mode == SessionMode::Editor,
        )?;
        let identity = mcdh_core::mcdk_session::inspect_process(pending.pid)?
            .ok_or_else(|| CoreError::InvalidInput("MCDK 启动器已退出".into()))?;
        let session = McdkSession {
            component_id: component.id,
            component_path: component.path,
            executable: identity.executable,
            created_at: identity.created_at,
            pid: pending.pid,
            version,
            mode,
            launch_directory: editor.as_ref().map(|e| e.directory().to_path_buf()),
        };
        manager.store.write(
            SESSION_KEY,
            &SessionState {
                session: Some(session.clone()),
                last_exit: None,
            },
        )?;
        let process = pending.resume()?;
        if let Some(editor) = editor {
            editor.persist();
        }
        Ok(StartedSession { session, process })
    }
    #[cfg(not(windows))]
    {
        let _ = (version, executable, directory);
        Err(CoreError::InvalidInput(
            "MCDK 启动仅支持 Windows x64".into(),
        ))
    }
}

pub async fn monitor(app: tauri::AppHandle, manager: McdkManager) {
    let mut previous = None;
    loop {
        let copy = manager.clone();
        let result = blocking(move || {
            let _lock = copy.index.try_lock_mutations()?;
            refresh_session(&copy.store)
        })
        .await;
        if result.is_ok()
            && let Ok(status) = manager.status()
            && previous.as_ref() != Some(&status)
        {
            manager.emit(&app);
            previous = Some(status);
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[cfg(all(test, windows))]
#[path = "mcde_native_tests.rs"]
mod native_tests;

#[cfg(windows)]
mod windows {
    use mcdh_core::{CoreError, Result};
    use std::os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use std::path::Path;
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_CONSOLE, CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
        EXTENDED_STARTUPINFO_PRESENT, INFINITE, InitializeProcThreadAttributeList,
        LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
        ResumeThread, STARTF_USESHOWWINDOW, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW,
        TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
    };

    struct LaunchLogs {
        files: [std::fs::File; 3],
        _handles: Box<[windows_sys::Win32::Foundation::HANDLE; 3]>,
        attributes: LPPROC_THREAD_ATTRIBUTE_LIST,
        _storage: Vec<usize>,
    }

    impl LaunchLogs {
        fn create(directory: &Path) -> Result<Self> {
            let output = |name: &str| {
                let path = directory.join(name);
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|e| CoreError::io(path, e))
            };
            let files = [
                std::fs::File::open("NUL").map_err(|e| CoreError::io("NUL", e))?,
                output("host.stdout.log")?,
                output("host.stderr.log")?,
            ];
            let mut handles = Box::new(files.each_ref().map(AsRawHandle::as_raw_handle));
            for handle in handles.iter().copied() {
                if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) }
                    == 0
                {
                    return Err(CoreError::io(directory, std::io::Error::last_os_error()));
                }
            }
            let mut size = 0;
            unsafe {
                InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
            }
            let mut storage = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
            let attributes = storage.as_mut_ptr().cast();
            if unsafe { InitializeProcThreadAttributeList(attributes, 1, 0, &mut size) } == 0 {
                return Err(CoreError::io(directory, std::io::Error::last_os_error()));
            }
            if unsafe {
                UpdateProcThreadAttribute(
                    attributes,
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    handles.as_mut_ptr().cast(),
                    std::mem::size_of_val(handles.as_ref()),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            } == 0
            {
                let error = std::io::Error::last_os_error();
                unsafe {
                    DeleteProcThreadAttributeList(attributes);
                }
                return Err(CoreError::io(directory, error));
            }
            Ok(Self {
                files,
                _handles: handles,
                attributes,
                _storage: storage,
            })
        }
    }

    impl Drop for LaunchLogs {
        fn drop(&mut self) {
            unsafe {
                DeleteProcThreadAttributeList(self.attributes);
            }
        }
    }

    pub struct PendingProcess {
        process: Option<OwnedHandle>,
        thread: OwnedHandle,
        pub pid: u32,
    }

    impl PendingProcess {
        #[cfg(test)]
        pub fn create(executable: &Path, directory: &Path) -> Result<Self> {
            Self::create_with_visibility(executable, directory, false)
        }

        pub fn create_with_visibility(
            executable: &Path,
            directory: &Path,
            hidden: bool,
        ) -> Result<Self> {
            let exe: Vec<u16> = executable
                .as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect();
            let cwd: Vec<u16> = directory.as_os_str().encode_wide().chain(Some(0)).collect();
            let mut command: Vec<u16> = format!("\"{}\"", executable.display())
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let logs = hidden.then(|| LaunchLogs::create(directory)).transpose()?;
            let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
            startup.StartupInfo.cb = if hidden {
                std::mem::size_of::<STARTUPINFOEXW>()
            } else {
                std::mem::size_of::<STARTUPINFOW>()
            } as u32;
            if let Some(logs) = &logs {
                startup.StartupInfo.dwFlags = STARTF_USESHOWWINDOW | STARTF_USESTDHANDLES;
                startup.StartupInfo.wShowWindow = 0;
                startup.StartupInfo.hStdInput = logs.files[0].as_raw_handle();
                startup.StartupInfo.hStdOutput = logs.files[1].as_raw_handle();
                startup.StartupInfo.hStdError = logs.files[2].as_raw_handle();
                startup.lpAttributeList = logs.attributes;
            }
            let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
            // Ordinary launches get an interactive console; editor launches inherit only the owned log handles.
            let success = unsafe {
                CreateProcessW(
                    exe.as_ptr(),
                    command.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    i32::from(hidden),
                    CREATE_NEW_CONSOLE
                        | CREATE_SUSPENDED
                        | if hidden {
                            EXTENDED_STARTUPINFO_PRESENT
                        } else {
                            0
                        },
                    std::ptr::null(),
                    cwd.as_ptr(),
                    &startup.StartupInfo,
                    &mut info,
                )
            };
            if success == 0 {
                return Err(CoreError::io(executable, std::io::Error::last_os_error()));
            }
            Ok(Self {
                process: Some(unsafe { OwnedHandle::from_raw_handle(info.hProcess) }),
                thread: unsafe { OwnedHandle::from_raw_handle(info.hThread) },
                pid: info.dwProcessId,
            })
        }

        pub fn resume(mut self) -> Result<OwnedHandle> {
            if unsafe { ResumeThread(self.thread.as_raw_handle()) } == u32::MAX {
                return Err(CoreError::io(
                    "MCDK process",
                    std::io::Error::last_os_error(),
                ));
            }
            Ok(self.process.take().expect("pending process handle"))
        }
    }

    impl Drop for PendingProcess {
        fn drop(&mut self) {
            // Only roll back a not-yet-started launcher if recording or resuming failed.
            if let Some(process) = &self.process {
                unsafe {
                    TerminateProcess(process.as_raw_handle(), 1);
                    WaitForSingleObject(process.as_raw_handle(), INFINITE);
                }
            }
        }
    }

    pub fn wait(process: &OwnedHandle) {
        unsafe {
            WaitForSingleObject(process.as_raw_handle(), INFINITE);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reports_missing_executable_without_creating_process() {
            let temp = tempfile::tempdir().unwrap();
            assert!(PendingProcess::create(&temp.path().join("missing.exe"), temp.path()).is_err());
        }

        #[test]
        #[ignore = "runs an isolated hidden console probe; requires rustc"]
        fn hidden_editor_host_redirects_logs_and_survives_handle_release() {
            use mcdh_core::mcdk_session::inspect_process;
            use std::{
                fs,
                process::Command,
                time::{Duration, Instant},
            };
            let temp = tempfile::tempdir().unwrap();
            let exe = temp.path().join("hidden probe.exe");
            assert!(
                Command::new("rustc")
                    .arg("--edition=2024")
                    .arg(
                        Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/fixtures/launch_probe.rs")
                    )
                    .arg("-o")
                    .arg(&exe)
                    .status()
                    .unwrap()
                    .success()
            );
            let pending = PendingProcess::create_with_visibility(&exe, temp.path(), true).unwrap();
            let pid = pending.pid;
            let process = pending.resume().unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !temp.path().join("probe-state.txt").is_file() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            let content = fs::read_to_string(temp.path().join("probe-state.txt")).unwrap();
            assert!(content.ends_with("\nfalse\nfalse"), "{content}");
            drop(process);
            assert!(inspect_process(pid).unwrap().unwrap().exit_code.is_none());
            let handle = unsafe {
                OwnedHandle::from_raw_handle(windows_sys::Win32::System::Threading::OpenProcess(
                    windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION
                        | windows_sys::Win32::System::Threading::PROCESS_SYNCHRONIZE,
                    0,
                    pid,
                ))
            };
            assert!(!handle.as_raw_handle().is_null());
            fs::write(temp.path().join("probe-stop"), "stop").unwrap();
            wait(&handle);
            assert_eq!(inspect_process(pid).unwrap().unwrap().exit_code, Some(7));
            assert!(
                fs::read_to_string(temp.path().join("host.stdout.log"))
                    .unwrap()
                    .contains("MCDH isolated launch probe")
            );
            assert_eq!(
                fs::read_to_string(temp.path().join("host.stderr.log")).unwrap(),
                ""
            );
        }

        #[test]
        #[ignore = "opens an isolated interactive console; requires rustc"]
        fn interactive_console_handles_unicode_cwd_and_survives_handle_release() {
            use mcdh_core::mcdk_session::inspect_process;
            use std::{
                fs,
                process::Command,
                time::{Duration, Instant},
            };
            let temp = tempfile::tempdir().unwrap();
            let cwd = temp.path().join("中文 目录 & (probe) % !");
            fs::create_dir(&cwd).unwrap();
            let exe = cwd.join("launch probe.exe");
            let fixture =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/launch_probe.rs");
            assert!(
                Command::new("rustc")
                    .arg("--edition=2024")
                    .arg(fixture)
                    .arg("-o")
                    .arg(&exe)
                    .status()
                    .unwrap()
                    .success()
            );
            let pending = PendingProcess::create(&exe, &cwd).unwrap();
            let pid = pending.pid;
            let process = pending.resume().unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !cwd.join("probe-state.txt").is_file() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            let content = fs::read_to_string(cwd.join("probe-state.txt")).unwrap();
            assert!(content.ends_with("\ntrue\ntrue"), "{content}");
            assert!(content.starts_with(&cwd.to_string_lossy().to_string()));
            drop(process);
            assert!(inspect_process(pid).unwrap().unwrap().exit_code.is_none());
            // Retain a query handle so Windows keeps the exit code available after the probe exits.
            let handle = unsafe {
                OwnedHandle::from_raw_handle(windows_sys::Win32::System::Threading::OpenProcess(
                    windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION
                        | windows_sys::Win32::System::Threading::PROCESS_SYNCHRONIZE,
                    0,
                    pid,
                ))
            };
            fs::write(cwd.join("probe-stop"), "stop").unwrap();
            wait(&handle);
            assert_eq!(inspect_process(pid).unwrap().unwrap().exit_code, Some(7));
        }
    }
}
