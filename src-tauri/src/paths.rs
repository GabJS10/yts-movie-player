//! User data directory layout: `~/.local/share/yts-player/{cache,library}`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

const APP_DIR_NAME: &str = "yts-player";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// Root of all user data (DB, settings, torrent data).
    pub data_dir: PathBuf,
    /// Streaming cache, cleaned up by LRU.
    pub cache_dir: PathBuf,
    /// Saved downloads.
    pub library_dir: PathBuf,
}

impl AppPaths {
    /// Layout rooted at the platform data dir (`$XDG_DATA_HOME` or `~/.local/share` on Linux).
    /// We use `dirs` instead of Tauri's `app_data_dir`, which would be named after the bundle id.
    pub fn default_location() -> AppResult<Self> {
        let base = dirs::data_dir()
            .ok_or_else(|| AppError::Internal("could not determine user data directory".into()))?;
        Ok(Self::new(base.join(APP_DIR_NAME)))
    }

    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        let data_dir = data_dir.as_ref().to_path_buf();
        Self {
            cache_dir: data_dir.join("cache"),
            library_dir: data_dir.join("library"),
            data_dir,
        }
    }

    /// Creates every directory of the layout if missing.
    pub fn ensure(&self) -> AppResult<()> {
        for dir in [&self.data_dir, &self.cache_dir, &self.library_dir] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_and_ensure() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path().join(APP_DIR_NAME));
        assert_eq!(paths.cache_dir, tmp.path().join("yts-player/cache"));
        assert_eq!(paths.library_dir, tmp.path().join("yts-player/library"));

        paths.ensure().unwrap();
        assert!(paths.cache_dir.is_dir());
        assert!(paths.library_dir.is_dir());
        // Idempotent.
        paths.ensure().unwrap();
    }

    #[test]
    fn default_location_ends_with_app_dir() {
        if let Ok(paths) = AppPaths::default_location() {
            assert!(paths.data_dir.ends_with(APP_DIR_NAME));
        }
    }
}
