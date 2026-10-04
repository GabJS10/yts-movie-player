//! End-to-end streaming without internet: a librqbit seeder session and the app's
//! TorrentEngine as downloader, connected through `initial_peers` on localhost (no DHT, no
//! trackers). The video is served by the real local HTTP server.

use std::net::{Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{
    create_torrent, AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerOptions, Session,
    SessionOptions,
};
use tokio::sync::broadcast;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};
use yts_player_lib::cache::{CacheManager, Evictor};
use yts_player_lib::error::AppError;
use yts_player_lib::images::ImageStore;
use yts_player_lib::stream;
use yts_player_lib::torrent::{EngineConfig, StreamRequest, TorrentEngine};
use yts_player_lib::types::{StreamPhase, StreamSource, TorrentStats, VideoCodec};

const VIDEO_NAME: &str = "Test.Movie.2024.1080p.WEBRip.x264.mp4";
const VIDEO_LEN: usize = 6 * 1024 * 1024 + 12_345;
const PIECE_LEN: u32 = 64 * 1024;

fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed;
    (0..len)
        .map(|_| {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (x >> 33) as u8
        })
        .collect()
}

struct Seeder {
    _dir: tempfile::TempDir,
    _session: Arc<Session>,
    addr: SocketAddr,
    torrent_bytes: bytes::Bytes,
    infohash: String,
    video: Vec<u8>,
}

/// Creates a YTS-like folder (video + .txt + .jpg), its torrent and a seeding session.
async fn start_seeder() -> Seeder {
    let dir = tempfile::tempdir().unwrap();
    let content = dir.path().join("Test Movie (2024) [1080p] [YTS]");
    std::fs::create_dir_all(&content).unwrap();
    let video = pseudo_random(VIDEO_LEN, 42);
    std::fs::write(content.join(VIDEO_NAME), &video).unwrap();
    std::fs::write(content.join("YTS.txt"), pseudo_random(300_000, 1)).unwrap();
    std::fs::write(content.join("www.YTS.jpg"), pseudo_random(70_000, 2)).unwrap();

    let torrent = create_torrent(
        &content,
        CreateTorrentOptions {
            name: None,
            trackers: Vec::new(),
            piece_length: Some(PIECE_LEN),
        },
        &BlockingSpawner::new(1),
    )
    .await
    .unwrap();
    let torrent_bytes = torrent.as_bytes().unwrap();

    let session = Session::new_with_opts(
        dir.path().to_path_buf(),
        SessionOptions {
            dht: None,
            disable_trackers: true,
            disable_local_service_discovery: true,
            listen: Some(ListenerOptions {
                listen_addr: (Ipv4Addr::LOCALHOST, 0).into(),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(torrent_bytes.clone()),
            Some(AddTorrentOptions {
                output_folder: Some(content.to_string_lossy().into_owned()),
                overwrite: true,
                ..Default::default()
            }),
        )
        .await
        .unwrap()
        .into_handle()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), handle.wait_until_completed())
        .await
        .unwrap()
        .unwrap();

    Seeder {
        addr: session.listen_addr().unwrap(),
        infohash: torrent.info_hash().as_string(),
        _dir: dir,
        _session: session,
        torrent_bytes,
        video,
    }
}

struct Downloader {
    tmp: tempfile::TempDir,
    engine: Arc<TorrentEngine>,
}

async fn start_downloader(
    initial_peers: Vec<SocketAddr>,
    tweak: impl FnOnce(&mut EngineConfig),
) -> Downloader {
    let tmp = tempfile::tempdir().unwrap();
    let listener = stream::bind().await.unwrap();
    let local_base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let mut cfg = EngineConfig {
        dht: false,
        trackers: Vec::new(),
        initial_peers,
        listen_addr: Some((Ipv4Addr::LOCALHOST, 0).into()),
        allowed_torrent_hosts: vec!["127.0.0.1".into()],
        buffer_target_bytes: 1024 * 1024,
        stats_interval: Duration::from_millis(150),
        ..EngineConfig::new(tmp.path().join("cache"), local_base.clone())
    };
    tweak(&mut cfg);
    let engine = TorrentEngine::new(cfg).await.unwrap();
    let images = Arc::new(ImageStore::new(tmp.path().join("img"), Vec::new(), local_base).unwrap());
    let router = stream::router(stream::ServerState {
        images,
        torrents: Some(Arc::clone(&engine)),
    });
    tokio::spawn(stream::serve(listener, router));
    Downloader { tmp, engine }
}

fn request(seeder: &Seeder, torrent_url: Option<String>) -> StreamRequest {
    StreamRequest {
        movie_id: 7,
        infohash: seeder.infohash.to_ascii_uppercase(),
        title: "Test Movie".into(),
        torrent_url,
        video_codec: VideoCodec::X264,
        seeds: 12,
    }
}

async fn get(url: &str, range: Option<&str>) -> (u16, reqwest::header::HeaderMap, Vec<u8>) {
    let client = reqwest::Client::new();
    let mut req = client.get(url);
    if let Some(r) = range {
        req = req.header("Range", r);
    }
    let resp = tokio::time::timeout(Duration::from_secs(30), req.send())
        .await
        .unwrap()
        .unwrap();
    let status = resp.status().as_u16();
    let headers = resp.headers().clone();
    let body = resp.bytes().await.unwrap().to_vec();
    (status, headers, body)
}

/// Collects stats until `phase` shows up (or timeout).
async fn collect_until(
    rx: &mut broadcast::Receiver<TorrentStats>,
    phase: StreamPhase,
) -> Vec<TorrentStats> {
    let mut all = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(40), async {
        while let Ok(s) = rx.recv().await {
            let reached = s.phase == phase;
            all.push(s);
            if reached {
                break;
            }
        }
    })
    .await;
    all
}

async fn collect_until_done(rx: &mut broadcast::Receiver<TorrentStats>) -> Vec<TorrentStats> {
    collect_until(rx, StreamPhase::Done).await
}

fn dedup_phases(stats: &[TorrentStats]) -> Vec<StreamPhase> {
    let mut phases: Vec<StreamPhase> = stats.iter().map(|s| s.phase).collect();
    phases.dedup();
    phases
}

fn is_ordered_subsequence(phases: &[StreamPhase]) -> bool {
    let order = [
        StreamPhase::Connecting,
        StreamPhase::Metadata,
        StreamPhase::Buffering,
        StreamPhase::Ready,
        StreamPhase::Done,
    ];
    // A seek to a missing range legitimately goes back from ready to buffering.
    let rank = |p: &StreamPhase| {
        let p = if *p == StreamPhase::Ready {
            StreamPhase::Buffering
        } else {
            *p
        };
        order.iter().position(|o| *o == p)
    };
    phases
        .iter()
        .map(rank)
        .collect::<Option<Vec<_>>>()
        .is_some_and(|r| r.windows(2).all(|w| w[0] <= w[1]))
}

fn file_in(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    walk(dir)
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == name))
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn streams_video_from_torrent_file_with_ranges_and_phases() {
    let seeder = start_seeder().await;
    let yts = MockServer::start().await;
    Mock::given(path(format!(
        "/torrent/download/{}",
        seeder.infohash.to_ascii_uppercase()
    )))
    .respond_with(
        ResponseTemplate::new(200)
            .set_body_raw(seeder.torrent_bytes.to_vec(), "application/x-bittorrent"),
    )
    .expect(1)
    .mount(&yts)
    .await;
    // Rate limit so the intermediate phases are observable.
    let dl = start_downloader(vec![seeder.addr], |c| {
        c.download_limit_bps = NonZeroU32::new(1_500_000);
    })
    .await;
    let mut rx = dl.engine.subscribe();
    let url = format!(
        "{}/torrent/download/{}",
        yts.uri(),
        seeder.infohash.to_ascii_uppercase()
    );

    let session = dl
        .engine
        .start_stream(request(&seeder, Some(url.clone())))
        .await
        .unwrap();
    assert_eq!(session.infohash, seeder.infohash);
    assert_eq!(session.movie_id, 7);
    assert_eq!(session.file_name, VIDEO_NAME);
    assert_eq!(session.file_size_bytes, VIDEO_LEN as u64);
    assert!(session.likely_playable);
    assert_eq!(session.resume_at_s, None);
    assert_eq!(session.source, StreamSource::Network);
    let prefix = format!("/stream/{}/", seeder.infohash);
    let file_idx: usize = session
        .stream_url
        .split_once(&prefix)
        .and_then(|(_, idx)| idx.parse().ok())
        .expect("stream URL ends with /stream/<infohash>/<fileIdx>");
    assert!(session.stream_url.starts_with("http://127.0.0.1:"));

    // Idempotent (and the .torrent is downloaded only once: `expect(1)`).
    let again = dl
        .engine
        .start_stream(request(&seeder, Some(url)))
        .await
        .unwrap();
    assert_eq!(again, session);

    // Before any read the position is 0: the 1 MB target fills while rate limited.
    let mut stats = collect_until(&mut rx, StreamPhase::Ready).await;
    assert_eq!(
        stats.last().map(|s| s.phase),
        Some(StreamPhase::Ready),
        "never ready: {:?}",
        dedup_phases(&stats)
    );

    // Range in the middle and at the end (MP4 moov atom) → 206 with the exact bytes.
    let (status, headers, body) = get(&session.stream_url, Some("bytes=3000000-3099999")).await;
    assert_eq!(status, 206);
    assert_eq!(body, seeder.video[3_000_000..3_100_000]);
    assert_eq!(
        headers["content-range"],
        format!("bytes 3000000-3099999/{VIDEO_LEN}")
    );
    assert_eq!(headers["accept-ranges"], "bytes");
    assert_eq!(headers["content-type"], "video/mp4");

    let (status, headers, body) = get(&session.stream_url, Some("bytes=-1000")).await;
    assert_eq!(status, 206);
    assert_eq!(body, seeder.video[VIDEO_LEN - 1000..]);
    assert_eq!(
        headers["content-range"],
        format!("bytes {}-{}/{VIDEO_LEN}", VIDEO_LEN - 1000, VIDEO_LEN - 1)
    );

    let (status, headers, _) = get(&session.stream_url, Some(&format!("bytes={VIDEO_LEN}-"))).await;
    assert_eq!(status, 416);
    assert_eq!(headers["content-range"], format!("bytes */{VIDEO_LEN}"));

    // Without Range → 200 with the whole file.
    let (status, headers, body) = get(&session.stream_url, None).await;
    assert_eq!(status, 200);
    assert_eq!(headers["content-length"], VIDEO_LEN.to_string());
    assert!(body == seeder.video, "full body differs");

    // Unknown file index / infohash → 404.
    let wrong_idx = session
        .stream_url
        .replace(&format!("/{file_idx}"), &format!("/{}", file_idx + 1));
    assert_eq!(get(&wrong_idx, None).await.0, 404);

    stats.extend(collect_until_done(&mut rx).await);
    let phases = dedup_phases(&stats);
    assert!(
        is_ordered_subsequence(&phases),
        "phases out of order: {phases:?}"
    );
    assert!(phases.contains(&StreamPhase::Ready), "{phases:?}");
    assert_eq!(phases.last(), Some(&StreamPhase::Done), "{phases:?}");
    assert!(stats
        .iter()
        .all(|s| s.seeds == 12 && s.infohash == seeder.infohash));
    // The file is smaller than the 64 MB window: pieceMap covers the whole file.
    let with_map: Vec<&TorrentStats> = stats.iter().filter(|s| s.piece_map.is_some()).collect();
    assert!(
        !with_map.is_empty(),
        "pieceMap never reported while streaming"
    );
    for s in &with_map {
        assert_eq!(s.piece_map.as_deref().map(str::len), Some(200));
        let w = s.piece_map_window.expect("pieceMapWindow with pieceMap");
        assert_eq!((w.start_byte, w.end_byte), (0, VIDEO_LEN as u64));
    }
    assert!(stats
        .iter()
        .all(|s| s.piece_map.is_some() == s.piece_map_window.is_some()));
    assert!(stats.iter().any(|s| s.peers > 0 && s.down_speed_bps > 0));
    let last = stats.last().unwrap();
    assert_eq!(last.progress, 1.0);
    assert_eq!(last.downloaded_bytes, VIDEO_LEN as u64);
    assert_eq!(last.available_ranges, vec![(0.0, 1.0)]);

    // Only the video was selected: the .txt and .jpg were not downloaded beyond edge pieces.
    let cache = dl.tmp.path().join("cache");
    let video_path = file_in(&cache, VIDEO_NAME).expect("video in cache/");
    assert_eq!(std::fs::read(video_path).unwrap(), seeder.video);
    for extra in ["YTS.txt", "www.YTS.jpg"] {
        if let Some(p) = file_in(&cache, extra) {
            let data = std::fs::read(&p).unwrap();
            // Only the bytes sharing an edge piece with the video may be written.
            let written = data.iter().filter(|&&b| b != 0).count();
            assert!(
                written <= 2 * PIECE_LEN as usize,
                "{extra} was downloaded ({written} bytes)"
            );
        }
    }

    // stop_stream pauses; stats stop; start_stream resumes.
    dl.engine.stop_stream(&seeder.infohash).await.unwrap();
    assert_eq!(dl.engine.is_paused(&seeder.infohash), Some(true));
    tokio::time::sleep(Duration::from_millis(400)).await;
    while rx.try_recv().is_ok() {}
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(rx.try_recv().is_err(), "stats emitted for a stopped stream");
    dl.engine
        .start_stream(request(&seeder, None))
        .await
        .unwrap();
    assert_eq!(dl.engine.is_paused(&seeder.infohash), Some(false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn falls_back_to_magnet_when_torrent_file_fails() {
    let seeder = start_seeder().await;
    let yts = MockServer::start().await;
    Mock::given(path("/torrent/download/broken"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&yts)
        .await;
    let dl = start_downloader(vec![seeder.addr], |_| {}).await;
    let mut rx = dl.engine.subscribe();

    let session = dl
        .engine
        .start_stream(request(
            &seeder,
            Some(format!("{}/torrent/download/broken", yts.uri())),
        ))
        .await
        .unwrap();
    assert_eq!(session.file_name, VIDEO_NAME);

    let (status, _, body) = get(&session.stream_url, Some("bytes=100-199")).await;
    assert_eq!(status, 206);
    assert_eq!(body, seeder.video[100..200]);

    let phases = dedup_phases(&collect_until_done(&mut rx).await);
    assert!(is_ordered_subsequence(&phases), "{phases:?}");
    assert_eq!(phases.last(), Some(&StreamPhase::Done));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn torrent_file_from_disallowed_host_is_ignored() {
    let seeder = start_seeder().await;
    let dl = start_downloader(vec![seeder.addr], |c| {
        c.allowed_torrent_hosts = vec!["yts.gg".into()];
    })
    .await;
    // localhost is not allowed: the URL is never fetched and the magnet path is used.
    let session = dl
        .engine
        .start_stream(request(
            &seeder,
            Some("http://127.0.0.1:9/torrent/download/x".into()),
        ))
        .await
        .unwrap();
    assert_eq!(session.file_name, VIDEO_NAME);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_peers_for_metadata_is_no_peers_error() {
    let seeder = start_seeder().await;
    // Nothing listens on port 9.
    let dl = start_downloader(vec!["127.0.0.1:9".parse().unwrap()], |c| {
        c.metadata_timeout = Duration::from_secs(2);
    })
    .await;
    let err = dl
        .engine
        .start_stream(request(&seeder, None))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NoPeers(_)), "{err:?}");
    assert_eq!(serde_json::to_value(&err).unwrap()["code"], "no_peers");
    // A failed start leaves nothing behind: stop is a no-op and the stream is unknown.
    dl.engine.stop_stream(&seeder.infohash).await.unwrap();
    assert!(dl.engine.session_url(&seeder.infohash).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_infohash_is_invalid_input() {
    let dl = start_downloader(Vec::new(), |_| {}).await;
    let err = dl
        .engine
        .start_stream(StreamRequest {
            movie_id: 1,
            infohash: "nope".into(),
            title: String::new(),
            torrent_url: None,
            video_codec: VideoCodec::X265,
            seeds: 0,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::InvalidInput(_)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn strict_mode_start_stop_start_resumes_the_same_session() {
    let seeder = start_seeder().await;
    let dl = start_downloader(vec![seeder.addr], |_| {}).await;
    let req = request(&seeder, None);

    // React StrictMode: start → stop → start fired back to back while the first start is
    // still resolving the magnet. They apply in call order, so the stream ends up running.
    let (first, stop, second) = tokio::join!(
        dl.engine.start_stream(req.clone()),
        dl.engine.stop_stream(&seeder.infohash),
        dl.engine.start_stream(req.clone()),
    );
    let first = first.unwrap();
    stop.unwrap();
    assert_eq!(second.unwrap(), first);
    assert_eq!(dl.engine.is_paused(&seeder.infohash), Some(false));

    // Same sequence once the stream is live.
    dl.engine.stop_stream(&seeder.infohash).await.unwrap();
    assert_eq!(dl.engine.is_paused(&seeder.infohash), Some(true));
    let resumed = dl.engine.start_stream(req.clone()).await.unwrap();
    assert_eq!(resumed, first);
    assert_eq!(dl.engine.is_paused(&seeder.infohash), Some(false));
    let (status, _, body) = get(&resumed.stream_url, Some("bytes=5000000-5000999")).await;
    assert_eq!(status, 206);
    assert_eq!(body, seeder.video[5_000_000..5_001_000]);

    // When the last call is a stop, the stream ends up paused.
    let (again, stop) = tokio::join!(
        dl.engine.start_stream(req.clone()),
        dl.engine.stop_stream(&seeder.infohash),
    );
    assert_eq!(again.unwrap(), first);
    stop.unwrap();
    assert_eq!(dl.engine.is_paused(&seeder.infohash), Some(true));
}

/// Files of `dir` that this process still has open (Linux: `/proc/self/fd`).
fn open_files_under(dir: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir("/proc/self/fd")
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| std::fs::read_link(e.path()).ok())
                .filter(|target| target.starts_with(dir))
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cache_lru_keeps_open_streams_and_evicts_stopped_ones_releasing_files() {
    let seeder = start_seeder().await;
    let dl = start_downloader(vec![seeder.addr], |_| {}).await;
    let mut rx = dl.engine.subscribe();
    let session = dl.engine.start_stream(request(&seeder, None)).await.unwrap();
    collect_until_done(&mut rx).await;

    // One folder per torrent: cache/<infohash>/…
    let dir = dl.engine.torrent_dir(&seeder.infohash);
    assert_eq!(dir, dl.tmp.path().join("cache").join(&seeder.infohash));
    assert!(file_in(&dir, VIDEO_NAME).is_some());
    let library = dl.tmp.path().join("library");
    std::fs::create_dir_all(library.join("Saved")).unwrap();
    std::fs::write(library.join("Saved/movie.mp4"), b"kept").unwrap();

    // Limit 0: everything should go, but the open stream is protected.
    let cache = CacheManager::new(
        dl.tmp.path().join("cache"),
        library.clone(),
        Arc::clone(&dl.engine) as Arc<dyn Evictor>,
        0,
    );
    assert!(dl.engine.in_use().contains(&seeder.infohash));
    assert_eq!(cache.enforce_limit().await.unwrap(), 0);
    assert_eq!(cache.clear().await.unwrap().freed_bytes, 0);
    assert!(dir.exists());

    // Stopped but a reader is still open (e.g. VLC): still protected.
    let file_idx: usize = session.stream_url.rsplit('/').next().unwrap().parse().unwrap();
    let (reader, _) = dl
        .engine
        .open_reader(&seeder.infohash, file_idx, 0)
        .await
        .unwrap();
    dl.engine.stop_stream(&seeder.infohash).await.unwrap();
    assert!(dl.engine.in_use().contains(&seeder.infohash));
    assert!(!dl.engine.evict(&seeder.infohash, &dir).await.unwrap());
    assert!(dir.exists());
    drop(reader);

    // Stopped and unused: evicted, out of the session and with no file left open, so the
    // space is really released (a paused librqbit torrent keeps its files open).
    assert!(dl.engine.in_use().is_empty());
    assert!(!open_files_under(&dir).is_empty() || cfg!(not(target_os = "linux")));
    let freed = cache.enforce_limit().await.unwrap();
    assert!(freed >= VIDEO_LEN as u64, "{freed}");
    assert!(!dir.exists());
    assert_eq!(dl.engine.is_paused(&seeder.infohash), None);
    assert!(open_files_under(&dir).is_empty());
    assert!(library.join("Saved/movie.mp4").exists());

    // Watching it again downloads it again into a fresh folder.
    let mut rx = dl.engine.subscribe();
    let again = dl.engine.start_stream(request(&seeder, None)).await.unwrap();
    collect_until_done(&mut rx).await;
    let (status, _, body) = get(&again.stream_url, Some("bytes=1000-1999")).await;
    assert_eq!(status, 206);
    assert_eq!(body, seeder.video[1000..2000]);
    assert!(dir.exists());
}
