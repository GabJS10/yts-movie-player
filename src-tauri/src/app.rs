//! Application-level things: version, log files and the "new version" notice
//! (`docs/IPC.md`, "Aplicación").
//!
//! Logs: `$XDG_STATE_HOME/yts-player/logs` (`~/.local/state/…`) on Linux,
//! `%LOCALAPPDATA%\yts-player\logs` on Windows, `<YTS_PLAYER_DATA_DIR>/logs` when that
//! variable is set; one file per day
//! (`yts-player.YYYY-MM-DD.log`), the last [`LOG_FILES_KEPT`] kept. Level `info` unless
//! `RUST_LOG` says otherwise. Secrets never reach them (`Settings`' `Debug` redacts them).
//!
//! Updates: only a notice. `check_for_update` asks GitHub for the latest release once per
//! start and never fails: no network, an odd answer or nothing newer → `None`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::types::UpdateInfo;

pub const APP_DIR_NAME: &str = "yts-player";
pub const REPO_URL: &str = "https://github.com/GabJS10/yts-movie-player";
pub const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/GabJS10/yts-movie-player/releases/latest";
pub const LOG_FILE_PREFIX: &str = "yts-player";
pub const LOG_FILE_SUFFIX: &str = "log";
pub const LOG_FILES_KEPT: usize = 7;

/// Version of this build (`Cargo.toml`, kept equal to `tauri.conf.json`).
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// `<YTS_PLAYER_DATA_DIR>/logs` if set; else `$XDG_STATE_HOME/yts-player/logs`
/// (`~/.local/state/…`) on Linux and `<data folder>\logs` where there is no state folder
/// (Windows, macOS).
pub fn logs_dir() -> Option<PathBuf> {
    if let Some(dir) = crate::paths::data_dir_override() {
        return Some(dir.join("logs"));
    }
    match dirs::state_dir() {
        Some(state) => Some(state.join(APP_DIR_NAME).join("logs")),
        None => crate::paths::data_dir().map(|d| d.join("logs")),
    }
}

/// Deletes the oldest log files so that at most `keep` remain. Only files named like ours
/// (`yts-player.<date>.log`) are touched. Returns how many were deleted.
pub fn prune_logs(dir: &Path, keep: usize) -> usize {
    let Ok(read) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut logs: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                    n.starts_with(&format!("{LOG_FILE_PREFIX}."))
                        && n.ends_with(&format!(".{LOG_FILE_SUFFIX}"))
                })
        })
        .collect();
    // The date in the name sorts chronologically.
    logs.sort();
    let excess = logs.len().saturating_sub(keep);
    logs.iter()
        .take(excess)
        .filter(|p| std::fs::remove_file(p).is_ok())
        .count()
}

/// `major.minor.patch` with an optional pre-release (`1.2.0-beta.1`); a leading `v` is
/// accepted. Build metadata (`+…`) is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl Version {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
        let s = s.split('+').next()?;
        let (core, pre) = match s.split_once('-') {
            Some((core, pre)) if !pre.is_empty() => (core, Some(pre.to_owned())),
            Some(_) => return None,
            None => (s, None),
        };
        let mut parts = core.split('.');
        let mut num = || parts.next()?.parse::<u64>().ok();
        let v = Self {
            major: num()?,
            minor: num()?,
            patch: num()?,
            pre,
        };
        parts.next().is_none().then_some(v)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    /// Semver precedence: a pre-release is older than its release; pre-releases compare
    /// by their dot-separated identifiers (numbers numerically).
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => {
                    let (mut a, mut b) = (a.split('.'), b.split('.'));
                    loop {
                        match (a.next(), b.next()) {
                            (None, None) => return Ordering::Equal,
                            (None, Some(_)) => return Ordering::Less,
                            (Some(_), None) => return Ordering::Greater,
                            (Some(x), Some(y)) => {
                                let o = match (x.parse::<u64>(), y.parse::<u64>()) {
                                    (Ok(x), Ok(y)) => x.cmp(&y),
                                    (Ok(_), Err(_)) => Ordering::Less,
                                    (Err(_), Ok(_)) => Ordering::Greater,
                                    (Err(_), Err(_)) => x.cmp(y),
                                };
                                if o != Ordering::Equal {
                                    return o;
                                }
                            }
                        }
                    }
                }
            })
    }
}

/// The part of GitHub's release JSON we use.
#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: Option<String>,
    html_url: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// The update to offer, from GitHub's `releases/latest` body, if newer than `current`.
pub fn update_from_release(body: &str, current: &str) -> Option<UpdateInfo> {
    let release: GithubRelease = serde_json::from_str(body).ok()?;
    if release.draft || release.prerelease {
        return None;
    }
    let tag = release.tag_name?;
    let latest = Version::parse(&tag)?;
    if latest.pre.is_some() || latest <= Version::parse(current)? {
        return None;
    }
    let url = release
        .html_url
        .filter(|u| u.starts_with("https://github.com/"))?;
    Some(UpdateInfo {
        version: format!("{}.{}.{}", latest.major, latest.minor, latest.patch),
        url,
        published_at: release.published_at.unwrap_or_default(),
    })
}

/// `check_for_update`, once per start.
pub struct UpdateChecker {
    url: String,
    current: String,
    timeout: Duration,
    result: tokio::sync::OnceCell<Option<UpdateInfo>>,
}

impl Default for UpdateChecker {
    fn default() -> Self {
        Self::new(LATEST_RELEASE_URL, version())
    }
}

impl UpdateChecker {
    pub fn new(url: &str, current: &str) -> Self {
        Self {
            url: url.to_owned(),
            current: current.to_owned(),
            timeout: Duration::from_secs(10),
            result: tokio::sync::OnceCell::new(),
        }
    }

    /// Never fails: anything unexpected is `None` (and logged).
    pub async fn check(&self) -> Option<UpdateInfo> {
        self.result
            .get_or_init(|| async {
                let found = self.fetch().await;
                match &found {
                    Some(u) => tracing::info!(version = %u.version, "new version available"),
                    None => tracing::info!(current = %self.current, "no newer version"),
                }
                found
            })
            .await
            .clone()
    }

    async fn fetch(&self) -> Option<UpdateInfo> {
        let client = reqwest::Client::builder()
            .user_agent(format!("{APP_DIR_NAME}/{}", self.current))
            .timeout(self.timeout)
            .build()
            .ok()?;
        let resp = match client
            .get(&self.url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::info!(error = %e, "could not check for updates");
                return None;
            }
        };
        if !resp.status().is_success() {
            tracing::info!(status = %resp.status(), "could not check for updates");
            return None;
        }
        let body = resp.text().await.ok()?;
        update_from_release(&body, &self.current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn parses_versions() {
        assert_eq!(
            v("v1.2.3"),
            Version {
                major: 1,
                minor: 2,
                patch: 3,
                pre: None
            }
        );
        assert_eq!(v("1.0.0-beta.2+build.5").pre.as_deref(), Some("beta.2"));
        for bad in ["", "1", "1.2", "1.2.3.4", "1.x.3", "v", "1.2.3-", "latest"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn semver_precedence() {
        let ordered = [
            "0.9.9",
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
            "1.0.1",
            "1.1.0",
            "1.10.0",
            "2.0.0",
        ];
        for w in ordered.windows(2) {
            assert!(v(w[0]) < v(w[1]), "{} < {}", w[0], w[1]);
        }
        assert_eq!(v("v1.0.0"), v("1.0.0"));
    }

    #[test]
    fn update_only_when_newer_and_published() {
        let body = |tag: &str, draft: bool, pre: bool| {
            serde_json::json!({
                "tag_name": tag,
                "html_url": format!("https://github.com/GabJS10/yts-movie-player/releases/tag/{tag}"),
                "published_at": "2026-11-01T10:00:00Z",
                "draft": draft,
                "prerelease": pre
            })
            .to_string()
        };
        let u = update_from_release(&body("v1.1.0", false, false), "1.0.0").unwrap();
        assert_eq!(u.version, "1.1.0");
        assert_eq!(
            u.url,
            "https://github.com/GabJS10/yts-movie-player/releases/tag/v1.1.0"
        );
        assert_eq!(u.published_at, "2026-11-01T10:00:00Z");
        assert_eq!(
            update_from_release(&body("v1.0.0", false, false), "1.0.0"),
            None
        );
        assert_eq!(
            update_from_release(&body("v0.9.0", false, false), "1.0.0"),
            None
        );
        assert_eq!(
            update_from_release(&body("v2.0.0", true, false), "1.0.0"),
            None
        );
        assert_eq!(
            update_from_release(&body("v2.0.0", false, true), "1.0.0"),
            None
        );
        assert_eq!(
            update_from_release(&body("v2.0.0-rc.1", false, false), "1.0.0"),
            None
        );
        assert_eq!(
            update_from_release(&body("nightly", false, false), "1.0.0"),
            None
        );
        assert_eq!(update_from_release("not json", "1.0.0"), None);
        assert_eq!(update_from_release("{}", "1.0.0"), None);
        // A link that is not GitHub's is not offered.
        let evil =
            serde_json::json!({ "tag_name": "v9.0.0", "html_url": "https://evil.example/x" });
        assert_eq!(update_from_release(&evil.to_string(), "1.0.0"), None);
    }

    #[test]
    fn prunes_old_logs_only() {
        let tmp = tempfile::tempdir().unwrap();
        for day in 1..=9 {
            std::fs::write(
                tmp.path().join(format!("yts-player.2026-10-0{day}.log")),
                b"x",
            )
            .unwrap();
        }
        std::fs::write(tmp.path().join("other.txt"), b"x").unwrap();
        std::fs::write(tmp.path().join("yts-player.notes"), b"x").unwrap();
        assert_eq!(prune_logs(tmp.path(), LOG_FILES_KEPT), 2);
        let mut left: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left.len(), 9);
        assert!(!left.contains(&"yts-player.2026-10-01.log".to_owned()));
        assert!(!left.contains(&"yts-player.2026-10-02.log".to_owned()));
        assert!(left.contains(&"yts-player.2026-10-09.log".to_owned()));
        assert!(left.contains(&"other.txt".to_owned()));
        assert_eq!(prune_logs(tmp.path(), LOG_FILES_KEPT), 0);
        assert_eq!(prune_logs(&tmp.path().join("missing"), 1), 0);
    }
}
