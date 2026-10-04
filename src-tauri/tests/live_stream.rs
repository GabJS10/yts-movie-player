//! Manual check against the real YTS API and swarm (needs internet, downloads real data).
//! Run with: `cargo test --test live_stream -- --ignored --nocapture`
//! Data goes to a temporary directory that is deleted at the end.

use std::sync::Arc;
use std::time::{Duration, Instant};

use yts_player_lib::images::ImageStore;
use yts_player_lib::stream;
use yts_player_lib::torrent::{EngineConfig, StreamRequest, TorrentEngine};
use yts_player_lib::types::{ListMoviesParams, Quality, SortBy, VideoCodec};
use yts_player_lib::yts::{YtsClient, YtsConfig};

const TARGET: u64 = 8 * 1024 * 1024;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs internet and downloads real torrent data"]
async fn live_1080p_x264_stream() {
    let tmp = tempfile::tempdir().unwrap();
    let listener = stream::bind().await.unwrap();
    let local_base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let config = YtsConfig::default();
    let allowed = ImageStore::default_allowed_hosts(&config.base_urls);
    let images = Arc::new(
        ImageStore::new(tmp.path().join("img"), allowed.clone(), local_base.clone()).unwrap(),
    );
    let yts = YtsClient::new(config, Arc::clone(&images)).unwrap();

    // Most seeded movies; pick the 1080p x264 torrent with the most seeds.
    let page = yts
        .list_movies(&ListMoviesParams {
            limit: Some(20),
            sort_by: Some(SortBy::Seeds),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut best = None;
    for m in &page.movies {
        let detail = yts.get_movie(m.id).await.unwrap();
        for t in detail.torrents {
            if t.quality == Quality::P1080
                && t.video_codec == VideoCodec::X264
                && best.as_ref().is_none_or(|(_, _, s)| t.seeds > *s)
            {
                best = Some((m.id, t.infohash.clone(), t.seeds));
            }
        }
    }
    let (movie_id, infohash, seeds) = best.expect("a 1080p x264 torrent");
    let tref = yts.torrent_ref(movie_id, &infohash).await.unwrap();
    println!("movie {movie_id} {:?} {infohash} seeds={seeds}", tref.title);

    let engine = TorrentEngine::new(EngineConfig {
        allowed_torrent_hosts: allowed,
        ..EngineConfig::new(tmp.path().join("cache"), local_base.clone())
    })
    .await
    .unwrap();
    tokio::spawn(stream::serve(
        listener,
        stream::router(stream::ServerState {
            images,
            torrents: Some(Arc::clone(&engine)),
            subs_dir: None,
        }),
    ));
    let mut rx = engine.subscribe();

    let t0 = Instant::now();
    let session = engine
        .start_stream(StreamRequest {
            movie_id,
            infohash: infohash.clone(),
            title: tref.title,
            torrent_url: tref.torrent_url,
            video_codec: tref.video_codec,
            seeds: tref.seeds,
        })
        .await
        .unwrap();
    let t_session = t0.elapsed();
    println!(
        "start_stream: {:.1}s → {} ({} bytes) {}",
        t_session.as_secs_f64(),
        session.file_name,
        session.file_size_bytes,
        session.stream_url
    );

    // Time until 8 MB contiguous from byte 0 (before any read).
    let mut t_buffer = None;
    let mut phases = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(180), async {
        while let Ok(s) = rx.recv().await {
            if phases.last() != Some(&s.phase) {
                println!(
                    "  {:>5.1}s {:?} peers={} down={} KiB/s buffered={} KiB",
                    t0.elapsed().as_secs_f64(),
                    s.phase,
                    s.peers,
                    s.down_speed_bps / 1024,
                    s.buffered_ahead_bytes / 1024
                );
                phases.push(s.phase);
            }
            if s.buffered_ahead_bytes >= TARGET {
                t_buffer = Some(t0.elapsed());
                break;
            }
        }
    })
    .await;
    match t_buffer {
        Some(t) => println!("8 MB buffered after {:.1}s", t.as_secs_f64()),
        None => println!("8 MB NOT buffered within 180s"),
    }

    let client = reqwest::Client::new();
    for range in ["bytes=0-1000".to_string(), "bytes=-1000000".to_string()] {
        let t = Instant::now();
        let resp = tokio::time::timeout(
            Duration::from_secs(180),
            client
                .get(&session.stream_url)
                .header("Range", &range)
                .send(),
        )
        .await
        .expect("range request timed out")
        .unwrap();
        let status = resp.status().as_u16();
        let content_range = resp
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let len = resp.bytes().await.unwrap().len();
        println!(
            "{range}: {status} {content_range} ({len} bytes) in {:.1}s",
            t.elapsed().as_secs_f64()
        );
        assert_eq!(status, 206);
    }

    engine.stop_stream(&infohash).await.unwrap();
    assert!(t_buffer.is_some(), "8 MB not buffered in time");
}
