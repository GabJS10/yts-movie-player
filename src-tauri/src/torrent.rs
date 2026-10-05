//! Torrent engine (librqbit): one session, streaming sessions keyed by infohash, file
//! readers for the local HTTP server and the `torrent://stats` feed.
//!
//! Adding a torrent: the `.torrent` from YTS is preferred (no metadata wait); if it can't
//! be downloaded quickly we fall back to a magnet. Only the largest video file is selected.
//!
//! The same torrent can be a stream, a download or both. Streams live in
//! `cache/<infohash>/`; downloads in their `library/` folder. A torrent runs while a stream
//! is open or its download wants it running (see [`TorrentEngine::reconcile`]). Finished
//! downloads are played straight from disk ([`TorrentEngine::serve_local`]).

use std::collections::{HashMap, HashSet};
use std::io::SeekFrom;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use librqbit::api::{Api, TorrentIdOrHash};
use librqbit::dht::{DhtPersistenceConfig, Id20};
use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, DhtSessionConfig, ListenerOptions,
    ManagedTorrent, Session, SessionOptions, TorrentStatsState,
};
use tokio::io::{AsyncRead, AsyncSeek, ReadBuf};
use tokio::sync::{broadcast, OnceCell};
use tokio::time::Instant;
use url::Url;

use crate::error::{AppError, AppResult};
use crate::images::{host_allowed, restricted_client, HostAllowlist};
use crate::platform::retry_locked;
use crate::types::{
    BackgroundError, PieceMapWindow, StreamPhase, StreamSession, StreamSource, TorrentStats,
    VideoCodec,
};

/// Public trackers added to every torrent and magnet.
///
/// The first three are the ones YTS recommends/embeds that are still alive; the rest are
/// well-known open trackers. Verified on 2026-10-03 with a BEP 15 UDP connect handshake.
/// Dead at that date (not included): 9.rarbg.to, tracker.cyberia.is, open.tracker.cl,
/// p4p.arenabg.ch/.com, tracker.coppersurfer.tk, tracker.openbittorrent.com,
/// glotorrents.pw, torrent.gresille.org, exodus.desync.com.
pub const TRACKERS: [&str; 7] = [
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://tracker.leechers-paradise.org:6969/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://explodie.org:6969/announce",
    "udp://tracker.dler.org:6969/announce",
];

/// `YTS_PLAYER_NO_DHT=1` (E2E): no DHT, no local discovery and no public trackers; only the
/// trackers inside each `.torrent` (the local seeder's).
pub const NO_DHT_ENV: &str = "YTS_PLAYER_NO_DHT";

/// Whether a `YTS_PLAYER_NO_DHT` value turns the public swarm off.
pub fn no_dht(value: Option<&str>) -> bool {
    value.is_some_and(|v| matches!(v.trim(), "1" | "true" | "yes"))
}

/// Read-ahead window librqbit prioritizes for every open file stream
/// (`PER_STREAM_BUF_DEFAULT` in librqbit 9.0.1). Mirrored to mark "priority" cells.
pub const STREAM_PRIORITY_WINDOW: u64 = 32 * 1024 * 1024;

pub const PIECE_MAP_CELLS: usize = 200;

/// Bytes covered by `pieceMap`, starting at the read position.
pub const PIECE_MAP_WINDOW: u64 = 64 * 1024 * 1024;

const VIDEO_EXTENSIONS: [&str; 7] = ["mp4", "mkv", "m4v", "webm", "avi", "mov", "ts"];

/// Builds `magnet:?xt=urn:btih:<hash>&dn=<name>&tr=<tracker>…`.
pub fn magnet(infohash: &str, name: &str, trackers: &[String]) -> String {
    let mut q = url::form_urlencoded::Serializer::new(String::new());
    if !name.is_empty() {
        q.append_pair("dn", name);
    }
    for tr in trackers {
        q.append_pair("tr", tr);
    }
    let rest = q.finish();
    if rest.is_empty() {
        format!("magnet:?xt=urn:btih:{infohash}")
    } else {
        format!("magnet:?xt=urn:btih:{infohash}&{rest}")
    }
}

pub fn is_valid_infohash(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Index of the largest video file, or of the largest file if none looks like a video.
pub fn pick_video_file(files: &[(PathBuf, u64)]) -> Option<usize> {
    let is_video = |p: &Path| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
    };
    let largest = |videos_only: bool| {
        files
            .iter()
            .enumerate()
            .filter(|(_, (p, _))| !videos_only || is_video(p))
            .max_by_key(|(_, (_, len))| *len)
            .map(|(i, _)| i)
    };
    largest(true).or_else(|| largest(false))
}

pub fn video_mime(file_name: &str) -> &'static str {
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mkv") => "video/x-matroska",
        Some("webm") => "video/webm",
        Some("avi") => "video/x-msvideo",
        Some("mov") => "video/quicktime",
        Some("ts") => "video/mp2t",
        _ => "application/octet-stream",
    }
}

// ---------------------------------------------------------------------------
// Piece geometry (pure, unit tested)
// ---------------------------------------------------------------------------

/// Where a file sits inside the torrent. Pieces have a fixed length except the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileGeometry {
    pub offset: u64,
    pub len: u64,
    pub piece_len: u64,
}

impl FileGeometry {
    fn end(&self) -> u64 {
        self.offset + self.len
    }

    fn piece_of(&self, abs: u64) -> usize {
        (abs / self.piece_len) as usize
    }

    /// Inclusive range of pieces covering the file.
    fn pieces(&self) -> Option<(usize, usize)> {
        (self.len > 0).then(|| (self.piece_of(self.offset), self.piece_of(self.end() - 1)))
    }

    fn piece_bytes(&self, piece: usize) -> (u64, u64) {
        let start = (piece as u64 * self.piece_len).max(self.offset);
        let end = ((piece as u64 + 1) * self.piece_len).min(self.end());
        (start, end)
    }
}

fn has(have: &[bool], piece: usize) -> bool {
    have.get(piece).copied().unwrap_or(false)
}

/// Downloaded parts of the file as `[start, end)` fractions of its length.
pub fn available_ranges(geo: &FileGeometry, have: &[bool]) -> Vec<(f64, f64)> {
    let Some((first, last)) = geo.pieces() else {
        return Vec::new();
    };
    let len = geo.len as f64;
    let mut ranges = Vec::new();
    let mut run: Option<(u64, u64)> = None;
    for piece in first..=last {
        if has(have, piece) {
            let (s, e) = geo.piece_bytes(piece);
            run = Some(match run {
                Some((start, _)) => (start, e),
                None => (s, e),
            });
        } else if let Some((s, e)) = run.take() {
            ranges.push(((s - geo.offset) as f64 / len, (e - geo.offset) as f64 / len));
        }
    }
    if let Some((s, e)) = run {
        ranges.push(((s - geo.offset) as f64 / len, (e - geo.offset) as f64 / len));
    }
    ranges
}

/// Contiguous downloaded bytes starting at `position` (relative to the file).
pub fn buffered_ahead(geo: &FileGeometry, have: &[bool], position: u64) -> u64 {
    if position >= geo.len {
        return 0;
    }
    let abs = geo.offset + position;
    let Some((_, last)) = geo.pieces() else {
        return 0;
    };
    let mut piece = geo.piece_of(abs);
    while piece <= last && has(have, piece) {
        piece += 1;
    }
    let end = (piece as u64 * geo.piece_len).min(geo.end());
    end.saturating_sub(abs)
}

/// Pieces librqbit prioritizes for readers at `positions` (one window per open stream).
pub fn priority_pieces(geo: &FileGeometry, positions: &[u64]) -> HashSet<usize> {
    let mut set = HashSet::new();
    for &pos in positions {
        if pos >= geo.len {
            continue;
        }
        let start = geo.offset + pos;
        let end = (start + STREAM_PRIORITY_WINDOW).min(geo.end());
        set.extend(geo.piece_of(start)..=geo.piece_of(end - 1));
    }
    set
}

/// `[start, end)` of the file shown in `pieceMap`: `window` bytes from `position`, or the
/// whole file when it is smaller than the window.
pub fn piece_map_window(file_len: u64, position: u64, window: u64) -> (u64, u64) {
    if file_len <= window {
        return (0, file_len);
    }
    let start = position.min(file_len - 1);
    (start, (start + window).min(file_len))
}

/// File bytes `[start, end)` sampled into `cells` cells: "1" all pieces downloaded, "2" a
/// missing piece is in a stream's priority window, "0" missing. ("3", in flight, is not
/// exposed by librqbit.)
pub fn piece_map(
    geo: &FileGeometry,
    have: &[bool],
    priority: &HashSet<usize>,
    (start, end): (u64, u64),
    cells: usize,
) -> String {
    let end = end.min(geo.len);
    let span = end.saturating_sub(start);
    (0..cells)
        .map(|i| {
            if span == 0 {
                return '0';
            }
            let b0 = start + span * i as u64 / cells as u64;
            let b1 = (start + span * (i as u64 + 1) / cells as u64)
                .max(b0 + 1)
                .min(end);
            let first = geo.piece_of(geo.offset + b0);
            let last = geo.piece_of(geo.offset + b1 - 1);
            let missing: Vec<usize> = (first..=last).filter(|&p| !has(have, p)).collect();
            if missing.is_empty() {
                '1'
            } else if missing.iter().any(|p| priority.contains(p)) {
                '2'
            } else {
                '0'
            }
        })
        .collect()
}

/// How a torrent that isn't in the session yet is being resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolving {
    TorrentFile,
    Magnet,
}

#[derive(Debug, Clone, Copy)]
pub struct PhaseInput {
    pub resolving: Option<Resolving>,
    pub peers: u32,
    pub downloaded: u64,
    pub file_len: u64,
    pub position: u64,
    pub buffered_ahead: u64,
    pub buffer_target: u64,
    /// Time without peers or without download speed.
    pub idle_for: Duration,
    pub stall_after: Duration,
    /// Long enough since `start_stream` (`noPeersAfter`) without ever connecting to a peer.
    pub no_peers: bool,
}

pub fn compute_phase(i: &PhaseInput) -> StreamPhase {
    let phase = base_phase(i);
    // The torrent keeps trying; the front offers another version meanwhile.
    if i.no_peers && !matches!(phase, StreamPhase::Ready | StreamPhase::Done) {
        StreamPhase::NoPeers
    } else {
        phase
    }
}

fn base_phase(i: &PhaseInput) -> StreamPhase {
    if i.resolving.is_none() && i.file_len > 0 && i.downloaded >= i.file_len {
        return StreamPhase::Done;
    }
    if i.idle_for >= i.stall_after {
        return StreamPhase::Stalled;
    }
    match i.resolving {
        Some(Resolving::Magnet) => return StreamPhase::Metadata,
        Some(Resolving::TorrentFile) => return StreamPhase::Connecting,
        None => {}
    }
    if i.peers == 0 && i.downloaded == 0 {
        return StreamPhase::Connecting;
    }
    // Near the end of the file the target can't be reached: whatever is left suffices
    // (nothing left to read at all also counts as ready).
    let target = i.buffer_target.min(i.file_len.saturating_sub(i.position));
    if i.buffered_ahead >= target {
        StreamPhase::Ready
    } else {
        StreamPhase::Buffering
    }
}

/// Parses a `Range` header against a resource of `len` bytes. Only the first range of a
/// multi-range request is honored. `Ok(None)` means no header (serve everything).
pub fn parse_range(header: Option<&str>, len: u64) -> Result<Option<(u64, u64)>, RangeError> {
    let Some(header) = header else {
        return Ok(None);
    };
    let spec = header
        .trim()
        .strip_prefix("bytes=")
        .ok_or(RangeError)?
        .split(',')
        .next()
        .ok_or(RangeError)?
        .trim();
    let (a, b) = spec.split_once('-').ok_or(RangeError)?;
    let (a, b) = (a.trim(), b.trim());
    if len == 0 {
        return Err(RangeError);
    }
    let range = if a.is_empty() {
        let n: u64 = b.parse().map_err(|_| RangeError)?;
        if n == 0 {
            return Err(RangeError);
        }
        (len - n.min(len), len - 1)
    } else {
        let start: u64 = a.parse().map_err(|_| RangeError)?;
        let end = if b.is_empty() {
            len - 1
        } else {
            b.parse::<u64>().map_err(|_| RangeError)?.min(len - 1)
        };
        if start >= len || start > end {
            return Err(RangeError);
        }
        (start, end)
    };
    Ok(Some(range))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeError;

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Where streamed torrents are written (`cache/`).
    pub output_dir: PathBuf,
    pub dht: bool,
    pub dht_state_file: Option<PathBuf>,
    /// `None`: random port on all interfaces.
    pub listen_addr: Option<SocketAddr>,
    /// Public trackers added to every torrent (on top of the ones in its `.torrent`).
    pub trackers: Vec<String>,
    /// No trackers at all, not even the `.torrent`'s (tests that only use `initial_peers`).
    pub disable_trackers: bool,
    /// Extra peers for every torrent (tests).
    pub initial_peers: Vec<SocketAddr>,
    /// Hosts the `.torrent` may be downloaded from.
    pub allowed_torrent_hosts: Vec<String>,
    pub torrent_file_timeout: Duration,
    pub metadata_timeout: Duration,
    pub init_timeout: Duration,
    pub buffer_target_bytes: u64,
    pub stats_interval: Duration,
    pub stall_after: Duration,
    /// `no_peers` phase after this long since `start_stream` without any peer.
    pub no_peers_after: Duration,
    pub download_limit_bps: Option<NonZeroU32>,
    pub upload_limit_bps: Option<NonZeroU32>,
    /// Local HTTP server origin, e.g. `http://127.0.0.1:4321`.
    pub local_base: String,
}

impl EngineConfig {
    pub fn new(output_dir: PathBuf, local_base: String) -> Self {
        Self {
            output_dir,
            dht: true,
            dht_state_file: None,
            listen_addr: None,
            trackers: TRACKERS.iter().map(|s| s.to_string()).collect(),
            disable_trackers: false,
            initial_peers: Vec::new(),
            allowed_torrent_hosts: Vec::new(),
            torrent_file_timeout: Duration::from_secs(5),
            metadata_timeout: Duration::from_secs(60),
            init_timeout: Duration::from_secs(120),
            buffer_target_bytes: 8 * 1024 * 1024,
            stats_interval: Duration::from_secs(1),
            stall_after: Duration::from_secs(30),
            no_peers_after: Duration::from_secs(60),
            download_limit_bps: None,
            upload_limit_bps: None,
            local_base,
        }
    }
}

/// Everything `start_stream` needs to know about the torrent (from the YTS catalog).
#[derive(Debug, Clone)]
pub struct StreamRequest {
    pub movie_id: u64,
    pub infohash: String,
    pub title: String,
    pub torrent_url: Option<String>,
    pub video_codec: VideoCodec,
    pub seeds: u32,
}

/// A download as the engine sees it: where it goes and whether it should run.
#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub stream: StreamRequest,
    /// `library/<Title (year) [quality]>/`.
    pub folder: PathBuf,
    /// Known `.torrent` (saved in the DB): no download, no metadata wait, works offline.
    pub torrent_bytes: Option<Bytes>,
    pub run: bool,
}

/// What [`TorrentEngine::add_download`] resolved.
#[derive(Debug, Clone)]
pub struct DownloadTorrent {
    /// Video path relative to the torrent folder (`cache/<infohash>/` or the library one).
    pub rel_path: PathBuf,
    pub file_len: u64,
    pub torrent_bytes: Bytes,
    /// Still in `cache/` (it was being streamed): waiting for [`TorrentEngine::promote`].
    pub in_cache: bool,
}

/// Live numbers of a download (see [`TorrentEngine::download_stats`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DownloadStats {
    /// Metadata known and torrent added (else still resolving: "queued").
    pub resolved: bool,
    pub file_len: u64,
    pub downloaded: u64,
    pub down_bps: u64,
    pub peers: u32,
    pub stalled: bool,
    pub error: Option<String>,
}

/// A finished download played from disk (`source: "library"`).
#[derive(Debug, Clone)]
pub struct LocalStream {
    pub infohash: String,
    pub movie_id: u64,
    pub path: PathBuf,
    pub video_codec: VideoCodec,
}

/// File index used in the URL of library streams (they don't go through the torrent).
pub const LOCAL_FILE_IDX: usize = 0;

struct LocalFile {
    movie_id: u64,
    path: PathBuf,
    len: u64,
    name: String,
    readers: Arc<Readers>,
}

/// Where a torrent's files are (fixed for the entry's life).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Output {
    /// `<cacheDir>/<infohash>/`, the cache folder at the time the stream started.
    Cache(PathBuf),
    Library(PathBuf),
}

impl Output {
    fn is_cache(&self) -> bool {
        matches!(self, Self::Cache(_))
    }

    fn folder(&self) -> &Path {
        match self {
            Self::Cache(dir) | Self::Library(dir) => dir,
        }
    }
}

struct Active {
    handle: Arc<ManagedTorrent>,
    file_idx: usize,
    file_name: String,
    rel_path: PathBuf,
    geo: FileGeometry,
}

#[derive(Default)]
struct Readers {
    next_id: AtomicU64,
    /// reader id → (position, last activity)
    map: Mutex<HashMap<u64, (u64, Instant)>>,
    /// Position of the last read, kept after the reader closes.
    last: Mutex<Option<u64>>,
}

impl Readers {
    fn open(&self, position: u64) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.update(id, position);
        id
    }

    fn update(&self, id: u64, position: u64) {
        if let Ok(mut map) = self.map.lock() {
            map.insert(id, (position, Instant::now()));
        }
        if let Ok(mut last) = self.last.lock() {
            *last = Some(position);
        }
    }

    fn has_open(&self) -> bool {
        self.map.lock().map(|m| !m.is_empty()).unwrap_or(false)
    }

    fn close(&self, id: u64) {
        if let Ok(mut map) = self.map.lock() {
            map.remove(&id);
        }
    }

    /// (positions of the open readers, current read position: the most recently active
    /// reader, else the last position read, else `None` before the first read)
    fn snapshot(&self) -> (Vec<u64>, Option<u64>) {
        let Ok(map) = self.map.lock() else {
            return (Vec::new(), None);
        };
        let positions = map.values().map(|(p, _)| *p).collect();
        let current = map
            .values()
            .max_by_key(|(_, t)| *t)
            .map(|(p, _)| *p)
            .or_else(|| self.last.lock().ok().and_then(|l| *l));
        (positions, current)
    }
}

struct Entry {
    req: StreamRequest,
    output: Output,
    torrent_bytes: Option<Bytes>,
    /// `None`: only a stream. `Some(run)`: also a download that wants to run or not.
    download: Mutex<Option<bool>>,
    started: Instant,
    resolving: Mutex<Option<Resolving>>,
    active: OnceCell<Active>,
    readers: Arc<Readers>,
    stopped: AtomicBool,
    last_alive: Mutex<Instant>,
    /// A peer was connected at some point (then `no_peers` never shows up).
    ever_connected: AtomicBool,
    /// Last `start_stream` that opened the stream (for `no_peers`).
    stream_started: Mutex<Instant>,
    /// Serializes start/stop for this torrent: they apply in call order (React StrictMode
    /// fires start → stop → start back to back).
    control: Arc<tokio::sync::Mutex<()>>,
}

impl Entry {
    fn new(req: StreamRequest, cache_folder: PathBuf) -> Self {
        Self::with_output(req, Output::Cache(cache_folder), None)
    }

    fn with_output(req: StreamRequest, output: Output, torrent_bytes: Option<Bytes>) -> Self {
        Self {
            req,
            output,
            torrent_bytes,
            download: Mutex::new(None),
            started: Instant::now(),
            resolving: Mutex::new(None),
            active: OnceCell::new(),
            readers: Arc::new(Readers::default()),
            stopped: AtomicBool::new(false),
            last_alive: Mutex::new(Instant::now()),
            ever_connected: AtomicBool::new(false),
            stream_started: Mutex::new(Instant::now()),
            control: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn download(&self) -> Option<bool> {
        self.download.lock().ok().and_then(|g| *g)
    }

    fn set_download(&self, run: Option<bool>) {
        if let Ok(mut g) = self.download.lock() {
            *g = run;
        }
    }

    /// An open stream, a reader (e.g. VLC) or a download: the cache must not touch it.
    fn in_use(&self) -> bool {
        !self.stopped.load(Ordering::Relaxed)
            || self.readers.has_open()
            || self.download().is_some()
    }

    fn set_resolving(&self, r: Option<Resolving>) {
        if let Ok(mut g) = self.resolving.lock() {
            *g = r;
        }
    }

    fn no_peers(&self, after: Duration) -> bool {
        !self.ever_connected.load(Ordering::Relaxed)
            && self
                .stream_started
                .lock()
                .map(|t| t.elapsed() >= after)
                .unwrap_or(false)
    }

    fn mark_alive(&self) {
        if let Ok(mut g) = self.last_alive.lock() {
            *g = Instant::now();
        }
    }

    fn idle_for(&self) -> Duration {
        self.last_alive
            .lock()
            .map(|t| t.elapsed())
            .unwrap_or_default()
    }
}

pub struct TorrentEngine {
    session: Arc<Session>,
    api: Api,
    cfg: EngineConfig,
    http: reqwest::Client,
    allowed_hosts: HostAllowlist,
    entries: Mutex<HashMap<String, Arc<Entry>>>,
    stats_tx: broadcast::Sender<TorrentStats>,
    /// `app://error` for torrents that fail in the background (once per failure).
    errors_tx: broadcast::Sender<BackgroundError>,
    reported_errors: Mutex<HashSet<String>>,
    /// `bufferTargetBytes` setting, changeable at runtime.
    buffer_target: AtomicU64,
    /// Finished downloads served from disk, by infohash.
    local: Mutex<HashMap<String, LocalFile>>,
    /// Where new streams go (`cacheDir`, changeable at runtime).
    cache_dir: std::sync::RwLock<PathBuf>,
}

fn torrent_err(context: &str, e: impl std::fmt::Display) -> AppError {
    AppError::Torrent(format!("{context}: {e:#}"))
}

impl TorrentEngine {
    pub async fn new(cfg: EngineConfig) -> AppResult<Arc<Self>> {
        tokio::fs::create_dir_all(&cfg.output_dir).await?;
        let dht = cfg.dht.then(|| DhtSessionConfig {
            persistence: cfg.dht_state_file.clone().map(|f| DhtPersistenceConfig {
                config_filename: Some(f),
                ..Default::default()
            }),
            ..Default::default()
        });
        let opts = SessionOptions {
            dht,
            disable_trackers: cfg.disable_trackers,
            listen: Some(ListenerOptions {
                listen_addr: cfg
                    .listen_addr
                    .unwrap_or_else(|| ListenerOptions::default().listen_addr),
                ..Default::default()
            }),
            disable_local_service_discovery: !cfg.dht,
            ratelimits: LimitsConfig {
                download_bps: cfg.download_limit_bps,
                upload_bps: cfg.upload_limit_bps,
            },
            ..Default::default()
        };
        let session = Session::new_with_opts(cfg.output_dir.clone(), opts)
            .await
            .map_err(|e| torrent_err("creating session", e))?;
        let allowed_hosts = HostAllowlist::new(cfg.allowed_torrent_hosts.iter().cloned());
        let http = restricted_client(
            allowed_hosts.clone(),
            cfg.torrent_file_timeout,
            cfg.torrent_file_timeout,
        )?;
        let (stats_tx, _) = broadcast::channel(64);
        let (errors_tx, _) = broadcast::channel(16);
        let cfg_buffer_target = cfg.buffer_target_bytes;
        let cfg_output_dir = cfg.output_dir.clone();
        let engine = Arc::new(Self {
            api: Api::new(Arc::clone(&session), None),
            session,
            cfg,
            http,
            allowed_hosts,
            entries: Mutex::new(HashMap::new()),
            stats_tx,
            errors_tx,
            reported_errors: Mutex::new(HashSet::new()),
            buffer_target: AtomicU64::new(cfg_buffer_target),
            local: Mutex::new(HashMap::new()),
            cache_dir: std::sync::RwLock::new(cfg_output_dir),
        });
        spawn_stats_loop(Arc::downgrade(&engine), engine.cfg.stats_interval);
        Ok(engine)
    }

    pub fn buffer_target_bytes(&self) -> u64 {
        self.buffer_target.load(Ordering::Relaxed)
    }

    pub fn set_buffer_target_bytes(&self, bytes: u64) {
        self.buffer_target.store(bytes, Ordering::Relaxed);
    }

    /// Replaces the hosts the `.torrent` may be downloaded from.
    pub fn set_allowed_torrent_hosts(&self, hosts: impl IntoIterator<Item = String>) {
        self.allowed_hosts.set(hosts);
    }

    /// Where new streams are written.
    pub fn cache_dir(&self) -> PathBuf {
        match self.cache_dir.read() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// New streams go to `dir`; open ones finish where they are.
    pub fn set_cache_dir(&self, dir: PathBuf) {
        tracing::info!(dir = %dir.display(), "cache folder for new streams");
        match self.cache_dir.write() {
            Ok(mut g) => *g = dir,
            Err(poisoned) => *poisoned.into_inner() = dir,
        }
    }

    /// Folder of a new streamed torrent: `<cacheDir>/<infohash>/`. Unit of the cache LRU.
    pub fn torrent_dir(&self, infohash: &str) -> PathBuf {
        self.cache_dir().join(infohash.to_ascii_lowercase())
    }

    /// The cache folder the torrent actually lives in, if it is a stream in the engine.
    pub fn cache_folder_of(&self, infohash: &str) -> Option<PathBuf> {
        let entry = self.entry(&infohash.to_ascii_lowercase())?;
        match &entry.output {
            Output::Cache(dir) => Some(dir.clone()),
            Output::Library(_) => None,
        }
    }

    /// Records an access for the cache LRU (the folder's mtime). Best effort.
    fn touch(&self, dir: &Path) {
        if let Err(e) = crate::platform::touch_dir(dir) {
            tracing::debug!(dir = %dir.display(), error = %e, "could not touch torrent folder");
        }
    }

    /// Before the first read, `pieceMap`/buffering start at this fraction of the file
    /// (the saved progress) instead of byte 0.
    pub fn hint_start_fraction(&self, infohash: &str, fraction: f64) {
        let Some(entry) = self.entry(&infohash.to_ascii_lowercase()) else {
            return;
        };
        let Some(active) = entry.active.get() else {
            return;
        };
        if !fraction.is_finite() || fraction <= 0.0 {
            return;
        }
        let byte = (active.geo.len as f64 * fraction.min(1.0)) as u64;
        if let Ok(mut last) = entry.readers.last.lock() {
            last.get_or_insert(byte);
        };
    }

    /// Infohashes the cache must not touch: streams not stopped (including ones still
    /// starting), any torrent with an open reader (e.g. an external player) and downloads
    /// still waiting in `cache/` to be moved to `library/`.
    pub fn in_use(&self) -> HashSet<String> {
        let Ok(entries) = self.entries.lock() else {
            return HashSet::new();
        };
        entries
            .iter()
            .filter(|(_, e)| e.in_use())
            .map(|(h, _)| h.clone())
            .collect()
    }

    /// Removes a stopped torrent from the session and deletes its folder. Returns `false`
    /// (and touches nothing) if the torrent is in use.
    ///
    /// The torrent must leave the librqbit session first: a paused torrent keeps its files
    /// open, and deleting them would not free any space until the app exits.
    ///
    /// Only the torrent whose files are in `dir` is touched: an old copy left in a previous
    /// cache folder is deleted even if the same infohash is streaming somewhere else.
    pub async fn evict(&self, infohash: &str, dir: &Path) -> AppResult<bool> {
        let infohash = infohash.to_ascii_lowercase();
        let entry = self.entry(&infohash).filter(|e| e.output.folder() == dir);
        let _control = match &entry {
            Some(e) => Some(Arc::clone(&e.control).lock_owned().await),
            None => None,
        };
        if let Some(e) = &entry {
            if e.in_use() {
                return Ok(false);
            }
            if let Ok(mut entries) = self.entries.lock() {
                if entries
                    .get(&infohash)
                    .is_some_and(|cur| Arc::ptr_eq(cur, e))
                {
                    entries.remove(&infohash);
                }
            }
        }
        if let Ok(id) = Id20::from_str(&infohash) {
            let in_dir = self
                .session
                .get(TorrentIdOrHash::Hash(id))
                .is_some_and(|h| h.output_folder() == dir);
            if in_dir {
                self.session
                    .delete(TorrentIdOrHash::Hash(id), false)
                    .await
                    .map_err(|e| torrent_err("removing torrent from session", e))?;
                tracing::info!(%infohash, "torrent removed from session");
            }
        }
        remove_path(dir).await?;
        Ok(true)
    }

    /// Closing: every torrent leaves the session (paused, files closed, nothing deleted),
    /// then the session stops (listener, DHT, trackers).
    pub async fn shutdown(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
        if let Ok(mut local) = self.local.lock() {
            local.clear();
        }
        let ids: Vec<Id20> = self
            .session
            .with_torrents(|torrents| torrents.map(|(_, t)| t.info_hash()).collect());
        for id in ids {
            if let Err(e) = self.session.delete(TorrentIdOrHash::Hash(id), false).await {
                tracing::warn!(infohash = %id.as_string(), error = %e, "could not close torrent");
            }
        }
        self.session.stop().await;
        tracing::info!("torrent session stopped");
    }

    /// Background torrent failures (disk full, permissions…) for `app://error`.
    pub fn subscribe_errors(&self) -> broadcast::Receiver<BackgroundError> {
        self.errors_tx.subscribe()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TorrentStats> {
        self.stats_tx.subscribe()
    }

    pub fn listen_addr(&self) -> Option<SocketAddr> {
        self.session.listen_addr()
    }

    fn entry(&self, infohash: &str) -> Option<Arc<Entry>> {
        self.entries.lock().ok()?.get(infohash).cloned()
    }

    /// Idempotent: returns the existing session if the torrent is already active.
    pub async fn start_stream(&self, req: StreamRequest) -> AppResult<StreamSession> {
        let infohash = req.infohash.to_ascii_lowercase();
        if !is_valid_infohash(&infohash) {
            return Err(AppError::InvalidInput(format!(
                "invalid infohash {infohash:?}"
            )));
        }
        let (entry, _control) = self
            .lock_entry(&infohash, || {
                Entry::new(
                    StreamRequest {
                        infohash: infohash.clone(),
                        ..req.clone()
                    },
                    self.torrent_dir(&infohash),
                )
            })
            .await?;
        let active = self.activate(&infohash, &entry).await?;

        if active.handle.is_paused() {
            self.unpause(active).await?;
        }
        if entry.stopped.swap(false, Ordering::Relaxed) {
            if let Ok(mut t) = entry.stream_started.lock() {
                *t = Instant::now();
            }
        }
        entry.mark_alive();
        if let Output::Cache(dir) = &entry.output {
            self.touch(dir);
        }

        Ok(StreamSession {
            infohash: infohash.clone(),
            movie_id: entry.req.movie_id,
            stream_url: self.stream_url(&infohash, active.file_idx),
            file_name: active.file_name.clone(),
            file_size_bytes: active.geo.len,
            video_codec: entry.req.video_codec,
            likely_playable: entry.req.video_codec == VideoCodec::X264,
            buffer_target_bytes: self.buffer_target_bytes(),
            resume_at_s: None,
            source: StreamSource::Network,
        })
    }

    /// The entry of `infohash` (created with `make` if missing) with its `control` lock.
    async fn lock_entry(
        &self,
        infohash: &str,
        make: impl Fn() -> Entry,
    ) -> AppResult<(Arc<Entry>, tokio::sync::OwnedMutexGuard<()>)> {
        loop {
            let entry = {
                let mut entries = self
                    .entries
                    .lock()
                    .map_err(|_| AppError::Internal("torrent entries lock poisoned".into()))?;
                Arc::clone(
                    entries
                        .entry(infohash.to_owned())
                        .or_insert_with(|| Arc::new(make())),
                )
            };
            let guard = Arc::clone(&entry.control).lock_owned().await;
            // A previous start may have failed (or a promotion replaced the entry) while
            // we waited.
            if self
                .entry(infohash)
                .is_some_and(|current| Arc::ptr_eq(&current, &entry))
            {
                return Ok((entry, guard));
            }
        }
    }

    /// Resolves the entry's torrent once. A failure drops the entry.
    async fn activate<'a>(&self, infohash: &str, entry: &'a Entry) -> AppResult<&'a Active> {
        match entry.active.get_or_try_init(|| self.resolve(entry)).await {
            Ok(active) => Ok(active),
            Err(e) => {
                entry.set_resolving(None);
                if let Ok(mut entries) = self.entries.lock() {
                    if entries
                        .get(infohash)
                        .is_some_and(|cur| std::ptr::eq(Arc::as_ptr(cur), entry))
                    {
                        entries.remove(infohash);
                    }
                }
                Err(e)
            }
        }
    }

    // -- Downloads ------------------------------------------------------------------------

    /// Adds a torrent as a download into `folder`, or marks the one already in the session.
    /// A torrent that was being streamed stays in `cache/` (`in_cache`) until
    /// [`Self::promote`] can move it without breaking the stream.
    pub async fn add_download(&self, d: DownloadRequest) -> AppResult<DownloadTorrent> {
        let infohash = d.stream.infohash.to_ascii_lowercase();
        if !is_valid_infohash(&infohash) {
            return Err(AppError::InvalidInput(format!(
                "invalid infohash {infohash:?}"
            )));
        }
        let (entry, _control) = self
            .lock_entry(&infohash, || {
                let e = Entry::with_output(
                    StreamRequest {
                        infohash: infohash.clone(),
                        ..d.stream.clone()
                    },
                    Output::Library(d.folder.clone()),
                    d.torrent_bytes.clone(),
                );
                // No stream yet.
                e.stopped.store(true, Ordering::Relaxed);
                e
            })
            .await?;
        entry.set_download(Some(d.run));
        let active = match self.activate(&infohash, &entry).await {
            Ok(active) => active,
            Err(e) => {
                entry.set_download(None);
                return Err(e);
            }
        };
        self.reconcile(&entry).await?;
        download_torrent(active, entry.output.is_cache())
    }

    /// Moves a download that was being streamed from `cache/<infohash>/` into `folder`
    /// without downloading it again: out of the session first (librqbit closes the files),
    /// move the video, then add it back pointing at `folder` (the pieces on disk are
    /// checked, not downloaded). `Ok(false)` while a stream or a reader still uses it.
    pub async fn promote(&self, infohash: &str, folder: &Path) -> AppResult<bool> {
        let infohash = infohash.to_ascii_lowercase();
        let old = self
            .entry(&infohash)
            .ok_or_else(|| AppError::NotFound(format!("no torrent {infohash}")))?;
        let old_control = Arc::clone(&old.control).lock_owned().await;
        let Output::Cache(src_dir) = old.output.clone() else {
            return Ok(true);
        };
        if !old.stopped.load(Ordering::Relaxed) || old.readers.has_open() {
            return Ok(false);
        }
        let Some(active) = old.active.get() else {
            return Ok(false);
        };
        let bytes = active
            .handle
            .with_metadata(|m| m.torrent_bytes.clone())
            .map_err(|e| torrent_err("reading metadata", e))?;
        let new = Arc::new(Entry::with_output(
            old.req.clone(),
            Output::Library(folder.to_path_buf()),
            Some(bytes),
        ));
        new.stopped.store(true, Ordering::Relaxed);
        new.set_download(Some(old.download().unwrap_or(true)));
        // Whoever waits on the old entry finds it replaced and waits on the new one.
        let _control = Arc::clone(&new.control).lock_owned().await;
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(infohash.clone(), Arc::clone(&new));
        }
        drop(old_control);

        let result = async {
            let id = Id20::from_str(&infohash).map_err(|e| torrent_err("parsing infohash", e))?;
            if self.session.get(TorrentIdOrHash::Hash(id)).is_some() {
                self.session
                    .delete(TorrentIdOrHash::Hash(id), false)
                    .await
                    .map_err(|e| torrent_err("removing torrent from session", e))?;
            }
            move_cache_folder(&src_dir, folder).await?;
            self.activate(&infohash, &new).await?;
            self.reconcile(&new).await
        }
        .await;
        if result.is_err() {
            if let Ok(mut entries) = self.entries.lock() {
                if entries
                    .get(&infohash)
                    .is_some_and(|cur| Arc::ptr_eq(cur, &new))
                {
                    entries.remove(&infohash);
                }
            }
        }
        result.map(|()| {
            tracing::info!(%infohash, folder = %folder.display(), "download moved to library");
            true
        })
    }

    /// Whether [`Self::promote`] would move the torrent now (no stream, no reader).
    pub fn can_promote(&self, infohash: &str) -> bool {
        self.entry(&infohash.to_ascii_lowercase()).is_some_and(|e| {
            e.output.is_cache()
                && e.active.get().is_some()
                && e.stopped.load(Ordering::Relaxed)
                && !e.readers.has_open()
        })
    }

    /// Moves everything in `cache/<infohash>/` into `folder` (merging) and deletes the
    /// cache folder. Not only the video: its first and last pieces share bytes with the
    /// neighbouring files, and without them those pieces would fail the check. The torrent
    /// must not be in the session. Returns whether the video (`rel_path`) was there.
    pub async fn move_from_cache(
        &self,
        infohash: &str,
        rel_path: &Path,
        folder: &Path,
    ) -> AppResult<bool> {
        let src_dir = self.torrent_dir(infohash);
        let moved = tokio::fs::symlink_metadata(src_dir.join(rel_path))
            .await
            .is_ok();
        move_cache_folder(&src_dir, folder).await?;
        Ok(moved)
    }

    /// Download state the torrent should follow: running or paused (a stream open on it
    /// keeps it running until it stops).
    pub async fn set_download_running(&self, infohash: &str, run: bool) -> AppResult<()> {
        let Some(entry) = self.entry(&infohash.to_ascii_lowercase()) else {
            return Ok(());
        };
        let _control = Arc::clone(&entry.control).lock_owned().await;
        entry.set_download(Some(run));
        self.reconcile(&entry).await
    }

    /// Pauses and resumes a running torrent: it announces again and retries the known
    /// peers (librqbit does not reconnect to a lost peer by itself).
    pub async fn restart_torrent(&self, infohash: &str) -> AppResult<()> {
        let Some(entry) = self.entry(&infohash.to_ascii_lowercase()) else {
            return Ok(());
        };
        let _control = Arc::clone(&entry.control).lock_owned().await;
        let Some(active) = entry.active.get() else {
            return Ok(());
        };
        if active.handle.is_paused() {
            return Ok(());
        }
        self.session
            .pause(&active.handle)
            .await
            .map_err(|e| torrent_err("pausing torrent", e))?;
        self.reconcile(&entry).await
    }

    /// No longer a download (still in `cache/`): back to a plain stream, paused unless it
    /// is open, and subject to the cache LRU again.
    pub async fn unmark_download(&self, infohash: &str) -> AppResult<()> {
        let Some(entry) = self.entry(&infohash.to_ascii_lowercase()) else {
            return Ok(());
        };
        let _control = Arc::clone(&entry.control).lock_owned().await;
        entry.set_download(None);
        self.reconcile(&entry).await
    }

    /// Takes the torrent out of the session (closing its files) even if it is being
    /// streamed, and forgets its library stream. Unknown infohash: no-op.
    pub async fn remove_torrent(&self, infohash: &str) -> AppResult<()> {
        let infohash = infohash.to_ascii_lowercase();
        self.unregister_local(&infohash);
        let entry = self.entry(&infohash);
        let _control = match &entry {
            Some(e) => Some(Arc::clone(&e.control).lock_owned().await),
            None => None,
        };
        if let (Some(e), Ok(mut entries)) = (&entry, self.entries.lock()) {
            if entries
                .get(&infohash)
                .is_some_and(|cur| Arc::ptr_eq(cur, e))
            {
                entries.remove(&infohash);
            }
        }
        let id = Id20::from_str(&infohash).map_err(|e| torrent_err("parsing infohash", e))?;
        if self.session.get(TorrentIdOrHash::Hash(id)).is_some() {
            self.session
                .delete(TorrentIdOrHash::Hash(id), false)
                .await
                .map_err(|e| torrent_err("removing torrent from session", e))?;
            tracing::info!(%infohash, "torrent removed from session");
        }
        Ok(())
    }

    /// A stream is open on the torrent or something is reading it.
    pub fn is_streaming(&self, infohash: &str) -> bool {
        self.entry(&infohash.to_ascii_lowercase())
            .is_some_and(|e| !e.stopped.load(Ordering::Relaxed) || e.readers.has_open())
    }

    /// Whether the torrent is in the engine (stream or download).
    pub fn has_torrent(&self, infohash: &str) -> bool {
        self.entry(&infohash.to_ascii_lowercase()).is_some()
    }

    /// Progress of a download's torrent; `None` if it is not in the engine.
    pub fn download_stats(&self, infohash: &str) -> Option<DownloadStats> {
        let entry = self.entry(&infohash.to_ascii_lowercase())?;
        let Some(active) = entry.active.get() else {
            return Some(DownloadStats::default());
        };
        let stats = active.handle.stats();
        let (peers, down_bps) = stats
            .live
            .as_ref()
            .map(|l| {
                (
                    l.snapshot.peer_stats.live,
                    mib_to_bytes(l.download_speed.mbps),
                )
            })
            .unwrap_or((0, 0));
        let len = active.geo.len;
        let downloaded = stats
            .file_progress
            .get(active.file_idx)
            .copied()
            .unwrap_or(0)
            .min(len);
        let paused = active.handle.is_paused();
        let initializing = matches!(stats.state, TorrentStatsState::Initializing { .. });
        if paused || initializing || downloaded >= len || (peers > 0 && down_bps > 0) {
            entry.mark_alive();
        }
        Some(DownloadStats {
            resolved: true,
            file_len: len,
            downloaded,
            down_bps,
            peers,
            stalled: entry.idle_for() >= self.cfg.stall_after,
            error: matches!(stats.state, TorrentStatsState::Error)
                .then(|| stats.error.unwrap_or_else(|| "torrent error".into())),
        })
    }

    /// Serves a finished download straight from disk (no torrent, no network).
    pub async fn serve_local(&self, s: LocalStream) -> AppResult<StreamSession> {
        let infohash = s.infohash.to_ascii_lowercase();
        let meta = tokio::fs::metadata(&s.path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::NotFound(format!("{} is missing", s.path.display()))
            } else {
                e.into()
            }
        })?;
        let name = s
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Ok(mut local) = self.local.lock() {
            let readers = local
                .get(&infohash)
                .filter(|l| l.path == s.path)
                .map(|l| Arc::clone(&l.readers))
                .unwrap_or_default();
            local.insert(
                infohash.clone(),
                LocalFile {
                    movie_id: s.movie_id,
                    path: s.path.clone(),
                    len: meta.len(),
                    name: name.clone(),
                    readers,
                },
            );
        }
        tracing::info!(%infohash, path = %s.path.display(), "playing from library");
        Ok(StreamSession {
            stream_url: self.stream_url(&infohash, LOCAL_FILE_IDX),
            infohash,
            movie_id: s.movie_id,
            file_name: name,
            file_size_bytes: meta.len(),
            video_codec: s.video_codec,
            likely_playable: s.video_codec == VideoCodec::X264,
            buffer_target_bytes: self.buffer_target_bytes(),
            resume_at_s: None,
            source: StreamSource::Library,
        })
    }

    pub fn unregister_local(&self, infohash: &str) {
        if let Ok(mut local) = self.local.lock() {
            local.remove(&infohash.to_ascii_lowercase());
        }
    }

    /// Whether a library stream of `infohash` has a reader open.
    pub fn local_in_use(&self, infohash: &str) -> bool {
        self.local
            .lock()
            .ok()
            .and_then(|l| {
                l.get(&infohash.to_ascii_lowercase())
                    .map(|f| f.readers.has_open())
            })
            .unwrap_or(false)
    }

    fn local_file(
        &self,
        infohash: &str,
        file_idx: usize,
    ) -> Option<(PathBuf, u64, String, Arc<Readers>)> {
        if file_idx != LOCAL_FILE_IDX {
            return None;
        }
        let local = self.local.lock().ok()?;
        let f = local.get(infohash)?;
        Some((
            f.path.clone(),
            f.len,
            f.name.clone(),
            Arc::clone(&f.readers),
        ))
    }

    /// Session-wide speed limits, applied immediately (`None` = unlimited).
    pub fn set_rate_limits(&self, down_bps: Option<NonZeroU32>, up_bps: Option<NonZeroU32>) {
        self.session.ratelimits.set_download_bps(down_bps);
        self.session.ratelimits.set_upload_bps(up_bps);
        tracing::info!(?down_bps, ?up_bps, "speed limits applied");
    }

    pub fn rate_limits(&self) -> (Option<NonZeroU32>, Option<NonZeroU32>) {
        (
            self.session.ratelimits.get_download_bps(),
            self.session.ratelimits.get_upload_bps(),
        )
    }

    fn stream_url(&self, infohash: &str, file_idx: usize) -> String {
        format!("{}/stream/{infohash}/{file_idx}", self.cfg.local_base)
    }

    /// Pauses a streaming torrent. Files stay in `cache/`. Unknown infohash: no-op.
    pub async fn stop_stream(&self, infohash: &str) -> AppResult<()> {
        let Some(entry) = self.entry(&infohash.to_ascii_lowercase()) else {
            return Ok(());
        };
        let _control = Arc::clone(&entry.control).lock_owned().await;
        entry.stopped.store(true, Ordering::Relaxed);
        if let Output::Cache(dir) = &entry.output {
            self.touch(dir);
        }
        // A torrent that is also a running download keeps going.
        self.reconcile(&entry).await
    }

    /// Resumes a paused torrent, first requeueing every missing piece of its video.
    ///
    /// librqbit 9.0.1 loses a piece when the torrent is paused while that piece's hash is
    /// being checked: it has already left `inflight` (which `pause` requeues) and the
    /// result can't be recorded any more ("chunk tracker empty, torrent was paused"). The
    /// piece is then neither had nor queued, nobody asks for it again and the download
    /// sits at the last piece forever, even with peers connected. Deselecting and
    /// reselecting the file while paused requeues every selected piece we don't have
    /// (and clears its chunk state); only the chunk tracker changes, no file is touched.
    async fn unpause(&self, active: &Active) -> AppResult<()> {
        let video: HashSet<usize> = [active.file_idx].into();
        let requeue = async {
            self.session
                .update_only_files(&active.handle, &HashSet::new())
                .await?;
            self.session.update_only_files(&active.handle, &video).await
        };
        if let Err(e) = requeue.await {
            // Not fatal: the file selection is put back below and resuming still works.
            tracing::warn!(error = %e, "could not requeue missing pieces");
            let _ = self.session.update_only_files(&active.handle, &video).await;
        }
        self.session
            .unpause(&active.handle)
            .await
            .map_err(|e| torrent_err("resuming torrent", e))
    }

    /// Runs or pauses the torrent: it runs while a stream is open or while its download
    /// wants it. Called with the entry's `control` lock held.
    async fn reconcile(&self, entry: &Entry) -> AppResult<()> {
        let Some(active) = entry.active.get() else {
            return Ok(());
        };
        let want = !entry.stopped.load(Ordering::Relaxed) || entry.download() == Some(true);
        let paused = active.handle.is_paused();
        if want && paused {
            self.unpause(active).await?;
            entry.mark_alive();
        } else if !want && !paused {
            self.session
                .pause(&active.handle)
                .await
                .map_err(|e| torrent_err("pausing torrent", e))?;
        }
        Ok(())
    }

    pub fn is_paused(&self, infohash: &str) -> Option<bool> {
        let entry = self.entry(&infohash.to_ascii_lowercase())?;
        entry.active.get().map(|a| a.handle.is_paused())
    }

    /// Movie of an active session (for the external player's automatic subtitles).
    pub fn session_movie_id(&self, infohash: &str) -> Option<u64> {
        let infohash = infohash.to_ascii_lowercase();
        let local = self
            .local
            .lock()
            .ok()
            .and_then(|l| l.get(&infohash).map(|f| f.movie_id));
        local.or_else(|| self.entry(&infohash).map(|e| e.req.movie_id))
    }

    /// Stream URL of an active session (for the external player).
    pub fn session_url(&self, infohash: &str) -> AppResult<String> {
        let infohash = infohash.to_ascii_lowercase();
        if self.local_file(&infohash, LOCAL_FILE_IDX).is_some() {
            return Ok(self.stream_url(&infohash, LOCAL_FILE_IDX));
        }
        let entry = self
            .entry(&infohash)
            .ok_or_else(|| AppError::NotFound(format!("no stream for {infohash}")))?;
        let active = entry
            .active
            .get()
            .ok_or_else(|| AppError::NotFound(format!("stream {infohash} not ready")))?;
        Ok(self.stream_url(&infohash, active.file_idx))
    }

    async fn resolve(&self, entry: &Entry) -> AppResult<Active> {
        let infohash = &entry.req.infohash;
        let id = Id20::from_str(infohash).map_err(|e| torrent_err("parsing infohash", e))?;

        let handle = if let Some(handle) = self.session.get(TorrentIdOrHash::Hash(id)) {
            handle
        } else {
            self.add(entry, id).await?
        };
        entry.set_resolving(None);

        tokio::time::timeout(self.cfg.init_timeout, handle.wait_until_initialized())
            .await
            .map_err(|_| AppError::Torrent("timed out checking existing files".into()))?
            .map_err(|e| torrent_err("initializing torrent", e))?;

        let file_idx = match handle.only_files().as_deref() {
            Some([idx]) => *idx,
            _ => handle
                .with_metadata(|m| {
                    let files: Vec<(PathBuf, u64)> = m
                        .file_infos
                        .iter()
                        .map(|f| (f.relative_filename.clone(), f.len))
                        .collect();
                    pick_video_file(&files)
                })
                .ok()
                .flatten()
                .ok_or_else(|| AppError::Torrent("torrent has no files".into()))?,
        };
        let (geo, file_name) = handle
            .with_metadata(|m| {
                let fi = m.file_infos.get(file_idx)?;
                Some((
                    FileGeometry {
                        offset: fi.offset_in_torrent,
                        len: fi.len,
                        piece_len: u64::from(m.lengths().default_piece_length()),
                    },
                    fi.relative_filename
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ))
            })
            .ok()
            .flatten()
            .ok_or_else(|| AppError::Torrent(format!("file {file_idx} not in torrent")))?;
        let rel_path = handle
            .with_metadata(|m| {
                m.file_infos
                    .get(file_idx)
                    .map(|f| f.relative_filename.clone())
            })
            .ok()
            .flatten()
            .unwrap_or_else(|| PathBuf::from(&file_name));
        tracing::info!(%infohash, file_idx, %file_name, len = geo.len, "torrent ready");
        Ok(Active {
            handle,
            file_idx,
            file_name,
            rel_path,
            geo,
        })
    }

    /// Adds the torrent selecting only the video file.
    async fn add(&self, entry: &Entry, id: Id20) -> AppResult<Arc<ManagedTorrent>> {
        let req = &entry.req;
        let list_opts = || AddTorrentOptions {
            list_only: true,
            initial_peers: Some(self.cfg.initial_peers.clone()),
            ..Default::default()
        };

        // 1) Known .torrent, else the one from YTS (no metadata wait).
        let mut listed = None;
        let bytes = match entry.torrent_bytes.clone() {
            Some(bytes) => Some(bytes),
            None => self.download_torrent_file(entry).await,
        };
        if let Some(bytes) = bytes {
            match self
                .session
                .add_torrent(AddTorrent::from_bytes(bytes), Some(list_opts()))
                .await
            {
                Ok(AddTorrentResponse::ListOnly(l)) if l.info_hash == id => listed = Some(l),
                Ok(AddTorrentResponse::ListOnly(l)) => tracing::warn!(
                    expected = %req.infohash,
                    got = %l.info_hash.as_string(),
                    "downloaded .torrent has a different infohash"
                ),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "invalid .torrent, falling back to magnet"),
            }
        }

        // 2) Magnet: resolve metadata from peers (DHT, trackers).
        let listed = match listed {
            Some(l) => l,
            None => {
                entry.set_resolving(Some(Resolving::Magnet));
                let uri = magnet(&req.infohash, &req.title, &self.cfg.trackers);
                let resp = tokio::time::timeout(
                    self.cfg.metadata_timeout,
                    self.session
                        .add_torrent(AddTorrent::from_url(uri), Some(list_opts())),
                )
                .await
                .map_err(|_| {
                    AppError::NoPeers(format!(
                        "no metadata for {} after {:?}",
                        req.infohash, self.cfg.metadata_timeout
                    ))
                })?
                .map_err(|e| {
                    // Every known peer failed and there is no DHT/tracker left to ask.
                    if format!("{e:#}").contains("address stream exhausted") {
                        AppError::NoPeers(format!("no peers for {}: {e:#}", req.infohash))
                    } else {
                        torrent_err("resolving magnet", e)
                    }
                })?;
                match resp {
                    AddTorrentResponse::ListOnly(l) => l,
                    _ => return Err(AppError::Torrent("unexpected add response".into())),
                }
            }
        };

        let files: Vec<(PathBuf, u64)> = listed
            .info
            .iter_file_details()
            .map(|fd| (fd.filename.to_pathbuf(), fd.len))
            .collect();
        let file_idx = pick_video_file(&files)
            .ok_or_else(|| AppError::Torrent("torrent has no files".into()))?;

        let mut peers = listed.seen_peers.clone();
        peers.extend(self.cfg.initial_peers.iter().copied());
        let (output_folder, sub_folder) = match &entry.output {
            Output::Cache(dir) | Output::Library(dir) => {
                (Some(dir.to_string_lossy().into_owned()), None::<String>)
            }
        };
        // A download added paused (restored after a restart) still checks its pieces.
        let paused = entry.stopped.load(Ordering::Relaxed) && entry.download() == Some(false);
        let resp = self
            .session
            .add_torrent(
                AddTorrent::from_bytes(listed.torrent_bytes.clone()),
                Some(AddTorrentOptions {
                    paused,
                    only_files: Some(vec![file_idx]),
                    output_folder,
                    sub_folder,
                    overwrite: true,
                    initial_peers: Some(peers),
                    trackers: Some(self.cfg.trackers.clone()),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|e| torrent_err("adding torrent", e))?;
        resp.into_handle()
            .ok_or_else(|| AppError::Torrent("torrent was not added".into()))
    }

    async fn download_torrent_file(&self, entry: &Entry) -> Option<Bytes> {
        let url = Url::parse(entry.req.torrent_url.as_deref()?).ok()?;
        if !host_allowed(&self.allowed_hosts, &url) {
            tracing::warn!(%url, "torrent URL host not allowed");
            return None;
        }
        entry.set_resolving(Some(Resolving::TorrentFile));
        let result = async {
            let resp = self
                .http
                .get(url.clone())
                .send()
                .await?
                .error_for_status()?;
            resp.bytes().await
        }
        .await;
        match result {
            Ok(bytes) => Some(bytes),
            Err(e) => {
                tracing::warn!(%url, error = %e, "could not download .torrent, using magnet");
                None
            }
        }
    }

    /// Opens a reader over the streamed file, positioned at `start`.
    pub async fn open_reader(
        &self,
        infohash: &str,
        file_idx: usize,
        start: u64,
    ) -> AppResult<(TrackedReader, String)> {
        let infohash = infohash.to_ascii_lowercase();
        if let Some((path, _, name, readers)) = self.local_file(&infohash, file_idx) {
            let mut file = tokio::fs::File::open(&path).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    AppError::NotFound(format!("{} is missing", path.display()))
                } else {
                    e.into()
                }
            })?;
            if start > 0 {
                tokio::io::AsyncSeekExt::seek(&mut file, SeekFrom::Start(start)).await?;
            }
            let id = readers.open(start);
            return Ok((
                TrackedReader {
                    inner: Box::pin(file),
                    readers,
                    id,
                    position: start,
                },
                name,
            ));
        }
        let entry = self
            .entry(&infohash)
            .ok_or_else(|| AppError::NotFound(format!("no stream for {infohash}")))?;
        let active = entry
            .active
            .get()
            .filter(|a| a.file_idx == file_idx)
            .ok_or_else(|| AppError::NotFound(format!("{infohash}/{file_idx}")))?;
        let mut stream = Arc::clone(&active.handle)
            .stream(file_idx)
            .await
            .map_err(|e| torrent_err("opening file stream", e))?;
        if start > 0 {
            tokio::io::AsyncSeekExt::seek(&mut stream, SeekFrom::Start(start)).await?;
        }
        let readers = Arc::clone(&entry.readers);
        let id = readers.open(start);
        Ok((
            TrackedReader {
                inner: Box::pin(stream),
                readers,
                id,
                position: start,
            },
            active.file_name.clone(),
        ))
    }

    /// File length of the active stream `infohash/file_idx`.
    pub fn file_len(&self, infohash: &str, file_idx: usize) -> Option<u64> {
        let infohash = infohash.to_ascii_lowercase();
        if let Some((_, len, _, _)) = self.local_file(&infohash, file_idx) {
            return Some(len);
        }
        let entry = self.entry(&infohash)?;
        let active = entry.active.get().filter(|a| a.file_idx == file_idx)?;
        Some(active.geo.len)
    }

    fn stats_for(&self, entry: &Entry) -> TorrentStats {
        let req = &entry.req;
        let resolving = entry.resolving.lock().ok().and_then(|g| *g);
        let Some(active) = entry.active.get() else {
            return TorrentStats {
                infohash: req.infohash.clone(),
                phase: compute_phase(&PhaseInput {
                    resolving: Some(resolving.unwrap_or(Resolving::TorrentFile)),
                    peers: 0,
                    downloaded: 0,
                    file_len: 0,
                    position: 0,
                    buffered_ahead: 0,
                    buffer_target: self.buffer_target_bytes(),
                    idle_for: entry.started.elapsed(),
                    stall_after: self.cfg.stall_after,
                    no_peers: entry.no_peers(self.cfg.no_peers_after),
                }),
                peers: 0,
                seeds: req.seeds,
                down_speed_bps: 0,
                up_speed_bps: 0,
                progress: 0.0,
                downloaded_bytes: 0,
                buffered_ahead_bytes: 0,
                available_ranges: Vec::new(),
                piece_map: None,
                piece_map_window: None,
            };
        };

        let stats = active.handle.stats();
        let (peers, down, up) = stats
            .live
            .as_ref()
            .map(|l| {
                (
                    l.snapshot.peer_stats.live,
                    mib_to_bytes(l.download_speed.mbps),
                    mib_to_bytes(l.upload_speed.mbps),
                )
            })
            .unwrap_or((0, 0, 0));
        let geo = active.geo;
        let downloaded = stats
            .file_progress
            .get(active.file_idx)
            .copied()
            .unwrap_or(0)
            .min(geo.len);
        let have: Vec<bool> = self
            .api
            .api_dump_haves(TorrentIdOrHash::Hash(active.handle.info_hash()))
            .map(|(bf, total)| (0..total as usize).map(|i| bf[i]).collect())
            .unwrap_or_default();

        let (positions, current) = entry.readers.snapshot();
        // Before the first read: resumeAtS (see `hint_start_fraction`) → byte 0.
        let position = current.unwrap_or(0);
        let buffered = buffered_ahead(&geo, &have, position);
        let complete = geo.len > 0 && downloaded >= geo.len;
        if complete || (peers > 0 && down > 0) {
            entry.mark_alive();
        }
        if peers > 0 {
            entry.ever_connected.store(true, Ordering::Relaxed);
        }
        if matches!(stats.state, TorrentStatsState::Error) {
            let message = stats
                .error
                .clone()
                .unwrap_or_else(|| "torrent error".into());
            let first = self
                .reported_errors
                .lock()
                .map(|mut r| r.insert(req.infohash.clone()))
                .unwrap_or(false);
            if first {
                tracing::warn!(infohash = %req.infohash, error = %message, "torrent error");
                let _ = self.errors_tx.send(BackgroundError::new(
                    &background_error(&message),
                    Some(req.infohash.clone()),
                ));
            }
        } else if let Ok(mut r) = self.reported_errors.lock() {
            r.remove(&req.infohash);
        }
        let phase = compute_phase(&PhaseInput {
            resolving: None,
            peers,
            downloaded,
            file_len: geo.len,
            position,
            buffered_ahead: buffered,
            buffer_target: self.buffer_target_bytes(),
            idle_for: entry.idle_for(),
            stall_after: self.cfg.stall_after,
            no_peers: entry.no_peers(self.cfg.no_peers_after),
        });
        // Reported for every active (not stopped) stream session.
        let window = piece_map_window(geo.len, position, PIECE_MAP_WINDOW);
        let piece_map = piece_map(
            &geo,
            &have,
            &priority_pieces(&geo, &positions),
            window,
            PIECE_MAP_CELLS,
        );
        TorrentStats {
            infohash: req.infohash.clone(),
            phase,
            peers,
            seeds: req.seeds,
            down_speed_bps: down,
            up_speed_bps: up,
            progress: if geo.len == 0 {
                0.0
            } else {
                downloaded as f64 / geo.len as f64
            },
            downloaded_bytes: downloaded,
            buffered_ahead_bytes: buffered,
            available_ranges: available_ranges(&geo, &have),
            piece_map: Some(piece_map),
            piece_map_window: Some(PieceMapWindow {
                start_byte: window.0,
                end_byte: window.1,
            }),
        }
    }

    fn emit_stats(&self) {
        let entries: Vec<Arc<Entry>> = match self.entries.lock() {
            Ok(e) => e.values().cloned().collect(),
            Err(_) => return,
        };
        for entry in entries {
            if entry.stopped.load(Ordering::Relaxed) {
                continue;
            }
            // No receivers is fine (nobody listening yet).
            let _ = self.stats_tx.send(self.stats_for(&entry));
        }
    }
}

fn download_torrent(active: &Active, in_cache: bool) -> AppResult<DownloadTorrent> {
    let torrent_bytes = active
        .handle
        .with_metadata(|m| m.torrent_bytes.clone())
        .map_err(|e| torrent_err("reading metadata", e))?;
    Ok(DownloadTorrent {
        rel_path: active.rel_path.clone(),
        file_len: active.geo.len,
        torrent_bytes,
        in_cache,
    })
}

/// A failure librqbit reports as text, with the code the front should show: a full disk
/// or a permission problem is `io`, anything else `torrent`.
pub fn background_error(message: &str) -> AppError {
    let lower = message.to_ascii_lowercase();
    let io_kind = if lower.contains("no space left") || lower.contains("os error 28") {
        Some(std::io::ErrorKind::StorageFull)
    } else if lower.contains("permission denied") || lower.contains("os error 13") {
        Some(std::io::ErrorKind::PermissionDenied)
    } else if lower.contains("read-only file system") || lower.contains("os error 30") {
        Some(std::io::ErrorKind::ReadOnlyFilesystem)
    } else {
        None
    };
    match io_kind {
        Some(kind) => AppError::Io(std::io::Error::new(kind, message.to_owned())),
        None => AppError::Torrent(message.to_owned()),
    }
}

/// `KB/s` setting (KiB/s, as the UI shows it) → bytes/s for librqbit. `None` = unlimited.
pub fn kbps_to_bps(kbps: Option<u32>) -> Option<NonZeroU32> {
    NonZeroU32::new(kbps?.saturating_mul(1024))
}

fn mib_to_bytes(mib_per_s: f64) -> u64 {
    (mib_per_s * 1024.0 * 1024.0).max(0.0) as u64
}

fn spawn_stats_loop(engine: Weak<TorrentEngine>, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let Some(engine) = engine.upgrade() else {
                break;
            };
            engine.emit_stats();
        }
    });
}

/// File stream that reports its read position (for `bufferedAheadBytes` and `pieceMap`).
pub struct TrackedReader {
    inner: Pin<Box<dyn AsyncReadSeek + Send>>,
    readers: Arc<Readers>,
    id: u64,
    position: u64,
}

trait AsyncReadSeek: AsyncRead + AsyncSeek {}
impl<T: AsyncRead + AsyncSeek> AsyncReadSeek for T {}

impl AsyncRead for TrackedReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let poll = self.inner.as_mut().poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = poll {
            let read = (buf.filled().len() - before) as u64;
            if read > 0 {
                self.position += read;
                self.readers.update(self.id, self.position);
            }
        }
        poll
    }
}

impl Drop for TrackedReader {
    fn drop(&mut self) {
        self.readers.close(self.id);
    }
}

/// Moves a file, creating the destination folder. Falls back to copy + delete when a
/// rename is not possible (another filesystem). A file locked for a moment (Windows) is
/// retried first.
async fn move_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if retry_locked(|| tokio::fs::rename(src, dst)).await.is_ok() {
        return Ok(());
    }
    tokio::fs::copy(src, dst).await?;
    remove_path(src).await
}

/// Moves a torrent folder from the cache into `folder` (merging) and deletes it.
async fn move_cache_folder(src_dir: &Path, folder: &Path) -> std::io::Result<()> {
    if tokio::fs::symlink_metadata(src_dir).await.is_ok() {
        move_tree(src_dir, folder).await?;
    }
    remove_path(src_dir).await
}

/// Moves the contents of `src` into `dst` (created if needed), file by file.
async fn move_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    let mut stack = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((from, to)) = stack.pop() {
        tokio::fs::create_dir_all(&to).await?;
        let mut rd = tokio::fs::read_dir(&from).await?;
        while let Some(item) = rd.next_entry().await? {
            let target = to.join(item.file_name());
            if item.file_type().await?.is_dir() {
                stack.push((item.path(), target));
            } else {
                move_file(&item.path(), &target).await?;
            }
        }
    }
    Ok(())
}

/// Deletes a file or a folder; missing is fine. On Windows a file still open elsewhere
/// (player, antivirus, indexer) is retried for a few seconds before giving up; the cache
/// cleanup then skips it and tries again on its next pass.
pub async fn remove_path(path: &Path) -> std::io::Result<()> {
    retry_locked(|| remove_path_once(path)).await
}

async fn remove_path_once(path: &Path) -> std::io::Result<()> {
    let result = match tokio::fs::symlink_metadata(path).await {
        Ok(m) if m.is_dir() => tokio::fs::remove_dir_all(path).await,
        Ok(_) => tokio::fs::remove_file(path).await,
        Err(e) => Err(e),
    };
    match result {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "e9cc8cb56c01edcb7e324a5b57a7b5d04520dd48";

    #[test]
    fn magnet_has_hash_name_and_trackers() {
        let trackers: Vec<String> = TRACKERS[..2].iter().map(|s| s.to_string()).collect();
        let m = magnet(HASH, "The Matrix (1999)", &trackers);
        assert_eq!(
            m,
            format!(
                "magnet:?xt=urn:btih:{HASH}&dn=The+Matrix+%281999%29\
                 &tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce\
                 &tr=udp%3A%2F%2Ftracker.leechers-paradise.org%3A6969%2Fannounce"
            )
        );
        let parsed = librqbit::Magnet::parse(&m).unwrap();
        assert_eq!(parsed.as_id20().unwrap().as_string(), HASH);
        assert_eq!(parsed.trackers.len(), 2);
        assert_eq!(magnet(HASH, "", &[]), format!("magnet:?xt=urn:btih:{HASH}"));
    }

    #[test]
    fn picks_largest_video_file() {
        let files = vec![
            (PathBuf::from("YTS.txt"), 900),
            (PathBuf::from("Movie.2021.1080p.mp4"), 500),
            (PathBuf::from("www.YTS.jpg"), 800),
            (PathBuf::from("sample.mkv"), 100),
        ];
        assert_eq!(pick_video_file(&files), Some(1));
        let no_video = vec![(PathBuf::from("a.txt"), 1), (PathBuf::from("b.bin"), 9)];
        assert_eq!(pick_video_file(&no_video), Some(1));
        assert_eq!(pick_video_file(&[]), None);
    }

    #[test]
    fn mime_by_extension() {
        assert_eq!(video_mime("a.MP4"), "video/mp4");
        assert_eq!(video_mime("a.mkv"), "video/x-matroska");
        assert_eq!(video_mime("noext"), "application/octet-stream");
    }

    /// File of 1000 bytes starting at byte 50 of the torrent, pieces of 100 bytes:
    /// covers pieces 0..=10 (piece 0 is shared with a previous file).
    const GEO: FileGeometry = FileGeometry {
        offset: 50,
        len: 1000,
        piece_len: 100,
    };

    fn have(pieces: &[usize]) -> Vec<bool> {
        let mut v = vec![false; 11];
        for &p in pieces {
            v[p] = true;
        }
        v
    }

    #[test]
    fn available_ranges_are_fractions_of_the_file() {
        assert!(available_ranges(&GEO, &have(&[])).is_empty());
        assert_eq!(
            available_ranges(&GEO, &have(&(0..=10).collect::<Vec<_>>())),
            vec![(0.0, 1.0)]
        );
        // Piece 0 covers file bytes [0, 50); pieces 1-2 → [50, 250); piece 10 → [950, 1000).
        assert_eq!(
            available_ranges(&GEO, &have(&[0, 1, 2, 5, 10])),
            vec![(0.0, 0.25), (0.45, 0.55), (0.95, 1.0)]
        );
    }

    #[test]
    fn buffered_ahead_counts_contiguous_bytes_from_position() {
        let h = have(&[0, 1, 2, 5]);
        assert_eq!(buffered_ahead(&GEO, &h, 0), 250);
        assert_eq!(buffered_ahead(&GEO, &h, 100), 150);
        assert_eq!(buffered_ahead(&GEO, &h, 300), 0);
        assert_eq!(buffered_ahead(&GEO, &h, 460), 90);
        assert_eq!(buffered_ahead(&GEO, &h, 1000), 0);
        let all = have(&(0..=10).collect::<Vec<_>>());
        assert_eq!(buffered_ahead(&GEO, &all, 10), 990);
    }

    #[test]
    fn piece_map_marks_ready_priority_and_missing() {
        let geo = FileGeometry {
            offset: 0,
            len: 1000,
            piece_len: 100,
        };
        let h = vec![
            true, true, false, false, false, false, false, false, false, true,
        ];
        let priority: HashSet<usize> = [2, 3].into_iter().collect();
        let whole = (0, 1000);
        assert_eq!(piece_map(&geo, &h, &priority, whole, 10), "1122000001");
        // Coarser map: a cell is "1" only if all its pieces are downloaded.
        assert_eq!(piece_map(&geo, &h, &priority, whole, 5), "12000");
        assert_eq!(
            piece_map(&geo, &h, &priority, whole, PIECE_MAP_CELLS).len(),
            PIECE_MAP_CELLS
        );
        // Window [100, 500): pieces 1..=4.
        assert_eq!(piece_map(&geo, &h, &priority, (100, 500), 4), "1220");
        // Window past the end of the file is clipped.
        assert_eq!(piece_map(&geo, &h, &priority, (800, 5000), 2), "01");
        // Tiny file, more cells than bytes.
        let tiny = FileGeometry {
            offset: 0,
            len: 3,
            piece_len: 100,
        };
        assert_eq!(
            piece_map(&tiny, &[true], &HashSet::new(), (0, 3), 6),
            "111111"
        );
    }

    #[test]
    fn piece_map_window_starts_at_read_position() {
        const MB: u64 = 1024 * 1024;
        let len = 2000 * MB;
        assert_eq!(piece_map_window(len, 0, PIECE_MAP_WINDOW), (0, 64 * MB));
        assert_eq!(
            piece_map_window(len, 500 * MB, PIECE_MAP_WINDOW),
            (500 * MB, 564 * MB)
        );
        // Near the end the window is clipped to the file.
        assert_eq!(
            piece_map_window(len, 1990 * MB, PIECE_MAP_WINDOW),
            (1990 * MB, len)
        );
        assert_eq!(piece_map_window(len, len, PIECE_MAP_WINDOW), (len - 1, len));
        // Files smaller than the window: the whole file.
        assert_eq!(
            piece_map_window(10 * MB, 5 * MB, PIECE_MAP_WINDOW),
            (0, 10 * MB)
        );
        assert_eq!(piece_map_window(0, 0, PIECE_MAP_WINDOW), (0, 0));
    }

    #[test]
    fn priority_window_follows_readers() {
        let geo = FileGeometry {
            offset: 0,
            len: 100 * 1024 * 1024,
            piece_len: 1024 * 1024,
        };
        let p = priority_pieces(&geo, &[0]);
        assert_eq!(p.len(), 32);
        assert!(p.contains(&0) && p.contains(&31) && !p.contains(&32));
        // A reader near the end (MP4 moov atom) gets its own window, clipped to the file.
        let p = priority_pieces(&geo, &[0, 99 * 1024 * 1024 + 10]);
        assert!(p.contains(&99) && p.len() == 33);
        assert!(priority_pieces(&geo, &[geo.len]).is_empty());
    }

    fn input() -> PhaseInput {
        PhaseInput {
            resolving: None,
            peers: 3,
            downloaded: 1000,
            file_len: 10_000,
            position: 0,
            buffered_ahead: 0,
            buffer_target: 500,
            idle_for: Duration::ZERO,
            stall_after: Duration::from_secs(30),
            no_peers: false,
        }
    }

    #[test]
    fn phases() {
        let i = input();
        assert_eq!(
            compute_phase(&PhaseInput {
                resolving: Some(Resolving::Magnet),
                ..i
            }),
            StreamPhase::Metadata
        );
        assert_eq!(
            compute_phase(&PhaseInput {
                resolving: Some(Resolving::TorrentFile),
                ..i
            }),
            StreamPhase::Connecting
        );
        assert_eq!(
            compute_phase(&PhaseInput {
                peers: 0,
                downloaded: 0,
                ..i
            }),
            StreamPhase::Connecting
        );
        assert_eq!(compute_phase(&i), StreamPhase::Buffering);
        assert_eq!(
            compute_phase(&PhaseInput {
                buffered_ahead: 500,
                ..i
            }),
            StreamPhase::Ready
        );
        // Near the end, the remaining bytes are enough.
        assert_eq!(
            compute_phase(&PhaseInput {
                position: 9_900,
                buffered_ahead: 100,
                ..i
            }),
            StreamPhase::Ready
        );
        assert_eq!(
            compute_phase(&PhaseInput {
                position: 10_000,
                buffered_ahead: 0,
                ..i
            }),
            StreamPhase::Ready
        );
        assert_eq!(
            compute_phase(&PhaseInput {
                idle_for: Duration::from_secs(31),
                ..i
            }),
            StreamPhase::Stalled
        );
        assert_eq!(
            compute_phase(&PhaseInput {
                downloaded: 10_000,
                idle_for: Duration::from_secs(99),
                ..i
            }),
            StreamPhase::Done
        );
    }

    #[test]
    fn no_peers_unless_ready_or_done() {
        let i = PhaseInput {
            no_peers: true,
            ..input()
        };
        for (resolving, peers, downloaded) in [
            (Some(Resolving::Magnet), 0, 0),
            (Some(Resolving::TorrentFile), 0, 0),
            (None, 0, 0),
            (None, 0, 1000),
        ] {
            let p = PhaseInput {
                resolving,
                peers,
                downloaded,
                ..i
            };
            assert_eq!(compute_phase(&p), StreamPhase::NoPeers, "{p:?}");
        }
        // Stalled turns into no_peers too.
        assert_eq!(
            compute_phase(&PhaseInput {
                idle_for: Duration::from_secs(99),
                ..i
            }),
            StreamPhase::NoPeers
        );
        // What is on disk is enough to play: not shown.
        assert_eq!(
            compute_phase(&PhaseInput {
                buffered_ahead: 500,
                ..i
            }),
            StreamPhase::Ready
        );
        assert_eq!(
            compute_phase(&PhaseInput {
                downloaded: 10_000,
                ..i
            }),
            StreamPhase::Done
        );
    }

    #[test]
    fn range_parsing() {
        let len = 1000;
        assert_eq!(parse_range(None, len), Ok(None));
        assert_eq!(parse_range(Some("bytes=0-99"), len), Ok(Some((0, 99))));
        assert_eq!(parse_range(Some("bytes=500-"), len), Ok(Some((500, 999))));
        assert_eq!(parse_range(Some("bytes=-100"), len), Ok(Some((900, 999))));
        assert_eq!(parse_range(Some("bytes=-5000"), len), Ok(Some((0, 999))));
        assert_eq!(
            parse_range(Some("bytes=900-5000"), len),
            Ok(Some((900, 999)))
        );
        assert_eq!(parse_range(Some("bytes=0-0, 10-20"), len), Ok(Some((0, 0))));
        assert_eq!(parse_range(Some(" bytes= 1 - 2 "), len), Ok(Some((1, 2))));
        for bad in [
            "bytes=1000-",
            "bytes=5-1",
            "bytes=-0",
            "bytes=abc",
            "bytes=-",
            "items=0-1",
            "bytes=a-b",
            "",
        ] {
            assert_eq!(parse_range(Some(bad), len), Err(RangeError), "{bad:?}");
        }
        assert_eq!(parse_range(Some("bytes=0-1"), 0), Err(RangeError));
    }

    #[test]
    fn infohash_validation() {
        assert!(is_valid_infohash(HASH));
        assert!(!is_valid_infohash("xyz"));
        assert!(!is_valid_infohash(&"g".repeat(40)));
    }

    #[test]
    fn no_dht_values() {
        assert!(no_dht(Some("1")) && no_dht(Some(" true ")));
        assert!(!no_dht(None) && !no_dht(Some("0")) && !no_dht(Some("")));
    }

    #[test]
    fn background_errors_map_to_codes() {
        let code = |m: &str| background_error(m).code();
        use crate::error::ErrorCode;
        assert_eq!(
            code("error writing: No space left on device (os error 28)"),
            ErrorCode::Io
        );
        assert_eq!(code("Permission denied (os error 13)"), ErrorCode::Io);
        assert_eq!(code("Read-only file system (os error 30)"), ErrorCode::Io);
        assert_eq!(code("bug: torrent in broken state"), ErrorCode::Torrent);
    }
}
