//! Shared application state, registered with `tauri::Builder::manage`.

use std::sync::Arc;

use crate::cache::CacheManager;
use crate::db::Db;
use crate::downloads::DownloadManager;
use crate::images::ImageStore;
use crate::paths::AppPaths;
use crate::settings::SettingsStore;
use crate::subtitles::SubtitlesClient;
use crate::torrent::TorrentEngine;
use crate::yts::YtsClient;

pub struct AppState {
    /// Default layout (`~/.local/share/yts-player`): DB, `subs/`, default cache and library.
    pub paths: AppPaths,
    /// Port of the local HTTP server (`stream.rs`).
    pub server_port: u16,
    pub db: Db,
    pub settings: Arc<SettingsStore>,
    pub images: Arc<ImageStore>,
    pub yts: YtsClient,
    pub torrents: Arc<TorrentEngine>,
    pub cache: Arc<CacheManager>,
    pub subtitles: SubtitlesClient,
    pub downloads: Arc<DownloadManager>,
}
