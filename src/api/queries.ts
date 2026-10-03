// TanStack Query hooks over the typed IPC client. Components read remote data only through these.

import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { getApiStatus, getMovie, getSuggestions, listMovies } from "./tauri";
import type { AppError, ListMoviesParams, MovieSummary } from "./types";

export const queryKeys = {
  movies: (params: ListMoviesParams) => ["movies", params] as const,
  movie: (id: number) => ["movie", id] as const,
  suggestions: (id: number) => ["suggestions", id] as const,
  hero: ["hero"] as const,
  apiStatus: ["api-status"] as const,
};

/** Errors that a retry cannot fix. */
const FINAL: ReadonlySet<AppError["code"]> = new Set(["not_found", "invalid_input"]);
const retry = (count: number, error: AppError) => !FINAL.has(error.code) && count < 1;

export const PAGE_SIZE = 20;

/** Flattened, de-duplicated pages plus the API total (movie_count). */
export type MovieList = { movies: MovieSummary[]; total: number };

/** Paginated catalog listing; `fetchNextPage` loads the next API page. */
export function useInfiniteMovies(
  params: Omit<ListMoviesParams, "page">,
  options: { enabled?: boolean } = {},
) {
  const base = { limit: PAGE_SIZE, ...params };
  return useInfiniteQuery<
    Awaited<ReturnType<typeof listMovies>>,
    AppError,
    MovieList,
    ReturnType<typeof queryKeys.movies>,
    number
  >({
    queryKey: queryKeys.movies(base),
    queryFn: ({ pageParam }) => listMovies({ ...base, page: pageParam }),
    initialPageParam: 1,
    getNextPageParam: (last) => (last.hasMore ? last.page + 1 : undefined),
    // Pages can overlap when the catalog shifts between requests; keep the first occurrence.
    select: (data) => {
      const seen = new Set<number>();
      const movies = data.pages.flatMap((p) => p.movies).filter((m) => !seen.has(m.id) && !!seen.add(m.id));
      return { movies, total: data.pages[0]?.total ?? 0 };
    },
    retry,
    enabled: options.enabled ?? true,
  });
}

export const HERO_MIN_RATING = 6.5;

/** Home banner: the most downloaded movie that has a background image and rating ≥ 6.5. */
export function useHeroMovie() {
  return useQuery<MovieSummary | null, AppError>({
    queryKey: queryKeys.hero,
    queryFn: async () => {
      const page = await listMovies({ sortBy: "download_count", limit: 20 });
      return page.movies.find((m) => m.backgroundUrl && m.rating >= HERO_MIN_RATING) ?? null;
    },
    retry,
  });
}

export function useMovie(movieId: number, options: { enabled?: boolean } = {}) {
  return useQuery({
    queryKey: queryKeys.movie(movieId),
    queryFn: () => getMovie(movieId),
    retry,
    enabled: options.enabled ?? true,
  });
}

export function useSuggestions(movieId: number) {
  return useQuery({
    queryKey: queryKeys.suggestions(movieId),
    queryFn: () => getSuggestions(movieId),
    retry,
  });
}

export function useApiStatus() {
  return useQuery({ queryKey: queryKeys.apiStatus, queryFn: getApiStatus, retry });
}
