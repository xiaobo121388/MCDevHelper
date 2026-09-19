use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{Manager, ipc::Channel};

use crate::{CommandResult, GitHubRelease, update_error};

const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;
const MAX_EXPANDED: u64 = 1024 * 1024 * 1024;
const WORKER: &str = include_str!("update-worker.ps1");
static UPDATING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Asset {
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum InstallKind {
    Installed,
    Portable,
}

impl InstallKind {
    fn detect(executable: &Path) -> Result<Self, String> {
        let name = executable
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("");
        if name.eq_ignore_ascii_case("MCDH.exe") {
            Ok(Self::Portable)
        } else if name.eq_ignore_ascii_case("mcdh-desktop.exe")
            && executable.with_file_name("uninstall.exe").is_file()
        {
            Ok(Self::Installed)
        } else {
            Err("当前不是受支持的安装版或便携版，开发构建不能执行本体更新。".into())
        }
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct Progress {
    phase: &'static str,
    downloaded_bytes: u64,
    total_bytes: u64,
}

fn progress(
    channel: &Channel<Progress>,
    phase: &'static str,
    downloaded_bytes: u64,
    total_bytes: u64,
) {
    let _ = channel.send(Progress {
        phase,
        downloaded_bytes,
        total_bytes,
    });
}

struct UpdateGuard;
impl UpdateGuard {
    fn acquire() -> Result<Self, String> {
        UPDATING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "已有本体更新正在进行。".to_string())?;
        Ok(Self)
    }
}
impl Drop for UpdateGuard {
    fn drop(&mut self) {
        UPDATING.store(false, Ordering::SeqCst);
    }
}

pub(crate) fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(15))
        .user_agent(format!("MCDH/{}", mcdh_core::VERSION))
        .https_only(true)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5
                || attempt.url().scheme() != "https"
                || !matches!(
                    attempt.url().host_str(),
                    Some(
                        "github.com"
                            | "api.github.com"
                            | "release-assets.githubusercontent.com"
                            | "objects.githubusercontent.com"
                    )
                )
            {
                attempt.error("Unexpected update redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())
}

pub(crate) async fn latest_release() -> Result<Option<GitHubRelease>, String> {
    let response = client(Duration::from_secs(15))?
        .get(crate::LATEST_RELEASE_API)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", crate::GITHUB_API_VERSION)
        .send()
        .await
        .map_err(|e| format!("无法连接 GitHub：{e}"))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let mut response = response.error_for_status().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("Release 响应过大".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| format!("Release 数据无法读取：{e}"))
}

fn candidate(
    release: &GitHubRelease,
    expected_version: &str,
    kind: InstallKind,
) -> Result<Asset, String> {
    if release.tag_name != expected_version {
        return Err("最新版本已变化，请重新检查更新。".into());
    }
    let version = semver::Version::parse(expected_version.trim_start_matches(['v', 'V']))
        .map_err(|_| "Release 版本号无效")?;
    if release.draft
        || release.prerelease
        || !version.pre.is_empty()
        || !version.build.is_empty()
        || !crate::release_is_newer(mcdh_core::VERSION, expected_version)
    {
        return Err("只能安装比当前版本更新的正式版本。".into());
    }
    let suffix = if kind == InstallKind::Installed {
        "setup.exe"
    } else {
        "portable.zip"
    };
    let name = format!("MCDH-{version}-windows-x64-{suffix}");
    let assets: Vec<_> = release.assets.iter().filter(|a| a.name == name).collect();
    if assets.len() != 1 {
        return Err(format!("Release 缺少唯一的 {name}，请等待发布完成。"));
    }
    let asset = assets[0];
    let expected_url = format!(
        "https://github.com/xiaobo121388/MCDevHelper/releases/download/{expected_version}/{name}"
    );
    if asset.browser_download_url != expected_url || asset.size == 0 || asset.size > MAX_DOWNLOAD {
        return Err("更新包来源或大小无效，已拒绝更新。".into());
    }
    digest(asset)?;
    Ok(asset.clone())
}

fn digest(asset: &Asset) -> Result<&str, String> {
    asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| "Release 缺少有效 SHA-256 摘要，已拒绝更新。".into())
}

fn verify(asset: &Asset, size: u64, hash: &str) -> Result<(), String> {
    if size != asset.size || !hash.eq_ignore_ascii_case(digest(asset)?) {
        Err("更新包大小或 SHA-256 校验失败，已保留原版本。".into())
    } else {
        Ok(())
    }
}

async fn download(asset: &Asset, path: &Path, channel: &Channel<Progress>) -> Result<(), String> {
    let mut response = client(Duration::from_secs(1800))?
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    if response.content_length().is_some_and(|n| n != asset.size) {
        return Err("更新包大小不符".into());
    }
    let mut file = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut size = 0;
    let mut last_emit = Instant::now();
    progress(channel, "downloading", size, asset.size);
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        size += chunk.len() as u64;
        if size > asset.size {
            return Err("更新包超过预期大小".into());
        }
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        hasher.update(&chunk);
        if last_emit.elapsed() > Duration::from_millis(150) {
            progress(channel, "downloading", size, asset.size);
            last_emit = Instant::now();
        }
    }
    file.sync_all().map_err(|e| e.to_string())?;
    progress(channel, "verifying", size, asset.size);
    verify(asset, size, &format!("{:x}", hasher.finalize()))
}

// Only release-owned paths may be replaced. Unknown/user files are never swept away.
fn portable_path(name: &str) -> Result<PathBuf, String> {
    if name.contains('\\') || name.contains(':') {
        return Err("更新压缩包路径无效".into());
    }
    let path = Path::new(name);
    if path
        .components()
        .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("更新压缩包包含越界路径".into());
    }
    let allowed = matches!(
        name,
        "MCDH.exe" | "mcdh-mcp.exe" | "LICENSE" | "README.md" | "THIRD_PARTY_LICENSES.md"
    ) || name.starts_with("mcdk/");
    if !allowed
        || path.components().any(|p| {
            let text = p.as_os_str().to_string_lossy();
            let stem = text.split('.').next().unwrap_or("").to_ascii_uppercase();
            text.ends_with(['.', ' '])
                || text.is_empty()
                || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.len() == 4
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        })
    {
        return Err(format!("更新压缩包包含非发布文件：{name}"));
    }
    Ok(path.to_path_buf())
}

fn extract_portable(archive: &Path, output: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if zip.len() > 4096 {
        return Err("更新压缩包文件数量超限".into());
    }
    let mut total = 0u64;
    let mut names = std::collections::HashSet::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let relative = portable_path(entry.name())?;
        if entry.is_symlink() || !names.insert(entry.name().to_ascii_lowercase()) {
            return Err("更新压缩包包含链接或重复路径".into());
        }
        total = total.checked_add(entry.size()).ok_or("解压大小溢出")?;
        if total > MAX_EXPANDED {
            return Err("更新压缩包解压大小超限".into());
        }
        let target = output.join(relative);
        fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|e| e.to_string())?;
        let expected = entry.size();
        let count = std::io::copy(&mut entry.by_ref().take(expected + 1), &mut file)
            .map_err(|e| e.to_string())?;
        if count != expected {
            return Err("更新文件解压大小不符".into());
        }
        file.sync_all().map_err(|e| e.to_string())?;
    }
    for required in [
        "MCDH.exe",
        "mcdh-mcp.exe",
        "mcdk/mcdk.exe",
        "mcdk/bundled.json",
    ] {
        if !output.join(required).is_file() {
            return Err(format!("更新压缩包缺少 {required}"));
        }
    }
    for executable in ["MCDH.exe", "mcdh-mcp.exe", "mcdk/mcdk.exe"] {
        verify_executable(&output.join(executable), false)?;
    }
    Ok(())
}

fn verify_executable(path: &Path, installer: bool) -> Result<(), String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let invalid = || format!("更新文件不是有效的 Windows 可执行程序：{}", path.display());
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos).map_err(|_| invalid())?;
    if &dos[..2] != b"MZ" {
        return Err(invalid());
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    if offset < 64 || offset + 26 > file.metadata().map_err(|e| e.to_string())?.len() {
        return Err(invalid());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut pe = [0u8; 26];
    file.read_exact(&mut pe).map_err(|_| invalid())?;
    let x64 = pe[4..6] == [0x64, 0x86] && pe[24..26] == [0x0b, 0x02];
    let x86 = pe[4..6] == [0x4c, 0x01] && pe[24..26] == [0x0b, 0x01];
    if &pe[..4] != b"PE\0\0" || !(x64 || installer && x86) || pe[22] & 2 == 0 || pe[23] & 0x20 != 0
    {
        return Err(invalid());
    }
    Ok(())
}

#[derive(Serialize)]
struct Job {
    parent_pid: u32,
    executable: PathBuf,
    kind: InstallKind,
    result_path: PathBuf,
    package_sha256: String,
}

#[tauri::command]
pub(crate) async fn install_app_update(
    app: tauri::AppHandle,
    version: String,
    on_progress: Channel<Progress>,
) -> CommandResult<()> {
    let guard = UpdateGuard::acquire().map_err(update_error)?;
    prepare_update(&app, &version, &on_progress)
        .await
        .map_err(update_error)?;
    // Keep concurrent requests blocked until the event loop completes shutdown.
    std::mem::forget(guard);
    app.exit(0);
    Ok(())
}

async fn prepare_update(
    app: &tauri::AppHandle,
    version: &str,
    channel: &Channel<Progress>,
) -> Result<(), String> {
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) || cfg!(debug_assertions) {
        return Err("本体自动更新仅支持 Windows x64 正式发布版本。".into());
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let kind = InstallKind::detect(&executable)?;
    let target = executable.parent().ok_or("应用目录无效")?;
    progress(channel, "checking", 0, 0);
    let release = latest_release().await?.ok_or("未找到公开 Release")?;
    let asset = candidate(&release, version, kind)?;
    let work = tempfile::Builder::new()
        .prefix(".mcdh-update-")
        .tempdir_in(target)
        .map_err(|e| format!("应用目录不可写，无法原地更新：{e}"))?;
    let package = work.path().join(if kind == InstallKind::Installed {
        "setup.exe"
    } else {
        "portable.zip"
    });
    download(&asset, &package, channel).await?;
    if kind == InstallKind::Portable {
        let output = work.path().join("staged");
        tauri::async_runtime::spawn_blocking(move || extract_portable(&package, &output))
            .await
            .map_err(|e| e.to_string())??;
    } else {
        verify_executable(&package, true)?;
    }
    let result_dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&result_dir).map_err(|e| e.to_string())?;
    let result_path = result_dir.join("last-update-error.txt");
    if result_path.exists() {
        fs::remove_file(&result_path).map_err(|e| e.to_string())?;
    }
    let job = Job {
        parent_pid: std::process::id(),
        executable,
        kind,
        result_path,
        package_sha256: digest(&asset)?.into(),
    };
    fs::write(
        work.path().join("job.json"),
        serde_json::to_vec(&job).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let script = work.path().join("worker.ps1");
    fs::write(&script, WORKER).map_err(|e| e.to_string())?;
    start_worker(&script, work.path()).await?;
    // The helper owns this directory after acknowledging it has a handle to our process.
    let _ = work.keep();
    progress(channel, "installing", asset.size, asset.size);
    Ok(())
}

async fn start_worker(script: &Path, work: &Path) -> Result<(), String> {
    let system = std::env::var_os("SystemRoot").ok_or("SystemRoot 未设置")?;
    let shell = PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = std::process::Command::new(shell);
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("无法启动更新辅助进程：{e}"))?;
    for _ in 0..150 {
        if work.join("ready").is_file() {
            return Ok(());
        }
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            return Err("更新辅助进程未能就绪，已保留原版本。".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("更新辅助进程启动超时，已保留原版本。".into())
}

#[tauri::command]
pub(crate) fn app_update_error(
    app: tauri::AppHandle,
    clear: bool,
) -> CommandResult<Option<String>> {
    let path = app
        .path()
        .app_local_data_dir()
        .map_err(|e| update_error(e.to_string()))?
        .join("last-update-error.txt");
    match fs::read_to_string(&path) {
        Ok(text) => {
            if clear {
                fs::remove_file(path).map_err(|e| update_error(e.to_string()))?;
            }
            Ok(Some(text))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(update_error(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture() -> Value {
        json!({"tag_name":"v2.0.0", "html_url":"https://github.com/xiaobo121388/MCDevHelper/releases/tag/v2.0.0",
        "draft":false, "prerelease":false, "assets":[{
            "name":"MCDH-2.0.0-windows-x64-portable.zip", "size":1024,
            "digest":format!("sha256:{}", "a".repeat(64)),
            "browser_download_url":"https://github.com/xiaobo121388/MCDevHelper/releases/download/v2.0.0/MCDH-2.0.0-windows-x64-portable.zip"
        }]})
    }

    fn select(value: Value) -> Result<Asset, String> {
        candidate(
            &serde_json::from_value(value).unwrap(),
            "v2.0.0",
            InstallKind::Portable,
        )
    }

    #[test]
    fn requires_new_stable_exact_official_asset_with_digest() {
        assert!(select(fixture()).is_ok());
        for (field, value) in [
            ("draft", json!(true)),
            ("prerelease", json!(true)),
            ("tag_name", json!("v3.0.0")),
        ] {
            let mut data = fixture();
            data[field] = value;
            assert!(select(data).is_err(), "{field}");
        }
        for (field, value) in [
            ("digest", Value::Null),
            ("digest", json!("sha256:bad")),
            ("size", json!(0)),
            ("size", json!(MAX_DOWNLOAD + 1)),
            (
                "browser_download_url",
                json!("https://evil.invalid/update.zip"),
            ),
            ("name", json!("other.zip")),
        ] {
            let mut data = fixture();
            data["assets"][0][field] = value;
            assert!(select(data).is_err(), "{field}");
        }
        let mut data = fixture();
        let duplicate = data["assets"][0].clone();
        data["assets"].as_array_mut().unwrap().push(duplicate);
        assert!(select(data).is_err());
        let release: GitHubRelease = serde_json::from_value(fixture()).unwrap();
        assert!(candidate(&release, "v2.0.0", InstallKind::Installed).is_err());
    }

    #[test]
    fn refuses_corrupt_and_truncated_downloads() {
        let asset = select(fixture()).unwrap();
        assert!(verify(&asset, 1024, &"a".repeat(64)).is_ok());
        assert!(verify(&asset, 1023, &"a".repeat(64)).is_err());
        assert!(verify(&asset, 1024, &"b".repeat(64)).is_err());
    }

    #[test]
    fn rejects_unsafe_and_non_release_paths() {
        for path in [
            "../MCDH.exe",
            "/MCDH.exe",
            "C:/MCDH.exe",
            "mcdk/../../config.json",
            "mcdk/file:stream",
            "mcdk/file.",
            "mcdk/file ",
            "settings.json",
            "mcdk\\..\\MCDH.exe",
            "mcdk/CON",
            "mcdk/LPT1.txt",
        ] {
            assert!(portable_path(path).is_err(), "{path}");
        }
        assert!(portable_path("mcdk/licenses/LICENSE").is_ok());
        assert!(portable_path("MCDH.exe").is_ok());
    }

    fn archive(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn extracts_complete_packages_and_rejects_missing_or_escaping_files() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("portable.zip");
        let exe = pe_fixture();
        let entries: Vec<(&str, &[u8])> = vec![
            ("MCDH.exe", &exe),
            ("mcdh-mcp.exe", &exe),
            ("mcdk/mcdk.exe", &exe),
            ("mcdk/bundled.json", b"{}"),
        ];
        archive(&package, &entries);
        extract_portable(&package, &dir.path().join("good")).unwrap();
        assert_eq!(fs::read(dir.path().join("good/MCDH.exe")).unwrap(), exe);
        archive(&package, &entries[..1]);
        assert!(extract_portable(&package, &dir.path().join("missing")).is_err());
        archive(&package, &[("../escape.exe", b"bad")]);
        assert!(extract_portable(&package, &dir.path().join("unsafe")).is_err());
        assert!(!dir.path().join("escape.exe").exists());
        archive(&package, &[("mcdk/file", b"a"), ("mcdk/FILE", b"b")]);
        assert!(extract_portable(&package, &dir.path().join("duplicates")).is_err());
    }

    fn pe_fixture() -> Vec<u8> {
        let mut bytes = vec![0; 128];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&[0x64, 0x86]);
        bytes[86] = 2;
        bytes[88..90].copy_from_slice(&[0x0b, 0x02]);
        bytes
    }

    #[test]
    fn rejects_invalid_executables_before_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.exe");
        fs::write(&path, pe_fixture()).unwrap();
        verify_executable(&path, false).unwrap();
        fs::write(&path, b"not an executable").unwrap();
        assert!(verify_executable(&path, true).is_err());
        let mut bytes = pe_fixture();
        bytes[68..70].copy_from_slice(&[0x4c, 0x01]);
        bytes[88..90].copy_from_slice(&[0x0b, 0x01]);
        fs::write(&path, bytes).unwrap();
        assert!(verify_executable(&path, false).is_err());
        verify_executable(&path, true).unwrap();
    }

    #[test]
    fn distinguishes_installations_and_blocks_parallel_requests() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            InstallKind::detect(&dir.path().join("MCDH.exe")).unwrap(),
            InstallKind::Portable
        );
        assert!(InstallKind::detect(&dir.path().join("mcdh-desktop.exe")).is_err());
        fs::write(dir.path().join("uninstall.exe"), b"uninstaller").unwrap();
        assert_eq!(
            InstallKind::detect(&dir.path().join("mcdh-desktop.exe")).unwrap(),
            InstallKind::Installed
        );
        let guard = UpdateGuard::acquire().unwrap();
        assert!(UpdateGuard::acquire().is_err());
        drop(guard);
        assert!(UpdateGuard::acquire().is_ok());
    }
}
