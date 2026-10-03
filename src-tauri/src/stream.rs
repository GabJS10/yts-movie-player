//! Local HTTP server on 127.0.0.1 (random port). Serves cached images (`/img/<hash>`);
//! phase 3 adds `/stream/<infohash>/<fileIdx>` with Range support.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::images::{ImageError, ImageStore};

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
}

/// Binds to a random port on the loopback interface.
pub async fn bind() -> std::io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await
}

pub fn router(state: ServerState) -> Router {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::HEAD])
        .allow_origin(AllowOrigin::list(
            ALLOWED_ORIGINS.map(HeaderValue::from_static),
        ));
    Router::new()
        .route("/img/{hash}", get(image))
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
