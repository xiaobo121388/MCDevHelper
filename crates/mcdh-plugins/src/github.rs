use std::{
    fmt,
    fs::File,
    io::Write,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{PluginError, Result, archive::MAX_DOWNLOAD, error::invalid};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub owner: String,
    pub repo: String,
}

impl fmt::Display for Repository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.repo)
    }
}

impl Repository {
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();
        let text = if input.starts_with("https://") {
            let url = Url::parse(input).map_err(invalid)?;
            if url.host_str() != Some("github.com")
                || !url.username().is_empty()
                || url.password().is_some()
                || url.port().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(invalid(
                    "Use a public GitHub repository URL without credentials, query or fragment",
                ));
            }
            url.path().trim_matches('/').to_owned()
        } else {
            input
                .strip_prefix("github:")
                .unwrap_or(input)
                .trim_end_matches('/')
                .to_owned()
        };
        let text = text.strip_suffix(".git").unwrap_or(&text);
        let parts: Vec<_> = text.split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|part| {
                part.is_empty()
                    || part.len() > 100
                    || part.starts_with('.')
                    || !part
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            })
        {
            return Err(invalid(
                "Enter owner/repo, github:owner/repo or https://github.com/owner/repo",
            ));
        }
        Ok(Self {
            owner: parts[0].to_ascii_lowercase(),
            repo: parts[1].to_ascii_lowercase(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSource {
    pub kind: String,
    pub reference: String,
    pub archive_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginProgress {
    pub phase: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(PluginError::new("cancelled", "Plugin operation cancelled"))
        } else {
            Ok(())
        }
    }
    pub(crate) async fn cancelled(&self) {
        while !self.is_cancelled() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

fn client() -> Result<Client> {
    Client::builder()
        .user_agent(format!("MCDH/{} plugins", mcdh_core::VERSION))
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5
                || attempt.url().scheme() != "https"
                || !matches!(
                    attempt.url().host_str(),
                    Some(
                        "api.github.com"
                            | "github.com"
                            | "codeload.github.com"
                            | "release-assets.githubusercontent.com"
                            | "objects.githubusercontent.com"
                    )
                )
            {
                attempt.error("Unexpected plugin download redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| PluginError::new("network_error", e))
}

fn api_url(repository: &Repository, segments: &[&str]) -> Result<Url> {
    let mut url = Url::parse("https://api.github.com/repos/").map_err(invalid)?;
    url.path_segments_mut()
        .map_err(|_| invalid("Invalid GitHub API URL"))?
        .pop_if_empty()
        .push(&repository.owner)
        .push(&repository.repo)
        .extend(segments.iter().copied());
    Ok(url)
}

fn http_error(status: StatusCode) -> PluginError {
    match status {
        StatusCode::NOT_FOUND => PluginError::new(
            "repository_not_found",
            "Public repository or artifact not found (404)",
        ),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => PluginError::new(
            "github_rate_limited",
            "GitHub denied the request or its public API rate limit was reached; retry later",
        ),
        _ => PluginError::new("network_error", format!("GitHub returned HTTP {status}")),
    }
}

async fn api(
    client: &Client,
    url: Url,
    optional: bool,
    cancel: &Cancellation,
) -> Result<Option<Value>> {
    let operation = async {
        let mut response = client
            .get(url)
            .timeout(Duration::from_secs(25))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2026-03-10")
            .send()
            .await
            .map_err(|e| PluginError::new("network_error", e))?;
        if optional && response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(http_error(response.status()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| PluginError::new("network_error", e))?
        {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(invalid("GitHub API response exceeds 2 MiB"));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(Some(serde_json::from_slice(&bytes)?))
    };
    tokio::select! { result = operation => result, () = cancel.cancelled() => Err(PluginError::new("cancelled", "Plugin download cancelled")) }
}

struct ReleaseAsset {
    url: Url,
    size: u64,
    digest: String,
    tag: String,
}

fn release_asset(repository: &Repository, release: &Value) -> Result<Option<ReleaseAsset>> {
    if release["draft"] == true || release["prerelease"] == true {
        return Ok(None);
    }
    let assets: Vec<_> = release["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|asset| asset["name"] == "mcdh-plugin.zip")
        .collect();
    if assets.is_empty() {
        return Ok(None);
    }
    if assets.len() != 1 {
        return Err(invalid("Release must contain exactly one mcdh-plugin.zip"));
    }
    let tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| invalid("Missing Release tag"))?;
    let version = semver::Version::parse(tag.trim_start_matches('v')).map_err(invalid)?;
    if !version.pre.is_empty() {
        return Err(invalid("Plugin Release is not a stable version"));
    }
    let asset = assets[0];
    let size = asset["size"]
        .as_u64()
        .filter(|size| *size > 0 && *size <= MAX_DOWNLOAD)
        .ok_or_else(|| invalid("Invalid plugin download size"))?;
    let digest = asset["digest"]
        .as_str()
        .and_then(|text| text.strip_prefix("sha256:"))
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| invalid("Plugin Release is missing a valid GitHub SHA-256 digest"))?;
    let mut expected = Url::parse("https://github.com/").map_err(invalid)?;
    expected
        .path_segments_mut()
        .map_err(|_| invalid("Invalid Release URL"))?
        .pop_if_empty()
        .push(&repository.owner)
        .push(&repository.repo)
        .extend(["releases", "download", tag, "mcdh-plugin.zip"]);
    let actual =
        Url::parse(asset["browser_download_url"].as_str().unwrap_or("")).map_err(invalid)?;
    let segments: Vec<_> = actual.path_segments().into_iter().flatten().collect();
    let tail: Vec<_> = expected
        .path_segments()
        .into_iter()
        .flatten()
        .skip(2)
        .collect();
    if actual.scheme() != "https"
        || actual.host_str() != Some("github.com")
        || !actual.username().is_empty()
        || actual.password().is_some()
        || actual.port().is_some()
        || actual.query().is_some()
        || actual.fragment().is_some()
        || segments.len() != 6
        || !segments[0].eq_ignore_ascii_case(&repository.owner)
        || !segments[1].eq_ignore_ascii_case(&repository.repo)
        || segments[2..] != tail
    {
        return Err(invalid(
            "Plugin asset URL does not belong to the selected repository Release",
        ));
    }
    Ok(Some(ReleaseAsset {
        url: actual,
        size,
        digest: digest.to_ascii_lowercase(),
        tag: tag.to_string(),
    }))
}

pub(crate) async fn download(
    repository: &Repository,
    destination: &Path,
    cancel: &Cancellation,
    progress: &(impl Fn(PluginProgress) + Send + Sync),
) -> Result<DownloadSource> {
    cancel.check()?;
    progress(PluginProgress {
        phase: "resolving".into(),
        downloaded_bytes: 0,
        total_bytes: None,
    });
    let client = client()?;
    let metadata = api(&client, api_url(repository, &[])?, false, cancel)
        .await?
        .ok_or_else(|| invalid("Missing repository"))?;
    if metadata["private"] == true {
        return Err(invalid("Private repositories are not supported"));
    }
    let release = api(
        &client,
        api_url(repository, &["releases", "latest"])?,
        true,
        cancel,
    )
    .await?;
    let asset = release
        .as_ref()
        .map(|release| release_asset(repository, release))
        .transpose()?
        .flatten();
    let (url, size, expected_hash, kind, reference) = if let Some(asset) = asset {
        (
            asset.url,
            Some(asset.size),
            Some(asset.digest),
            "release",
            asset.tag,
        )
    } else {
        let branch = metadata["default_branch"]
            .as_str()
            .ok_or_else(|| invalid("Missing default branch"))?;
        let commit = api(
            &client,
            api_url(repository, &["commits", branch])?,
            false,
            cancel,
        )
        .await?
        .ok_or_else(|| invalid("Missing default-branch commit"))?;
        let sha = commit["sha"]
            .as_str()
            .filter(|sha| sha.len() == 40 && sha.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| invalid("Invalid GitHub commit SHA"))?
            .to_owned();
        let url = Url::parse(&format!(
            "https://codeload.github.com/{}/{}/zip/{sha}",
            repository.owner, repository.repo
        ))
        .map_err(invalid)?;
        (url, None, None, "branch", sha)
    };
    let digest = download_archive(
        &client,
        url,
        destination,
        size,
        expected_hash.as_deref(),
        cancel,
        progress,
    )
    .await?;
    Ok(DownloadSource {
        kind: kind.into(),
        reference,
        archive_sha256: digest,
    })
}

async fn download_archive(
    client: &Client,
    url: Url,
    destination: &Path,
    size: Option<u64>,
    expected_hash: Option<&str>,
    cancel: &Cancellation,
    progress: &(impl Fn(PluginProgress) + Send + Sync),
) -> Result<String> {
    cancel.check()?;
    let operation = async {
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|e| PluginError::new("network_error", e))?;
        if !response.status().is_success() {
            return Err(http_error(response.status()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_DOWNLOAD || size.is_some_and(|size| size != length))
        {
            return Err(invalid("Unexpected plugin download size"));
        }
        let mut file = File::create(destination)?;
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;
        let mut last_progress = std::time::Instant::now();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| PluginError::new("network_error", e))?
        {
            downloaded += chunk.len() as u64;
            if downloaded > MAX_DOWNLOAD || size.is_some_and(|size| downloaded > size) {
                return Err(invalid("Plugin download exceeded its expected size"));
            }
            file.write_all(&chunk)?;
            hasher.update(&chunk);
            if last_progress.elapsed() >= Duration::from_millis(100) {
                progress(PluginProgress {
                    phase: "downloading".into(),
                    downloaded_bytes: downloaded,
                    total_bytes: size,
                });
                last_progress = std::time::Instant::now();
            }
        }
        file.sync_all()?;
        let digest = format!("{:x}", hasher.finalize());
        if size.is_some_and(|size| size != downloaded)
            || expected_hash.is_some_and(|hash| hash != digest)
        {
            return Err(PluginError::new(
                "integrity_error",
                "Plugin SHA-256 or size verification failed; previous version was retained",
            ));
        }
        progress(PluginProgress {
            phase: "validating".into(),
            downloaded_bytes: downloaded,
            total_bytes: Some(downloaded),
        });
        Ok(digest)
    };
    tokio::select! { result = operation => result, () = cancel.cancelled() => Err(PluginError::new("cancelled", "Plugin download cancelled")) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn response(
        status: u16,
        headers: &str,
        body: Vec<u8>,
        delay: Duration,
    ) -> (Url, std::thread::JoinHandle<()>) {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!(
            "http://{}/fixture",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let headers = headers.to_owned();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).unwrap() > 0);
            std::thread::sleep(delay);
            let _ = write!(
                socket,
                "HTTP/1.1 {status} Fixture\r\n{headers}Connection: close\r\n\r\n"
            );
            let _ = socket.write_all(&body);
        });
        (url, server)
    }

    #[tokio::test]
    async fn api_reports_http_failures_optional_release_and_oversized_metadata() {
        let client = Client::builder().no_proxy().build().unwrap();
        for (status, optional, expected) in [
            (404, false, "repository_not_found"),
            (429, false, "github_rate_limited"),
            (403, false, "github_rate_limited"),
            (503, false, "network_error"),
        ] {
            let (url, server) = response(status, "Content-Length: 0\r\n", vec![], Duration::ZERO);
            assert_eq!(
                api(&client, url, optional, &Cancellation::default())
                    .await
                    .unwrap_err()
                    .code,
                expected
            );
            server.join().unwrap();
        }
        let (url, server) = response(404, "Content-Length: 0\r\n", vec![], Duration::ZERO);
        assert!(
            api(&client, url, true, &Cancellation::default())
                .await
                .unwrap()
                .is_none()
        );
        server.join().unwrap();
        let (url, server) = response(200, "", vec![b' '; 2 * 1024 * 1024 + 1], Duration::ZERO);
        assert!(
            api(&client, url, false, &Cancellation::default())
                .await
                .unwrap_err()
                .message
                .contains("2 MiB")
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn streamed_archives_verify_size_digest_disconnect_and_cancellation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("download.zip");
        let client = Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let digest = format!("{:x}", Sha256::digest(b"archive"));
        let (url, server) = response(
            200,
            "Content-Length: 7\r\n",
            b"archive".to_vec(),
            Duration::ZERO,
        );
        assert_eq!(
            download_archive(
                &client,
                url,
                &path,
                Some(7),
                Some(&digest),
                &Cancellation::default(),
                &|_| {}
            )
            .await
            .unwrap(),
            digest
        );
        server.join().unwrap();
        let (url, server) = response(
            200,
            "Content-Length: 7\r\n",
            b"archive".to_vec(),
            Duration::ZERO,
        );
        assert_eq!(
            download_archive(
                &client,
                url,
                &path,
                Some(7),
                Some(&"0".repeat(64)),
                &Cancellation::default(),
                &|_| {}
            )
            .await
            .unwrap_err()
            .code,
            "integrity_error"
        );
        server.join().unwrap();
        let (url, server) = response(
            200,
            "Content-Length: 7\r\n",
            b"archive".to_vec(),
            Duration::ZERO,
        );
        assert!(
            download_archive(
                &client,
                url,
                &path,
                Some(8),
                None,
                &Cancellation::default(),
                &|_| {}
            )
            .await
            .unwrap_err()
            .message
            .contains("size")
        );
        server.join().unwrap();
        let (url, server) = response(
            200,
            &format!("Content-Length: {}\r\n", MAX_DOWNLOAD + 1),
            vec![],
            Duration::ZERO,
        );
        assert!(
            download_archive(
                &client,
                url,
                &path,
                None,
                None,
                &Cancellation::default(),
                &|_| {}
            )
            .await
            .unwrap_err()
            .message
            .contains("size")
        );
        server.join().unwrap();
        let (url, server) = response(
            200,
            "Content-Length: 7\r\n",
            b"cut".to_vec(),
            Duration::ZERO,
        );
        assert_eq!(
            download_archive(
                &client,
                url,
                &path,
                Some(7),
                None,
                &Cancellation::default(),
                &|_| {}
            )
            .await
            .unwrap_err()
            .code,
            "network_error"
        );
        server.join().unwrap();
        let (url, server) = response(
            200,
            "Content-Length: 7\r\n",
            b"archive".to_vec(),
            Duration::from_millis(200),
        );
        let cancel = Cancellation::default();
        let signal = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(40)).await;
            signal.cancel();
        });
        assert_eq!(
            download_archive(&client, url, &path, None, None, &cancel, &|_| {})
                .await
                .unwrap_err()
                .code,
            "cancelled"
        );
        server.join().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!(
            "http://{}/offline",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        drop(listener);
        assert_eq!(
            download_archive(
                &client,
                url,
                &path,
                None,
                None,
                &Cancellation::default(),
                &|_| {}
            )
            .await
            .unwrap_err()
            .code,
            "network_error"
        );
    }

    #[test]
    fn accepts_three_repository_forms_and_rejects_ambiguous_inputs() {
        let expected = Repository::parse("Owner/Repo").unwrap();
        for source in [
            "github:Owner/Repo",
            "https://github.com/Owner/Repo",
            "https://github.com/Owner/Repo.git/",
        ] {
            assert_eq!(Repository::parse(source).unwrap(), expected);
        }
        for source in [
            "repo",
            "https://evil.example/o/r",
            "http://github.com/o/r",
            "https://user@github.com/o/r",
            "https://github.com/o/r?x=1",
            "o/r/tree/main",
            "o/../r",
            "$(whoami)/r",
        ] {
            assert!(Repository::parse(source).is_err(), "{source}");
        }
    }

    #[test]
    fn invalid_release_assets_never_fall_back_to_branch() {
        let repository = Repository::parse("owner/repo").unwrap();
        let mut release = json!({"tag_name":"v1.0.0", "draft":false, "prerelease":false, "assets":[{
            "name":"mcdh-plugin.zip", "size":100, "digest":format!("sha256:{}", "a".repeat(64)),
            "browser_download_url":"https://github.com/owner/repo/releases/download/v1.0.0/mcdh-plugin.zip"
        }]});
        assert!(release_asset(&repository, &release).unwrap().is_some());
        release["assets"][0]["browser_download_url"] =
            json!("https://github.com/Owner/Repo/releases/download/v1.0.0/mcdh-plugin.zip");
        assert!(release_asset(&repository, &release).unwrap().is_some());
        release["assets"][0]["browser_download_url"] =
            json!("https://github.com/other/repo/releases/download/v1.0.0/mcdh-plugin.zip");
        assert!(release_asset(&repository, &release).is_err());
        release["assets"][0]["browser_download_url"] =
            json!("https://github.com/owner/repo/releases/download/v1.0.0/mcdh-plugin.zip");
        release["assets"][0]["digest"] = Value::Null;
        assert!(release_asset(&repository, &release).is_err());
        release["assets"] = json!([]);
        assert!(release_asset(&repository, &release).unwrap().is_none());
        assert_eq!(
            http_error(StatusCode::NOT_FOUND).code,
            "repository_not_found"
        );
        assert_eq!(
            http_error(StatusCode::TOO_MANY_REQUESTS).code,
            "github_rate_limited"
        );
    }
}
