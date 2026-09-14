use anyhow::Result;
use serde::Deserialize;
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Debug, Default)]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub latest: String,
    pub url: String,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
}

pub fn check_updates() -> Receiver<UpdateInfo> {
    let (tx, rx) = mpsc::channel();
    let endpoint = option_env!("SSHO_RELEASES_URL")
        .map(str::to_owned)
        .or_else(|| {
            option_env!("SSHO_REPOSITORY")
                .map(|repo| format!("https://api.github.com/repos/{repo}/releases/latest"))
        });
    if let Some(endpoint) = endpoint {
        std::thread::spawn(move || {
            let fetch = || -> Result<UpdateInfo> {
                let client = reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(5))
                    .user_agent(concat!("ssho/", env!("CARGO_PKG_VERSION")))
                    .build()?;
                let json = client.get(endpoint).send()?.error_for_status()?.text()?;
                interpret_release(&json, env!("CARGO_PKG_VERSION"))
            };
            if let Ok(info) = fetch() {
                let _ = tx.send(info);
            }
        });
    }
    rx
}
pub fn interpret_release(json: &str, current: &str) -> Result<UpdateInfo> {
    let release: Release = serde_json::from_str(json)?;
    let mut info = UpdateInfo {
        current: current.into(),
        latest: release.tag_name.clone(),
        url: release.html_url,
        available: false,
    };
    if current == "dev" || release.prerelease || release.draft {
        return Ok(info);
    }
    let latest = semver::Version::parse(release.tag_name.trim_start_matches('v'))?;
    let current = semver::Version::parse(current.trim_start_matches('v'))?;
    info.available = latest.pre.is_empty() && latest > current;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_new_stable_versions_produce_notifications() {
        let release = r#"{"tag_name":"v1.2.0","html_url":"https://example.org/release"}"#;
        assert!(interpret_release(release, "0.9.0").unwrap().available);
        assert!(!interpret_release(release, "1.2.0").unwrap().available);
        assert!(!interpret_release(release, "dev").unwrap().available);
        assert!(
            !interpret_release(r#"{"tag_name":"v2.0.0-rc1","prerelease":true}"#, "1.0.0")
                .unwrap()
                .available
        );
        assert!(interpret_release(r#"{"tag_name":"invalid"}"#, "1.0.0").is_err());
    }
}
