import { afterEach, describe, expect, it, vi } from "vitest";
import { clearQuota, markQuotaExhausted, recordStatus, useSubtitlesQuota } from "./subtitlesQuota";

afterEach(() => {
  vi.useRealTimers();
  clearQuota();
});

describe("cupo agotado mode", () => {
  it("ends by itself when the quota renews", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-04T20:00:00Z"));
    markQuotaExhausted("2026-10-05T00:00:00Z");
    expect(useSubtitlesQuota.getState()).toEqual({ exhausted: true, resetAt: "2026-10-05T00:00:00Z" });
    vi.advanceTimersByTime(4 * 3600_000 - 1);
    expect(useSubtitlesQuota.getState().exhausted).toBe(true);
    vi.advanceTimersByTime(1);
    expect(useSubtitlesQuota.getState().exhausted).toBe(false);
  });

  it("follows get_subtitles_status: 0 left enters it, some left leaves it, unknown changes nothing", () => {
    const status = { configured: true, loggedIn: true, resetAt: null };
    recordStatus({ ...status, remainingDownloads: null });
    expect(useSubtitlesQuota.getState().exhausted).toBe(false);
    recordStatus({ ...status, remainingDownloads: 0 });
    expect(useSubtitlesQuota.getState().exhausted).toBe(true);
    recordStatus({ ...status, remainingDownloads: null });
    expect(useSubtitlesQuota.getState().exhausted).toBe(true);
    recordStatus({ ...status, remainingDownloads: 5 });
    expect(useSubtitlesQuota.getState().exhausted).toBe(false);
  });
});
