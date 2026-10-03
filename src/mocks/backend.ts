// In-memory stand-in for the Rust backend, used only in the browser (`npm run dev`) and in tests.
// Every response follows docs/IPC.md / src/api/types.ts.

import type {
  AppError,
  CommandArgs,
  CommandName,
  CommandResult,
  ContinueItem,
  Download,
  MovieDetail,
  MovieSummary,
  Progress,
  Settings,
  StreamSession,
  SubtitleOption,
  TorrentStats,
} from "../api/types";
import catalog from "./catalog.json";

type CatalogMovie = Omit<MovieDetail, "isFavorite" | "progress" | "download">;
type Catalog = { movies: CatalogMovie[]; order: Record<string, number[]> };

const CATALOG = catalog as unknown as Catalog;
const STREAM_PORT = 47213;

function fail(code: AppError["code"], message: string): never {
  const err: AppError = { code, message };
  throw err;
}

export function toSummary(m: CatalogMovie): MovieSummary {
  return {
    id: m.id,
    imdbCode: m.imdbCode,
    title: m.title,
    year: m.year,
    rating: m.rating,
    runtimeMin: m.runtimeMin,
    genres: m.genres,
    coverUrl: m.coverUrl,
    coverLargeUrl: m.coverLargeUrl,
    backgroundUrl: m.backgroundUrl,
    qualities: m.qualities,
    hasX264: m.hasX264,
  };
}

const iso = (minutesAgo: number) => new Date(Date.now() - minutesAgo * 60_000).toISOString();

type Handler<C extends CommandName> = (args: CommandArgs<C>) => CommandResult<C>;
type Handlers = { [C in CommandName]: Handler<C> };

export type MockBackend = {
  handle: (cmd: string, args?: unknown) => unknown;
  /** Stats for every active torrent, as the backend would emit them each second. */
  tick: () => TorrentStats[];
};

export function createMockBackend(): MockBackend {
  const byId = new Map(CATALOG.movies.map((m) => [m.id, m]));
  const movie = (id: number) => byId.get(id) ?? fail("not_found", `movie ${id} not found`);
  const torrentOf = (infohash: string) => {
    for (const m of CATALOG.movies) {
      const t = m.torrents.find((x) => x.infohash === infohash);
      if (t) return { m, t };
    }
    return fail("not_found", `torrent ${infohash} not found`);
  };

  const favorites: MovieSummary[] = [3175, 3304, 3709, 7062, 1632]
    .filter((id) => byId.has(id))
    .map((id) => toSummary(movie(id)));

  const progress = new Map<number, Progress>();
  for (const [id, pos, min] of [
    [10960, 3720, 30],
    [7062, 1260, 300],
    [8462, 6900, 2000],
  ] as const) {
    const m = byId.get(id);
    if (m)
      progress.set(id, {
        movieId: id,
        positionS: pos,
        durationS: m.runtimeMin * 60,
        finished: false,
        updatedAt: iso(min),
      });
  }

  const downloads = new Map<string, Download>();
  const seedDownload = (id: number, quality: string, state: Download["state"], fraction: number) => {
    const m = byId.get(id);
    const t = m?.torrents.find((x) => x.quality === quality);
    if (!m || !t) return;
    downloads.set(t.infohash, {
      infohash: t.infohash,
      movie: toSummary(m),
      quality: t.quality,
      videoCodec: t.videoCodec,
      state,
      progress: fraction,
      sizeBytes: t.sizeBytes,
      downloadedBytes: Math.round(t.sizeBytes * fraction),
      downSpeedBps: state === "active" ? 5.2 * 1024 * 1024 : 0,
      peers: state === "active" ? 41 : 0,
      etaS: state === "active" ? 240 : null,
      path: state === "done" ? `~/.local/share/yts-player/library/${m.title} (${m.year})` : null,
      error: null,
      addedAt: iso(90),
    });
  };
  seedDownload(3175, "1080p", "active", 0.634);
  seedDownload(7062, "1080p", "active", 0.21);
  seedDownload(3709, "720p", "paused", 0.478);
  seedDownload(10960, "2160p", "stalled", 0.041);
  seedDownload(3304, "1080p", "done", 1);

  const streams = new Map<string, StreamSession>();

  let settings: Settings = {
    apiBaseUrls: ["https://movies-api.accel.li/api/v2/", "https://yts.gg/api/v2/"],
    openSubtitlesApiKey: null,
    subtitleLang: "es",
    autoSubtitles: true,
    preferredQuality: "1080p",
    preferX264: true,
    externalPlayer: "vlc",
    bufferTargetBytes: 8 * 1024 * 1024,
    downLimitKbps: null,
    upLimitKbps: 512,
    seedAfterDownload: false,
    listenPort: null,
    dataDir: "~/.local/share/yts-player",
    cacheLimitBytes: 10 * 1024 ** 3,
  };

  const detail = (m: CatalogMovie): MovieDetail => ({
    ...m,
    isFavorite: favorites.some((f) => f.id === m.id),
    progress: progress.get(m.id) ?? null,
    download: [...downloads.values()].find((d) => d.movie.id === m.id) ?? null,
  });

  const handlers: Handlers = {
    list_movies: ({ params }) => {
      const page = params.page ?? 1;
      const limit = Math.min(50, Math.max(1, params.limit ?? 20));
      const q = params.query?.trim().toLowerCase();
      let list = CATALOG.movies.filter(
        (m) =>
          (!q || m.title.toLowerCase().includes(q) || String(m.year) === q || m.imdbCode === q) &&
          (!params.genre || m.genres.some((g) => g.toLowerCase() === params.genre)) &&
          (!params.quality ||
            (params.quality === "1080p.x265"
              ? m.torrents.some((t) => t.quality === "1080p" && t.videoCodec === "x265")
              : m.qualities.includes(params.quality))) &&
          m.rating >= (params.minimumRating ?? 0),
      );
      const sortBy = params.sortBy ?? "date_added";
      const dir = params.orderBy === "asc" ? -1 : 1;
      const rank = (id: number) => {
        const order = CATALOG.order[sortBy] ?? [];
        const i = order.indexOf(id);
        return i < 0 ? Number.MAX_SAFE_INTEGER : i;
      };
      const seeds = (m: CatalogMovie) => Math.max(0, ...m.torrents.map((t) => t.seeds));
      list = [...list].sort((a, b) => {
        switch (sortBy) {
          case "title":
            return a.title.localeCompare(b.title, "es") * -dir;
          case "year":
            return (b.year - a.year) * dir;
          case "rating":
            return (b.rating - a.rating) * dir;
          case "seeds":
          case "peers":
            return (seeds(b) - seeds(a)) * dir;
          default:
            return (rank(a.id) - rank(b.id)) * dir;
        }
      });
      const start = (page - 1) * limit;
      return {
        movies: list.slice(start, start + limit).map(toSummary),
        total: list.length,
        page,
        limit,
        hasMore: start + limit < list.length,
      };
    },
    get_movie: ({ movieId }) => detail(movie(movieId)),
    get_suggestions: ({ movieId }) => {
      const m = movie(movieId);
      const shared = (x: CatalogMovie) => x.genres.filter((g) => m.genres.includes(g)).length;
      return CATALOG.movies
        .filter((x) => x.id !== m.id && shared(x) >= 2)
        .sort((a, b) => shared(b) - shared(a) || b.rating - a.rating)
        .slice(0, 4)
        .map(toSummary);
    },
    get_api_status: () => [
      { baseUrl: settings.apiBaseUrls[0] ?? "", role: "active", latencyMs: 182, ok: true },
      ...settings.apiBaseUrls
        .slice(1)
        .map((baseUrl) => ({ baseUrl, role: "fallback" as const, latencyMs: 240, ok: true })),
    ],

    start_stream: ({ movieId, infohash }) => {
      const existing = streams.get(infohash);
      if (existing) return existing;
      const { t } = torrentOf(infohash);
      const done = downloads.get(infohash)?.state === "done";
      const session: StreamSession = {
        infohash,
        movieId,
        streamUrl: `http://127.0.0.1:${STREAM_PORT}/stream/${infohash}/0`,
        fileName: `${movie(movieId).title}.${t.quality}.${t.videoCodec}.mp4`,
        fileSizeBytes: t.sizeBytes,
        videoCodec: t.videoCodec,
        likelyPlayable: t.videoCodec === "x264",
        bufferTargetBytes: settings.bufferTargetBytes,
        resumeAtS: progress.get(movieId)?.positionS ?? null,
        source: done ? "library" : "network",
      };
      streams.set(infohash, session);
      return session;
    },
    stop_stream: ({ infohash }) => {
      streams.delete(infohash);
    },
    open_external_player: ({ infohash }) => {
      torrentOf(infohash);
    },

    search_subtitles: ({ movieId, lang }) => {
      const m = movie(movieId);
      const options: SubtitleOption[] = [
        {
          id: `${m.id}-${lang}-1`,
          lang,
          label: `${m.title}.${m.year}.1080p.BluRay.x264-[YTS]`,
          downloads: 4812,
          hearingImpaired: false,
          matchesRelease: true,
        },
        {
          id: `${m.id}-${lang}-2`,
          lang,
          label: `${m.title}.${m.year}.WEBRip`,
          downloads: 1290,
          hearingImpaired: true,
          matchesRelease: false,
        },
      ];
      return options;
    },
    load_subtitle: ({ subtitleId }) => ({
      trackUrl: `http://127.0.0.1:${STREAM_PORT}/subs/${subtitleId}.vtt`,
      lang: subtitleId.split("-")[1] ?? null,
      label: "OpenSubtitles",
    }),
    load_subtitle_file: ({ path }) => ({
      trackUrl: `http://127.0.0.1:${STREAM_PORT}/subs/local.vtt`,
      lang: null,
      label: path.split("/").pop() ?? path,
    }),

    list_favorites: () => [...favorites],
    add_favorite: ({ movie: m }) => {
      if (!favorites.some((f) => f.id === m.id)) favorites.unshift(m);
    },
    remove_favorite: ({ movieId }) => {
      const i = favorites.findIndex((f) => f.id === movieId);
      if (i >= 0) favorites.splice(i, 1);
    },

    save_progress: ({ movie: m, positionS, durationS }) => {
      const p: Progress = {
        movieId: m.id,
        positionS,
        durationS,
        finished: durationS > 0 && positionS / durationS >= 0.92,
        updatedAt: new Date().toISOString(),
      };
      progress.set(m.id, p);
      return p;
    },
    get_progress: ({ movieId }) => progress.get(movieId) ?? null,
    list_continue_watching: () =>
      [...progress.values()]
        .filter((p) => !p.finished && byId.has(p.movieId))
        .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
        .map((p): ContinueItem => ({ movie: toSummary(movie(p.movieId)), progress: p })),
    remove_progress: ({ movieId }) => {
      progress.delete(movieId);
    },

    start_download: ({ movie: m, infohash }) => {
      const existing = downloads.get(infohash);
      if (existing) return existing;
      const { t } = torrentOf(infohash);
      const d: Download = {
        infohash,
        movie: m,
        quality: t.quality,
        videoCodec: t.videoCodec,
        state: "queued",
        progress: 0,
        sizeBytes: t.sizeBytes,
        downloadedBytes: 0,
        downSpeedBps: 0,
        peers: 0,
        etaS: null,
        path: null,
        error: null,
        addedAt: new Date().toISOString(),
      };
      downloads.set(infohash, d);
      return d;
    },
    list_downloads: () => [...downloads.values()],
    pause_download: ({ infohash }) => {
      const d = downloads.get(infohash) ?? fail("not_found", `download ${infohash} not found`);
      const next: Download = { ...d, state: "paused", downSpeedBps: 0, peers: 0, etaS: null };
      downloads.set(infohash, next);
      return next;
    },
    resume_download: ({ infohash }) => {
      const d = downloads.get(infohash) ?? fail("not_found", `download ${infohash} not found`);
      const next: Download = { ...d, state: "active" };
      downloads.set(infohash, next);
      return next;
    },
    remove_download: ({ infohash }) => {
      downloads.delete(infohash);
    },
    open_download_folder: ({ infohash }) => {
      if (!downloads.has(infohash)) fail("not_found", `download ${infohash} not found`);
    },

    get_settings: () => settings,
    update_settings: ({ patch }) => {
      settings = { ...settings, ...patch };
      return settings;
    },
    get_storage_usage: () => ({
      cacheBytes: Math.round(3.1 * 1024 ** 3),
      cacheLimitBytes: settings.cacheLimitBytes,
      libraryBytes: Math.round(24.6 * 1024 ** 3),
      freeDiskBytes: 180 * 1024 ** 3,
    }),
    clear_cache: () => ({ freedBytes: Math.round(3.1 * 1024 ** 3) }),

    open_trailer_window: () => undefined,
  };

  const handle = (cmd: string, args?: unknown): unknown => {
    if (!(cmd in handlers)) fail("internal", `mock: unknown command ${cmd}`);
    const h = handlers[cmd as CommandName] as (a: unknown) => unknown;
    return h(args);
  };

  const tick = (): TorrentStats[] => {
    const out: TorrentStats[] = [];
    for (const d of downloads.values()) {
      if (d.state !== "active") continue;
      const speed = Math.max(0.4, d.downSpeedBps / 1048576 + (Math.random() - 0.5) * 0.6) * 1048576;
      const downloadedBytes = Math.min(d.sizeBytes, d.downloadedBytes + speed);
      const next: Download = {
        ...d,
        downSpeedBps: Math.round(speed),
        downloadedBytes: Math.round(downloadedBytes),
        progress: downloadedBytes / d.sizeBytes,
        state: downloadedBytes >= d.sizeBytes ? "done" : "active",
        etaS: Math.round((d.sizeBytes - downloadedBytes) / speed),
      };
      downloads.set(d.infohash, next);
      out.push({
        infohash: d.infohash,
        phase: next.state === "done" ? "done" : "ready",
        peers: next.peers,
        seeds: Math.round(next.peers * 0.8),
        downSpeedBps: next.downSpeedBps,
        upSpeedBps: 400 * 1024,
        progress: next.progress,
        downloadedBytes: next.downloadedBytes,
        bufferedAheadBytes: 0,
        availableRanges: [[0, next.progress]],
        pieceMap: null,
      });
    }
    return out;
  };

  return { handle, tick };
}
