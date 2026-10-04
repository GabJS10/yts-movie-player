//! Downloads end to end without internet: a local librqbit seeder, the app's TorrentEngine
//! and the DownloadManager on a real DB, connected through `initial_peers` on localhost.

mod common;

use std::net::{Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use common::*;
use tokio::sync::broadcast;
use yts_player_lib::db::{self, Db};
use yts_player_lib::downloads::{DownloadManager, DownloadsConfig, FreeSpaceFn, TorrentInfo};
use yts_player_lib::error::AppError;
use yts_player_lib::images::ImageStore;
use yts_player_lib::stream;
use yts_player_lib::torrent::{EngineConfig, StreamRequest, TorrentEngine};
use yts_player_lib::types::{
    Download, DownloadChanged, DownloadState, MovieDetail, MovieSummary, Quality, StreamPhase,
    StreamSource, VideoCodec,
};

const MOVIE_ID: u64 = 7;
const FOLDER: &str = "Test Movie (2024) [1080p]";

fn summary() -> MovieSummary {
    MovieSummary {
        id: MOVIE_ID,
        imdb_code: "tt0000007".into(),
        title: "Test Movie".into(),
        year: 2024,
        rating: 7.0,
        runtime_min: 100,
        genres: vec!["Drama".into()],
        cover_url: None,
        cover_large_url: None,
        background_url: None,
        qualities: vec![Quality::P1080],
        has_x264: true,
        max_seeds: 12,
    }
}

fn detail() -> MovieDetail {
    MovieDetail {
        summary_fields: summary(),
        summary: "A test.".into(),
        language: "en".into(),
        mpa_rating: None,
        yt_trailer_code: None,
        screenshot_urls: vec![],
        cast: vec![],
        torrents: vec![],
        is_favorite: false,
        progress: None,
        download: None,
        offline: false,
    }
}

fn torrent_info(seeder: &Seeder) -> TorrentInfo {
    TorrentInfo {
        infohash: seeder.infohash.to_ascii_uppercase(),
        quality: Quality::P1080,
        video_codec: VideoCodec::X264,
        size_bytes: VIDEO_LEN as u64 + 400_000,
        title: "Test Movie".into(),
        torrent_url: None,
        seeds: 12,
    }
}

/// App side: engine + local HTTP server + DB + download manager, over `root`.
struct App {
    root: PathBuf,
    engine: Arc<TorrentEngine>,
    downloads: Arc<DownloadManager>,
    db: Db,
}

impl App {
    fn library(&self) -> PathBuf {
        self.root.join("library")
    }

    fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
}

fn unlimited_space() -> FreeSpaceFn {
    Arc::new(|_: &Path| u64::MAX / 2)
}

async fn start_app(
    root: &Path,
    peers: Vec<SocketAddr>,
    seed: bool,
    free_space: FreeSpaceFn,
    tweak: impl FnOnce(&mut EngineConfig),
) -> App {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "yts_player_lib=warn".into()))
        .try_init();
    let listener = stream::bind().await.unwrap();
    let local_base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let mut cfg = EngineConfig {
        dht: false,
        trackers: Vec::new(),
        initial_peers: peers,
        listen_addr: Some((Ipv4Addr::LOCALHOST, 0).into()),
        metadata_timeout: Duration::from_secs(10),
        stats_interval: Duration::from_millis(150),
        ..EngineConfig::new(root.join("cache"), local_base.clone())
    };
    tweak(&mut cfg);
    let engine = TorrentEngine::new(cfg).await.unwrap();
    let images = Arc::new(ImageStore::new(root.join("img"), Vec::new(), local_base).unwrap());
    let router = stream::router(stream::ServerState {
        images: Arc::clone(&images),
        torrents: Some(Arc::clone(&engine)),
        subs_dir: None,
    });
    tokio::spawn(stream::serve(listener, router));
    let db = Db::open(&root.join(db::DB_FILE)).unwrap();
    let downloads = DownloadManager::new(DownloadsConfig {
        db: db.clone(),
        images,
        engine: Arc::clone(&engine),
        library_dir: root.join("library"),
        seed_after_download: seed,
        free_space,
    });
    downloads.restore().await.unwrap();
    downloads.resume_all();
    downloads.spawn_tick(Duration::from_millis(100));
    App {
        root: root.to_path_buf(),
        engine,
        downloads,
        db,
    }
}

/// Polls until the download satisfies `pred` (or panics after the timeout).
async fn wait_for(app: &App, infohash: &str, what: &str, pred: impl Fn(&Download) -> bool) {
    let found = tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            if app.downloads.get(infohash).is_some_and(|d| pred(&d)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        found.is_ok(),
        "timed out waiting for {what}: {:?}",
        app.downloads.get(infohash)
    );
}

fn states(rx: &mut broadcast::Receiver<DownloadChanged>) -> Vec<Option<DownloadState>> {
    let mut out = Vec::new();
    while let Ok(c) = rx.try_recv() {
        out.push(c.download.map(|d| d.state));
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn download_pause_resume_finish_play_from_library_and_remove() {
    let seeder = start_seeder().await;
    let tmp = tempfile::tempdir().unwrap();
    let app = start_app(
        tmp.path(),
        vec![seeder.addr],
        false,
        unlimited_space(),
        |_| {},
    )
    .await;
    // Speed limits apply on the fly: slow enough to pause half way.
    app.engine
        .set_rate_limits(NonZeroU32::new(1_000_000), NonZeroU32::new(500_000));
    assert_eq!(
        app.engine.rate_limits(),
        (NonZeroU32::new(1_000_000), NonZeroU32::new(500_000))
    );
    let mut rx = app.downloads.subscribe();
    let ih = seeder.infohash.clone();

    let d = app
        .downloads
        .start(summary(), &detail(), torrent_info(&seeder))
        .await
        .unwrap();
    assert_eq!(d.infohash, ih);
    assert_eq!(d.state, DownloadState::Queued);
    assert_eq!(d.movie.id, MOVIE_ID);
    assert_eq!(
        (d.quality, d.video_codec),
        (Quality::P1080, VideoCodec::X264)
    );
    // Idempotent.
    let again = app
        .downloads
        .start(summary(), &detail(), torrent_info(&seeder))
        .await
        .unwrap();
    assert_eq!(again.added_at, d.added_at);
    assert_eq!(app.downloads.list().len(), 1);
    // The movie page has its offline copy.
    assert!(app
        .db
        .movie_detail(MOVIE_ID, "http://127.0.0.1:1")
        .await
        .unwrap()
        .is_some());

    wait_for(&app, &ih, "some progress", |d| {
        d.state == DownloadState::Active && d.downloaded_bytes > 0
    })
    .await;
    let d = app.downloads.get(&ih).unwrap();
    assert_eq!(d.size_bytes, VIDEO_LEN as u64);
    assert_eq!(
        d.path.as_deref().map(Path::new),
        Some(app.library().join(FOLDER).as_path())
    );
    assert!(d.progress > 0.0 && d.progress < 1.0, "{d:?}");

    // Pause: the torrent stops and progress holds.
    let paused = app.downloads.pause(&ih).await.unwrap();
    assert_eq!(paused.state, DownloadState::Paused);
    assert_eq!(app.engine.is_paused(&ih), Some(true));
    tokio::time::sleep(Duration::from_millis(600)).await;
    let held = app.downloads.get(&ih).unwrap();
    assert_eq!(held.state, DownloadState::Paused);
    assert!(held.downloaded_bytes < VIDEO_LEN as u64);
    assert_eq!((held.down_speed_bps, held.eta_s), (0, None));

    // Resume and finish (unlimited now).
    app.engine.set_rate_limits(None, None);
    let resumed = app.downloads.resume(&ih).await.unwrap();
    assert_eq!(resumed.state, DownloadState::Active);
    wait_for(&app, &ih, "done", |d| d.state == DownloadState::Done).await;
    let done = app.downloads.get(&ih).unwrap();
    assert_eq!(done.progress, 1.0);
    assert_eq!(done.downloaded_bytes, VIDEO_LEN as u64);
    let video = app.library().join(FOLDER).join(VIDEO_NAME);
    assert_eq!(std::fs::read(&video).unwrap(), seeder.video);
    // Never touched the cache.
    assert!(!app.engine.torrent_dir(&ih).exists());
    // Without seedAfterDownload the torrent is paused once finished.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(app.engine.is_paused(&ih), Some(true));

    let seen = states(&mut rx);
    let mut dedup = seen.clone();
    dedup.dedup();
    assert_eq!(
        dedup,
        [
            Some(DownloadState::Queued),
            Some(DownloadState::Active),
            Some(DownloadState::Paused),
            Some(DownloadState::Active),
            Some(DownloadState::Done),
        ],
        "{seen:?}"
    );
    assert_eq!(seen.len(), dedup.len(), "events only on changes: {seen:?}");

    // Seeding turned on: the finished torrent runs again; off: paused.
    app.downloads.set_seed_after_download(true).await;
    assert_eq!(app.engine.is_paused(&ih), Some(false));
    app.downloads.set_seed_after_download(false).await;
    assert_eq!(app.engine.is_paused(&ih), Some(true));

    // Played from library/: no torrent involved.
    let local = app.downloads.library_stream(&ih).expect("library stream");
    let session = app.engine.serve_local(local).await.unwrap();
    assert_eq!(session.source, StreamSource::Library);
    assert_eq!(session.file_size_bytes, VIDEO_LEN as u64);
    assert_eq!(session.file_name, VIDEO_NAME);
    assert_eq!(app.engine.session_movie_id(&ih), Some(MOVIE_ID));
    assert_eq!(app.engine.session_url(&ih).unwrap(), session.stream_url);
    let (status, headers, body) = get(&session.stream_url, Some("bytes=4000000-4000999")).await;
    assert_eq!(status, 206);
    assert_eq!(body, seeder.video[4_000_000..4_001_000]);
    assert_eq!(
        headers["content-range"],
        format!("bytes 4000000-4000999/{VIDEO_LEN}")
    );
    assert_eq!(get(&session.stream_url, None).await.2.len(), VIDEO_LEN);

    assert_eq!(
        app.downloads.folder(&ih).unwrap(),
        app.library().join(FOLDER)
    );

    // Remove with files: out of the session first, nothing left open, folder gone.
    assert!(!open_files_under(&app.library()).is_empty() || cfg!(not(target_os = "linux")));
    app.downloads.remove(&ih, true).await.unwrap();
    assert!(open_files_under(&app.library()).is_empty());
    assert!(!app.library().join(FOLDER).exists());
    assert!(app.downloads.list().is_empty());
    assert!(!app.engine.has_torrent(&ih));
    assert_eq!(get(&session.stream_url, None).await.0, 404);
    assert_eq!(states(&mut rx).last(), Some(&None));
    assert!(app.db.list_downloads("").await.unwrap().is_empty());
    assert!(app.db.movie_detail(MOVIE_ID, "").await.unwrap().is_none());
    assert!(matches!(
        app.downloads.remove(&ih, true).await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn promotes_a_stream_to_library_without_downloading_again() {
    let seeder = start_seeder().await;
    let tmp = tempfile::tempdir().unwrap();
    let app = start_app(
        tmp.path(),
        vec![seeder.addr],
        false,
        unlimited_space(),
        |_| {},
    )
    .await;
    let ih = seeder.infohash.clone();
    let mut stats = app.engine.subscribe();
    let session = app
        .engine
        .start_stream(StreamRequest {
            movie_id: MOVIE_ID,
            infohash: ih.clone(),
            title: "Test Movie".into(),
            torrent_url: None,
            video_codec: VideoCodec::X264,
            seeds: 12,
        })
        .await
        .unwrap();
    // Watch it until it is all in cache/.
    let _ = tokio::time::timeout(Duration::from_secs(40), async {
        while let Ok(s) = stats.recv().await {
            if s.phase == StreamPhase::Done {
                break;
            }
        }
    })
    .await;
    let cache_dir = app.engine.torrent_dir(&ih);
    assert!(file_in(&cache_dir, VIDEO_NAME).is_some());

    // The seeder goes away: from now on nothing can be downloaded again.
    seeder.session.stop().await;

    // Download while the stream is open: it is marked and stays in cache/ meanwhile.
    let d = app
        .downloads
        .start(summary(), &detail(), torrent_info(&seeder))
        .await
        .unwrap();
    assert_eq!(d.infohash, ih);
    wait_for(&app, &ih, "resolved", |d| d.state != DownloadState::Queued).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(cache_dir.exists(), "moved while the stream was open");
    assert!(app.engine.in_use().contains(&ih));
    let (status, _, body) = get(&session.stream_url, Some("bytes=100-199")).await;
    assert_eq!(status, 206);
    assert_eq!(body, seeder.video[100..200]);

    // The stream stops: moved to library/, checked (not downloaded) and done.
    app.engine.stop_stream(&ih).await.unwrap();
    wait_for(&app, &ih, "done after promotion", |d| {
        d.state == DownloadState::Done
    })
    .await;
    assert!(!cache_dir.exists());
    let video = app.library().join(FOLDER).join(VIDEO_NAME);
    assert_eq!(std::fs::read(&video).unwrap(), seeder.video);
    // No longer part of the cache: nothing for the LRU to see in cache/.
    assert!(!app.cache().join(&ih).exists());

    // Watching it again plays the file.
    let local = app.downloads.library_stream(&ih).unwrap();
    let again = app.engine.serve_local(local).await.unwrap();
    assert_eq!(again.source, StreamSource::Library);
    let (status, _, body) = get(&again.stream_url, Some("bytes=-500")).await;
    assert_eq!(status, 206);
    assert_eq!(body, seeder.video[VIDEO_LEN - 500..]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn not_enough_space_creates_nothing() {
    let seeder = start_seeder().await;
    let tmp = tempfile::tempdir().unwrap();
    let app = start_app(
        tmp.path(),
        vec![seeder.addr],
        false,
        Arc::new(|_: &Path| 1024 * 1024),
        |_| {},
    )
    .await;
    let err = app
        .downloads
        .start(summary(), &detail(), torrent_info(&seeder))
        .await
        .unwrap_err();
    assert_eq!(serde_json::to_value(&err).unwrap()["code"], "io");
    assert!(err.to_string().contains("not enough free space"), "{err}");
    assert!(app.downloads.list().is_empty());
    assert!(app.db.list_downloads("").await.unwrap().is_empty());
    assert!(!app.library().join(FOLDER).exists());
    assert!(!app.engine.has_torrent(&seeder.infohash));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn downloads_survive_a_restart_offline() {
    let seeder = start_seeder().await;
    let tmp = tempfile::tempdir().unwrap();
    let ih = seeder.infohash.clone();

    // 1st run: download part of it slowly, pause and quit.
    {
        let app = start_app(
            tmp.path(),
            vec![seeder.addr],
            false,
            unlimited_space(),
            |c| c.download_limit_bps = NonZeroU32::new(1_000_000),
        )
        .await;
        app.downloads
            .start(summary(), &detail(), torrent_info(&seeder))
            .await
            .unwrap();
        wait_for(&app, &ih, "some progress", |d| {
            d.downloaded_bytes > 512 * 1024
        })
        .await;
        app.downloads.pause(&ih).await.unwrap();
        app.downloads.tick(true).await;
        app.engine.shutdown().await;
    }

    // 2nd run without any peer (offline): the paused download is back as it was, from the
    // saved .torrent, with its pieces checked from disk.
    let partial = {
        let app = start_app(tmp.path(), Vec::new(), false, unlimited_space(), |_| {}).await;
        let d = app.downloads.get(&ih).expect("restored");
        assert_eq!(d.state, DownloadState::Paused);
        assert_eq!(d.movie.title, "Test Movie");
        assert!(d.downloaded_bytes > 0, "{d:?}");
        wait_for(&app, &ih, "torrent back in the session", |_| {
            app.engine.download_stats(&ih).is_some_and(|s| s.resolved)
        })
        .await;
        assert_eq!(app.engine.is_paused(&ih), Some(true));
        let s = app.engine.download_stats(&ih).unwrap();
        assert!(s.downloaded > 0 && s.downloaded < VIDEO_LEN as u64, "{s:?}");
        app.engine.shutdown().await;
        s.downloaded
    };

    // 3rd run with the seeder: resume finishes it, keeping what was there.
    {
        let app = start_app(
            tmp.path(),
            vec![seeder.addr],
            false,
            unlimited_space(),
            |_| {},
        )
        .await;
        wait_for(&app, &ih, "restored", |d| d.downloaded_bytes >= partial).await;
        app.downloads.resume(&ih).await.unwrap();
        wait_for(&app, &ih, "done", |d| d.state == DownloadState::Done).await;
        app.downloads.tick(true).await;
        app.engine.shutdown().await;
    }

    // 4th run offline: done, not even in the torrent session, plays from library/.
    {
        let app = start_app(tmp.path(), Vec::new(), false, unlimited_space(), |_| {}).await;
        let d = app.downloads.get(&ih).unwrap();
        assert_eq!(d.state, DownloadState::Done);
        assert_eq!(d.progress, 1.0);
        assert!(!app.engine.has_torrent(&ih));
        assert_eq!(app.downloads.for_movie(MOVIE_ID).unwrap().infohash, ih);
        let session = app
            .engine
            .serve_local(app.downloads.library_stream(&ih).unwrap())
            .await
            .unwrap();
        let (status, _, body) = get(&session.stream_url, Some("bytes=0-999")).await;
        assert_eq!(status, 206);
        assert_eq!(body, seeder.video[..1000]);
        // The movie page works offline from the saved copy.
        let copy = app.db.movie_detail(MOVIE_ID, "").await.unwrap().unwrap();
        assert_eq!(copy.summary, "A test.");
        app.engine.shutdown().await;
    }

    // The file was deleted by hand: the download comes back in `error`.
    std::fs::remove_file(tmp.path().join("library").join(FOLDER).join(VIDEO_NAME)).unwrap();
    let app = start_app(tmp.path(), Vec::new(), false, unlimited_space(), |_| {}).await;
    let d = app.downloads.get(&ih).unwrap();
    assert_eq!(d.state, DownloadState::Error);
    assert!(d.error.as_deref().unwrap().contains("missing"));
    assert!(app.downloads.library_stream(&ih).is_none());
}
