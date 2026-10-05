//! E2E support (`docs/IPC.md`, "Entorno de pruebas"): the `e2e_seeder` example as a real
//! process, found only through the tracker in its `.torrent` (no DHT, no initial peers, as
//! with `YTS_PLAYER_NO_DHT=1`), and `XDG_DATA_HOME`.

mod common;

use std::io::{BufRead, BufReader};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use common::*;
use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};
use yts_player_lib::images::ImageStore;
use yts_player_lib::stream;
use yts_player_lib::torrent::{EngineConfig, StreamRequest, TorrentEngine};
use yts_player_lib::types::{StreamPhase, VideoCodec};

/// `target/<profile>/examples/e2e_seeder`, built by `cargo test` with the other targets.
fn seeder_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let profile_dir = exe.parent().and_then(|deps| deps.parent()).unwrap();
    profile_dir
        .join("examples")
        .join(format!("e2e_seeder{}", std::env::consts::EXE_SUFFIX))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn seeder_prints_json_seeds_through_its_tracker_and_stops_on_sigterm() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("Short Movie (2024).mp4");
    let video = pseudo_random(3 * 1024 * 1024 + 777, 9);
    std::fs::write(&file, &video).unwrap();

    let bin = seeder_bin();
    assert!(bin.exists(), "build the example first: {}", bin.display());
    let mut child = Command::new(&bin)
        .arg(&file)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let line = tokio::task::spawn_blocking(move || {
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).unwrap();
        line
    });
    let line = tokio::time::timeout(Duration::from_secs(60), line)
        .await
        .expect("seeder printed nothing")
        .unwrap();
    let info: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    let infohash = info["infohash"].as_str().unwrap().to_owned();
    assert_eq!(infohash.len(), 40);
    assert!(info["port"].as_u64().unwrap() > 0);
    let torrent_path = PathBuf::from(info["torrentPath"].as_str().unwrap());
    assert_eq!(
        torrent_path,
        std::fs::canonicalize(&file)
            .unwrap()
            .with_extension("mp4.torrent")
    );
    let torrent = std::fs::read(&torrent_path).unwrap();

    // The fake YTS server hands out the .torrent; the app finds the seeder via its tracker.
    let yts = MockServer::start().await;
    Mock::given(path(format!("/torrent/download/{infohash}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(torrent, "application/x-bittorrent"))
        .mount(&yts)
        .await;
    let listener = stream::bind().await.unwrap();
    let local_base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let engine = TorrentEngine::new(EngineConfig {
        dht: false,
        trackers: Vec::new(),
        disable_trackers: false,
        initial_peers: Vec::new(),
        listen_addr: Some((Ipv4Addr::LOCALHOST, 0).into()),
        allowed_torrent_hosts: vec!["127.0.0.1".into()],
        stats_interval: Duration::from_millis(150),
        ..EngineConfig::new(tmp.path().join("cache"), local_base.clone())
    })
    .await
    .unwrap();
    let images = Arc::new(ImageStore::new(tmp.path().join("img"), Vec::new(), local_base).unwrap());
    tokio::spawn(stream::serve(
        listener,
        stream::router(stream::ServerState {
            images,
            torrents: Some(Arc::clone(&engine)),
            subs_dir: None,
        }),
    ));
    let mut rx = engine.subscribe();
    let session = engine
        .start_stream(StreamRequest {
            movie_id: 1,
            infohash: infohash.clone(),
            title: "Short Movie".into(),
            torrent_url: Some(format!("{}/torrent/download/{infohash}", yts.uri())),
            video_codec: VideoCodec::X264,
            seeds: 1,
        })
        .await
        .unwrap();
    let done = tokio::time::timeout(Duration::from_secs(60), async {
        while let Ok(s) = rx.recv().await {
            if s.phase == StreamPhase::Done {
                return;
            }
        }
    })
    .await;
    assert!(done.is_ok(), "never downloaded from the seeder");
    let (status, _, body) = get(&session.stream_url, None).await;
    assert_eq!(status, 200);
    assert!(body == video, "content differs");

    // SIGTERM: clean exit. Windows has no SIGTERM: the process is killed.
    #[cfg(unix)]
    // SAFETY: plain kill(2) on our own child process.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    #[cfg(not(unix))]
    child.kill().unwrap();
    let status = tokio::task::spawn_blocking(move || child.wait().unwrap());
    let status = tokio::time::timeout(Duration::from_secs(20), status)
        .await
        .expect("seeder did not stop")
        .unwrap();
    if cfg!(unix) {
        assert!(status.success(), "{status:?}");
    }
    engine.shutdown().await;
}

#[cfg(target_os = "linux")]
#[test]
fn folders_follow_xdg_data_home_and_xdg_state_home() {
    use yts_player_lib::paths::AppPaths;

    let tmp = tempfile::tempdir().unwrap();
    // The only test in this binary that touches the environment.
    std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
    std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));
    let paths = AppPaths::default_location().unwrap();
    assert_eq!(paths.data_dir, tmp.path().join("data/yts-player"));
    assert_eq!(paths.cache_dir, tmp.path().join("data/yts-player/cache"));
    assert_eq!(
        paths.library_dir,
        tmp.path().join("data/yts-player/library")
    );
    assert_eq!(
        yts_player_lib::app::logs_dir(),
        Some(tmp.path().join("state/yts-player/logs"))
    );
}

#[test]
fn seeder_without_a_file_fails_with_usage() {
    let out = Command::new(seeder_bin()).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage"));
}
