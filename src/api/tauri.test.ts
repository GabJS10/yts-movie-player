import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "../mocks/backend";
import {
  getMovie,
  getSubtitlesStatus,
  listMovies,
  loadSubtitleFile,
  onTorrentStats,
  pickSubtitleFile,
  searchSubtitles,
  toAppError,
  updateSettings,
} from "./tauri";
import type { TorrentStats } from "./types";

describe("tauri api (mock backend)", () => {
  beforeEach(() => {
    const backend = createMockBackend();
    mockIPC((cmd, args) => backend.handle(cmd, args), { shouldMockEvents: true });
  });

  it("list_movies returns a typed page", async () => {
    const page = await listMovies({ query: "interstellar", limit: 5 });
    expect(page.total).toBe(1);
    expect(page.hasMore).toBe(false);
    const [movie] = page.movies;
    expect(movie?.title).toBe("Interstellar");
    expect(movie?.qualities).toContain("1080p");
    expect(typeof movie?.hasX264).toBe("boolean");
  });

  it("list_movies filters by lowercase genre and paginates", async () => {
    const page = await listMovies({ genre: "animation", limit: 3, page: 1 });
    expect(page.movies).toHaveLength(3);
    expect(page.hasMore).toBe(true);
    expect(page.movies.every((m) => m.genres.includes("Animation"))).toBe(true);
  });

  it("get_movie returns detail with lowercase infohashes", async () => {
    const movie = await getMovie(1632);
    expect(movie.torrents.length).toBeGreaterThan(0);
    for (const t of movie.torrents) expect(t.infohash).toMatch(/^[0-9a-f]{40}$/);
    expect(movie.isFavorite).toBe(true);
  });

  it("rejects with an AppError", async () => {
    await expect(getMovie(999_999_999)).rejects.toMatchObject({ code: "not_found" });
  });

  it("update_settings returns the full settings", async () => {
    const s = await updateSettings({ preferX264: false });
    expect(s.preferX264).toBe(false);
    expect(s.subtitleLang).toBe("es");
  });

  it("update_settings: null clears a nullable field, absent keys stay", async () => {
    const before = await updateSettings({});
    expect(before.upLimitKbps).toBe(512);
    const after = await updateSettings({ upLimitKbps: null });
    expect(after.upLimitKbps).toBeNull();
    expect(after.subtitleLang).toBe(before.subtitleLang);
  });

  it("delivers typed events", async () => {
    const handler = vi.fn<(s: TorrentStats) => void>();
    const unlisten = await onTorrentStats(handler);
    const stats = createMockBackend().tick()[0];
    expect(stats).toBeDefined();
    await emit("torrent://stats", stats);
    expect(handler).toHaveBeenCalledWith(stats);
    unlisten();
  });

  it("normalizes non-contract errors to internal", () => {
    expect(toAppError(new Error("boom"))).toEqual({ code: "internal", message: "boom" });
    expect(toAppError({ code: "no_peers", message: "x" })).toEqual({ code: "no_peers", message: "x" });
  });
});

describe("subtitles wrappers", () => {
  it("search_subtitles without a key rejects with subtitles_auth; the status reports the quota", async () => {
    const backend = createMockBackend();
    mockIPC((cmd, args) => backend.handle(cmd, args));
    expect(await getSubtitlesStatus()).toMatchObject({
      configured: true,
      loggedIn: false,
      remainingDownloads: null,
    });
    await updateSettings({ openSubtitlesApiKey: null });
    await expect(searchSubtitles(1632, "es")).rejects.toMatchObject({ code: "subtitles_auth" });
    // A local file always works.
    expect(await loadSubtitleFile("/x/peli.srt")).toMatchObject({ label: "peli.srt", lang: null });
  });

  it("pickSubtitleFile asks the dialog plugin for one .srt/.vtt and returns its path, or null if cancelled", async () => {
    let answer: unknown = "/home/u/peli.srt";
    const seen: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd !== "plugin:dialog|open") throw new Error(cmd);
      seen.push(args);
      return answer;
    });
    expect(await pickSubtitleFile()).toBe("/home/u/peli.srt");
    expect(seen[0]).toMatchObject({
      options: { multiple: false, directory: false, filters: [{ extensions: ["srt", "vtt"] }] },
    });
    answer = null;
    expect(await pickSubtitleFile()).toBeNull();
  });
});
