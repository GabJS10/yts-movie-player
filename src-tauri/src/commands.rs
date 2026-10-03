//! `#[tauri::command]` handlers. Thin layer: validate input, delegate to modules,
//! return `AppResult<T>`. Every command must match `docs/IPC.md`.

use tauri::State;

use crate::error::AppResult;
use crate::state::AppState;
use crate::torrent::{launch_external_player, StreamRequest};
use crate::types::{
    ApiEndpointStatus, ListMoviesParams, MovieDetail, MoviePage, MovieSummary, StreamSession,
};

/// Fixed until Settings arrive (phase 4).
const EXTERNAL_PLAYER: &str = "vlc";

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
    state.yts.get_movie(movie_id).await
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

#[tauri::command]
pub async fn start_stream(
    state: State<'_, AppState>,
    movie_id: u64,
    infohash: String,
) -> AppResult<StreamSession> {
    tracing::debug!(movie_id, %infohash, "start_stream");
    let torrent = state.yts.torrent_ref(movie_id, &infohash).await?;
    state
        .torrents
        .start_stream(StreamRequest {
            movie_id,
            infohash,
            title: torrent.title,
            torrent_url: torrent.torrent_url,
            video_codec: torrent.video_codec,
            seeds: torrent.seeds,
        })
        .await
}

#[tauri::command]
pub async fn stop_stream(state: State<'_, AppState>, infohash: String) -> AppResult<()> {
    tracing::debug!(%infohash, "stop_stream");
    state.torrents.stop_stream(&infohash).await
}

#[tauri::command]
pub async fn open_external_player(state: State<'_, AppState>, infohash: String) -> AppResult<()> {
    let url = state.torrents.session_url(&infohash)?;
    tracing::info!(%url, player = EXTERNAL_PLAYER, "opening external player");
    launch_external_player(EXTERNAL_PLAYER, &url)
}
