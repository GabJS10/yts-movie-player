//! Shared application state, registered with `tauri::Builder::manage`.

use std::sync::Arc;

use crate::images::ImageStore;
use crate::paths::AppPaths;
use crate::yts::YtsClient;

/// Grows with each phase (torrent session, DB…).
pub struct AppState {
    pub paths: AppPaths,
    /// Port of the local HTTP server (`stream.rs`).
    pub server_port: u16,
    pub images: Arc<ImageStore>,
    pub yts: YtsClient,
}
