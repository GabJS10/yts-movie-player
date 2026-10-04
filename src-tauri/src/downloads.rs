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
//!
//! Each download keeps its own folder, so changing `downloadsDir` only affects new ones;
//! [`DownloadManager::start_move`] moves the existing ones (rename on the same disk, copy
//! and delete across disks) with `downloads://move-progress`. A download whose folder's
//! parent is gone (unmounted disk) is `unavailable`: its torrent leaves the session and
//! comes back by itself when the folder returns.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use tokio::sync::broadcast;

use crate::cache::{allocated_size, free_disk_bytes};
use crate::db::{Db, DownloadRow};
use crate::error::{AppError, AppResult};
use crate::images::ImageStore;
use crate::settings::dir_available;
use crate::torrent::{
    is_valid_infohash, remove_path, DownloadRequest, LocalStream, StreamRequest, TorrentEngine,
};
use crate::types::{
    BackgroundError, Download, DownloadChanged, DownloadState, MoveFailure, MoveProgress,
    MovieDetail, MovieSummary, Quality, VideoCodec,
};

pub const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Free space kept on top of what is left to download.
pub const SPACE_MARGIN_BYTES: u64 = 64 * 1024 * 1024;

/// Longest folder name we create (bytes); ext4 allows 255.
const MAX_FOLDER_LEN: usize = 180;

/// Chunk used when copying across disks (cancellation is checked between chunks).
const COPY_CHUNK: usize = 1024 * 1024;

/// `downloads://move-progress` at most this often while copying.
const MOVE_PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// A stalled download without peers is restarted (re-announce, known peers again) at most
/// this often: librqbit does not reconnect to a lost peer by itself.
const KICK_EVERY: Duration = Duration::from_secs(60);

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
    /// `downloadsDir`: where new downloads go.
    pub library_dir: PathBuf,
    pub seed_after_download: bool,
    pub free_space: FreeSpaceFn,
    /// Tests: always copy + delete when moving, as if the folders were on two disks.
    pub always_copy: bool,
    /// Tests: pause after each copied chunk, to cancel half way.
    pub copy_chunk_delay: Option<Duration>,
}

impl DownloadsConfig {
    pub fn new(
        db: Db,
        images: Arc<ImageStore>,
        engine: Arc<TorrentEngine>,
        library_dir: PathBuf,
        seed_after_download: bool,
    ) -> Self {
        Self {
            db,
            images,
            engine,
            library_dir,
            seed_after_download,
            free_space: Arc::new(free_disk_bytes),
            always_copy: false,
            copy_chunk_delay: None,
        }
    }
}

struct Record {
    row: DownloadRow,
    /// Adding the torrent (metadata, piece check): shown as `queued`.
    resolving: bool,
    /// Still in `cache/` (it was being streamed): waiting to be moved to `library/`.
    in_cache: bool,
    /// Its folder's parent is gone (unmounted disk).
    unavailable: bool,
    /// Being moved by `move_downloads`.
    moving: bool,
    /// Last time a stalled torrent was restarted to look for peers again.
    last_kick: Option<tokio::time::Instant>,
}

impl Record {
    fn new(row: DownloadRow) -> Self {
        Self {
            row,
            resolving: false,
            in_cache: false,
            unavailable: false,
            moving: false,
            last_kick: None,
        }
    }
}

/// The folder a download lives in (the `downloadsDir` it was created or moved into).
fn root_of(row: &DownloadRow) -> PathBuf {
    Path::new(&row.path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// Downloads disk usage and folder state (part of `StorageUsage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadsUsage {
    pub library_bytes: u64,
    pub free_bytes: u64,
    pub dir_available: bool,
    pub outside_dir: u64,
}

enum MoveError {
    Cancelled,
    Failed(String),
}

pub struct DownloadManager {
    cfg: DownloadsConfig,
    library_dir: std::sync::RwLock<PathBuf>,
    seed: AtomicBool,
    move_running: AtomicBool,
    move_cancel: AtomicBool,
    move_events: broadcast::Sender<MoveProgress>,
    records: Mutex<HashMap<String, Record>>,
    /// Last state emitted per download, to emit only changes.
    emitted: Mutex<HashMap<String, DownloadState>>,
    events: broadcast::Sender<DownloadChanged>,
    /// `app://error` when a download fails in the background.
    errors: broadcast::Sender<BackgroundError>,
    /// Set by [`DownloadManager::shutdown`]: the tick stops.
    stopped: AtomicBool,
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
        let (move_events, _) = broadcast::channel(64);
        let (errors, _) = broadcast::channel(16);
        Arc::new(Self {
            library_dir: std::sync::RwLock::new(cfg.library_dir.clone()),
            move_running: AtomicBool::new(false),
            move_cancel: AtomicBool::new(false),
            move_events,
            errors,
            stopped: AtomicBool::new(false),
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

    /// Background failures (disk full during a download…) for `app://error`.
    pub fn subscribe_errors(&self) -> broadcast::Receiver<BackgroundError> {
        self.errors.subscribe()
    }

    /// `downloads://move-progress` payloads.
    pub fn subscribe_moves(&self) -> broadcast::Receiver<MoveProgress> {
        self.move_events.subscribe()
    }

    /// `downloadsDir`: where new downloads go.
    pub fn downloads_dir(&self) -> PathBuf {
        match self.library_dir.read() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// New downloads go to `dir`; existing ones stay until [`Self::start_move`].
    pub fn set_downloads_dir(&self, dir: PathBuf) {
        tracing::info!(dir = %dir.display(), "downloads folder for new downloads");
        match self.library_dir.write() {
            Ok(mut g) => *g = dir,
            Err(poisoned) => *poisoned.into_inner() = dir,
        }
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
            let unavailable = !root_of(&row).is_dir();
            if unavailable {
                tracing::warn!(infohash = %row.infohash, path = %row.path, "download folder not available");
            } else if row.state == DONE {
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
                        unavailable,
                        ..Record::new(row)
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
        for infohash in self.infohashes() {
            let available = self
                .with_record(&infohash, |r| !r.unavailable)
                .unwrap_or(false);
            if available && self.wants_torrent(&infohash) {
                self.spawn_resolve(&infohash);
            }
        }
    }

    /// Whether the download's torrent belongs in the session (active or paused, or
    /// finished and seeding).
    fn wants_torrent(&self, infohash: &str) -> bool {
        let seed = self.seed.load(Ordering::Relaxed);
        self.row(infohash)
            .is_some_and(|row| match row.state.as_str() {
                ACTIVE | PAUSED => true,
                DONE => seed,
                _ => false,
            })
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
                if manager.stopped.load(Ordering::SeqCst) {
                    break;
                }
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
            _ if rec.moving || rec.unavailable => None,
            DownloadState::Done | DownloadState::Error => None,
            _ => self.cfg.engine.download_stats(&row.infohash),
        };
        let resolved = stats.as_ref().is_some_and(|s| s.resolved);
        let state = match stored {
            _ if rec.moving => DownloadState::Moving,
            _ if rec.unavailable => DownloadState::Unavailable,
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
    /// Marks the download `error` and reports it on `app://error` with its code.
    async fn fail(&self, infohash: &str, err: &AppError) {
        self.set_state(infohash, ERROR, Some(err.to_string())).await;
        let _ = self
            .errors
            .send(BackgroundError::new(err, Some(infohash.to_owned())));
    }

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
        let library_dir = self.downloads_dir();
        if !dir_available(&library_dir) {
            return Err(AppError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "downloads folder {} is not available",
                    library_dir.display()
                ),
            )));
        }
        let free = (self.cfg.free_space)(&library_dir);
        check_space(free, needed)?;

        let taken: Vec<PathBuf> = self
            .records
            .lock()
            .map(|r| r.values().map(|r| PathBuf::from(&r.row.path)).collect())
            .unwrap_or_default();
        let folder = unique_folder(
            &library_dir,
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
                    resolving: true,
                    ..Record::new(row)
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
                    self.fail(infohash, &e).await;
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
                let (resolving, parked) = self
                    .with_record(&infohash, |r| (r.resolving, r.unavailable || r.moving))
                    .unwrap_or_default();
                if self.cfg.engine.has_torrent(&infohash) {
                    self.cfg
                        .engine
                        .set_download_running(&infohash, true)
                        .await?;
                } else if !resolving && !parked {
                    self.spawn_resolve(&infohash);
                }
                self.emit_if_changed(&infohash);
            }
            ERROR
                if self
                    .with_record(&infohash, |r| r.unavailable || r.moving)
                    .unwrap_or(true) => {}
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
        let (in_cache, moving) = self
            .with_record(&infohash, |r| (r.in_cache, r.moving))
            .ok_or_else(|| not_found(&infohash))?;
        if moving {
            return Err(AppError::InvalidInput(format!(
                "download {infohash} is being moved"
            )));
        }
        let row = self.row(&infohash).ok_or_else(|| not_found(&infohash))?;
        let cache_folder = self.cfg.engine.cache_folder_of(&infohash);
        if in_cache && !delete_files {
            // Still a stream in `cache/`: back to a plain cached stream.
            self.cfg.engine.unmark_download(&infohash).await?;
        } else {
            self.cfg.engine.remove_torrent(&infohash).await?;
        }
        if delete_files {
            remove_path(Path::new(&row.path)).await?;
            if in_cache {
                let dir = cache_folder.unwrap_or_else(|| self.cfg.engine.torrent_dir(&infohash));
                remove_path(&dir).await?;
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

    /// Closing the app: stops the tick, cancels a move (waiting for it to clean up) and
    /// saves every download's state and progress. Bounded by the caller's timeout.
    pub async fn shutdown(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.cancel_move();
        while self.move_running() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let _ops = self.ops.lock().await;
        for infohash in self.infohashes() {
            let Some(view) = self.get(&infohash) else {
                continue;
            };
            let row = self.with_record(&infohash, |r| {
                if r.row.state != DONE {
                    r.row.size_bytes = view.size_bytes;
                    r.row.downloaded_bytes = r.row.downloaded_bytes.max(view.downloaded_bytes);
                }
                r.row.clone()
            });
            if let Some(row) = row {
                self.persist(&row).await;
            }
        }
        tracing::info!("downloads saved");
    }

    /// Follows the download's folder: gone → `unavailable` (torrent out of the session);
    /// back → resumes as it was. Returns whether the tick should skip the rest for it.
    async fn check_availability(
        self: &Arc<Self>,
        infohash: &str,
        row: &DownloadRow,
        unavailable: bool,
    ) -> bool {
        let available = root_of(row).is_dir();
        match (available, unavailable) {
            (false, false) => {
                tracing::warn!(%infohash, path = %row.path, "download folder disappeared");
                self.with_record(infohash, |r| r.unavailable = true);
                if let Err(e) = self.cfg.engine.remove_torrent(infohash).await {
                    tracing::warn!(%infohash, error = %e, "could not release the torrent");
                }
                true
            }
            (false, true) => true,
            (true, true) => {
                tracing::info!(%infohash, path = %row.path, "download folder is back");
                self.with_record(infohash, |r| r.unavailable = false);
                let missing =
                    row.state == DONE && Self::video_path(row).is_none_or(|p| !p.is_file());
                if missing {
                    self.fail(
                        infohash,
                        &AppError::NotFound("downloaded file is missing".into()),
                    )
                    .await;
                } else if self.wants_torrent(infohash) {
                    self.spawn_resolve(infohash);
                }
                true
            }
            (true, false) => false,
        }
    }

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
                    manager.fail(&infohash, &e).await;
                }
            }
            manager.emit_if_changed(&infohash);
        });
    }

    /// One pass: promotions, completions, errors, state change events and (with
    /// `persist`) progress saved to the DB.
    pub async fn tick(self: &Arc<Self>, persist: bool) {
        let _ops = self.ops.lock().await;
        if self.stopped.load(Ordering::SeqCst) {
            return;
        }
        for infohash in self.infohashes() {
            let Some((row, resolving, in_cache, unavailable, moving)) =
                self.with_record(&infohash, |r| {
                    (
                        r.row.clone(),
                        r.resolving,
                        r.in_cache,
                        r.unavailable,
                        r.moving,
                    )
                })
            else {
                continue;
            };
            if moving {
                self.emit_if_changed(&infohash);
                continue;
            }
            if !in_cache && self.check_availability(&infohash, &row, unavailable).await {
                self.emit_if_changed(&infohash);
                continue;
            }
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
                    // librqbit only gives text: a full disk becomes `io`.
                    let message = s.error.unwrap_or_default();
                    self.fail(&infohash, &crate::torrent::background_error(&message))
                        .await;
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
                Some(s)
                    if row.state == ACTIVE
                        && s.resolved
                        && s.stalled
                        && s.peers == 0
                        && self
                            .with_record(&infohash, |r| {
                                r.last_kick.is_none_or(|t| t.elapsed() >= KICK_EVERY)
                            })
                            .unwrap_or(false) =>
                {
                    self.with_record(&infohash, |r| {
                        r.last_kick = Some(tokio::time::Instant::now())
                    });
                    tracing::info!(%infohash, "stalled without peers, looking for peers again");
                    if let Err(e) = self.cfg.engine.restart_torrent(&infohash).await {
                        tracing::warn!(%infohash, error = %e, "could not restart the torrent");
                    }
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

impl DownloadManager {
    // -- Storage and moving ------------------------------------------------------------------

    /// Size of every download (wherever it is) and the state of `downloadsDir`.
    pub async fn usage(&self) -> DownloadsUsage {
        let dir = self.downloads_dir();
        let folders: Vec<PathBuf> = self
            .records
            .lock()
            .map(|r| r.values().map(|r| PathBuf::from(&r.row.path)).collect())
            .unwrap_or_default();
        let free_space = Arc::clone(&self.cfg.free_space);
        tokio::task::spawn_blocking(move || {
            let mut unique = folders.clone();
            unique.sort();
            unique.dedup();
            let dir_available = dir_available(&dir);
            DownloadsUsage {
                library_bytes: unique.iter().map(|f| allocated_size(f)).sum(),
                free_bytes: if dir_available { free_space(&dir) } else { 0 },
                dir_available,
                outside_dir: folders
                    .iter()
                    .filter(|f| f.parent() != Some(dir.as_path()))
                    .count() as u64,
            }
        })
        .await
        .unwrap_or(DownloadsUsage {
            library_bytes: 0,
            free_bytes: 0,
            dir_available: false,
            outside_dir: 0,
        })
    }

    pub fn move_running(&self) -> bool {
        self.move_running.load(Ordering::SeqCst)
    }

    /// `move_downloads`: moves every download that is not in `downloadsDir` there, one at
    /// a time in the background. `invalid_input` if a move is already running or the
    /// folder is not available.
    pub fn start_move(self: &Arc<Self>) -> AppResult<()> {
        let dest = self.downloads_dir();
        if !dir_available(&dest) {
            return Err(AppError::InvalidInput(format!(
                "downloads folder {} is not available",
                dest.display()
            )));
        }
        if self.move_running.swap(true, Ordering::SeqCst) {
            return Err(AppError::InvalidInput("a move is already running".into()));
        }
        self.move_cancel.store(false, Ordering::SeqCst);
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager.run_move(dest).await;
            manager.move_running.store(false, Ordering::SeqCst);
        });
        Ok(())
    }

    /// `cancel_move_downloads`: the download being copied stays where it was (the partial
    /// copy is deleted) and the rest are not moved. No move running: no-op.
    pub fn cancel_move(&self) {
        if self.move_running() {
            tracing::info!("cancelling the move of downloads");
            self.move_cancel.store(true, Ordering::SeqCst);
        }
    }

    fn emit_move(&self, p: &MoveProgress) {
        let _ = self.move_events.send(p.clone());
    }

    fn taken_folders(&self) -> Vec<PathBuf> {
        self.records
            .lock()
            .map(|r| r.values().map(|r| PathBuf::from(&r.row.path)).collect())
            .unwrap_or_default()
    }

    /// A folder in `dest` with the download's name, free on disk and among downloads.
    fn target_folder(&self, row: &DownloadRow, dest: &Path) -> PathBuf {
        let name = Path::new(&row.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| row.infohash.clone());
        let mut taken = self.taken_folders();
        loop {
            let candidate = unique_folder(dest, &name, &taken);
            if !candidate.exists() {
                return candidate;
            }
            taken.push(candidate);
        }
    }

    async fn set_path(&self, infohash: &str, path: &Path) -> AppResult<()> {
        let path = path.to_string_lossy().into_owned();
        self.cfg.db.set_download_path(infohash, &path).await?;
        self.with_record(infohash, |r| r.row.path = path);
        Ok(())
    }

    async fn run_move(self: &Arc<Self>, dest: PathBuf) {
        let mut candidates: Vec<(String, String)> = self
            .records
            .lock()
            .map(|r| {
                r.values()
                    .filter(|r| root_of(&r.row) != dest && !r.unavailable && !r.moving)
                    .map(|r| (r.row.added_at.clone(), r.row.infohash.clone()))
                    .collect()
            })
            .unwrap_or_default();
        candidates.sort();
        let mut items = Vec::new();
        for (_, infohash) in candidates {
            let Some((row, in_cache)) =
                self.with_record(&infohash, |r| (r.row.clone(), r.in_cache))
            else {
                continue;
            };
            if in_cache {
                // Nothing in the library yet: it will be promoted straight to the new folder.
                let target = self.target_folder(&row, &dest);
                if let Err(e) = self.set_path(&infohash, &target).await {
                    tracing::warn!(%infohash, error = %e, "could not retarget download");
                }
                continue;
            }
            let src = PathBuf::from(&row.path);
            let size = tokio::task::spawn_blocking(move || tree_len(&src))
                .await
                .unwrap_or(0);
            items.push((infohash, size));
        }

        let mut progress = MoveProgress {
            total: items.len() as u32,
            bytes_total: items.iter().map(|(_, s)| s).sum(),
            ..Default::default()
        };
        tracing::info!(count = items.len(), bytes = progress.bytes_total, dest = %dest.display(), "moving downloads");
        self.emit_move(&progress);
        let mut base = 0;
        for (i, (infohash, size)) in items.iter().enumerate() {
            if self.move_cancel.load(Ordering::SeqCst) {
                progress.cancelled = true;
                break;
            }
            progress.index = i as u32 + 1;
            progress.infohash = Some(infohash.clone());
            progress.bytes_done = base;
            self.emit_move(&progress);
            match self.move_one(infohash, &dest, base, &mut progress).await {
                Ok(()) => {}
                Err(MoveError::Cancelled) => progress.cancelled = true,
                Err(MoveError::Failed(message)) => {
                    tracing::warn!(%infohash, %message, "could not move download");
                    progress.failed.push(MoveFailure {
                        infohash: infohash.clone(),
                        message,
                    });
                }
            }
            if progress.cancelled {
                break;
            }
            base += size;
            progress.bytes_done = base;
        }
        progress.finished = true;
        tracing::info!(
            cancelled = progress.cancelled,
            failed = progress.failed.len(),
            "move of downloads finished"
        );
        self.emit_move(&progress);
    }

    /// Moves one download: out of the torrent session (files closed), folder moved, then
    /// back with its previous state (in its old folder if the move failed).
    async fn move_one(
        self: &Arc<Self>,
        infohash: &str,
        dest: &Path,
        base: u64,
        progress: &mut MoveProgress,
    ) -> Result<(), MoveError> {
        let row = {
            let _ops = self.ops.lock().await;
            let (row, resolving) = self
                .with_record(infohash, |r| (r.row.clone(), r.resolving))
                .ok_or_else(|| MoveError::Failed("download removed".into()))?;
            if resolving {
                return Err(MoveError::Failed("download is busy, try again".into()));
            }
            self.with_record(infohash, |r| r.moving = true);
            self.emit_if_changed(infohash);
            row
        };
        let released = self.cfg.engine.remove_torrent(infohash).await;
        let result = match released {
            Ok(()) => self.relocate(&row, dest, base, progress).await,
            Err(e) => Err(MoveError::Failed(e.to_string())),
        };
        let _ops = self.ops.lock().await;
        let result = match result {
            Ok(target) => self
                .set_path(infohash, &target)
                .await
                .map_err(|e| MoveError::Failed(e.to_string())),
            Err(e) => Err(e),
        };
        self.with_record(infohash, |r| r.moving = false);
        if self.wants_torrent(infohash) {
            self.spawn_resolve(infohash);
        }
        self.emit_if_changed(infohash);
        result
    }

    /// Moves the folder: `rename` on the same disk; otherwise space check, copy (with
    /// progress and cancellation) and delete the source once the copy is complete.
    async fn relocate(
        &self,
        row: &DownloadRow,
        dest: &Path,
        base: u64,
        progress: &mut MoveProgress,
    ) -> Result<PathBuf, MoveError> {
        let src = PathBuf::from(&row.path);
        let target = self.target_folder(row, dest);
        if tokio::fs::symlink_metadata(&src).await.is_err() {
            // Nothing on disk yet (e.g. just created): only the path changes.
            return Ok(target);
        }
        if !self.cfg.always_copy {
            match tokio::fs::rename(&src, &target).await {
                Ok(()) => {
                    tracing::info!(infohash = %row.infohash, to = %target.display(), "download folder renamed");
                    return Ok(target);
                }
                Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {}
                Err(e) => {
                    return Err(MoveError::Failed(format!(
                        "moving {} to {}: {e}",
                        src.display(),
                        target.display()
                    )))
                }
            }
        }
        let size = {
            let src = src.clone();
            tokio::task::spawn_blocking(move || tree_len(&src))
                .await
                .unwrap_or(0)
        };
        check_space((self.cfg.free_space)(dest), size)
            .map_err(|e| MoveError::Failed(e.to_string()))?;
        match self.copy_tree(&src, &target, base, progress).await {
            Ok(()) => {
                if let Err(e) = remove_path(&src).await {
                    tracing::warn!(src = %src.display(), error = %e, "copied, but the source could not be deleted");
                }
                tracing::info!(infohash = %row.infohash, to = %target.display(), "download folder copied");
                Ok(target)
            }
            Err(e) => {
                if let Err(err) = remove_path(&target).await {
                    tracing::warn!(target = %target.display(), error = %err, "could not delete the partial copy");
                }
                Err(e)
            }
        }
    }

    async fn copy_tree(
        &self,
        src: &Path,
        dst: &Path,
        base: u64,
        progress: &mut MoveProgress,
    ) -> Result<(), MoveError> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let io = |what: &str, p: &Path, e: std::io::Error| {
            MoveError::Failed(format!("{what} {}: {e}", p.display()))
        };
        let files = {
            let dir = src.to_path_buf();
            tokio::task::spawn_blocking(move || list_files(&dir))
                .await
                .map_err(|e| MoveError::Failed(e.to_string()))?
                .map_err(|e| io("reading", src, e))?
        };
        let copied = AtomicU64::new(0);
        let mut last_emit = tokio::time::Instant::now();
        let mut buf = vec![0u8; COPY_CHUNK];
        tokio::fs::create_dir_all(dst)
            .await
            .map_err(|e| io("creating", dst, e))?;
        for rel in files {
            let (from, to) = (src.join(&rel), dst.join(&rel));
            if let Some(parent) = to.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| io("creating", parent, e))?;
            }
            let mut input = tokio::fs::File::open(&from)
                .await
                .map_err(|e| io("opening", &from, e))?;
            let mut output = tokio::fs::File::create(&to)
                .await
                .map_err(|e| io("creating", &to, e))?;
            loop {
                if self.move_cancel.load(Ordering::SeqCst) {
                    return Err(MoveError::Cancelled);
                }
                let n = input
                    .read(&mut buf)
                    .await
                    .map_err(|e| io("reading", &from, e))?;
                if n == 0 {
                    break;
                }
                output
                    .write_all(&buf[..n])
                    .await
                    .map_err(|e| io("writing", &to, e))?;
                let done = copied.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
                if last_emit.elapsed() >= MOVE_PROGRESS_EVERY {
                    last_emit = tokio::time::Instant::now();
                    progress.bytes_done = base + done;
                    self.emit_move(progress);
                }
                if let Some(delay) = self.cfg.copy_chunk_delay {
                    tokio::time::sleep(delay).await;
                }
            }
            output.flush().await.map_err(|e| io("writing", &to, e))?;
            output.sync_all().await.map_err(|e| io("syncing", &to, e))?;
        }
        Ok(())
    }
}

/// Files under `dir`, relative to it.
fn list_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for item in std::fs::read_dir(dir.join(&rel))? {
            let item = item?;
            let child = rel.join(item.file_name());
            if item.file_type()?.is_dir() {
                stack.push(child);
            } else {
                out.push(child);
            }
        }
    }
    Ok(out)
}

/// Bytes a copy of `path` writes (apparent lengths; sparse holes are written too).
fn tree_len(path: &Path) -> u64 {
    list_files(path)
        .map(|files| {
            files
                .iter()
                .filter_map(|f| std::fs::metadata(path.join(f)).ok())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
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
