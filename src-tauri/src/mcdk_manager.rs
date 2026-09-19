use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use mcdh_core::{
    CoreError, LocalIndex, Result,
    mcdk::{McdkStore, McdkVersion, RELEASE_API},
};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::{CommandResult, mcdk_release::Release};

const EVENT: &str = "mcdk-status-changed";
const UPDATE_KEY: &str = "mcdk.update";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    Downloading,
    Available,
    Updated,
    Error,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct UpdateState {
    phase: UpdatePhase,
    candidate: Option<McdkVersion>,
    last_checked_at: Option<String>,
    error: Option<String>,
    downloaded_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McdkStatus {
    pub session: Option<mcdh_core::mcdk_session::McdkSession>,
    pub last_exit: Option<mcdh_core::mcdk_session::McdkExit>,
    pub current_version: Option<String>,
    pub available: bool,
    pub auto_update: bool,
    pub phase: UpdatePhase,
    pub latest_version: Option<String>,
    pub last_checked_at: Option<String>,
    pub error: Option<String>,
    pub downloaded_bytes: u64,
    pub download_size: Option<u64>,
}

#[derive(Clone)]
pub struct McdkManager {
    pub index: LocalIndex,
    pub store: McdkStore,
    pub bundled_binary: PathBuf,
}

impl McdkManager {
    pub fn new(index: LocalIndex, bundled_binary: PathBuf) -> Self {
        Self {
            store: McdkStore::new(index.clone()),
            index,
            bundled_binary,
        }
    }

    pub fn status(&self) -> Result<McdkStatus> {
        let installed = self.store.installed()?;
        let preferences = self.store.preferences()?;
        let mut update: UpdateState = self.store.read(UPDATE_KEY)?;
        // A process crash must not leave the UI permanently in a busy state.
        if matches!(
            update.phase,
            UpdatePhase::Checking | UpdatePhase::Downloading
        ) && self.store.lock("update").is_ok()
        {
            update.phase = if update.candidate.is_some() {
                UpdatePhase::Available
            } else {
                UpdatePhase::Idle
            };
        }
        let version = installed
            .current
            .filter(|v| self.store.executable(v).is_ok_and(|p| p.is_file()))
            .or_else(|| self.bundled_binary.is_file().then(McdkVersion::bundled));
        let session: mcdh_core::mcdk_session::SessionState =
            self.store.read(mcdh_core::mcdk_session::SESSION_KEY)?;
        Ok(McdkStatus {
            session: session.session,
            last_exit: session.last_exit,
            current_version: version.as_ref().map(|v| v.version.clone()),
            available: version.is_some(),
            auto_update: preferences.auto_update,
            phase: update.phase,
            latest_version: update.candidate.as_ref().map(|v| v.version.clone()),
            last_checked_at: update.last_checked_at,
            error: update.error,
            downloaded_bytes: update.downloaded_bytes,
            download_size: update.candidate.map(|v| v.size),
        })
    }

    pub fn emit(&self, app: &tauri::AppHandle) {
        if let Ok(status) = self.status() {
            let _ = app.emit(EVENT, status);
        }
    }

    fn save_update(&self, app: &tauri::AppHandle, update: &UpdateState) -> Result<()> {
        self.store.write(UPDATE_KEY, update)?;
        self.emit(app);
        Ok(())
    }

    pub async fn initialize(&self, app: &tauri::AppHandle) {
        let manager = self.clone();
        let result = blocking(move || {
            let _lock = manager.store.lock("state")?;
            manager.store.ensure_ready(&manager.bundled_binary)
        })
        .await;
        if let Err(error) = result {
            let _ = self.save_update(
                app,
                &UpdateState {
                    phase: UpdatePhase::Error,
                    error: Some(format!("内置 MCDK 不可用：{error}")),
                    ..Default::default()
                },
            );
        }
        self.emit(app);
        if self.store.preferences().is_ok_and(|p| p.auto_update) {
            let _ = self.check(app, true).await;
        }
    }

    pub async fn check(&self, app: &tauri::AppHandle, automatic: bool) -> Result<McdkStatus> {
        let _update_lock = self.store.lock("update")?;
        let preferences = self.store.preferences()?;
        if automatic && !preferences.auto_update {
            return self.status();
        }
        let generation = (automatic || preferences.auto_update).then_some(preferences.generation);
        let mut update = UpdateState {
            phase: UpdatePhase::Checking,
            last_checked_at: Some(Utc::now().to_rfc3339()),
            ..Default::default()
        };
        self.save_update(app, &update)?;
        let outcome = self.fetch_candidate().await;
        match outcome {
            Ok(candidate) => {
                update.candidate = candidate;
                update.phase = if update.candidate.is_some() {
                    UpdatePhase::Available
                } else {
                    UpdatePhase::Idle
                };
                self.save_update(app, &update)?;
                if update.candidate.is_some() && generation.is_some() {
                    self.download(app, &mut update, generation).await?;
                }
            }
            Err(error) => {
                self.fail(app, &mut update, &error)?;
                return Err(error);
            }
        }
        self.status()
    }

    async fn fetch_candidate(&self) -> Result<Option<McdkVersion>> {
        let client = http_client(Duration::from_secs(15))?;
        let response = client
            .get(RELEASE_API)
            .send()
            .await
            .map_err(network_error)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let bytes = bounded_body(response, 1024 * 1024).await?;
        let release: Release =
            serde_json::from_slice(&bytes).map_err(|e| CoreError::json("MCDK Release", e))?;
        let current = self
            .store
            .installed()?
            .current
            .or_else(|| self.bundled_binary.is_file().then(McdkVersion::bundled));
        release.candidate(current.as_ref())
    }

    pub async fn install(&self, app: &tauri::AppHandle) -> Result<McdkStatus> {
        let _update_lock = self.store.lock("update")?;
        let mut update: UpdateState = self.store.read(UPDATE_KEY)?;
        if update.candidate.is_none() {
            return Err(CoreError::InvalidInput("请先检查 MCDK 更新".into()));
        }
        self.download(app, &mut update, None).await?;
        self.status()
    }

    fn fail(
        &self,
        app: &tauri::AppHandle,
        update: &mut UpdateState,
        error: &CoreError,
    ) -> Result<()> {
        update.phase = UpdatePhase::Error;
        update.error = Some(format!("MCDK 更新失败，已保留原版本：{error}"));
        self.save_update(app, update)
    }

    async fn download(
        &self,
        app: &tauri::AppHandle,
        update: &mut UpdateState,
        generation: Option<u64>,
    ) -> Result<()> {
        let candidate = update
            .candidate
            .clone()
            .ok_or_else(|| CoreError::InvalidInput("缺少 MCDK 更新候选".into()))?;
        if !allowed(&self.store, generation)? {
            return Ok(());
        }
        update.phase = UpdatePhase::Downloading;
        update.error = None;
        update.downloaded_bytes = 0;
        self.save_update(app, update)?;
        let result = self
            .download_inner(app, update, candidate, generation)
            .await;
        match result {
            Ok(activated) => {
                update.phase = if activated {
                    UpdatePhase::Updated
                } else {
                    UpdatePhase::Available
                };
                if activated {
                    update.candidate = None;
                }
                self.save_update(app, update)
            }
            Err(error) => {
                self.fail(app, update, &error)?;
                Err(error)
            }
        }
    }

    async fn download_inner(
        &self,
        app: &tauri::AppHandle,
        update: &mut UpdateState,
        candidate: McdkVersion,
        generation: Option<u64>,
    ) -> Result<bool> {
        let client = http_client(Duration::from_secs(120))?;
        let fetch = client.get(candidate.download_url()?).send();
        let mut response = tokio::select! {
            result = fetch => result.map_err(network_error)?.error_for_status().map_err(network_error)?,
            _ = cancelled(self.store.clone(), generation) => return Ok(false),
        };
        if response
            .content_length()
            .is_some_and(|n| n != candidate.size)
        {
            return Err(CoreError::InvalidInput(
                "MCDK 下载大小与 Release 不符".into(),
            ));
        }
        let root = self.store.root();
        let mut file = tempfile::Builder::new()
            .prefix("download-")
            .tempfile_in(&root)
            .map_err(|e| CoreError::io(&root, e))?;
        let mut last_emit = std::time::Instant::now();
        loop {
            let chunk = tokio::select! {
                result = response.chunk() => result.map_err(network_error)?,
                _ = cancelled(self.store.clone(), generation) => return Ok(false),
            };
            let Some(chunk) = chunk else {
                break;
            };
            update.downloaded_bytes += chunk.len() as u64;
            if update.downloaded_bytes > candidate.size {
                return Err(CoreError::InvalidInput("MCDK 下载超过大小限制".into()));
            }
            file.write_all(&chunk)
                .map_err(|e| CoreError::io(file.path(), e))?;
            if last_emit.elapsed() >= Duration::from_millis(250) {
                self.save_update(app, update)?;
                last_emit = std::time::Instant::now();
            }
        }
        file.as_file()
            .sync_all()
            .map_err(|e| CoreError::io(file.path(), e))?;
        let store = self.store.clone();
        blocking(move || {
            let _lock = store.lock("state")?;
            if !allowed(&store, generation)? {
                return Ok(false);
            }
            store.stage(&candidate, file.path())?;
            store.activate(&candidate)?;
            Ok(true)
        })
        .await
    }
}

fn allowed(store: &McdkStore, generation: Option<u64>) -> Result<bool> {
    generation.map_or(Ok(true), |g| store.automatic_allowed(g))
}

async fn cancelled(store: McdkStore, generation: Option<u64>) {
    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if !allowed(&store, generation).unwrap_or(false) {
            return;
        }
    }
}

fn http_client(timeout: Duration) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(10))
        .user_agent(format!("MCDH/{}", mcdh_core::VERSION))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let host = attempt.url().host_str().unwrap_or("");
            if attempt.previous().len() > 5
                || attempt.url().scheme() != "https"
                || !matches!(
                    host,
                    "github.com"
                        | "api.github.com"
                        | "release-assets.githubusercontent.com"
                        | "objects.githubusercontent.com"
                )
            {
                attempt.error("Unexpected MCDK redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(network_error)
}

async fn bounded_body(response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    let mut response = response.error_for_status().map_err(network_error)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if bytes.len() + chunk.len() > limit {
            return Err(CoreError::InvalidInput("MCDK Release 响应过大".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn network_error(error: reqwest::Error) -> CoreError {
    CoreError::InvalidInput(format!("无法下载 MCDK 官方资源：{error}"))
}

pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| CoreError::InvalidInput(format!("MCDK 后台任务失败：{e}")))?
}

#[tauri::command]
pub async fn mcdk_status(manager: State<'_, McdkManager>) -> CommandResult<McdkStatus> {
    manager.status().map_err(|e| e.payload())
}

#[tauri::command]
pub async fn set_mcdk_auto_update(
    app: tauri::AppHandle,
    manager: State<'_, McdkManager>,
    enabled: bool,
) -> CommandResult<McdkStatus> {
    manager
        .store
        .set_auto_update(enabled)
        .map_err(|e| e.payload())?;
    manager.emit(&app);
    manager.status().map_err(|e| e.payload())
}

#[tauri::command]
pub async fn check_mcdk_update(
    app: tauri::AppHandle,
    manager: State<'_, McdkManager>,
) -> CommandResult<McdkStatus> {
    manager.check(&app, false).await.map_err(|e| e.payload())
}

#[tauri::command]
pub async fn install_mcdk_update(
    app: tauri::AppHandle,
    manager: State<'_, McdkManager>,
) -> CommandResult<McdkStatus> {
    manager.install(&app).await.map_err(|e| e.payload())
}

pub fn setup(app: &mut tauri::App) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let resource = app.path().resource_dir()?;
    let packaged = resource.join("release-resources/mcdk/mcdk.exe");
    let portable = resource.join("mcdk/mcdk.exe");
    let binary = if packaged.is_file() {
        packaged
    } else if portable.is_file() {
        portable
    } else if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("release-resources/mcdk/mcdk.exe")
    } else {
        packaged
    };
    let manager = McdkManager::new(app.state::<crate::AppState>().index.clone(), binary);
    app.manage(manager.clone());
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(crate::mcdk_launch::monitor(handle.clone(), manager.clone()));
    tauri::async_runtime::spawn(async move {
        manager.initialize(&handle).await;
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn automatic_download_cancels_but_manual_download_remains_allowed() {
        let temp = tempfile::tempdir().unwrap();
        let store = McdkStore::new(LocalIndex::open(temp.path().join("db")).unwrap());
        store.set_auto_update(false).unwrap();
        tokio::time::timeout(Duration::from_secs(2), cancelled(store.clone(), Some(0)))
            .await
            .unwrap();
        assert!(allowed(&store, None).unwrap());
        store.set_auto_update(true).unwrap();
        assert!(!allowed(&store, Some(0)).unwrap());
    }

    #[test]
    fn stale_download_state_recovers_and_preferences_persist_across_instances() {
        let temp = tempfile::tempdir().unwrap();
        let manager = McdkManager::new(
            LocalIndex::open(temp.path().join("db")).unwrap(),
            temp.path().join("missing.exe"),
        );
        manager
            .store
            .write(
                UPDATE_KEY,
                &UpdateState {
                    phase: UpdatePhase::Downloading,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(matches!(manager.status().unwrap().phase, UpdatePhase::Idle));
        let _lock = manager.store.lock("update").unwrap();
        assert!(matches!(
            manager.status().unwrap().phase,
            UpdatePhase::Downloading
        ));
        manager.store.set_auto_update(false).unwrap();
        let other = McdkManager::new(
            LocalIndex::open(temp.path().join("db")).unwrap(),
            temp.path().join("missing.exe"),
        );
        assert!(!other.status().unwrap().auto_update);
        assert!(other.store.lock("update").is_err());
    }

    #[tokio::test]
    async fn rejects_http_errors_and_oversized_metadata() {
        use std::io::{Read, Write};
        for (status, body) in [("200 OK", "123456"), ("429 Too Many Requests", "limited")] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0u8; 2048];
                stream.read(&mut request).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            });
            let response = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .get(format!("http://{address}"))
                .send()
                .await
                .unwrap();
            assert!(bounded_body(response, 4).await.is_err());
            server.join().unwrap();
        }
    }

    #[test]
    #[ignore = "requires pnpm mcdk:prepare"]
    fn bundled_binary_installs_offline_and_recovers_from_managed_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let store = McdkStore::new(LocalIndex::open(temp.path().join("db")).unwrap());
        let bundled =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("release-resources/mcdk/mcdk.exe");
        let _lock = store.lock("state").unwrap();
        let (version, managed) = store.ready_executable(&bundled).unwrap();
        assert_ne!(managed, bundled);
        assert_eq!(version, McdkVersion::bundled());
        std::fs::write(&managed, "corrupt").unwrap();
        assert_eq!(store.ready_executable(&bundled).unwrap().1, bundled);
    }
}
