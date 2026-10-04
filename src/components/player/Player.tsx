import { useNavigate } from "@tanstack/react-router";
import { useCallback, useEffect, useMemo, useReducer, useRef, useState, type SyntheticEvent } from "react";
import { startStream, stopStream, toAppError } from "../../api/tauri";
import type { MovieDetail, Torrent } from "../../api/types";
import { isFullscreen, setFullscreen } from "../../lib/fullscreen";
import {
  initialPlayerState,
  isAudioOnly,
  isCodecError,
  playerReducer,
  scrubLayers,
  shortcutAction,
} from "../../lib/player";
import { onFileDrop } from "../../lib/fileDrop";
import { toSummary } from "../../lib/movie";
import { formatDelay, isSubtitleFile } from "../../lib/subtitles";
import { pickDefaultTorrent } from "../../lib/versions";
import { useSwarmStore } from "../../store/swarm";
import { useUiStore } from "../../store/ui";
import { ErrorState } from "../ErrorState";
import { BufferScreen } from "./BufferScreen";
import { CodecError } from "./CodecError";
import { PlayerControls } from "./PlayerControls";
import { SubtitleLayer } from "./SubtitleLayer";
import { SubtitleMenu } from "./SubtitleMenu";
import { SubtitleNotice } from "./SubtitleNotice";
import { useProgressSaver } from "./useProgressSaver";
import { useSubtitles } from "./useSubtitles";

const HIDE_CONTROLS_MS = 3000;
const OSD_MS = 1500;

type Props = {
  movie: MovieDetail;
  torrent: Torrent;
  /** "Desde el principio": ignore the saved position (resumeAtS). */
  fromStart?: boolean;
};

/** Stream session lifecycle, buffer pre-roll, <video> with custom controls and codec fallback. */
export function Player({ movie, torrent, fromStart = false }: Props) {
  const navigate = useNavigate();
  const [state, dispatch] = useReducer(playerReducer, initialPlayerState);
  const root = useRef<HTMLDivElement>(null);
  const video = useRef<HTMLVideoElement>(null);
  const { infohash } = torrent;

  // ── Session: start on mount, stop on leave (any route change, back, unmount) ──
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let alive = true;
    startStream(movie.id, infohash)
      .then((session) => alive && dispatch({ type: "session", session }))
      .catch((err: unknown) => alive && dispatch({ type: "start-failed", error: toAppError(err) }));
    return () => {
      alive = false;
      stopStream(infohash).catch(() => {
        // Leaving anyway; the backend pauses idle streams on its own.
      });
    };
  }, [movie.id, infohash, attempt]);

  // ── torrent://stats from the app-wide store ──
  const stats = useSwarmStore((s) => s.stats[infohash] ?? null);
  useEffect(() => {
    if (stats) dispatch({ type: "stats", stats });
  }, [stats]);

  // Same quality ladder, x264 only: the suggestion when this file won't decode.
  const alternative = useMemo(
    () =>
      torrent.videoCodec === "x264"
        ? null
        : pickDefaultTorrent(
            movie.torrents.filter((t) => t.videoCodec === "x264" && t.infohash !== infohash),
          ),
    [movie.torrents, torrent.videoCodec, infohash],
  );

  // ── <video> mirror state ──
  const [time, setTime] = useState({ current: 0, duration: 0 });
  const [buffered, setBuffered] = useState<TimeRanges | null>(null);
  // The element's real paused flag drives the play/pause icon: the reducer status can lag behind
  // when WebKitGTK skips events (e.g. a seek fires `pause` and resumes without `playing`).
  const [isPaused, setIsPaused] = useState(true);
  const { volume, muted, setVolume, toggleMuted } = useUiStore();
  const [fullscreen, setFs] = useState(false);
  const [chromeVisible, setChromeVisible] = useState(true);
  const hideTimer = useRef<number | undefined>(undefined);
  // The subtitle menu keeps the controls up while it's open.
  const [menuOpen, setMenuOpen] = useState(false);
  const menuOpenRef = useRef(false);
  useEffect(() => {
    menuOpenRef.current = menuOpen;
  }, [menuOpen]);

  const showChrome = useCallback(() => {
    setChromeVisible(true);
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => {
      if (video.current && !video.current.paused && !menuOpenRef.current) setChromeVisible(false);
    }, HIDE_CONTROLS_MS);
  }, []);
  useEffect(() => () => window.clearTimeout(hideTimer.current), []);

  useEffect(() => {
    if (video.current) {
      video.current.volume = volume;
      video.current.muted = muted;
    }
  }, [volume, muted, state.status]);

  const back = useCallback(
    () => void navigate({ to: "/movie/$movieId", params: { movieId: movie.id } }),
    [navigate, movie.id],
  );

  const togglePlay = useCallback(() => {
    const v = video.current;
    if (!v) return;
    if (v.paused) void v.play().catch(() => undefined);
    else v.pause();
    showChrome();
  }, [showChrome]);

  const seekTo = useCallback(
    (seconds: number) => {
      const v = video.current;
      if (!v || !Number.isFinite(v.duration)) return;
      v.currentTime = Math.min(v.duration, Math.max(0, seconds));
      showChrome();
    },
    [showChrome],
  );

  const toggleFullscreen = useCallback(async () => {
    if (!root.current) return;
    const on = !(await isFullscreen());
    await setFullscreen(on, root.current).catch(() => undefined);
    setFs(await isFullscreen());
  }, []);

  useEffect(() => {
    const sync = () => setFs(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", sync);
    return () => document.removeEventListener("fullscreenchange", sync);
  }, []);

  // ── Subtitles ──
  const preroll = state.status === "starting" || state.status === "buffering";
  const playable = !!state.session && !preroll && state.status !== "failed" && state.status !== "codec-error";
  const subs = useSubtitles({ movieId: movie.id, infohash, started: playable, menuOpen });
  const { nudgeDelay, loadFile } = subs;
  const [osd, setOsd] = useState<string | null>(null);
  const osdTimer = useRef<number | undefined>(undefined);
  const flash = useCallback((text: string) => {
    setOsd(text);
    window.clearTimeout(osdTimer.current);
    osdTimer.current = window.setTimeout(() => setOsd(null), OSD_MS);
  }, []);
  useEffect(() => () => window.clearTimeout(osdTimer.current), []);
  const nudge = useCallback(
    (dir: 1 | -1) => {
      nudgeDelay(dir);
      flash(`Retraso de subtítulos: ${formatDelay(useUiStore.getState().subtitleDelay[movie.id] ?? 0)}`);
    },
    [nudgeDelay, flash, movie.id],
  );

  // Dropping a .srt/.vtt on the window loads it (Tauri gives the real path).
  const [dragging, setDragging] = useState(false);
  useEffect(() => {
    let off: (() => void) | undefined;
    let alive = true;
    void onFileDrop((e) => {
      if (e.type === "leave") setDragging(false);
      else if (e.type === "enter") setDragging(e.paths.some(isSubtitleFile));
      else {
        setDragging(false);
        const path = e.paths.find(isSubtitleFile);
        if (path) void loadFile(path);
        else if (e.paths.length) flash("Solo se pueden cargar subtítulos .srt o .vtt");
      }
    }).then((unlisten) => {
      if (alive) off = unlisten;
      else unlisten();
    });
    return () => {
      alive = false;
      off?.();
    };
  }, [loadFile, flash]);

  // ── Keyboard ──
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement | null)?.closest?.("input, select, textarea, [role=menu]")) return;
      if (preroll || state.status === "codec-error" || state.status === "failed") {
        if (e.key === "Enter" && preroll) dispatch({ type: "force-start" });
        else if (e.key === "Escape") back();
        return;
      }
      const action = shortcutAction(e.key);
      if (!action) return;
      e.preventDefault();
      const v = video.current;
      switch (action) {
        case "toggle":
          togglePlay();
          break;
        case "back10":
          if (v) seekTo(v.currentTime - 10);
          break;
        case "forward10":
          if (v) seekTo(v.currentTime + 10);
          break;
        case "volUp":
          setVolume(volume + 0.1);
          showChrome();
          break;
        case "volDown":
          setVolume(volume - 0.1);
          showChrome();
          break;
        case "mute":
          toggleMuted();
          showChrome();
          break;
        case "fullscreen":
          void toggleFullscreen();
          break;
        case "subsEarlier":
          nudge(-1);
          break;
        case "subsLater":
          nudge(1);
          break;
        case "escape":
          void isFullscreen().then((fs) => (fs ? toggleFullscreen() : back()));
          break;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [
    preroll,
    state.status,
    back,
    togglePlay,
    seekTo,
    setVolume,
    volume,
    toggleMuted,
    showChrome,
    toggleFullscreen,
    nudge,
  ]);

  // ── Progress: save_progress every 10 s, on pause, on ended and on leave ──
  const summary = useMemo(() => toSummary(movie), [movie]);
  const progress = useProgressSaver(video, summary, !!state.session && !preroll);
  const resumeAtS = fromStart ? null : (state.session?.resumeAtS ?? null);

  // ── <video> events ──
  const onLoadedMetadata = (e: SyntheticEvent<HTMLVideoElement>) => {
    const v = e.currentTarget;
    if (isAudioOnly(v)) {
      dispatch({ type: "codec-error" });
      return;
    }
    if (resumeAtS && resumeAtS < v.duration - 5) v.currentTime = resumeAtS;
    setTime({ current: v.currentTime, duration: v.duration });
  };
  // WebKitGTK/GStreamer often recovers from a stall without firing `playing` again.
  const onResumable = (e: SyntheticEvent<HTMLVideoElement>) => {
    if (!e.currentTarget.paused) dispatch({ type: "video-resumed" });
  };
  const onError = (e: SyntheticEvent<HTMLVideoElement>) => {
    if (isCodecError(e.currentTarget.error?.code)) dispatch({ type: "codec-error" });
    else dispatch({ type: "video-waiting" });
  };

  const layers = scrubLayers({
    currentTime: time.current,
    duration: time.duration,
    buffered,
    availableRanges: state.stats?.availableRanges ?? null,
  });

  const showVideo = !!state.session && !preroll && state.status !== "failed";
  const art = movie.screenshotUrls[0] ?? movie.backgroundUrl ?? movie.coverLargeUrl;

  return (
    <div
      ref={root}
      className={`fixed inset-0 z-[100] overflow-hidden bg-black ${!chromeVisible ? "cursor-none" : ""}`}
      onMouseMove={showChrome}
      data-status={state.status}
    >
      {(preroll || state.status === "codec-error" || state.status === "failed") && art && (
        <img
          src={art}
          alt=""
          className="absolute inset-0 size-full scale-[1.06] object-cover opacity-20 blur-[22px] saturate-[.6]"
        />
      )}

      {showVideo && state.session && (
        <video
          ref={video}
          src={state.session.streamUrl}
          autoPlay
          playsInline
          className={`absolute inset-0 size-full bg-black object-contain ${state.status === "codec-error" ? "invisible" : ""}`}
          onLoadedMetadata={onLoadedMetadata}
          onTimeUpdate={(e) => {
            const v = e.currentTarget;
            setTime({ current: v.currentTime, duration: v.duration });
            setIsPaused(v.paused);
            progress.track();
            dispatch({ type: "video-timeupdate", currentTime: v.currentTime, paused: v.paused });
          }}
          onDurationChange={(e) => {
            const duration = e.currentTarget.duration;
            setTime((t) => ({ ...t, duration }));
          }}
          onProgress={(e) => setBuffered(e.currentTarget.buffered)}
          onPlay={(e) => {
            setIsPaused(e.currentTarget.paused);
            dispatch({ type: "video-play" });
          }}
          onPlaying={(e) => {
            setIsPaused(e.currentTarget.paused);
            dispatch({ type: "video-playing" });
            showChrome();
          }}
          onPause={(e) => {
            setIsPaused(e.currentTarget.paused);
            dispatch({ type: "video-pause" });
            setChromeVisible(true);
            progress.onPause();
          }}
          onEnded={() => progress.onEnded()}
          onWaiting={() => dispatch({ type: "video-waiting" })}
          onSeeking={() => dispatch({ type: "video-seeking" })}
          onSeeked={onResumable}
          onCanPlay={onResumable}
          onCanPlayThrough={onResumable}
          onError={onError}
          onClick={togglePlay}
          data-testid="video"
        />
      )}

      {playable && <SubtitleLayer video={video} cues={subs.cues} delay={subs.delay} raised={chromeVisible} />}

      {playable && subs.notice && (
        <SubtitleNotice
          notice={subs.notice}
          onDismiss={subs.dismissNotice}
          onFallback={subs.tryFallback}
          onPickFile={() => void subs.pickFile()}
        />
      )}

      {osd && (
        <div
          role="status"
          className="pointer-events-none absolute top-[84px] right-gutter rounded-lg bg-black/75 px-4 py-2 text-sm font-semibold tnum"
        >
          {osd}
        </div>
      )}

      {dragging && (
        <div className="pointer-events-none absolute inset-4 z-20 grid place-items-center rounded-lg border-2 border-dashed border-green bg-black/60 text-lg font-bold">
          Suelta el archivo para cargar los subtítulos
        </div>
      )}

      {preroll && (
        <BufferScreen
          movie={movie}
          torrent={torrent}
          session={state.session}
          stats={state.stats}
          alternative={alternative}
          subtitles={subs.externalArgs}
          resumeAtS={
            fromStart
              ? null
              : (state.session?.resumeAtS ??
                (movie.progress && !movie.progress.finished ? movie.progress.positionS : null))
          }
          onBack={back}
        />
      )}

      {state.status === "failed" && state.error && (
        <div className="absolute inset-0 grid place-items-center p-6">
          <ErrorState
            error={state.error}
            onRetry={() => {
              dispatch({ type: "retry" });
              setAttempt((n) => n + 1);
            }}
            onBack={back}
            backLabel="Volver a la ficha"
          />
        </div>
      )}

      {state.status === "codec-error" && (
        <CodecError
          movieId={movie.id}
          torrent={torrent}
          alternative={alternative}
          subtitles={subs.externalArgs}
        />
      )}

      {state.status === "waiting" && (
        <div className="pointer-events-none absolute inset-0 grid place-items-center" role="status">
          <div className="flex items-center gap-3 rounded-lg bg-black/70 px-5 py-3 text-[15px] font-semibold">
            <span className="pulse-dot" aria-hidden="true" />
            Esperando datos… · {state.stats?.peers ?? 0} peers
          </div>
        </div>
      )}

      {showVideo && state.status !== "codec-error" && (
        <div
          className={`transition-opacity duration-300 ${chromeVisible ? "opacity-100" : "pointer-events-none opacity-0"}`}
        >
          <PlayerControls
            title={movie.title}
            torrent={torrent}
            stats={state.stats}
            playing={!isPaused}
            currentTime={time.current}
            duration={time.duration}
            layers={layers}
            volume={volume}
            muted={muted}
            fullscreen={fullscreen}
            onToggle={togglePlay}
            onSeek={seekTo}
            onSkip={(d) => video.current && seekTo(video.current.currentTime + d)}
            onVolume={setVolume}
            onMute={toggleMuted}
            onFullscreen={() => void toggleFullscreen()}
            onBack={back}
            subtitles={
              <SubtitleMenu
                subs={subs}
                open={menuOpen}
                onOpenChange={(open) => {
                  setMenuOpen(open);
                  showChrome();
                }}
              />
            }
          />
        </div>
      )}
    </div>
  );
}
