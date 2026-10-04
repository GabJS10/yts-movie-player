//! SQLite (rusqlite) with versioned migrations (`PRAGMA user_version`).
//!
//! Every access goes through [`Db::call`], which runs the closure on the blocking pool so
//! the async runtime never waits on disk. Movies are stored as `MovieSummary` JSON with
//! the local server origin stripped (`/img/<hash>`); it is put back with the current
//! port when reading (see the URL conventions in `docs/IPC.md`).

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use url::Url;

use crate::error::{AppError, AppResult};
use crate::images::ImageStore;
use crate::types::{CastMember, ContinueItem, MovieDetail, MovieSummary, Progress};

pub const DB_FILE: &str = "yts-player.db";

/// From this fraction of the duration on, a movie counts as watched (`finished`).
pub const FINISHED_RATIO: f64 = 0.92;

/// Migration `i` takes the schema from version `i` to `i + 1`. Append only, never edit.
pub const MIGRATIONS: &[&str] = &[MIGRATION_1, MIGRATION_2];

const MIGRATION_1: &str = "
CREATE TABLE favorites (
    movie_id   INTEGER PRIMARY KEY,
    movie      TEXT NOT NULL,          -- MovieSummary JSON, local URLs without origin
    added_at   TEXT NOT NULL
);
CREATE TABLE progress (
    movie_id   INTEGER PRIMARY KEY,
    movie      TEXT NOT NULL,
    position_s REAL NOT NULL,
    duration_s REAL NOT NULL,
    finished   INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    seq        INTEGER NOT NULL        -- increases on every save (same-millisecond ties)
);
CREATE INDEX progress_by_update ON progress (finished, seq);
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL                -- JSON
);
-- hash → remote URL, so saved covers keep working after a restart.
CREATE TABLE images (
    hash TEXT PRIMARY KEY,
    url  TEXT NOT NULL
);
-- Used from phase 6.
CREATE TABLE downloads (
    infohash    TEXT PRIMARY KEY,
    movie_id    INTEGER NOT NULL,
    movie       TEXT NOT NULL,
    quality     TEXT NOT NULL,
    video_codec TEXT NOT NULL,
    state       TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL DEFAULT 0,
    path        TEXT,
    error       TEXT,
    added_at    TEXT NOT NULL
);
";

/// Phase 6: what a download needs to survive a restart without network.
const MIGRATION_2: &str = "
ALTER TABLE downloads ADD COLUMN title TEXT NOT NULL DEFAULT '';
ALTER TABLE downloads ADD COLUMN torrent_url TEXT;
ALTER TABLE downloads ADD COLUMN torrent BLOB;              -- .torrent bytes
ALTER TABLE downloads ADD COLUMN rel_path TEXT;             -- video path inside `path`
ALTER TABLE downloads ADD COLUMN downloaded_bytes INTEGER NOT NULL DEFAULT 0;
-- MovieDetail JSON saved when downloading (local URLs without origin), for offline use.
CREATE TABLE movie_details (
    movie_id   INTEGER PRIMARY KEY,
    detail     TEXT NOT NULL,
    saved_at   TEXT NOT NULL
);
";

/// A row of `downloads`.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadRow {
    pub infohash: String,
    pub movie_id: u64,
    /// Stored summary (local URLs without origin).
    pub movie: MovieSummary,
    pub quality: String,
    pub video_codec: String,
    /// "active" | "paused" | "done" | "error".
    pub state: String,
    pub size_bytes: u64,
    /// Library folder (absolute).
    pub path: String,
    pub error: Option<String>,
    pub added_at: String,
    pub title: String,
    pub torrent_url: Option<String>,
    pub torrent: Option<Vec<u8>>,
    pub rel_path: Option<String>,
    pub downloaded_bytes: u64,
}

const DOWNLOAD_COLUMNS: &str = "infohash, movie_id, movie, quality, video_codec, state, \
     size_bytes, path, error, added_at, title, torrent_url, torrent, rel_path, downloaded_bytes";

fn download_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(DownloadRow, String)> {
    let movie_json: String = r.get(2)?;
    Ok((
        DownloadRow {
            infohash: r.get(0)?,
            movie_id: r.get::<_, i64>(1)? as u64,
            movie: placeholder_movie(),
            quality: r.get(3)?,
            video_codec: r.get(4)?,
            state: r.get(5)?,
            size_bytes: r.get::<_, i64>(6)?.max(0) as u64,
            path: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            error: r.get(8)?,
            added_at: r.get(9)?,
            title: r.get(10)?,
            torrent_url: r.get(11)?,
            torrent: r.get(12)?,
            rel_path: r.get(13)?,
            downloaded_bytes: r.get::<_, i64>(14)?.max(0) as u64,
        },
        movie_json,
    ))
}

fn placeholder_movie() -> MovieSummary {
    MovieSummary {
        id: 0,
        imdb_code: String::new(),
        title: String::new(),
        year: 0,
        rating: 0.0,
        runtime_min: 0,
        genres: Vec::new(),
        cover_url: None,
        cover_large_url: None,
        background_url: None,
        qualities: Vec::new(),
        has_x264: false,
        max_seeds: 0,
    }
}

/// ISO 8601 UTC with milliseconds, e.g. `2026-10-03T12:00:00.123Z`.
const NOW: &str = "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";

/// Applies the pending migrations, each in its own transaction. Returns the final version.
pub fn migrate(conn: &mut Connection, migrations: &[&str]) -> AppResult<usize> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let current = usize::try_from(current).unwrap_or(0);
    if current > migrations.len() {
        return Err(AppError::Db(format!(
            "database schema v{current} is newer than this app (v{})",
            migrations.len()
        )));
    }
    for (idx, sql) in migrations.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (idx + 1) as i64)?;
        tx.commit()?;
        tracing::info!(version = idx + 1, "database migrated");
    }
    Ok(migrations.len())
}

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::init(Connection::open(path)?, MIGRATIONS)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?, MIGRATIONS)
    }

    fn init(mut conn: Connection, migrations: &[&str]) -> AppResult<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        // Returns the resulting mode as a row ("memory" for in-memory DBs).
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        migrate(&mut conn, migrations)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs `f` with the connection on the blocking thread pool.
    pub async fn call<T, F>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let mut conn = conn
                .lock()
                .map_err(|_| AppError::Db("connection lock poisoned".into()))?;
            f(&mut conn)
        })
        .await
        .map_err(|e| AppError::Internal(format!("db task: {e}")))?
    }

    // -- Image registry -----------------------------------------------------------------

    /// Every saved `hash → remote URL` (loaded into the `ImageStore` at startup).
    pub async fn images(&self) -> AppResult<Vec<(String, String)>> {
        self.call(|c| {
            let mut stmt = c.prepare("SELECT hash, url FROM images")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .await
    }

    // -- Mi lista -----------------------------------------------------------------------

    /// Re-adding keeps the original date (and position in the list) but refreshes the data.
    pub async fn add_favorite(&self, images: &ImageStore, movie: &MovieSummary) -> AppResult<()> {
        let (json, imgs) = stored(images, movie)?;
        let id = movie.id;
        self.call(move |c| {
            let tx = c.transaction()?;
            save_images(&tx, &imgs)?;
            tx.execute(
                &format!(
                    "INSERT INTO favorites (movie_id, movie, added_at) VALUES (?1, ?2, {NOW})
                     ON CONFLICT (movie_id) DO UPDATE SET movie = excluded.movie"
                ),
                params![id as i64, json],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn remove_favorite(&self, movie_id: u64) -> AppResult<()> {
        self.call(move |c| {
            c.execute(
                "DELETE FROM favorites WHERE movie_id = ?1",
                [movie_id as i64],
            )?;
            Ok(())
        })
        .await
    }

    /// Most recent first.
    pub async fn list_favorites(&self, local_base: &str) -> AppResult<Vec<MovieSummary>> {
        let rows: Vec<String> = self
            .call(|c| {
                let mut stmt =
                    c.prepare("SELECT movie FROM favorites ORDER BY added_at DESC, rowid DESC")?;
                let rows = stmt
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<_, _>>()?;
                Ok(rows)
            })
            .await?;
        Ok(rows
            .iter()
            .filter_map(|json| load_movie(json, local_base))
            .collect())
    }

    pub async fn is_favorite(&self, movie_id: u64) -> AppResult<bool> {
        self.call(move |c| {
            Ok(c.query_row(
                "SELECT 1 FROM favorites WHERE movie_id = ?1",
                [movie_id as i64],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
        })
        .await
    }

    // -- Continuar viendo ---------------------------------------------------------------

    pub async fn save_progress(
        &self,
        images: &ImageStore,
        movie: &MovieSummary,
        position_s: f64,
        duration_s: f64,
    ) -> AppResult<Progress> {
        let (position_s, duration_s) = validate_progress(position_s, duration_s)?;
        let finished = is_finished(position_s, duration_s);
        let (json, imgs) = stored(images, movie)?;
        let id = movie.id;
        self.call(move |c| {
            let tx = c.transaction()?;
            save_images(&tx, &imgs)?;
            tx.execute(
                &format!(
                    "INSERT INTO progress
                         (movie_id, movie, position_s, duration_s, finished, updated_at, seq)
                     VALUES (?1, ?2, ?3, ?4, ?5, {NOW},
                             (SELECT COALESCE(MAX(seq), 0) + 1 FROM progress))
                     ON CONFLICT (movie_id) DO UPDATE SET
                         movie = excluded.movie, position_s = excluded.position_s,
                         duration_s = excluded.duration_s, finished = excluded.finished,
                         updated_at = excluded.updated_at, seq = excluded.seq"
                ),
                params![id as i64, json, position_s, duration_s, finished],
            )?;
            let progress = get_progress(&tx, id)?
                .ok_or_else(|| AppError::Db("progress row vanished".into()))?;
            tx.commit()?;
            Ok(progress)
        })
        .await
    }

    pub async fn get_progress(&self, movie_id: u64) -> AppResult<Option<Progress>> {
        self.call(move |c| get_progress(c, movie_id)).await
    }

    /// Unfinished only, most recently updated first.
    pub async fn list_continue_watching(&self, local_base: &str) -> AppResult<Vec<ContinueItem>> {
        let rows: Vec<(String, Progress)> = self
            .call(|c| {
                let mut stmt = c.prepare(
                    "SELECT movie, movie_id, position_s, duration_s, finished, updated_at
                     FROM progress WHERE finished = 0
                     ORDER BY seq DESC",
                )?;
                let rows = stmt
                    .query_map([], |r| Ok((r.get(0)?, progress_from_row(r, 1)?)))?
                    .collect::<Result<_, _>>()?;
                Ok(rows)
            })
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(json, progress)| {
                Some(ContinueItem {
                    movie: load_movie(&json, local_base)?,
                    progress,
                })
            })
            .collect())
    }

    /// Every movie with progress (in progress and finished), most recent first.
    pub async fn progress_movies(&self, local_base: &str) -> AppResult<Vec<MovieSummary>> {
        let rows: Vec<String> = self
            .call(|c| {
                let mut stmt = c.prepare("SELECT movie FROM progress ORDER BY seq DESC")?;
                let rows = stmt
                    .query_map([], |r| r.get(0))?
                    .collect::<Result<_, _>>()?;
                Ok(rows)
            })
            .await?;
        Ok(rows
            .iter()
            .filter_map(|json| load_movie(json, local_base))
            .collect())
    }

    pub async fn remove_progress(&self, movie_id: u64) -> AppResult<()> {
        self.call(move |c| {
            c.execute(
                "DELETE FROM progress WHERE movie_id = ?1",
                [movie_id as i64],
            )?;
            Ok(())
        })
        .await
    }

    // -- Downloads ------------------------------------------------------------------------

    /// Inserts or replaces a download (the movie is stored without the local origin).
    pub async fn put_download(&self, images: &ImageStore, row: &DownloadRow) -> AppResult<()> {
        let (json, imgs) = stored(images, &row.movie)?;
        let row = row.clone();
        self.call(move |c| {
            let tx = c.transaction()?;
            save_images(&tx, &imgs)?;
            tx.execute(
                &format!(
                    "INSERT OR REPLACE INTO downloads ({DOWNLOAD_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"
                ),
                params![
                    row.infohash,
                    row.movie_id as i64,
                    json,
                    row.quality,
                    row.video_codec,
                    row.state,
                    row.size_bytes as i64,
                    row.path,
                    row.error,
                    row.added_at,
                    row.title,
                    row.torrent_url,
                    row.torrent,
                    row.rel_path,
                    row.downloaded_bytes as i64,
                ],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// Updates the fields that change while downloading.
    pub async fn update_download(
        &self,
        infohash: &str,
        state: &str,
        error: Option<String>,
        size_bytes: u64,
        downloaded_bytes: u64,
    ) -> AppResult<()> {
        let (infohash, state) = (infohash.to_owned(), state.to_owned());
        self.call(move |c| {
            c.execute(
                "UPDATE downloads SET state = ?2, error = ?3, size_bytes = ?4, downloaded_bytes = ?5
                 WHERE infohash = ?1",
                params![infohash, state, error, size_bytes as i64, downloaded_bytes as i64],
            )?;
            Ok(())
        })
        .await
    }

    /// Saves what the torrent resolved: `.torrent` bytes, video path and size.
    pub async fn set_download_torrent(
        &self,
        infohash: &str,
        torrent: Vec<u8>,
        rel_path: String,
        size_bytes: u64,
    ) -> AppResult<()> {
        let infohash = infohash.to_owned();
        self.call(move |c| {
            c.execute(
                "UPDATE downloads SET torrent = ?2, rel_path = ?3, size_bytes = ?4
                 WHERE infohash = ?1",
                params![infohash, torrent, rel_path, size_bytes as i64],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn set_download_path(&self, infohash: &str, path: &str) -> AppResult<()> {
        let (infohash, path) = (infohash.to_owned(), path.to_owned());
        self.call(move |c| {
            c.execute(
                "UPDATE downloads SET path = ?2 WHERE infohash = ?1",
                params![infohash, path],
            )?;
            Ok(())
        })
        .await
    }

    /// Every download, oldest first, with the current local origin on its movie.
    pub async fn list_downloads(&self, local_base: &str) -> AppResult<Vec<DownloadRow>> {
        let rows: Vec<(DownloadRow, String)> = self
            .call(|c| {
                let mut stmt = c.prepare(&format!(
                    "SELECT {DOWNLOAD_COLUMNS} FROM downloads ORDER BY added_at, rowid"
                ))?;
                let rows = stmt
                    .query_map([], download_from_row)?
                    .collect::<Result<_, _>>()?;
                Ok(rows)
            })
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(mut row, json)| {
                row.movie = load_movie(&json, local_base)?;
                Some(row)
            })
            .collect())
    }

    /// Deletes the download and, if it was the movie's last one, its saved detail.
    pub async fn remove_download(&self, infohash: &str) -> AppResult<()> {
        let infohash = infohash.to_owned();
        self.call(move |c| {
            let tx = c.transaction()?;
            let movie_id: Option<i64> = tx
                .query_row(
                    "SELECT movie_id FROM downloads WHERE infohash = ?1",
                    [&infohash],
                    |r| r.get(0),
                )
                .optional()?;
            tx.execute("DELETE FROM downloads WHERE infohash = ?1", [&infohash])?;
            if let Some(id) = movie_id {
                tx.execute(
                    "DELETE FROM movie_details WHERE movie_id = ?1
                     AND NOT EXISTS (SELECT 1 FROM downloads WHERE movie_id = ?1)",
                    [id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    // -- Offline movie details ------------------------------------------------------------

    pub async fn put_movie_detail(
        &self,
        images: &ImageStore,
        detail: &MovieDetail,
    ) -> AppResult<()> {
        let stored_detail = detail_to_stored(detail);
        let imgs: Vec<(String, String)> = detail_image_paths(&stored_detail)
            .filter_map(|p| {
                let hash = p.strip_prefix("/img/")?;
                Some((hash.to_owned(), images.remote_url(hash)?))
            })
            .collect();
        let json = serde_json::to_string(&stored_detail)
            .map_err(|e| AppError::Internal(format!("serializing movie detail: {e}")))?;
        let id = detail.summary_fields.id;
        self.call(move |c| {
            let tx = c.transaction()?;
            save_images(&tx, &imgs)?;
            tx.execute(
                &format!(
                    "INSERT OR REPLACE INTO movie_details (movie_id, detail, saved_at)
                     VALUES (?1, ?2, {NOW})"
                ),
                params![id as i64, json],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// The saved detail (current local origin, user fields cleared), if any.
    pub async fn movie_detail(
        &self,
        movie_id: u64,
        local_base: &str,
    ) -> AppResult<Option<MovieDetail>> {
        let json: Option<String> = self
            .call(move |c| {
                Ok(c.query_row(
                    "SELECT detail FROM movie_details WHERE movie_id = ?1",
                    [movie_id as i64],
                    |r| r.get(0),
                )
                .optional()?)
            })
            .await?;
        Ok(
            json.and_then(|j| match serde_json::from_str::<MovieDetail>(&j) {
                Ok(d) => Some(detail_from_stored(d, local_base)),
                Err(e) => {
                    tracing::warn!(movie_id, error = %e, "unreadable saved movie detail");
                    None
                }
            }),
        )
    }

    // -- Settings (raw rows; the logic lives in `settings.rs`) --------------------------

    pub async fn settings_rows(&self) -> AppResult<Vec<(String, String)>> {
        self.call(|c| {
            let mut stmt = c.prepare("SELECT key, value FROM settings")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn delete_settings(&self, keys: Vec<String>) -> AppResult<()> {
        self.call(move |c| {
            for key in &keys {
                c.execute("DELETE FROM settings WHERE key = ?1", [key])?;
            }
            Ok(())
        })
        .await
    }

    /// Upserts the given rows in one transaction.
    pub async fn put_settings(&self, rows: Vec<(String, String)>) -> AppResult<()> {
        self.call(move |c| {
            let tx = c.transaction()?;
            for (key, value) in &rows {
                tx.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                    params![key, value],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }
}

fn save_images(conn: &Connection, images: &[(String, String)]) -> AppResult<()> {
    for (hash, url) in images {
        conn.execute(
            "INSERT OR REPLACE INTO images (hash, url) VALUES (?1, ?2)",
            params![hash, url],
        )?;
    }
    Ok(())
}

fn progress_from_row(r: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Progress> {
    Ok(Progress {
        movie_id: r.get::<_, i64>(first)? as u64,
        position_s: r.get(first + 1)?,
        duration_s: r.get(first + 2)?,
        finished: r.get(first + 3)?,
        updated_at: r.get(first + 4)?,
    })
}

fn get_progress(conn: &Connection, movie_id: u64) -> AppResult<Option<Progress>> {
    Ok(conn
        .query_row(
            "SELECT movie_id, position_s, duration_s, finished, updated_at
             FROM progress WHERE movie_id = ?1",
            [movie_id as i64],
            |r| progress_from_row(r, 0),
        )
        .optional()?)
}

pub fn is_finished(position_s: f64, duration_s: f64) -> bool {
    duration_s > 0.0 && position_s >= duration_s * FINISHED_RATIO
}

/// Both finite, duration > 0, position ≥ 0 (clamped to the duration).
pub fn validate_progress(position_s: f64, duration_s: f64) -> AppResult<(f64, f64)> {
    if !position_s.is_finite() || !duration_s.is_finite() || duration_s <= 0.0 || position_s < 0.0 {
        return Err(AppError::InvalidInput(format!(
            "invalid progress {position_s}/{duration_s}"
        )));
    }
    Ok((position_s.min(duration_s), duration_s))
}

// -- Local URL rewriting ------------------------------------------------------------------

/// `http://127.0.0.1:<any port>/img/<hash>` (or an already stripped `/img/<hash>`) →
/// `/img/<hash>`. Anything else is not ours and is dropped.
pub fn strip_local_origin(url: &str) -> Option<String> {
    let path = if url.starts_with('/') {
        url.to_owned()
    } else {
        let u = Url::parse(url).ok()?;
        if u.scheme() != "http" || u.host_str() != Some("127.0.0.1") {
            return None;
        }
        u.path().to_owned()
    };
    let hash = path.strip_prefix("/img/")?;
    (!hash.is_empty() && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some(path)
}

/// The summary as stored: local URLs without origin.
pub fn to_stored(movie: &MovieSummary) -> MovieSummary {
    let strip = |u: &Option<String>| u.as_deref().and_then(strip_local_origin);
    MovieSummary {
        cover_url: strip(&movie.cover_url),
        cover_large_url: strip(&movie.cover_large_url),
        background_url: strip(&movie.background_url),
        ..movie.clone()
    }
}

/// A stored summary with the current local origin put back.
pub fn from_stored(movie: MovieSummary, local_base: &str) -> MovieSummary {
    let base = local_base.trim_end_matches('/');
    let abs = |u: Option<String>| u.map(|p| format!("{base}{p}"));
    MovieSummary {
        cover_url: abs(movie.cover_url),
        cover_large_url: abs(movie.cover_large_url),
        background_url: abs(movie.background_url),
        ..movie
    }
}

fn strip_opt(u: &Option<String>) -> Option<String> {
    u.as_deref().and_then(strip_local_origin)
}

/// The detail as stored: local URLs without origin and no per-user fields (favorite,
/// progress and download are read live).
pub fn detail_to_stored(d: &MovieDetail) -> MovieDetail {
    MovieDetail {
        summary_fields: to_stored(&d.summary_fields),
        screenshot_urls: d
            .screenshot_urls
            .iter()
            .filter_map(|u| strip_local_origin(u))
            .collect(),
        cast: d
            .cast
            .iter()
            .map(|c| CastMember {
                image_url: strip_opt(&c.image_url),
                ..c.clone()
            })
            .collect(),
        is_favorite: false,
        progress: None,
        download: None,
        offline: false,
        ..d.clone()
    }
}

pub fn detail_from_stored(d: MovieDetail, local_base: &str) -> MovieDetail {
    let base = local_base.trim_end_matches('/');
    MovieDetail {
        summary_fields: from_stored(d.summary_fields.clone(), local_base),
        screenshot_urls: d
            .screenshot_urls
            .iter()
            .map(|p| format!("{base}{p}"))
            .collect(),
        cast: d
            .cast
            .iter()
            .map(|c| CastMember {
                image_url: c.image_url.as_ref().map(|p| format!("{base}{p}")),
                ..c.clone()
            })
            .collect(),
        ..d
    }
}

fn detail_image_paths(d: &MovieDetail) -> impl Iterator<Item = &String> {
    let s = &d.summary_fields;
    [&s.cover_url, &s.cover_large_url, &s.background_url]
        .into_iter()
        .flatten()
        .chain(d.screenshot_urls.iter())
        .chain(d.cast.iter().filter_map(|c| c.image_url.as_ref()))
}

/// JSON to store plus the `hash → remote URL` pairs its images need after a restart.
fn stored(images: &ImageStore, movie: &MovieSummary) -> AppResult<(String, Vec<(String, String)>)> {
    let movie = to_stored(movie);
    let imgs = [
        &movie.cover_url,
        &movie.cover_large_url,
        &movie.background_url,
    ]
    .into_iter()
    .flatten()
    .filter_map(|p| {
        let hash = p.strip_prefix("/img/")?;
        Some((hash.to_owned(), images.remote_url(hash)?))
    })
    .collect();
    let json = serde_json::to_string(&movie)
        .map_err(|e| AppError::Internal(format!("serializing movie: {e}")))?;
    Ok((json, imgs))
}

fn load_movie(json: &str, local_base: &str) -> Option<MovieSummary> {
    match serde_json::from_str(json) {
        Ok(movie) => Some(from_stored(movie, local_base)),
        Err(e) => {
            tracing::warn!(error = %e, "skipping unreadable stored movie");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Quality;

    const BASE: &str = "http://127.0.0.1:4000";
    const REMOTE: &str = "https://yts.gg/assets/images/movies/x/medium-cover.jpg";

    fn images() -> (tempfile::TempDir, ImageStore) {
        let tmp = tempfile::tempdir().unwrap();
        let store = ImageStore::new(tmp.path().join("img"), Vec::<String>::new(), BASE).unwrap();
        (tmp, store)
    }

    fn movie(id: u64, images: &ImageStore) -> MovieSummary {
        MovieSummary {
            id,
            imdb_code: format!("tt{id}"),
            title: format!("Movie {id}"),
            year: 2021,
            rating: 7.1,
            runtime_min: 120,
            genres: vec!["Drama".into()],
            cover_url: images.local_url(Some(REMOTE)),
            cover_large_url: images.local_url(Some(REMOTE)),
            background_url: None,
            qualities: vec![Quality::P1080],
            has_x264: true,
            max_seeds: 10,
        }
    }

    fn tables(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn migrates_from_scratch() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(DB_FILE);
        drop(Db::open(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        assert_eq!(
            tables(&conn),
            [
                "downloads",
                "favorites",
                "images",
                "movie_details",
                "progress",
                "settings"
            ]
        );
        // Reopening is a no-op.
        drop(conn);
        drop(Db::open(&path).unwrap());
    }

    #[test]
    fn migrates_between_versions_keeping_data() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(migrate(&mut conn, &MIGRATIONS[..1]).unwrap(), 1);
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('subtitleLang', '\"en\"')",
            [],
        )
        .unwrap();
        let v2 = "ALTER TABLE settings ADD COLUMN note TEXT; CREATE TABLE extra (x INTEGER);";
        let next: Vec<&str> = MIGRATIONS[..1].iter().copied().chain([v2]).collect();
        assert_eq!(migrate(&mut conn, &next).unwrap(), 2);
        let value: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'subtitleLang'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, "\"en\"");
        assert!(tables(&conn).contains(&"extra".to_owned()));
        // A failing migration rolls back and leaves the version untouched.
        let broken: Vec<&str> = next
            .iter()
            .copied()
            .chain(["CREATE TABLE extra (y);"])
            .collect();
        assert!(migrate(&mut conn, &broken).is_err());
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
        // A DB from a newer app is refused.
        assert!(matches!(
            migrate(&mut conn, &MIGRATIONS[..1]),
            Err(AppError::Db(_))
        ));
    }

    #[test]
    fn strips_and_restores_the_local_origin() {
        assert_eq!(
            strip_local_origin("http://127.0.0.1:4321/img/abcdef12").as_deref(),
            Some("/img/abcdef12")
        );
        assert_eq!(
            strip_local_origin("/img/abcdef12").as_deref(),
            Some("/img/abcdef12")
        );
        for bad in [
            "https://yts.gg/img/abcd",
            "http://evil.example/img/abcd",
            "http://127.0.0.1:1/stream/abcd",
            "http://127.0.0.1:1/img/../x",
            "/img/",
            "garbage",
        ] {
            assert_eq!(strip_local_origin(bad), None, "{bad}");
        }

        let (_tmp, imgs) = images();
        let m = movie(1, &imgs);
        let s = to_stored(&m);
        let hash = ImageStore::hash_of(REMOTE);
        assert_eq!(s.cover_url, Some(format!("/img/{hash}")));
        assert_eq!(s.background_url, None);
        let back = from_stored(s, "http://127.0.0.1:9999/");
        assert_eq!(
            back.cover_url,
            Some(format!("http://127.0.0.1:9999/img/{hash}"))
        );
        assert_eq!(back.title, m.title);
    }

    #[tokio::test]
    async fn favorites_crud_and_url_rewrite_on_read() {
        let db = Db::open_in_memory().unwrap();
        let (_tmp, imgs) = images();
        assert!(db.list_favorites(BASE).await.unwrap().is_empty());

        db.add_favorite(&imgs, &movie(1, &imgs)).await.unwrap();
        db.add_favorite(&imgs, &movie(2, &imgs)).await.unwrap();
        assert!(db.is_favorite(1).await.unwrap());
        assert!(!db.is_favorite(3).await.unwrap());

        // Read back after a "restart" on another port: newest first, new origin.
        let list = db.list_favorites("http://127.0.0.1:5555").await.unwrap();
        assert_eq!(list.iter().map(|m| m.id).collect::<Vec<_>>(), [2, 1]);
        let hash = ImageStore::hash_of(REMOTE);
        assert_eq!(
            list[0].cover_url,
            Some(format!("http://127.0.0.1:5555/img/{hash}"))
        );
        // The image registry was saved for the restart.
        assert_eq!(db.images().await.unwrap(), [(hash, REMOTE.to_owned())]);

        // Re-adding updates the data without moving it to the top.
        let mut renamed = movie(1, &imgs);
        renamed.title = "Renamed".into();
        db.add_favorite(&imgs, &renamed).await.unwrap();
        let list = db.list_favorites(BASE).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].title, "Renamed");

        db.remove_favorite(2).await.unwrap();
        db.remove_favorite(42).await.unwrap(); // unknown: no-op
        assert_eq!(db.list_favorites(BASE).await.unwrap().len(), 1);
        assert!(!db.is_favorite(2).await.unwrap());
    }

    #[test]
    fn finished_from_92_percent() {
        assert!(!is_finished(91.9, 100.0));
        assert!(is_finished(92.0, 100.0));
        assert!(is_finished(100.0, 100.0));
        assert!(!is_finished(0.0, 0.0));
        assert!(validate_progress(-1.0, 100.0).is_err());
        assert!(validate_progress(1.0, 0.0).is_err());
        assert!(validate_progress(f64::NAN, 100.0).is_err());
        assert!(validate_progress(1.0, f64::INFINITY).is_err());
        assert_eq!(validate_progress(150.0, 100.0).unwrap(), (100.0, 100.0));
    }

    #[tokio::test]
    async fn progress_crud_and_continue_watching() {
        let db = Db::open_in_memory().unwrap();
        let (_tmp, imgs) = images();
        assert_eq!(db.get_progress(1).await.unwrap(), None);

        let p = db
            .save_progress(&imgs, &movie(1, &imgs), 600.0, 6000.0)
            .await
            .unwrap();
        assert_eq!((p.movie_id, p.position_s, p.duration_s), (1, 600.0, 6000.0));
        assert!(!p.finished);
        assert!(p.updated_at.ends_with('Z') && p.updated_at.contains('T'));
        assert_eq!(db.get_progress(1).await.unwrap(), Some(p));

        db.save_progress(&imgs, &movie(2, &imgs), 10.0, 6000.0)
            .await
            .unwrap();
        let items = db.list_continue_watching(BASE).await.unwrap();
        assert_eq!(items.iter().map(|i| i.movie.id).collect::<Vec<_>>(), [2, 1]);
        assert!(items[0]
            .movie
            .cover_url
            .as_deref()
            .unwrap()
            .starts_with(BASE));

        // Updating moves it to the front.
        db.save_progress(&imgs, &movie(1, &imgs), 700.0, 6000.0)
            .await
            .unwrap();
        let items = db.list_continue_watching(BASE).await.unwrap();
        assert_eq!(items[0].movie.id, 1);
        assert_eq!(items[0].progress.position_s, 700.0);

        // Past 92 %: finished and out of the row, but still readable.
        let p = db
            .save_progress(&imgs, &movie(1, &imgs), 5600.0, 6000.0)
            .await
            .unwrap();
        assert!(p.finished);
        let items = db.list_continue_watching(BASE).await.unwrap();
        assert_eq!(items.iter().map(|i| i.movie.id).collect::<Vec<_>>(), [2]);
        assert!(db.get_progress(1).await.unwrap().unwrap().finished);

        // Watching again from the start un-finishes it.
        let p = db
            .save_progress(&imgs, &movie(1, &imgs), 30.0, 6000.0)
            .await
            .unwrap();
        assert!(!p.finished);

        assert!(matches!(
            db.save_progress(&imgs, &movie(3, &imgs), 1.0, 0.0).await,
            Err(AppError::InvalidInput(_))
        ));

        db.remove_progress(2).await.unwrap();
        assert_eq!(db.get_progress(2).await.unwrap(), None);
        assert_eq!(db.list_continue_watching(BASE).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn data_survives_reopening_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(DB_FILE);
        let (_tmp, imgs) = images();
        {
            let db = Db::open(&path).unwrap();
            db.add_favorite(&imgs, &movie(7, &imgs)).await.unwrap();
            db.save_progress(&imgs, &movie(7, &imgs), 42.0, 100.0)
                .await
                .unwrap();
            db.put_settings(vec![("subtitleLang".into(), "\"en\"".into())])
                .await
                .unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.list_favorites(BASE).await.unwrap()[0].id, 7);
        assert_eq!(db.get_progress(7).await.unwrap().unwrap().position_s, 42.0);
        assert_eq!(
            db.settings_rows().await.unwrap(),
            [("subtitleLang".to_owned(), "\"en\"".to_owned())]
        );
    }

    fn download_row(infohash: &str, movie_id: u64, imgs: &ImageStore) -> DownloadRow {
        DownloadRow {
            infohash: infohash.into(),
            movie_id,
            movie: movie(movie_id, imgs),
            quality: "1080p".into(),
            video_codec: "x264".into(),
            state: "active".into(),
            size_bytes: 1000,
            path: format!("/lib/Movie {movie_id} (2021) [1080p]"),
            error: None,
            added_at: format!("2026-10-04T12:00:0{movie_id}.000Z"),
            title: format!("Movie {movie_id}"),
            torrent_url: Some("https://yts.gg/torrent/download/X".into()),
            torrent: None,
            rel_path: None,
            downloaded_bytes: 0,
        }
    }

    fn detail(id: u64, imgs: &ImageStore) -> MovieDetail {
        MovieDetail {
            summary_fields: movie(id, imgs),
            summary: "Plot".into(),
            language: "en".into(),
            mpa_rating: Some("R".into()),
            yt_trailer_code: Some("abc".into()),
            screenshot_urls: vec![imgs.local_url(Some(REMOTE)).unwrap()],
            cast: vec![CastMember {
                name: "Actor".into(),
                character: None,
                image_url: imgs.local_url(Some(REMOTE)),
            }],
            torrents: vec![],
            is_favorite: true,
            progress: None,
            download: None,
            offline: false,
        }
    }

    #[tokio::test]
    async fn downloads_crud_survives_reopening() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(DB_FILE);
        let (_tmp, imgs) = images();
        {
            let db = Db::open(&path).unwrap();
            db.put_download(&imgs, &download_row("aa", 1, &imgs))
                .await
                .unwrap();
            db.put_download(&imgs, &download_row("bb", 2, &imgs))
                .await
                .unwrap();
            db.set_download_torrent("aa", vec![1, 2, 3], "Movie.mp4".into(), 900)
                .await
                .unwrap();
            db.update_download("aa", "paused", None, 900, 450)
                .await
                .unwrap();
            db.update_download("bb", "error", Some("boom".into()), 1000, 0)
                .await
                .unwrap();
        }
        let db = Db::open(&path).unwrap();
        let rows = db.list_downloads("http://127.0.0.1:7777").await.unwrap();
        assert_eq!(rows.len(), 2);
        let a = &rows[0];
        assert_eq!(a.infohash, "aa");
        assert_eq!(a.state, "paused");
        assert_eq!(a.torrent.as_deref(), Some(&[1u8, 2, 3][..]));
        assert_eq!(a.rel_path.as_deref(), Some("Movie.mp4"));
        assert_eq!((a.size_bytes, a.downloaded_bytes), (900, 450));
        assert!(a
            .movie
            .cover_url
            .as_deref()
            .unwrap()
            .starts_with("http://127.0.0.1:7777/img/"));
        assert_eq!(rows[1].error.as_deref(), Some("boom"));

        db.remove_download("aa").await.unwrap();
        db.remove_download("zz").await.unwrap(); // unknown: no-op
        assert_eq!(db.list_downloads(BASE).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn movie_detail_copy_for_offline_use() {
        let db = Db::open_in_memory().unwrap();
        let (_tmp, imgs) = images();
        assert_eq!(db.movie_detail(1, BASE).await.unwrap(), None);
        db.put_movie_detail(&imgs, &detail(1, &imgs)).await.unwrap();
        db.put_download(&imgs, &download_row("aa", 1, &imgs))
            .await
            .unwrap();
        db.put_download(&imgs, &download_row("bb", 1, &imgs))
            .await
            .unwrap();

        // Read back on another port: local URLs follow, user fields are not stored.
        let base = "http://127.0.0.1:5555";
        let d = db.movie_detail(1, base).await.unwrap().unwrap();
        let hash = ImageStore::hash_of(REMOTE);
        let local = format!("{base}/img/{hash}");
        assert_eq!(d.summary_fields.cover_url.as_deref(), Some(local.as_str()));
        assert_eq!(d.screenshot_urls, std::slice::from_ref(&local));
        assert_eq!(d.cast[0].image_url.as_deref(), Some(local.as_str()));
        assert!(!d.is_favorite && !d.offline);
        assert_eq!(d.summary, "Plot");
        assert!(db
            .images()
            .await
            .unwrap()
            .contains(&(hash, REMOTE.to_owned())));

        // Kept while another download of the movie remains.
        db.remove_download("aa").await.unwrap();
        assert!(db.movie_detail(1, base).await.unwrap().is_some());
        db.remove_download("bb").await.unwrap();
        assert_eq!(db.movie_detail(1, base).await.unwrap(), None);
    }

    #[test]
    fn migration_2_keeps_phase_1_download_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, &MIGRATIONS[..1]).unwrap();
        conn.execute(
            "INSERT INTO downloads (infohash, movie_id, movie, quality, video_codec, state, added_at)
             VALUES ('aa', 1, '{}', '1080p', 'x264', 'active', 'now')",
            [],
        )
        .unwrap();
        migrate(&mut conn, MIGRATIONS).unwrap();
        let (title, downloaded): (String, i64) = conn
            .query_row(
                "SELECT title, downloaded_bytes FROM downloads WHERE infohash = 'aa'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((title.as_str(), downloaded), ("", 0));
    }
}
