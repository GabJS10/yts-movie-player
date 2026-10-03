//! Image proxy cache. Remote YTS image URLs are registered under a hash and exposed to the
//! frontend as `http://127.0.0.1:<port>/img/<hash>`; the local server (`stream.rs`) resolves
//! the hash, downloads the image once into `cache/img/<hash>` and serves it from disk.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use bytes::Bytes;
use reqwest::redirect;
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{AppError, AppResult};

/// Image hosts that are always allowed, besides the hosts of the API base URLs.
pub const YTS_IMAGE_HOSTS: [&str; 2] = ["yts.gg", "img.yts.gg"];

const HASH_LEN: usize = 32;
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("unknown image hash")]
    NotFound,
    #[error("image host not allowed: {0}")]
    Forbidden(String),
    #[error("upstream error: {0}")]
    Upstream(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct ImageStore {
    local_base: String,
    dir: PathBuf,
    allowed_hosts: Arc<HashSet<String>>,
    registry: RwLock<HashMap<String, String>>,
    http: reqwest::Client,
}

impl ImageStore {
    /// `local_base` is the local server origin, e.g. `http://127.0.0.1:4321`.
    pub fn new(
        dir: PathBuf,
        allowed_hosts: impl IntoIterator<Item = String>,
        local_base: impl Into<String>,
    ) -> AppResult<Self> {
        std::fs::create_dir_all(&dir)?;
        let allowed_hosts: Arc<HashSet<String>> = Arc::new(
            allowed_hosts
                .into_iter()
                .map(|h| h.to_ascii_lowercase())
                .collect(),
        );
        let http = restricted_client(
            Arc::clone(&allowed_hosts),
            Duration::from_secs(5),
            Duration::from_secs(20),
        )?;
        Ok(Self {
            local_base: local_base.into().trim_end_matches('/').to_owned(),
            dir,
            allowed_hosts,
            registry: RwLock::new(HashMap::new()),
            http,
        })
    }

    /// Default allowlist: the YTS image hosts plus the hosts of the API base URLs.
    pub fn default_allowed_hosts(base_urls: &[String]) -> Vec<String> {
        let mut hosts: Vec<String> = YTS_IMAGE_HOSTS.iter().map(|h| h.to_string()).collect();
        hosts.extend(
            base_urls
                .iter()
                .filter_map(|u| Url::parse(u).ok()?.host_str().map(str::to_owned)),
        );
        hosts
    }

    pub fn hash_of(remote_url: &str) -> String {
        let digest = Sha256::digest(remote_url.as_bytes());
        hex::encode(digest)[..HASH_LEN].to_owned()
    }

    /// Registers a remote URL and returns its local URL. `None` for empty or invalid URLs.
    pub fn local_url(&self, remote_url: Option<&str>) -> Option<String> {
        let remote = remote_url?.trim();
        if remote.is_empty() || Url::parse(remote).is_err() {
            return None;
        }
        let hash = Self::hash_of(remote);
        if let Ok(mut registry) = self.registry.write() {
            registry
                .entry(hash.clone())
                .or_insert_with(|| remote.to_owned());
        }
        Some(format!("{}/img/{hash}", self.local_base))
    }

    fn lookup(&self, hash: &str) -> Option<String> {
        self.registry.read().ok()?.get(hash).cloned()
    }

    /// Returns the image bytes and their content type, downloading them on first use.
    pub async fn get(&self, hash: &str) -> Result<(Bytes, &'static str), ImageError> {
        if hash.len() != HASH_LEN || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ImageError::NotFound);
        }
        let path = self.dir.join(hash);
        match tokio::fs::read(&path).await {
            Ok(data) => {
                let data = Bytes::from(data);
                let mime = sniff_mime(&data);
                return Ok((data, mime));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }

        let remote = self.lookup(hash).ok_or(ImageError::NotFound)?;
        let url = Url::parse(&remote).map_err(|_| ImageError::NotFound)?;
        if !host_allowed(&self.allowed_hosts, &url) {
            return Err(ImageError::Forbidden(
                url.host_str().unwrap_or_default().to_owned(),
            ));
        }

        let data = self.download(url).await?;
        let mime = sniff_mime(&data);
        if !mime.starts_with("image/") {
            return Err(ImageError::Upstream("response is not an image".into()));
        }
        write_atomically(&path, &data).await?;
        Ok((data, mime))
    }

    async fn download(&self, url: Url) -> Result<Bytes, ImageError> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| ImageError::Upstream(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ImageError::Upstream(format!("status {}", resp.status())));
        }
        if resp
            .content_length()
            .is_some_and(|len| len > MAX_IMAGE_BYTES as u64)
        {
            return Err(ImageError::Upstream("image too large".into()));
        }
        let data = resp
            .bytes()
            .await
            .map_err(|e| ImageError::Upstream(e.to_string()))?;
        if data.len() > MAX_IMAGE_BYTES {
            return Err(ImageError::Upstream("image too large".into()));
        }
        Ok(data)
    }
}

/// HTTP client that only follows redirects towards `hosts` (yts.gg → img.yts.gg).
/// Callers must still check the host of the initial URL with [`host_allowed`].
pub fn restricted_client(
    hosts: Arc<HashSet<String>>,
    connect_timeout: Duration,
    timeout: Duration,
) -> AppResult<reqwest::Client> {
    let policy = redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.error("too many redirects")
        } else if host_allowed(&hosts, attempt.url()) {
            attempt.follow()
        } else {
            let host = attempt.url().host_str().unwrap_or_default().to_owned();
            attempt.error(format!("redirect to disallowed host {host}"))
        }
    });
    reqwest::Client::builder()
        .connect_timeout(connect_timeout)
        .timeout(timeout)
        .redirect(policy)
        .build()
        .map_err(|e| AppError::Internal(format!("http client: {e}")))
}

pub fn host_allowed(hosts: &HashSet<String>, url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url
            .host_str()
            .is_some_and(|h| hosts.contains(&h.to_ascii_lowercase()))
}

/// Writes to a temporary file and renames it, so concurrent readers never see partial data.
async fn write_atomically(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp-{}-{n}", std::process::id()));
    tokio::fs::write(&tmp, data).await?;
    if let Err(e) = tokio::fs::rename(&tmp, path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }
    Ok(())
}

pub fn sniff_mime(data: &[u8]) -> &'static str {
    match data {
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => "image/webp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ImageStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = ImageStore::new(
            tmp.path().join("img"),
            ImageStore::default_allowed_hosts(&["https://movies-api.accel.li/api/v2/".into()]),
            "http://127.0.0.1:9/",
        )
        .unwrap();
        (tmp, store)
    }

    #[test]
    fn local_url_registers_and_is_stable() {
        let (_tmp, store) = store();
        let remote = "https://yts.gg/assets/images/movies/x/medium-cover.jpg";
        let local = store.local_url(Some(remote)).unwrap();
        let hash = ImageStore::hash_of(remote);
        assert_eq!(local, format!("http://127.0.0.1:9/img/{hash}"));
        assert_eq!(hash.len(), HASH_LEN);
        assert_eq!(store.lookup(&hash).as_deref(), Some(remote));
        assert_eq!(store.local_url(Some(remote)).unwrap(), local);
        assert_eq!(store.local_url(Some("")), None);
        assert_eq!(store.local_url(Some("not a url")), None);
        assert_eq!(store.local_url(None), None);
    }

    #[test]
    fn allowlist_includes_yts_and_api_hosts() {
        let (_tmp, store) = store();
        let ok = |u: &str| host_allowed(&store.allowed_hosts, &Url::parse(u).unwrap());
        assert!(ok("https://yts.gg/a.jpg"));
        assert!(ok("https://img.yts.gg/a.jpg"));
        assert!(ok("https://movies-api.accel.li/a.jpg"));
        assert!(!ok("https://evil.example/a.jpg"));
        assert!(!ok("ftp://yts.gg/a.jpg"));
    }

    #[test]
    fn sniffs_common_formats() {
        assert_eq!(sniff_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
        assert_eq!(sniff_mime(b"\x89PNG\r\n"), "image/png");
        assert_eq!(sniff_mime(b"RIFF0000WEBPVP8"), "image/webp");
        assert_eq!(sniff_mime(b"<html>"), "application/octet-stream");
    }

    #[tokio::test]
    async fn rejects_malformed_hashes() {
        let (_tmp, store) = store();
        assert!(matches!(
            store.get("../../etc/passwd").await,
            Err(ImageError::NotFound)
        ));
    }
}
