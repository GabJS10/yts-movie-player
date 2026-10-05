//! User data directory layout: `<data>/{cache,library}`, where `<data>` is
//! `~/.local/share/yts-player` on Linux (`$XDG_DATA_HOME`) and
//! `%LOCALAPPDATA%\yts-player` on Windows (local, not roaming: the cache and the downloads
//! must not travel with the profile). `YTS_PLAYER_DATA_DIR` replaces it on every platform
//! (E2E tests).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

const APP_DIR_NAME: &str = "yts-player";
/// Environment variable that replaces the data folder (and puts the logs in `<it>/logs`).
pub const DATA_DIR_ENV: &str = "YTS_PLAYER_DATA_DIR";

/// The override of [`DATA_DIR_ENV`], if set and not empty.
pub fn data_dir_override() -> Option<PathBuf> {
    override_from(std::env::var_os(DATA_DIR_ENV))
}

fn override_from(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The platform folder that holds ours.
fn platform_base() -> Option<PathBuf> {
    if cfg!(windows) {
        dirs::data_local_dir()
    } else {
        dirs::data_dir()
    }
}

/// The data folder: the override, else `<platform base>/yts-player`.
pub fn data_dir() -> Option<PathBuf> {
    resolve_data_dir(data_dir_override(), platform_base())
}

fn resolve_data_dir(over: Option<PathBuf>, base: Option<PathBuf>) -> Option<PathBuf> {
    over.or_else(|| base.map(|b| b.join(APP_DIR_NAME)))
}

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
    /// Layout rooted at [`data_dir`]. We use `dirs` instead of Tauri's `app_data_dir`,
    /// which would be named after the bundle id.
    pub fn default_location() -> AppResult<Self> {
        let dir = data_dir()
            .ok_or_else(|| AppError::Internal("could not determine user data directory".into()))?;
        Ok(Self::new(dir))
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
        assert_eq!(paths.cache_dir, tmp.path().join("yts-player").join("cache"));
        assert_eq!(
            paths.library_dir,
            tmp.path().join("yts-player").join("library")
        );

        paths.ensure().unwrap();
        assert!(paths.cache_dir.is_dir());
        assert!(paths.library_dir.is_dir());
        // Idempotent.
        paths.ensure().unwrap();
    }

    #[test]
    fn override_wins_over_the_platform_folder() {
        let base = Some(PathBuf::from("base"));
        assert_eq!(
            resolve_data_dir(None, base.clone()),
            Some(Path::new("base").join(APP_DIR_NAME))
        );
        let over = override_from(Some("e2e-data".into()));
        assert_eq!(
            resolve_data_dir(over, base.clone()),
            Some(PathBuf::from("e2e-data"))
        );
        // Empty means unset.
        assert_eq!(override_from(Some(OsString::new())), None);
        assert_eq!(resolve_data_dir(None, None), None);
    }

    #[test]
    fn default_location_ends_with_app_dir() {
        if data_dir_override().is_some() {
            return;
        }
        if let Ok(paths) = AppPaths::default_location() {
            assert!(paths.data_dir.ends_with(APP_DIR_NAME));
        }
    }
}
