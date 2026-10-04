//! `#[tauri::command]` handlers. Thin layer: validate input, delegate to modules,
//! return `AppResult<T>`. Every command must match `docs/IPC.md`.

use tauri::State;

use crate::error::AppResult;
use crate::images::ImageStore;
use crate::state::AppState;
use crate::torrent::{launch_external_player, StreamRequest};
use crate::types::{
    ApiEndpointStatus, ClearCacheResult, ContinueItem, ListMoviesParams, MovieDetail, MoviePage,
    MovieSummary, Progress, Settings, SettingsPatch, StorageUsage, StreamSession,
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
    let mut detail = state.yts.get_movie(movie_id).await?;
    detail.is_favorite = state.db.is_favorite(movie_id).await?;
    detail.progress = state.db.get_progress(movie_id).await?;
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
    let torrent = state.yts.torrent_ref(movie_id, &infohash).await?;
    let mut session = state
        .torrents
        .start_stream(StreamRequest {
            movie_id,
            infohash,
            title: torrent.title,
            torrent_url: torrent.torrent_url,
            video_codec: torrent.video_codec,
            seeds: torrent.seeds,
        })
        .await?;
    let progress = state.db.get_progress(movie_id).await?;
    session.resume_at_s = resume_at(progress.as_ref());
    if let (Some(at), Some(p)) = (session.resume_at_s, &progress) {
        state
            .torrents
            .hint_start_fraction(&session.infohash, at / p.duration_s);
    }
    // Make room for the new stream (it is protected while open).
    state.cache.enforce_soon();
    Ok(session)
}

#[tauri::command]
pub async fn stop_stream(state: State<'_, AppState>, infohash: String) -> AppResult<()> {
    tracing::debug!(%infohash, "stop_stream");
    state.torrents.stop_stream(&infohash).await
}

#[tauri::command]
pub async fn open_external_player(state: State<'_, AppState>, infohash: String) -> AppResult<()> {
    let url = state.torrents.session_url(&infohash)?;
    let player = state.settings.get().external_player;
    tracing::info!(%url, %player, "opening external player");
    launch_external_player(&player, &url)
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
// Settings and storage
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    Ok(state.settings.get())
}

/// Applied now: `apiBaseUrls`, `bufferTargetBytes`, `cacheLimitBytes` (and the ones read
/// on use: `externalPlayer`; `preferredQuality`/`preferX264` are used by the front).
/// Stored only until phase 6: speed limits, `listenPort`, `seedAfterDownload`.
/// `dataDir`: on the next start.
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
    if before.data_dir != after.data_dir {
        tracing::info!(data_dir = %after.data_dir, "dataDir changed, applies on restart");
    }
    Ok(after)
}

#[tauri::command]
pub async fn get_storage_usage(state: State<'_, AppState>) -> AppResult<StorageUsage> {
    state.cache.usage().await
}

#[tauri::command]
pub async fn clear_cache(state: State<'_, AppState>) -> AppResult<ClearCacheResult> {
    state.cache.clear().await
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
