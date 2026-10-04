// TanStack Query hooks over the typed IPC client. Components read remote data only through these.

import {
  useMutation,
  useQuery,
  useQueryClient,
  useInfiniteQuery,
  type QueryClient,
} from "@tanstack/react-query";
import { useEffect, useMemo } from "react";
import { DOWNLOAD_POLL_MS, downloadForMovie } from "../lib/downloads";
import { formatBytes } from "../lib/format";
import type { TorrentPrefs } from "../lib/versions";
import { showToast } from "../store/toast";
import { describeError } from "./errors";
import {
  addFavorite,
  clearCache,
  getApiStatus,
  getMovie,
  getSettings,
  getStorageUsage,
  getSuggestions,
  listContinueWatching,
  listDownloads,
  listFavorites,
  listMovies,
  onDownloadChanged,
  openDownloadFolder,
  pauseDownload,
  removeDownload,
  removeFavorite,
  removeProgress,
  resumeDownload,
  startDownload,
  updateSettings,
} from "./tauri";
import type {
  AppError,
  ContinueItem,
  Download,
  ListMoviesParams,
  MovieDetail,
  MovieSummary,
  Progress,
  Settings,
  SettingsPatch,
} from "./types";

export const queryKeys = {
  movies: (params: ListMoviesParams) => ["movies", params] as const,
  movie: (id: number) => ["movie", id] as const,
  suggestions: (id: number) => ["suggestions", id] as const,
  hero: ["hero"] as const,
  apiStatus: ["api-status"] as const,
  favorites: ["favorites"] as const,
  continueWatching: ["continue-watching"] as const,
  settings: ["settings"] as const,
  storage: ["storage"] as const,
  downloads: ["downloads"] as const,
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

export function useSuggestions(movieId: number, options: { enabled?: boolean } = {}) {
  return useQuery({
    queryKey: queryKeys.suggestions(movieId),
    queryFn: () => getSuggestions(movieId),
    retry,
    enabled: options.enabled ?? true,
  });
}

/** Base URLs with their latency; `live` re-measures every 15 s (Ajustes › Catálogo). */
export function useApiStatus(options: { live?: boolean } = {}) {
  return useQuery({
    queryKey: queryKeys.apiStatus,
    queryFn: getApiStatus,
    retry,
    staleTime: 0,
    refetchInterval: options.live ? 15_000 : false,
  });
}

// ───────── Mi lista ─────────

export function useFavorites() {
  return useQuery({ queryKey: queryKeys.favorites, queryFn: listFavorites, retry, staleTime: 0 });
}

type FavoriteVars = { movie: MovieSummary; on: boolean };
type FavoriteSnapshot = { detail?: MovieDetail; list?: MovieSummary[] };

/**
 * ♥ toggle: flips the movie page and Mi lista caches at once; if the backend fails, both go back to
 * what they were and a toast says so.
 */
export function useToggleFavorite() {
  const qc = useQueryClient();
  return useMutation<void, AppError, FavoriteVars, FavoriteSnapshot>({
    mutationFn: ({ movie, on }) => (on ? addFavorite(movie) : removeFavorite(movie.id)),
    onMutate: async ({ movie, on }) => {
      await Promise.all([
        qc.cancelQueries({ queryKey: queryKeys.movie(movie.id) }),
        qc.cancelQueries({ queryKey: queryKeys.favorites }),
      ]);
      const detail = qc.getQueryData<MovieDetail>(queryKeys.movie(movie.id));
      const list = qc.getQueryData<MovieSummary[]>(queryKeys.favorites);
      if (detail) qc.setQueryData<MovieDetail>(queryKeys.movie(movie.id), { ...detail, isFavorite: on });
      if (list) {
        const rest = list.filter((m) => m.id !== movie.id);
        qc.setQueryData<MovieSummary[]>(queryKeys.favorites, on ? [movie, ...rest] : rest);
      }
      return { detail, list };
    },
    onError: (_err, { movie, on }, snap) => {
      if (snap?.detail) qc.setQueryData(queryKeys.movie(movie.id), snap.detail);
      if (snap?.list) qc.setQueryData(queryKeys.favorites, snap.list);
      showToast(on ? "No se pudo añadir a Mi lista" : "No se pudo quitar de Mi lista", "error");
    },
    onSuccess: (_data, { on }) => showToast(on ? "Añadida a Mi lista" : "Quitada de Mi lista"),
    onSettled: () => qc.invalidateQueries({ queryKey: queryKeys.favorites }),
  });
}

// ───────── Continuar viendo ─────────

export function useContinueWatching() {
  return useQuery({
    queryKey: queryKeys.continueWatching,
    queryFn: listContinueWatching,
    retry,
    staleTime: 0,
  });
}

/** After save_progress: the movie page shows the new position without refetching get_movie. */
export function applySavedProgress(qc: QueryClient, progress: Progress) {
  const detail = qc.getQueryData<MovieDetail>(queryKeys.movie(progress.movieId));
  if (detail) qc.setQueryData<MovieDetail>(queryKeys.movie(progress.movieId), { ...detail, progress });
  void qc.invalidateQueries({ queryKey: queryKeys.continueWatching, refetchType: "active" });
}

export function useRemoveProgress() {
  const qc = useQueryClient();
  return useMutation<void, AppError, number, { list?: ContinueItem[]; detail?: MovieDetail }>({
    mutationFn: removeProgress,
    onMutate: async (movieId) => {
      await qc.cancelQueries({ queryKey: queryKeys.continueWatching });
      const list = qc.getQueryData<ContinueItem[]>(queryKeys.continueWatching);
      const detail = qc.getQueryData<MovieDetail>(queryKeys.movie(movieId));
      if (list)
        qc.setQueryData<ContinueItem[]>(
          queryKeys.continueWatching,
          list.filter((i) => i.movie.id !== movieId),
        );
      if (detail) qc.setQueryData<MovieDetail>(queryKeys.movie(movieId), { ...detail, progress: null });
      return { list, detail };
    },
    onError: (_err, movieId, snap) => {
      if (snap?.list) qc.setQueryData(queryKeys.continueWatching, snap.list);
      if (snap?.detail) qc.setQueryData(queryKeys.movie(movieId), snap.detail);
      showToast("No se pudo quitar de Continuar viendo", "error");
    },
    onSettled: () => qc.invalidateQueries({ queryKey: queryKeys.continueWatching }),
  });
}

// ───────── Ajustes ─────────

export function useSettings() {
  // Only this client changes them (update_settings returns the applied values).
  return useQuery({ queryKey: queryKeys.settings, queryFn: getSettings, retry, staleTime: Infinity });
}

/** preferredQuality + preferX264 for pickDefaultTorrent; null until the settings load. */
export function useTorrentPrefs(): TorrentPrefs | null {
  const settings = useSettings().data;
  const preferredQuality = settings?.preferredQuality;
  const preferX264 = settings?.preferX264;
  return useMemo(
    () =>
      preferredQuality !== undefined && preferX264 !== undefined ? { preferredQuality, preferX264 } : null,
    [preferredQuality, preferX264],
  );
}

/** Catalog data that depends on which YTS mirror answers. */
const CATALOG_KEYS = [["movies"], queryKeys.hero, ["movie"], ["suggestions"], queryKeys.apiStatus];

/**
 * update_settings with the patch shown right away; on failure the previous values come back and a
 * toast explains why. Changing the base URLs refetches the catalog without restarting.
 */
export function useUpdateSettings() {
  const qc = useQueryClient();
  return useMutation<Settings, AppError, SettingsPatch, { prev?: Settings }>({
    mutationFn: updateSettings,
    onMutate: async (patch) => {
      await qc.cancelQueries({ queryKey: queryKeys.settings });
      const prev = qc.getQueryData<Settings>(queryKeys.settings);
      if (prev) qc.setQueryData<Settings>(queryKeys.settings, { ...prev, ...patch });
      return { prev };
    },
    onError: (err, _patch, ctx) => {
      if (ctx?.prev) qc.setQueryData(queryKeys.settings, ctx.prev);
      const copy = describeError(err);
      showToast(
        err.code === "invalid_input" ? `No se guardó: ${copy.title.toLowerCase()}` : copy.title,
        "error",
      );
    },
    onSuccess: (settings, patch) => {
      qc.setQueryData(queryKeys.settings, settings);
      if (patch.seedAfterDownload !== undefined) void qc.invalidateQueries({ queryKey: queryKeys.downloads });
      if (patch.apiBaseUrls) for (const key of CATALOG_KEYS) void qc.invalidateQueries({ queryKey: key });
      if (patch.cacheLimitBytes !== undefined) void qc.invalidateQueries({ queryKey: queryKeys.storage });
    },
  });
}

// ───────── Almacenamiento ─────────

export function useStorageUsage() {
  return useQuery({
    queryKey: queryKeys.storage,
    queryFn: getStorageUsage,
    retry,
    staleTime: 0,
    refetchInterval: 10_000,
  });
}

export function useClearCache() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: clearCache,
    onSuccess: ({ freedBytes }) =>
      showToast(
        freedBytes > 0 ? `Caché vaciada: ${formatBytes(freedBytes)} liberados` : "La caché ya estaba vacía",
      ),
    onError: (err: AppError) => showToast(describeError(err).title, "error"),
    onSettled: () => qc.invalidateQueries({ queryKey: queryKeys.storage }),
  });
}

// ───────── Descargas ─────────

/** list_downloads; `poll` refetches every second (only while the window is visible). */
export function useDownloads(options: { poll?: boolean } = {}) {
  return useQuery({
    queryKey: queryKeys.downloads,
    queryFn: listDownloads,
    retry,
    staleTime: 0,
    refetchInterval: options.poll ? DOWNLOAD_POLL_MS : false,
  });
}

/**
 * A download changed (command answer or `download://changed`; `null` = removed): patch the list and the
 * movie page right away, then let the backend confirm.
 */
export function applyDownload(qc: QueryClient, infohash: string, download: Download | null) {
  const list = qc.getQueryData<Download[]>(queryKeys.downloads);
  const previous = list?.find((d) => d.infohash === infohash) ?? null;
  if (list) {
    const rest = list.filter((d) => d.infohash !== infohash);
    qc.setQueryData<Download[]>(queryKeys.downloads, download ? [download, ...rest] : rest);
  }
  const movieId = download?.movie.id ?? previous?.movie.id;
  const updated = qc.getQueryData<Download[]>(queryKeys.downloads);
  for (const [key, detail] of qc.getQueriesData<MovieDetail>({ queryKey: ["movie"] })) {
    if (!detail) continue;
    const mine = detail.download?.infohash === infohash;
    if (detail.id !== movieId && !mine) continue;
    let next = detail.download;
    if (updated) next = downloadForMovie(updated, detail.id);
    else if (download) next = !next || mine || download.state === "done" ? download : next;
    else if (mine) next = null;
    qc.setQueryData<MovieDetail>(key, { ...detail, download: next });
  }
  if (!download || download.state === "done") void qc.invalidateQueries({ queryKey: queryKeys.storage });
}

/** Keeps the downloads cache in step with `download://changed`. Mounted once, in the app shell. */
export function useDownloadEvents() {
  const qc = useQueryClient();
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let gone = false;
    onDownloadChanged(({ infohash, download }) => applyDownload(qc, infohash, download))
      .then((fn) => (gone ? fn() : (unlisten = fn)))
      .catch(() => undefined);
    return () => {
      gone = true;
      unlisten?.();
    };
  }, [qc]);
}

const failToast = (what: string) => (err: AppError) =>
  showToast(`${what}. ${describeError(err).action}`, "error");

export function useStartDownload() {
  const qc = useQueryClient();
  return useMutation<Download, AppError, { movie: MovieSummary; infohash: string }>({
    mutationFn: ({ movie, infohash }) => startDownload(movie, infohash),
    onSuccess: (download) => {
      applyDownload(qc, download.infohash, download);
      showToast(`Descargando ${download.movie.title} · ${download.quality}`);
    },
    onError: failToast("No se pudo descargar"),
    onSettled: () => qc.invalidateQueries({ queryKey: queryKeys.downloads }),
  });
}

/** Pausar / Reanudar. */
export function useToggleDownload() {
  const qc = useQueryClient();
  return useMutation<Download, AppError, { infohash: string; pause: boolean }>({
    mutationFn: ({ infohash, pause }) => (pause ? pauseDownload(infohash) : resumeDownload(infohash)),
    onSuccess: (download) => applyDownload(qc, download.infohash, download),
    onError: (err, { pause }) => failToast(pause ? "No se pudo pausar" : "No se pudo reanudar")(err),
  });
}

export function useRemoveDownload() {
  const qc = useQueryClient();
  return useMutation<void, AppError, { download: Download; deleteFiles: boolean }>({
    mutationFn: ({ download, deleteFiles }) => removeDownload(download.infohash, deleteFiles),
    onSuccess: (_void, { download, deleteFiles }) => {
      applyDownload(qc, download.infohash, null);
      showToast(
        deleteFiles
          ? `${download.movie.title} borrada del disco`
          : `${download.movie.title} quitada; los archivos siguen en su carpeta`,
      );
    },
    onError: failToast("No se pudo quitar la descarga"),
    onSettled: () => qc.invalidateQueries({ queryKey: queryKeys.downloads }),
  });
}

export function useOpenDownloadFolder() {
  return useMutation<void, AppError, string>({
    mutationFn: openDownloadFolder,
    onError: failToast("No se pudo abrir la carpeta"),
  });
}
