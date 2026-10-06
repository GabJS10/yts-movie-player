//! OpenSubtitles client against wiremock: search + ranking, download + disk cache, login,
//! auth/quota errors, local files, the `/subs` route and secrets kept out of the logs.

use std::sync::Arc;

use wiremock::matchers::{body_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use yts_player_lib::error::AppError;
use yts_player_lib::images::ImageStore;
use yts_player_lib::stream;
use yts_player_lib::subtitles::{
    user_agent, Credentials, Release, SubtitlesClient, SubtitlesConfig,
};
use yts_player_lib::types::{Quality, TorrentSource};

const KEY: &str = "test-api-key-123";
const USER: &str = "movie-fan";
const PASS: &str = "s3cret-pass";
const TOKEN: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.test-token-value";
const LOCAL: &str = "http://127.0.0.1:1";
const IMDB: &str = "tt0133093";

fn fixture(name: &str) -> String {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn fixture_bytes(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn json(name: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(fixture(name), "application/json")
}

struct Harness {
    tmp: tempfile::TempDir,
    server: MockServer,
    client: SubtitlesClient,
}

fn creds(login: bool) -> Credentials {
    Credentials {
        api_key: Some(KEY.into()),
        username: login.then(|| USER.into()),
        password: login.then(|| PASS.into()),
    }
}

async fn harness(creds: Credentials) -> Harness {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let cfg = SubtitlesConfig {
        base_url: format!("{}/api/v1", server.uri()),
        ..SubtitlesConfig::new(tmp.path().join("subs"), LOCAL.into())
    };
    let client = SubtitlesClient::new(cfg, creds).unwrap();
    Harness {
        tmp,
        server,
        client,
    }
}

async fn mount_search(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v1/subtitles"))
        .and(query_param("imdb_id", "133093"))
        .and(query_param("languages", "es"))
        .and(header("Api-Key", KEY))
        .and(header("User-Agent", user_agent().as_str()))
        .respond_with(json("opensubtitles/search.json"))
        .mount(server)
        .await;
}

/// `/download` answering with a link to `/files/sub.srt` on the same mock server.
async fn mount_download(server: &MockServer) {
    let body = fixture("opensubtitles/download.json")
        .replace("LINK", &format!("{}/files/sub.srt", server.uri()));
    Mock::given(method("POST"))
        .and(path("/api/v1/download"))
        .and(header("Api-Key", KEY))
        .and(body_json(serde_json::json!({ "file_id": 9003 })))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/files/sub.srt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes(fixture_bytes("subs/messy-cp1252.srt")),
        )
        .mount(server)
        .await;
}

async fn mount_login(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/v1/login"))
        .and(header("Api-Key", KEY))
        .and(body_json(
            serde_json::json!({ "username": USER, "password": PASS }),
        ))
        .respond_with(json("opensubtitles/login.json"))
        .mount(server)
        .await;
}

async fn count(server: &MockServer, p: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == p)
        .count()
}

async fn requests_to(server: &MockServer, p: &str) -> Vec<Request> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.url.path() == p)
        .collect()
}

fn bearer(r: &Request) -> Option<String> {
    r.headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

const BLURAY_1080: Release = Release {
    quality: Quality::P1080,
    source: TorrentSource::Bluray,
};

#[tokio::test]
async fn search_ranks_and_flags_results() {
    let h = harness(creds(false)).await;
    mount_search(&h.server).await;

    let results = h
        .client
        .search(IMDB, "ES", Some(BLURAY_1080))
        .await
        .unwrap();
    let ids: Vec<&str> = results.iter().map(|o| o.id.as_str()).collect();
    // Release matches (plain first, then HI) → others (plain, then AI/machine).
    // Forced-only and file-less entries are dropped.
    assert_eq!(ids, ["9003", "9008", "9002", "9001", "9004", "9006"]);
    let by_id = |id: &str| results.iter().find(|o| o.id == id).unwrap();
    assert!(by_id("9003").matches_release);
    assert!(by_id("9002").hearing_impaired && by_id("9002").matches_release);
    assert!(!by_id("9001").matches_release); // 720p
    assert!(by_id("9004").ai_translated); // machine_translated
    assert!(by_id("9006").ai_translated);
    assert_eq!(by_id("9008").lang, "pt");
    assert_eq!(
        by_id("9003").label.as_deref(),
        Some("The.Matrix.1999.1080p.BrRip.x264.YIFY")
    );
    // No release and no file name → null label (the UI names it).
    assert_eq!(by_id("9006").label, None);
    // pageUrl from attributes.url; missing or not an opensubtitles https page → null.
    assert_eq!(
        by_id("9003").page_url.as_deref(),
        Some("https://www.opensubtitles.com/es/subtitles/103")
    );
    assert_eq!(by_id("9006").page_url, None);
    assert_eq!(by_id("9008").page_url, None);
    assert!(results.iter().all(|o| !o.cached));
    // Without a release nothing "matches": plain by downloads, then HI/AI.
    let plain = h.client.search(IMDB, "es", None).await.unwrap();
    assert!(plain.iter().all(|o| !o.matches_release));
    assert_eq!(plain[0].id, "9001");
}

#[tokio::test]
async fn without_key_api_calls_fail_with_subtitles_auth_but_files_load() {
    let h = harness(Credentials::default()).await;
    assert!(matches!(
        h.client.search(IMDB, "es", None).await,
        Err(AppError::SubtitlesAuth(_))
    ));
    assert!(matches!(
        h.client.load("9003").await,
        Err(AppError::SubtitlesAuth(_))
    ));
    let status = h.client.status().await.unwrap();
    assert!(!status.configured && !status.logged_in);
    assert_eq!(count(&h.server, "/api/v1/subtitles").await, 0);

    let file = h.tmp.path().join("Mi Película.SRT");
    std::fs::write(&file, fixture_bytes("subs/messy-cp1252.srt")).unwrap();
    let track = h.client.load_file(&file).await.unwrap();
    assert_eq!(track.label.as_deref(), Some("Mi Película.SRT"));
    assert_eq!(track.lang, None);
    let id = track
        .track_url
        .strip_prefix(&format!("{LOCAL}/subs/"))
        .and_then(|s| s.strip_suffix(".vtt"))
        .unwrap();
    assert!(id.starts_with("f-"));
    let vtt = std::fs::read_to_string(h.tmp.path().join("subs").join(format!("{id}.vtt"))).unwrap();
    assert_eq!(vtt, fixture("subs/messy-cp1252.expected.vtt"));

    // Wrong extension, missing file, not a subtitle.
    let txt = h.tmp.path().join("notes.txt");
    std::fs::write(&txt, "hola").unwrap();
    assert!(matches!(
        h.client.load_file(&txt).await,
        Err(AppError::InvalidInput(_))
    ));
    assert!(matches!(
        h.client.load_file(&h.tmp.path().join("missing.srt")).await,
        Err(AppError::NotFound(_))
    ));
    let junk = h.tmp.path().join("junk.srt");
    std::fs::write(&junk, "no timings here").unwrap();
    assert!(matches!(
        h.client.load_file(&junk).await,
        Err(AppError::InvalidInput(_))
    ));
}

#[tokio::test]
async fn download_converts_and_disk_cache_avoids_a_second_download() {
    let h = harness(creds(false)).await;
    mount_search(&h.server).await;
    mount_download(&h.server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/infos/formats"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(r#"{"data":{}}"#, "application/json"))
        .mount(&h.server)
        .await;
    h.client
        .search(IMDB, "es", Some(BLURAY_1080))
        .await
        .unwrap();

    let track = h.client.load("9003").await.unwrap();
    assert_eq!(track.track_url, format!("{LOCAL}/subs/9003.vtt"));
    assert_eq!(track.lang.as_deref(), Some("es"));
    assert_eq!(
        track.label.as_deref(),
        Some("The.Matrix.1999.1080p.BrRip.x264.YIFY")
    );
    let cached = h.tmp.path().join("subs/9003.vtt");
    assert_eq!(
        std::fs::read_to_string(&cached).unwrap(),
        fixture("subs/messy-cp1252.expected.vtt")
    );
    // Anonymous: no Authorization header.
    let downloads = requests_to(&h.server, "/api/v1/download").await;
    assert_eq!(downloads.len(), 1);
    assert_eq!(bearer(&downloads[0]), None);

    // Quota info from the download response.
    let status = h.client.status().await.unwrap();
    assert!(status.configured && !status.logged_in);
    assert_eq!(status.remaining_downloads, Some(2));
    assert_eq!(status.reset_at.as_deref(), Some("2026-10-04T13:03:16.000Z"));

    // The search now reports it as cached (loading it is free).
    let results = h
        .client
        .search(IMDB, "es", Some(BLURAY_1080))
        .await
        .unwrap();
    for o in &results {
        assert_eq!(o.cached, o.id == "9003", "{}", o.id);
    }

    // Second load: from disk, no API call, no quota.
    let again = h.client.load("9003").await.unwrap();
    assert_eq!(again, track);
    assert_eq!(count(&h.server, "/api/v1/download").await, 1);
    assert_eq!(count(&h.server, "/files/sub.srt").await, 1);

    assert!(matches!(
        h.client.load("../etc/passwd").await,
        Err(AppError::InvalidInput(_))
    ));
}

#[tokio::test]
async fn login_once_and_use_the_token() {
    let h = harness(creds(true)).await;
    mount_login(&h.server).await;
    mount_search(&h.server).await;
    mount_download(&h.server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/infos/user"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(json("opensubtitles/user.json"))
        .mount(&h.server)
        .await;

    h.client.search(IMDB, "es", None).await.unwrap();
    h.client.load("9003").await.unwrap();
    let status = h.client.status().await.unwrap();
    assert!(status.configured && status.logged_in);
    assert_eq!(status.remaining_downloads, Some(17));

    // One login for the whole session; the token goes with every API call.
    assert_eq!(count(&h.server, "/api/v1/login").await, 1);
    for p in [
        "/api/v1/subtitles",
        "/api/v1/download",
        "/api/v1/infos/user",
    ] {
        for r in requests_to(&h.server, p).await {
            assert_eq!(
                bearer(&r).as_deref(),
                Some(format!("Bearer {TOKEN}").as_str()),
                "{p}"
            );
        }
    }

    // New credentials: the token is dropped and the next call logs in again.
    h.client.set_credentials(creds(true)).await; // same → no-op
    h.client.search(IMDB, "es", None).await.unwrap();
    assert_eq!(count(&h.server, "/api/v1/login").await, 1);
    h.client.set_credentials(creds(false)).await;
    h.client.search(IMDB, "es", None).await.unwrap();
    let last = requests_to(&h.server, "/api/v1/subtitles").await;
    assert_eq!(bearer(last.last().unwrap()), None);
}

#[tokio::test]
async fn rejected_login_is_not_retried_and_search_goes_on_anonymously() {
    let h = harness(creds(true)).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/login"))
        .respond_with(ResponseTemplate::new(401).set_body_raw(
            r#"{"message":"Error, invalid username/password","status":401}"#,
            "application/json",
        ))
        .mount(&h.server)
        .await;
    mount_search(&h.server).await;

    h.client.search(IMDB, "es", None).await.unwrap();
    h.client.search(IMDB, "es", None).await.unwrap();
    assert_eq!(count(&h.server, "/api/v1/login").await, 1);
    for r in requests_to(&h.server, "/api/v1/subtitles").await {
        assert_eq!(bearer(&r), None);
    }
    // "Probar" tries again and reports it.
    assert!(matches!(
        h.client.status().await,
        Err(AppError::SubtitlesAuth(_))
    ));
    assert_eq!(count(&h.server, "/api/v1/login").await, 2);
}

#[tokio::test]
async fn invalid_key_is_subtitles_auth() {
    let h = harness(creds(false)).await;
    for p in ["/api/v1/subtitles", "/api/v1/infos/formats"] {
        Mock::given(path(p))
            .respond_with(ResponseTemplate::new(403).set_body_raw(
                r#"{"message":"You cannot consume this service"}"#,
                "application/json",
            ))
            .mount(&h.server)
            .await;
    }
    assert!(matches!(
        h.client.search(IMDB, "es", None).await,
        Err(AppError::SubtitlesAuth(_))
    ));
    assert!(matches!(
        h.client.status().await,
        Err(AppError::SubtitlesAuth(_))
    ));
}

#[tokio::test]
async fn quota_exhausted_and_rate_limit_are_subtitles_quota() {
    let h = harness(creds(false)).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/download"))
        .respond_with(ResponseTemplate::new(406).set_body_raw(
            fixture("opensubtitles/download_quota.json"),
            "application/json",
        ))
        .mount(&h.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/infos/formats"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(r#"{"data":{}}"#, "application/json"))
        .mount(&h.server)
        .await;

    match h.client.load("9003").await {
        Err(AppError::SubtitlesQuota(msg)) => {
            assert!(msg.contains("2026-10-04T13:03:16.000Z"), "{msg}")
        }
        other => panic!("{other:?}"),
    }
    let status = h.client.status().await.unwrap();
    assert_eq!(status.remaining_downloads, Some(0));
    assert_eq!(status.reset_at.as_deref(), Some("2026-10-04T13:03:16.000Z"));
    assert!(!h.tmp.path().join("subs/9003.vtt").exists());

    Mock::given(method("GET"))
        .and(path("/api/v1/subtitles"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
        .mount(&h.server)
        .await;
    assert!(matches!(
        h.client.search(IMDB, "es", None).await,
        Err(AppError::SubtitlesQuota(_))
    ));
}

#[tokio::test]
async fn subs_route_serves_vtt_with_cors() {
    let h = harness(creds(false)).await;
    let subs = h.tmp.path().join("subs");
    std::fs::write(subs.join("9003.vtt"), "WEBVTT\n\n").unwrap();

    let listener = stream::bind().await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let images =
        Arc::new(ImageStore::new(h.tmp.path().join("img"), Vec::new(), base.clone()).unwrap());
    tokio::spawn(stream::serve(
        listener,
        stream::router(stream::ServerState {
            images,
            torrents: None,
            subs_dir: Some(subs),
        }),
    ));
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("{base}/subs/9003.vtt"))
        .header("Origin", "tauri://localhost")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()["access-control-allow-origin"],
        "tauri://localhost"
    );
    assert!(resp.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/vtt"));
    assert_eq!(resp.text().await.unwrap(), "WEBVTT\n\n");
    for bad in ["missing.vtt", "9003.srt", "..%2F9003.vtt", "f-zz.vtt"] {
        let status = http
            .get(format!("{base}/subs/{bad}"))
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, 404, "{bad}");
    }
}
