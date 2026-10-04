import type { MovieDetail, MovieSummary } from "../api/types";

/**
 * The MovieSummary part of a MovieDetail: what add_favorite and save_progress store so Mi lista and
 * Continuar viendo render offline. URLs go as received; the backend strips the local origin itself.
 */
export function toSummary(m: MovieSummary | MovieDetail): MovieSummary {
  return {
    id: m.id,
    imdbCode: m.imdbCode,
    title: m.title,
    year: m.year,
    rating: m.rating,
    runtimeMin: m.runtimeMin,
    genres: m.genres,
    coverUrl: m.coverUrl,
    coverLargeUrl: m.coverLargeUrl,
    backgroundUrl: m.backgroundUrl,
    qualities: m.qualities,
    hasX264: m.hasX264,
    maxSeeds: m.maxSeeds,
  };
}

export type FavoriteSort = "added" | "rating" | "year" | "title";

/** list_favorites already comes most recent first ("added"); the other orders are client-side. */
export function sortFavorites(movies: readonly MovieSummary[], sort: FavoriteSort): MovieSummary[] {
  const list = [...movies];
  if (sort === "rating") list.sort((a, b) => b.rating - a.rating);
  else if (sort === "year") list.sort((a, b) => b.year - a.year);
  else if (sort === "title") list.sort((a, b) => a.title.localeCompare(b.title, "es"));
  return list;
}
