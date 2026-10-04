//! Secrets (API key, username, password, token) never reach the logs, even at TRACE level
//! (which includes reqwest/hyper). In its own test binary because it installs a global
//! subscriber (`tracing` caches callsite interest process-wide).

use std::sync::{Arc, Mutex};

use wiremock::matchers::path;
use wiremock::{Mock, MockServer, ResponseTemplate};
use yts_player_lib::subtitles::{Credentials, Release, SubtitlesClient, SubtitlesConfig};
use yts_player_lib::types::{Quality, TorrentSource};

const KEY: &str = "test-api-key-123";
const USER: &str = "movie-fan";
const PASS: &str = "s3cret-pass";
const TOKEN: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.test-token-value";

fn fixture(name: &str) -> String {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// Collects everything written by the tracing subscriber.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn secrets_never_reach_the_logs() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    // Global: the only test in this binary.
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let cfg = SubtitlesConfig {
        base_url: format!("{}/api/v1", server.uri()),
        ..SubtitlesConfig::new(tmp.path().join("subs"), "http://127.0.0.1:1".into())
    };
    let creds = |login: bool| Credentials {
        api_key: Some(KEY.into()),
        username: login.then(|| USER.into()),
        password: login.then(|| PASS.into()),
    };
    let client = SubtitlesClient::new(cfg, creds(true)).unwrap();
    let ok =
        |name: &str| ResponseTemplate::new(200).set_body_raw(fixture(name), "application/json");
    let download = fixture("opensubtitles/download.json")
        .replace("LINK", &format!("{}/files/sub.srt", server.uri()));
    Mock::given(path("/api/v1/login"))
        .respond_with(ok("opensubtitles/login.json"))
        .mount(&server)
        .await;
    Mock::given(path("/api/v1/subtitles"))
        .respond_with(ok("opensubtitles/search.json"))
        .mount(&server)
        .await;
    Mock::given(path("/api/v1/download"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(download, "application/json"))
        .mount(&server)
        .await;
    Mock::given(path("/files/sub.srt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("subs/bom-utf8.srt"), "text/plain"),
        )
        .mount(&server)
        .await;
    Mock::given(path("/api/v1/infos/user"))
        .respond_with(ok("opensubtitles/user.json"))
        .mount(&server)
        .await;

    let release = Release {
        quality: Quality::P1080,
        source: TorrentSource::Bluray,
    };
    client
        .search("tt0133093", "es", Some(release))
        .await
        .unwrap();
    client.load("9003").await.unwrap();
    client.status().await.unwrap();
    client.set_credentials(creds(false)).await;

    let logs = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("subtitle downloaded"), "logging was captured");
    for secret in [KEY, USER, PASS, TOKEN] {
        assert!(!logs.contains(secret), "{secret} leaked:\n{logs}");
    }
}
