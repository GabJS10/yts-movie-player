//! Closing the app in order, with a time limit: downloads saved (and a move cancelled),
//! torrents stopped (librqbit closes the files and the listener), the local HTTP server
//! stopped (port freed) and the DB's WAL written back. Whatever is left after the limit is
//! cut by the process exit; nothing here can hang the close.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::downloads::DownloadManager;
use crate::torrent::TorrentEngine;

pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Returns whether everything finished within `limit`.
pub async fn shutdown(
    downloads: &DownloadManager,
    engine: &TorrentEngine,
    db: &Db,
    server: &CancellationToken,
    limit: Duration,
) -> bool {
    let started = tokio::time::Instant::now();
    let steps = async {
        downloads.shutdown().await;
        engine.shutdown().await;
        server.cancel();
        if let Err(e) = db.checkpoint().await {
            tracing::warn!(error = %e, "could not checkpoint the database");
        }
    };
    let done = tokio::time::timeout(limit, steps).await.is_ok();
    // The server stops even if an earlier step ran out of time.
    server.cancel();
    if done {
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            "shutdown complete"
        );
    } else {
        tracing::warn!(?limit, "shutdown timed out, exiting anyway");
    }
    done
}
