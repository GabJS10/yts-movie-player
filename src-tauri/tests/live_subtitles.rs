//! Manual check against the real OpenSubtitles API (spends 1 download of quota).
//! The key is read from the environment, never from the repo:
//!
//! ```sh
//! OPENSUBTITLES_API_KEY=… [OPENSUBTITLES_USERNAME=… OPENSUBTITLES_PASSWORD=…] \
//!   cargo test --test live_subtitles -- --ignored --nocapture
//! ```
//!
//! Prints the real quota numbers (remaining / reset) and checks that an invalid key is
//! reported as `subtitles_auth` by the key check used for "Probar".

use yts_player_lib::error::AppError;
use yts_player_lib::subtitles::{Credentials, Release, SubtitlesClient, SubtitlesConfig};
use yts_player_lib::types::{Quality, TorrentSource};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn client(dir: &std::path::Path, creds: Credentials) -> SubtitlesClient {
    SubtitlesClient::new(
        SubtitlesConfig::new(dir.join("subs"), "http://127.0.0.1:1".into()),
        creds,
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "needs OPENSUBTITLES_API_KEY and spends one download"]
async fn live_opensubtitles() {
    let Some(api_key) = env("OPENSUBTITLES_API_KEY") else {
        panic!("set OPENSUBTITLES_API_KEY");
    };
    let tmp = tempfile::tempdir().unwrap();

    let bad = client(
        tmp.path(),
        Credentials {
            api_key: Some("definitely-not-a-valid-key".into()),
            ..Default::default()
        },
    );
    match bad.status().await {
        Err(AppError::SubtitlesAuth(_)) => println!("invalid key → subtitles_auth ✔"),
        other => panic!("invalid key gave {other:?}"),
    }

    let os = client(
        tmp.path(),
        Credentials {
            api_key: Some(api_key),
            username: env("OPENSUBTITLES_USERNAME"),
            password: env("OPENSUBTITLES_PASSWORD"),
        },
    );
    let status = os.status().await.unwrap();
    println!("status before: {status:?}");

    // The Matrix (1999), YTS 1080p BluRay.
    let release = Release {
        quality: Quality::P1080,
        source: TorrentSource::Bluray,
    };
    let results = os.search("tt0133093", "es", Some(release)).await.unwrap();
    println!("{} results in es; top 5:", results.len());
    for o in results.iter().take(5) {
        println!(
            "  {} match={} hi={} ai={} dl={} {}",
            o.id, o.matches_release, o.hearing_impaired, o.ai_translated, o.downloads, o.label
        );
    }
    let first = results.first().expect("some Spanish subtitles");
    let track = os.load(&first.id).await.unwrap();
    println!("loaded {:?}", track);
    let vtt = std::fs::read_to_string(tmp.path().join(format!("subs/{}.vtt", first.id))).unwrap();
    assert!(vtt.starts_with("WEBVTT\n\n"));
    println!("{} cues", vtt.matches(" --> ").count());

    let status = os.status().await.unwrap();
    println!("status after: {status:?}");
}
