//! Local HTTP server on 127.0.0.1 (random port). Serves cached images (`/img/<hash>`) and
//! torrent files (`/stream/<infohash>/<fileIdx>`, with Range support) and converted
//! subtitles (`/subs/<id>.vtt`).

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::error::AppError;
use crate::images::{ImageError, ImageStore};
use crate::subtitles::{is_valid_sub_id, vtt_path};
use crate::torrent::{parse_range, video_mime, TorrentEngine};

/// WebView origins (Linux/macOS, Windows and the Vite dev server).
const ALLOWED_ORIGINS: [&str; 4] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost:1420",
];

#[derive(Clone)]
pub struct ServerState {
    pub images: Arc<ImageStore>,
    pub torrents: Option<Arc<TorrentEngine>>,
    /// `.vtt` cache (`<data>/subs`), served at `/subs/<id>.vtt`.
    pub subs_dir: Option<PathBuf>,
}

/// Binds to a random port on the loopback interface.
pub async fn bind() -> std::io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await
}

pub fn router(state: ServerState) -> Router {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::HEAD])
        .allow_headers([header::RANGE])
        .expose_headers([
            header::CONTENT_RANGE,
            header::ACCEPT_RANGES,
            header::CONTENT_LENGTH,
        ])
        .allow_origin(AllowOrigin::list(
            ALLOWED_ORIGINS.map(HeaderValue::from_static),
        ));
    Router::new()
        .route("/img/{hash}", get(image))
        .route("/stream/{infohash}/{file_idx}", get(stream))
        .route("/subs/{file}", get(subtitle))
        .layer(cors)
        .with_state(state)
}

/// Runs the server until the process exits.
pub async fn serve(listener: TcpListener, router: Router) {
    if let Err(e) = axum::serve(listener, router).await {
        tracing::error!(error = %e, "local HTTP server stopped");
    }
}

async fn image(State(state): State<ServerState>, Path(hash): Path<String>) -> Response {
    match state.images.get(&hash).await {
        Ok((data, mime)) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            data,
        )
            .into_response(),
        Err(e) => {
            let status = match &e {
                ImageError::NotFound => StatusCode::NOT_FOUND,
                ImageError::Forbidden(_) => StatusCode::FORBIDDEN,
                ImageError::Upstream(_) => StatusCode::BAD_GATEWAY,
                ImageError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            if status != StatusCode::NOT_FOUND {
                tracing::warn!(%hash, error = %e, "image request failed");
            }
            (status, e.to_string()).into_response()
        }
    }
}

async fn subtitle(State(state): State<ServerState>, Path(file): Path<String>) -> Response {
    let (Some(dir), Some(id)) = (state.subs_dir.as_ref(), file.strip_suffix(".vtt")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !is_valid_sub_id(id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match tokio::fs::read(vtt_path(dir, id)).await {
        Ok(data) => (
            [
                (header::CONTENT_TYPE, "text/vtt; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            data,
        )
            .into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::warn!(%id, error = %e, "could not read subtitle");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn stream(
    State(state): State<ServerState>,
    Path((infohash, file_idx)): Path<(String, usize)>,
    headers: HeaderMap,
) -> Response {
    let Some(engine) = state.torrents.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(len) = engine.file_len(&infohash, file_idx) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let range = match parse_range(range_header, len) {
        Ok(range) => range,
        Err(_) => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{len}"))],
            )
                .into_response()
        }
    };
    let (start, end) = range.unwrap_or((0, len.saturating_sub(1)));
    let (reader, file_name) = match engine.open_reader(&infohash, file_idx, start).await {
        Ok(r) => r,
        Err(AppError::NotFound(_)) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::warn!(%infohash, file_idx, error = %e, "could not open stream");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let length = if len == 0 { 0 } else { end - start + 1 };
    let body = Body::from_stream(ReaderStream::with_capacity(reader.take(length), 64 * 1024));
    let mut response = Response::builder()
        .header(header::CONTENT_TYPE, video_mime(&file_name))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, length);
    response = if range.is_some() {
        response
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
    } else {
        response.status(StatusCode::OK)
    };
    response
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
