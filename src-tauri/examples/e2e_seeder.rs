//! Torrent seeder for the E2E tests (`e2e/`), with no internet.
//!
//! ```text
//! cargo run --example e2e_seeder -- <video file> [--torrent <out.torrent>] [--port <peer port>]
//! ```
//!
//! Creates a `.torrent` for the file (default `<file>.torrent`) whose only tracker is a tiny
//! HTTP tracker served by this process on 127.0.0.1, seeds the file on 127.0.0.1 and prints
//! one JSON line on stdout: `{"infohash": "...", "torrentPath": "...", "port": <peer port>}`.
//! The app, started with `YTS_PLAYER_NO_DHT=1`, finds the seeder through that tracker.
//! Logs go to stderr. Seeds until SIGTERM or Ctrl+C.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use axum::routing::get;
use axum::Router;
use librqbit::spawn_utils::BlockingSpawner;
use librqbit::{
    create_torrent, AddTorrent, AddTorrentOptions, CreateTorrentOptions, ListenerOptions, Session,
    SessionOptions,
};

type Error = Box<dyn std::error::Error + Send + Sync>;

struct Args {
    file: PathBuf,
    torrent: Option<PathBuf>,
    port: u16,
}

fn usage() -> Error {
    "usage: e2e_seeder <video file> [--torrent <out.torrent>] [--port <peer port>]".into()
}

fn parse_args() -> Result<Args, Error> {
    let mut args = std::env::args().skip(1);
    let (mut file, mut torrent, mut port) = (None, None, 0);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--torrent" => torrent = Some(PathBuf::from(args.next().ok_or_else(usage)?)),
            "--port" => port = args.next().ok_or_else(usage)?.parse()?,
            "-h" | "--help" => return Err(usage()),
            _ if file.is_none() => file = Some(PathBuf::from(arg)),
            _ => return Err(usage()),
        }
    }
    Ok(Args {
        file: file.ok_or_else(usage)?,
        torrent,
        port,
    })
}

/// Tracker announce response: every client gets the seeder (compact peer list, BEP 23).
fn announce_response(peer: SocketAddr) -> Vec<u8> {
    let mut body = b"d8:completei1e10:incompletei0e8:intervali30e5:peers6:".to_vec();
    if let SocketAddr::V4(v4) = peer {
        body.extend_from_slice(&v4.ip().octets());
        body.extend_from_slice(&v4.port().to_be_bytes());
    }
    body.push(b'e');
    body
}

async fn run(args: Args) -> Result<(), Error> {
    let file = std::fs::canonicalize(&args.file)?;
    if !file.is_file() {
        return Err(format!("{} is not a file", file.display()).into());
    }
    let dir = file
        .parent()
        .ok_or("file has no parent folder")?
        .to_path_buf();
    let torrent_path = args.torrent.unwrap_or_else(|| {
        let mut name = file.as_os_str().to_owned();
        name.push(".torrent");
        PathBuf::from(name)
    });

    // Tracker first: its URL goes inside the .torrent.
    let tracker = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let tracker_url = format!("http://{}/announce", tracker.local_addr()?);

    let created = create_torrent(
        &file,
        CreateTorrentOptions {
            name: None,
            trackers: vec![tracker_url.clone()],
            piece_length: None,
        },
        &BlockingSpawner::new(1),
    )
    .await?;
    let bytes = created.as_bytes()?;
    std::fs::write(&torrent_path, &bytes)?;
    let infohash = created.info_hash().as_string();

    let session = Session::new_with_opts(
        dir.clone(),
        SessionOptions {
            dht: None,
            // The seeder does not need to announce: the tracker always answers with it.
            disable_trackers: true,
            disable_local_service_discovery: true,
            listen: Some(ListenerOptions {
                listen_addr: (Ipv4Addr::LOCALHOST, args.port).into(),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await?;
    let handle = session
        .add_torrent(
            AddTorrent::from_bytes(bytes),
            Some(AddTorrentOptions {
                output_folder: Some(dir.to_string_lossy().into_owned()),
                overwrite: true,
                ..Default::default()
            }),
        )
        .await?
        .into_handle()
        .ok_or("torrent was not added")?;
    tokio::time::timeout(Duration::from_secs(600), handle.wait_until_completed())
        .await
        .map_err(|_| "timed out checking the file")??;
    let peer = session.listen_addr().ok_or("seeder is not listening")?;

    let router = Router::new().route(
        "/announce",
        get(move || async move { announce_response(peer) }),
    );
    tokio::spawn(async move {
        if let Err(e) = axum::serve(tracker, router).await {
            eprintln!("e2e_seeder: tracker stopped: {e}");
        }
    });

    eprintln!(
        "e2e_seeder: seeding {} ({infohash}) on {peer}, tracker {tracker_url}",
        file.display()
    );
    println!(
        "{}",
        serde_json::json!({
            "infohash": infohash,
            "torrentPath": torrent_path,
            "port": peer.port(),
        })
    );

    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    eprintln!("e2e_seeder: stopping");
    session.stop().await;
    Ok(())
}

#[tokio::main]
async fn main() {
    let result = match parse_args() {
        Ok(args) => run(args).await,
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        eprintln!("e2e_seeder: {e}");
        std::process::exit(1);
    }
}
