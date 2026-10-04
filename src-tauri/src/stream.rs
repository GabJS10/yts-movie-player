//! Local HTTP server on 127.0.0.1 (random port). Serves cached images (`/img/<hash>`) and
//! torrent files (`/stream/<infohash>/<fileIdx>`, with Range support) and converted
//! subtitles (`/subs/<id>.vtt`), plus the trailer page (`/trailer/<ytTrailerCode>`).
//!
//! Trailer: YouTube refuses an embed without a `Referer` (error 153, "Video player
//! configuration error"). Checked in WebKit on 2026-10-04: the embed loaded straight in a
//! window, or in an iframe with `referrerpolicy="no-referrer"`, fails with 153; the same
//! iframe inside a page served from `http://127.0.0.1:<port>` (which sends that origin as
//! `Referer`) plays. `tauri://localhost` sends no usable `Referer`, so the trailer window
//! loads this page instead of the embed URL.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
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

/// WebView origins (Linux/macOS and Windows).
const ALLOWED_ORIGINS: [&str; 3] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

/// The Vite dev server (`npm run tauri dev`); not allowed in release builds.
const DEV_ORIGIN: &str = "http://localhost:1420";

fn allowed_origins() -> Vec<HeaderValue> {
    let mut origins: Vec<HeaderValue> = ALLOWED_ORIGINS
        .iter()
        .map(|o| HeaderValue::from_static(o))
        .collect();
    if cfg!(debug_assertions) {
        origins.push(HeaderValue::from_static(DEV_ORIGIN));
    }
    origins
}

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
        .allow_origin(AllowOrigin::list(allowed_origins()));
    Router::new()
        .route("/img/{hash}", get(image))
        .route("/stream/{infohash}/{file_idx}", get(stream))
        .route("/subs/{file}", get(subtitle))
        .route("/trailer/{code}", get(trailer))
        .layer(cors)
        .with_state(state)
}

/// Runs the server until the process exits.
pub async fn serve(listener: TcpListener, router: Router) {
    serve_until(listener, router, tokio_util::sync::CancellationToken::new()).await;
}

/// Runs the server until `stop` is cancelled (it stops accepting at once, which frees the
/// port) or it fails. Returns the error, if any.
pub async fn serve_until(
    listener: TcpListener,
    router: Router,
    stop: tokio_util::sync::CancellationToken,
) -> Option<std::io::Error> {
    let result = axum::serve(listener, router)
        .with_graceful_shutdown(stop.cancelled_owned())
        .await;
    match result {
        Ok(()) => {
            tracing::info!("local HTTP server stopped");
            None
        }
        Err(e) => {
            tracing::error!(error = %e, "local HTTP server failed");
            Some(e)
        }
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

/// A YouTube video id: 6–20 of `[A-Za-z0-9_-]` (they are 11 today).
pub fn is_valid_yt_code(code: &str) -> bool {
    (6..=20).contains(&code.len())
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn escape_html(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// Full-window page with the `youtube-nocookie` player (YouTube IFrame API,
/// `enablejsapi=1`). `origin` is this server's origin (`http://127.0.0.1:<port>`), sent by
/// the player iframe as `Referer`. Player events go to `window.parent` (the app's modal)
/// and `window.opener` as `postMessage({ source: "yts-trailer", event, code? })`
/// (`docs/IPC.md`, "Tráiler"); a failure to load the API counts as `error`.
pub fn trailer_page(code: &str, title: &str, origin: &str) -> String {
    // `code` is validated ([A-Za-z0-9_-]); JSON keeps `origin` safe inside the script.
    let origin_js = serde_json::to_string(origin).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="referrer" content="strict-origin-when-cross-origin">
<title>{title}</title>
<style>html,body{{margin:0;height:100%;background:#000;overflow:hidden}}#player,iframe{{border:0;width:100%;height:100%;display:block}}</style>
</head>
<body>
<div id="player"></div>
<script>
(function () {{
  var targets = [];
  if (window.parent && window.parent !== window) targets.push(window.parent);
  if (window.opener) targets.push(window.opener);
  var sent = {{}};
  function send(event, code) {{
    var msg = {{ source: "yts-trailer", event: event }};
    if (typeof code === "number") msg.code = code;
    targets.forEach(function (w) {{
      try {{ w.postMessage(msg, "*"); }} catch (e) {{}}
    }});
  }}
  window.ytsTrailerSend = send;
  window.onYouTubeIframeAPIReady = function () {{
    new YT.Player("player", {{
      host: "https://www.youtube-nocookie.com",
      videoId: "{code}",
      playerVars: {{ autoplay: 1, rel: 0, playsinline: 1, enablejsapi: 1, origin: {origin_js} }},
      events: {{
        onReady: function () {{ send("ready"); }},
        onStateChange: function (e) {{
          if (e.data === YT.PlayerState.PLAYING && !sent.playing) {{ sent.playing = true; send("playing"); }}
          if (e.data === YT.PlayerState.ENDED) {{ sent.playing = false; send("ended"); }}
        }},
        onError: function (e) {{ send("error", Number(e.data)); }}
      }}
    }});
  }};
}})();
</script>
<script src="https://www.youtube.com/iframe_api" onerror="window.ytsTrailerSend('error')"></script>
</body>
</html>
"#,
        title = escape_html(title),
    )
}

#[derive(serde::Deserialize)]
struct TrailerQuery {
    #[serde(default)]
    title: String,
}

async fn trailer(
    Path(code): Path<String>,
    Query(q): Query<TrailerQuery>,
    headers: HeaderMap,
) -> Response {
    if !is_valid_yt_code(&code) {
        return StatusCode::NOT_FOUND.into_response();
    }
    // Only our own loopback origin goes to YouTube as `origin`/`Referer`.
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .filter(|h| h.starts_with("127.0.0.1:") || h.starts_with("localhost:"))
        .unwrap_or("127.0.0.1");
    let title = if q.title.trim().is_empty() {
        "Tráiler".to_owned()
    } else {
        q.title.chars().take(200).collect()
    };
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        trailer_page(&code, &title, &format!("http://{host}")),
    )
        .into_response()
}

/// `MovieDetail.trailerUrl`: the trailer page for a valid code, else `None`.
pub fn trailer_url_for(local_base: &str, code: Option<&str>, title: &str) -> Option<String> {
    let code = code?.trim();
    is_valid_yt_code(code).then(|| trailer_url(local_base, code, title))
}

/// Local URL of the trailer page (`open_trailer_window`).
pub fn trailer_url(local_base: &str, code: &str, title: &str) -> String {
    let title: String = url::form_urlencoded::byte_serialize(title.as_bytes()).collect();
    format!(
        "{}/trailer/{code}?title={title}",
        local_base.trim_end_matches('/')
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_codes() {
        assert!(is_valid_yt_code("9ix7TUGVYIo"));
        assert!(is_valid_yt_code("a-b_c1"));
        for bad in [
            "",
            "short",
            "has space x",
            "x\"><script",
            "../../etc",
            &"a".repeat(21),
        ] {
            assert!(!is_valid_yt_code(bad), "{bad}");
        }
    }

    #[test]
    fn trailer_page_uses_the_iframe_api_and_escapes_the_title() {
        let page = trailer_page("9ix7TUGVYIo", "Tom & \"Jerry\" <3", "http://127.0.0.1:4321");
        assert!(page.contains(r#"videoId: "9ix7TUGVYIo""#));
        assert!(page.contains(r#"host: "https://www.youtube-nocookie.com""#));
        assert!(page.contains(r#"enablejsapi: 1, origin: "http://127.0.0.1:4321""#));
        assert!(page.contains(r#"<script src="https://www.youtube.com/iframe_api""#));
        assert!(page.contains(r#"source: "yts-trailer""#));
        for event in ["\"ready\"", "\"playing\"", "\"ended\"", "\"error\""] {
            assert!(page.contains(event), "{event}");
        }
        assert!(page.contains(r#"content="strict-origin-when-cross-origin""#));
        assert!(page.contains("<title>Tom &amp; &quot;Jerry&quot; &lt;3</title>"));
        assert!(!page.contains("<3"));
        assert_eq!(
            trailer_url("http://127.0.0.1:4321/", "9ix7TUGVYIo", "Dune: Parte 2"),
            "http://127.0.0.1:4321/trailer/9ix7TUGVYIo?title=Dune%3A+Parte+2"
        );
        assert_eq!(
            trailer_url_for("http://127.0.0.1:1", Some("bad code!"), "x"),
            None
        );
        assert_eq!(trailer_url_for("http://127.0.0.1:1", None, "x"), None);
    }
}
