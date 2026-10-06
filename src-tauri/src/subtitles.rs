//! OpenSubtitles REST client (`/subtitles`, `/download`, optional `/login`), ranking of
//! the results and the on-disk `.vtt` cache served at `/subs/<id>.vtt`.
//!
//! - Every request carries `Api-Key` and `User-Agent: YTSPlayer v<version>`.
//! - With username and password in Settings we log in once per session (the token is
//!   kept in memory). A failed login is not retried with the same credentials: the API
//!   rate-limits `/login` hard (1/s, 10/min, 30/h). After login, requests go to the
//!   `base_url` the API returns (e.g. `vip-api.opensubtitles.com`).
//! - Downloads are cached in `<data>/subs/<fileId>.vtt`: loading again costs no quota.
//! - The key, the username, the password and the token never reach the logs.
//!
//! Quota (OpenSubtitles help center, 2026-10): 5 downloads/24 h per IP without login;
//! with a free account the two official articles disagree (10 vs 20/day) and the real
//! number comes back as `allowed_downloads` on login; VIP up to 1000. Searching is free.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use reqwest::{Method, RequestBuilder, StatusCode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{AppError, AppResult};
use crate::types::{Quality, SubtitleOption, SubtitleTrack, SubtitlesStatus, TorrentSource};
use crate::vtt;

pub const DEFAULT_BASE_URL: &str = "https://api.opensubtitles.com/api/v1";
/// Folder of the `.vtt` cache inside the app data dir.
pub const SUBS_DIR: &str = "subs";
const MAX_SUBTITLE_BYTES: u64 = 10 * 1024 * 1024;

pub fn user_agent() -> String {
    format!("YTSPlayer v{}", env!("CARGO_PKG_VERSION"))
}

/// Ids served at `/subs/<id>.vtt`: OpenSubtitles file ids (digits) or `f-<hex>` for
/// local files. Anything else is rejected (no path tricks).
pub fn is_valid_sub_id(id: &str) -> bool {
    let local = id.strip_prefix("f-").is_some_and(|h| {
        !h.is_empty() && h.len() <= 64 && h.bytes().all(|b| b.is_ascii_hexdigit())
    });
    let remote = !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit());
    local || remote
}

pub fn vtt_path(subs_dir: &Path, id: &str) -> PathBuf {
    subs_dir.join(format!("{id}.vtt"))
}

// ---------------------------------------------------------------------------
// Ranking (pure)
// ---------------------------------------------------------------------------

/// What we know about the YTS torrent being played, to match subtitle releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Release {
    pub quality: Quality,
    pub source: TorrentSource,
}

fn tokens(release: &str) -> Vec<String> {
    release
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .flat_map(|t| {
            // Keep "web-dl" whole and also its parts.
            let mut v = vec![t.to_owned()];
            if t.contains('-') {
                v.extend(t.split('-').map(str::to_owned));
            }
            v
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// Same quality and same kind of source as the YTS torrent (e.g. `1080p` + `BluRay`).
pub fn matches_release(label: &str, release: &Release) -> bool {
    let toks = tokens(label);
    let has = |words: &[&str]| toks.iter().any(|t| words.contains(&t.as_str()));
    let quality = match release.quality {
        Quality::P480 => has(&["480p"]),
        Quality::P720 => has(&["720p"]),
        Quality::P1080 => has(&["1080p"]),
        Quality::P2160 => has(&["2160p", "4k", "uhd"]),
        Quality::ThreeD => has(&["3d"]),
    };
    let source = match release.source {
        TorrentSource::Bluray => has(&["bluray", "blu-ray", "brrip", "bdrip", "bdrem", "bd"]),
        TorrentSource::Web => has(&["web", "webrip", "web-dl", "webdl"]),
    };
    quality && source
}

/// Release matches first; hearing-impaired and AI/machine-translated go after the rest
/// (they are still listed when there is nothing else); then most downloaded.
pub fn rank(options: &mut [SubtitleOption]) {
    options.sort_by_key(|o| {
        (
            !o.matches_release,
            o.hearing_impaired || o.ai_translated,
            std::cmp::Reverse(o.downloads),
        )
    });
}

/// ISO 639-1 from an OpenSubtitles language code (`pt-BR` → `pt`, `ea` → `es`).
pub fn normalize_lang(code: &str) -> String {
    let base = code
        .split(['-', '_'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if base == "ea" {
        "es".into()
    } else {
        base
    }
}

/// The subtitle's page, only if it is an https page on opensubtitles (the front opens it
/// in the system browser).
pub fn page_url(raw: &str) -> Option<String> {
    let url = Url::parse(raw.trim()).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let ours = ["opensubtitles.com", "opensubtitles.org"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")));
    (url.scheme() == "https" && ours).then(|| url.to_string())
}

/// `tt0133093` → `133093` (the API wants no prefix and no leading zeros).
pub fn imdb_number(imdb_code: &str) -> Option<String> {
    let digits = imdb_code.trim().trim_start_matches("tt");
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = digits.trim_start_matches('0');
    (!n.is_empty()).then(|| n.to_owned())
}

// ---------------------------------------------------------------------------
// API models (tolerant)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<SearchItem>,
}

#[derive(Debug, Deserialize, Default)]
struct SearchItem {
    #[serde(default)]
    attributes: Attributes,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Attributes {
    language: Option<String>,
    download_count: Option<u64>,
    hearing_impaired: Option<bool>,
    ai_translated: Option<bool>,
    machine_translated: Option<bool>,
    foreign_parts_only: Option<bool>,
    release: Option<String>,
    url: Option<String>,
    files: Vec<ApiFile>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ApiFile {
    file_id: Option<u64>,
    file_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DownloadResponse {
    link: Option<String>,
    remaining: Option<i64>,
    reset_time_utc: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct LoginResponse {
    token: Option<String>,
    base_url: Option<String>,
    user: Option<LoginUser>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct LoginUser {
    allowed_downloads: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct UserInfoResponse {
    #[serde(default)]
    data: UserInfo,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct UserInfo {
    remaining_downloads: Option<i64>,
    allowed_downloads: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ErrorBody {
    message: Option<String>,
    reset_time_utc: Option<String>,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SubtitlesConfig {
    pub base_url: String,
    pub user_agent: String,
    pub timeout: Duration,
    /// `<data>/subs`.
    pub subs_dir: PathBuf,
    /// Local server origin, e.g. `http://127.0.0.1:4321`.
    pub local_base: String,
}

impl SubtitlesConfig {
    pub fn new(subs_dir: PathBuf, local_base: String) -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.into(),
            user_agent: user_agent(),
            timeout: Duration::from_secs(15),
            subs_dir,
            local_base,
        }
    }
}

/// No `Debug`: these are secrets.
#[derive(Clone, Default, PartialEq)]
pub struct Credentials {
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Clone)]
struct Session {
    token: String,
    /// API base to use after login.
    api_base: String,
}

enum Login {
    /// Not attempted yet with the current credentials.
    Pending,
    LoggedIn(Session),
    /// Rejected (401): not retried until the credentials change.
    Failed(String),
    /// No username/password.
    Anonymous,
}

#[derive(Default)]
struct Quota {
    remaining: Option<i64>,
    reset_at: Option<String>,
}

pub struct SubtitlesClient {
    http: reqwest::Client,
    cfg: SubtitlesConfig,
    creds: RwLock<Credentials>,
    login: tokio::sync::Mutex<Login>,
    quota: Mutex<Quota>,
    /// file id → (lang, label), from searches, to label loaded tracks.
    labels: Mutex<HashMap<String, (String, Option<String>)>>,
}

fn net_err(e: reqwest::Error) -> AppError {
    // reqwest's Display includes the URL, which has no secrets (they go in headers).
    AppError::Network(format!("OpenSubtitles: {e}"))
}

impl SubtitlesClient {
    pub fn new(cfg: SubtitlesConfig, creds: Credentials) -> AppResult<Self> {
        std::fs::create_dir_all(&cfg.subs_dir)?;
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(cfg.timeout)
            .build()
            .map_err(|e| AppError::Internal(format!("http client: {e}")))?;
        let login = initial_login(&creds);
        Ok(Self {
            http,
            cfg,
            creds: RwLock::new(creds),
            login: tokio::sync::Mutex::new(login),
            quota: Mutex::new(Quota::default()),
            labels: Mutex::new(HashMap::new()),
        })
    }

    fn creds(&self) -> Credentials {
        match self.creds.read() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// New credentials from Settings. Drops the session token if they changed.
    pub async fn set_credentials(&self, creds: Credentials) {
        if self.creds() == creds {
            return;
        }
        let mut login = self.login.lock().await;
        *login = initial_login(&creds);
        match self.creds.write() {
            Ok(mut g) => *g = creds,
            Err(poisoned) => *poisoned.into_inner() = creds,
        }
        if let Ok(mut q) = self.quota.lock() {
            q.remaining = None;
        }
        tracing::info!("OpenSubtitles credentials changed, session reset");
    }

    fn api_key(&self) -> AppResult<String> {
        self.creds()
            .api_key
            .ok_or_else(|| AppError::SubtitlesAuth("no OpenSubtitles API key configured".into()))
    }

    fn request(&self, method: Method, base: &str, path: &str, key: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}{path}", base.trim_end_matches('/')))
            .header("Api-Key", key)
            .header(reqwest::header::USER_AGENT, &self.cfg.user_agent)
            .header(reqwest::header::ACCEPT, "application/json")
    }

    /// Logs in if there are credentials and it wasn't tried yet. `Err` only when the
    /// login was rejected (now or before) and `strict` is set.
    async fn session(&self, strict: bool) -> AppResult<Option<Session>> {
        let mut login = self.login.lock().await;
        if let Login::Pending = *login {
            *login = match self.do_login().await {
                Ok(session) => Login::LoggedIn(session),
                Err(AppError::SubtitlesAuth(msg)) => {
                    tracing::warn!("OpenSubtitles login rejected, continuing without session");
                    Login::Failed(msg)
                }
                // Network/quota errors: try again next time.
                Err(e) => return if strict { Err(e) } else { Ok(None) },
            };
        }
        match &*login {
            Login::LoggedIn(s) => Ok(Some(s.clone())),
            Login::Failed(msg) if strict => Err(AppError::SubtitlesAuth(msg.clone())),
            _ => Ok(None),
        }
    }

    async fn do_login(&self) -> AppResult<Session> {
        let key = self.api_key()?;
        let creds = self.creds();
        let (Some(username), Some(password)) = (creds.username, creds.password) else {
            return Err(AppError::Internal("login without credentials".into()));
        };
        let resp = self
            .request(Method::POST, &self.cfg.base_url, "/login", &key)
            .json(&serde_json::json!({ "username": username, "password": password }))
            .send()
            .await
            .map_err(net_err)?;
        let status = resp.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(AppError::SubtitlesAuth(
                "OpenSubtitles rejected the username or password".into(),
            ));
        }
        let body: LoginResponse = self.parse(resp, "login").await?;
        let token = body
            .token
            .ok_or_else(|| AppError::SubtitlesAuth("login response without token".into()))?;
        if let Some(allowed) = body.user.and_then(|u| u.allowed_downloads) {
            tracing::info!(allowed_downloads = allowed, "logged in to OpenSubtitles");
        }
        Ok(Session {
            token,
            api_base: self.api_base_after_login(body.base_url.as_deref()),
        })
    }

    /// The API may move a logged-in user to another host (VIP). Only followed when we
    /// talk to the real API and the new host is an opensubtitles.com one.
    fn api_base_after_login(&self, base_url: Option<&str>) -> String {
        let configured = &self.cfg.base_url;
        let is_os =
            |host: &str| host == "opensubtitles.com" || host.ends_with(".opensubtitles.com");
        let real_api = Url::parse(configured)
            .ok()
            .and_then(|u| u.host_str().map(is_os))
            .unwrap_or(false);
        match base_url.map(|b| {
            b.trim()
                .trim_start_matches("https://")
                .trim_end_matches('/')
        }) {
            Some(host) if real_api && is_os(host) && !host.contains('/') => {
                format!("https://{host}/api/v1")
            }
            _ => configured.clone(),
        }
    }

    /// Maps HTTP errors to `AppError` and parses the JSON body.
    async fn parse<T: for<'de> Deserialize<'de> + Default>(
        &self,
        resp: reqwest::Response,
        what: &str,
    ) -> AppResult<T> {
        let status = resp.status();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = resp.bytes().await.map_err(net_err)?;
        if status.is_success() {
            return serde_json::from_slice(&bytes).map_err(|e| {
                AppError::Internal(format!("OpenSubtitles {what}: invalid JSON: {e}"))
            });
        }
        let body: ErrorBody = serde_json::from_slice(&bytes).unwrap_or_default();
        let detail = body.message.unwrap_or_default();
        Err(match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                AppError::SubtitlesAuth(format!("OpenSubtitles {what}: {status} {detail}"))
            }
            StatusCode::NOT_ACCEPTABLE => {
                self.set_reset(body.reset_time_utc.clone());
                self.set_remaining(Some(0));
                AppError::SubtitlesQuota(format!(
                    "download quota exhausted, renews at {}: {detail}",
                    body.reset_time_utc.as_deref().unwrap_or("unknown")
                ))
            }
            StatusCode::TOO_MANY_REQUESTS => AppError::SubtitlesQuota(format!(
                "OpenSubtitles rate limit, retry after {}s",
                retry_after.as_deref().unwrap_or("a few ")
            )),
            StatusCode::NOT_FOUND => AppError::NotFound(format!("OpenSubtitles {what}: {detail}")),
            _ => AppError::Network(format!("OpenSubtitles {what}: {status} {detail}")),
        })
    }

    fn set_reset(&self, reset: Option<String>) {
        if let (Ok(mut q), Some(r)) = (self.quota.lock(), reset) {
            q.reset_at = Some(r);
        }
    }

    fn set_remaining(&self, remaining: Option<i64>) {
        if let (Ok(mut q), Some(r)) = (self.quota.lock(), remaining) {
            q.remaining = Some(r);
        }
    }

    /// Subtitles for a movie in `lang`, ranked (see [`rank`]).
    pub async fn search(
        &self,
        imdb_code: &str,
        lang: &str,
        release: Option<Release>,
    ) -> AppResult<Vec<SubtitleOption>> {
        let key = self.api_key()?;
        let imdb = imdb_number(imdb_code)
            .ok_or_else(|| AppError::InvalidInput(format!("invalid imdb code {imdb_code:?}")))?;
        let lang = lang.trim().to_ascii_lowercase();
        if lang.is_empty()
            || lang.len() > 8
            || !lang.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-')
        {
            return Err(AppError::InvalidInput(format!("invalid language {lang:?}")));
        }
        let session = self.session(false).await?;
        let base = session
            .as_ref()
            .map_or(self.cfg.base_url.as_str(), |s| s.api_base.as_str());
        // Parameters sorted and lowercase, as the API asks (avoids redirects, hits cache).
        let path = format!("/subtitles?foreign_parts_only=exclude&imdb_id={imdb}&languages={lang}");
        let mut req = self.request(Method::GET, base, &path, &key);
        if let Some(s) = &session {
            req = req.bearer_auth(&s.token);
        }
        let resp = req.send().await.map_err(net_err)?;
        let body: SearchResponse = self.parse(resp, "search").await?;

        let mut options: Vec<SubtitleOption> = body
            .data
            .into_iter()
            .filter_map(|item| {
                let a = item.attributes;
                if a.foreign_parts_only == Some(true) {
                    return None;
                }
                let file = a.files.first()?;
                let id = file.file_id?.to_string();
                let label = a
                    .release
                    .as_deref()
                    .map(str::trim)
                    .filter(|r| !r.is_empty())
                    .or(file.file_name.as_deref())
                    .map(str::to_owned);
                Some(SubtitleOption {
                    matches_release: release
                        .zip(label.as_deref())
                        .is_some_and(|(r, l)| matches_release(l, &r)),
                    lang: a
                        .language
                        .as_deref()
                        .map_or_else(|| lang.clone(), normalize_lang),
                    downloads: a.download_count.unwrap_or(0),
                    hearing_impaired: a.hearing_impaired.unwrap_or(false),
                    ai_translated: a.ai_translated.unwrap_or(false)
                        || a.machine_translated.unwrap_or(false),
                    page_url: a.url.as_deref().and_then(page_url),
                    cached: self.cached_file(&id).is_file(),
                    id,
                    label,
                })
            })
            .collect();
        // A subtitle can show up twice (several files): keep the first.
        let mut seen = std::collections::HashSet::new();
        options.retain(|o| seen.insert(o.id.clone()));
        rank(&mut options);
        if let Ok(mut labels) = self.labels.lock() {
            for o in &options {
                labels.insert(o.id.clone(), (o.lang.clone(), o.label.clone()));
            }
        }
        tracing::debug!(imdb, %lang, results = options.len(), "subtitle search");
        Ok(options)
    }

    fn track(&self, id: &str, lang: Option<String>, label: Option<String>) -> SubtitleTrack {
        SubtitleTrack {
            track_url: format!(
                "{}/subs/{id}.vtt",
                self.cfg.local_base.trim_end_matches('/')
            ),
            lang,
            label,
        }
    }

    fn label_of(&self, id: &str) -> (Option<String>, Option<String>) {
        match self.labels.lock().ok().and_then(|l| l.get(id).cloned()) {
            Some((lang, label)) => (Some(lang), label),
            None => (None, None),
        }
    }

    /// Downloads (or takes from the disk cache) an OpenSubtitles file as `.vtt`.
    pub async fn load(&self, file_id: &str) -> AppResult<SubtitleTrack> {
        let file_id = file_id.trim();
        if !file_id.bytes().all(|b| b.is_ascii_digit()) || !is_valid_sub_id(file_id) {
            return Err(AppError::InvalidInput(format!(
                "invalid subtitle id {file_id:?}"
            )));
        }
        let path = vtt_path(&self.cfg.subs_dir, file_id);
        let (lang, label) = self.label_of(file_id);
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            tracing::debug!(file_id, "subtitle from disk cache");
            return Ok(self.track(file_id, lang, label));
        }

        let key = self.api_key()?;
        let mut session = self.session(false).await?;
        let mut retried = false;
        let link = loop {
            let base = session
                .as_ref()
                .map_or(self.cfg.base_url.as_str(), |s| s.api_base.as_str());
            let mut req = self
                .request(Method::POST, base, "/download", &key)
                .json(&serde_json::json!({ "file_id": file_id.parse::<u64>().unwrap_or(0) }));
            if let Some(s) = &session {
                req = req.bearer_auth(&s.token);
            }
            let resp = req.send().await.map_err(net_err)?;
            // An expired token: log in again once.
            if resp.status() == StatusCode::UNAUTHORIZED && session.is_some() && !retried {
                retried = true;
                *self.login.lock().await = initial_login(&self.creds());
                session = self.session(false).await?;
                continue;
            }
            let body: DownloadResponse = self.parse(resp, "download").await?;
            self.set_remaining(body.remaining);
            self.set_reset(body.reset_time_utc);
            if let Some(msg) = &body.message {
                tracing::debug!(remaining = ?body.remaining, %msg, "OpenSubtitles download");
            }
            break body
                .link
                .ok_or_else(|| AppError::Internal("download response without link".into()))?;
        };

        let bytes = self.fetch_file(&link).await?;
        let vtt = vtt::to_vtt(&bytes)
            .ok_or_else(|| AppError::InvalidInput(format!("subtitle {file_id} has no cues")))?;
        write_atomically(&path, vtt.as_bytes()).await?;
        tracing::info!(file_id, "subtitle downloaded and cached");
        Ok(self.track(file_id, lang, label))
    }

    async fn fetch_file(&self, link: &str) -> AppResult<Vec<u8>> {
        let url =
            Url::parse(link).map_err(|_| AppError::Internal("invalid download link".into()))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(AppError::Internal("invalid download link".into()));
        }
        let resp = self
            .http
            .get(url)
            .header(reqwest::header::USER_AGENT, &self.cfg.user_agent)
            .send()
            .await
            .map_err(net_err)?;
        if !resp.status().is_success() {
            return Err(AppError::Network(format!(
                "subtitle file download: {}",
                resp.status()
            )));
        }
        if resp
            .content_length()
            .is_some_and(|l| l > MAX_SUBTITLE_BYTES)
        {
            return Err(AppError::InvalidInput("subtitle file too large".into()));
        }
        let bytes = resp.bytes().await.map_err(net_err)?;
        if bytes.len() as u64 > MAX_SUBTITLE_BYTES {
            return Err(AppError::InvalidInput("subtitle file too large".into()));
        }
        Ok(bytes.to_vec())
    }

    /// Loads a local `.srt`/`.vtt` (file dialog or drag & drop). Works without a key.
    pub async fn load_file(&self, path: &Path) -> AppResult<SubtitleTrack> {
        Ok(self.import_file(path).await?.1)
    }

    /// The cached `.vtt` behind an id returned by [`Self::load`] or [`Self::import_file`].
    pub fn cached_file(&self, id: &str) -> PathBuf {
        vtt_path(&self.cfg.subs_dir, id)
    }

    pub fn subs_dir(&self) -> &Path {
        &self.cfg.subs_dir
    }

    pub fn has_api_key(&self) -> bool {
        self.creds().api_key.is_some()
    }

    /// [`Self::load_file`] that also returns the id of the cached `.vtt`.
    pub async fn import_file(&self, path: &Path) -> AppResult<(String, SubtitleTrack)> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(ext.as_deref(), Some("srt" | "vtt")) {
            return Err(AppError::InvalidInput("only .srt and .vtt files".into()));
        }
        let meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => AppError::NotFound(format!("{}", path.display())),
                _ => e.into(),
            })?;
        if !meta.is_file() || meta.len() > MAX_SUBTITLE_BYTES {
            return Err(AppError::InvalidInput("not a subtitle file".into()));
        }
        let bytes = tokio::fs::read(path).await?;
        let vtt = vtt::to_vtt(&bytes)
            .ok_or_else(|| AppError::InvalidInput("no subtitles found in the file".into()))?;
        let id = format!("f-{}", &hex::encode(Sha256::digest(vtt.as_bytes()))[..16]);
        write_atomically(&vtt_path(&self.cfg.subs_dir, &id), vtt.as_bytes()).await?;
        let label = path.file_name().map(|n| n.to_string_lossy().into_owned());
        tracing::info!(%id, "local subtitle loaded");
        let track = self.track(&id, None, label);
        Ok((id, track))
    }

    /// "Probar" in Settings: validates the key (and the login, if configured).
    pub async fn status(&self) -> AppResult<SubtitlesStatus> {
        let creds = self.creds();
        let Some(key) = creds.api_key.clone() else {
            return Ok(SubtitlesStatus {
                configured: false,
                logged_in: false,
                remaining_downloads: None,
                reset_at: None,
            });
        };
        // "Probar" retries a rejected login: the user may have just fixed it upstream.
        {
            let mut login = self.login.lock().await;
            if let Login::Failed(_) = *login {
                *login = initial_login(&creds);
            }
        }
        let session = self.session(true).await?;
        match &session {
            Some(s) => {
                let resp = self
                    .request(Method::GET, &s.api_base, "/infos/user", &key)
                    .bearer_auth(&s.token)
                    .send()
                    .await
                    .map_err(net_err)?;
                let info: UserInfoResponse = self.parse(resp, "user info").await?;
                self.set_remaining(
                    info.data
                        .remaining_downloads
                        .or(info.data.allowed_downloads),
                );
            }
            None => {
                // Cheap authenticated call that costs no quota.
                let resp = self
                    .request(Method::GET, &self.cfg.base_url, "/infos/formats", &key)
                    .send()
                    .await
                    .map_err(net_err)?;
                self.parse::<serde_json::Value>(resp, "key check").await?;
            }
        }
        let (remaining, reset_at) = self
            .quota
            .lock()
            .map(|q| (q.remaining, q.reset_at.clone()))
            .unwrap_or_default();
        Ok(SubtitlesStatus {
            configured: true,
            logged_in: session.is_some(),
            remaining_downloads: remaining,
            reset_at,
        })
    }
}

fn initial_login(creds: &Credentials) -> Login {
    match (&creds.api_key, &creds.username, &creds.password) {
        (Some(_), Some(_), Some(_)) => Login::Pending,
        _ => Login::Anonymous,
    }
}

async fn write_atomically(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    tokio::fs::write(&tmp, data).await?;
    if let Err(e) = tokio::fs::rename(&tmp, path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(id: &str, matches: bool, hi: bool, ai: bool, downloads: u64) -> SubtitleOption {
        SubtitleOption {
            id: id.into(),
            lang: "es".into(),
            label: Some(id.into()),
            downloads,
            hearing_impaired: hi,
            matches_release: matches,
            ai_translated: ai,
            page_url: None,
            cached: false,
        }
    }

    #[test]
    fn ranking_order() {
        let mut v = vec![
            opt("hi-popular", false, true, false, 9_000),
            opt("plain", false, false, false, 100),
            opt("match-ai", true, false, true, 5_000),
            opt("match", true, false, false, 10),
            opt("plain-popular", false, false, false, 1_000),
            opt("ai", false, false, true, 50_000),
        ];
        rank(&mut v);
        let ids: Vec<&str> = v.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "match",
                "match-ai",
                "plain-popular",
                "plain",
                "ai",
                "hi-popular"
            ]
        );
        // Only HI/AI available: still listed, by downloads.
        let mut only = vec![
            opt("a", false, true, false, 1),
            opt("b", false, false, true, 2),
        ];
        rank(&mut only);
        assert_eq!(only[0].id, "b");
    }

    #[test]
    fn release_matching() {
        let bluray = Release {
            quality: Quality::P1080,
            source: TorrentSource::Bluray,
        };
        let web = Release {
            quality: Quality::P720,
            source: TorrentSource::Web,
        };
        assert!(matches_release(
            "The.Matrix.1999.1080p.BluRay.x264-YTS",
            &bluray
        ));
        assert!(matches_release(
            "The Matrix (1999) [1080p] [BrRip] YIFY",
            &bluray
        ));
        assert!(!matches_release(
            "The.Matrix.1999.720p.BluRay.x264",
            &bluray
        ));
        assert!(!matches_release("The.Matrix.1999.1080p.WEB-DL", &bluray));
        assert!(matches_release("Movie.2021.720p.WEB-DL.DDP5.1", &web));
        assert!(matches_release("Movie.2021.720p.WEBRip.x264-RARBG", &web));
        assert!(!matches_release("Movie.2021.1080p.WEBRip", &web));
        // "web" must be a token, not a substring.
        assert!(!matches_release("Spider-Webster.720p.HDTV", &web));
        let uhd = Release {
            quality: Quality::P2160,
            source: TorrentSource::Bluray,
        };
        assert!(matches_release("Movie.2160p.UHD.BluRay.x265", &uhd));
    }

    #[test]
    fn helpers() {
        assert_eq!(imdb_number("tt0133093").as_deref(), Some("133093"));
        assert_eq!(imdb_number("tt10838180").as_deref(), Some("10838180"));
        assert_eq!(imdb_number("tt"), None);
        assert_eq!(imdb_number("tt0000000"), None);
        assert_eq!(imdb_number("abc"), None);
        assert_eq!(normalize_lang("pt-BR"), "pt");
        assert_eq!(normalize_lang("ES"), "es");
        assert_eq!(normalize_lang("ea"), "es");
        assert!(is_valid_sub_id("1234567"));
        assert!(is_valid_sub_id("f-0123abcd"));
        for bad in ["", "../x", "f-", "f-zz", "12a", "1/2", "f-../../etc"] {
            assert!(!is_valid_sub_id(bad), "{bad}");
        }
        assert!(user_agent().starts_with("YTSPlayer v"));
        assert_eq!(
            page_url(" https://www.opensubtitles.com/es/subtitles/x ").as_deref(),
            Some("https://www.opensubtitles.com/es/subtitles/x")
        );
        assert!(page_url("https://opensubtitles.org/a").is_some());
        for bad in [
            "http://www.opensubtitles.com/a",
            "https://evil.example/a",
            "https://opensubtitles.com.evil.example/a",
            "javascript:alert(1)",
            "",
        ] {
            assert_eq!(page_url(bad), None, "{bad}");
        }
    }
}
