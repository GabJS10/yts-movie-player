// Typed wrappers over Tauri IPC. The only module in the frontend that talks to Rust.
// Contract: docs/IPC.md. Types: ./types.ts.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isTauri } from "../lib/runtime";
import type {
  AppError,
  CommandArgs,
  CommandName,
  CommandResult,
  EventMap,
  EventName,
  ErrorCode,
  ExternalSubtitleArgs,
  ListMoviesParams,
  MovieSummary,
  SettingsPatch,
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
export const getFeatured = () => call("get_featured");
export const getHomeProfile = () => call("get_home_profile");

// ───────── Streaming ─────────
export const startStream = (movieId: number, infohash: string) => call("start_stream", { movieId, infohash });
export const stopStream = (infohash: string) => call("stop_stream", { infohash });
/** Opens VLC (Ajustes › Reproducción) with the active subtitle, if any. Absent keys are not sent. */
export const openExternalPlayer = (infohash: string, subtitles: ExternalSubtitleArgs = {}) =>
  call("open_external_player", {
    infohash,
    ...(subtitles.subtitleId !== undefined ? { subtitleId: subtitles.subtitleId } : {}),
    ...(subtitles.subtitlePath !== undefined ? { subtitlePath: subtitles.subtitlePath } : {}),
    ...(subtitles.subtitleDelayMs !== undefined ? { subtitleDelayMs: subtitles.subtitleDelayMs } : {}),
    ...(subtitles.subtitlesOff ? { subtitlesOff: true } : {}),
  });

// ───────── Subtitles ─────────
export const searchSubtitles = (movieId: number, lang: string, infohash?: string) =>
  call("search_subtitles", infohash === undefined ? { movieId, lang } : { movieId, lang, infohash });
export const loadSubtitle = (subtitleId: string) => call("load_subtitle", { subtitleId });
export const loadSubtitleFile = (path: string) => call("load_subtitle_file", { path });
export const getSubtitlesStatus = () => call("get_subtitles_status");

/** "Cargar archivo…": native file dialog (tauri-plugin-dialog). Resolves to the path, or null if cancelled. */
export async function pickSubtitleFile(): Promise<string | null> {
  try {
    const path = await openDialog({
      title: "Cargar subtítulos",
      multiple: false,
      directory: false,
      filters: [{ name: "Subtítulos", extensions: ["srt", "vtt"] }],
    });
    return typeof path === "string" ? path : null;
  } catch (err) {
    throw toAppError(err);
  }
}

/** Native folder picker (Ajustes › Almacenamiento). Resolves to the absolute path, or null if cancelled. */
export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  try {
    const path = await openDialog({ title, directory: true, multiple: false, defaultPath });
    return typeof path === "string" ? path : null;
  } catch (err) {
    throw toAppError(err);
  }
}

/** Opens a web page in the system browser (tauri-plugin-opener); a new tab outside Tauri (dev). */
export async function openExternalUrl(url: string): Promise<void> {
  if (!isTauri()) {
    window.open(url, "_blank", "noopener");
    return;
  }
  try {
    await openUrl(url);
  } catch (err) {
    throw toAppError(err);
  }
}

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
/** Moves every download outside downloadsDir into it, in the background (progress: onMoveProgress). */
export const moveDownloads = () => call("move_downloads");
export const cancelMoveDownloads = () => call("cancel_move_downloads");

// ───────── Settings & storage ─────────
export const getSettings = () => call("get_settings");
export const updateSettings = (patch: SettingsPatch) => call("update_settings", { patch });
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
export const onMoveProgress = (handler: (p: EventMap["downloads://move-progress"]) => void) =>
  on("downloads://move-progress", handler);
export const onBackgroundError = (handler: (p: EventMap["app://error"]) => void) =>
  on("app://error", handler);
