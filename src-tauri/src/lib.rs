pub mod commands;
pub mod db;
pub mod error;
pub mod paths;
pub mod state;
pub mod stream;
pub mod subtitles;
pub mod torrent;
pub mod types;
pub mod yts;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::paths::AppPaths;
use crate::state::AppState;

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // try_init: ignore the error if a subscriber is already set.
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let paths = AppPaths::default_location()?;
            paths.ensure()?;
            tracing::info!(data_dir = %paths.data_dir.display(), "data directories ready");
            app.manage(AppState::new(paths));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
