//! "New version" notice against a fake GitHub API, and the log files.

use std::time::Duration;

use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yts_player_lib::app::{self, UpdateChecker};

const LATEST: &str = "/repos/GabJS10/yts-movie-player/releases/latest";

fn release(tag: &str) -> serde_json::Value {
    serde_json::json!({
        "tag_name": tag,
        "name": tag,
        "html_url": format!("https://github.com/GabJS10/yts-movie-player/releases/tag/{tag}"),
        "published_at": "2026-11-01T10:00:00Z",
        "draft": false,
        "prerelease": false,
        "assets": []
    })
}

async fn github(resp: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LATEST))
        .and(header_exists("user-agent"))
        .respond_with(resp)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn offers_a_newer_release_once_per_start() {
    let server = github(ResponseTemplate::new(200).set_body_json(release("v1.2.0"))).await;
    let checker = UpdateChecker::new(&format!("{}{LATEST}", server.uri()), "1.0.0");
    let update = checker.check().await.expect("update");
    assert_eq!(update.version, "1.2.0");
    assert_eq!(
        update.url,
        "https://github.com/GabJS10/yts-movie-player/releases/tag/v1.2.0"
    );
    assert_eq!(update.published_at, "2026-11-01T10:00:00Z");
    // Cached: GitHub is asked only once.
    assert_eq!(checker.check().await, Some(update));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    let ua = server.received_requests().await.unwrap()[0]
        .headers
        .get("user-agent")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(ua, "yts-player/1.0.0");
}

#[tokio::test]
async fn never_fails() {
    for resp in [
        ResponseTemplate::new(200).set_body_json(release("v1.0.0")), // same version
        ResponseTemplate::new(200).set_body_json(release("v0.9.0")), // older
        ResponseTemplate::new(403).set_body_string("rate limited"),
        ResponseTemplate::new(404),
        ResponseTemplate::new(200).set_body_string("<html>not json</html>"),
        ResponseTemplate::new(200)
            .set_body_json(release("v9.9.9"))
            .set_delay(Duration::from_secs(30)),
    ] {
        let server = github(resp).await;
        let checker = UpdateChecker::new(&format!("{}{LATEST}", server.uri()), "1.0.0");
        let started = std::time::Instant::now();
        // The slow one is cut by the client's timeout (10 s).
        assert_eq!(checker.check().await, None);
        assert!(started.elapsed() < Duration::from_secs(15));
    }
    // No network at all.
    let checker = UpdateChecker::new("http://127.0.0.1:9/releases/latest", "1.0.0");
    assert_eq!(checker.check().await, None);
}

#[test]
fn log_files_are_daily_and_named_for_the_app() {
    let tmp = tempfile::tempdir().unwrap();
    let appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(app::LOG_FILE_PREFIX)
        .filename_suffix(app::LOG_FILE_SUFFIX)
        .max_log_files(app::LOG_FILES_KEPT)
        .build(tmp.path())
        .unwrap();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(appender))
        .finish();
    tracing::subscriber::with_default(subscriber, || tracing::info!("hello from the test"));
    let files: Vec<String> = std::fs::read_dir(tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    let name = &files[0];
    // yts-player.YYYY-MM-DD.log
    assert!(
        name.starts_with("yts-player.") && name.ends_with(".log"),
        "{name}"
    );
    assert_eq!(name.len(), "yts-player.2026-10-04.log".len(), "{name}");
    let text = std::fs::read_to_string(tmp.path().join(name)).unwrap();
    assert!(text.contains("hello from the test"));
}
