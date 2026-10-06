//! Subtitles for the external player: every `ExternalPlayerResult` case through
//! `prepare_subtitle`, with a real `SubtitlesClient` against wiremock.

use std::path::Path;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use yts_player_lib::error::{AppError, AppResult};
use yts_player_lib::external_player::{
    player_args, prepare_subtitle, subtitle_request, PlayerKind, SubtitleRequest,
};
use yts_player_lib::subtitles::{Credentials, SubtitlesClient, SubtitlesConfig};
use yts_player_lib::types::{ExternalSubtitle, SubtitleOption};

const URL: &str = "http://127.0.0.1:4321/stream/abc/0";
const VTT: &str = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHola\n\n";

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

struct Harness {
    tmp: tempfile::TempDir,
    server: MockServer,
    client: SubtitlesClient,
}

async fn harness(key: bool) -> Harness {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let cfg = SubtitlesConfig {
        base_url: format!("{}/api/v1", server.uri()),
        ..SubtitlesConfig::new(tmp.path().join("subs"), "http://127.0.0.1:1".into())
    };
    let creds = Credentials {
        api_key: key.then(|| "k".into()),
        ..Default::default()
    };
    Harness {
        client: SubtitlesClient::new(cfg, creds).unwrap(),
        tmp,
        server,
    }
}

fn option(id: &str) -> SubtitleOption {
    SubtitleOption {
        id: id.into(),
        lang: "es".into(),
        label: Some("x".into()),
        downloads: 1,
        hearing_impaired: false,
        matches_release: true,
        ai_translated: false,
        page_url: None,
        cached: false,
    }
}

/// A search that must not run.
async fn no_search() -> AppResult<Vec<SubtitleOption>> {
    panic!("search must not run")
}

async fn api_calls(server: &MockServer) -> usize {
    server.received_requests().await.unwrap_or_default().len()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[tokio::test]
async fn off_nothing_unsupported_and_no_key_do_not_search() {
    let h = harness(true).await;
    let req = |off| subtitle_request(off, Some("9003".into()), None, true, true);

    // "Desactivados" in the player.
    let (file, outcome) =
        prepare_subtitle(&h.client, PlayerKind::Vlc, req(true), no_search(), 0).await;
    assert_eq!((file, outcome), (None, ExternalSubtitle::None));
    // Not requested and automatic subtitles off.
    let nothing = subtitle_request(false, None, None, false, true);
    let (file, outcome) =
        prepare_subtitle(&h.client, PlayerKind::Vlc, nothing, no_search(), 0).await;
    assert_eq!((file, outcome), (None, ExternalSubtitle::None));
    // Unknown player: no download, no search.
    let (file, outcome) =
        prepare_subtitle(&h.client, PlayerKind::Unknown, req(false), no_search(), 0).await;
    assert_eq!((file, outcome), (None, ExternalSubtitle::UnsupportedPlayer));
    // Automatic subtitles on but no key.
    let (file, outcome) = prepare_subtitle(
        &h.client,
        PlayerKind::Vlc,
        SubtitleRequest::NoKey,
        no_search(),
        0,
    )
    .await;
    assert_eq!((file, outcome), (None, ExternalSubtitle::NoKey));
    assert_eq!(api_calls(&h.server).await, 0);
}

#[tokio::test]
async fn chosen_subtitle_from_disk_cache_with_delay_even_without_key() {
    let h = harness(false).await;
    std::fs::write(h.tmp.path().join("subs/9003.vtt"), VTT).unwrap();
    let (file, outcome) = prepare_subtitle(
        &h.client,
        PlayerKind::Vlc,
        SubtitleRequest::Id("9003".into()),
        no_search(),
        2_500,
    )
    .await;
    assert_eq!(outcome, ExternalSubtitle::Loaded);
    let file = file.unwrap();
    assert_eq!(read(&file), "1\n00:00:03,500 --> 00:00:04,500\nHola\n\n");
    assert_eq!(
        player_args(PlayerKind::Vlc, URL, Some(&file)),
        [format!("--sub-file={}", file.display()), URL.to_owned()]
    );
    assert_eq!(api_calls(&h.server).await, 0);

    // Not cached and no key.
    let (file, outcome) = prepare_subtitle(
        &h.client,
        PlayerKind::Mpv,
        SubtitleRequest::Id("1234".into()),
        no_search(),
        0,
    )
    .await;
    assert_eq!((file, outcome), (None, ExternalSubtitle::NoKey));
}

#[tokio::test]
async fn own_file_loaded_or_not_found() {
    let h = harness(false).await;
    let srt = h.tmp.path().join("mine.srt");
    std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nMío\n").unwrap();
    let (file, outcome) = prepare_subtitle(
        &h.client,
        PlayerKind::Mpv,
        SubtitleRequest::File(srt),
        no_search(),
        -500,
    )
    .await;
    assert_eq!(outcome, ExternalSubtitle::Loaded);
    assert_eq!(
        read(&file.unwrap()),
        "1\n00:00:00,500 --> 00:00:01,500\nMío\n\n"
    );

    let (file, outcome) = prepare_subtitle(
        &h.client,
        PlayerKind::Mpv,
        SubtitleRequest::File(h.tmp.path().join("gone.srt")),
        no_search(),
        0,
    )
    .await;
    assert_eq!((file, outcome), (None, ExternalSubtitle::NotFound));

    let junk = h.tmp.path().join("junk.srt");
    std::fs::write(&junk, "nothing here").unwrap();
    let (file, outcome) = prepare_subtitle(
        &h.client,
        PlayerKind::Mpv,
        SubtitleRequest::File(junk),
        no_search(),
        0,
    )
    .await;
    assert_eq!((file, outcome), (None, ExternalSubtitle::Error));
}

#[tokio::test]
async fn automatic_search_uses_the_first_result() {
    let h = harness(true).await;
    let body = fixture("opensubtitles/download.json")
        .replace("LINK", &format!("{}/files/sub.srt", h.server.uri()));
    Mock::given(method("POST"))
        .and(path("/api/v1/download"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&h.server)
        .await;
    Mock::given(path("/files/sub.srt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("1\n00:00:01,000 --> 00:00:02,000\nAuto\n"),
        )
        .mount(&h.server)
        .await;

    let search = async { Ok(vec![option("9003"), option("9004")]) };
    let (file, outcome) =
        prepare_subtitle(&h.client, PlayerKind::Vlc, SubtitleRequest::Auto, search, 0).await;
    assert_eq!(outcome, ExternalSubtitle::Loaded);
    assert!(read(&file.unwrap()).contains("Auto"));
    assert!(h.tmp.path().join("subs/9003.vtt").exists());

    // Same again: the disk cache is used (no second download).
    let search = async { Ok(vec![option("9003")]) };
    let (_, outcome) =
        prepare_subtitle(&h.client, PlayerKind::Vlc, SubtitleRequest::Auto, search, 0).await;
    assert_eq!(outcome, ExternalSubtitle::Loaded);
    let downloads = h
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/api/v1/download")
        .count();
    assert_eq!(downloads, 1);

    // Nothing in that language.
    let (file, outcome) = prepare_subtitle(
        &h.client,
        PlayerKind::Vlc,
        SubtitleRequest::Auto,
        async { Ok(vec![]) },
        0,
    )
    .await;
    assert_eq!((file, outcome), (None, ExternalSubtitle::NotFound));

    // Search failures map to their outcome.
    for (err, expected) in [
        (
            AppError::SubtitlesQuota("q".into()),
            ExternalSubtitle::Quota,
        ),
        (AppError::SubtitlesAuth("a".into()), ExternalSubtitle::NoKey),
        (AppError::Network("n".into()), ExternalSubtitle::Error),
    ] {
        let (file, outcome) = prepare_subtitle(
            &h.client,
            PlayerKind::Vlc,
            SubtitleRequest::Auto,
            async { Err(err) },
            0,
        )
        .await;
        assert_eq!((file, outcome), (None, expected));
    }
}

#[tokio::test]
async fn download_quota_exhausted_is_quota() {
    let h = harness(true).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/download"))
        .respond_with(ResponseTemplate::new(406).set_body_raw(
            fixture("opensubtitles/download_quota.json"),
            "application/json",
        ))
        .mount(&h.server)
        .await;
    let (file, outcome) = prepare_subtitle(
        &h.client,
        PlayerKind::Vlc,
        SubtitleRequest::Id("9003".into()),
        no_search(),
        0,
    )
    .await;
    assert_eq!((file, outcome), (None, ExternalSubtitle::Quota));
}
