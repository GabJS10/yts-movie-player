import { create } from "zustand";
import { getSubtitlesStatus } from "../api/tauri";
import type { SubtitlesStatus } from "../api/types";

// "Cupo agotado" mode (IPC v0.9), for the session: after a subtitles_quota error or a status with no
// downloads left, options not in the disk cache open their OpenSubtitles page instead of loading.
// Ends by itself at resetAt.

type QuotaState = {
  exhausted: boolean;
  /** ISO 8601, when OpenSubtitles renews the quota (null = unknown). */
  resetAt: string | null;
};

export const useSubtitlesQuota = create<QuotaState>()(() => ({ exhausted: false, resetAt: null }));

let expiry: number | undefined;

export function markQuotaExhausted(resetAt: string | null) {
  window.clearTimeout(expiry);
  useSubtitlesQuota.setState({ exhausted: true, resetAt });
  const ms = resetAt ? Date.parse(resetAt) - Date.now() : Number.NaN;
  if (Number.isFinite(ms)) {
    // setTimeout overflows past ~24.8 days; the quota renews daily anyway.
    expiry = window.setTimeout(clearQuota, Math.max(0, Math.min(ms, 2 ** 31 - 1)));
  }
}

export function clearQuota() {
  window.clearTimeout(expiry);
  useSubtitlesQuota.setState({ exhausted: false, resetAt: null });
}

/** Records what a status says about the quota (0 left → exhausted). */
export function recordStatus(status: SubtitlesStatus) {
  if (status.remainingDownloads === 0) markQuotaExhausted(status.resetAt);
  else if (status.remainingDownloads !== null) clearQuota();
}

/** get_subtitles_status, keeping the quota mode in sync. */
export async function fetchSubtitlesStatus(): Promise<SubtitlesStatus> {
  const status = await getSubtitlesStatus();
  recordStatus(status);
  return status;
}

/** After a quota error: enter the mode with the renewal time from the status (if it answers). */
export async function onQuotaExhausted(): Promise<string | null> {
  const resetAt = await getSubtitlesStatus().then(
    (s) => s.resetAt,
    () => null,
  );
  markQuotaExhausted(resetAt);
  return resetAt;
}
