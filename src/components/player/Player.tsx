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
import { pickDefaultTorrent } from "../../lib/versions";
import { useSwarmStore } from "../../store/swarm";
import { useUiStore } from "../../store/ui";
import { ErrorState } from "../ErrorState";
import { BufferScreen } from "./BufferScreen";
import { CodecError } from "./CodecError";
import { PlayerControls } from "./PlayerControls";

const HIDE_CONTROLS_MS = 3000;

type Props = { movie: MovieDetail; torrent: Torrent };

/** Stream session lifecycle, buffer pre-roll, <video> with custom controls and codec fallback. */
export function Player({ movie, torrent }: Props) {
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

  const showChrome = useCallback(() => {
    setChromeVisible(true);
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => {
      if (video.current && !video.current.paused) setChromeVisible(false);
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

  // ── Keyboard ──
  const preroll = state.status === "starting" || state.status === "buffering";
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement | null)?.closest?.("input, select, textarea")) return;
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
  ]);

  // ── <video> events ──
  const onLoadedMetadata = (e: SyntheticEvent<HTMLVideoElement>) => {
    const v = e.currentTarget;
    if (isAudioOnly(v)) {
      dispatch({ type: "codec-error" });
      return;
    }
    const resume = state.session?.resumeAtS;
    if (resume && resume < v.duration - 5) v.currentTime = resume;
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
          }}
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

      {preroll && (
        <BufferScreen
          movie={movie}
          torrent={torrent}
          session={state.session}
          stats={state.stats}
          alternative={alternative}
          resumeAtS={state.session?.resumeAtS ?? movie.progress?.positionS ?? null}
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
          />
        </div>
      )}

      {state.status === "codec-error" && (
        <CodecError movieId={movie.id} torrent={torrent} alternative={alternative} />
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
          />
        </div>
      )}
    </div>
  );
}
