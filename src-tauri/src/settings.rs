//! User settings: defaults, `SettingsPatch` semantics, validation and persistence.
//!
//! Each setting is a row `key → JSON value` in the `settings` table. Only keys the user
//! changed are stored, so untouched settings follow the defaults of the running version.
//! A stored value that no longer parses or validates falls back to its default.
//!
//! Folders (`downloadsDir`, `cacheDir`): validated without touching the disk here (absolute,
//! not inside each other); when they change, [`SettingsStore::update`] also checks that they
//! exist or can be created and are writable. A stored folder that is missing at startup
//! (unmounted disk) is kept: the callers decide what to do (see [`dir_available`]).

use std::path::{Component, Path};
use std::sync::RwLock;

use serde_json::{Map, Value};
use url::Url;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::types::{Quality, Settings, SettingsPatch};
use crate::yts::DEFAULT_BASE_URLS;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;

pub const DEFAULT_CACHE_LIMIT_BYTES: u64 = 10 * GIB;
pub const MIN_CACHE_LIMIT_BYTES: u64 = 256 * MIB;
pub const DEFAULT_BUFFER_TARGET_BYTES: u64 = 8 * MIB;
pub const MIN_BUFFER_TARGET_BYTES: u64 = MIB;
pub const MAX_BUFFER_TARGET_BYTES: u64 = 256 * MIB;
pub const MAX_API_BASE_URLS: usize = 10;
/// `Number.MAX_SAFE_INTEGER`: byte counts must survive the trip through JavaScript.
pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
const MAX_TEXT_LEN: usize = 512;

pub fn defaults(data_dir: &Path) -> Settings {
    Settings {
        api_base_urls: DEFAULT_BASE_URLS.iter().map(|s| s.to_string()).collect(),
        open_subtitles_api_key: None,
        open_subtitles_username: None,
        open_subtitles_password: None,
        subtitle_lang: "es".into(),
        auto_subtitles: true,
        preferred_quality: Quality::P1080,
        prefer_x264: true,
        external_player: "vlc".into(),
        buffer_target_bytes: DEFAULT_BUFFER_TARGET_BYTES,
        down_limit_kbps: None,
        up_limit_kbps: None,
        seed_after_download: false,
        listen_port: None,
        downloads_dir: data_dir.join("library").to_string_lossy().into_owned(),
        cache_dir: data_dir.join("cache").to_string_lossy().into_owned(),
        cache_limit_bytes: DEFAULT_CACHE_LIMIT_BYTES,
    }
}

/// Applies a patch: absent key = unchanged; `null` on a nullable setting = cleared (for
/// the folders: back to the default one).
pub fn apply_patch(current: &Settings, defaults: &Settings, patch: SettingsPatch) -> Settings {
    let s = current.clone();
    Settings {
        api_base_urls: patch.api_base_urls.unwrap_or(s.api_base_urls),
        open_subtitles_api_key: patch
            .open_subtitles_api_key
            .unwrap_or(s.open_subtitles_api_key),
        open_subtitles_username: patch
            .open_subtitles_username
            .unwrap_or(s.open_subtitles_username),
        open_subtitles_password: patch
            .open_subtitles_password
            .unwrap_or(s.open_subtitles_password),
        subtitle_lang: patch.subtitle_lang.unwrap_or(s.subtitle_lang),
        auto_subtitles: patch.auto_subtitles.unwrap_or(s.auto_subtitles),
        preferred_quality: patch.preferred_quality.unwrap_or(s.preferred_quality),
        prefer_x264: patch.prefer_x264.unwrap_or(s.prefer_x264),
        external_player: patch.external_player.unwrap_or(s.external_player),
        buffer_target_bytes: patch.buffer_target_bytes.unwrap_or(s.buffer_target_bytes),
        down_limit_kbps: patch.down_limit_kbps.unwrap_or(s.down_limit_kbps),
        up_limit_kbps: patch.up_limit_kbps.unwrap_or(s.up_limit_kbps),
        seed_after_download: patch.seed_after_download.unwrap_or(s.seed_after_download),
        listen_port: patch.listen_port.unwrap_or(s.listen_port),
        downloads_dir: match patch.downloads_dir {
            Some(v) => v.unwrap_or_else(|| defaults.downloads_dir.clone()),
            None => s.downloads_dir,
        },
        cache_dir: match patch.cache_dir {
            Some(v) => v.unwrap_or_else(|| defaults.cache_dir.clone()),
            None => s.cache_dir,
        },
        cache_limit_bytes: patch.cache_limit_bytes.unwrap_or(s.cache_limit_bytes),
    }
}

fn invalid(msg: impl Into<String>) -> AppError {
    AppError::InvalidInput(msg.into())
}

fn clean_text(field: &str, value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.len() > MAX_TEXT_LEN || value.chars().any(char::is_control) {
        return Err(invalid(format!("{field}: invalid text")));
    }
    Ok(value.to_owned())
}

/// Checks every setting and returns the normalized version (trimmed text, base URLs with
/// a trailing slash and without duplicates, empty API key → `null`).
pub fn validate(s: Settings) -> AppResult<Settings> {
    let mut urls: Vec<String> = Vec::new();
    for raw in &s.api_base_urls {
        let url = Url::parse(raw.trim()).map_err(|_| invalid(format!("apiBaseUrls: {raw:?}")))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(invalid(format!("apiBaseUrls: {raw:?} is not http(s)")));
        }
        let normalized = format!("{}/", url.as_str().trim_end_matches('/'));
        if !urls.contains(&normalized) {
            urls.push(normalized);
        }
    }
    if urls.is_empty() || urls.len() > MAX_API_BASE_URLS {
        return Err(invalid(format!(
            "apiBaseUrls: between 1 and {MAX_API_BASE_URLS} URLs"
        )));
    }

    // Error messages name the field, never the value (these are secrets).
    let secret = |field: &str, value: &Option<String>| -> AppResult<Option<String>> {
        match value {
            Some(v) => Ok(Some(clean_text(field, v)?).filter(|v| !v.is_empty())),
            None => Ok(None),
        }
    };
    let api_key = secret("openSubtitlesApiKey", &s.open_subtitles_api_key)?;
    let username = secret("openSubtitlesUsername", &s.open_subtitles_username)?;
    // Not trimmed: spaces may be part of a password.
    let password = match &s.open_subtitles_password {
        Some(p) if p.len() > MAX_TEXT_LEN || p.chars().any(char::is_control) => {
            return Err(invalid("openSubtitlesPassword: invalid text"));
        }
        Some(p) if p.is_empty() => None,
        other => other.clone(),
    };

    let lang = s.subtitle_lang.trim().to_ascii_lowercase();
    if !(2..=3).contains(&lang.len()) || !lang.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(invalid(format!("subtitleLang: {:?}", s.subtitle_lang)));
    }

    let player = clean_text("externalPlayer", &s.external_player)?;
    if player.is_empty() {
        return Err(invalid("externalPlayer: empty"));
    }

    if !(MIN_BUFFER_TARGET_BYTES..=MAX_BUFFER_TARGET_BYTES).contains(&s.buffer_target_bytes) {
        return Err(invalid(format!(
            "bufferTargetBytes: {} not in {MIN_BUFFER_TARGET_BYTES}..={MAX_BUFFER_TARGET_BYTES}",
            s.buffer_target_bytes
        )));
    }
    for (field, limit) in [
        ("downLimitKbps", s.down_limit_kbps),
        ("upLimitKbps", s.up_limit_kbps),
    ] {
        if limit == Some(0) {
            return Err(invalid(format!("{field}: use null for no limit")));
        }
    }
    if s.listen_port.is_some_and(|p| p < 1024) {
        return Err(invalid(format!(
            "listenPort: {:?} below 1024",
            s.listen_port
        )));
    }

    let downloads_dir = clean_dir("downloadsDir", &s.downloads_dir)?;
    let cache_dir = clean_dir("cacheDir", &s.cache_dir)?;
    if nested(&downloads_dir, &cache_dir) {
        return Err(invalid(format!(
            "downloadsDir {downloads_dir:?} and cacheDir {cache_dir:?} are inside each other"
        )));
    }

    if !(MIN_CACHE_LIMIT_BYTES..=MAX_SAFE_INTEGER).contains(&s.cache_limit_bytes) {
        return Err(invalid(format!(
            "cacheLimitBytes: {} below {MIN_CACHE_LIMIT_BYTES}",
            s.cache_limit_bytes
        )));
    }

    Ok(Settings {
        api_base_urls: urls,
        open_subtitles_api_key: api_key,
        open_subtitles_username: username,
        open_subtitles_password: password,
        subtitle_lang: lang,
        external_player: player,
        downloads_dir,
        cache_dir,
        ..s
    })
}

/// `YTS_PLAYER_API_BASE_URLS` (E2E): comma separated list that replaces `apiBaseUrls`
/// for this run (not stored).
pub const API_BASE_URLS_ENV: &str = "YTS_PLAYER_API_BASE_URLS";

/// The URLs of a `YTS_PLAYER_API_BASE_URLS` value, normalized; `None` if unset, empty or
/// invalid (with a warning).
pub fn api_base_urls_override(value: Option<&str>) -> Option<Vec<String>> {
    let urls: Vec<String> = value?
        .split(',')
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_owned)
        .collect();
    if urls.is_empty() {
        return None;
    }
    match validate(Settings {
        api_base_urls: urls,
        ..defaults(Path::new("/"))
    }) {
        Ok(s) => Some(s.api_base_urls),
        Err(e) => {
            tracing::warn!(error = %e, "ignoring {API_BASE_URLS_ENV}");
            None
        }
    }
}

/// Absolute, without `..`, without a trailing slash.
fn clean_dir(field: &str, value: &str) -> AppResult<String> {
    let dir = clean_text(field, value)?;
    let path = Path::new(&dir);
    if !path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
        return Err(invalid(format!("{field}: {dir:?} is not an absolute path")));
    }
    let normalized: std::path::PathBuf = path.components().collect();
    Ok(normalized.to_string_lossy().into_owned())
}

/// One folder is the other or is inside it (ignoring case on Windows).
pub fn nested(a: &str, b: &str) -> bool {
    crate::platform::nested(Path::new(a), Path::new(b))
}

/// The folder exists and this process can write in it (on Windows, through a temporary
/// file that is deleted when closed).
pub fn dir_available(path: &Path) -> bool {
    path.is_dir() && crate::platform::dir_writable(path)
}

/// For a folder the user just chose: creates it if needed and checks that a file can be
/// written in it. Errors are `invalid_input` (the UI shows them next to the field).
pub fn prepare_dir(field: &str, dir: &Path) -> AppResult<()> {
    let fail = |e: std::io::Error| invalid(format!("{field}: {}: {e}", dir.display()));
    std::fs::create_dir_all(dir).map_err(fail)?;
    let probe = dir.join(".yts-player-write-test");
    std::fs::write(&probe, b"ok").map_err(fail)?;
    std::fs::remove_file(&probe).map_err(fail)?;
    Ok(())
}

/// Stored rows from before v0.11: `dataDir` becomes `cacheDir = <dataDir>/cache` and
/// `downloadsDir = <dataDir>/library` (unless those are stored already). Returns the new
/// rows (only values that differ from the defaults are kept) and whether anything changed.
pub fn migrate_rows(
    defaults: &Settings,
    rows: Vec<(String, String)>,
) -> (Vec<(String, String)>, bool) {
    let Some(pos) = rows.iter().position(|(k, _)| k == "dataDir") else {
        return (rows, false);
    };
    let mut rows = rows;
    let (_, raw) = rows.remove(pos);
    let Ok(Value::String(data_dir)) = serde_json::from_str::<Value>(&raw) else {
        return (rows, true);
    };
    let base = Path::new(&data_dir);
    for (key, sub, default) in [
        ("cacheDir", "cache", &defaults.cache_dir),
        ("downloadsDir", "library", &defaults.downloads_dir),
    ] {
        let value = base.join(sub).to_string_lossy().into_owned();
        if rows.iter().any(|(k, _)| k == key) || &value == default {
            continue;
        }
        rows.push((key.to_owned(), Value::String(value).to_string()));
    }
    (rows, true)
}

fn to_map(s: &Settings) -> AppResult<Map<String, Value>> {
    match serde_json::to_value(s) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(AppError::Internal("settings are not a JSON object".into())),
    }
}

fn from_map(map: Map<String, Value>) -> Option<Settings> {
    serde_json::from_value(Value::Object(map)).ok()
}

/// Defaults overlaid with the stored rows. Unknown keys and values that don't parse or
/// validate are ignored (with a warning), one key at a time.
pub fn overlay(defaults: &Settings, rows: &[(String, String)]) -> AppResult<Settings> {
    let mut map = to_map(defaults)?;
    for (key, raw) in rows {
        if !map.contains_key(key) {
            tracing::warn!(%key, "ignoring unknown stored setting");
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            tracing::warn!(%key, "ignoring unreadable stored setting");
            continue;
        };
        let previous = map.insert(key.clone(), value);
        let valid = from_map(map.clone()).is_some_and(|s| validate(s).is_ok());
        if !valid {
            tracing::warn!(%key, "ignoring invalid stored setting, using the default");
            if let Some(previous) = previous {
                map.insert(key.clone(), previous);
            }
        }
    }
    let settings = from_map(map).ok_or_else(|| AppError::Internal("settings overlay".into()))?;
    validate(settings)
}

/// Rows to store so that `next` is persisted, given what was there before.
pub fn changed_rows(before: &Settings, next: &Settings) -> AppResult<Vec<(String, String)>> {
    let before = to_map(before)?;
    to_map(next)?
        .into_iter()
        .filter(|(k, v)| before.get(k) != Some(v))
        .map(|(k, v)| {
            serde_json::to_string(&v)
                .map(|json| (k, json))
                .map_err(|e| AppError::Internal(format!("serializing setting: {e}")))
        })
        .collect()
}

/// Current settings in memory, backed by the DB.
pub struct SettingsStore {
    db: Db,
    defaults: Settings,
    current: RwLock<Settings>,
    /// Serializes updates (read-modify-write).
    update_lock: tokio::sync::Mutex<()>,
}

impl SettingsStore {
    pub async fn load(db: Db, defaults: &Settings) -> AppResult<Self> {
        let (rows, migrated) = migrate_rows(defaults, db.settings_rows().await?);
        if migrated {
            tracing::info!("migrating dataDir to cacheDir/downloadsDir");
            db.delete_settings(vec!["dataDir".to_owned()]).await?;
            db.put_settings(
                rows.iter()
                    .filter(|(k, _)| k == "cacheDir" || k == "downloadsDir")
                    .cloned()
                    .collect(),
            )
            .await?;
        }
        let current = overlay(defaults, &rows)?;
        Ok(Self {
            db,
            defaults: defaults.clone(),
            current: RwLock::new(current),
            update_lock: tokio::sync::Mutex::new(()),
        })
    }

    pub fn get(&self) -> Settings {
        match self.current.read() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Validates, stores and returns `(before, after)`. Nothing changes if it fails.
    pub async fn update(&self, patch: SettingsPatch) -> AppResult<(Settings, Settings)> {
        let _guard = self.update_lock.lock().await;
        let before = self.get();
        let next = validate(apply_patch(&before, &self.defaults, patch))?;
        let dirs: Vec<(&str, String)> = [
            ("downloadsDir", &before.downloads_dir, &next.downloads_dir),
            ("cacheDir", &before.cache_dir, &next.cache_dir),
        ]
        .into_iter()
        .filter(|(_, a, b)| a != b)
        .map(|(f, _, b)| (f, b.clone()))
        .collect();
        if !dirs.is_empty() {
            let (downloads, cache) = (next.downloads_dir.clone(), next.cache_dir.clone());
            let owned: Vec<(String, String)> = dirs
                .iter()
                .map(|(f, d)| (f.to_string(), d.clone()))
                .collect();
            tokio::task::spawn_blocking(move || {
                for (field, dir) in &owned {
                    prepare_dir(field, Path::new(dir))?;
                }
                // Through symlinks too.
                let real = |d: &str| std::fs::canonicalize(d).ok();
                if let (Some(a), Some(b)) = (real(&downloads), real(&cache)) {
                    if crate::platform::nested(&a, &b) {
                        return Err(invalid("downloadsDir and cacheDir are inside each other"));
                    }
                }
                Ok(())
            })
            .await
            .map_err(|e| AppError::Internal(format!("settings task: {e}")))??;
        }
        let rows = changed_rows(&before, &next)?;
        if !rows.is_empty() {
            self.db.put_settings(rows).await?;
        }
        match self.current.write() {
            Ok(mut g) => *g = next.clone(),
            Err(poisoned) => *poisoned.into_inner() = next.clone(),
        }
        Ok((before, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Settings {
        // Absolute on every platform (`C:\home\u\…` on Windows).
        let root = if cfg!(windows) { r"C:\" } else { "/" };
        let data = ["home", "u", ".local", "share", "yts-player"]
            .iter()
            .fold(std::path::PathBuf::from(root), |p, c| p.join(c));
        defaults(&data)
    }

    fn patch(v: Value) -> SettingsPatch {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn defaults_are_valid() {
        let s = base();
        assert_eq!(validate(s.clone()).unwrap(), s);
        assert_eq!(s.cache_limit_bytes, 10 * 1024 * 1024 * 1024);
        assert_eq!(s.preferred_quality, Quality::P1080);
        assert!(s.prefer_x264);
        assert_eq!(s.external_player, "vlc");
    }

    #[test]
    fn absent_keeps_and_null_clears() {
        let mut s = base();
        s.open_subtitles_api_key = Some("key".into());
        s.down_limit_kbps = Some(100);
        s.up_limit_kbps = Some(50);
        s.listen_port = Some(6881);

        // Empty patch: nothing changes.
        assert_eq!(apply_patch(&s, &base(), patch(json!({}))), s);

        // null clears only the given keys; absent ones stay.
        let p = apply_patch(
            &s,
            &base(),
            patch(json!({ "openSubtitlesApiKey": null, "listenPort": null })),
        );
        assert_eq!(p.open_subtitles_api_key, None);
        assert_eq!(p.listen_port, None);
        assert_eq!(p.down_limit_kbps, Some(100));
        assert_eq!(p.up_limit_kbps, Some(50));

        // Values replace.
        let p = apply_patch(
            &s,
            &base(),
            patch(json!({ "downLimitKbps": 300, "preferredQuality": "720p", "preferX264": false })),
        );
        assert_eq!(p.down_limit_kbps, Some(300));
        assert_eq!(p.preferred_quality, Quality::P720);
        assert!(!p.prefer_x264);
        assert_eq!(p.open_subtitles_api_key.as_deref(), Some("key"));
    }

    #[test]
    fn validation_normalizes() {
        let s = validate(Settings {
            api_base_urls: vec![
                " https://mirror.example/api/v2 ".into(),
                "https://mirror.example/api/v2/".into(),
                "http://other.example/api/v2/".into(),
            ],
            open_subtitles_api_key: Some("   ".into()),
            open_subtitles_username: Some(" gabriel ".into()),
            open_subtitles_password: Some(" pass word ".into()),
            subtitle_lang: " EN ".into(),
            external_player: " mpv ".into(),
            ..base()
        })
        .unwrap();
        assert_eq!(
            s.api_base_urls,
            [
                "https://mirror.example/api/v2/",
                "http://other.example/api/v2/"
            ]
        );
        assert_eq!(s.open_subtitles_api_key, None);
        assert_eq!(s.open_subtitles_username.as_deref(), Some("gabriel"));
        assert_eq!(s.open_subtitles_password.as_deref(), Some(" pass word "));
        assert_eq!(s.subtitle_lang, "en");
        assert_eq!(s.external_player, "mpv");
    }

    #[test]
    fn validation_rejects_bad_values() {
        let cases: Vec<Settings> = vec![
            Settings {
                api_base_urls: vec![],
                ..base()
            },
            Settings {
                api_base_urls: vec!["ftp://x.example/".into()],
                ..base()
            },
            Settings {
                api_base_urls: vec!["not a url".into()],
                ..base()
            },
            Settings {
                api_base_urls: vec!["https://a.example/".into(); 1]
                    .into_iter()
                    .chain((0..10).map(|i| format!("https://m{i}.example/")))
                    .collect(),
                ..base()
            },
            Settings {
                subtitle_lang: "spanish".into(),
                ..base()
            },
            Settings {
                subtitle_lang: "e1".into(),
                ..base()
            },
            Settings {
                external_player: "  ".into(),
                ..base()
            },
            Settings {
                external_player: "vlc\n--evil".into(),
                ..base()
            },
            Settings {
                buffer_target_bytes: 1000,
                ..base()
            },
            Settings {
                buffer_target_bytes: 1024 * MIB,
                ..base()
            },
            Settings {
                down_limit_kbps: Some(0),
                ..base()
            },
            Settings {
                up_limit_kbps: Some(0),
                ..base()
            },
            Settings {
                listen_port: Some(80),
                ..base()
            },
            Settings {
                downloads_dir: "relative/dir".into(),
                ..base()
            },
            Settings {
                cache_dir: "".into(),
                ..base()
            },
            Settings {
                cache_dir: "/a/../etc".into(),
                ..base()
            },
            Settings {
                downloads_dir: "/media/disk".into(),
                cache_dir: "/media/disk/cache".into(),
                ..base()
            },
            Settings {
                downloads_dir: "/media/disk/x/".into(),
                cache_dir: "/media/disk/x".into(),
                ..base()
            },
            Settings {
                open_subtitles_password: Some("a\nb".into()),
                ..base()
            },
            Settings {
                open_subtitles_username: Some("x".repeat(600)),
                ..base()
            },
            Settings {
                cache_limit_bytes: 10 * MIB,
                ..base()
            },
            Settings {
                cache_limit_bytes: u64::MAX,
                ..base()
            },
        ];
        for s in cases {
            assert!(
                matches!(validate(s.clone()), Err(AppError::InvalidInput(_))),
                "{s:?}"
            );
        }
    }

    #[test]
    fn overlay_ignores_unknown_and_invalid_rows() {
        let rows = vec![
            ("subtitleLang".to_owned(), "\"en\"".to_owned()),
            ("cacheLimitBytes".to_owned(), "1".to_owned()), // below the minimum
            ("preferX264".to_owned(), "\"yes\"".to_owned()), // wrong type
            ("bogus".to_owned(), "1".to_owned()),
            ("listenPort".to_owned(), "not json".to_owned()),
            ("downLimitKbps".to_owned(), "250".to_owned()),
        ];
        let s = overlay(&base(), &rows).unwrap();
        assert_eq!(s.subtitle_lang, "en");
        assert_eq!(s.down_limit_kbps, Some(250));
        assert_eq!(s.cache_limit_bytes, DEFAULT_CACHE_LIMIT_BYTES);
        assert!(s.prefer_x264);
        assert_eq!(s.listen_port, None);
    }

    #[tokio::test]
    async fn store_persists_only_changes_and_survives_reload() {
        let db = Db::open_in_memory().unwrap();
        let store = SettingsStore::load(db.clone(), &base()).await.unwrap();
        assert_eq!(store.get(), base());

        let (before, after) = store
            .update(patch(
                json!({ "cacheLimitBytes": 2 * GIB, "openSubtitlesApiKey": "k" }),
            ))
            .await
            .unwrap();
        assert_eq!(before, base());
        assert_eq!(after.cache_limit_bytes, 2 * GIB);
        let mut rows = db.settings_rows().await.unwrap();
        rows.sort();
        assert_eq!(
            rows,
            [
                ("cacheLimitBytes".to_owned(), (2 * GIB).to_string()),
                ("openSubtitlesApiKey".to_owned(), "\"k\"".to_owned()),
            ]
        );

        // An invalid patch changes nothing, in memory or on disk.
        assert!(store
            .update(patch(json!({ "subtitleLang": "es", "listenPort": 1 })))
            .await
            .is_err());
        assert_eq!(store.get(), after);

        // null clears and is stored as null.
        let (_, after) = store
            .update(patch(json!({ "openSubtitlesApiKey": null })))
            .await
            .unwrap();
        assert_eq!(after.open_subtitles_api_key, None);

        let reloaded = SettingsStore::load(db, &base()).await.unwrap();
        assert_eq!(reloaded.get(), after);
    }

    #[test]
    fn folders_default_reset_and_normalize() {
        let d = base();
        assert_eq!(d.downloads_dir, "/home/u/.local/share/yts-player/library");
        assert_eq!(d.cache_dir, "/home/u/.local/share/yts-player/cache");
        let mut s = d.clone();
        s.cache_dir = "/media/disk/c".into();
        s.downloads_dir = "/media/disk/d".into();
        // null = back to the default; absent = unchanged.
        let p = apply_patch(&s, &d, patch(json!({ "cacheDir": null })));
        assert_eq!(p.cache_dir, d.cache_dir);
        assert_eq!(p.downloads_dir, "/media/disk/d");
        // Trailing slash removed; siblings with a common prefix are not nested.
        let v = validate(Settings {
            downloads_dir: "/media/disk/movies/".into(),
            cache_dir: "/media/disk/movies-cache".into(),
            ..d
        })
        .unwrap();
        assert_eq!(v.downloads_dir, "/media/disk/movies");
        assert!(!nested(&v.downloads_dir, &v.cache_dir));
    }

    #[test]
    fn data_dir_rows_migrate_to_the_two_folders() {
        let d = base();
        let rows = vec![
            ("dataDir".to_owned(), "\"/media/disk/yts\"".to_owned()),
            ("subtitleLang".to_owned(), "\"en\"".to_owned()),
        ];
        let (rows, changed) = migrate_rows(&d, rows);
        assert!(changed);
        let s = overlay(&d, &rows).unwrap();
        assert_eq!(s.cache_dir, "/media/disk/yts/cache");
        assert_eq!(s.downloads_dir, "/media/disk/yts/library");
        assert_eq!(s.subtitle_lang, "en");
        assert!(!rows.iter().any(|(k, _)| k == "dataDir"));

        // The default dataDir migrates to the defaults: nothing stored.
        let rows = vec![(
            "dataDir".to_owned(),
            "\"/home/u/.local/share/yts-player\"".to_owned(),
        )];
        let (rows, changed) = migrate_rows(&d, rows);
        assert!(changed && rows.is_empty());
        // Already migrated: untouched.
        let rows = vec![("cacheDir".to_owned(), "\"/x/c\"".to_owned())];
        assert_eq!(migrate_rows(&d, rows.clone()), (rows, false));
    }

    #[tokio::test]
    async fn store_migrates_data_dir_and_checks_folders_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        db.put_settings(vec![(
            "dataDir".into(),
            serde_json::to_string(&tmp.path().join("old")).unwrap(),
        )])
        .await
        .unwrap();
        let store = SettingsStore::load(db.clone(), &base()).await.unwrap();
        assert_eq!(
            store.get().cache_dir,
            tmp.path().join("old/cache").to_string_lossy()
        );
        let mut rows = db.settings_rows().await.unwrap();
        rows.sort();
        assert_eq!(
            rows.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            ["cacheDir", "downloadsDir"]
        );

        // A new folder is created and must be writable.
        let new_dir = tmp.path().join("new/downloads");
        let (_, after) = store
            .update(patch(json!({ "downloadsDir": new_dir })))
            .await
            .unwrap();
        assert!(new_dir.is_dir());
        assert!(dir_available(&new_dir));
        assert_eq!(after.downloads_dir, new_dir.to_string_lossy());
        assert!(std::fs::read_dir(&new_dir).unwrap().next().is_none());

        // Not creatable (a file is in the way) → invalid_input, nothing changes.
        let file = tmp.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        let err = store
            .update(patch(json!({ "cacheDir": file.join("sub") })))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
        assert_eq!(store.get(), after);

        // Nested through a symlink (Windows needs privileges to create one).
        #[cfg(unix)]
        {
            let link = tmp.path().join("link");
            std::os::unix::fs::symlink(&new_dir, &link).unwrap();
            let err = store
                .update(patch(json!({ "cacheDir": link.join("cache") })))
                .await
                .unwrap_err();
            assert!(matches!(err, AppError::InvalidInput(_)), "{err:?}");
        }
        assert!(!dir_available(&tmp.path().join("missing")));
    }

    #[test]
    fn api_base_urls_from_the_environment() {
        assert_eq!(api_base_urls_override(None), None);
        assert_eq!(api_base_urls_override(Some(" , ")), None);
        assert_eq!(api_base_urls_override(Some("not a url")), None);
        assert_eq!(
            api_base_urls_override(Some("http://127.0.0.1:8123/api/v2, http://127.0.0.1:9/x/")),
            Some(vec![
                "http://127.0.0.1:8123/api/v2/".to_owned(),
                "http://127.0.0.1:9/x/".to_owned()
            ])
        );
    }
}
