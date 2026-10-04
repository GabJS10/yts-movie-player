import type { TrailerMessage } from "../api/types";

// Trailer chain (fase 7): modal with the embed → open_trailer_window → the system browser; "Ver en
// YouTube" always available. YouTube refuses embeds without a usable Referer (error 153), which is why
// the modal loads the backend's local trailer page; see src-tauri/src/stream.rs.

/** Without a "ready" from the player by then, the modal is given up. */
export const TRAILER_TIMEOUT_MS = 8000;

export type TrailerSignal = Pick<TrailerMessage, "event" | "code">;

/**
 * What a message from the trailer iframe means. Two senders: the backend's local page
 * (`{ source: "yts-trailer", event, code }`) and a direct YouTube embed with enablejsapi=1 (JSON strings
 * with onReady / onStateChange / onError / infoDelivery).
 */
export function parseTrailerMessage(data: unknown): TrailerSignal | null {
  if (data && typeof data === "object" && (data as { source?: unknown }).source === "yts-trailer") {
    const { event, code } = data as { event?: unknown; code?: unknown };
    if (event === "ready" || event === "playing" || event === "ended" || event === "error")
      return typeof code === "number" ? { event, code } : { event };
    return null;
  }
  if (typeof data !== "string" || !data.startsWith("{")) return null;
  let msg: { event?: unknown; info?: unknown };
  try {
    msg = JSON.parse(data) as typeof msg;
  } catch {
    return null;
  }
  if (msg.event === "onReady") return { event: "ready" };
  if (msg.event === "onError")
    return typeof msg.info === "number" ? { event: "error", code: msg.info } : { event: "error" };
  if (msg.event === "onStateChange" && msg.info === 1) return { event: "playing" };
  if (msg.event === "onStateChange" && msg.info === 0) return { event: "ended" };
  if (msg.event === "infoDelivery" && (msg.info as { playerState?: unknown } | null)?.playerState === 1)
    return { event: "playing" };
  return null;
}

/** Only messages from the trailer page's own origin count (IPC v0.12.1). */
export function originOf(url: string): string | null {
  try {
    // Relative URLs (the dev mock's page) resolve against the app.
    return new URL(url, window.location.href).origin;
  } catch {
    return null;
  }
}

/** The trailer on youtube.com (the last step of the chain and "Ver en YouTube"). */
export const youtubeWatchUrl = (code: string) =>
  `https://www.youtube.com/watch?v=${encodeURIComponent(code)}`;
