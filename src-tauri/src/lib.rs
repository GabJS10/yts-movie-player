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

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::images::ImageStore;
use crate::paths::AppPaths;
use crate::state::AppState;
use crate::yts::{YtsClient, YtsConfig};

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // try_init: ignore the error if a subscriber is already set.
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn build_state() -> Result<AppState, Box<dyn std::error::Error>> {
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
    let yts = YtsClient::new(config, Arc::clone(&images))?;

    let router = stream::router(stream::ServerState {
        images: Arc::clone(&images),
    });
    tauri::async_runtime::spawn(stream::serve(listener, router));
    tracing::info!(%local_base, "local HTTP server listening");

    Ok(AppState {
        paths,
        server_port,
        images,
        yts,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(build_state()?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_movies,
            commands::get_movie,
            commands::get_suggestions,
            commands::get_api_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
