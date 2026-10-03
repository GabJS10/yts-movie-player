// Typed wrappers over Tauri IPC. The only module in the frontend that talks to Rust.
// Contract: docs/IPC.md. Types: ./types.ts.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppError,
  CommandArgs,
  CommandName,
  CommandResult,
  EventMap,
  EventName,
  ErrorCode,
  ListMoviesParams,
  MovieSummary,
  Settings,
} from "./types";

const ERROR_CODES: readonly ErrorCode[] = [
  "network",
  "api_unavailable",
  "not_found",
  "invalid_input",
  "torrent",
  "no_peers",
  "subtitles_auth",
  "subtitles_quota",
  "external_player_missing",
  "io",
  "db",
  "internal",
];

export function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.code === "string" &&
    (ERROR_CODES as readonly string[]).includes(v.code) &&
    typeof v.message === "string"
  );
}

/** Any rejection that is not a contract `AppError` (e.g. a Tauri-level failure) becomes `internal`. */
export function toAppError(value: unknown): AppError {
  if (isAppError(value)) return value;
  const message =
    value instanceof Error ? value.message : typeof value === "string" ? value : JSON.stringify(value);
  return { code: "internal", message };
}

export async function call<C extends CommandName>(
  cmd: C,
  ...args: CommandArgs<C> extends undefined ? [] : [CommandArgs<C>]
): Promise<CommandResult<C>> {
  try {
    return await invoke<CommandResult<C>>(cmd, args[0] as Record<string, unknown> | undefined);
  } catch (err) {
    throw toAppError(err);
  }
}

export function on<E extends EventName>(
  event: E,
  handler: (payload: EventMap[E]) => void,
): Promise<UnlistenFn> {
  return listen<EventMap[E]>(event, (e) => handler(e.payload));
}

// ───────── Catalog ─────────
export const listMovies = (params: ListMoviesParams) => call("list_movies", { params });
export const getMovie = (movieId: number) => call("get_movie", { movieId });
export const getSuggestions = (movieId: number) => call("get_suggestions", { movieId });
export const getApiStatus = () => call("get_api_status");

// ───────── Streaming ─────────
export const startStream = (movieId: number, infohash: string) => call("start_stream", { movieId, infohash });
export const stopStream = (infohash: string) => call("stop_stream", { infohash });
export const openExternalPlayer = (infohash: string) => call("open_external_player", { infohash });

// ───────── Subtitles ─────────
export const searchSubtitles = (movieId: number, lang: string, infohash?: string) =>
  call("search_subtitles", infohash === undefined ? { movieId, lang } : { movieId, lang, infohash });
export const loadSubtitle = (subtitleId: string) => call("load_subtitle", { subtitleId });
export const loadSubtitleFile = (path: string) => call("load_subtitle_file", { path });

// ───────── My list ─────────
export const listFavorites = () => call("list_favorites");
export const addFavorite = (movie: MovieSummary) => call("add_favorite", { movie });
export const removeFavorite = (movieId: number) => call("remove_favorite", { movieId });

// ───────── Continue watching ─────────
export const saveProgress = (movie: MovieSummary, positionS: number, durationS: number) =>
  call("save_progress", { movie, positionS, durationS });
export const getProgress = (movieId: number) => call("get_progress", { movieId });
export const listContinueWatching = () => call("list_continue_watching");
export const removeProgress = (movieId: number) => call("remove_progress", { movieId });

// ───────── Downloads ─────────
export const startDownload = (movie: MovieSummary, infohash: string) =>
  call("start_download", { movie, infohash });
export const listDownloads = () => call("list_downloads");
export const pauseDownload = (infohash: string) => call("pause_download", { infohash });
export const resumeDownload = (infohash: string) => call("resume_download", { infohash });
export const removeDownload = (infohash: string, deleteFiles: boolean) =>
  call("remove_download", { infohash, deleteFiles });
export const openDownloadFolder = (infohash: string) => call("open_download_folder", { infohash });

// ───────── Settings & storage ─────────
export const getSettings = () => call("get_settings");
export const updateSettings = (patch: Partial<Settings>) => call("update_settings", { patch });
export const getStorageUsage = () => call("get_storage_usage");
export const clearCache = () => call("clear_cache");

// ───────── Trailer ─────────
export const openTrailerWindow = (ytTrailerCode: string, title: string) =>
  call("open_trailer_window", { ytTrailerCode, title });

// ───────── Events ─────────
export const onTorrentStats = (handler: (p: EventMap["torrent://stats"]) => void) =>
  on("torrent://stats", handler);
export const onDownloadChanged = (handler: (p: EventMap["download://changed"]) => void) =>
  on("download://changed", handler);
export const onBackgroundError = (handler: (p: EventMap["app://error"]) => void) =>
  on("app://error", handler);
