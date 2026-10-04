//! Recommendations for the Home page: the rotating banner (`get_featured`) and the
//! personal rows (`get_home_profile`). See `docs/IPC.md`, "Recomendaciones".
//!
//! Sources, most recent first: progress (in progress and finished), Mi lista and
//! downloads. From them: YTS `movie_suggestions` of the recent ones ("Porque viste X",
//! "Porque está en tu lista"), well rated movies of the most watched genres (random page)
//! and, to fill up, trending and recent movies (random page). Movies already watched, in
//! progress, in the list or downloaded are left out, as are repeats and movies without
//! seeds; at most [`MAX_PER_SOURCE`] come from the same source movie.
//!
//! Results are computed once per session (random seed per app start) and kept in memory.
//! Without network both are empty and nothing is cached, so they are computed again once
//! the network is back.

use std::collections::{HashMap, HashSet};

use futures_util::future::join_all;

use crate::error::{AppError, AppResult};
use crate::types::{
    BecauseWatchedRow, FeaturedItem, FeaturedReason, HomeProfile, ListMoviesParams, MovieDetail,
    MovieSummary, SortBy,
};
use crate::yts::YtsClient;

pub const FEATURED_COUNT: usize = 6;
pub const MAX_PER_SOURCE: usize = 2;
/// Recent history movies whose suggestions are asked for.
const SOURCES: usize = 4;
/// Most watched genres used for the "Para ti: <género>" picks.
const TOP_GENRES: usize = 2;
const GENRE_MIN_RATING: u8 = 7;
/// Movies in the "Porque viste X" row.
const ROW_LEN: usize = 20;

/// Small PRNG (xorshift64*): only for picking, no crypto.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    /// Seeded from the clock: different on every app start.
    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        Self::new(nanos ^ u64::from(std::process::id()).rotate_left(32))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// In `0..n` (`n > 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n.max(1)
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// Progress (watching or watched).
    Watched,
    /// Mi lista or a download.
    List,
}

#[derive(Debug, Clone)]
pub struct HistoryItem {
    pub movie: MovieSummary,
    pub kind: SourceKind,
}

/// What the user has done, most recent first, one item per movie.
#[derive(Debug, Clone, Default)]
pub struct History {
    pub items: Vec<HistoryItem>,
    genre_scores: HashMap<String, u32>,
    excluded: HashSet<u64>,
}

impl History {
    /// `progress` most recent first (in progress and finished); `favorites` and
    /// `downloads` most recent first.
    pub fn new(
        progress: Vec<MovieSummary>,
        favorites: Vec<MovieSummary>,
        downloads: Vec<MovieSummary>,
    ) -> Self {
        let mut h = Self::default();
        for (movies, kind, weight) in [
            (progress, SourceKind::Watched, 3),
            (favorites, SourceKind::List, 2),
            (downloads, SourceKind::List, 2),
        ] {
            for movie in movies {
                for genre in &movie.genres {
                    *h.genre_scores
                        .entry(genre.trim().to_ascii_lowercase())
                        .or_default() += weight;
                }
                if h.excluded.insert(movie.id) {
                    h.items.push(HistoryItem { movie, kind });
                }
            }
        }
        h.genre_scores.remove("");
        h
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Already watched, in progress, in the list or downloaded.
    pub fn excludes(&self, movie_id: u64) -> bool {
        self.excluded.contains(&movie_id)
    }

    /// Genres (lowercase) by affinity, highest first; ties alphabetically.
    pub fn genre_order(&self) -> Vec<String> {
        let mut genres: Vec<(&String, &u32)> = self.genre_scores.iter().collect();
        genres.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        genres.into_iter().map(|(g, _)| g.clone()).collect()
    }

    /// The most recently watched movie ("Porque viste X").
    pub fn last_watched(&self) -> Option<&MovieSummary> {
        self.items
            .iter()
            .find(|i| i.kind == SourceKind::Watched)
            .map(|i| &i.movie)
    }
}

/// A movie that could go in the banner, and why.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub movie: MovieSummary,
    pub reason: FeaturedReason,
}

fn source_of(reason: &FeaturedReason) -> Option<u64> {
    match reason {
        FeaturedReason::BecauseWatched {
            source_movie_id, ..
        }
        | FeaturedReason::BecauseList {
            source_movie_id, ..
        } => Some(*source_movie_id),
        _ => None,
    }
}

/// Personal candidates (in history order) alternating between sources, most recent source
/// first, shuffled within each source. Reversed: picks are popped from the end.
fn by_source(personal: Vec<Candidate>, rng: &mut Rng) -> Vec<Candidate> {
    let mut groups: Vec<(Option<u64>, Vec<Candidate>)> = Vec::new();
    for c in personal {
        let src = source_of(&c.reason);
        match groups.iter_mut().find(|(s, _)| *s == src) {
            Some((_, g)) => g.push(c),
            None => groups.push((src, vec![c])),
        }
    }
    for (_, g) in &mut groups {
        rng.shuffle(g);
        g.reverse();
    }
    let mut queue = Vec::new();
    while groups.iter().any(|(_, g)| !g.is_empty()) {
        for (_, g) in &mut groups {
            if let Some(c) = g.pop() {
                queue.push(c);
            }
        }
    }
    queue.reverse();
    queue
}

/// Up to `count` candidates mixing the reasons (personal, genre, personal, trending, …),
/// without excluded movies, repeats, movies without seeds, nor more than
/// [`MAX_PER_SOURCE`] from the same source. Pools are shuffled with `rng`.
pub fn pick_featured(
    history: &History,
    mut personal: Vec<Candidate>,
    mut genre: Vec<Candidate>,
    mut trending: Vec<Candidate>,
    count: usize,
    rng: &mut Rng,
) -> Vec<Candidate> {
    for pool in [&mut personal, &mut genre, &mut trending] {
        pool.retain(|c| !history.excludes(c.movie.id) && c.movie.max_seeds > 0);
    }
    for pool in [&mut genre, &mut trending] {
        rng.shuffle(pool);
    }
    personal = by_source(personal, rng);
    let mut pools = [personal, genre, trending];
    // Indexes into `pools`: personal twice as often as the others.
    const PATTERN: [usize; 4] = [0, 1, 0, 2];
    let mut picked: Vec<Candidate> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();
    let mut per_source: HashMap<u64, usize> = HashMap::new();
    let mut step = 0;
    while picked.len() < count && pools.iter().any(|p| !p.is_empty()) {
        let pool = &mut pools[PATTERN[step % PATTERN.len()]];
        step += 1;
        while let Some(c) = pool.pop() {
            if seen.contains(&c.movie.id) {
                continue;
            }
            if let Some(src) = source_of(&c.reason) {
                let n = per_source.entry(src).or_default();
                if *n >= MAX_PER_SOURCE {
                    continue;
                }
                *n += 1;
            }
            seen.insert(c.movie.id);
            picked.push(c);
            break;
        }
    }
    picked
}

fn is_offline(e: &AppError) -> bool {
    matches!(e, AppError::Network(_) | AppError::ApiUnavailable(_))
}

/// Session cache of the recommendations.
pub struct Recommender {
    seed: u64,
    featured: tokio::sync::Mutex<Option<Vec<FeaturedItem>>>,
    profile: tokio::sync::Mutex<Option<HomeProfile>>,
}

impl Default for Recommender {
    fn default() -> Self {
        Self::new(Rng::from_time().next_u64())
    }
}

impl Recommender {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            featured: tokio::sync::Mutex::new(None),
            profile: tokio::sync::Mutex::new(None),
        }
    }

    /// `get_featured`: computed once per session; `[]` without network.
    pub async fn featured(&self, yts: &YtsClient, history: &History) -> Vec<FeaturedItem> {
        let mut cache = self.featured.lock().await;
        if let Some(items) = cache.as_ref() {
            return items.clone();
        }
        match self.compute_featured(yts, history).await {
            Ok(items) => {
                tracing::info!(count = items.len(), "featured movies for this session");
                *cache = Some(items.clone());
                items
            }
            Err(e) => {
                tracing::info!(error = %e, "no featured movies (offline)");
                Vec::new()
            }
        }
    }

    async fn compute_featured(
        &self,
        yts: &YtsClient,
        history: &History,
    ) -> AppResult<Vec<FeaturedItem>> {
        let mut rng = Rng::new(self.seed);
        let mut reached = false;
        let mut last_error = None;
        let mut note = |r: &AppResult<Vec<MovieSummary>>| match r {
            Ok(_) => reached = true,
            Err(e) => {
                if is_offline(e) {
                    last_error = Some(AppError::Network(e.to_string()));
                } else {
                    tracing::warn!(error = %e, "recommendation source failed");
                }
            }
        };

        // Personal: suggestions of the most recent history movies.
        let sources: Vec<&HistoryItem> = history.items.iter().take(SOURCES).collect();
        let suggestions = join_all(sources.iter().map(|s| yts.suggestions(s.movie.id))).await;
        let mut personal = Vec::new();
        for (source, result) in sources.iter().zip(suggestions) {
            note(&result);
            let reason = match source.kind {
                SourceKind::Watched => FeaturedReason::BecauseWatched {
                    source_movie_id: source.movie.id,
                    source_title: source.movie.title.clone(),
                },
                SourceKind::List => FeaturedReason::BecauseList {
                    source_movie_id: source.movie.id,
                    source_title: source.movie.title.clone(),
                },
            };
            personal.extend(
                result
                    .unwrap_or_default()
                    .into_iter()
                    .map(|movie| Candidate {
                        movie,
                        reason: reason.clone(),
                    }),
            );
        }

        // Well rated movies of the most watched genres, random page.
        let genres: Vec<String> = history.genre_order().into_iter().take(TOP_GENRES).collect();
        let genre_params: Vec<ListMoviesParams> = genres
            .iter()
            .map(|g| ListMoviesParams {
                genre: Some(g.clone()),
                minimum_rating: Some(GENRE_MIN_RATING),
                sort_by: Some(SortBy::DownloadCount),
                page: Some(1 + rng.below(5) as u32),
                limit: Some(20),
                ..Default::default()
            })
            .collect();
        let genre_pages = join_all(genre_params.iter().map(|p| yts.list_movies(p))).await;
        let mut genre = Vec::new();
        for (g, result) in genres.iter().zip(genre_pages) {
            let result = result.map(|p| p.movies);
            note(&result);
            genre.extend(
                result
                    .unwrap_or_default()
                    .into_iter()
                    .map(|movie| Candidate {
                        movie,
                        reason: FeaturedReason::Genre { genre: g.clone() },
                    }),
            );
        }

        // Trending and recent, random pages: fills up (and is all there is without
        // history).
        let trending_params = [
            (SortBy::DownloadCount, 1 + rng.below(3) as u32),
            (SortBy::LikeCount, 1 + rng.below(3) as u32),
            (SortBy::DateAdded, 1),
        ];
        let trending_params: Vec<ListMoviesParams> = trending_params
            .iter()
            .map(|(sort, page)| ListMoviesParams {
                sort_by: Some(*sort),
                page: Some(*page),
                limit: Some(20),
                ..Default::default()
            })
            .collect();
        let trending_pages = join_all(trending_params.iter().map(|p| yts.list_movies(p))).await;
        let mut trending = Vec::new();
        for result in trending_pages {
            let result = result.map(|p| p.movies);
            note(&result);
            trending.extend(
                result
                    .unwrap_or_default()
                    .into_iter()
                    .map(|movie| Candidate {
                        movie,
                        reason: FeaturedReason::Trending,
                    }),
            );
        }
        if !reached {
            return Err(last_error.unwrap_or_else(|| AppError::Network("no sources".into())));
        }

        // Pick a few extra: a detail may fail.
        let picked = pick_featured(
            history,
            personal,
            genre,
            trending,
            FEATURED_COUNT + 2,
            &mut rng,
        );
        let details: Vec<AppResult<MovieDetail>> =
            join_all(picked.iter().map(|c| yts.get_movie(c.movie.id))).await;
        Ok(picked
            .into_iter()
            .zip(details)
            .filter_map(|(c, d)| match d {
                Ok(movie) => Some(FeaturedItem {
                    movie,
                    reason: c.reason,
                }),
                Err(e) => {
                    tracing::warn!(movie_id = c.movie.id, error = %e, "featured movie skipped");
                    None
                }
            })
            .take(FEATURED_COUNT)
            .collect())
    }

    /// `get_home_profile`: computed once per session; empty without network.
    pub async fn home_profile(&self, yts: &YtsClient, history: &History) -> HomeProfile {
        let mut cache = self.profile.lock().await;
        if let Some(p) = cache.as_ref() {
            return p.clone();
        }
        let because_watched = match history.last_watched() {
            None => None,
            Some(source) => match yts.suggestions(source.id).await {
                Ok(movies) => {
                    let movies: Vec<MovieSummary> = movies
                        .into_iter()
                        .filter(|m| !history.excludes(m.id))
                        .take(ROW_LEN)
                        .collect();
                    (!movies.is_empty()).then(|| BecauseWatchedRow {
                        source_movie_id: source.id,
                        source_title: source.title.clone(),
                        movies,
                    })
                }
                Err(e) if is_offline(&e) => {
                    tracing::info!(error = %e, "no home profile (offline)");
                    return HomeProfile::default();
                }
                Err(e) => {
                    tracing::warn!(error = %e, "\"because you watched\" row failed");
                    None
                }
            },
        };
        let profile = HomeProfile {
            because_watched,
            genre_order: history.genre_order(),
        };
        *cache = Some(profile.clone());
        profile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie(id: u64, genres: &[&str], seeds: u32) -> MovieSummary {
        MovieSummary {
            id,
            imdb_code: format!("tt{id}"),
            title: format!("Movie {id}"),
            year: 2020,
            rating: 7.0,
            runtime_min: 100,
            genres: genres.iter().map(|g| g.to_string()).collect(),
            cover_url: None,
            cover_large_url: None,
            background_url: None,
            qualities: vec![],
            has_x264: true,
            max_seeds: seeds,
        }
    }

    fn cand(id: u64, reason: FeaturedReason) -> Candidate {
        Candidate {
            movie: movie(id, &[], 10),
            reason,
        }
    }

    fn watched(src: u64) -> FeaturedReason {
        FeaturedReason::BecauseWatched {
            source_movie_id: src,
            source_title: format!("Movie {src}"),
        }
    }

    #[test]
    fn history_orders_dedupes_and_scores_genres() {
        let h = History::new(
            vec![movie(1, &["Sci-Fi", "Action"], 1), movie(2, &["Drama"], 1)],
            vec![movie(3, &["Sci-Fi"], 1), movie(1, &["Sci-Fi", "Action"], 1)],
            vec![movie(4, &["Film-Noir"], 1)],
        );
        assert_eq!(
            h.items.iter().map(|i| i.movie.id).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_eq!(h.items[2].kind, SourceKind::List);
        // sci-fi 3+2+2, action 3+2, drama 3, film-noir 2.
        assert_eq!(h.genre_order(), ["sci-fi", "action", "drama", "film-noir"]);
        assert!(h.excludes(4) && !h.excludes(5));
        assert_eq!(h.last_watched().map(|m| m.id), Some(1));
        assert!(History::default().genre_order().is_empty());
        assert!(History::new(vec![], vec![movie(9, &[], 1)], vec![])
            .last_watched()
            .is_none());
    }

    #[test]
    fn pick_mixes_reasons_and_applies_the_rules() {
        let h = History::new(vec![movie(1, &["Drama"], 1)], vec![], vec![]);
        let personal = vec![
            cand(10, watched(1)),
            cand(11, watched(1)),
            cand(12, watched(1)), // third from the same source: out
            cand(1, watched(1)),  // already watched: out
            Candidate {
                movie: movie(13, &[], 0), // no seeds: out
                reason: watched(1),
            },
        ];
        let genre = vec![
            cand(
                20,
                FeaturedReason::Genre {
                    genre: "drama".into(),
                },
            ),
            cand(
                10,
                FeaturedReason::Genre {
                    genre: "drama".into(),
                },
            ), // repeat
            cand(
                21,
                FeaturedReason::Genre {
                    genre: "drama".into(),
                },
            ),
        ];
        let trending = (30..40)
            .map(|id| cand(id, FeaturedReason::Trending))
            .collect();
        let picked = pick_featured(&h, personal, genre, trending, 6, &mut Rng::new(7));
        let ids: Vec<u64> = picked.iter().map(|c| c.movie.id).collect();
        assert_eq!(ids.len(), 6, "{ids:?}");
        let unique: HashSet<u64> = ids.iter().copied().collect();
        assert_eq!(unique.len(), 6);
        assert!(!ids.contains(&1) && !ids.contains(&13));
        let from_source_1 = picked
            .iter()
            .filter(|c| source_of(&c.reason) == Some(1))
            .count();
        assert_eq!(from_source_1, 2);
        let kinds: HashSet<&str> = picked
            .iter()
            .map(|c| match c.reason {
                FeaturedReason::BecauseWatched { .. } => "watched",
                FeaturedReason::Genre { .. } => "genre",
                FeaturedReason::Trending => "trending",
                FeaturedReason::BecauseList { .. } => "list",
            })
            .collect();
        assert_eq!(kinds, ["watched", "genre", "trending"].into());
    }

    #[test]
    fn without_history_it_is_all_trending_and_varies_with_the_seed() {
        let h = History::default();
        let trending: Vec<Candidate> = (1..=30)
            .map(|id| cand(id, FeaturedReason::Trending))
            .collect();
        let pick = |seed| {
            pick_featured(&h, vec![], vec![], trending.clone(), 6, &mut Rng::new(seed))
                .iter()
                .map(|c| c.movie.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(pick(1).len(), 6);
        assert_eq!(pick(1), pick(1));
        assert_ne!(pick(1), pick(2));
        // Fewer candidates than slots: all of them.
        assert_eq!(
            pick_featured(
                &h,
                vec![],
                vec![],
                trending[..3].to_vec(),
                6,
                &mut Rng::new(1)
            )
            .len(),
            3
        );
    }

    #[test]
    fn personal_picks_alternate_sources_most_recent_first() {
        let h = History::default();
        // Candidates arrive in history order: source 1 (most recent), then source 2.
        let personal: Vec<Candidate> = (10..15)
            .map(|id| cand(id, watched(1)))
            .chain((20..25).map(|id| cand(id, watched(2))))
            .collect();
        for seed in 1..20 {
            let picked =
                pick_featured(&h, personal.clone(), vec![], vec![], 4, &mut Rng::new(seed));
            let sources: Vec<Option<u64>> = picked.iter().map(|c| source_of(&c.reason)).collect();
            assert_eq!(sources, [Some(1), Some(2), Some(1), Some(2)], "seed {seed}");
        }
    }

    #[test]
    fn rng_is_spread() {
        let mut rng = Rng::new(42);
        let mut counts = [0; 5];
        for _ in 0..5000 {
            counts[rng.below(5) as usize] += 1;
        }
        assert!(counts.iter().all(|&c| c > 800), "{counts:?}");
    }
}
