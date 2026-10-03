//! Shared application state, registered with `tauri::Builder::manage`.

use crate::paths::AppPaths;

/// Grows with each phase (YTS client, torrent session, stream server, DB).
#[derive(Debug)]
pub struct AppState {
    pub paths: AppPaths,
}

impl AppState {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }
}
