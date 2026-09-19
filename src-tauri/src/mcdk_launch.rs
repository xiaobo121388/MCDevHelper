use crate::{
    CommandResult,
    mcdk_manager::{McdkManager, blocking},
};
use mcdh_core::{
    CoreError, Result,
    mcdk_session::{McdkSession, SESSION_KEY, SessionState, refresh_session},
};
use tauri::State;

#[tauri::command]
pub async fn launch_component_game(
    app: tauri::AppHandle,
    manager: State<'_, McdkManager>,
    component_id: String,
) -> CommandResult<McdkSession> {
    let manager = manager.inner().clone();
    let session = blocking(move || launch(&app, &manager, &component_id)).await;
    session.map_err(|e| e.payload())
}

fn launch(
    app: &tauri::AppHandle,
    manager: &McdkManager,
    component_id: &str,
) -> Result<McdkSession> {
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
    let (version, executable) = manager.store.ready_executable(&manager.bundled_binary)?;
    #[cfg(windows)]
    {
        let pending = windows::PendingProcess::create(&executable, &component.path)?;
        let identity = mcdh_core::mcdk_session::inspect_process(pending.pid)?
            .ok_or_else(|| CoreError::InvalidInput("MCDK 启动器已退出".into()))?;
        let session = McdkSession {
            component_id: component.id,
            component_path: component.path,
            executable: identity.executable,
            created_at: identity.created_at,
            pid: pending.pid,
            version: version.version,
        };
        manager.store.write(
            SESSION_KEY,
            &SessionState {
                session: Some(session.clone()),
                last_exit: None,
            },
        )?;
        let process = pending.resume()?;
        manager.emit(app);
        let manager = manager.clone();
        let app = app.clone();
        std::thread::spawn(move || {
            windows::wait(&process);
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
        Ok(session)
    }
    #[cfg(not(windows))]
    {
        let _ = (app, version, executable);
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

#[cfg(windows)]
mod windows {
    use mcdh_core::{CoreError, Result};
    use std::os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    };
    use std::path::Path;
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_CONSOLE, CREATE_SUSPENDED, CreateProcessW, INFINITE, PROCESS_INFORMATION,
        ResumeThread, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
    };

    pub struct PendingProcess {
        process: Option<OwnedHandle>,
        thread: OwnedHandle,
        pub pid: u32,
    }

    impl PendingProcess {
        pub fn create(executable: &Path, directory: &Path) -> Result<Self> {
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
            let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
            // Do not inherit GUI stdio handles: Windows initializes interactive handles for the new console.
            let success = unsafe {
                CreateProcessW(
                    exe.as_ptr(),
                    command.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    CREATE_NEW_CONSOLE | CREATE_SUSPENDED,
                    std::ptr::null(),
                    cwd.as_ptr(),
                    &startup,
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
