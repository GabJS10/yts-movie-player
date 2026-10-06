import { getT, type Messages } from "../i18n";
import type { AppError, ErrorCode } from "./types";

// User-facing copy for every contract error code lives in the dictionaries (src/i18n, `errors`).
export const describeError = (error: AppError) => getT().errors[error.code];

/** Where to go from each error, besides (or instead of) retrying. */
export type ErrorNext = {
  /** A retry can help (temporary failures). */
  retry: boolean;
  link?: { to: "/" | "/downloads" | "/settings"; hash?: string; label: keyof Messages["errorLinks"] };
};

export const ERROR_NEXT: Record<ErrorCode, ErrorNext> = {
  network: { retry: true, link: { to: "/downloads", label: "downloads" } },
  api_unavailable: {
    retry: true,
    link: { to: "/settings", hash: "s-catalogo", label: "catalogServers" },
  },
  not_found: { retry: false, link: { to: "/", label: "home" } },
  invalid_input: { retry: true },
  torrent: { retry: true },
  no_peers: { retry: true },
  subtitles_auth: {
    retry: false,
    link: { to: "/settings", hash: "s-subs", label: "subtitlesSettings" },
  },
  // With an OpenSubtitles account the daily quota is larger.
  subtitles_quota: {
    retry: false,
    link: { to: "/settings", hash: "s-subs", label: "moreQuota" },
  },
  external_player_missing: {
    retry: true,
    link: { to: "/settings", hash: "s-play", label: "playbackSettings" },
  },
  io: { retry: true, link: { to: "/settings", hash: "s-disk", label: "storageSettings" } },
  db: { retry: true },
  internal: { retry: true },
};
