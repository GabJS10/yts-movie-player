pub mod commands;
pub mod db;
pub mod error;
pub mod images;
pub mod paths;
pub mod state;
pub mod stream;
pub mod subtitles;
pub mod torrent;
pub mod types;
pub mod yts;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tracing_subscriber::EnvFilter;

use crate::images::ImageStore;
use crate::paths::AppPaths;
use crate::state::AppState;
use crate::torrent::{EngineConfig, TorrentEngine};
use crate::types::events;
use crate::yts::{YtsClient, YtsConfig};

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // try_init: ignore the error if a subscriber is already set.
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn build_state(app: AppHandle) -> Result<AppState, Box<dyn std::error::Error>> {
    let paths = AppPaths::default_location()?;
    paths.ensure()?;
    tracing::info!(data_dir = %paths.data_dir.display(), "data directories ready");

    let listener = tauri::async_runtime::block_on(stream::bind())?;
    let server_port = listener.local_addr()?.port();
    let local_base = format!("http://127.0.0.1:{server_port}");

    let config = YtsConfig::default();
    let images = Arc::new(ImageStore::new(
        paths.cache_dir.join("img"),
        ImageStore::default_allowed_hosts(&config.base_urls),
        local_base.clone(),
    )?);
    let allowed_hosts = ImageStore::default_allowed_hosts(&config.base_urls);
    let yts = YtsClient::new(config, Arc::clone(&images))?;

    let torrents = tauri::async_runtime::block_on(TorrentEngine::new(EngineConfig {
        dht_state_file: Some(paths.data_dir.join("dht.json")),
        allowed_torrent_hosts: allowed_hosts,
        ..EngineConfig::new(paths.cache_dir.clone(), local_base.clone())
    }))?;
    forward_torrent_stats(app, &torrents);

    let router = stream::router(stream::ServerState {
        images: Arc::clone(&images),
        torrents: Some(Arc::clone(&torrents)),
    });
    tauri::async_runtime::spawn(stream::serve(listener, router));
    tracing::info!(%local_base, "local HTTP server listening");

    Ok(AppState {
        paths,
        server_port,
        images,
        yts,
        torrents,
    })
}

/// Re-emits engine stats as the `torrent://stats` event.
fn forward_torrent_stats(app: AppHandle, torrents: &TorrentEngine) {
    let mut rx = torrents.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(stats) => {
                    if let Err(e) = app.emit(events::TORRENT_STATS, &stats) {
                        tracing::warn!(error = %e, "could not emit torrent stats");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(build_state(app.handle().clone())?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_movies,
            commands::get_movie,
            commands::get_suggestions,
            commands::get_api_status,
            commands::start_stream,
            commands::stop_stream,
            commands::open_external_player,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
