import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, type RefObject } from "react";
import { applySavedProgress } from "../../api/queries";
import { saveProgress } from "../../api/tauri";
import type { MovieSummary } from "../../api/types";

export const SAVE_EVERY_MS = 10_000;
/** Below this, opening and leaving a movie doesn't put it in Continuar viendo. */
export const MIN_SAVED_POSITION_S = 5;

type Position = { positionS: number; durationS: number };

/**
 * save_progress every 10 s while the <video> really plays, on pause, on ended and when leaving.
 * Reads the element's own paused/currentTime/duration: WebKitGTK skips `playing` too often to rely on
 * it (src/lib/player.ts). Failures are silent: the next save retries, and playback must not stop.
 */
export function useProgressSaver(
  video: RefObject<HTMLVideoElement | null>,
  movie: MovieSummary,
  active: boolean,
) {
  const qc = useQueryClient();
  // Last position read from the element: on unmount the element may already be gone.
  const last = useRef<Position | null>(null);
  const savedAt = useRef<number | null>(null);

  const read = useCallback((): Position | null => {
    const v = video.current;
    if (v && Number.isFinite(v.currentTime)) {
      const durationS = Number.isFinite(v.duration) && v.duration > 0 ? v.duration : movie.runtimeMin * 60;
      last.current = { positionS: v.currentTime, durationS };
    }
    return last.current;
  }, [video, movie.runtimeMin]);

  const save = useCallback(
    (at: Position | null) => {
      if (!at || at.durationS <= 0 || at.positionS < MIN_SAVED_POSITION_S) return;
      if (savedAt.current !== null && Math.abs(savedAt.current - at.positionS) < 1) return;
      savedAt.current = at.positionS;
      saveProgress(movie, at.positionS, at.durationS)
        .then((progress) => applySavedProgress(qc, progress))
        .catch((err: unknown) => {
          savedAt.current = null;
          console.warn("save_progress failed", err);
        });
    },
    [movie, qc],
  );

  useEffect(() => {
    if (!active) return;
    const id = window.setInterval(() => {
      const v = video.current;
      if (v && !v.paused) save(read());
    }, SAVE_EVERY_MS);
    return () => window.clearInterval(id);
  }, [active, video, read, save]);

  // Leaving the player (back, Escape, route change): save where it was.
  const saveRef = useRef(() => save(last.current));
  useEffect(() => {
    saveRef.current = () => save(read());
  }, [read, save]);
  useEffect(() => () => saveRef.current(), []);

  return {
    /** From timeupdate: keeps the last position for the save on leave. */
    track: () => void read(),
    onPause: () => save(read()),
    /** The end counts as watched (≥ 92 % → finished, out of Continuar viendo). */
    onEnded: () => {
      const at = read();
      if (at) save({ positionS: at.durationS, durationS: at.durationS });
    },
  };
}
