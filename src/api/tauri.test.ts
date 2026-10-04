import { mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toSummary } from "../lib/movie";
import { createMockBackend } from "../mocks/backend";
import {
  getMovie,
  getSubtitlesStatus,
  listDownloads,
  listMovies,
  loadSubtitleFile,
  onDownloadChanged,
  onTorrentStats,
  openDownloadFolder,
  pauseDownload,
  pickSubtitleFile,
  removeDownload,
  resumeDownload,
  searchSubtitles,
  startDownload,
  toAppError,
  updateSettings,
} from "./tauri";
import type { DownloadChanged, TorrentStats } from "./types";

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

describe("downloads wrappers (IPC v0.10)", () => {
  const calls: { cmd: string; args: unknown }[] = [];
  let backend: ReturnType<typeof createMockBackend>;
  beforeEach(() => {
    calls.length = 0;
    backend = createMockBackend();
    backend.onDownloadChanged((p) => void emit("download://changed", p));
    mockIPC(
      (cmd, args) => {
        calls.push({ cmd, args });
        return backend.handle(cmd, args);
      },
      { shouldMockEvents: true },
    );
  });

  it("send the contract arguments and return Download", async () => {
    const movie = await getMovie(1632);
    const t = movie.torrents[0]!;
    const summary = toSummary(movie);
    const d = await startDownload(summary, t.infohash);
    expect(d).toMatchObject({ infohash: t.infohash, state: "queued", progress: 0, quality: t.quality });
    expect(calls.at(-1)).toEqual({ cmd: "start_download", args: { movie: summary, infohash: t.infohash } });

    expect((await listDownloads()).some((x) => x.infohash === t.infohash)).toBe(true);
    expect((await pauseDownload(t.infohash)).state).toBe("paused");
    expect((await resumeDownload(t.infohash)).state).toBe("active");
    await openDownloadFolder(t.infohash);
    expect(calls.at(-1)).toEqual({ cmd: "open_download_folder", args: { infohash: t.infohash } });
    await removeDownload(t.infohash, true);
    expect(calls.at(-1)).toEqual({
      cmd: "remove_download",
      args: { infohash: t.infohash, deleteFiles: true },
    });
    await expect(pauseDownload(t.infohash)).rejects.toMatchObject({ code: "not_found" });
  });

  it("download://changed carries the new state, and null once removed", async () => {
    const handler = vi.fn<(p: DownloadChanged) => void>();
    const unlisten = await onDownloadChanged(handler);
    const movie = await getMovie(1632);
    const infohash = movie.torrents[0]!.infohash;
    await startDownload(toSummary(movie), infohash);
    await vi.waitFor(() =>
      expect(handler).toHaveBeenLastCalledWith({
        infohash,
        download: expect.objectContaining({ state: "queued" }),
      }),
    );
    backend.tick(); // the mock's queue starts at once
    await vi.waitFor(() =>
      expect(handler).toHaveBeenLastCalledWith({
        infohash,
        download: expect.objectContaining({ state: "active" }),
      }),
    );
    backend.completeDownload(infohash);
    await vi.waitFor(() =>
      expect(handler).toHaveBeenLastCalledWith({
        infohash,
        download: expect.objectContaining({ state: "done", progress: 1, path: expect.stringContaining("[") }),
      }),
    );
    await removeDownload(infohash, false);
    await vi.waitFor(() => expect(handler).toHaveBeenLastCalledWith({ infohash, download: null }));
    unlisten();
  });

  it("offline: get_movie answers with the saved copy of downloaded movies only", async () => {
    backend.setOffline(true);
    const copy = await getMovie(3304);
    expect(copy.offline).toBe(true);
    expect(copy.download?.state).toBe("done");
    await expect(getMovie(1632)).rejects.toMatchObject({ code: "network" });
    await expect(listMovies({})).rejects.toMatchObject({ code: "network" });
    backend.setOffline(false);
    expect((await getMovie(1632)).offline).toBe(false);
  });
});
