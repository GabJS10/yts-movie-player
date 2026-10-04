//! YTS client against real API fixtures served by wiremock: parsing, conversion,
//! failover, cache and primary recovery.

use std::sync::Arc;
use std::time::Duration;

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yts_player_lib::error::AppError;
use yts_player_lib::images::ImageStore;
use yts_player_lib::types::{EndpointRole, ListMoviesParams, Quality, TorrentSource, VideoCodec};
use yts_player_lib::yts::{YtsClient, YtsConfig};

const LOCAL: &str = "http://127.0.0.1:1";

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

fn json(name: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(fixture(name), "application/json")
}

fn base(server: &MockServer) -> String {
    format!("{}/api/v2/", server.uri())
}

struct Harness {
    _tmp: tempfile::TempDir,
    client: YtsClient,
}

fn client_with(base_urls: Vec<String>, tweak: impl FnOnce(&mut YtsConfig)) -> Harness {
    let tmp = tempfile::tempdir().unwrap();
    let images = Arc::new(ImageStore::new(tmp.path().join("img"), Vec::new(), LOCAL).unwrap());
    let mut config = YtsConfig {
        base_urls,
        request_timeout: Duration::from_secs(2),
        ..YtsConfig::default()
    };
    tweak(&mut config);
    Harness {
        _tmp: tmp,
        client: YtsClient::new(config, images).unwrap(),
    }
}

fn client(base_urls: Vec<String>) -> Harness {
    client_with(base_urls, |_| {})
}

async fn mount_fixture(server: &MockServer, endpoint: &str, name: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v2/{endpoint}")))
        .respond_with(json(name))
        .mount(server)
        .await;
}

async fn requests(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

fn is_local_img(url: &str) -> bool {
    url.starts_with(&format!("{LOCAL}/img/")) && url.len() == LOCAL.len() + 5 + 32
}

// ---------------------------------------------------------------------------
// Parsing and conversion of real fixtures
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_movies_fixture() {
    let server = MockServer::start().await;
    mount_fixture(&server, "list_movies.json", "list_movies.json").await;
    let h = client(vec![base(&server)]);

    let page = h
        .client
        .list_movies(&ListMoviesParams {
            limit: Some(5),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 77549);
    assert_eq!((page.page, page.limit), (1, 5));
    assert!(page.has_more);
    assert_eq!(page.movies.len(), 5);

    let first = &page.movies[0];
    assert_eq!(first.id, 78890);
    assert_eq!(first.title, "Melody");
    assert!(first.imdb_code.starts_with("tt"));
    assert_eq!(first.qualities, vec![Quality::P720, Quality::P1080]);
    assert!(first.has_x264);
    // Seeds per torrent in the fixture: [0, 0], [0, 100], [0, 23], [56, 100].
    assert_eq!(first.max_seeds, 0);
    let max_seeds: Vec<u32> = page.movies.iter().map(|m| m.max_seeds).collect();
    assert_eq!(max_seeds, vec![0, 0, 100, 23, 100]);
    assert!(is_local_img(first.cover_url.as_deref().unwrap()));
    assert!(is_local_img(first.cover_large_url.as_deref().unwrap()));
    assert!(is_local_img(first.background_url.as_deref().unwrap()));
    assert_ne!(first.cover_url, first.cover_large_url);

    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs[0].url.query(), Some("page=1&limit=5"));
}

#[tokio::test]
async fn movie_details_fixture() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/movie_details.json"))
        .and(query_param("movie_id", "38698"))
        .and(query_param("with_images", "true"))
        .and(query_param("with_cast", "true"))
        .respond_with(json("movie_details.json"))
        .mount(&server)
        .await;
    let h = client(vec![base(&server)]);

    let movie = h.client.get_movie(38698).await.unwrap();
    let s = &movie.summary_fields;
    assert_eq!(s.title, "The Matrix Resurrections");
    assert_eq!((s.year, s.runtime_min, s.rating), (2021, 148, 5.6));
    assert_eq!(s.genres, vec!["Action", "Sci-Fi"]);
    assert_eq!(
        s.qualities,
        vec![Quality::P720, Quality::P1080, Quality::P2160]
    );
    assert!(s.has_x264);
    // Seeds per torrent: 37, 93, 39, 100, 100.
    assert_eq!(s.max_seeds, 100);
    assert_eq!(movie.screenshot_urls.len(), 3);
    assert!(movie.screenshot_urls.iter().all(|u| is_local_img(u)));
    assert_eq!(
        movie.screenshot_urls[0],
        format!(
            "{LOCAL}/img/{}",
            ImageStore::hash_of(
                "https://yts.gg/assets/images/movies/the_matrix_resurrections_2021/large-screenshot1.jpg"
            )
        )
    );
    assert!(movie
        .summary
        .starts_with("Return to a world of two realities"));
    assert_eq!(movie.language, "en");
    assert_eq!(movie.mpa_rating.as_deref(), Some("TV-PG"));
    assert_eq!(movie.yt_trailer_code.as_deref(), Some("nNpvWBuTfrc"));
    assert_eq!(
        movie.trailer_url.as_deref(),
        Some("http://127.0.0.1:1/trailer/nNpvWBuTfrc?title=The+Matrix+Resurrections")
    );
    assert!(!movie.is_favorite);
    assert!(movie.progress.is_none() && movie.download.is_none());

    assert_eq!(movie.cast.len(), 4);
    assert_eq!(movie.cast[0].name, "Keanu Reeves");
    assert_eq!(
        movie.cast[0].character.as_deref(),
        Some("Neo / Thomas Anderson")
    );
    assert!(is_local_img(movie.cast[0].image_url.as_deref().unwrap()));

    assert_eq!(movie.torrents.len(), 5);
    let t = &movie.torrents[0];
    assert_eq!(t.infohash, "107facda1820df8212022863fffa19a971563595");
    assert_eq!(t.quality, Quality::P720);
    assert_eq!(t.source, TorrentSource::Bluray);
    assert_eq!(t.video_codec, VideoCodec::X264);
    assert_eq!(t.bit_depth, Some(8));
    assert_eq!(t.audio_channels.as_deref(), Some("2.0"));
    assert_eq!(t.size_bytes, 1_428_076_626);
    assert_eq!((t.seeds, t.peers), (37, 3));
    assert_eq!(t.uploaded_at.as_deref(), Some("2022-02-19T13:38:40Z"));
    let uhd = &movie.torrents[4];
    assert_eq!(
        (uhd.quality, uhd.video_codec, uhd.bit_depth),
        (Quality::P2160, VideoCodec::X265, Some(10))
    );

    // Serialized MovieDetail exposes the summary fields at the top level.
    let value = serde_json::to_value(&movie).unwrap();
    assert_eq!(value["id"], 38698);
    assert_eq!(value["hasX264"], true);
    assert_eq!(value["torrents"][0]["source"], "bluray");
}

#[tokio::test]
async fn movie_details_unknown_id_is_not_found() {
    let server = MockServer::start().await;
    mount_fixture(
        &server,
        "movie_details.json",
        "movie_details_not_found.json",
    )
    .await;
    let h = client(vec![base(&server)]);
    assert!(matches!(
        h.client.get_movie(999_999_999).await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn empty_search_fixture() {
    let server = MockServer::start().await;
    mount_fixture(&server, "list_movies.json", "list_movies_empty.json").await;
    let h = client(vec![base(&server)]);

    let page = h
        .client
        .list_movies(&ListMoviesParams {
            query: Some("zzzxqqnotamovie".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(page.movies.is_empty());
    assert_eq!(page.total, 0);
    assert!(!page.has_more);
}

#[tokio::test]
async fn suggestions_fixture() {
    let server = MockServer::start().await;
    mount_fixture(&server, "movie_suggestions.json", "movie_suggestions.json").await;
    let h = client(vec![base(&server)]);

    let movies = h.client.suggestions(38698).await.unwrap();
    // movie_count is 0 in the payload; the movies are what counts.
    assert_eq!(movies.len(), 4);
    assert_eq!(movies[0].title, "Don't Look Up");
    // Suggestions have no large cover: falls back to the medium one.
    assert!(movies[0].cover_url.is_some());
    assert_eq!(movies[0].cover_large_url, movies[0].cover_url);
    assert!(movies
        .iter()
        .any(|m| m.qualities.contains(&Quality::ThreeD)));
    for m in &movies {
        let mut sorted = m.qualities.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(m.qualities, sorted);
    }
}

#[tokio::test]
async fn missing_covers_become_null_and_large_falls_back_to_medium() {
    let server = MockServer::start().await;
    let body = r#"{"status":"ok","data":{"movie_count":2,"movies":[
        {"id":1,"title":"No covers","medium_cover_image":"","large_cover_image":null},
        {"id":2,"title":"Medium only","medium_cover_image":"https://yts.gg/assets/m.jpg"}
    ]}}"#;
    Mock::given(path("/api/v2/list_movies.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;
    let h = client(vec![base(&server)]);

    let page = h
        .client
        .list_movies(&ListMoviesParams::default())
        .await
        .unwrap();
    let (none, medium) = (&page.movies[0], &page.movies[1]);
    assert_eq!(
        (none.cover_url.as_deref(), none.cover_large_url.as_deref()),
        (None, None)
    );
    assert_eq!(none.background_url, None);
    assert!(is_local_img(medium.cover_url.as_deref().unwrap()));
    assert_eq!(medium.cover_large_url, medium.cover_url);

    let value = serde_json::to_value(none).unwrap();
    assert_eq!(value["coverUrl"], serde_json::Value::Null);
    assert_eq!(value["coverLargeUrl"], serde_json::Value::Null);
}

#[tokio::test]
async fn details_without_torrents_or_screenshots() {
    let server = MockServer::start().await;
    let body = r#"{"status":"ok","data":{"movie":{"id":7,"title":"Bare",
        "large_screenshot_image1":"","large_screenshot_image2":null,
        "large_screenshot_image3":"https://yts.gg/assets/s3.jpg"}}}"#;
    Mock::given(path("/api/v2/movie_details.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;
    let h = client(vec![base(&server)]);

    let movie = h.client.get_movie(7).await.unwrap();
    assert_eq!(movie.summary_fields.max_seeds, 0);
    assert!(movie.torrents.is_empty());
    // Empty and null screenshots are dropped.
    assert_eq!(movie.screenshot_urls.len(), 1);
    assert!(is_local_img(&movie.screenshot_urls[0]));

    let value = serde_json::to_value(&movie).unwrap();
    assert_eq!(value["maxSeeds"], 0);
    assert_eq!(value["screenshotUrls"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn details_with_no_screenshots_is_empty_list() {
    let server = MockServer::start().await;
    let body = r#"{"status":"ok","data":{"movie":{"id":8,"title":"None"}}}"#;
    Mock::given(path("/api/v2/movie_details.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;
    let h = client(vec![base(&server)]);
    let value = serde_json::to_value(h.client.get_movie(8).await.unwrap()).unwrap();
    assert_eq!(value["screenshotUrls"], serde_json::json!([]));
}

// ---------------------------------------------------------------------------
// Failover, cache and recovery
// ---------------------------------------------------------------------------

async fn assert_failover(primary_response: ResponseTemplate) {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(primary_response)
        .mount(&primary)
        .await;
    mount_fixture(&fallback, "list_movies.json", "list_movies.json").await;
    let h = client_with(vec![base(&primary), base(&fallback)], |c| {
        c.request_timeout = Duration::from_millis(500);
    });

    let page = h
        .client
        .list_movies(&ListMoviesParams::default())
        .await
        .unwrap();
    assert_eq!(page.movies.len(), 5);
    assert_eq!(h.client.active_base_url(), base(&fallback));

    // The next (uncached) request goes straight to the fallback.
    h.client
        .list_movies(&ListMoviesParams {
            page: Some(2),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(requests(&primary).await, 1);
    assert_eq!(requests(&fallback).await, 2);
}

#[tokio::test]
async fn fails_over_on_5xx() {
    assert_failover(ResponseTemplate::new(503)).await;
}

#[tokio::test]
async fn fails_over_on_timeout() {
    assert_failover(json("list_movies.json").set_delay(Duration::from_secs(3))).await;
}

#[tokio::test]
async fn fails_over_on_html() {
    assert_failover(
        ResponseTemplate::new(200).set_body_raw("<html>Just a moment...</html>", "text/html"),
    )
    .await;
}

#[tokio::test]
async fn fails_over_on_status_not_ok() {
    assert_failover(ResponseTemplate::new(200).set_body_raw(
        r#"{"status":"error","status_message":"Something went wrong"}"#,
        "application/json",
    ))
    .await;
}

#[tokio::test]
async fn caches_responses() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/list_movies.json"))
        .respond_with(json("list_movies.json"))
        .expect(1)
        .mount(&server)
        .await;
    let h = client(vec![base(&server)]);
    let params = ListMoviesParams {
        query: Some("matrix".into()),
        ..Default::default()
    };
    let first = h.client.list_movies(&params).await.unwrap();
    let second = h.client.list_movies(&params).await.unwrap();
    assert_eq!(first, second);
    // `expect(1)` is verified when the server is dropped.
}

#[tokio::test]
async fn concurrent_identical_requests_share_one_upstream_call() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v2/list_movies.json"))
        .respond_with(json("list_movies.json").set_delay(Duration::from_millis(200)))
        .expect(1)
        .mount(&server)
        .await;
    let h = client(vec![base(&server)]);
    // `page: None` and `page: Some(1)` map to the same canonical query.
    let a = ListMoviesParams::default();
    let b = ListMoviesParams {
        page: Some(1),
        ..Default::default()
    };
    let (ra, rb) = tokio::join!(h.client.list_movies(&a), h.client.list_movies(&b));
    assert_eq!(ra.unwrap(), rb.unwrap());
}

#[tokio::test]
async fn returns_to_primary_after_recheck_interval() {
    let primary = MockServer::start().await;
    let fallback = MockServer::start().await;
    // Primary fails once, then recovers.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&primary)
        .await;
    mount_fixture(&primary, "list_movies.json", "list_movies.json").await;
    mount_fixture(&fallback, "list_movies.json", "list_movies.json").await;
    let h = client_with(vec![base(&primary), base(&fallback)], |c| {
        c.primary_recheck_interval = Duration::from_millis(300);
    });
    let page = |n| ListMoviesParams {
        page: Some(n),
        ..Default::default()
    };

    h.client.list_movies(&page(1)).await.unwrap();
    assert_eq!(h.client.active_base_url(), base(&fallback));

    // Before the interval: stays on the fallback without touching the primary.
    h.client.list_movies(&page(2)).await.unwrap();
    assert_eq!(requests(&primary).await, 1);

    tokio::time::sleep(Duration::from_millis(350)).await;
    h.client.list_movies(&page(3)).await.unwrap();
    assert_eq!(h.client.active_base_url(), base(&primary));
    assert_eq!(requests(&primary).await, 2);
    assert_eq!(requests(&fallback).await, 2);
}

#[tokio::test]
async fn all_endpoints_unreachable_is_network_error() {
    // Nothing listens on these ports.
    let h = client(vec![
        "http://127.0.0.1:9/api/v2/".into(),
        "http://127.0.0.1:10/api/v2/".into(),
    ]);
    assert!(matches!(
        h.client.list_movies(&ListMoviesParams::default()).await,
        Err(AppError::Network(_))
    ));
}

#[tokio::test]
async fn all_endpoints_broken_is_api_unavailable() {
    let a = MockServer::start().await;
    let b = MockServer::start().await;
    for s in [&a, &b] {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(s)
            .await;
    }
    let h = client(vec![base(&a), base(&b)]);
    let err = h
        .client
        .list_movies(&ListMoviesParams::default())
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::ApiUnavailable(_)));
    assert_eq!(
        serde_json::to_value(&err).unwrap()["code"],
        "api_unavailable"
    );
}

#[tokio::test]
async fn api_status_probes_every_endpoint() {
    let good = MockServer::start().await;
    let bad = MockServer::start().await;
    Mock::given(path("/api/v2/list_movies.json"))
        .and(query_param("limit", "1"))
        .respond_with(json("list_movies.json"))
        .mount(&good)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&bad)
        .await;
    let h = client(vec![base(&good), base(&bad)]);

    let status = h.client.api_status().await;
    assert_eq!(status.len(), 2);
    assert_eq!(status[0].base_url, base(&good));
    assert_eq!(status[0].role, EndpointRole::Active);
    assert!(status[0].ok && status[0].latency_ms.is_some());
    assert_eq!(status[1].role, EndpointRole::Fallback);
    assert!(!status[1].ok && status[1].latency_ms.is_none());
}

#[tokio::test]
async fn base_urls_change_at_runtime_and_drop_the_cache() {
    let old = MockServer::start().await;
    let new = MockServer::start().await;
    mount_fixture(&old, "list_movies.json", "list_movies.json").await;
    mount_fixture(&new, "list_movies.json", "list_movies.json").await;
    let h = client(vec![base(&old)]);
    let params = ListMoviesParams::default();

    h.client.list_movies(&params).await.unwrap();
    assert_eq!(requests(&old).await, 1);

    // Same list: no-op, the cache stays.
    h.client.set_base_urls(&[base(&old)]).unwrap();
    h.client.list_movies(&params).await.unwrap();
    assert_eq!(requests(&old).await, 1);

    // New list: same request goes to the new server (cache dropped).
    h.client.set_base_urls(&[base(&new), base(&old)]).unwrap();
    assert_eq!(h.client.active_base_url(), base(&new));
    h.client.list_movies(&params).await.unwrap();
    assert_eq!(requests(&new).await, 1);
    assert_eq!(requests(&old).await, 1);
    let status = h.client.api_status().await;
    assert_eq!(status.len(), 2);
    assert_eq!(status[0].base_url, base(&new));
    assert_eq!(status[0].role, EndpointRole::Active);

    assert!(matches!(
        h.client.set_base_urls(&[]),
        Err(AppError::InvalidInput(_))
    ));
}
