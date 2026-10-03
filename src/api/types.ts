// IPC contract types. Mirror of docs/IPC.md (v0.6); keep both in sync in the same change.
// Optional outputs are `T | null` (never undefined); optional inputs are `field?: T` (omit = default).

// ───────── Errors ─────────

export type ErrorCode =
  | "network"
  | "api_unavailable"
  | "not_found"
  | "invalid_input"
  | "torrent"
  | "no_peers"
  | "subtitles_auth"
  | "subtitles_quota"
  | "external_player_missing"
  | "io"
  | "db"
  | "internal";

export type AppError = {
  code: ErrorCode;
  /** Technical detail in English, for logs. Never shown to the user as-is. */
  message: string;
};

// ───────── Shared types ─────────

export type Quality = "480p" | "720p" | "1080p" | "2160p" | "3D";
export type VideoCodec = "x264" | "x265";
export type TorrentSource = "bluray" | "web";

export type Torrent = {
  infohash: string;
  quality: Quality;
  source: TorrentSource;
  videoCodec: VideoCodec;
  bitDepth: number | null;
  audioChannels: string | null;
  sizeBytes: number;
  seeds: number;
  peers: number;
  uploadedAt: string | null;
};

export type MovieSummary = {
  id: number;
  imdbCode: string;
  title: string;
  year: number;
  rating: number;
  runtimeMin: number;
  genres: string[];
  /** null when YTS has no cover: the UI shows a placeholder. */
  coverUrl: string | null;
  coverLargeUrl: string | null;
  backgroundUrl: string | null;
  qualities: Quality[];
  hasX264: boolean;
  /** Highest seed count among its torrents (card swarm signal); 0 without torrents. */
  maxSeeds: number;
};

export type CastMember = {
  name: string;
  character: string | null;
  imageUrl: string | null;
};

export type Progress = {
  movieId: number;
  positionS: number;
  durationS: number;
  finished: boolean;
  updatedAt: string;
};

export type MovieDetail = MovieSummary & {
  summary: string;
  language: string;
  mpaRating: string | null;
  ytTrailerCode: string | null;
  cast: CastMember[];
  /** Sharp 1280 px stills; [] if none. Hero and movie page use [0] ?? backgroundUrl (which is small and blurred). */
  screenshotUrls: string[];
  torrents: Torrent[];
  isFavorite: boolean;
  progress: Progress | null;
  download: Download | null;
};

export type MoviePage = {
  movies: MovieSummary[];
  total: number;
  page: number;
  limit: number;
  hasMore: boolean;
};

// ───────── Catalog ─────────

export type SortBy =
  "title" | "year" | "rating" | "peers" | "seeds" | "download_count" | "like_count" | "date_added";

export type OrderBy = "desc" | "asc";

export type ListMoviesParams = {
  page?: number;
  limit?: number;
  query?: string;
  genre?: string;
  quality?: Quality | "1080p.x265";
  minimumRating?: number;
  sortBy?: SortBy;
  orderBy?: OrderBy;
};

export type ApiEndpointStatus = {
  baseUrl: string;
  role: "active" | "fallback";
  latencyMs: number | null;
  ok: boolean;
};

// ───────── Streaming ─────────

export type StreamSession = {
  infohash: string;
  movieId: number;
  streamUrl: string;
  fileName: string;
  fileSizeBytes: number;
  videoCodec: VideoCodec;
  likelyPlayable: boolean;
  bufferTargetBytes: number;
  resumeAtS: number | null;
  source: "network" | "library";
};

// ───────── Subtitles ─────────

export type SubtitleOption = {
  id: string;
  lang: string;
  label: string;
  downloads: number;
  hearingImpaired: boolean;
  matchesRelease: boolean;
};

export type SubtitleTrack = {
  trackUrl: string;
  lang: string | null;
  label: string;
};

// ───────── Continue watching ─────────

export type ContinueItem = { movie: MovieSummary; progress: Progress };

// ───────── Downloads ─────────

export type DownloadState = "queued" | "active" | "paused" | "stalled" | "done" | "error";

export type Download = {
  infohash: string;
  movie: MovieSummary;
  quality: Quality;
  videoCodec: VideoCodec;
  state: DownloadState;
  progress: number;
  sizeBytes: number;
  downloadedBytes: number;
  downSpeedBps: number;
  peers: number;
  etaS: number | null;
  path: string | null;
  error: string | null;
  addedAt: string;
};

// ───────── Settings & storage ─────────

export type Settings = {
  apiBaseUrls: string[];
  openSubtitlesApiKey: string | null;
  subtitleLang: string;
  autoSubtitles: boolean;
  preferredQuality: Quality;
  preferX264: boolean;
  externalPlayer: string;
  bufferTargetBytes: number;
  downLimitKbps: number | null;
  upLimitKbps: number | null;
  seedAfterDownload: boolean;
  listenPort: number | null;
  dataDir: string;
  cacheLimitBytes: number;
};

/**
 * Partial settings update. Absent key = leave unchanged. On nullable fields
 * (openSubtitlesApiKey, downLimitKbps, upLimitKbps, listenPort) `null` = clear the value.
 * `dataDir` is stored but only applied after restarting the app.
 */
export type SettingsPatch = Partial<Settings>;

export type ClearCacheResult = { freedBytes: number };

export type StorageUsage = {
  cacheBytes: number;
  cacheLimitBytes: number;
  libraryBytes: number;
  freeDiskBytes: number;
};

// ───────── Events ─────────

export type StreamPhase = "connecting" | "metadata" | "buffering" | "ready" | "stalled" | "seeding" | "done";

export type PieceMapWindow = { startByte: number; endByte: number };

export type TorrentStats = {
  infohash: string;
  phase: StreamPhase;
  peers: number;
  seeds: number;
  downSpeedBps: number;
  upSpeedBps: number;
  progress: number;
  downloadedBytes: number;
  bufferedAheadBytes: number;
  /** Fractions 0–1 already on disk. */
  availableRanges: [number, number][];
  /**
   * 200 cells over `pieceMapWindow` (not the whole file): "0" missing, "1" ready, "2" priority,
   * "3" arriving (reserved, not emitted yet). Only while a stream is open.
   */
  pieceMap: string | null;
  /** Byte range the pieceMap covers: 64 MB from the read position (the whole file if smaller). */
  pieceMapWindow: PieceMapWindow | null;
};

export type DownloadChanged = { infohash: string; download: Download | null };

export type BackgroundError = AppError & { infohash: string | null };

export type EventMap = {
  "torrent://stats": TorrentStats;
  "download://changed": DownloadChanged;
  "app://error": BackgroundError;
};

export type EventName = keyof EventMap;

// ───────── Command map (name → args / result) ─────────

export type CommandMap = {
  list_movies: { args: { params: ListMoviesParams }; result: MoviePage };
  get_movie: { args: { movieId: number }; result: MovieDetail };
  get_suggestions: { args: { movieId: number }; result: MovieSummary[] };
  get_api_status: { args: undefined; result: ApiEndpointStatus[] };

  start_stream: { args: { movieId: number; infohash: string }; result: StreamSession };
  stop_stream: { args: { infohash: string }; result: void };
  open_external_player: { args: { infohash: string }; result: void };

  search_subtitles: {
    args: { movieId: number; lang: string; infohash?: string };
    result: SubtitleOption[];
  };
  load_subtitle: { args: { subtitleId: string }; result: SubtitleTrack };
  load_subtitle_file: { args: { path: string }; result: SubtitleTrack };

  list_favorites: { args: undefined; result: MovieSummary[] };
  add_favorite: { args: { movie: MovieSummary }; result: void };
  remove_favorite: { args: { movieId: number }; result: void };

  save_progress: {
    args: { movie: MovieSummary; positionS: number; durationS: number };
    result: Progress;
  };
  get_progress: { args: { movieId: number }; result: Progress | null };
  list_continue_watching: { args: undefined; result: ContinueItem[] };
  remove_progress: { args: { movieId: number }; result: void };

  start_download: { args: { movie: MovieSummary; infohash: string }; result: Download };
  list_downloads: { args: undefined; result: Download[] };
  pause_download: { args: { infohash: string }; result: Download };
  resume_download: { args: { infohash: string }; result: Download };
  remove_download: { args: { infohash: string; deleteFiles: boolean }; result: void };
  open_download_folder: { args: { infohash: string }; result: void };

  get_settings: { args: undefined; result: Settings };
  update_settings: { args: { patch: SettingsPatch }; result: Settings };
  get_storage_usage: { args: undefined; result: StorageUsage };
  clear_cache: { args: undefined; result: ClearCacheResult };

  open_trailer_window: { args: { ytTrailerCode: string; title: string }; result: void };
};

export type CommandName = keyof CommandMap;
export type CommandArgs<C extends CommandName> = CommandMap[C]["args"];
export type CommandResult<C extends CommandName> = CommandMap[C]["result"];
