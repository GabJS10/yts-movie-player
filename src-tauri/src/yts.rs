//! YTS API client: base URL failover, in-memory cache, tolerant serde models and
//! conversion to IPC types. See `docs/YTS-API.md`.

use std::str::FromStr;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use bytes::Bytes;
use moka::future::Cache;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use tokio::time::Instant;

use crate::error::{AppError, AppResult};
use crate::images::ImageStore;
use crate::types::{
    ApiEndpointStatus, CastMember, EndpointRole, ListMoviesParams, MovieDetail, MoviePage,
    MovieSummary, OrderBy, Quality, QualityFilter, SortBy, Torrent, TorrentSource, VideoCodec,
};

/// Defaults of the `apiBaseUrls` setting.
pub const DEFAULT_BASE_URLS: [&str; 2] = [
    "https://movies-api.accel.li/api/v2/",
    "https://yts.gg/api/v2/",
];

const DEFAULT_LIMIT: u32 = 20;
const MAX_LIMIT: u32 = 50;

#[derive(Debug, Clone)]
pub struct YtsConfig {
    /// In order of preference; index 0 is the primary.
    pub base_urls: Vec<String>,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    /// How often to go back to the primary after failing over.
    pub primary_recheck_interval: Duration,
    pub cache_ttl: Duration,
    pub cache_capacity: u64,
}

impl Default for YtsConfig {
    fn default() -> Self {
        Self {
            base_urls: DEFAULT_BASE_URLS.iter().map(|s| s.to_string()).collect(),
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(10),
            primary_recheck_interval: Duration::from_secs(10 * 60),
            cache_ttl: Duration::from_secs(30 * 60),
            cache_capacity: 500,
        }
    }
}

/// What the torrent engine needs to know about a YTS torrent.
#[derive(Debug, Clone, PartialEq)]
pub struct TorrentRef {
    pub title: String,
    pub torrent_url: Option<String>,
    pub video_codec: VideoCodec,
    /// Swarm seeds reported by YTS (static); used as `TorrentStats.seeds`.
    pub seeds: u32,
}

struct EndpointState {
    active: usize,
    last_primary_check: Instant,
}

pub struct YtsClient {
    http: reqwest::Client,
    /// Replaced as a whole when the settings change; readers take a snapshot.
    base_urls: RwLock<Arc<Vec<String>>>,
    recheck_interval: Duration,
    endpoints: Mutex<EndpointState>,
    cache: Cache<String, Bytes>,
    images: Arc<ImageStore>,
}

/// Cloneable failure of a whole fetch (shared between coalesced callers).
#[derive(Debug, Clone)]
enum FetchError {
    /// Every base URL was unreachable.
    Network(String),
    /// At least one base URL answered, but none with a usable response.
    Unavailable(String),
}

impl From<FetchError> for AppError {
    fn from(e: FetchError) -> Self {
        match e {
            FetchError::Network(msg) => AppError::Network(msg),
            FetchError::Unavailable(msg) => AppError::ApiUnavailable(msg),
        }
    }
}

/// Why a single base URL attempt failed.
enum AttemptError {
    /// Connection refused, DNS failure or timeout.
    Network(String),
    /// Reached the server but the response was unusable (5xx, invalid JSON, status != ok).
    Bad(String),
}

impl YtsClient {
    pub fn new(config: YtsConfig, images: Arc<ImageStore>) -> AppResult<Self> {
        let base_urls = normalize_base_urls(&config.base_urls)?;
        let http = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .build()
            .map_err(|e| AppError::Internal(format!("http client: {e}")))?;
        Ok(Self {
            http,
            base_urls: RwLock::new(Arc::new(base_urls)),
            recheck_interval: config.primary_recheck_interval,
            endpoints: Mutex::new(EndpointState {
                active: 0,
                last_primary_check: Instant::now(),
            }),
            cache: Cache::builder()
                .max_capacity(config.cache_capacity)
                .time_to_live(config.cache_ttl)
                .build(),
            images,
        })
    }

    /// Replaces the base URLs at runtime: back to the first one, with an empty cache.
    pub fn set_base_urls(&self, urls: &[String]) -> AppResult<()> {
        let urls = normalize_base_urls(urls)?;
        if **self.urls() == urls {
            return Ok(());
        }
        match self.base_urls.write() {
            Ok(mut g) => *g = Arc::new(urls),
            Err(_) => return Err(AppError::Internal("base URL lock poisoned".into())),
        }
        if let Ok(mut state) = self.endpoints.lock() {
            state.active = 0;
            state.last_primary_check = Instant::now();
        }
        self.cache.invalidate_all();
        tracing::info!(urls = ?self.urls(), "YTS base URLs changed");
        Ok(())
    }

    fn urls(&self) -> Arc<Vec<String>> {
        match self.base_urls.read() {
            Ok(g) => Arc::clone(&g),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }

    pub async fn list_movies(&self, params: &ListMoviesParams) -> AppResult<MoviePage> {
        let (query, page, limit) = list_query(params)?;
        let data: RawListData = self.fetch(&format!("list_movies.json?{query}")).await?;
        let total = data.movie_count.unwrap_or(0);
        Ok(MoviePage {
            movies: self.summaries(data.movies),
            total,
            page,
            limit,
            has_more: u64::from(page) * u64::from(limit) < total,
        })
    }

    pub async fn get_movie(&self, movie_id: u64) -> AppResult<MovieDetail> {
        let data: RawDetailsData = self.fetch(&details_path(movie_id)).await?;
        // An unknown id comes back as status "ok" with an empty movie whose id is 0.
        match data.movie {
            Some(movie) if movie.id.unwrap_or(0) != 0 => Ok(self.detail(movie)),
            _ => Err(AppError::NotFound(format!("movie {movie_id}"))),
        }
    }

    /// Internal data needed to stream one torrent of a movie (not part of the IPC).
    pub async fn torrent_ref(&self, movie_id: u64, infohash: &str) -> AppResult<TorrentRef> {
        let data: RawDetailsData = self.fetch(&details_path(movie_id)).await?;
        let movie = data
            .movie
            .filter(|m| m.id.unwrap_or(0) != 0)
            .ok_or_else(|| AppError::NotFound(format!("movie {movie_id}")))?;
        let raw = movie
            .torrents
            .iter()
            .flatten()
            .find(|t| {
                t.hash
                    .as_deref()
                    .is_some_and(|h| h.trim().eq_ignore_ascii_case(infohash))
            })
            .ok_or_else(|| AppError::NotFound(format!("torrent {infohash} of movie {movie_id}")))?;
        let torrent = convert_torrent(raw)
            .ok_or_else(|| AppError::NotFound(format!("torrent {infohash} is not usable")))?;
        Ok(TorrentRef {
            title: non_empty(&movie.title)
                .or_else(|| non_empty(&movie.title_english))
                .unwrap_or_default(),
            torrent_url: non_empty(&raw.url),
            video_codec: torrent.video_codec,
            seeds: torrent.seeds,
        })
    }

    pub async fn suggestions(&self, movie_id: u64) -> AppResult<Vec<MovieSummary>> {
        // `movie_count` is always 0 here: only `movies` matters.
        let data: RawListData = self
            .fetch(&format!("movie_suggestions.json?movie_id={movie_id}"))
            .await?;
        Ok(self.summaries(data.movies))
    }

    /// Probes every base URL in parallel (uncached) and reports latency.
    pub async fn api_status(&self) -> Vec<ApiEndpointStatus> {
        let active = self.active_index();
        let urls = self.urls();
        let probes = urls.iter().enumerate().map(|(idx, base)| async move {
            let started = Instant::now();
            let result = self
                .attempt::<RawListData>(base, "list_movies.json?limit=1")
                .await;
            let ok = result.is_ok();
            ApiEndpointStatus {
                base_url: base.clone(),
                role: if idx == active {
                    EndpointRole::Active
                } else {
                    EndpointRole::Fallback
                },
                latency_ms: ok.then(|| started.elapsed().as_millis() as u64),
                ok,
            }
        });
        futures_util::future::join_all(probes).await
    }

    pub fn active_base_url(&self) -> String {
        let urls = self.urls();
        urls.get(self.active_index())
            .or_else(|| urls.first())
            .cloned()
            .unwrap_or_default()
    }

    fn active_index(&self) -> usize {
        self.endpoints.lock().map(|s| s.active).unwrap_or(0)
    }

    /// Cached GET with failover. `path` is relative to the base URL, e.g. `list_movies.json?…`.
    /// Concurrent calls for the same path share a single upstream request.
    async fn fetch<T: DeserializeOwned>(&self, path: &str) -> AppResult<T> {
        let body = self
            .cache
            .try_get_with(path.to_owned(), self.fetch_uncached::<T>(path))
            .await
            .map_err(|e| AppError::from((*e).clone()))?;
        // Already validated against `T` before being cached.
        parse_body(&body).map_err(|_| AppError::Internal(format!("cached response for {path}")))
    }

    async fn fetch_uncached<T: DeserializeOwned>(&self, path: &str) -> Result<Bytes, FetchError> {
        let mut all_network = true;
        let mut last_error = String::new();
        let urls = self.urls();
        for idx in self.attempt_order(urls.len()) {
            let base = &urls[idx];
            let result = match self.attempt_raw(base, path).await {
                Ok(body) => parse_body::<T>(&body).map(|_| body),
                Err(e) => Err(e),
            };
            match result {
                Ok(body) => {
                    tracing::debug!(%base, %path, bytes = body.len(), "YTS response ok");
                    self.mark_success(&urls, idx);
                    return Ok(body);
                }
                Err(AttemptError::Network(msg)) => {
                    tracing::warn!(%base, %path, error = %msg, "YTS endpoint unreachable");
                    last_error = msg;
                }
                Err(AttemptError::Bad(msg)) => {
                    all_network = false;
                    tracing::warn!(%base, %path, error = %msg, "YTS endpoint failed");
                    last_error = msg;
                }
            }
        }
        Err(if all_network {
            FetchError::Network(last_error)
        } else {
            FetchError::Unavailable(last_error)
        })
    }

    async fn attempt<T: DeserializeOwned>(
        &self,
        base: &str,
        path: &str,
    ) -> Result<T, AttemptError> {
        let body = self.attempt_raw(base, path).await?;
        parse_body(&body)
    }

    async fn attempt_raw(&self, base: &str, path: &str) -> Result<Bytes, AttemptError> {
        let resp = self
            .http
            .get(format!("{base}{path}"))
            .send()
            .await
            .map_err(classify)?;
        let status = resp.status();
        if !status.is_success() {
            return Err(AttemptError::Bad(format!("HTTP {status}")));
        }
        resp.bytes().await.map_err(classify)
    }

    /// Active endpoint first, then the rest in preference order. If we failed over and the
    /// recheck interval elapsed, the primary goes first again.
    fn attempt_order(&self, len: usize) -> Vec<usize> {
        let first = match self.endpoints.lock() {
            Ok(mut state) => {
                if state.active != 0 && state.last_primary_check.elapsed() >= self.recheck_interval
                {
                    state.last_primary_check = Instant::now();
                    0
                } else {
                    state.active
                }
            }
            Err(_) => 0,
        };
        let first = if first < len { first } else { 0 };
        std::iter::once(first)
            .chain((0..len).filter(|&i| i != first))
            .collect()
    }

    fn mark_success(&self, urls: &Arc<Vec<String>>, idx: usize) {
        // The list may have been replaced while this request was in flight.
        if !Arc::ptr_eq(urls, &self.urls()) {
            return;
        }
        if let Ok(mut state) = self.endpoints.lock() {
            if state.active != idx {
                tracing::info!(base = %urls[idx], "switching active YTS endpoint");
                state.active = idx;
                state.last_primary_check = Instant::now();
            }
        }
    }

    fn summaries(&self, movies: Vec<RawMovie>) -> Vec<MovieSummary> {
        movies
            .into_iter()
            .filter(|m| m.id.unwrap_or(0) != 0)
            .map(|m| self.summary(&m, &convert_torrents(&m.torrents)))
            .collect()
    }

    fn summary(&self, m: &RawMovie, torrents: &[Torrent]) -> MovieSummary {
        let img = |u: &Option<String>| self.images.local_url(u.as_deref());
        let cover = img(&m.medium_cover_image)
            .or_else(|| img(&m.large_cover_image))
            .or_else(|| img(&m.small_cover_image));
        let cover_large = img(&m.large_cover_image).or_else(|| cover.clone());
        let mut qualities: Vec<Quality> = torrents.iter().map(|t| t.quality).collect();
        qualities.sort();
        qualities.dedup();
        MovieSummary {
            id: m.id.unwrap_or(0),
            imdb_code: m.imdb_code.clone().unwrap_or_default(),
            title: non_empty(&m.title)
                .or_else(|| non_empty(&m.title_english))
                .unwrap_or_default(),
            year: m.year.unwrap_or(0),
            rating: m.rating.unwrap_or(0.0),
            runtime_min: m.runtime.unwrap_or(0),
            genres: m.genres.clone().unwrap_or_default(),
            cover_url: cover,
            cover_large_url: cover_large,
            background_url: img(&m.background_image).or_else(|| img(&m.background_image_original)),
            qualities,
            has_x264: torrents.iter().any(|t| t.video_codec == VideoCodec::X264),
            max_seeds: torrents.iter().map(|t| t.seeds).max().unwrap_or(0),
        }
    }

    fn detail(&self, m: RawMovie) -> MovieDetail {
        let torrents = convert_torrents(&m.torrents);
        let summary_fields = self.summary(&m, &torrents);
        let screenshot_urls = [
            &m.large_screenshot_image1,
            &m.large_screenshot_image2,
            &m.large_screenshot_image3,
        ]
        .into_iter()
        .filter_map(|u| self.images.local_url(u.as_deref()))
        .collect();
        let cast = m
            .cast
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| {
                Some(CastMember {
                    name: non_empty(&c.name)?,
                    character: non_empty(&c.character_name),
                    image_url: self.images.local_url(c.url_small_image.as_deref()),
                })
            })
            .collect();
        let trailer_url = crate::stream::trailer_url_for(
            self.images.local_base(),
            m.yt_trailer_code.as_deref(),
            &summary_fields.title,
        );
        MovieDetail {
            summary_fields,
            summary: non_empty(&m.description_full)
                .or_else(|| non_empty(&m.summary))
                .or_else(|| non_empty(&m.description_intro))
                .unwrap_or_default(),
            language: m.language.unwrap_or_default(),
            mpa_rating: non_empty(&m.mpa_rating),
            trailer_url,
            yt_trailer_code: non_empty(&m.yt_trailer_code),
            screenshot_urls,
            cast,
            torrents,
            // Filled from the DB in phase 4.
            is_favorite: false,
            progress: None,
            download: None,
            offline: false,
        }
    }
}

fn normalize_base_urls(urls: &[String]) -> AppResult<Vec<String>> {
    if urls.is_empty() {
        return Err(AppError::InvalidInput("no YTS base URLs configured".into()));
    }
    Ok(urls
        .iter()
        .map(|u| format!("{}/", u.trim().trim_end_matches('/')))
        .collect())
}

fn details_path(movie_id: u64) -> String {
    format!("movie_details.json?movie_id={movie_id}&with_images=true&with_cast=true")
}

fn classify(e: reqwest::Error) -> AttemptError {
    if e.is_connect() || e.is_timeout() {
        AttemptError::Network(e.to_string())
    } else {
        AttemptError::Bad(e.to_string())
    }
}

fn parse_body<T: DeserializeOwned>(body: &[u8]) -> Result<T, AttemptError> {
    let envelope: RawEnvelope<T> = serde_json::from_slice(body)
        .map_err(|e| AttemptError::Bad(format!("invalid JSON: {e}")))?;
    let status = envelope.status.unwrap_or_default();
    if status != "ok" {
        let msg = envelope.status_message.unwrap_or_default();
        return Err(AttemptError::Bad(format!("status {status:?}: {msg}")));
    }
    envelope
        .data
        .ok_or_else(|| AttemptError::Bad("missing data".into()))
}

/// Builds the canonical (deterministic, cache-key friendly) query for `list_movies.json`.
fn list_query(p: &ListMoviesParams) -> AppResult<(String, u32, u32)> {
    let page = p.page.unwrap_or(1);
    let limit = p.limit.unwrap_or(DEFAULT_LIMIT);
    if page == 0 {
        return Err(AppError::InvalidInput("page must be >= 1".into()));
    }
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(AppError::InvalidInput(format!(
            "limit must be between 1 and {MAX_LIMIT}"
        )));
    }
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    q.append_pair("page", &page.to_string());
    q.append_pair("limit", &limit.to_string());
    if let Some(term) = p.query.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        q.append_pair("query_term", term);
    }
    if let Some(genre) = p.genre.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        q.append_pair("genre", &genre.to_ascii_lowercase());
    }
    if let Some(quality) = p.quality {
        q.append_pair("quality", quality_filter_str(quality));
    }
    if let Some(rating) = p.minimum_rating {
        if rating > 9 {
            return Err(AppError::InvalidInput(
                "minimumRating must be between 0 and 9".into(),
            ));
        }
        q.append_pair("minimum_rating", &rating.to_string());
    }
    if let Some(sort) = p.sort_by {
        q.append_pair("sort_by", sort_by_str(sort));
    }
    if let Some(order) = p.order_by {
        q.append_pair(
            "order_by",
            match order {
                OrderBy::Desc => "desc",
                OrderBy::Asc => "asc",
            },
        );
    }
    Ok((q.finish(), page, limit))
}

fn quality_filter_str(q: QualityFilter) -> &'static str {
    match q {
        QualityFilter::P480 => "480p",
        QualityFilter::P720 => "720p",
        QualityFilter::P1080 => "1080p",
        QualityFilter::P1080X265 => "1080p.x265",
        QualityFilter::P2160 => "2160p",
        QualityFilter::ThreeD => "3D",
    }
}

fn sort_by_str(s: SortBy) -> &'static str {
    match s {
        SortBy::Title => "title",
        SortBy::Year => "year",
        SortBy::Rating => "rating",
        SortBy::Peers => "peers",
        SortBy::Seeds => "seeds",
        SortBy::DownloadCount => "download_count",
        SortBy::LikeCount => "like_count",
        SortBy::DateAdded => "date_added",
    }
}

fn convert_torrents(raw: &Option<Vec<RawTorrent>>) -> Vec<Torrent> {
    raw.iter().flatten().filter_map(convert_torrent).collect()
}

fn convert_torrent(t: &RawTorrent) -> Option<Torrent> {
    let infohash = t.hash.as_deref()?.trim().to_ascii_lowercase();
    if infohash.len() != 40 || !infohash.bytes().all(|b| b.is_ascii_hexdigit()) {
        tracing::warn!(hash = ?t.hash, "skipping torrent with invalid hash");
        return None;
    }
    let quality = match t.quality.as_deref().map(str::trim) {
        Some("480p") => Quality::P480,
        Some("720p") => Quality::P720,
        Some("1080p") => Quality::P1080,
        Some("2160p") => Quality::P2160,
        Some(q) if q.eq_ignore_ascii_case("3d") => Quality::ThreeD,
        other => {
            tracing::warn!(quality = ?other, "skipping torrent with unknown quality");
            return None;
        }
    };
    let video_codec = match t.video_codec.as_deref().map(str::trim) {
        Some(c) if c.eq_ignore_ascii_case("x265") || c.eq_ignore_ascii_case("hevc") => {
            VideoCodec::X265
        }
        _ => VideoCodec::X264,
    };
    Some(Torrent {
        infohash,
        quality,
        // `movie_suggestions` omits `type`; TorrentSource maps unknown values to web.
        source: t.source.unwrap_or(TorrentSource::Web),
        video_codec,
        bit_depth: t.bit_depth,
        audio_channels: non_empty(&t.audio_channels),
        size_bytes: t.size_bytes.unwrap_or(0),
        seeds: t.seeds.unwrap_or(0),
        peers: t.peers.unwrap_or(0),
        uploaded_at: t.date_uploaded_unix.and_then(unix_to_iso8601),
    })
}

fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Formats a unix timestamp as ISO 8601 UTC (`2022-02-19T14:38:40Z`).
fn unix_to_iso8601(ts: i64) -> Option<String> {
    if ts <= 0 {
        return None;
    }
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    ))
}

// ---------------------------------------------------------------------------
// Raw API models (tolerant: every field optional, numbers may come as strings)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawEnvelope<T> {
    status: Option<String>,
    status_message: Option<String>,
    data: Option<T>,
}

#[derive(Deserialize)]
struct RawListData {
    #[serde(default, deserialize_with = "flex")]
    movie_count: Option<u64>,
    #[serde(default, deserialize_with = "null_as_default")]
    movies: Vec<RawMovie>,
}

#[derive(Deserialize)]
struct RawDetailsData {
    movie: Option<RawMovie>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawMovie {
    #[serde(deserialize_with = "flex")]
    id: Option<u64>,
    imdb_code: Option<String>,
    title: Option<String>,
    title_english: Option<String>,
    #[serde(deserialize_with = "flex")]
    year: Option<u32>,
    #[serde(deserialize_with = "flex")]
    rating: Option<f64>,
    #[serde(deserialize_with = "flex")]
    runtime: Option<u32>,
    genres: Option<Vec<String>>,
    summary: Option<String>,
    description_intro: Option<String>,
    description_full: Option<String>,
    yt_trailer_code: Option<String>,
    language: Option<String>,
    mpa_rating: Option<String>,
    background_image: Option<String>,
    background_image_original: Option<String>,
    small_cover_image: Option<String>,
    medium_cover_image: Option<String>,
    large_cover_image: Option<String>,
    large_screenshot_image1: Option<String>,
    large_screenshot_image2: Option<String>,
    large_screenshot_image3: Option<String>,
    cast: Option<Vec<RawCast>>,
    torrents: Option<Vec<RawTorrent>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawCast {
    name: Option<String>,
    character_name: Option<String>,
    url_small_image: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawTorrent {
    /// `.torrent` download URL (e.g. `https://yts.gg/torrent/download/<HASH>`).
    url: Option<String>,
    hash: Option<String>,
    quality: Option<String>,
    #[serde(rename = "type")]
    source: Option<TorrentSource>,
    video_codec: Option<String>,
    #[serde(deserialize_with = "flex")]
    bit_depth: Option<u8>,
    audio_channels: Option<String>,
    #[serde(deserialize_with = "flex")]
    seeds: Option<u32>,
    #[serde(deserialize_with = "flex")]
    peers: Option<u32>,
    #[serde(deserialize_with = "flex")]
    size_bytes: Option<u64>,
    #[serde(deserialize_with = "flex")]
    date_uploaded_unix: Option<i64>,
}

/// Accepts a number, a numeric string (`"8"`), an empty string or null.
fn flex<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
{
    Ok(
        match Option::<serde_json::Value>::deserialize(deserializer)? {
            Some(serde_json::Value::Number(n)) => n.to_string().parse().ok().or_else(|| {
                n.as_f64()
                    .and_then(|f| format!("{}", f.trunc()).parse().ok())
            }),
            Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
            _ => None,
        },
    )
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_from_unix() {
        assert_eq!(
            unix_to_iso8601(1_645_277_920).as_deref(),
            Some("2022-02-19T13:38:40Z")
        );
        assert_eq!(
            unix_to_iso8601(951_782_400).as_deref(),
            Some("2000-02-29T00:00:00Z")
        );
        assert_eq!(unix_to_iso8601(0), None);
    }

    #[test]
    fn flex_numbers() {
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "flex")]
            a: Option<u8>,
            #[serde(default, deserialize_with = "flex")]
            b: Option<f64>,
        }
        let t: T = serde_json::from_str(r#"{"a":"10","b":5}"#).unwrap();
        assert_eq!((t.a, t.b), (Some(10), Some(5.0)));
        let t: T = serde_json::from_str(r#"{"a":"","b":null}"#).unwrap();
        assert_eq!((t.a, t.b), (None, None));
        let t: T = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!((t.a, t.b), (None, None));
    }

    #[test]
    fn list_query_defaults_and_validation() {
        let (q, page, limit) = list_query(&ListMoviesParams::default()).unwrap();
        assert_eq!((q.as_str(), page, limit), ("page=1&limit=20", 1, 20));

        let (q, _, _) = list_query(&ListMoviesParams {
            query: Some("  the matrix ".into()),
            genre: Some("Sci-Fi".into()),
            quality: Some(QualityFilter::P1080X265),
            minimum_rating: Some(7),
            sort_by: Some(SortBy::DownloadCount),
            order_by: Some(OrderBy::Asc),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            q,
            "page=1&limit=20&query_term=the+matrix&genre=sci-fi&quality=1080p.x265\
             &minimum_rating=7&sort_by=download_count&order_by=asc"
        );

        for bad in [
            ListMoviesParams {
                page: Some(0),
                ..Default::default()
            },
            ListMoviesParams {
                limit: Some(51),
                ..Default::default()
            },
            ListMoviesParams {
                minimum_rating: Some(10),
                ..Default::default()
            },
        ] {
            assert!(matches!(list_query(&bad), Err(AppError::InvalidInput(_))));
        }
    }

    #[test]
    fn torrent_conversion_is_tolerant() {
        let raw: RawTorrent = serde_json::from_str(
            r#"{"hash":"C8B8584842A104F6CC90A55C545F98B04C165186","quality":"2160p",
                "is_repack":"","video_codec":"x265","bit_depth":"10","audio_channels":"5.1",
                "seeds":100,"peers":23,"size_bytes":7086696038,"date_uploaded_unix":1640398525}"#,
        )
        .unwrap();
        let t = convert_torrent(&raw).unwrap();
        assert_eq!(t.infohash, "c8b8584842a104f6cc90a55c545f98b04c165186");
        assert_eq!(t.quality, Quality::P2160);
        assert_eq!(t.source, TorrentSource::Web);
        assert_eq!(t.video_codec, VideoCodec::X265);
        assert_eq!(t.bit_depth, Some(10));

        let bad_hash: RawTorrent =
            serde_json::from_str(r#"{"hash":"xyz","quality":"720p"}"#).unwrap();
        assert!(convert_torrent(&bad_hash).is_none());
        let bad_quality: RawTorrent = serde_json::from_str(
            r#"{"hash":"C8B8584842A104F6CC90A55C545F98B04C165186","quality":"8K"}"#,
        )
        .unwrap();
        assert!(convert_torrent(&bad_quality).is_none());
    }
}
