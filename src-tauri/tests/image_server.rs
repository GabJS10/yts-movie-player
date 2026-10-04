//! Local HTTP server: `/img/<hash>` downloads, stores and serves images from disk.

use std::sync::Arc;

use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};
use yts_player_lib::images::ImageStore;
use yts_player_lib::stream;

const JPEG: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00,
];

struct Harness {
    tmp: tempfile::TempDir,
    images: Arc<ImageStore>,
}

/// Starts the real router on a random port. Only `127.0.0.1` (wiremock) is allowed.
async fn start() -> Harness {
    let tmp = tempfile::tempdir().unwrap();
    let listener = stream::bind().await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let images = Arc::new(
        ImageStore::new(
            tmp.path().join("img"),
            vec!["127.0.0.1".to_string()],
            format!("http://127.0.0.1:{port}"),
        )
        .unwrap(),
    );
    let router = stream::router(stream::ServerState {
        images: Arc::clone(&images),
        torrents: None,
        subs_dir: None,
    });
    tokio::spawn(stream::serve(listener, router));
    Harness { tmp, images }
}

#[tokio::test]
async fn downloads_follows_redirect_stores_and_serves_from_disk() {
    let upstream = MockServer::start().await;
    // Same shape as yts.gg/assets/... → 301 → img.yts.gg/assets/...
    Mock::given(path("/assets/cover.jpg"))
        .respond_with(
            ResponseTemplate::new(301)
                .insert_header("location", format!("{}/img-host/cover.jpg", upstream.uri())),
        )
        .mount(&upstream)
        .await;
    Mock::given(path("/img-host/cover.jpg"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(JPEG, "image/jpeg"))
        .expect(1)
        .mount(&upstream)
        .await;

    let h = start().await;
    let remote = format!("{}/assets/cover.jpg", upstream.uri());
    let local = h.images.local_url(Some(&remote)).unwrap();

    for _ in 0..2 {
        let resp = reqwest::get(&local).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers()["content-type"], "image/jpeg");
        assert!(resp.headers()["cache-control"]
            .to_str()
            .unwrap()
            .contains("max-age"));
        assert_eq!(resp.bytes().await.unwrap().as_ref(), JPEG);
    }

    let hash = ImageStore::hash_of(&remote);
    assert_eq!(
        std::fs::read(h.tmp.path().join("img").join(&hash)).unwrap(),
        JPEG
    );

    // Served from disk even if upstream disappears.
    drop(upstream);
    let resp = reqwest::get(&local).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn disallowed_host_is_forbidden() {
    let h = start().await;
    let local = h
        .images
        .local_url(Some("https://evil.example/cover.jpg"))
        .unwrap();
    assert_eq!(reqwest::get(&local).await.unwrap().status(), 403);
}

#[tokio::test]
async fn redirect_to_disallowed_host_fails() {
    let upstream = MockServer::start().await;
    Mock::given(path("/assets/cover.jpg"))
        .respond_with(
            ResponseTemplate::new(301).insert_header("location", "https://evil.example/x.jpg"),
        )
        .mount(&upstream)
        .await;
    let h = start().await;
    let local = h
        .images
        .local_url(Some(&format!("{}/assets/cover.jpg", upstream.uri())))
        .unwrap();
    assert_eq!(reqwest::get(&local).await.unwrap().status(), 502);
}

#[tokio::test]
async fn non_image_response_is_rejected() {
    let upstream = MockServer::start().await;
    Mock::given(path("/assets/cover.jpg"))
        .respond_with(ResponseTemplate::new(200).set_body_raw("<html></html>", "text/html"))
        .mount(&upstream)
        .await;
    let h = start().await;
    let local = h
        .images
        .local_url(Some(&format!("{}/assets/cover.jpg", upstream.uri())))
        .unwrap();
    assert_eq!(reqwest::get(&local).await.unwrap().status(), 502);
}

#[tokio::test]
async fn unknown_hash_is_not_found() {
    let h = start().await;
    let base = h.images.local_url(Some("https://yts.gg/x.jpg")).unwrap();
    let unknown = format!("{}{}", &base[..base.len() - 32], "0".repeat(32));
    assert_eq!(reqwest::get(&unknown).await.unwrap().status(), 404);
}
