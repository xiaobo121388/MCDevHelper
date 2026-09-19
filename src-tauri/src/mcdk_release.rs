use mcdh_core::{CoreError, Result, mcdk::McdkVersion};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

impl Release {
    pub fn candidate(self, current: Option<&McdkVersion>) -> Result<Option<McdkVersion>> {
        let invalid = |text: &str| CoreError::InvalidInput(text.into());
        if self.draft || self.prerelease {
            return Ok(None);
        }
        let parsed = semver::Version::parse(self.tag_name.trim_start_matches('v'))
            .map_err(|_| invalid("MCDK Release 版本号无效"))?;
        if !parsed.pre.is_empty() || !parsed.build.is_empty() {
            return Ok(None);
        }
        if current.is_some_and(|v| v.validate().is_ok_and(|v| v >= parsed)) {
            return Ok(None);
        }
        let matches: Vec<_> = self
            .assets
            .into_iter()
            .filter(|a| a.name == "mcdk.exe")
            .collect();
        if matches.len() != 1 {
            return Err(invalid("MCDK Release 缺少唯一的 mcdk.exe 资产"));
        }
        let asset = matches.into_iter().next().unwrap();
        let digest = asset
            .digest
            .as_deref()
            .and_then(|s| s.strip_prefix("sha256:"))
            .ok_or_else(|| invalid("MCDK Release 缺少 SHA-256 摘要，已拒绝更新"))?;
        let candidate = McdkVersion {
            version: parsed.to_string(),
            tag: self.tag_name,
            asset_name: asset.name,
            size: asset.size,
            sha256: digest.to_ascii_lowercase(),
        };
        if candidate.download_url()? != asset.browser_download_url {
            return Err(invalid("MCDK 下载地址不属于预期的官方 Release"));
        }
        Ok(Some(candidate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn fixture() -> Value {
        json!({"tag_name":"v2.0.0", "draft":false, "prerelease":false, "assets":[{
            "name":"mcdk.exe", "size":2048, "digest":format!("sha256:{}", "a".repeat(64)),
            "browser_download_url":"https://github.com/GitHub-Zero123/MCDevTool/releases/download/v2.0.0/mcdk.exe"
        }]})
    }

    fn candidate(value: Value) -> Result<Option<McdkVersion>> {
        serde_json::from_value::<Release>(value)
            .unwrap()
            .candidate(Some(&McdkVersion::bundled()))
    }

    #[test]
    fn accepts_only_new_stable_official_assets() {
        assert_eq!(candidate(fixture()).unwrap().unwrap().version, "2.0.0");
        let mut value = fixture();
        value["prerelease"] = json!(true);
        assert!(candidate(value).unwrap().is_none());
        let mut value = fixture();
        value["tag_name"] = json!("v1.0.0");
        assert!(candidate(value).unwrap().is_none());
        let mut value = fixture();
        value["tag_name"] = json!("v3.0.0-beta.1");
        assert!(candidate(value).unwrap().is_none());
        let mut value = fixture();
        value["assets"][0]["digest"] = Value::Null;
        assert!(candidate(value).is_err());
        let mut value = fixture();
        value["assets"][0]["browser_download_url"] = json!("https://example.com/mcdk.exe");
        assert!(candidate(value).is_err());
        let mut value = fixture();
        value["assets"][0]["size"] = json!(u64::MAX);
        assert!(candidate(value).is_err());
    }
}
