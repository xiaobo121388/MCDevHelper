//! Persistent process identities shared by desktop launches and component mutations.

use crate::{CoreError, LocalIndex, Result, mcdk::McdkStore};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SESSION_KEY: &str = "mcdk.session";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McdkSession {
    pub component_id: String,
    pub component_path: PathBuf,
    pub executable: PathBuf,
    pub version: String,
    pub pid: u32,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McdkExit {
    pub id: String,
    pub component_id: String,
    pub exit_code: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    pub session: Option<McdkSession>,
    pub last_exit: Option<McdkExit>,
}

pub struct ProcessSnapshot {
    pub executable: PathBuf,
    pub created_at: String,
    pub exit_code: Option<u32>,
}

pub fn same_path(left: &Path, right: &Path) -> bool {
    path_key(left) == path_key(right)
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_start_matches(r"\\?\")
        .trim_end_matches('\\')
        .to_lowercase()
}

pub fn matches_process(session: &McdkSession, process: &ProcessSnapshot) -> bool {
    session.created_at == process.created_at
        && (same_path(&session.executable, &process.executable)
            || (process.exit_code.is_some() && process.executable.as_os_str().is_empty()))
}

// The component mutation lock serializes refresh, launch, and protected file changes.
pub fn refresh_session(store: &McdkStore) -> Result<SessionState> {
    let mut state: SessionState = store.read(SESSION_KEY)?;
    if let Some(session) = &state.session {
        let process = inspect_process(session.pid)?;
        let matching = process.as_ref().filter(|p| matches_process(session, p));
        if matching.is_some_and(|p| p.exit_code.is_none()) {
            return Ok(state);
        }
        state.last_exit = Some(McdkExit {
            id: format!("{}:{}", session.pid, session.created_at),
            component_id: session.component_id.clone(),
            exit_code: matching.and_then(|p| p.exit_code),
        });
        state.session = None;
        store.write(SESSION_KEY, &state)?;
    }
    Ok(state)
}

pub fn assert_component_idle(index: &LocalIndex, path: &Path) -> Result<()> {
    let state = refresh_session(&McdkStore::new(index.clone()))?;
    if let Some(session) = state.session {
        let target = path_key(&crate::path_utils::canonicalize(path)?);
        let running = path_key(&session.component_path);
        if target == running
            || target.starts_with(&(running.clone() + "\\"))
            || running.starts_with(&(target + "\\"))
        {
            return Err(CoreError::InvalidInput(
                "组件正在 MCDK 中运行，请先退出游戏再修改或移动组件".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
pub fn inspect_process(pid: u32) -> Result<Option<ProcessSnapshot>> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::{ERROR_INVALID_PARAMETER, FILETIME, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{
            GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SYNCHRONIZE, QueryFullProcessImageNameW, WaitForSingleObject,
        },
    };
    let fail = || CoreError::io("MCDK process", std::io::Error::last_os_error());
    // Each handle is owned locally; inspecting a process never terminates it.
    unsafe {
        let raw = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        );
        if raw.is_null() {
            if std::io::Error::last_os_error().raw_os_error()
                == Some(ERROR_INVALID_PARAMETER as i32)
            {
                return Ok(None);
            }
            return Err(fail());
        }
        let handle = OwnedHandle::from_raw_handle(raw);
        let raw = handle.as_raw_handle();
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        let mut times = [FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        }; 4];
        let [created, exited, kernel, user] = &mut times;
        if GetProcessTimes(raw, created, exited, kernel, user) == 0 {
            return Err(fail());
        }
        let exit_code = match WaitForSingleObject(raw, 0) {
            WAIT_TIMEOUT => None,
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if GetExitCodeProcess(raw, &mut code) == 0 {
                    return Err(fail());
                }
                Some(code)
            }
            _ => return Err(fail()),
        };
        // Windows can no longer return an image path for an exited process (ERROR_GEN_FAILURE).
        // Its retained handle and creation time still identify the exit record without PID reuse.
        let executable = if QueryFullProcessImageNameW(raw, 0, path.as_mut_ptr(), &mut length) != 0
        {
            PathBuf::from(String::from_utf16_lossy(&path[..length as usize]))
        } else if exit_code.is_some() {
            PathBuf::new()
        } else {
            return Err(fail());
        };
        Ok(Some(ProcessSnapshot {
            executable,
            created_at: (((times[0].dwHighDateTime as u64) << 32) | times[0].dwLowDateTime as u64)
                .to_string(),
            exit_code,
        }))
    }
}

#[cfg(not(windows))]
pub fn inspect_process(_pid: u32) -> Result<Option<ProcessSnapshot>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_reused_pids_and_different_executables() {
        let session = McdkSession {
            component_id: "id".into(),
            component_path: "D:/Project".into(),
            executable: "D:/Tools/mcdk.exe".into(),
            version: "1.6.1".into(),
            pid: 10,
            created_at: "100".into(),
        };
        let mut snapshot = ProcessSnapshot {
            executable: "d:\\tools\\MCDK.exe".into(),
            created_at: "100".into(),
            exit_code: None,
        };
        assert!(matches_process(&session, &snapshot));
        snapshot.created_at = "101".into();
        assert!(!matches_process(&session, &snapshot));
        snapshot.created_at = "100".into();
        snapshot.executable = "D:/Other/mcdk.exe".into();
        assert!(!matches_process(&session, &snapshot));
    }

    #[test]
    #[cfg(windows)]
    fn recovers_live_session_and_blocks_mutations_across_instances() {
        let temp = tempfile::tempdir().unwrap();
        let index = LocalIndex::open(temp.path().join("db")).unwrap();
        let store = McdkStore::new(index.clone());
        let process = inspect_process(std::process::id()).unwrap().unwrap();
        let session = McdkSession {
            component_id: "running".into(),
            component_path: temp.path().to_path_buf(),
            executable: process.executable,
            created_at: process.created_at,
            pid: std::process::id(),
            version: "1.6.1".into(),
        };
        store
            .write(
                SESSION_KEY,
                &SessionState {
                    session: Some(session.clone()),
                    last_exit: None,
                },
            )
            .unwrap();
        let other = McdkStore::new(LocalIndex::open(temp.path().join("db")).unwrap());
        assert_eq!(refresh_session(&other).unwrap().session, Some(session));
        assert!(assert_component_idle(&index, temp.path()).is_err());
        let mut state: SessionState = store.read(SESSION_KEY).unwrap();
        state.session.as_mut().unwrap().created_at = "reused".into();
        store.write(SESSION_KEY, &state).unwrap();
        assert_component_idle(&index, temp.path()).unwrap();
        assert!(refresh_session(&other).unwrap().session.is_none());
    }
}
