//! Recommendations (`get_featured`, `get_home_profile`) against a fake YTS API built from
//! the real fixtures: every `movie_details` answer carries the requested id.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use yts_player_lib::images::ImageStore;
use yts_player_lib::recommend::{History, Recommender};
use yts_player_lib::types::{FeaturedReason, MovieSummary, Quality};
use yts_player_lib::yts::{YtsClient, YtsConfig};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

/// `movie_details.json` with the id (and title) of the request.
fn details_for(req: &Request) -> ResponseTemplate {
    let id: u64 = req
        .url
        .query_pairs()
        .find(|(k, _)| k == "movie_id")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body: serde_json::Value = serde_json::from_str(&fixture("movie_details.json")).unwrap();
    body["data"]["movie"]["id"] = id.into();
    body["data"]["movie"]["title"] = format!("Movie {id}").into();
    ResponseTemplate::new(200).set_body_json(body)
}

async fn fake_yts() -> MockServer {
    let server = MockServer::start().await;
    for (endpoint, name) in [
        ("list_movies.json", "list_movies.json"),
        ("movie_suggestions.json", "movie_suggestions.json"),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v2/{endpoint}")))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(fixture(name), "application/json"),
            )
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v2/movie_details.json"))
        .respond_with(details_for)
        .mount(&server)
        .await;
    server
}

fn client(tmp: &tempfile::TempDir, base: String) -> YtsClient {
    let images = Arc::new(
        ImageStore::new(tmp.path().join("img"), Vec::new(), "http://127.0.0.1:1").unwrap(),
    );
    YtsClient::new(
        YtsConfig {
            base_urls: vec![base],
            request_timeout: Duration::from_secs(2),
            ..YtsConfig::default()
        },
        images,
    )
    .unwrap()
}

fn movie(id: u64, title: &str, genres: &[&str]) -> MovieSummary {
    MovieSummary {
        id,
        imdb_code: format!("tt{id}"),
        title: title.into(),
        year: 2021,
        rating: 7.0,
        runtime_min: 120,
        genres: genres.iter().map(|g| g.to_string()).collect(),
        cover_url: None,
        cover_large_url: None,
        background_url: None,
        qualities: vec![Quality::P1080],
        has_x264: true,
        max_seeds: 10,
    }
}

/// Suggestions fixture ids (all with seeds) and list fixture ids without seeds.
const SUGGESTED: [u64; 4] = [38746, 38982, 39166, 40617];
const NO_SEEDS: [u64; 2] = [78890, 78887];

#[tokio::test]
async fn featured_mixes_reasons_skips_watched_and_is_stable_in_the_session() {
    let server = fake_yts().await;
    let tmp = tempfile::tempdir().unwrap();
    let yts = client(&tmp, format!("{}/api/v2/", server.uri()));
    // Watched one of the suggested movies; another one is in the list.
    let history = History::new(
        vec![
            movie(1000, "The Source", &["Action", "Sci-Fi"]),
            movie(SUGGESTED[0], "Seen", &["Action"]),
        ],
        vec![movie(SUGGESTED[1], "Listed", &["Sci-Fi"])],
        vec![],
    );
    let rec = Recommender::new(12345);
    let items = rec.featured(&yts, &history).await;
    assert!(!items.is_empty() && items.len() <= 6, "{}", items.len());

    let ids: Vec<u64> = items.iter().map(|i| i.movie.summary_fields.id).collect();
    assert_eq!(
        ids.iter().collect::<HashSet<_>>().len(),
        ids.len(),
        "{ids:?}"
    );
    for excluded in [1000, SUGGESTED[0], SUGGESTED[1]].iter().chain(&NO_SEEDS) {
        assert!(!ids.contains(excluded), "{excluded} in {ids:?}");
    }
    // Full movie pages, not summaries.
    assert!(items.iter().all(|i| !i.movie.summary.is_empty()));
    assert!(items
        .iter()
        .all(|i| i.movie.summary_fields.title.starts_with("Movie ")));
    // Personal picks name their source, at most 2 per source.
    let mut per_source = std::collections::HashMap::new();
    for i in &items {
        if let FeaturedReason::BecauseWatched {
            source_movie_id, ..
        }
        | FeaturedReason::BecauseList {
            source_movie_id, ..
        } = &i.reason
        {
            *per_source.entry(*source_movie_id).or_insert(0) += 1;
        }
    }
    assert!(per_source.values().all(|&n| n <= 2), "{per_source:?}");
    assert!(
        items
            .iter()
            .any(|i| matches!(&i.reason, FeaturedReason::BecauseWatched { source_movie_id: 1000, source_title } if source_title == "The Source")),
        "{:?}",
        items.iter().map(|i| &i.reason).collect::<Vec<_>>()
    );
    assert!(items
        .iter()
        .any(|i| matches!(&i.reason, FeaturedReason::Genre { genre } if genre == "action" || genre == "sci-fi")));

    // Same session: same answer, without asking YTS again.
    let requests = server.received_requests().await.unwrap().len();
    assert_eq!(rec.featured(&yts, &history).await, items);
    assert_eq!(server.received_requests().await.unwrap().len(), requests);
}

#[tokio::test]
async fn without_history_featured_is_trending() {
    let server = fake_yts().await;
    let tmp = tempfile::tempdir().unwrap();
    let yts = client(&tmp, format!("{}/api/v2/", server.uri()));
    let items = Recommender::new(7)
        .featured(&yts, &History::default())
        .await;
    assert!(!items.is_empty());
    assert!(items.iter().all(|i| i.reason == FeaturedReason::Trending));
    let ids: Vec<u64> = items.iter().map(|i| i.movie.summary_fields.id).collect();
    assert!(NO_SEEDS.iter().all(|id| !ids.contains(id)), "{ids:?}");
}

#[tokio::test]
async fn home_profile_has_the_because_watched_row_and_genre_order() {
    let server = fake_yts().await;
    let tmp = tempfile::tempdir().unwrap();
    let yts = client(&tmp, format!("{}/api/v2/", server.uri()));
    let history = History::new(
        vec![
            movie(1000, "The Source", &["Drama"]),
            movie(SUGGESTED[2], "Seen", &["Drama", "Sci-Fi"]),
        ],
        vec![movie(5, "Listed", &["Sci-Fi"])],
        vec![],
    );
    let rec = Recommender::new(1);
    let profile = rec.home_profile(&yts, &history).await;
    let row = profile.because_watched.expect("row");
    assert_eq!(
        (row.source_movie_id, row.source_title.as_str()),
        (1000, "The Source")
    );
    let ids: Vec<u64> = row.movies.iter().map(|m| m.id).collect();
    assert_eq!(ids, [SUGGESTED[0], SUGGESTED[1], SUGGESTED[3]]);
    // drama 3+3, sci-fi 3+2.
    assert_eq!(profile.genre_order, ["drama", "sci-fi"]);

    // No history: no row, default order, no network needed.
    let empty = Recommender::new(1)
        .home_profile(&yts, &History::default())
        .await;
    assert_eq!(empty.because_watched, None);
    assert!(empty.genre_order.is_empty());
}

#[tokio::test]
async fn offline_both_are_empty_without_failing_and_not_cached() {
    let tmp = tempfile::tempdir().unwrap();
    // Nothing listens on port 9.
    let yts = client(&tmp, "http://127.0.0.1:9/api/v2/".into());
    let history = History::new(vec![movie(1000, "The Source", &["Drama"])], vec![], vec![]);
    let rec = Recommender::new(1);
    assert!(rec.featured(&yts, &history).await.is_empty());
    let profile = rec.home_profile(&yts, &history).await;
    assert_eq!(profile.because_watched, None);
    assert!(profile.genre_order.is_empty());

    // Network back (another client, same recommender): computed now.
    let server = fake_yts().await;
    let online = client(&tmp, format!("{}/api/v2/", server.uri()));
    assert!(!rec.featured(&online, &history).await.is_empty());
    assert!(rec
        .home_profile(&online, &history)
        .await
        .because_watched
        .is_some());
}
