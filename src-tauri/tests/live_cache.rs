//! Manual disk-usage check against the real YTS swarm (needs internet, downloads ~100 MB+).
//! Run with: `cargo test --test live_cache -- --ignored --nocapture`
//!
//! Streams a real torrent for a while and compares, for the cache folder: apparent size
//! (what `ls -l`/`du --apparent-size` show), allocated size (what `du` shows) and the drop
//! of free space (`df`). Then checks what deleting the files frees while the torrent is
//! still paused in the session vs. after evicting it through the engine.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use yts_player_lib::cache::{allocated_size, free_disk_bytes, CacheManager, Evictor};
use yts_player_lib::images::ImageStore;
use yts_player_lib::torrent::{EngineConfig, StreamRequest, TorrentEngine};
use yts_player_lib::types::{ListMoviesParams, Quality, SortBy, VideoCodec};
use yts_player_lib::yts::{YtsClient, YtsConfig};

const MB: u64 = 1024 * 1024;

fn apparent_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    std::fs::read_dir(path)
        .map(|rd| rd.flatten().map(|e| apparent_size(&e.path())).sum())
        .unwrap_or(0)
}

fn report(label: &str, dir: &Path, free_before: u64) {
    let free = free_disk_bytes(dir.parent().unwrap_or(dir));
    println!(
        "{label:<38} apparent={:>6} MB  allocated(du)={:>5} MB  df drop={:>5} MB",
        apparent_size(dir) / MB,
        allocated_size(dir) / MB,
        free_before.saturating_sub(free) as i64 / MB as i64,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs internet and downloads real torrent data"]
async fn live_disk_usage_and_eviction() {
    // Same filesystem as the real cache (not /tmp, which may be tmpfs).
    let base = dirs::data_dir().unwrap().join("yts-player-live-cache-test");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let cache_dir = base.join("cache");

    let config = YtsConfig::default();
    let allowed = ImageStore::default_allowed_hosts(&config.base_urls);
    let images =
        Arc::new(ImageStore::new(base.join("img"), allowed.clone(), "http://127.0.0.1:1").unwrap());
    let yts = YtsClient::new(config, images).unwrap();
    let page = yts
        .list_movies(&ListMoviesParams {
            limit: Some(10),
            sort_by: Some(SortBy::Seeds),
            ..Default::default()
        })
        .await
        .unwrap();
    let mut best = None;
    for m in &page.movies {
        for t in yts.get_movie(m.id).await.unwrap().torrents {
            if t.quality == Quality::P1080
                && t.video_codec == VideoCodec::X264
                && best.as_ref().is_none_or(|(_, _, s)| t.seeds > *s)
            {
                best = Some((m.id, t.infohash.clone(), t.seeds));
            }
        }
    }
    let (movie_id, infohash, _) = best.expect("a 1080p x264 torrent");
    let tref = yts.torrent_ref(movie_id, &infohash).await.unwrap();

    let free_before = free_disk_bytes(&base);
    let engine = TorrentEngine::new(EngineConfig {
        allowed_torrent_hosts: allowed,
        ..EngineConfig::new(cache_dir.clone(), "http://127.0.0.1:1".into())
    })
    .await
    .unwrap();
    let session = engine
        .start_stream(StreamRequest {
            movie_id,
            infohash: infohash.clone(),
            title: tref.title.clone(),
            torrent_url: tref.torrent_url,
            video_codec: tref.video_codec,
            seeds: tref.seeds,
        })
        .await
        .unwrap();
    println!(
        "{} — {} ({} MB)",
        tref.title,
        session.file_name,
        session.file_size_bytes / MB
    );
    let dir = engine.torrent_dir(&infohash);
    for s in [5u64, 30, 60] {
        tokio::time::sleep(Duration::from_secs(if s == 5 {
            5
        } else {
            25 + s / 60 * 5
        }))
        .await;
        report(&format!("streaming, ~{s}s"), &dir, free_before);
    }

    engine.stop_stream(&infohash).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    report("stopped (paused in session)", &dir, free_before);

    // Deleting the video by hand while the paused torrent is still in the session (what an
    // `rm` with the app open does): the name goes away but the space doesn't.
    let video = walk_largest(&dir);
    let allocated_video = allocated_size(&video);
    std::fs::remove_file(&video).unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!(
        "rm video ({} MB allocated) while paused: du={} MB, df drop={} MB",
        allocated_video / MB,
        allocated_size(&dir) / MB,
        free_before.saturating_sub(free_disk_bytes(&base)) / MB
    );

    // Evicting through the engine removes the torrent from the session: files are closed.
    let cache = CacheManager::new(
        cache_dir.clone(),
        Arc::clone(&engine) as Arc<dyn Evictor>,
        0,
    );
    cache.enforce_limit().await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    println!(
        "after evict: folder exists={}, df drop={} MB",
        dir.exists(),
        free_before.saturating_sub(free_disk_bytes(&base)) as i64 / MB as i64
    );
    let _ = std::fs::remove_dir_all(&base);
}

fn walk_largest(dir: &Path) -> std::path::PathBuf {
    let mut best = (0, dir.to_path_buf());
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let len = p.metadata().map(|m| m.len()).unwrap_or(0);
                if len > best.0 {
                    best = (len, p);
                }
            }
        }
    }
    best.1
}
