pub mod cache;
pub mod commands;
pub mod db;
pub mod downloads;
pub mod error;
pub mod external_player;
pub mod images;
pub mod paths;
pub mod settings;
pub mod state;
pub mod stream;
pub mod subtitles;
pub mod torrent;
pub mod types;
pub mod vtt;
pub mod yts;

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tracing_subscriber::EnvFilter;

use crate::cache::CacheManager;
use crate::db::Db;
use crate::downloads::{DownloadManager, DownloadsConfig};
use crate::images::ImageStore;
use crate::paths::AppPaths;
use crate::settings::SettingsStore;
use crate::state::AppState;
use crate::subtitles::{SubtitlesClient, SubtitlesConfig};
use crate::torrent::{kbps_to_bps, EngineConfig, TorrentEngine};
use crate::types::events;
use crate::yts::{YtsClient, YtsConfig};

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // try_init: ignore the error if a subscriber is already set.
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn build_state(app: AppHandle) -> Result<AppState, Box<dyn std::error::Error>> {
    // The DB, the DHT state and the settings always live in the default location;
    // `dataDir` (applied at startup) only moves `cache/` and `library/`.
    let app_paths = AppPaths::default_location()?;
    std::fs::create_dir_all(&app_paths.data_dir)?;
    let db = Db::open(&app_paths.data_dir.join(db::DB_FILE))?;
    let settings = Arc::new(tauri::async_runtime::block_on(SettingsStore::load(
        db.clone(),
        &settings::defaults(&app_paths.data_dir),
    ))?);
    let current = settings.get();

    let paths = AppPaths::new(&current.data_dir);
    paths.ensure()?;
    tracing::info!(data_dir = %paths.data_dir.display(), "data directories ready");

    let listener = tauri::async_runtime::block_on(stream::bind())?;
    let server_port = listener.local_addr()?.port();
    let local_base = format!("http://127.0.0.1:{server_port}");

    let config = YtsConfig {
        base_urls: current.api_base_urls.clone(),
        ..YtsConfig::default()
    };
    let allowed_hosts = ImageStore::default_allowed_hosts(&config.base_urls);
    let images = Arc::new(ImageStore::new(
        paths.cache_dir.join(cache::IMG_DIR),
        allowed_hosts.clone(),
        local_base.clone(),
    )?);
    // Covers of saved movies must resolve after a restart.
    images.register(tauri::async_runtime::block_on(db.images())?);
    let yts = YtsClient::new(config, Arc::clone(&images))?;

    let engine_config = EngineConfig {
        dht_state_file: Some(app_paths.data_dir.join("dht.json")),
        allowed_torrent_hosts: allowed_hosts,
        buffer_target_bytes: current.buffer_target_bytes,
        download_limit_bps: kbps_to_bps(current.down_limit_kbps),
        upload_limit_bps: kbps_to_bps(current.up_limit_kbps),
        // `listenPort` is read only here: it applies on restart.
        listen_addr: current
            .listen_port
            .map(|port| (std::net::Ipv6Addr::UNSPECIFIED, port).into()),
        ..EngineConfig::new(paths.cache_dir.clone(), local_base.clone())
    };
    let torrents = match tauri::async_runtime::block_on(TorrentEngine::new(engine_config.clone())) {
        Ok(engine) => engine,
        Err(e) if engine_config.listen_addr.is_some() => {
            tracing::warn!(error = %e, port = ?current.listen_port, "could not listen on the configured port, using a random one");
            tauri::async_runtime::block_on(TorrentEngine::new(EngineConfig {
                listen_addr: None,
                ..engine_config
            }))?
        }
        Err(e) => return Err(e.into()),
    };
    tracing::info!(listen = ?torrents.listen_addr(), "torrent session ready");
    forward_torrent_stats(app.clone(), &torrents);

    let downloads = DownloadManager::new(DownloadsConfig {
        db: db.clone(),
        images: Arc::clone(&images),
        engine: Arc::clone(&torrents),
        library_dir: paths.library_dir.clone(),
        seed_after_download: current.seed_after_download,
        free_space: Arc::new(cache::free_disk_bytes),
    });
    // Before the cache cleanup: downloads still in `cache/` are moved out first.
    tauri::async_runtime::block_on(downloads.restore())?;
    tauri::async_runtime::block_on(async {
        downloads.resume_all();
        downloads.spawn_tick(downloads::TICK_INTERVAL);
    });
    forward_download_changes(app, &downloads);

    let cache = Arc::new(CacheManager::new(
        paths.cache_dir.clone(),
        paths.library_dir.clone(),
        Arc::clone(&torrents) as Arc<dyn cache::Evictor>,
        current.cache_limit_bytes,
    ));
    // Startup cleanup, then periodic (needs the runtime context for `tokio::spawn`).
    tauri::async_runtime::block_on(async { cache.spawn_periodic(cache::ENFORCE_INTERVAL) });

    // In the default location, like the DB: not part of the cache LRU.
    let subs_dir = app_paths.data_dir.join(subtitles::SUBS_DIR);
    let subtitles = SubtitlesClient::new(
        SubtitlesConfig::new(subs_dir.clone(), local_base.clone()),
        commands::subtitle_credentials(&current),
    )?;

    let router = stream::router(stream::ServerState {
        images: Arc::clone(&images),
        torrents: Some(Arc::clone(&torrents)),
        subs_dir: Some(subs_dir),
    });
    tauri::async_runtime::spawn(stream::serve(listener, router));
    tracing::info!(%local_base, "local HTTP server listening");

    Ok(AppState {
        paths,
        server_port,
        db,
        settings,
        images,
        yts,
        torrents,
        cache,
        subtitles,
        downloads,
    })
}

/// Re-emits download state changes as the `download://changed` event.
fn forward_download_changes(app: AppHandle, downloads: &DownloadManager) {
    let mut rx = downloads.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(change) => {
                    if let Err(e) = app.emit(events::DOWNLOAD_CHANGED, &change) {
                        tracing::warn!(error = %e, "could not emit download change");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
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
        .plugin(tauri_plugin_dialog::init())
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
            commands::list_favorites,
            commands::add_favorite,
            commands::remove_favorite,
            commands::save_progress,
            commands::get_progress,
            commands::list_continue_watching,
            commands::remove_progress,
            commands::get_settings,
            commands::update_settings,
            commands::get_storage_usage,
            commands::clear_cache,
            commands::search_subtitles,
            commands::load_subtitle,
            commands::load_subtitle_file,
            commands::get_subtitles_status,
            commands::start_download,
            commands::list_downloads,
            commands::pause_download,
            commands::resume_download,
            commands::remove_download,
            commands::open_download_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
