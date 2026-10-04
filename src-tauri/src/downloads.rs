//! Downloads ("Descargas"): movies saved in `library/<Title (year) [quality]>/` to watch
//! without network.
//!
//! The DB keeps everything a download needs to come back after a restart offline: the
//! movie, the `.torrent` bytes, the video path and the last known state. A copy of the
//! `MovieDetail` is saved too, so the movie page works without network (`offline: true`).
//!
//! Stored states are `active`, `paused`, `done` and `error`; `queued` (still resolving
//! the torrent) and `stalled` (no peers or no speed for a while) are derived live. A
//! background tick (1 s) follows progress, moves streams that became downloads out of
//! `cache/` ([`TorrentEngine::promote`]), marks finished ones `done` and emits
//! `download://changed` whenever a state changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use tokio::sync::broadcast;

use crate::db::{Db, DownloadRow};
use crate::error::{AppError, AppResult};
use crate::images::ImageStore;
use crate::torrent::{
    is_valid_infohash, remove_path, DownloadRequest, LocalStream, StreamRequest, TorrentEngine,
};
use crate::types::{
    Download, DownloadChanged, DownloadState, MovieDetail, MovieSummary, Quality, VideoCodec,
};

pub const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Free space kept on top of what is left to download.
pub const SPACE_MARGIN_BYTES: u64 = 64 * 1024 * 1024;

/// Longest folder name we create (bytes); ext4 allows 255.
const MAX_FOLDER_LEN: usize = 180;

/// How often (in ticks) the progress of active downloads is written to the DB.
const PERSIST_EVERY_TICKS: u64 = 15;

const ACTIVE: &str = "active";
const PAUSED: &str = "paused";
const DONE: &str = "done";
const ERROR: &str = "error";

/// Free bytes on the filesystem that holds a path (swappable in tests).
pub type FreeSpaceFn = Arc<dyn Fn(&Path) -> u64 + Send + Sync>;

/// The YTS torrent to download (from the catalog).
#[derive(Debug, Clone)]
pub struct TorrentInfo {
    pub infohash: String,
    pub quality: Quality,
    pub video_codec: VideoCodec,
    /// Whole torrent, from YTS: used for the space check until the video size is known.
    pub size_bytes: u64,
    pub title: String,
    pub torrent_url: Option<String>,
    pub seeds: u32,
}

#[derive(Clone)]
pub struct DownloadsConfig {
    pub db: Db,
    pub images: Arc<ImageStore>,
    pub engine: Arc<TorrentEngine>,
    pub library_dir: PathBuf,
    pub seed_after_download: bool,
    pub free_space: FreeSpaceFn,
}

struct Record {
    row: DownloadRow,
    /// Adding the torrent (metadata, piece check): shown as `queued`.
    resolving: bool,
    /// Still in `cache/` (it was being streamed): waiting to be moved to `library/`.
    in_cache: bool,
}

pub struct DownloadManager {
    cfg: DownloadsConfig,
    seed: AtomicBool,
    records: Mutex<HashMap<String, Record>>,
    /// Last state emitted per download, to emit only changes.
    emitted: Mutex<HashMap<String, DownloadState>>,
    events: broadcast::Sender<DownloadChanged>,
    /// Serializes the commands (start/pause/resume/remove) and the tick.
    ops: tokio::sync::Mutex<()>,
}

fn not_found(infohash: &str) -> AppError {
    AppError::NotFound(format!("no download {infohash}"))
}

/// Folder name `Title (year) [quality]`, without characters that are invalid on common
/// filesystems (`/ \ : * ? " < > |`, control characters) and without trailing dots.
pub fn folder_name(title: &str, year: u32, quality: Quality) -> String {
    let clean: String = title
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut title = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let title_trimmed = title.trim_matches(|c: char| c == '.' || c == ' ');
    title = if title_trimmed.is_empty() {
        "Movie".to_owned()
    } else {
        title_trimmed.to_owned()
    };
    let suffix = if year > 0 {
        format!(" ({year}) [{}]", quality.as_str())
    } else {
        format!(" [{}]", quality.as_str())
    };
    let max_title = MAX_FOLDER_LEN.saturating_sub(suffix.len());
    if title.len() > max_title {
        let mut cut = max_title;
        while !title.is_char_boundary(cut) {
            cut -= 1;
        }
        title.truncate(cut);
        title = title.trim_end().to_owned();
    }
    format!("{title}{suffix}")
}

/// `name`, or `name (2)`, `name (3)`… if another download already uses it.
fn unique_folder(library: &Path, name: &str, taken: &[PathBuf]) -> PathBuf {
    let mut n = 1;
    loop {
        let candidate = if n == 1 {
            library.join(name)
        } else {
            library.join(format!("{name} ({n})"))
        };
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Fails with `io` if `free` can't hold `needed` (plus a margin).
pub fn check_space(free: u64, needed: u64) -> AppResult<()> {
    if free >= needed.saturating_add(SPACE_MARGIN_BYTES) {
        return Ok(());
    }
    Err(AppError::Io(std::io::Error::new(
        std::io::ErrorKind::StorageFull,
        format!("not enough free space: {needed} bytes needed, {free} available"),
    )))
}

/// ISO 8601 UTC with milliseconds, like the DB (`2026-10-04T12:00:00.123Z`).
pub fn iso8601(t: SystemTime) -> String {
    let d = t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs();
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        d.subsec_millis()
    )
}

fn quality_from(s: &str) -> Quality {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).unwrap_or(Quality::P1080)
}

fn codec_from(s: &str) -> VideoCodec {
    if s.eq_ignore_ascii_case("x265") {
        VideoCodec::X265
    } else {
        VideoCodec::X264
    }
}

fn codec_str(c: VideoCodec) -> &'static str {
    match c {
        VideoCodec::X264 => "x264",
        VideoCodec::X265 => "x265",
    }
}

impl DownloadManager {
    pub fn new(cfg: DownloadsConfig) -> Arc<Self> {
        let (events, _) = broadcast::channel(64);
        Arc::new(Self {
            seed: AtomicBool::new(cfg.seed_after_download),
            cfg,
            records: Mutex::new(HashMap::new()),
            emitted: Mutex::new(HashMap::new()),
            events,
            ops: tokio::sync::Mutex::new(()),
        })
    }

    /// `download://changed` payloads.
    pub fn subscribe(&self) -> broadcast::Receiver<DownloadChanged> {
        self.events.subscribe()
    }

    fn local_base(&self) -> &str {
        self.cfg.images.local_base()
    }

    fn with_record<T>(&self, infohash: &str, f: impl FnOnce(&mut Record) -> T) -> Option<T> {
        self.records.lock().ok()?.get_mut(infohash).map(f)
    }

    fn row(&self, infohash: &str) -> Option<DownloadRow> {
        self.with_record(infohash, |r| r.row.clone())
    }

    fn infohashes(&self) -> Vec<String> {
        self.records
            .lock()
            .map(|r| r.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn video_path(row: &DownloadRow) -> Option<PathBuf> {
        Some(Path::new(&row.path).join(row.rel_path.as_deref()?))
    }

    // -- Startup ---------------------------------------------------------------------------

    /// Loads the downloads from the DB and fixes the files, before the cache cleanup runs:
    /// a finished download whose video is gone → `error`; a download that was still in
    /// `cache/` when the app closed is moved to its library folder now (nothing is in the
    /// torrent session yet). Torrents are added afterwards by [`Self::resume_all`].
    pub async fn restore(&self) -> AppResult<()> {
        let rows = self.cfg.db.list_downloads(self.local_base()).await?;
        for mut row in rows {
            if row.state == DONE {
                let exists = match Self::video_path(&row) {
                    Some(p) => tokio::fs::metadata(&p).await.is_ok(),
                    None => false,
                };
                if !exists {
                    tracing::warn!(infohash = %row.infohash, path = %row.path, "downloaded file is missing");
                    row.state = ERROR.into();
                    row.error = Some("downloaded file is missing".into());
                    self.persist(&row).await;
                }
            } else if row.state != ERROR {
                if let Some(rel) = row.rel_path.clone() {
                    match self
                        .cfg
                        .engine
                        .move_from_cache(&row.infohash, Path::new(&rel), Path::new(&row.path))
                        .await
                    {
                        Ok(true) => {
                            tracing::info!(infohash = %row.infohash, "moved download from cache to library")
                        }
                        Ok(false) => {}
                        Err(e) => {
                            tracing::warn!(infohash = %row.infohash, error = %e, "could not move download from cache")
                        }
                    }
                }
            }
            let state = self.state_of_row(&row);
            if let Ok(mut emitted) = self.emitted.lock() {
                emitted.insert(row.infohash.clone(), state);
            }
            if let Ok(mut records) = self.records.lock() {
                records.insert(
                    row.infohash.clone(),
                    Record {
                        row,
                        resolving: false,
                        in_cache: false,
                    },
                );
            }
        }
        tracing::info!(count = self.infohashes().len(), "downloads restored");
        Ok(())
    }

    /// Puts the restored torrents back in the session: active and paused downloads (from
    /// the saved `.torrent`, no network needed to start) and finished ones if seeding.
    pub fn resume_all(self: &Arc<Self>) {
        let seed = self.seed.load(Ordering::Relaxed);
        for infohash in self.infohashes() {
            let Some(row) = self.row(&infohash) else {
                continue;
            };
            let wanted = match row.state.as_str() {
                ACTIVE | PAUSED => true,
                DONE => seed,
                _ => false,
            };
            if wanted {
                self.spawn_resolve(&infohash);
            }
        }
    }

    fn state_of_row(&self, row: &DownloadRow) -> DownloadState {
        match row.state.as_str() {
            DONE => DownloadState::Done,
            PAUSED => DownloadState::Paused,
            ERROR => DownloadState::Error,
            _ => DownloadState::Active,
        }
    }

    /// Spawns the periodic tick while the manager is alive.
    pub fn spawn_tick(self: &Arc<Self>, interval: Duration) {
        let weak: Weak<Self> = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut n: u64 = 0;
            loop {
                ticker.tick().await;
                let Some(manager) = weak.upgrade() else {
                    break;
                };
                n += 1;
                manager.tick(n.is_multiple_of(PERSIST_EVERY_TICKS)).await;
            }
        });
    }

    // -- Views -----------------------------------------------------------------------------

    fn view(&self, rec: &Record) -> Download {
        let row = &rec.row;
        let stored = self.state_of_row(row);
        let stats = match stored {
            DownloadState::Done | DownloadState::Error => None,
            _ => self.cfg.engine.download_stats(&row.infohash),
        };
        let resolved = stats.as_ref().is_some_and(|s| s.resolved);
        let state = match stored {
            DownloadState::Active if rec.resolving || !resolved => DownloadState::Queued,
            DownloadState::Active if stats.as_ref().is_some_and(|s| s.stalled) => {
                DownloadState::Stalled
            }
            other => other,
        };
        let size = stats
            .as_ref()
            .filter(|s| s.resolved && s.file_len > 0)
            .map(|s| s.file_len)
            .unwrap_or(row.size_bytes);
        let downloaded = if stored == DownloadState::Done {
            size
        } else {
            let live = stats.as_ref().map(|s| s.downloaded).unwrap_or(0);
            live.max(row.downloaded_bytes).min(size)
        };
        let running = matches!(state, DownloadState::Active | DownloadState::Stalled);
        let (speed, peers) = match (&stats, running) {
            (Some(s), true) => (s.down_bps, s.peers),
            _ => (0, 0),
        };
        let eta_s = (running && speed > 0).then(|| size.saturating_sub(downloaded) / speed);
        let path = Path::new(&row.path);
        Download {
            infohash: row.infohash.clone(),
            movie: row.movie.clone(),
            quality: quality_from(&row.quality),
            video_codec: codec_from(&row.video_codec),
            state,
            progress: if size == 0 {
                0.0
            } else {
                downloaded as f64 / size as f64
            },
            size_bytes: size,
            downloaded_bytes: downloaded,
            down_speed_bps: speed,
            peers,
            eta_s,
            path: path.is_dir().then(|| row.path.clone()),
            error: row.error.clone(),
            added_at: row.added_at.clone(),
        }
    }

    pub fn get(&self, infohash: &str) -> Option<Download> {
        let records = self.records.lock().ok()?;
        records
            .get(&infohash.to_ascii_lowercase())
            .map(|r| self.view(r))
    }

    /// Most recent first.
    pub fn list(&self) -> Vec<Download> {
        let Ok(records) = self.records.lock() else {
            return Vec::new();
        };
        let mut list: Vec<Download> = records.values().map(|r| self.view(r)).collect();
        list.sort_by(|a, b| b.added_at.cmp(&a.added_at));
        list
    }

    /// The movie's download (the most recent if there are several).
    pub fn for_movie(&self, movie_id: u64) -> Option<Download> {
        self.list().into_iter().find(|d| d.movie.id == movie_id)
    }

    /// A finished download plays from its file (`source: "library"`).
    pub fn library_stream(&self, infohash: &str) -> Option<LocalStream> {
        let row = self.row(&infohash.to_ascii_lowercase())?;
        if row.state != DONE {
            return None;
        }
        Some(LocalStream {
            infohash: row.infohash.clone(),
            movie_id: row.movie_id,
            path: Self::video_path(&row)?,
            video_codec: codec_from(&row.video_codec),
        })
    }

    /// What `start_stream` needs for a download in progress, without asking YTS (works
    /// offline: the torrent is already in the engine or its bytes are saved).
    pub fn stream_request(&self, infohash: &str) -> Option<StreamRequest> {
        let row = self.row(&infohash.to_ascii_lowercase())?;
        (row.state == ACTIVE || row.state == PAUSED).then(|| StreamRequest {
            movie_id: row.movie_id,
            infohash: row.infohash.clone(),
            title: row.title.clone(),
            torrent_url: row.torrent_url.clone(),
            video_codec: codec_from(&row.video_codec),
            seeds: 0,
        })
    }

    pub fn folder(&self, infohash: &str) -> AppResult<PathBuf> {
        let infohash = infohash.to_ascii_lowercase();
        let row = self.row(&infohash).ok_or_else(|| not_found(&infohash))?;
        let path = PathBuf::from(&row.path);
        if !path.is_dir() {
            return Err(AppError::NotFound(format!(
                "folder of {infohash} does not exist yet"
            )));
        }
        Ok(path)
    }

    // -- Events ----------------------------------------------------------------------------

    /// Emits `download://changed` if the state differs from the last one emitted.
    fn emit_if_changed(&self, infohash: &str) {
        let Some(d) = self.get(infohash) else {
            return;
        };
        let changed = self
            .emitted
            .lock()
            .map(|mut m| m.insert(infohash.to_owned(), d.state) != Some(d.state))
            .unwrap_or(true);
        if changed {
            tracing::info!(%infohash, state = ?d.state, "download state changed");
            let _ = self.events.send(DownloadChanged {
                infohash: infohash.to_owned(),
                download: Some(d),
            });
        }
    }

    fn emit_removed(&self, infohash: &str) {
        if let Ok(mut m) = self.emitted.lock() {
            m.remove(infohash);
        }
        let _ = self.events.send(DownloadChanged {
            infohash: infohash.to_owned(),
            download: None,
        });
    }

    async fn persist(&self, row: &DownloadRow) {
        if let Err(e) = self
            .cfg
            .db
            .update_download(
                &row.infohash,
                &row.state,
                row.error.clone(),
                row.size_bytes,
                row.downloaded_bytes,
            )
            .await
        {
            tracing::warn!(infohash = %row.infohash, error = %e, "could not save download");
        }
    }

    /// Changes the stored state (and live numbers) of a record and saves it.
    async fn set_state(&self, infohash: &str, state: &str, error: Option<String>) {
        let view = self.get(infohash);
        let row = self.with_record(infohash, |r| {
            r.row.state = state.to_owned();
            r.row.error = error;
            if let Some(v) = &view {
                r.row.size_bytes = v.size_bytes;
                r.row.downloaded_bytes = if state == DONE {
                    v.size_bytes
                } else {
                    v.downloaded_bytes
                };
            }
            r.row.clone()
        });
        if let Some(row) = row {
            self.persist(&row).await;
        }
    }

    // -- Commands --------------------------------------------------------------------------

    /// Creates a download (or returns the existing one). Checks the free space against
    /// what is left to download, saves the movie detail for offline use and adds the
    /// torrent in the background (`queued` until it is ready). A torrent being streamed is
    /// promoted: what is already downloaded is kept.
    pub async fn start(
        self: &Arc<Self>,
        movie: MovieSummary,
        detail: &MovieDetail,
        torrent: TorrentInfo,
    ) -> AppResult<Download> {
        let infohash = torrent.infohash.trim().to_ascii_lowercase();
        if !is_valid_infohash(&infohash) {
            return Err(AppError::InvalidInput(format!(
                "invalid infohash {infohash:?}"
            )));
        }
        let _ops = self.ops.lock().await;
        if let Some(d) = self.get(&infohash) {
            return Ok(d);
        }

        // Space: what is left of the video if it is being streamed, else the torrent size.
        let needed = match self.cfg.engine.download_stats(&infohash) {
            Some(s) if s.resolved && s.file_len > 0 => s.file_len.saturating_sub(s.downloaded),
            _ => torrent.size_bytes,
        };
        tokio::fs::create_dir_all(&self.cfg.library_dir).await?;
        let free = (self.cfg.free_space)(&self.cfg.library_dir);
        check_space(free, needed)?;

        let taken: Vec<PathBuf> = self
            .records
            .lock()
            .map(|r| r.values().map(|r| PathBuf::from(&r.row.path)).collect())
            .unwrap_or_default();
        let folder = unique_folder(
            &self.cfg.library_dir,
            &folder_name(&movie.title, movie.year, torrent.quality),
            &taken,
        );

        self.cfg
            .db
            .put_movie_detail(&self.cfg.images, detail)
            .await?;
        let row = DownloadRow {
            infohash: infohash.clone(),
            movie_id: movie.id,
            movie,
            quality: torrent.quality.as_str().to_owned(),
            video_codec: codec_str(torrent.video_codec).to_owned(),
            state: ACTIVE.into(),
            size_bytes: torrent.size_bytes,
            path: folder.to_string_lossy().into_owned(),
            error: None,
            added_at: iso8601(SystemTime::now()),
            title: torrent.title,
            torrent_url: torrent.torrent_url,
            torrent: None,
            rel_path: None,
            downloaded_bytes: 0,
        };
        self.cfg.db.put_download(&self.cfg.images, &row).await?;
        if let Ok(mut records) = self.records.lock() {
            records.insert(
                infohash.clone(),
                Record {
                    row,
                    resolving: true,
                    in_cache: false,
                },
            );
        }
        tracing::info!(%infohash, folder = %folder.display(), "download created");
        self.emit_if_changed(&infohash);
        self.spawn_resolve(&infohash);
        self.get(&infohash).ok_or_else(|| not_found(&infohash))
    }

    /// Adds the torrent of a download to the engine in the background.
    fn spawn_resolve(self: &Arc<Self>, infohash: &str) {
        let manager = Arc::clone(self);
        let infohash = infohash.to_owned();
        self.with_record(&infohash, |r| r.resolving = true);
        tokio::spawn(async move {
            manager.resolve(&infohash).await;
        });
    }

    async fn resolve(&self, infohash: &str) {
        let Some(row) = self.row(infohash) else {
            return;
        };
        let seed = self.seed.load(Ordering::Relaxed);
        let run = row.state == ACTIVE || (row.state == DONE && seed);
        let req = DownloadRequest {
            stream: StreamRequest {
                movie_id: row.movie_id,
                infohash: row.infohash.clone(),
                title: row.title.clone(),
                torrent_url: row.torrent_url.clone(),
                video_codec: codec_from(&row.video_codec),
                seeds: 0,
            },
            folder: PathBuf::from(&row.path),
            torrent_bytes: row.torrent.clone().map(Bytes::from),
            run,
        };
        let result = self.cfg.engine.add_download(req).await;
        let _ops = self.ops.lock().await;
        let Some(row) = self.row(infohash) else {
            // Removed while resolving.
            if let Err(e) = self.cfg.engine.remove_torrent(infohash).await {
                tracing::warn!(%infohash, error = %e, "could not drop removed download");
            }
            return;
        };
        match result {
            Ok(t) => {
                let rel = t.rel_path.to_string_lossy().into_owned();
                if row.torrent.is_none() || row.rel_path.as_deref() != Some(rel.as_str()) {
                    if let Err(e) = self
                        .cfg
                        .db
                        .set_download_torrent(
                            infohash,
                            t.torrent_bytes.to_vec(),
                            rel.clone(),
                            t.file_len,
                        )
                        .await
                    {
                        tracing::warn!(%infohash, error = %e, "could not save the torrent");
                    }
                }
                self.with_record(infohash, |r| {
                    r.resolving = false;
                    r.in_cache = t.in_cache;
                    r.row.torrent = Some(t.torrent_bytes.to_vec());
                    r.row.rel_path = Some(rel);
                    r.row.size_bytes = t.file_len;
                });
                // Paused (or seeding turned off) while resolving.
                let run =
                    row.state == ACTIVE || (row.state == DONE && self.seed.load(Ordering::Relaxed));
                if let Err(e) = self.cfg.engine.set_download_running(infohash, run).await {
                    tracing::warn!(%infohash, error = %e, "could not apply download state");
                }
            }
            Err(e) => {
                tracing::warn!(%infohash, error = %e, "could not add download");
                self.with_record(infohash, |r| r.resolving = false);
                if row.state != DONE {
                    self.set_state(infohash, ERROR, Some(e.to_string())).await;
                }
            }
        }
        self.emit_if_changed(infohash);
    }

    pub async fn pause(&self, infohash: &str) -> AppResult<Download> {
        let infohash = infohash.to_ascii_lowercase();
        let _ops = self.ops.lock().await;
        let row = self.row(&infohash).ok_or_else(|| not_found(&infohash))?;
        if row.state == ACTIVE {
            self.set_state(&infohash, PAUSED, None).await;
            self.cfg
                .engine
                .set_download_running(&infohash, false)
                .await?;
            self.emit_if_changed(&infohash);
        }
        self.get(&infohash).ok_or_else(|| not_found(&infohash))
    }

    /// Resumes a paused download; on one in `error`, tries again from what is on disk.
    pub async fn resume(self: &Arc<Self>, infohash: &str) -> AppResult<Download> {
        let infohash = infohash.to_ascii_lowercase();
        let _ops = self.ops.lock().await;
        let row = self.row(&infohash).ok_or_else(|| not_found(&infohash))?;
        match row.state.as_str() {
            PAUSED => {
                self.set_state(&infohash, ACTIVE, None).await;
                if self.cfg.engine.has_torrent(&infohash) {
                    self.cfg
                        .engine
                        .set_download_running(&infohash, true)
                        .await?;
                } else if !self
                    .with_record(&infohash, |r| r.resolving)
                    .unwrap_or(false)
                {
                    self.spawn_resolve(&infohash);
                }
                self.emit_if_changed(&infohash);
            }
            ERROR => {
                if let Some(rel) = &row.rel_path {
                    if !self.cfg.engine.has_torrent(&infohash) {
                        self.cfg
                            .engine
                            .move_from_cache(&infohash, Path::new(rel), Path::new(&row.path))
                            .await?;
                    }
                }
                self.set_state(&infohash, ACTIVE, None).await;
                self.spawn_resolve(&infohash);
                self.emit_if_changed(&infohash);
            }
            _ => {}
        }
        self.get(&infohash).ok_or_else(|| not_found(&infohash))
    }

    /// Removes a download. The torrent leaves the session **before** any file is deleted
    /// (otherwise librqbit keeps the file open and the space is not freed). Without
    /// `delete_files`, the files stay where they are.
    pub async fn remove(&self, infohash: &str, delete_files: bool) -> AppResult<()> {
        let infohash = infohash.to_ascii_lowercase();
        let _ops = self.ops.lock().await;
        let in_cache = self
            .with_record(&infohash, |r| r.in_cache)
            .ok_or_else(|| not_found(&infohash))?;
        let row = self.row(&infohash).ok_or_else(|| not_found(&infohash))?;
        if in_cache && !delete_files {
            // Still a stream in `cache/`: back to a plain cached stream.
            self.cfg.engine.unmark_download(&infohash).await?;
        } else {
            self.cfg.engine.remove_torrent(&infohash).await?;
        }
        if delete_files {
            remove_path(Path::new(&row.path)).await?;
            if in_cache {
                remove_path(&self.cfg.engine.torrent_dir(&infohash)).await?;
            }
        }
        self.cfg.db.remove_download(&infohash).await?;
        if let Ok(mut records) = self.records.lock() {
            records.remove(&infohash);
        }
        tracing::info!(%infohash, delete_files, "download removed");
        self.emit_removed(&infohash);
        Ok(())
    }

    /// `seedAfterDownload`, applied now to the finished downloads.
    pub async fn set_seed_after_download(self: &Arc<Self>, seed: bool) {
        if self.seed.swap(seed, Ordering::Relaxed) == seed {
            return;
        }
        let _ops = self.ops.lock().await;
        for infohash in self.infohashes() {
            if self.row(&infohash).is_none_or(|r| r.state != DONE) {
                continue;
            }
            if self.cfg.engine.has_torrent(&infohash) {
                if let Err(e) = self.cfg.engine.set_download_running(&infohash, seed).await {
                    tracing::warn!(%infohash, error = %e, "could not change seeding");
                }
            } else if seed {
                self.spawn_resolve(&infohash);
            }
        }
    }

    // -- Background ------------------------------------------------------------------------

    /// Moves a promoted stream to `library/` (shown as `queued` while its pieces are
    /// checked again in the new place).
    fn spawn_promote(self: &Arc<Self>, infohash: &str, folder: PathBuf) {
        self.with_record(infohash, |r| r.resolving = true);
        let manager = Arc::clone(self);
        let infohash = infohash.to_owned();
        tokio::spawn(async move {
            let result = manager.cfg.engine.promote(&infohash, &folder).await;
            let _ops = manager.ops.lock().await;
            match result {
                Ok(moved) => {
                    manager.with_record(&infohash, |r| {
                        r.resolving = false;
                        r.in_cache = !moved;
                    });
                }
                Err(e) => {
                    tracing::warn!(%infohash, error = %e, "could not move download to library");
                    manager.with_record(&infohash, |r| {
                        r.resolving = false;
                        r.in_cache = false;
                    });
                    manager
                        .set_state(&infohash, ERROR, Some(e.to_string()))
                        .await;
                }
            }
            manager.emit_if_changed(&infohash);
        });
    }

    /// One pass: promotions, completions, errors, state change events and (with
    /// `persist`) progress saved to the DB.
    pub async fn tick(self: &Arc<Self>, persist: bool) {
        let _ops = self.ops.lock().await;
        for infohash in self.infohashes() {
            let Some((row, resolving, in_cache)) =
                self.with_record(&infohash, |r| (r.row.clone(), r.resolving, r.in_cache))
            else {
                continue;
            };
            if row.state != ACTIVE && row.state != PAUSED {
                self.emit_if_changed(&infohash);
                continue;
            }
            if in_cache && !resolving && self.cfg.engine.can_promote(&infohash) {
                self.spawn_promote(&infohash, PathBuf::from(&row.path));
                continue;
            }
            let stats = self.cfg.engine.download_stats(&infohash);
            match stats {
                Some(s) if s.error.is_some() && !resolving => {
                    self.set_state(&infohash, ERROR, s.error).await;
                }
                Some(s)
                    if s.resolved
                        && !resolving
                        && !in_cache
                        && s.file_len > 0
                        && s.downloaded >= s.file_len =>
                {
                    self.set_state(&infohash, DONE, None).await;
                    let seed = self.seed.load(Ordering::Relaxed);
                    if let Err(e) = self.cfg.engine.set_download_running(&infohash, seed).await {
                        tracing::warn!(%infohash, error = %e, "could not stop the finished torrent");
                    }
                    tracing::info!(%infohash, seed, "download finished");
                }
                Some(s) if persist && s.resolved && s.downloaded > row.downloaded_bytes => {
                    let state = row.state.clone();
                    self.set_state(&infohash, &state, None).await;
                }
                _ => {}
            }
            self.emit_if_changed(&infohash);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_are_readable_and_safe() {
        assert_eq!(
            folder_name("The Matrix", 1999, Quality::P1080),
            "The Matrix (1999) [1080p]"
        );
        assert_eq!(
            folder_name("Face/Off: Director's Cut?", 1997, Quality::P720),
            "Face Off Director's Cut (1997) [720p]"
        );
        assert_eq!(
            folder_name("  ...\u{0}  ", 2020, Quality::ThreeD),
            "Movie (2020) [3D]"
        );
        assert_eq!(folder_name("Se7en.", 0, Quality::P2160), "Se7en [2160p]");
        let long = folder_name(&"ñ".repeat(300), 2001, Quality::P480);
        assert!(long.len() <= MAX_FOLDER_LEN, "{}", long.len());
        assert!(long.ends_with(" (2001) [480p]"));
    }

    #[test]
    fn folders_get_a_suffix_when_taken() {
        let lib = Path::new("/lib");
        assert_eq!(unique_folder(lib, "A [1080p]", &[]), lib.join("A [1080p]"));
        let taken = vec![lib.join("A [1080p]"), lib.join("A [1080p] (2)")];
        assert_eq!(
            unique_folder(lib, "A [1080p]", &taken),
            lib.join("A [1080p] (3)")
        );
    }

    #[test]
    fn space_check_includes_a_margin() {
        assert!(check_space(10 * SPACE_MARGIN_BYTES, SPACE_MARGIN_BYTES).is_ok());
        let err = check_space(SPACE_MARGIN_BYTES, 1).unwrap_err();
        assert_eq!(serde_json::to_value(&err).unwrap()["code"], "io");
        assert!(err.to_string().contains("not enough free space"));
        assert!(check_space(u64::MAX - 1, u64::MAX).is_err());
    }

    #[test]
    fn iso8601_matches_known_dates() {
        let at = |s: u64, ms: u64| SystemTime::UNIX_EPOCH + Duration::from_millis(s * 1000 + ms);
        assert_eq!(iso8601(at(0, 0)), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(at(951_782_400, 5)), "2000-02-29T00:00:00.005Z");
        assert_eq!(iso8601(at(1_791_115_200, 123)), "2026-10-04T12:00:00.123Z");
    }
}
