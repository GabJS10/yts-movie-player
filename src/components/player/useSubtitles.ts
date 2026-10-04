import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { useSettings } from "../../api/queries";
import {
  getSubtitlesStatus,
  loadSubtitle,
  loadSubtitleFile,
  pickSubtitleFile,
  searchSubtitles,
  toAppError,
} from "../../api/tauri";
import type { AppError, SubtitleOption, SubtitleTrack } from "../../api/types";
import { FALLBACK_LANG, parseVtt, stepDelay, type Cue } from "../../lib/subtitles";
import { useUiStore } from "../../store/ui";

export type SubtitleSelection =
  { kind: "off" } | { kind: "option"; option: SubtitleOption } | { kind: "file"; label: string };

export type SubtitleNotice =
  /** autoSubtitles is on but there's no API key. */
  | { kind: "no-key" }
  /** The key was rejected. */
  | { kind: "auth" }
  /** Nothing in the preferred language: offer English. */
  | { kind: "none"; lang: string }
  | { kind: "quota"; resetAt: string | null }
  | { kind: "error"; error: AppError };

const subtitlesKey = (movieId: number, lang: string, infohash: string) =>
  ["subtitles", movieId, lang, infohash] as const;

/** The .vtt the backend serves (or a data: URL from the mock) → cues. */
async function fetchCues(track: SubtitleTrack): Promise<Cue[]> {
  const res = await fetch(track.trackUrl);
  if (!res.ok) throw new Error(`subtitle fetch failed: ${res.status}`);
  return parseVtt(await res.text());
}

type Params = {
  movieId: number;
  infohash: string;
  /** The video is on screen: time to auto-load. */
  started: boolean;
  /** The menu is open: list the options of the language shown. */
  menuOpen: boolean;
};

/**
 * Subtitles for one playback session: auto-load per Ajustes (autoSubtitles + subtitleLang, offering
 * English when there's nothing), manual choice, local files, the per-movie delay and the notices.
 */
export function useSubtitles({ movieId, infohash, started, menuOpen }: Params) {
  const qc = useQueryClient();
  const settings = useSettings().data;
  const hasKey = !!settings?.openSubtitlesApiKey;
  const preferred = settings?.subtitleLang ?? "es";

  const [lang, setLang] = useState<string | null>(null);
  const shownLang = lang ?? preferred;
  const options = useQuery({
    queryKey: subtitlesKey(movieId, shownLang, infohash),
    queryFn: () => searchSubtitles(movieId, shownLang, infohash),
    enabled: menuOpen && hasKey,
    staleTime: Infinity,
    retry: false,
  });

  const [selection, setSelection] = useState<SubtitleSelection>({ kind: "off" });
  const [cues, setCues] = useState<Cue[]>([]);
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [notice, setNotice] = useState<SubtitleNotice | null>(null);
  // Each load bumps this; a slower earlier load must not replace a newer choice.
  const generation = useRef(0);
  const cache = useRef(new Map<string, Cue[]>());

  const delay = useUiStore((s) => s.subtitleDelay[movieId] ?? 0);
  const setDelayFor = useUiStore((s) => s.setSubtitleDelay);
  const nudgeDelay = useCallback(
    (dir: 1 | -1) => setDelayFor(movieId, stepDelay(useUiStore.getState().subtitleDelay[movieId] ?? 0, dir)),
    [movieId, setDelayFor],
  );

  const fail = useCallback(async (err: unknown) => {
    const error = toAppError(err);
    if (error.code === "subtitles_quota") {
      const resetAt = await getSubtitlesStatus().then(
        (s) => s.resetAt,
        () => null,
      );
      setNotice({ kind: "quota", resetAt });
    } else if (error.code === "subtitles_auth") setNotice({ kind: "auth" });
    else setNotice({ kind: "error", error });
  }, []);

  /** Loads a track and shows it, unless something newer was chosen meanwhile. */
  const apply = useCallback(
    async (key: string, next: SubtitleSelection, load: () => Promise<SubtitleTrack>) => {
      const gen = ++generation.current;
      setLoadingId(key);
      try {
        let loaded = cache.current.get(key);
        let label: string | null = null;
        if (!loaded) {
          const track = await load();
          label = track.label;
          loaded = await fetchCues(track);
          cache.current.set(key, loaded);
        }
        if (gen !== generation.current) return;
        setCues(loaded);
        setSelection(next.kind === "file" && label ? { kind: "file", label } : next);
        setNotice(null);
      } catch (err) {
        if (gen === generation.current) await fail(err);
      } finally {
        if (gen === generation.current) setLoadingId(null);
      }
    },
    [fail],
  );

  const choose = useCallback(
    (option: SubtitleOption) => apply(option.id, { kind: "option", option }, () => loadSubtitle(option.id)),
    [apply],
  );

  const loadFile = useCallback(
    (path: string) =>
      apply(`file:${path}`, { kind: "file", label: path.split(/[\\/]/).pop() ?? path }, () =>
        loadSubtitleFile(path),
      ),
    [apply],
  );

  const pickFile = useCallback(async () => {
    try {
      const path = await pickSubtitleFile();
      if (path) await loadFile(path);
    } catch (err) {
      await fail(err);
    }
  }, [fail, loadFile]);

  const turnOff = useCallback(() => {
    generation.current++;
    setLoadingId(null);
    setSelection({ kind: "off" });
    setCues([]);
  }, []);

  /** Search `code` and load its first result; tells the user when there's none. */
  const loadFirstIn = useCallback(
    async (code: string) => {
      setLang(code);
      try {
        const found = await qc.fetchQuery({
          queryKey: subtitlesKey(movieId, code, infohash),
          queryFn: () => searchSubtitles(movieId, code, infohash),
          staleTime: Infinity,
          retry: false,
        });
        const first = found[0];
        if (first) await choose(first);
        else setNotice({ kind: "none", lang: code });
      } catch (err) {
        await fail(err);
      }
    },
    [qc, movieId, infohash, choose, fail],
  );

  const dismissNotice = useCallback(() => setNotice(null), []);
  const tryFallback = useCallback(() => void loadFirstIn(FALLBACK_LANG), [loadFirstIn]);

  // ── Auto-load once, when playback starts ──
  const autoRan = useRef(false);
  useEffect(() => {
    if (!started || !settings || autoRan.current) return;
    autoRan.current = true;
    if (!settings.autoSubtitles) return;
    if (!settings.openSubtitlesApiKey) {
      queueMicrotask(() => setNotice({ kind: "no-key" }));
      return;
    }
    const code = settings.subtitleLang;
    queueMicrotask(() => void loadFirstIn(code));
  }, [started, settings, loadFirstIn]);

  return {
    hasKey,
    preferred,
    shownLang,
    showLang: setLang,
    options,
    selection,
    cues,
    loadingId,
    notice,
    dismissNotice,
    delay,
    nudgeDelay,
    choose,
    loadFile,
    pickFile,
    turnOff,
    /** "Usar inglés" from the notice. */
    tryFallback,
  };
}

export type Subtitles = ReturnType<typeof useSubtitles>;
