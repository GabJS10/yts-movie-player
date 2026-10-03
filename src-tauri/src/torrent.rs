//! Torrent engine (librqbit): one session, streaming sessions keyed by infohash, file
//! readers for the local HTTP server and the `torrent://stats` feed.
//!
//! Adding a torrent: the `.torrent` from YTS is preferred (no metadata wait); if it can't
//! be downloaded quickly we fall back to a magnet. Only the largest video file is selected.

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
use crate::images::{host_allowed, restricted_client};
use crate::types::{StreamPhase, StreamSession, StreamSource, TorrentStats, VideoCodec};

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

/// Read-ahead window librqbit prioritizes for every open file stream
/// (`PER_STREAM_BUF_DEFAULT` in librqbit 9.0.1). Mirrored to mark "priority" cells.
pub const STREAM_PRIORITY_WINDOW: u64 = 32 * 1024 * 1024;

pub const PIECE_MAP_CELLS: usize = 200;

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

/// File sampled into `cells` cells: "1" all pieces downloaded, "2" a missing piece is in a
/// stream's priority window, "0" missing. ("3", in flight, is not exposed by librqbit.)
pub fn piece_map(
    geo: &FileGeometry,
    have: &[bool],
    priority: &HashSet<usize>,
    cells: usize,
) -> String {
    (0..cells)
        .map(|i| {
            if geo.len == 0 {
                return '0';
            }
            let b0 = geo.len * i as u64 / cells as u64;
            let b1 = (geo.len * (i as u64 + 1) / cells as u64)
                .max(b0 + 1)
                .min(geo.len);
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
}

pub fn compute_phase(i: &PhaseInput) -> StreamPhase {
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
    // Near the end of the file the target can't be reached: whatever is left suffices.
    let target = i
        .buffer_target
        .min(i.file_len.saturating_sub(i.position))
        .max(1);
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
    pub trackers: Vec<String>,
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
    pub download_limit_bps: Option<NonZeroU32>,
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
            initial_peers: Vec::new(),
            allowed_torrent_hosts: Vec::new(),
            torrent_file_timeout: Duration::from_secs(5),
            metadata_timeout: Duration::from_secs(60),
            init_timeout: Duration::from_secs(120),
            buffer_target_bytes: 8 * 1024 * 1024,
            stats_interval: Duration::from_secs(1),
            stall_after: Duration::from_secs(30),
            download_limit_bps: None,
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

struct Active {
    handle: Arc<ManagedTorrent>,
    file_idx: usize,
    file_name: String,
    geo: FileGeometry,
}

#[derive(Default)]
struct Readers {
    next_id: AtomicU64,
    /// reader id → (position, last activity)
    map: Mutex<HashMap<u64, (u64, Instant)>>,
}

impl Readers {
    fn open(&self, position: u64) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut map) = self.map.lock() {
            map.insert(id, (position, Instant::now()));
        }
        id
    }

    fn update(&self, id: u64, position: u64) {
        if let Ok(mut map) = self.map.lock() {
            map.insert(id, (position, Instant::now()));
        }
    }

    fn close(&self, id: u64) {
        if let Ok(mut map) = self.map.lock() {
            map.remove(&id);
        }
    }

    /// (all positions, position of the most recently active reader)
    fn snapshot(&self) -> (Vec<u64>, Option<u64>) {
        let Ok(map) = self.map.lock() else {
            return (Vec::new(), None);
        };
        let positions = map.values().map(|(p, _)| *p).collect();
        let current = map.values().max_by_key(|(_, t)| *t).map(|(p, _)| *p);
        (positions, current)
    }
}

struct Entry {
    req: StreamRequest,
    started: Instant,
    resolving: Mutex<Option<Resolving>>,
    active: OnceCell<Active>,
    readers: Arc<Readers>,
    stopped: AtomicBool,
    last_alive: Mutex<Instant>,
}

impl Entry {
    fn new(req: StreamRequest) -> Self {
        Self {
            req,
            started: Instant::now(),
            resolving: Mutex::new(None),
            active: OnceCell::new(),
            readers: Arc::new(Readers::default()),
            stopped: AtomicBool::new(false),
            last_alive: Mutex::new(Instant::now()),
        }
    }

    fn set_resolving(&self, r: Option<Resolving>) {
        if let Ok(mut g) = self.resolving.lock() {
            *g = r;
        }
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
    allowed_hosts: Arc<HashSet<String>>,
    entries: Mutex<HashMap<String, Arc<Entry>>>,
    stats_tx: broadcast::Sender<TorrentStats>,
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
            disable_trackers: cfg.trackers.is_empty(),
            listen: Some(ListenerOptions {
                listen_addr: cfg
                    .listen_addr
                    .unwrap_or_else(|| ListenerOptions::default().listen_addr),
                ..Default::default()
            }),
            disable_local_service_discovery: !cfg.dht,
            ratelimits: LimitsConfig {
                download_bps: cfg.download_limit_bps,
                upload_bps: None,
            },
            ..Default::default()
        };
        let session = Session::new_with_opts(cfg.output_dir.clone(), opts)
            .await
            .map_err(|e| torrent_err("creating session", e))?;
        let allowed_hosts: Arc<HashSet<String>> = Arc::new(
            cfg.allowed_torrent_hosts
                .iter()
                .map(|h| h.to_ascii_lowercase())
                .collect(),
        );
        let http = restricted_client(
            Arc::clone(&allowed_hosts),
            cfg.torrent_file_timeout,
            cfg.torrent_file_timeout,
        )?;
        let (stats_tx, _) = broadcast::channel(64);
        let engine = Arc::new(Self {
            api: Api::new(Arc::clone(&session), None),
            session,
            cfg,
            http,
            allowed_hosts,
            entries: Mutex::new(HashMap::new()),
            stats_tx,
        });
        spawn_stats_loop(Arc::downgrade(&engine), engine.cfg.stats_interval);
        Ok(engine)
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
        let entry = {
            let mut entries = self
                .entries
                .lock()
                .map_err(|_| AppError::Internal("torrent entries lock poisoned".into()))?;
            Arc::clone(entries.entry(infohash.clone()).or_insert_with(|| {
                Arc::new(Entry::new(StreamRequest {
                    infohash: infohash.clone(),
                    ..req.clone()
                }))
            }))
        };

        let active = match entry.active.get_or_try_init(|| self.resolve(&entry)).await {
            Ok(active) => active,
            Err(e) => {
                entry.set_resolving(None);
                if let Ok(mut entries) = self.entries.lock() {
                    entries.remove(&infohash);
                }
                return Err(e);
            }
        };

        if active.handle.is_paused() {
            self.session
                .unpause(&active.handle)
                .await
                .map_err(|e| torrent_err("resuming torrent", e))?;
        }
        entry.stopped.store(false, Ordering::Relaxed);
        entry.mark_alive();

        Ok(StreamSession {
            infohash: infohash.clone(),
            movie_id: entry.req.movie_id,
            stream_url: self.stream_url(&infohash, active.file_idx),
            file_name: active.file_name.clone(),
            file_size_bytes: active.geo.len,
            video_codec: entry.req.video_codec,
            likely_playable: entry.req.video_codec == VideoCodec::X264,
            buffer_target_bytes: self.cfg.buffer_target_bytes,
            resume_at_s: None,
            source: StreamSource::Network,
        })
    }

    fn stream_url(&self, infohash: &str, file_idx: usize) -> String {
        format!("{}/stream/{infohash}/{file_idx}", self.cfg.local_base)
    }

    /// Pauses a streaming torrent. Files stay in `cache/`. Unknown infohash: no-op.
    pub async fn stop_stream(&self, infohash: &str) -> AppResult<()> {
        let Some(entry) = self.entry(&infohash.to_ascii_lowercase()) else {
            return Ok(());
        };
        entry.stopped.store(true, Ordering::Relaxed);
        // Phase 6: torrents that are also downloads must keep running.
        if let Some(active) = entry.active.get() {
            if !active.handle.is_paused() {
                self.session
                    .pause(&active.handle)
                    .await
                    .map_err(|e| torrent_err("pausing torrent", e))?;
            }
        }
        Ok(())
    }

    pub fn is_paused(&self, infohash: &str) -> Option<bool> {
        let entry = self.entry(&infohash.to_ascii_lowercase())?;
        entry.active.get().map(|a| a.handle.is_paused())
    }

    /// Stream URL of an active session (for the external player).
    pub fn session_url(&self, infohash: &str) -> AppResult<String> {
        let infohash = infohash.to_ascii_lowercase();
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
        tracing::info!(%infohash, file_idx, %file_name, len = geo.len, "stream ready");
        Ok(Active {
            handle,
            file_idx,
            file_name,
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

        // 1) .torrent from YTS (no metadata wait).
        let mut listed = None;
        if let Some(bytes) = self.download_torrent_file(entry).await {
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
        let resp = self
            .session
            .add_torrent(
                AddTorrent::from_bytes(listed.torrent_bytes.clone()),
                Some(AddTorrentOptions {
                    only_files: Some(vec![file_idx]),
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
        let entry = self.entry(&infohash.to_ascii_lowercase())?;
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
                    buffer_target: self.cfg.buffer_target_bytes,
                    idle_for: entry.started.elapsed(),
                    stall_after: self.cfg.stall_after,
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
        let position = current.unwrap_or(0);
        let buffered = buffered_ahead(&geo, &have, position);
        let complete = geo.len > 0 && downloaded >= geo.len;
        if complete || (peers > 0 && down > 0) {
            entry.mark_alive();
        }
        if matches!(stats.state, TorrentStatsState::Error) {
            tracing::warn!(infohash = %req.infohash, error = ?stats.error, "torrent error");
        }
        let phase = compute_phase(&PhaseInput {
            resolving: None,
            peers,
            downloaded,
            file_len: geo.len,
            position,
            buffered_ahead: buffered,
            buffer_target: self.cfg.buffer_target_bytes,
            idle_for: entry.idle_for(),
            stall_after: self.cfg.stall_after,
        });
        let piece_map = (!positions.is_empty()).then(|| {
            piece_map(
                &geo,
                &have,
                &priority_pieces(&geo, &positions),
                PIECE_MAP_CELLS,
            )
        });
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
            piece_map,
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

/// Launches `player <url>` detached. Missing binary → `external_player_missing`.
pub fn launch_external_player(player: &str, url: &str) -> AppResult<()> {
    let path = find_in_path(player)
        .ok_or_else(|| AppError::ExternalPlayerMissing(format!("{player} not found in PATH")))?;
    let mut child = tokio::process::Command::new(path)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    // Reap the process when it exits.
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    Ok(())
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let candidate = Path::new(program);
    if candidate.components().count() > 1 {
        return is_executable(candidate).then(|| candidate.to_path_buf());
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(program))
            .find(|p| is_executable(p))
    })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
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
        assert_eq!(piece_map(&geo, &h, &priority, 10), "1122000001");
        // Coarser map: a cell is "1" only if all its pieces are downloaded.
        assert_eq!(piece_map(&geo, &h, &priority, 5), "12000");
        assert_eq!(
            piece_map(&geo, &h, &priority, PIECE_MAP_CELLS).len(),
            PIECE_MAP_CELLS
        );
        // Tiny file, more cells than bytes.
        let tiny = FileGeometry {
            offset: 0,
            len: 3,
            piece_len: 100,
        };
        assert_eq!(piece_map(&tiny, &[true], &HashSet::new(), 6), "111111");
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
}
