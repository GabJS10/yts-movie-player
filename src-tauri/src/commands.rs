//! `#[tauri::command]` handlers. Thin layer: validate input, delegate to modules,
//! return `AppResult<T>`. Every command must match `docs/IPC.md`.

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::downloads::TorrentInfo;
use crate::error::{AppError, AppResult};
use crate::external_player::{self, player_kind, subtitle_request};
use crate::images::ImageStore;
use crate::state::AppState;
use crate::subtitles::{Credentials, Release};
use crate::torrent::{kbps_to_bps, StreamRequest};
use crate::types::{
    ApiEndpointStatus, AppInfo, ClearCacheResult, ContinueItem, Download, ExternalPlayerResult,
    FeaturedItem, HomeProfile, ListMoviesParams, MovieDetail, MoviePage, MovieSummary, Progress,
    Settings, SettingsPatch, StorageUsage, StreamSession, SubtitleOption, SubtitleTrack,
    SubtitlesStatus, UpdateInfo,
};

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_movies(
    state: State<'_, AppState>,
    params: ListMoviesParams,
) -> AppResult<MoviePage> {
    tracing::debug!(?params, "list_movies");
    state.yts.list_movies(&params).await
}

#[tauri::command]
pub async fn get_movie(state: State<'_, AppState>, movie_id: u64) -> AppResult<MovieDetail> {
    tracing::debug!(movie_id, "get_movie");
    let download = state.downloads.for_movie(movie_id);
    let mut detail = match state.yts.get_movie(movie_id).await {
        Ok(detail) => {
            if download.is_some() {
                // Keep the offline copy fresh.
                if let Err(e) = state.db.put_movie_detail(&state.images, &detail).await {
                    tracing::warn!(movie_id, error = %e, "could not refresh the offline copy");
                }
            }
            detail
        }
        // No network: the copy saved when downloading, if any.
        Err(e @ (AppError::Network(_) | AppError::ApiUnavailable(_))) => {
            match state.db.movie_detail(movie_id, local_base(&state)).await? {
                Some(copy) => {
                    tracing::info!(movie_id, "offline: using the saved movie detail");
                    MovieDetail {
                        offline: true,
                        ..copy
                    }
                }
                None => return Err(e),
            }
        }
        Err(e) => return Err(e),
    };
    detail.is_favorite = state.db.is_favorite(movie_id).await?;
    detail.progress = state.db.get_progress(movie_id).await?;
    detail.download = download;
    Ok(detail)
}

#[tauri::command]
pub async fn get_suggestions(
    state: State<'_, AppState>,
    movie_id: u64,
) -> AppResult<Vec<MovieSummary>> {
    tracing::debug!(movie_id, "get_suggestions");
    state.yts.suggestions(movie_id).await
}

/// History for the recommendations: progress, Mi lista and downloads.
async fn history(state: &AppState) -> AppResult<crate::recommend::History> {
    let base = local_base(state);
    Ok(crate::recommend::History::new(
        state.db.progress_movies(base).await?,
        state.db.list_favorites(base).await?,
        state
            .downloads
            .list()
            .into_iter()
            .map(|d| d.movie)
            .collect(),
    ))
}

#[tauri::command]
pub async fn get_featured(state: State<'_, AppState>) -> AppResult<Vec<FeaturedItem>> {
    let history = history(&state).await?;
    Ok(state.recommender.featured(&state.yts, &history).await)
}

#[tauri::command]
pub async fn get_home_profile(state: State<'_, AppState>) -> AppResult<HomeProfile> {
    let history = history(&state).await?;
    Ok(state.recommender.home_profile(&state.yts, &history).await)
}

#[tauri::command]
pub async fn get_api_status(state: State<'_, AppState>) -> AppResult<Vec<ApiEndpointStatus>> {
    Ok(state.yts.api_status().await)
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

/// Where to resume: the saved position, unless the movie was watched (`finished`).
pub fn resume_at(progress: Option<&Progress>) -> Option<f64> {
    progress
        .filter(|p| !p.finished && p.position_s > 0.0)
        .map(|p| p.position_s)
}

#[tauri::command]
pub async fn start_stream(
    state: State<'_, AppState>,
    movie_id: u64,
    infohash: String,
) -> AppResult<StreamSession> {
    tracing::debug!(movie_id, %infohash, "start_stream");
    let network = state.downloads.library_stream(&infohash).is_none();
    let mut session = if let Some(local) = state.downloads.library_stream(&infohash) {
        // Finished download: straight from `library/`, no torrent and no network.
        state.torrents.serve_local(local).await?
    } else {
        // A download in progress is already known: no need to ask YTS (works offline).
        let req = match state.downloads.stream_request(&infohash) {
            Some(req) => req,
            None => {
                let torrent = state.yts.torrent_ref(movie_id, &infohash).await?;
                StreamRequest {
                    movie_id,
                    infohash,
                    title: torrent.title,
                    torrent_url: torrent.torrent_url,
                    video_codec: torrent.video_codec,
                    seeds: torrent.seeds,
                }
            }
        };
        state.torrents.start_stream(req).await?
    };
    let progress = state.db.get_progress(movie_id).await?;
    session.resume_at_s = resume_at(progress.as_ref());
    if let (Some(at), Some(p)) = (session.resume_at_s, &progress) {
        state
            .torrents
            .hint_start_fraction(&session.infohash, at / p.duration_s);
    }
    if network {
        // Make room for the new stream (it is protected while open).
        state.cache.enforce_soon();
    }
    Ok(session)
}

#[tauri::command]
pub async fn stop_stream(state: State<'_, AppState>, infohash: String) -> AppResult<()> {
    tracing::debug!(%infohash, "stop_stream");
    state.torrents.stop_stream(&infohash).await?;
    // A stream left in a previous cache folder is deleted once closed.
    state.cache.enforce_soon();
    Ok(())
}

/// Opens the player from Settings with the stream and, when possible, the subtitles
/// (see `external_player.rs`). Subtitle problems never stop it from opening.
#[tauri::command]
pub async fn open_external_player(
    state: State<'_, AppState>,
    infohash: String,
    subtitle_id: Option<String>,
    subtitle_path: Option<String>,
    subtitle_delay_ms: Option<i64>,
    subtitles_off: Option<bool>,
) -> AppResult<ExternalPlayerResult> {
    let url = state.torrents.session_url(&infohash)?;
    let settings = state.settings.get();
    // Before touching subtitles: no quota spent if the player isn't there.
    let player = external_player::resolve_player(&settings.external_player)?;
    let kind = player_kind(&settings.external_player);
    let request = subtitle_request(
        subtitles_off.unwrap_or(false),
        subtitle_id,
        subtitle_path,
        settings.auto_subtitles,
        state.subtitles.has_api_key(),
    );
    let search = async {
        let movie_id = state
            .torrents
            .session_movie_id(&infohash)
            .ok_or_else(|| AppError::NotFound(format!("no stream for {infohash}")))?;
        search_for_movie(&state, movie_id, &settings.subtitle_lang, Some(&infohash)).await
    };
    let (subtitle_file, subtitle) = external_player::prepare_subtitle(
        &state.subtitles,
        kind,
        request,
        search,
        subtitle_delay_ms.unwrap_or(0),
    )
    .await;
    let args = external_player::player_args(kind, &url, subtitle_file.as_deref());
    tracing::info!(player = %player.display(), ?args, ?subtitle, "opening external player");
    external_player::launch(&player, &args)?;
    Ok(ExternalPlayerResult { subtitle })
}

// ---------------------------------------------------------------------------
// Mi lista
// ---------------------------------------------------------------------------

fn local_base(state: &AppState) -> &str {
    state.images.local_base()
}

#[tauri::command]
pub async fn list_favorites(state: State<'_, AppState>) -> AppResult<Vec<MovieSummary>> {
    state.db.list_favorites(local_base(&state)).await
}

#[tauri::command]
pub async fn add_favorite(state: State<'_, AppState>, movie: MovieSummary) -> AppResult<()> {
    tracing::debug!(movie_id = movie.id, "add_favorite");
    state.db.add_favorite(&state.images, &movie).await
}

#[tauri::command]
pub async fn remove_favorite(state: State<'_, AppState>, movie_id: u64) -> AppResult<()> {
    tracing::debug!(movie_id, "remove_favorite");
    state.db.remove_favorite(movie_id).await
}

// ---------------------------------------------------------------------------
// Continuar viendo
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn save_progress(
    state: State<'_, AppState>,
    movie: MovieSummary,
    position_s: f64,
    duration_s: f64,
) -> AppResult<Progress> {
    tracing::debug!(movie_id = movie.id, position_s, duration_s, "save_progress");
    state
        .db
        .save_progress(&state.images, &movie, position_s, duration_s)
        .await
}

#[tauri::command]
pub async fn get_progress(
    state: State<'_, AppState>,
    movie_id: u64,
) -> AppResult<Option<Progress>> {
    state.db.get_progress(movie_id).await
}

#[tauri::command]
pub async fn list_continue_watching(state: State<'_, AppState>) -> AppResult<Vec<ContinueItem>> {
    state.db.list_continue_watching(local_base(&state)).await
}

#[tauri::command]
pub async fn remove_progress(state: State<'_, AppState>, movie_id: u64) -> AppResult<()> {
    tracing::debug!(movie_id, "remove_progress");
    state.db.remove_progress(movie_id).await
}

// ---------------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    movie: MovieSummary,
    infohash: String,
) -> AppResult<Download> {
    tracing::debug!(movie_id = movie.id, %infohash, "start_download");
    if let Some(existing) = state.downloads.get(&infohash) {
        return Ok(existing);
    }
    let detail = state.yts.get_movie(movie.id).await?;
    let torrent = detail
        .torrents
        .iter()
        .find(|t| t.infohash.eq_ignore_ascii_case(infohash.trim()))
        .ok_or_else(|| AppError::NotFound(format!("torrent {infohash} of movie {}", movie.id)))?
        .clone();
    let r = state.yts.torrent_ref(movie.id, &infohash).await?;
    let info = TorrentInfo {
        infohash: torrent.infohash,
        quality: torrent.quality,
        video_codec: torrent.video_codec,
        size_bytes: torrent.size_bytes,
        title: r.title,
        torrent_url: r.torrent_url,
        seeds: r.seeds,
    };
    state.downloads.start(movie, &detail, info).await
}

#[tauri::command]
pub async fn list_downloads(state: State<'_, AppState>) -> AppResult<Vec<Download>> {
    Ok(state.downloads.list())
}

#[tauri::command]
pub async fn pause_download(state: State<'_, AppState>, infohash: String) -> AppResult<Download> {
    tracing::debug!(%infohash, "pause_download");
    state.downloads.pause(&infohash).await
}

#[tauri::command]
pub async fn resume_download(state: State<'_, AppState>, infohash: String) -> AppResult<Download> {
    tracing::debug!(%infohash, "resume_download");
    state.downloads.resume(&infohash).await
}

#[tauri::command]
pub async fn remove_download(
    state: State<'_, AppState>,
    infohash: String,
    delete_files: bool,
) -> AppResult<()> {
    tracing::debug!(%infohash, delete_files, "remove_download");
    state.downloads.remove(&infohash, delete_files).await
}

#[tauri::command]
pub async fn move_downloads(state: State<'_, AppState>) -> AppResult<()> {
    tracing::debug!("move_downloads");
    state.downloads.start_move()
}

#[tauri::command]
pub async fn cancel_move_downloads(state: State<'_, AppState>) -> AppResult<()> {
    tracing::debug!("cancel_move_downloads");
    state.downloads.cancel_move();
    Ok(())
}

#[tauri::command]
pub async fn open_download_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    infohash: String,
) -> AppResult<()> {
    let folder = state.downloads.folder(&infohash)?;
    app.opener()
        .open_path(folder.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Internal(format!("opening {}: {e}", folder.display())))
}

// ---------------------------------------------------------------------------
// Settings and storage
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    Ok(state.settings.get())
}

/// Applied now: `apiBaseUrls`, `bufferTargetBytes`, `cacheLimitBytes`, speed limits,
/// `seedAfterDownload` (and the ones read on use: `externalPlayer`;
/// `preferredQuality`/`preferX264` are used by the front).
/// Folders (`cacheDir`, `downloadsDir`) too, see `apply_cache_dir`. On the next start:
/// `listenPort`.
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> AppResult<Settings> {
    // Never log the patch itself: it may carry the OpenSubtitles API key.
    let (before, after) = state.settings.update(patch).await?;
    if before.api_base_urls != after.api_base_urls {
        state.yts.set_base_urls(&after.api_base_urls)?;
        let hosts = ImageStore::default_allowed_hosts(&after.api_base_urls);
        state.images.set_allowed_hosts(hosts.clone());
        state.torrents.set_allowed_torrent_hosts(hosts);
    }
    state
        .torrents
        .set_buffer_target_bytes(after.buffer_target_bytes);
    if before.cache_limit_bytes != after.cache_limit_bytes {
        state.cache.set_limit_bytes(after.cache_limit_bytes);
        state.cache.enforce_soon();
    }
    let creds = subtitle_credentials(&after);
    if subtitle_credentials(&before) != creds {
        state.subtitles.set_credentials(creds).await;
    }
    if (before.down_limit_kbps, before.up_limit_kbps)
        != (after.down_limit_kbps, after.up_limit_kbps)
    {
        state.torrents.set_rate_limits(
            kbps_to_bps(after.down_limit_kbps),
            kbps_to_bps(after.up_limit_kbps),
        );
    }
    if before.seed_after_download != after.seed_after_download {
        state
            .downloads
            .set_seed_after_download(after.seed_after_download)
            .await;
    }
    if before.listen_port != after.listen_port {
        tracing::info!(listen_port = ?after.listen_port, "listenPort changed, applies on restart");
    }
    if before.cache_dir != after.cache_dir {
        if let Some(warning) = apply_cache_dir(&state) {
            tracing::warn!(error = %warning, "cache folder not available");
        }
    }
    if before.downloads_dir != after.downloads_dir {
        state
            .downloads
            .set_downloads_dir(std::path::PathBuf::from(&after.downloads_dir));
    }
    Ok(after)
}

// ---------------------------------------------------------------------------
// Subtitles
// ---------------------------------------------------------------------------

pub fn subtitle_credentials(s: &Settings) -> Credentials {
    Credentials {
        api_key: s.open_subtitles_api_key.clone(),
        username: s.open_subtitles_username.clone(),
        password: s.open_subtitles_password.clone(),
    }
}

#[tauri::command]
pub async fn search_subtitles(
    state: State<'_, AppState>,
    movie_id: u64,
    lang: String,
    infohash: Option<String>,
) -> AppResult<Vec<SubtitleOption>> {
    tracing::debug!(movie_id, %lang, ?infohash, "search_subtitles");
    search_for_movie(&state, movie_id, &lang, infohash.as_deref()).await
}

/// Ranked subtitles for a movie, matching the release of `infohash` when given.
async fn search_for_movie(
    state: &AppState,
    movie_id: u64,
    lang: &str,
    infohash: Option<&str>,
) -> AppResult<Vec<SubtitleOption>> {
    // Cached by the YTS client (the movie page already asked for it).
    let movie = state.yts.get_movie(movie_id).await?;
    let release = infohash.and_then(|h| {
        movie
            .torrents
            .iter()
            .find(|t| t.infohash.eq_ignore_ascii_case(h.trim()))
            .map(|t| Release {
                quality: t.quality,
                source: t.source,
            })
    });
    state
        .subtitles
        .search(&movie.summary_fields.imdb_code, lang, release)
        .await
}

#[tauri::command]
pub async fn load_subtitle(
    state: State<'_, AppState>,
    subtitle_id: String,
) -> AppResult<SubtitleTrack> {
    tracing::debug!(%subtitle_id, "load_subtitle");
    state.subtitles.load(&subtitle_id).await
}

#[tauri::command]
pub async fn load_subtitle_file(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<SubtitleTrack> {
    state.subtitles.load_file(std::path::Path::new(&path)).await
}

#[tauri::command]
pub async fn get_subtitles_status(state: State<'_, AppState>) -> AppResult<SubtitlesStatus> {
    state.subtitles.status().await
}

/// Points the engine and the cache at `cacheDir`, or at the default folder while it is not
/// available (the previous folder is emptied in the background). Returns the warning to
/// show when it falls back to the default folder.
pub fn apply_cache_dir(state: &AppState) -> Option<AppError> {
    let configured = std::path::PathBuf::from(state.settings.get().cache_dir);
    let (dir, available) = crate::cache::effective_cache_dir(&configured, &state.paths.cache_dir);
    if state.torrents.cache_dir() == dir {
        return None;
    }
    state.torrents.set_cache_dir(dir.clone());
    state.cache.set_cache_dir(dir.clone());
    state.cache.enforce_soon();
    (!available).then(|| cache_fallback_warning(&configured, &dir))
}

pub fn cache_fallback_warning(configured: &std::path::Path, used: &std::path::Path) -> AppError {
    AppError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "cache folder {} is not available, streaming uses {}",
            configured.display(),
            used.display()
        ),
    ))
}

#[tauri::command]
pub async fn get_storage_usage(state: State<'_, AppState>) -> AppResult<StorageUsage> {
    let cache = state.cache.usage().await?;
    let downloads = state.downloads.usage().await;
    let configured_cache = std::path::PathBuf::from(state.settings.get().cache_dir);
    Ok(StorageUsage {
        cache_bytes: cache.cache_bytes,
        cache_limit_bytes: state.cache.limit_bytes(),
        library_bytes: downloads.library_bytes,
        cache_free_bytes: cache.free_bytes,
        downloads_free_bytes: downloads.free_bytes,
        cache_dir_available: crate::settings::dir_available(&configured_cache),
        downloads_dir_available: downloads.dir_available,
        downloads_outside_dir: downloads.outside_dir,
        default_downloads_dir: state.paths.library_dir.to_string_lossy().into_owned(),
        default_cache_dir: state.paths.cache_dir.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn clear_cache(state: State<'_, AppState>) -> AppResult<ClearCacheResult> {
    state.cache.clear().await
}

// ---------------------------------------------------------------------------
// Trailer
// ---------------------------------------------------------------------------

const TRAILER_WINDOW: &str = "trailer";

/// Plan B for the trailer: a separate window with the local `/trailer/<code>` page (see
/// `stream.rs`: the embed needs a `Referer`, which this page provides). Reuses the window
/// if it is already open. The window has no IPC access (no capability lists it).
/// `title` is the window title as the UI wants it (already localized), used as is.
#[tauri::command]
pub async fn open_trailer_window(
    app: AppHandle,
    state: State<'_, AppState>,
    yt_trailer_code: String,
    title: String,
) -> AppResult<()> {
    let code = yt_trailer_code.trim();
    if !crate::stream::is_valid_yt_code(code) {
        return Err(AppError::InvalidInput(format!(
            "invalid trailer code {code:?}"
        )));
    }
    let title: String = title.trim().chars().take(200).collect();
    let url = crate::stream::trailer_url(local_base(&state), code, &title);
    let parsed =
        url::Url::parse(&url).map_err(|e| AppError::Internal(format!("trailer URL {url}: {e}")))?;
    tracing::info!(%code, "opening trailer window");
    use tauri::Manager;
    if let Some(window) = app.get_webview_window(TRAILER_WINDOW) {
        window
            .navigate(parsed)
            .and_then(|()| window.set_title(&title))
            .and_then(|()| window.set_focus())
            .map_err(|e| AppError::Internal(format!("trailer window: {e}")))?;
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(&app, TRAILER_WINDOW, tauri::WebviewUrl::External(parsed))
        .title(&title)
        .inner_size(1280.0, 720.0)
        .min_inner_size(480.0, 270.0)
        .center()
        .background_color(tauri::window::Color(0, 0, 0, 255))
        .build()
        .map_err(|e| AppError::Internal(format!("trailer window: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_app_info(state: State<'_, AppState>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: crate::app::version().to_owned(),
        logs_dir: crate::app::logs_dir()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default(),
        data_dir: state.paths.data_dir.to_string_lossy().into_owned(),
        repo_url: crate::app::REPO_URL.to_owned(),
    })
}

/// Never fails: `null` when there is nothing newer or GitHub can't be reached.
#[tauri::command]
pub async fn check_for_update(state: State<'_, AppState>) -> AppResult<Option<UpdateInfo>> {
    Ok(state.updates.check().await)
}

#[tauri::command]
pub async fn open_logs_folder(app: AppHandle) -> AppResult<()> {
    let dir = crate::app::logs_dir()
        .ok_or_else(|| AppError::NotFound("no logs folder on this system".into()))?;
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Internal(format!("opening {}: {e}", dir.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_only_unfinished_progress() {
        let p = |position_s, finished| Progress {
            movie_id: 1,
            position_s,
            duration_s: 100.0,
            finished,
            updated_at: String::new(),
        };
        assert_eq!(resume_at(None), None);
        assert_eq!(resume_at(Some(&p(42.5, false))), Some(42.5));
        assert_eq!(resume_at(Some(&p(95.0, true))), None);
        assert_eq!(resume_at(Some(&p(0.0, false))), None);
    }
}
