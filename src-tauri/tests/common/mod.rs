//! Shared helpers for the torrent integration tests: a local librqbit seeder (no DHT,
//! no trackers) with a YTS-like folder, HTTP helpers and file checks.
#![allow(dead_code)]

use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{
    create_torrent, AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerOptions, Session,
    SessionOptions,
};

pub const VIDEO_NAME: &str = "Test.Movie.2024.1080p.WEBRip.x264.mp4";
pub const VIDEO_LEN: usize = 6 * 1024 * 1024 + 12_345;
pub const PIECE_LEN: u32 = 64 * 1024;

pub fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
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

pub struct Seeder {
    pub _dir: tempfile::TempDir,
    pub session: Arc<Session>,
    pub addr: SocketAddr,
    pub torrent_bytes: bytes::Bytes,
    pub infohash: String,
    pub video: Vec<u8>,
}

/// Creates a YTS-like folder (video + .txt + .jpg), its torrent and a seeding session.
pub async fn start_seeder() -> Seeder {
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
        session,
        torrent_bytes,
        video,
    }
}

pub async fn get(url: &str, range: Option<&str>) -> (u16, reqwest::header::HeaderMap, Vec<u8>) {
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

pub fn file_in(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    walk(dir)
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == name))
}

pub fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
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

/// Files of `dir` that this process still has open (Linux: `/proc/self/fd`).
pub fn open_files_under(dir: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir("/proc/self/fd")
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| std::fs::read_link(e.path()).ok())
                .filter(|target| target.starts_with(dir))
                .collect()
        })
        .unwrap_or_default()
}
