//! Performance check (phase 7), run by hand: memory and CPU of the app side with 4
//! downloads and 1 stream at the same time, and background tasks left after the shutdown.
//!
//! ```text
//! cargo build --release --example e2e_seeder
//! cargo test --release --test perf -- --ignored --nocapture
//! ```
//!
//! Measured on 2026-10-04 (release, 4 × 96 MiB downloads + 1 stream, 12 MiB/s total limit):
//! RSS 6 MiB before → 22.6 MiB max; 43 % of one core on average (piece hashing at ~10
//! MiB/s plus the player reading the stream as fast as it arrives); shutdown 1.4 s; 0
//! background tasks left.
//!
//! The seeders are separate processes (`examples/e2e_seeder`), so only the app side
//! (engine, download manager, local HTTP server, DB) is measured.

// Reads /proc and sends SIGTERM.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};
use yts_player_lib::db::{self, Db};
use yts_player_lib::downloads::{DownloadManager, DownloadsConfig, TorrentInfo};
use yts_player_lib::images::ImageStore;
use yts_player_lib::stream;
use yts_player_lib::torrent::{EngineConfig, StreamRequest, TorrentEngine};
use yts_player_lib::types::{
    DownloadState, MovieDetail, MovieSummary, Quality, StreamPhase, VideoCodec,
};

const MIB: usize = 1024 * 1024;
const FILE_LEN: usize = 96 * MIB;
/// Total download limit, so everything is still running while measuring.
const LIMIT_BPS: u32 = 12 * MIB as u32;
const SAMPLE_FOR: Duration = Duration::from_secs(20);

fn seeder_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.parent()
        .and_then(|d| d.parent())
        .unwrap()
        .join("examples/e2e_seeder")
}

struct Seeder {
    child: Child,
    infohash: String,
    torrent: Vec<u8>,
}

impl Drop for Seeder {
    fn drop(&mut self) {
        // SAFETY: kill(2) on our own child.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        let _ = self.child.wait();
    }
}

fn start_seeder(dir: &Path, n: u64) -> Seeder {
    let file = dir.join(format!("movie{n}.mp4"));
    // Different content per file (different infohash), written in 1 MiB chunks.
    let mut data = Vec::with_capacity(FILE_LEN);
    let mut x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    while data.len() < FILE_LEN {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        data.extend_from_slice(&x.to_le_bytes());
    }
    std::fs::write(&file, &data).unwrap();
    let mut child = Command::new(seeder_bin())
        .arg(&file)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let info: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    Seeder {
        infohash: info["infohash"].as_str().unwrap().to_owned(),
        torrent: std::fs::read(info["torrentPath"].as_str().unwrap()).unwrap(),
        child,
    }
}

/// (resident memory in MiB, CPU seconds used by this process).
fn proc_sample() -> (f64, f64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let rss_kb: f64 = status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    // Fields after the command name (which is in parentheses).
    let rest = &stat[stat.rfind(')').unwrap() + 2..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let ticks: f64 = fields[11].parse::<f64>().unwrap() + fields[12].parse::<f64>().unwrap();
    // SAFETY: sysconf has no preconditions.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    (rss_kb / 1024.0, ticks / hz)
}

fn movie(id: u64) -> MovieSummary {
    MovieSummary {
        id,
        imdb_code: format!("tt{id}"),
        title: format!("Perf Movie {id}"),
        year: 2024,
        rating: 7.0,
        runtime_min: 100,
        genres: vec!["Drama".into()],
        cover_url: None,
        cover_large_url: None,
        background_url: None,
        qualities: vec![Quality::P1080],
        has_x264: true,
        max_seeds: 1,
    }
}

fn detail(id: u64) -> MovieDetail {
    MovieDetail {
        summary_fields: movie(id),
        summary: String::new(),
        language: "en".into(),
        mpa_rating: None,
        yt_trailer_code: None,
        trailer_url: None,
        screenshot_urls: vec![],
        cast: vec![],
        torrents: vec![],
        is_favorite: false,
        progress: None,
        download: None,
        offline: false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "performance numbers, run by hand"]
async fn four_downloads_and_a_stream() {
    let tasks = || {
        tokio::runtime::Handle::current()
            .metrics()
            .num_alive_tasks()
    };
    let tmp = tempfile::tempdir().unwrap();
    let seeds_dir = tmp.path().join("seeds");
    std::fs::create_dir_all(&seeds_dir).unwrap();
    let seeders: Vec<Seeder> = (1..=5).map(|n| start_seeder(&seeds_dir, n)).collect();
    let yts = MockServer::start().await;
    for s in &seeders {
        Mock::given(path(format!("/torrent/{}", s.infohash)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(s.torrent.clone(), "application/x-bittorrent"),
            )
            .mount(&yts)
            .await;
    }
    let url = |s: &Seeder| format!("{}/torrent/{}", yts.uri(), s.infohash);

    let baseline_tasks = tasks();
    let (rss0, _) = proc_sample();
    let root = tmp.path().join("app");
    let listener = stream::bind().await.unwrap();
    let local_base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let engine = TorrentEngine::new(EngineConfig {
        dht: false,
        trackers: Vec::new(),
        listen_addr: Some((Ipv4Addr::LOCALHOST, 0).into()),
        allowed_torrent_hosts: vec!["127.0.0.1".into()],
        download_limit_bps: std::num::NonZeroU32::new(LIMIT_BPS),
        ..EngineConfig::new(root.join("cache"), local_base.clone())
    })
    .await
    .unwrap();
    let images = Arc::new(ImageStore::new(root.join("img"), Vec::new(), local_base).unwrap());
    let server_stop = tokio_util::sync::CancellationToken::new();
    tokio::spawn(stream::serve_until(
        listener,
        stream::router(stream::ServerState {
            images: Arc::clone(&images),
            torrents: Some(Arc::clone(&engine)),
            subs_dir: None,
        }),
        server_stop.clone(),
    ));
    std::fs::create_dir_all(root.join("library")).unwrap();
    let db = Db::open(&root.join(db::DB_FILE)).unwrap();
    let downloads = DownloadManager::new(DownloadsConfig::new(
        db.clone(),
        images,
        Arc::clone(&engine),
        root.join("library"),
        false,
    ));
    downloads.restore().await.unwrap();
    downloads.spawn_tick(Duration::from_secs(1));

    for (i, s) in seeders[..4].iter().enumerate() {
        let id = i as u64 + 1;
        downloads
            .start(
                movie(id),
                &detail(id),
                TorrentInfo {
                    infohash: s.infohash.clone(),
                    quality: Quality::P1080,
                    video_codec: VideoCodec::X264,
                    size_bytes: FILE_LEN as u64,
                    title: format!("Perf Movie {id}"),
                    torrent_url: Some(url(s)),
                    seeds: 1,
                },
            )
            .await
            .unwrap();
    }
    // The stream: a player reading the whole file at its own pace.
    let stream_seeder = &seeders[4];
    let session = engine
        .start_stream(StreamRequest {
            movie_id: 5,
            infohash: stream_seeder.infohash.clone(),
            title: "Perf Stream".into(),
            torrent_url: Some(url(stream_seeder)),
            video_codec: VideoCodec::X264,
            seeds: 1,
        })
        .await
        .unwrap();
    let mut stats = engine.subscribe();
    let stream_url = session.stream_url.clone();
    let player = tokio::spawn(async move {
        let mut resp = reqwest::get(&stream_url).await.unwrap();
        let mut read = 0usize;
        while let Ok(Some(chunk)) = resp.chunk().await {
            read += chunk.len();
        }
        read
    });

    println!("\n  t(s)  RSS(MiB)  CPU(%)  descargas(MiB/s)  stream  tareas");
    let (_, mut cpu_prev) = proc_sample();
    let started = Instant::now();
    let mut t_prev = started;
    let (mut rss_max, mut cpu_sum, mut samples) = (0f64, 0f64, 0u32);
    while started.elapsed() < SAMPLE_FOR {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let (rss, cpu) = proc_sample();
        let now = Instant::now();
        let pct = 100.0 * (cpu - cpu_prev) / (now - t_prev).as_secs_f64();
        cpu_prev = cpu;
        t_prev = now;
        let down: u64 = downloads.list().iter().map(|d| d.down_speed_bps).sum();
        let mut phase = None;
        while let Ok(s) = stats.try_recv() {
            phase = Some(s.phase);
        }
        println!(
            "  {:>4.0}  {:>8.1}  {:>6.1}  {:>16.1}  {:>6}  {:>6}",
            started.elapsed().as_secs_f64(),
            rss,
            pct,
            down as f64 / MIB as f64,
            phase.map(|p| format!("{p:?}")).unwrap_or_default(),
            tasks()
        );
        rss_max = rss_max.max(rss);
        cpu_sum += pct;
        samples += 1;
    }
    let states: Vec<DownloadState> = downloads.list().iter().map(|d| d.state).collect();
    println!(
        "  RSS antes: {rss0:.1} MiB, máx: {rss_max:.1} MiB; CPU media: {:.1} % de un núcleo; estados: {states:?}",
        cpu_sum / samples as f64
    );

    let t = Instant::now();
    let done = yts_player_lib::lifecycle::shutdown(
        &downloads,
        &engine,
        &db,
        &server_stop,
        yts_player_lib::lifecycle::SHUTDOWN_TIMEOUT,
    )
    .await;
    println!("  cierre: {:?} (completo: {done})", t.elapsed());
    player.abort();
    drop((downloads, engine, db));
    tokio::time::sleep(Duration::from_secs(2)).await;
    let left = tasks();
    println!("  tareas: {baseline_tasks} antes, {left} después del cierre");
    assert!(done);
    // Wiremock's server tasks are part of both counts; nothing of ours may be left.
    assert!(
        left <= baseline_tasks,
        "{left} tasks left (baseline {baseline_tasks})"
    );
    let _ = StreamPhase::Done;
}
