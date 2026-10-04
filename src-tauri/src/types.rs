//! Types shared with the frontend over IPC. Mirrors `docs/IPC.md` exactly:
//! camelCase fields, `Option<T>` serialized as `null` (never skipped), enums as strings.

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{AppError, ErrorCode};

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Quality {
    #[serde(rename = "480p")]
    P480,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "2160p")]
    P2160,
    #[serde(rename = "3D")]
    ThreeD,
}

/// Quality filter accepted by `list_movies` (YTS also allows `1080p.x265` there).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityFilter {
    #[serde(rename = "480p")]
    P480,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "1080p.x265")]
    P1080X265,
    #[serde(rename = "2160p")]
    P2160,
    #[serde(rename = "3D")]
    ThreeD,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    X264,
    X265,
}

/// `type` field of a YTS torrent. Unknown values deserialize as `Web` (with a warning)
/// so a new label from YTS never breaks parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TorrentSource {
    Bluray,
    Web,
}

impl<'de> Deserialize<'de> for TorrentSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "bluray" => Self::Bluray,
            "web" => Self::Web,
            _ => {
                tracing::warn!(value = %raw, "unknown torrent source, falling back to web");
                Self::Web
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Torrent {
    pub infohash: String,
    pub quality: Quality,
    pub source: TorrentSource,
    pub video_codec: VideoCodec,
    pub bit_depth: Option<u8>,
    pub audio_channels: Option<String>,
    pub size_bytes: u64,
    pub seeds: u32,
    pub peers: u32,
    pub uploaded_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovieSummary {
    pub id: u64,
    pub imdb_code: String,
    pub title: String,
    pub year: u32,
    pub rating: f64,
    pub runtime_min: u32,
    pub genres: Vec<String>,
    /// `null` when the API has no cover.
    pub cover_url: Option<String>,
    /// Falls back to the medium cover; `null` if neither exists.
    pub cover_large_url: Option<String>,
    pub background_url: Option<String>,
    pub qualities: Vec<Quality>,
    #[serde(rename = "hasX264")]
    pub has_x264: bool,
    /// Highest seed count among its torrents; 0 without torrents.
    pub max_seeds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CastMember {
    pub name: String,
    pub character: Option<String>,
    pub image_url: Option<String>,
}

/// `MovieSummary & { … }` in TypeScript: the summary fields are flattened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovieDetail {
    #[serde(flatten)]
    pub summary_fields: MovieSummary,
    pub summary: String,
    pub language: String,
    pub mpa_rating: Option<String>,
    pub yt_trailer_code: Option<String>,
    /// Large screenshots served by the local server; empty if none.
    pub screenshot_urls: Vec<String>,
    pub cast: Vec<CastMember>,
    pub torrents: Vec<Torrent>,
    pub is_favorite: bool,
    pub progress: Option<Progress>,
    pub download: Option<Download>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoviePage {
    pub movies: Vec<MovieSummary>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
    pub has_more: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    Title,
    Year,
    Rating,
    Peers,
    Seeds,
    DownloadCount,
    LikeCount,
    DateAdded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderBy {
    Desc,
    Asc,
}

/// Input of `list_movies`. Every field is optional; defaults are applied by the YTS client.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListMoviesParams {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub query: Option<String>,
    pub genre: Option<String>,
    pub quality: Option<QualityFilter>,
    pub minimum_rating: Option<u8>,
    pub sort_by: Option<SortBy>,
    pub order_by: Option<OrderBy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointRole {
    Active,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiEndpointStatus {
    pub base_url: String,
    pub role: EndpointRole,
    pub latency_ms: Option<u64>,
    pub ok: bool,
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamSource {
    Network,
    Library,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamSession {
    pub infohash: String,
    pub movie_id: u64,
    pub stream_url: String,
    pub file_name: String,
    pub file_size_bytes: u64,
    pub video_codec: VideoCodec,
    pub likely_playable: bool,
    pub buffer_target_bytes: u64,
    pub resume_at_s: Option<f64>,
    pub source: StreamSource,
}

// ---------------------------------------------------------------------------
// Subtitles
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleOption {
    pub id: String,
    pub lang: String,
    pub label: String,
    pub downloads: u64,
    pub hearing_impaired: bool,
    pub matches_release: bool,
    /// Translated by AI or by machine.
    pub ai_translated: bool,
    /// Its page on opensubtitles.com (to download it by hand when the quota is gone).
    pub page_url: Option<String>,
    /// Already in the disk cache: loading it costs no quota.
    pub cached: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleTrack {
    pub track_url: String,
    pub lang: Option<String>,
    pub label: String,
}

/// Result of `get_subtitles_status` (the "Probar" button in Settings).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitlesStatus {
    pub configured: bool,
    pub logged_in: bool,
    pub remaining_downloads: Option<i64>,
    /// ISO 8601, last known value.
    pub reset_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Progress / continue watching
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub movie_id: u64,
    pub position_s: f64,
    pub duration_s: f64,
    pub finished: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinueItem {
    pub movie: MovieSummary,
    pub progress: Progress,
}

// ---------------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Queued,
    Active,
    Paused,
    Stalled,
    Done,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub infohash: String,
    pub movie: MovieSummary,
    pub quality: Quality,
    pub video_codec: VideoCodec,
    pub state: DownloadState,
    pub progress: f64,
    pub size_bytes: u64,
    pub downloaded_bytes: u64,
    pub down_speed_bps: u64,
    pub peers: u32,
    pub eta_s: Option<u64>,
    pub path: Option<String>,
    pub error: Option<String>,
    pub added_at: String,
}

// ---------------------------------------------------------------------------
// Settings and storage
// ---------------------------------------------------------------------------

/// `Debug` is written by hand (below) so the OpenSubtitles secrets never reach a log.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub api_base_urls: Vec<String>,
    pub open_subtitles_api_key: Option<String>,
    pub open_subtitles_username: Option<String>,
    pub open_subtitles_password: Option<String>,
    pub subtitle_lang: String,
    pub auto_subtitles: bool,
    pub preferred_quality: Quality,
    #[serde(rename = "preferX264")]
    pub prefer_x264: bool,
    pub external_player: String,
    pub buffer_target_bytes: u64,
    pub down_limit_kbps: Option<u32>,
    pub up_limit_kbps: Option<u32>,
    pub seed_after_download: bool,
    pub listen_port: Option<u16>,
    pub data_dir: String,
    pub cache_limit_bytes: u64,
}

/// `Partial<Settings>` for `update_settings`. For nullable settings, an absent key
/// (`None`) leaves the value unchanged while an explicit `null` (`Some(None)`) clears it.
#[derive(Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    pub api_base_urls: Option<Vec<String>>,
    #[serde(deserialize_with = "present")]
    pub open_subtitles_api_key: Option<Option<String>>,
    #[serde(deserialize_with = "present")]
    pub open_subtitles_username: Option<Option<String>>,
    #[serde(deserialize_with = "present")]
    pub open_subtitles_password: Option<Option<String>>,
    pub subtitle_lang: Option<String>,
    pub auto_subtitles: Option<bool>,
    pub preferred_quality: Option<Quality>,
    #[serde(rename = "preferX264")]
    pub prefer_x264: Option<bool>,
    pub external_player: Option<String>,
    pub buffer_target_bytes: Option<u64>,
    #[serde(deserialize_with = "present")]
    pub down_limit_kbps: Option<Option<u32>>,
    #[serde(deserialize_with = "present")]
    pub up_limit_kbps: Option<Option<u32>>,
    pub seed_after_download: Option<bool>,
    #[serde(deserialize_with = "present")]
    pub listen_port: Option<Option<u16>>,
    pub data_dir: Option<String>,
    pub cache_limit_bytes: Option<u64>,
}

/// `Some("***")` / `None`: shows whether a secret is set without showing it.
fn redact<T>(secret: &Option<T>) -> Option<&'static str> {
    secret.as_ref().map(|_| "***")
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Settings")
            .field("api_base_urls", &self.api_base_urls)
            .field(
                "open_subtitles_api_key",
                &redact(&self.open_subtitles_api_key),
            )
            .field(
                "open_subtitles_username",
                &redact(&self.open_subtitles_username),
            )
            .field(
                "open_subtitles_password",
                &redact(&self.open_subtitles_password),
            )
            .field("subtitle_lang", &self.subtitle_lang)
            .field("auto_subtitles", &self.auto_subtitles)
            .field("preferred_quality", &self.preferred_quality)
            .field("prefer_x264", &self.prefer_x264)
            .field("external_player", &self.external_player)
            .field("buffer_target_bytes", &self.buffer_target_bytes)
            .field("down_limit_kbps", &self.down_limit_kbps)
            .field("up_limit_kbps", &self.up_limit_kbps)
            .field("seed_after_download", &self.seed_after_download)
            .field("listen_port", &self.listen_port)
            .field("data_dir", &self.data_dir)
            .field("cache_limit_bytes", &self.cache_limit_bytes)
            .finish()
    }
}

impl std::fmt::Debug for SettingsPatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let secret = |v: &Option<Option<String>>| v.as_ref().map(|inner| redact(inner));
        f.debug_struct("SettingsPatch")
            .field("api_base_urls", &self.api_base_urls)
            .field(
                "open_subtitles_api_key",
                &secret(&self.open_subtitles_api_key),
            )
            .field(
                "open_subtitles_username",
                &secret(&self.open_subtitles_username),
            )
            .field(
                "open_subtitles_password",
                &secret(&self.open_subtitles_password),
            )
            .field("subtitle_lang", &self.subtitle_lang)
            .field("auto_subtitles", &self.auto_subtitles)
            .field("preferred_quality", &self.preferred_quality)
            .field("prefer_x264", &self.prefer_x264)
            .field("external_player", &self.external_player)
            .field("buffer_target_bytes", &self.buffer_target_bytes)
            .field("down_limit_kbps", &self.down_limit_kbps)
            .field("up_limit_kbps", &self.up_limit_kbps)
            .field("seed_after_download", &self.seed_after_download)
            .field("listen_port", &self.listen_port)
            .field("data_dir", &self.data_dir)
            .field("cache_limit_bytes", &self.cache_limit_bytes)
            .finish()
    }
}

/// Only called when the key is present, so `null` becomes `Some(None)`.
fn present<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsage {
    pub cache_bytes: u64,
    pub cache_limit_bytes: u64,
    pub library_bytes: u64,
    pub free_disk_bytes: u64,
}

/// Return value of `clear_cache`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearCacheResult {
    pub freed_bytes: u64,
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamPhase {
    Connecting,
    Metadata,
    Buffering,
    Ready,
    Stalled,
    Seeding,
    Done,
}

/// Payload of `torrent://stats`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TorrentStats {
    pub infohash: String,
    pub phase: StreamPhase,
    pub peers: u32,
    pub seeds: u32,
    pub down_speed_bps: u64,
    pub up_speed_bps: u64,
    pub progress: f64,
    pub downloaded_bytes: u64,
    pub buffered_ahead_bytes: u64,
    pub available_ranges: Vec<(f64, f64)>,
    pub piece_map: Option<String>,
    /// Byte window `pieceMap` is sampled over; `None` when `piece_map` is `None`.
    pub piece_map_window: Option<PieceMapWindow>,
}

/// `[startByte, endByte)` of the file covered by `pieceMap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceMapWindow {
    pub start_byte: u64,
    pub end_byte: u64,
}

/// Payload of `download://changed`. `download: None` means it was removed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadChanged {
    pub infohash: String,
    pub download: Option<Download>,
}

/// Payload of `app://error` (`AppError & { infohash }`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundError {
    pub code: ErrorCode,
    pub message: String,
    pub infohash: Option<String>,
}

impl BackgroundError {
    pub fn new(err: &AppError, infohash: Option<String>) -> Self {
        Self {
            code: err.code(),
            message: err.to_string(),
            infohash,
        }
    }
}

/// Outcome of the subtitles part of `open_external_player`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalSubtitle {
    Loaded,
    /// Not requested and automatic subtitles are off.
    None,
    NoKey,
    Quota,
    NotFound,
    UnsupportedPlayer,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalPlayerResult {
    pub subtitle: ExternalSubtitle,
}

pub mod events {
    pub const TORRENT_STATS: &str = "torrent://stats";
    pub const DOWNLOAD_CHANGED: &str = "download://changed";
    pub const APP_ERROR: &str = "app://error";
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const HASH: &str = "0123456789abcdef0123456789abcdef01234567";

    fn to_json<T: Serialize>(value: &T) -> Value {
        serde_json::to_value(value).unwrap()
    }

    fn summary() -> MovieSummary {
        MovieSummary {
            id: 38698,
            imdb_code: "tt10838180".into(),
            title: "The Matrix Resurrections".into(),
            year: 2021,
            rating: 5.6,
            runtime_min: 148,
            genres: vec!["Action".into(), "Sci-Fi".into()],
            cover_url: Some("http://127.0.0.1:1/img/a".into()),
            cover_large_url: Some("http://127.0.0.1:1/img/b".into()),
            background_url: None,
            qualities: vec![Quality::P720, Quality::P1080, Quality::P2160],
            has_x264: true,
            max_seeds: 100,
        }
    }

    fn summary_json() -> Value {
        json!({
            "id": 38698,
            "imdbCode": "tt10838180",
            "title": "The Matrix Resurrections",
            "year": 2021,
            "rating": 5.6,
            "runtimeMin": 148,
            "genres": ["Action", "Sci-Fi"],
            "coverUrl": "http://127.0.0.1:1/img/a",
            "coverLargeUrl": "http://127.0.0.1:1/img/b",
            "backgroundUrl": null,
            "qualities": ["720p", "1080p", "2160p"],
            "hasX264": true,
            "maxSeeds": 100
        })
    }

    fn progress() -> Progress {
        Progress {
            movie_id: 38698,
            position_s: 61.5,
            duration_s: 8880.0,
            finished: false,
            updated_at: "2026-10-03T12:00:00Z".into(),
        }
    }

    fn progress_json() -> Value {
        json!({
            "movieId": 38698,
            "positionS": 61.5,
            "durationS": 8880.0,
            "finished": false,
            "updatedAt": "2026-10-03T12:00:00Z"
        })
    }

    fn download() -> Download {
        Download {
            infohash: HASH.into(),
            movie: summary(),
            quality: Quality::P1080,
            video_codec: VideoCodec::X264,
            state: DownloadState::Active,
            progress: 0.25,
            size_bytes: 2_000_000_000,
            downloaded_bytes: 500_000_000,
            down_speed_bps: 1_000_000,
            peers: 12,
            eta_s: None,
            path: None,
            error: None,
            added_at: "2026-10-03T12:00:00Z".into(),
        }
    }

    fn download_json() -> Value {
        json!({
            "infohash": HASH,
            "movie": summary_json(),
            "quality": "1080p",
            "videoCodec": "x264",
            "state": "active",
            "progress": 0.25,
            "sizeBytes": 2_000_000_000u64,
            "downloadedBytes": 500_000_000u64,
            "downSpeedBps": 1_000_000,
            "peers": 12,
            "etaS": null,
            "path": null,
            "error": null,
            "addedAt": "2026-10-03T12:00:00Z"
        })
    }

    #[test]
    fn enums_use_ipc_strings() {
        assert_eq!(
            to_json(&[
                Quality::P480,
                Quality::P720,
                Quality::P1080,
                Quality::P2160,
                Quality::ThreeD
            ]),
            json!(["480p", "720p", "1080p", "2160p", "3D"])
        );
        assert_eq!(to_json(&QualityFilter::P1080X265), json!("1080p.x265"));
        assert_eq!(
            to_json(&[VideoCodec::X264, VideoCodec::X265]),
            json!(["x264", "x265"])
        );
        assert_eq!(
            to_json(&[TorrentSource::Bluray, TorrentSource::Web]),
            json!(["bluray", "web"])
        );
        assert_eq!(
            to_json(&[SortBy::DownloadCount, SortBy::LikeCount, SortBy::DateAdded]),
            json!(["download_count", "like_count", "date_added"])
        );
        assert_eq!(
            to_json(&[OrderBy::Desc, OrderBy::Asc]),
            json!(["desc", "asc"])
        );
        assert_eq!(
            to_json(&[EndpointRole::Active, EndpointRole::Fallback]),
            json!(["active", "fallback"])
        );
        assert_eq!(
            to_json(&[StreamSource::Network, StreamSource::Library]),
            json!(["network", "library"])
        );
        assert_eq!(
            to_json(&[
                DownloadState::Queued,
                DownloadState::Active,
                DownloadState::Paused,
                DownloadState::Stalled,
                DownloadState::Done,
                DownloadState::Error
            ]),
            json!(["queued", "active", "paused", "stalled", "done", "error"])
        );
        assert_eq!(
            to_json(&[
                StreamPhase::Connecting,
                StreamPhase::Metadata,
                StreamPhase::Buffering,
                StreamPhase::Ready,
                StreamPhase::Stalled,
                StreamPhase::Seeding,
                StreamPhase::Done
            ]),
            json!([
                "connecting",
                "metadata",
                "buffering",
                "ready",
                "stalled",
                "seeding",
                "done"
            ])
        );
    }

    #[test]
    fn torrent_source_deserializes_unknown_as_web() {
        let parsed: Vec<TorrentSource> =
            serde_json::from_value(json!(["bluray", "web", "BluRay", "webrip", ""])).unwrap();
        assert_eq!(
            parsed,
            [
                TorrentSource::Bluray,
                TorrentSource::Web,
                TorrentSource::Bluray,
                TorrentSource::Web,
                TorrentSource::Web
            ]
        );
        assert!(serde_json::from_value::<TorrentSource>(json!(1)).is_err());
    }

    #[test]
    fn torrent() {
        let t = Torrent {
            infohash: HASH.into(),
            quality: Quality::P2160,
            source: TorrentSource::Bluray,
            video_codec: VideoCodec::X265,
            bit_depth: Some(10),
            audio_channels: None,
            size_bytes: 5_000_000_000,
            seeds: 100,
            peers: 20,
            uploaded_at: None,
        };
        assert_eq!(
            to_json(&t),
            json!({
                "infohash": HASH,
                "quality": "2160p",
                "source": "bluray",
                "videoCodec": "x265",
                "bitDepth": 10,
                "audioChannels": null,
                "sizeBytes": 5_000_000_000u64,
                "seeds": 100,
                "peers": 20,
                "uploadedAt": null
            })
        );
    }

    #[test]
    fn movie_summary_roundtrips() {
        assert_eq!(to_json(&summary()), summary_json());
        // The frontend sends it back in add_favorite / save_progress / start_download.
        let back: MovieSummary = serde_json::from_value(summary_json()).unwrap();
        assert_eq!(back, summary());
    }

    #[test]
    fn movie_summary_without_covers_serializes_null() {
        let movie = MovieSummary {
            cover_url: None,
            cover_large_url: None,
            ..summary()
        };
        let value = to_json(&movie);
        assert_eq!(value["coverUrl"], Value::Null);
        assert_eq!(value["coverLargeUrl"], Value::Null);
        assert!(value.as_object().unwrap().contains_key("coverUrl"));
    }

    #[test]
    fn movie_detail_flattens_summary() {
        let detail = MovieDetail {
            summary_fields: summary(),
            summary: "Neo again.".into(),
            language: "en".into(),
            mpa_rating: None,
            yt_trailer_code: Some("9ix7TUGVYIo".into()),
            screenshot_urls: vec!["http://127.0.0.1:1/img/c".into()],
            cast: vec![CastMember {
                name: "Keanu Reeves".into(),
                character: Some("Neo".into()),
                image_url: None,
            }],
            torrents: vec![],
            is_favorite: false,
            progress: Some(progress()),
            download: None,
        };
        let mut expected = summary_json();
        let extra = json!({
            "summary": "Neo again.",
            "language": "en",
            "mpaRating": null,
            "ytTrailerCode": "9ix7TUGVYIo",
            "screenshotUrls": ["http://127.0.0.1:1/img/c"],
            "cast": [{ "name": "Keanu Reeves", "character": "Neo", "imageUrl": null }],
            "torrents": [],
            "isFavorite": false,
            "progress": progress_json(),
            "download": null
        });
        expected
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(to_json(&detail), expected);
    }

    #[test]
    fn movie_page_with_no_results() {
        let page = MoviePage {
            movies: vec![],
            total: 0,
            page: 1,
            limit: 20,
            has_more: false,
        };
        assert_eq!(
            to_json(&page),
            json!({ "movies": [], "total": 0, "page": 1, "limit": 20, "hasMore": false })
        );
    }

    #[test]
    fn list_movies_params_accepts_partial_input() {
        let empty: ListMoviesParams = serde_json::from_value(json!({})).unwrap();
        assert_eq!(empty, ListMoviesParams::default());

        let full: ListMoviesParams = serde_json::from_value(json!({
            "page": 2,
            "limit": 50,
            "query": "matrix",
            "genre": "sci-fi",
            "quality": "1080p.x265",
            "minimumRating": 7,
            "sortBy": "download_count",
            "orderBy": "asc"
        }))
        .unwrap();
        assert_eq!(
            full,
            ListMoviesParams {
                page: Some(2),
                limit: Some(50),
                query: Some("matrix".into()),
                genre: Some("sci-fi".into()),
                quality: Some(QualityFilter::P1080X265),
                minimum_rating: Some(7),
                sort_by: Some(SortBy::DownloadCount),
                order_by: Some(OrderBy::Asc),
            }
        );
    }

    #[test]
    fn api_endpoint_status() {
        let s = ApiEndpointStatus {
            base_url: "https://movies-api.accel.li/api/v2/".into(),
            role: EndpointRole::Active,
            latency_ms: None,
            ok: false,
        };
        assert_eq!(
            to_json(&s),
            json!({
                "baseUrl": "https://movies-api.accel.li/api/v2/",
                "role": "active",
                "latencyMs": null,
                "ok": false
            })
        );
    }

    #[test]
    fn stream_session() {
        let s = StreamSession {
            infohash: HASH.into(),
            movie_id: 1,
            stream_url: format!("http://127.0.0.1:1/stream/{HASH}/0"),
            file_name: "movie.mp4".into(),
            file_size_bytes: 10,
            video_codec: VideoCodec::X264,
            likely_playable: true,
            buffer_target_bytes: 8_000_000,
            resume_at_s: None,
            source: StreamSource::Network,
        };
        assert_eq!(
            to_json(&s),
            json!({
                "infohash": HASH,
                "movieId": 1,
                "streamUrl": format!("http://127.0.0.1:1/stream/{HASH}/0"),
                "fileName": "movie.mp4",
                "fileSizeBytes": 10,
                "videoCodec": "x264",
                "likelyPlayable": true,
                "bufferTargetBytes": 8_000_000,
                "resumeAtS": null,
                "source": "network"
            })
        );
    }

    #[test]
    fn subtitles() {
        let option = SubtitleOption {
            id: "123".into(),
            lang: "es".into(),
            label: "Release.1080p".into(),
            downloads: 5,
            hearing_impaired: false,
            matches_release: true,
            ai_translated: false,
            page_url: Some("https://www.opensubtitles.com/es/subtitles/x".into()),
            cached: true,
        };
        assert_eq!(
            to_json(&option),
            json!({
                "id": "123",
                "lang": "es",
                "label": "Release.1080p",
                "downloads": 5,
                "hearingImpaired": false,
                "matchesRelease": true,
                "aiTranslated": false,
                "pageUrl": "https://www.opensubtitles.com/es/subtitles/x",
                "cached": true
            })
        );
        let no_page = SubtitleOption {
            page_url: None,
            ..option
        };
        assert_eq!(to_json(&no_page)["pageUrl"], Value::Null);
        let track = SubtitleTrack {
            track_url: "http://127.0.0.1:1/subs/123.vtt".into(),
            lang: None,
            label: "manual.srt".into(),
        };
        assert_eq!(
            to_json(&track),
            json!({ "trackUrl": "http://127.0.0.1:1/subs/123.vtt", "lang": null, "label": "manual.srt" })
        );
        let status = SubtitlesStatus {
            configured: true,
            logged_in: false,
            remaining_downloads: None,
            reset_at: Some("2026-10-04T13:03:16Z".into()),
        };
        assert_eq!(
            to_json(&status),
            json!({
                "configured": true,
                "loggedIn": false,
                "remainingDownloads": null,
                "resetAt": "2026-10-04T13:03:16Z"
            })
        );
    }

    #[test]
    fn external_player_result() {
        let cases = [
            (ExternalSubtitle::Loaded, "loaded"),
            (ExternalSubtitle::None, "none"),
            (ExternalSubtitle::NoKey, "no_key"),
            (ExternalSubtitle::Quota, "quota"),
            (ExternalSubtitle::NotFound, "not_found"),
            (ExternalSubtitle::UnsupportedPlayer, "unsupported_player"),
            (ExternalSubtitle::Error, "error"),
        ];
        for (subtitle, s) in cases {
            assert_eq!(
                to_json(&ExternalPlayerResult { subtitle }),
                json!({ "subtitle": s })
            );
        }
    }

    #[test]
    fn progress_and_continue_item() {
        assert_eq!(to_json(&progress()), progress_json());
        let item = ContinueItem {
            movie: summary(),
            progress: progress(),
        };
        assert_eq!(
            to_json(&item),
            json!({ "movie": summary_json(), "progress": progress_json() })
        );
    }

    #[test]
    fn download_and_download_changed() {
        assert_eq!(to_json(&download()), download_json());
        let changed = DownloadChanged {
            infohash: HASH.into(),
            download: Some(download()),
        };
        assert_eq!(
            to_json(&changed),
            json!({ "infohash": HASH, "download": download_json() })
        );
        let removed = DownloadChanged {
            infohash: HASH.into(),
            download: None,
        };
        assert_eq!(
            to_json(&removed),
            json!({ "infohash": HASH, "download": null })
        );
    }

    #[test]
    fn settings() {
        let s = Settings {
            api_base_urls: vec!["https://movies-api.accel.li/api/v2/".into()],
            open_subtitles_api_key: None,
            open_subtitles_username: Some("user".into()),
            open_subtitles_password: Some("secret-pass".into()),
            subtitle_lang: "es".into(),
            auto_subtitles: true,
            preferred_quality: Quality::P1080,
            prefer_x264: true,
            external_player: "vlc".into(),
            buffer_target_bytes: 8_000_000,
            down_limit_kbps: None,
            up_limit_kbps: Some(500),
            seed_after_download: false,
            listen_port: None,
            data_dir: "/home/u/.local/share/yts-player".into(),
            cache_limit_bytes: 10_000_000_000,
        };
        assert_eq!(
            to_json(&s),
            json!({
                "apiBaseUrls": ["https://movies-api.accel.li/api/v2/"],
                "openSubtitlesApiKey": null,
                "openSubtitlesUsername": "user",
                "openSubtitlesPassword": "secret-pass",
                "subtitleLang": "es",
                "autoSubtitles": true,
                "preferredQuality": "1080p",
                "preferX264": true,
                "externalPlayer": "vlc",
                "bufferTargetBytes": 8_000_000,
                "downLimitKbps": null,
                "upLimitKbps": 500,
                "seedAfterDownload": false,
                "listenPort": null,
                "dataDir": "/home/u/.local/share/yts-player",
                "cacheLimitBytes": 10_000_000_000u64
            })
        );
        // Secrets never show up in Debug output (logs).
        let debug = format!("{s:?}");
        assert!(
            !debug.contains("secret-pass") && !debug.contains("\"user\""),
            "{debug}"
        );
        assert!(
            debug.contains("open_subtitles_password: Some(\"***\")"),
            "{debug}"
        );
        let patch: SettingsPatch = serde_json::from_value(json!({
            "openSubtitlesApiKey": "k3y-value",
            "openSubtitlesPassword": null
        }))
        .unwrap();
        let debug = format!("{patch:?}");
        assert!(!debug.contains("k3y-value"), "{debug}");
        assert!(
            debug.contains("open_subtitles_password: Some(None)"),
            "{debug}"
        );
    }

    #[test]
    fn settings_patch_distinguishes_absent_from_null() {
        let patch: SettingsPatch = serde_json::from_value(json!({
            "preferX264": false,
            "downLimitKbps": null,
            "upLimitKbps": 250
        }))
        .unwrap();
        assert_eq!(
            patch,
            SettingsPatch {
                prefer_x264: Some(false),
                down_limit_kbps: Some(None),
                up_limit_kbps: Some(Some(250)),
                ..Default::default()
            }
        );
        assert_eq!(patch.listen_port, None);
        assert_eq!(patch.open_subtitles_api_key, None);
    }

    #[test]
    fn storage_and_clear_cache() {
        let usage = StorageUsage {
            cache_bytes: 1,
            cache_limit_bytes: 2,
            library_bytes: 3,
            free_disk_bytes: 4,
        };
        assert_eq!(
            to_json(&usage),
            json!({ "cacheBytes": 1, "cacheLimitBytes": 2, "libraryBytes": 3, "freeDiskBytes": 4 })
        );
        assert_eq!(
            to_json(&ClearCacheResult { freed_bytes: 7 }),
            json!({ "freedBytes": 7 })
        );
    }

    #[test]
    fn torrent_stats() {
        let stats = TorrentStats {
            infohash: HASH.into(),
            phase: StreamPhase::Buffering,
            peers: 3,
            seeds: 2,
            down_speed_bps: 100,
            up_speed_bps: 0,
            progress: 0.5,
            downloaded_bytes: 50,
            buffered_ahead_bytes: 10,
            available_ranges: vec![(0.0, 0.25), (0.5, 0.75)],
            piece_map: None,
            piece_map_window: None,
        };
        let with_map = TorrentStats {
            piece_map: Some("012".into()),
            piece_map_window: Some(PieceMapWindow {
                start_byte: 1024,
                end_byte: 4096,
            }),
            ..stats.clone()
        };
        let value = to_json(&with_map);
        assert_eq!(value["pieceMap"], "012");
        assert_eq!(
            value["pieceMapWindow"],
            json!({ "startByte": 1024, "endByte": 4096 })
        );
        assert_eq!(
            to_json(&stats),
            json!({
                "infohash": HASH,
                "phase": "buffering",
                "peers": 3,
                "seeds": 2,
                "downSpeedBps": 100,
                "upSpeedBps": 0,
                "progress": 0.5,
                "downloadedBytes": 50,
                "bufferedAheadBytes": 10,
                "availableRanges": [[0.0, 0.25], [0.5, 0.75]],
                "pieceMap": null,
                "pieceMapWindow": null
            })
        );
    }

    #[test]
    fn background_error() {
        let err = AppError::Io(std::io::Error::other("disk full"));
        assert_eq!(
            to_json(&BackgroundError::new(&err, Some(HASH.into()))),
            json!({ "code": "io", "message": "io error: disk full", "infohash": HASH })
        );
        assert_eq!(
            to_json(&BackgroundError::new(&AppError::Internal("x".into()), None)),
            json!({ "code": "internal", "message": "internal error: x", "infohash": null })
        );
    }
}
