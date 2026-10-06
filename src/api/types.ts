// IPC contract types. Mirror of docs/IPC.md (v0.13); keep both in sync in the same change.
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
  /** Local trailer page (http://127.0.0.1:<port>/trailer/<code>?title=…), session-only; null without a trailer. */
  trailerUrl: string | null;
  cast: CastMember[];
  /** Sharp 1280 px stills; [] if none. Hero and movie page use [0] ?? backgroundUrl (which is small and blurred). */
  screenshotUrls: string[];
  torrents: Torrent[];
  isFavorite: boolean;
  progress: Progress | null;
  /** Any version of this movie being downloaded or already downloaded. */
  download: Download | null;
  /** The copy saved when downloading (no network): fields missing from it come empty. */
  offline: boolean;
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

/** postMessage from the /trailer page to its parent (and opener). `code`: YouTube's, only on "error". */
export type TrailerMessage = {
  source: "yts-trailer";
  event: "ready" | "playing" | "ended" | "error";
  code?: number;
};

// ───────── Recommendations ─────────

export type FeaturedReason =
  | { kind: "because_watched"; sourceMovieId: number; sourceTitle: string }
  | { kind: "because_list"; sourceMovieId: number; sourceTitle: string }
  /** Lowercase, as in ListMoviesParams.genre. */
  | { kind: "genre"; genre: string }
  | { kind: "trending" };

/** A full movie (stills, summary, trailer) so the banner needs no more calls. */
export type FeaturedItem = { movie: MovieDetail; reason: FeaturedReason };

export type HomeProfile = {
  /** "Porque viste X" row. */
  becauseWatched: { sourceMovieId: number; sourceTitle: string; movies: MovieSummary[] } | null;
  /** Lowercase genres by affinity; empty = no history (the front's default order). */
  genreOrder: string[];
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

/** Subtitles for the external player; none of them = the backend picks per Ajustes. */
export type ExternalSubtitleArgs = {
  /** OpenSubtitles file chosen in the player. */
  subtitleId?: string;
  /** The user's own .srt/.vtt. */
  subtitlePath?: string;
  /** Positive = subtitles show later. */
  subtitleDelayMs?: number;
  /** The user chose "Desactivados": open without subtitles and don't search. */
  subtitlesOff?: boolean;
};

export type ExternalPlayerResult = {
  /** "none" = subtitlesOff, or not requested and auto-load is off. The player opens either way. */
  subtitle: "loaded" | "none" | "no_key" | "quota" | "not_found" | "unsupported_player" | "error";
};

// ───────── Subtitles ─────────

export type SubtitleOption = {
  id: string;
  lang: string;
  /** Release (or file) name; null when OpenSubtitles gives none: the UI names it. */
  label: string | null;
  downloads: number;
  hearingImpaired: boolean;
  matchesRelease: boolean;
  /** Translated by AI or machine. */
  aiTranslated: boolean;
  /** Its page on opensubtitles.com (to download it by hand when the quota is spent). */
  pageUrl: string | null;
  /** Already in the disk cache: loading it doesn't spend quota. */
  cached: boolean;
};

export type SubtitleTrack = {
  trackUrl: string;
  lang: string | null;
  /** Release or file name; null when unknown. */
  label: string | null;
};

export type SubtitlesStatus = {
  /** There is an API key. */
  configured: boolean;
  /** Username and password are set and the login worked. */
  loggedIn: boolean;
  /** null when OpenSubtitles doesn't report it (no login). */
  remainingDownloads: number | null;
  /** ISO 8601: when the daily quota renews (last known value). */
  resetAt: string | null;
};

// ───────── Continue watching ─────────

export type ContinueItem = { movie: MovieSummary; progress: Progress };

// ───────── Downloads ─────────

/** unavailable = its folder is gone (unmounted disk); moving = being moved by move_downloads. */
export type DownloadState =
  "queued" | "active" | "paused" | "stalled" | "done" | "error" | "unavailable" | "moving";

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
  openSubtitlesUsername: string | null;
  openSubtitlesPassword: string | null;
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
  /**
   * Absolute path, as the OS writes it; default <dataDir>/library (StorageUsage.defaultDownloadsDir). Applies at
   * once; existing downloads stay.
   */
  downloadsDir: string;
  /** Absolute path; default <dataDir>/cache (StorageUsage.defaultCacheDir). Applies at once; the old cache is dropped. */
  cacheDir: string;
  cacheLimitBytes: number;
};

/** Settings fields whose patch accepts `null` to go back to the default folder. */
type FolderKey = "downloadsDir" | "cacheDir";

/**
 * Partial settings update. Absent key = leave unchanged. On nullable fields (openSubtitlesApiKey,
 * openSubtitlesUsername, openSubtitlesPassword, downLimitKbps, upLimitKbps, listenPort, downloadsDir,
 * cacheDir) `null` = clear the value (no key, no limit, automatic port, default folder).
 */
export type SettingsPatch = Partial<Omit<Settings, FolderKey>> & { [K in FolderKey]?: string | null };

export type ClearCacheResult = { freedBytes: number };

export type StorageUsage = {
  cacheBytes: number;
  cacheLimitBytes: number;
  /** All downloads, wherever they are. */
  libraryBytes: number;
  /** Free space on cacheDir's disk. */
  cacheFreeBytes: number;
  /** Free space on downloadsDir's disk. */
  downloadsFreeBytes: number;
  /** false = missing or not writable (streaming falls back to the default folder). */
  cacheDirAvailable: boolean;
  downloadsDirAvailable: boolean;
  /** Downloads still in another folder (to offer "Mover"). */
  downloadsOutsideDir: number;
  /** For "Restablecer" and to tell whether a folder is the default one. */
  defaultDownloadsDir: string;
  defaultCacheDir: string;
};

// ───────── Application ─────────

export type AppInfo = {
  /** The package's, e.g. "1.0.0". */
  version: string;
  /**
   * Linux: ~/.local/state/yts-player/logs (or $XDG_STATE_HOME/…); Windows: %LOCALAPPDATA%\yts-player\logs.
   * Show it as given; never build it in the frontend.
   */
  logsDir: string;
  /**
   * DB, settings and subtitles. Linux: ~/.local/share/yts-player; Windows: %LOCALAPPDATA%\yts-player;
   * YTS_PLAYER_DATA_DIR overrides it (E2E).
   */
  dataDir: string;
  repoUrl: string;
};

/** A newer release on GitHub. */
export type UpdateInfo = {
  version: string;
  /** The release page (html_url). */
  url: string;
  publishedAt: string;
};

// ───────── Events ─────────

/** no_peers: 60 s after start_stream without ever connecting to a peer (the torrent keeps trying). */
export type StreamPhase =
  "connecting" | "metadata" | "buffering" | "ready" | "stalled" | "no_peers" | "seeding" | "done";

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

/** downloads://move-progress, ~4 times a second during move_downloads and once at the end. */
export type MoveProgress = {
  /** 1-based: the download being moved. */
  index: number;
  total: number;
  infohash: string | null;
  /** Of the whole move. */
  bytesDone: number;
  bytesTotal: number;
  finished: boolean;
  cancelled: boolean;
  /** Not moved (they stay where they were). `message` is technical, not for the UI. */
  failed: { infohash: string; message: string }[];
};

export type EventMap = {
  "torrent://stats": TorrentStats;
  "download://changed": DownloadChanged;
  "app://error": BackgroundError;
  "downloads://move-progress": MoveProgress;
};

export type EventName = keyof EventMap;

// ───────── Command map (name → args / result) ─────────

export type CommandMap = {
  list_movies: { args: { params: ListMoviesParams }; result: MoviePage };
  get_movie: { args: { movieId: number }; result: MovieDetail };
  get_suggestions: { args: { movieId: number }; result: MovieSummary[] };
  get_api_status: { args: undefined; result: ApiEndpointStatus[] };
  get_featured: { args: undefined; result: FeaturedItem[] };
  get_home_profile: { args: undefined; result: HomeProfile };

  start_stream: { args: { movieId: number; infohash: string }; result: StreamSession };
  stop_stream: { args: { infohash: string }; result: void };
  open_external_player: { args: { infohash: string } & ExternalSubtitleArgs; result: ExternalPlayerResult };

  search_subtitles: {
    args: { movieId: number; lang: string; infohash?: string };
    result: SubtitleOption[];
  };
  load_subtitle: { args: { subtitleId: string }; result: SubtitleTrack };
  load_subtitle_file: { args: { path: string }; result: SubtitleTrack };
  get_subtitles_status: { args: undefined; result: SubtitlesStatus };

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
  move_downloads: { args: undefined; result: void };
  cancel_move_downloads: { args: undefined; result: void };

  get_settings: { args: undefined; result: Settings };
  update_settings: { args: { patch: SettingsPatch }; result: Settings };
  get_storage_usage: { args: undefined; result: StorageUsage };
  clear_cache: { args: undefined; result: ClearCacheResult };

  open_trailer_window: { args: { ytTrailerCode: string; title: string }; result: void };

  get_app_info: { args: undefined; result: AppInfo };
  /** null when there's nothing newer, offline or an unreadable answer: it never fails. */
  check_for_update: { args: undefined; result: UpdateInfo | null };
  open_logs_folder: { args: undefined; result: void };
};

export type CommandName = keyof CommandMap;
export type CommandArgs<C extends CommandName> = CommandMap[C]["args"];
export type CommandResult<C extends CommandName> = CommandMap[C]["result"];
