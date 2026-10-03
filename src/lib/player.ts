// Pure player logic: state machine, progress-bar layers, piece map and shortcuts. No React, no IPC.

import type { AppError, PieceMapWindow, StreamPhase, StreamSession, TorrentStats } from "../api/types";

// ───────── State machine ─────────

export type PlayerStatus =
  | "starting" // start_stream in flight
  | "buffering" // session open, below bufferTargetBytes
  | "playing" // <video> mounted and playing (or about to)
  | "paused"
  | "waiting" // playback ran out of data
  | "codec-error" // WebKitGTK can't decode this file
  | "failed"; // start_stream failed

export type PlayerState = {
  status: PlayerStatus;
  session: StreamSession | null;
  stats: TorrentStats | null;
  error: AppError | null;
};

export type PlayerEvent =
  | { type: "session"; session: StreamSession }
  | { type: "stats"; stats: TorrentStats }
  | { type: "force-start" }
  | { type: "start-failed"; error: AppError }
  | { type: "video-playing" }
  | { type: "video-pause" }
  | { type: "video-waiting" }
  | { type: "codec-error" }
  | { type: "retry" };

export const initialPlayerState: PlayerState = {
  status: "starting",
  session: null,
  stats: null,
  error: null,
};

/** Enough contiguous data to start, or the backend says so; a downloaded file starts right away. */
export function isBufferReady(session: StreamSession, stats: TorrentStats | null): boolean {
  if (session.source === "library") return true;
  if (!stats) return false;
  if (stats.phase === "ready" || stats.phase === "seeding" || stats.phase === "done") return true;
  return stats.bufferedAheadBytes >= session.bufferTargetBytes;
}

const isPreroll = (s: PlayerStatus) => s === "starting" || s === "buffering";
const isTerminal = (s: PlayerStatus) => s === "codec-error" || s === "failed";

export function playerReducer(state: PlayerState, event: PlayerEvent): PlayerState {
  if (event.type === "retry") return initialPlayerState;
  if (isTerminal(state.status) && event.type !== "session") return state;
  switch (event.type) {
    case "session": {
      const next = { ...state, session: event.session, error: null };
      return { ...next, status: isBufferReady(event.session, state.stats) ? "playing" : "buffering" };
    }
    case "stats": {
      const next = { ...state, stats: event.stats };
      if (isPreroll(state.status) && state.session && isBufferReady(state.session, event.stats))
        return { ...next, status: "playing" };
      if (state.status === "playing" && event.stats.phase === "stalled")
        return { ...next, status: "waiting" };
      return next;
    }
    case "force-start":
      return state.session && isPreroll(state.status) ? { ...state, status: "playing" } : state;
    case "start-failed":
      return { ...state, status: "failed", error: event.error };
    case "video-playing":
      return isPreroll(state.status) ? state : { ...state, status: "playing" };
    case "video-pause":
      return state.status === "playing" || state.status === "waiting"
        ? { ...state, status: "paused" }
        : state;
    case "video-waiting":
      return state.status === "playing" ? { ...state, status: "waiting" } : state;
    case "codec-error":
      return { ...state, status: "codec-error" };
  }
}

export const PHASE_TEXT: Record<StreamPhase, string> = {
  connecting: "Conectando al enjambre…",
  metadata: "Resolviendo el magnet…", // only on the magnet fallback (no .torrent from YTS)
  buffering: "Llenando el búfer…",
  ready: "Listo",
  stalled: "Sin datos: nadie está enviando piezas",
  seeding: "Listo",
  done: "Listo",
};

// ───────── Codec detection ─────────

const MEDIA_ERR_DECODE = 3;
const MEDIA_ERR_SRC_NOT_SUPPORTED = 4;

/** A <video> error that means "this codec won't play here" (vs. a network hiccup). */
export const isCodecError = (code: number | undefined) =>
  code === MEDIA_ERR_DECODE || code === MEDIA_ERR_SRC_NOT_SUPPORTED;

/** HEVC in WebKitGTK often plays audio with no picture: metadata loads with a 0×0 video track. */
export const isAudioOnly = (video: Pick<HTMLVideoElement, "videoWidth" | "videoHeight">) =>
  video.videoWidth === 0 || video.videoHeight === 0;

// ───────── Progress bar layers ─────────

export type Span = { start: number; end: number }; // fractions 0–1

const clamp01 = (n: number) => Math.min(1, Math.max(0, n));

/** <video>.buffered (seconds) → fractions of the duration. */
export function timeRangesToSpans(
  ranges: Pick<TimeRanges, "length" | "start" | "end">,
  duration: number,
): Span[] {
  if (!duration || !Number.isFinite(duration)) return [];
  const out: Span[] = [];
  for (let i = 0; i < ranges.length; i++) {
    out.push({ start: clamp01(ranges.start(i) / duration), end: clamp01(ranges.end(i) / duration) });
  }
  return out;
}

/** torrent://stats availableRanges → merged, sorted, clamped spans (pieces already on disk). */
export function availableToSpans(ranges: readonly [number, number][]): Span[] {
  const sorted = ranges
    .map(([a, b]) => ({ start: clamp01(Math.min(a, b)), end: clamp01(Math.max(a, b)) }))
    .filter((s) => s.end > s.start)
    .sort((a, b) => a.start - b.start);
  const merged: Span[] = [];
  for (const s of sorted) {
    const last = merged[merged.length - 1];
    if (last && s.start <= last.end) last.end = Math.max(last.end, s.end);
    else merged.push({ ...s });
  }
  return merged;
}

export type ScrubLayers = { played: number; buffered: Span[]; available: Span[] };

/** The three progress-bar layers: watched, in the <video> buffer, already downloaded. */
export function scrubLayers(input: {
  currentTime: number;
  duration: number;
  buffered: Pick<TimeRanges, "length" | "start" | "end"> | null;
  availableRanges: readonly [number, number][] | null;
}): ScrubLayers {
  const { currentTime, duration } = input;
  return {
    played: duration > 0 ? clamp01(currentTime / duration) : 0,
    buffered: input.buffered ? timeRangesToSpans(input.buffered, duration) : [],
    available: input.availableRanges ? availableToSpans(input.availableRanges) : [],
  };
}

// ───────── Piece map ─────────

export type PieceCell = "missing" | "ready" | "priority" | "arriving";
const CELL: Record<string, PieceCell> = { "0": "missing", "1": "ready", "2": "priority", "3": "arriving" };
export const PIECE_CELLS = 200;

/** pieceMap string ("0"–"3" per cell) → exactly 200 cells; missing data renders as missing. */
export function parsePieceMap(map: string | null): PieceCell[] {
  return Array.from({ length: PIECE_CELLS }, (_, i) => CELL[map?.[i] ?? "0"] ?? "missing");
}

// ───────── Shortcuts ─────────

export type PlayerAction =
  "toggle" | "back10" | "forward10" | "fullscreen" | "mute" | "escape" | "volUp" | "volDown";

export function shortcutAction(key: string): PlayerAction | null {
  switch (key) {
    case " ":
    case "k":
    case "K":
      return "toggle";
    case "ArrowLeft":
      return "back10";
    case "ArrowRight":
      return "forward10";
    case "ArrowUp":
      return "volUp";
    case "ArrowDown":
      return "volDown";
    case "f":
    case "F":
      return "fullscreen";
    case "m":
    case "M":
      return "mute";
    case "Escape":
      return "escape";
    default:
      return null;
  }
}

/** "1:02:03" / "4:05". */
export function formatClock(seconds: number): string {
  const s = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

const WINDOW_MB = 1024 * 1024;

/** "Próximos 64 MB desde la posición de lectura": what the 200 cells span. */
export function windowCaption(w: PieceMapWindow | null): string | null {
  if (!w || w.endByte <= w.startByte) return null;
  const mb = Math.round((w.endByte - w.startByte) / WINDOW_MB);
  return w.startByte === 0 && mb < 64
    ? `El archivo entero (${mb} MB)`
    : `Próximos ${mb} MB desde la posición de lectura`;
}
