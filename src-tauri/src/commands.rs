//! `#[tauri::command]` handlers. Thin layer: validate input, delegate to modules,
//! return `AppResult<T>`. Every command must match `docs/IPC.md`.

use tauri::State;

use crate::error::AppResult;
use crate::state::AppState;
use crate::types::{ApiEndpointStatus, ListMoviesParams, MovieDetail, MoviePage, MovieSummary};

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
