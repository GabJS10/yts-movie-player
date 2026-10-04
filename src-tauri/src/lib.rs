pub mod cache;
pub mod commands;
pub mod db;
pub mod downloads;
pub mod error;
pub mod external_player;
pub mod images;
pub mod lifecycle;
pub mod paths;
pub mod recommend;
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
    // The DB, the DHT state, the settings, `subs/` and the image cache always live in the
    // default location; `cacheDir` and `downloadsDir` are chosen apart (applied on the fly).
    let app_paths = AppPaths::default_location()?;
    std::fs::create_dir_all(&app_paths.data_dir)?;
    let db = Db::open(&app_paths.data_dir.join(db::DB_FILE))?;
    let settings = Arc::new(tauri::async_runtime::block_on(SettingsStore::load(
        db.clone(),
        &settings::defaults(&app_paths.data_dir),
    ))?);
    let current = settings.get();

    let paths = app_paths.clone();
    paths.ensure()?;
    // A configured folder on a disk that is not mounted is not created: the cache falls
    // back to the default one (with a warning) and downloads there are `unavailable`.
    let configured_cache = std::path::PathBuf::from(&current.cache_dir);
    let (cache_dir, cache_available) =
        cache::effective_cache_dir(&configured_cache, &paths.cache_dir);
    tracing::info!(
        data_dir = %paths.data_dir.display(),
        cache_dir = %cache_dir.display(),
        cache_available,
        downloads_dir = %current.downloads_dir,
        "data directories ready"
    );

    let listener = tauri::async_runtime::block_on(stream::bind())?;
    let server_port = listener.local_addr()?.port();
    let local_base = format!("http://127.0.0.1:{server_port}");

    let config = YtsConfig {
        // E2E: `YTS_PLAYER_API_BASE_URLS` points at a fake YTS server for this run.
        base_urls: settings::api_base_urls_override(
            std::env::var(settings::API_BASE_URLS_ENV).ok().as_deref(),
        )
        .inspect(|urls| tracing::info!(?urls, "API base URLs from the environment"))
        .unwrap_or_else(|| current.api_base_urls.clone()),
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

    // E2E: `YTS_PLAYER_NO_DHT=1` keeps the torrents off the public swarm.
    let no_dht = torrent::no_dht(std::env::var(torrent::NO_DHT_ENV).ok().as_deref());
    if no_dht {
        tracing::info!("no DHT and no public trackers ({})", torrent::NO_DHT_ENV);
    }
    let engine_config = EngineConfig {
        dht: !no_dht,
        trackers: if no_dht {
            Vec::new()
        } else {
            torrent::TRACKERS.iter().map(|t| t.to_string()).collect()
        },
        dht_state_file: Some(app_paths.data_dir.join("dht.json")),
        allowed_torrent_hosts: allowed_hosts,
        buffer_target_bytes: current.buffer_target_bytes,
        download_limit_bps: kbps_to_bps(current.down_limit_kbps),
        upload_limit_bps: kbps_to_bps(current.up_limit_kbps),
        // `listenPort` is read only here: it applies on restart.
        listen_addr: current
            .listen_port
            .map(|port| (std::net::Ipv6Addr::UNSPECIFIED, port).into()),
        ..EngineConfig::new(cache_dir.clone(), local_base.clone())
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

    let downloads = DownloadManager::new(DownloadsConfig::new(
        db.clone(),
        Arc::clone(&images),
        Arc::clone(&torrents),
        std::path::PathBuf::from(&current.downloads_dir),
        current.seed_after_download,
    ));
    // Before the cache cleanup: downloads still in `cache/` are moved out first.
    tauri::async_runtime::block_on(downloads.restore())?;
    tauri::async_runtime::block_on(async {
        downloads.resume_all();
        downloads.spawn_tick(downloads::TICK_INTERVAL);
    });
    forward_download_changes(app.clone(), &downloads);
    forward_errors(app.clone(), torrents.subscribe_errors());
    forward_errors(app.clone(), downloads.subscribe_errors());
    forward_move_progress(app.clone(), &downloads);

    let cache = Arc::new(CacheManager::new(
        cache_dir.clone(),
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
    let server_stop = tokio_util::sync::CancellationToken::new();
    {
        let (app, stop) = (app.clone(), server_stop.clone());
        tauri::async_runtime::spawn(async move {
            // Stopped on purpose (closing) is fine; anything else leaves the front without
            // images, streams or subtitles.
            let failure = stream::serve_until(listener, router, stop.clone()).await;
            if !stop.is_cancelled() {
                let err = error::AppError::Internal(format!(
                    "local HTTP server stopped: {}",
                    failure.map(|e| e.to_string()).unwrap_or_default()
                ));
                emit_error(&app, &err, None);
            }
        });
    }
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
        recommender: recommend::Recommender::default(),
        server_stop,
    })
}

fn emit_error(app: &AppHandle, err: &error::AppError, infohash: Option<String>) {
    let payload = types::BackgroundError::new(err, infohash);
    if let Err(e) = app.emit(events::APP_ERROR, &payload) {
        tracing::warn!(error = %e, "could not emit app error");
    }
}

/// Re-emits background failures as `app://error`.
fn forward_errors(
    app: AppHandle,
    mut rx: tokio::sync::broadcast::Receiver<types::BackgroundError>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(payload) => {
                    if let Err(e) = app.emit(events::APP_ERROR, &payload) {
                        tracing::warn!(error = %e, "could not emit app error");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Re-emits `move_downloads` progress as `downloads://move-progress`.
fn forward_move_progress(app: AppHandle, downloads: &DownloadManager) {
    let mut rx = downloads.subscribe_moves();
    tauri::async_runtime::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(progress) => {
                    if let Err(e) = app.emit(events::MOVE_PROGRESS, &progress) {
                        tracing::warn!(error = %e, "could not emit move progress");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// How often `cacheDir` is checked (unmounted → default folder, back → configured one).
const CACHE_DIR_CHECK: std::time::Duration = std::time::Duration::from_secs(30);

/// Watches `cacheDir` and reports a fallback to the default folder with `app://error`.
/// At startup the warning waits a little so the WebView is listening.
fn watch_cache_dir(app: AppHandle, startup_warning: Option<error::AppError>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let emit = |err: &error::AppError| emit_error(&app, err, None);
        if let Some(warning) = &startup_warning {
            emit(warning);
        }
        let mut ticker = tokio::time::interval(CACHE_DIR_CHECK);
        loop {
            ticker.tick().await;
            let state = app.state::<AppState>();
            if let Some(warning) = commands::apply_cache_dir(&state) {
                tracing::warn!(error = %warning, "cache folder not available");
                emit(&warning);
            }
        }
    });
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
            let state = build_state(app.handle().clone())?;
            let configured = std::path::PathBuf::from(state.settings.get().cache_dir);
            let warning = (state.torrents.cache_dir() != configured).then(|| {
                commands::cache_fallback_warning(&configured, &state.torrents.cache_dir())
            });
            if let Some(w) = &warning {
                tracing::warn!(error = %w, "cache folder not available at startup");
            }
            app.manage(state);
            watch_cache_dir(app.handle().clone(), warning);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_movies,
            commands::get_movie,
            commands::get_suggestions,
            commands::get_api_status,
            commands::get_featured,
            commands::get_home_profile,
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
            commands::move_downloads,
            commands::cancel_move_downloads,
            commands::open_trailer_window,
        ])
        // Closing the main window closes the app (a trailer window left open does not keep
        // it alive).
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };
                tauri::async_runtime::block_on(lifecycle::shutdown(
                    &state.downloads,
                    &state.torrents,
                    &state.db,
                    &state.server_stop,
                    lifecycle::SHUTDOWN_TIMEOUT,
                ));
            }
        });
}
