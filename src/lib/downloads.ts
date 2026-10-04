import type { Download, DownloadState } from "../api/types";
import { formatRuntime } from "./format";

/** Progress of list_downloads while the Downloads page (or a downloading movie page) is visible. */
export const DOWNLOAD_POLL_MS = 1000;

/** Downloading or about to: what the navigation counter counts. */
export const isActive = (d: Download) =>
  d.state === "queued" || d.state === "active" || d.state === "stalled";

/** Can be paused (otherwise, if not done, resumed). */
export const canPause = (d: Download) => isActive(d);

export const activeCount = (list: readonly Download[] | undefined) => list?.filter(isActive).length ?? 0;

/** The download shown for a movie: a finished version first, then whichever is in progress. */
export function downloadForMovie(list: readonly Download[], movieId: number): Download | null {
  const mine = list.filter((d) => d.movie.id === movieId);
  return mine.find((d) => d.state === "done") ?? mine[0] ?? null;
}

/** Downloads page groups: "En curso" (newest first) and "En la biblioteca". */
export function groupDownloads(list: readonly Download[]) {
  const newest = [...list].sort((a, b) => b.addedAt.localeCompare(a.addedAt));
  return {
    inProgress: newest.filter((d) => d.state !== "done"),
    library: newest.filter((d) => d.state === "done"),
  };
}

export const STATE_LABEL: Record<DownloadState, string> = {
  queued: "En cola",
  active: "Descargando",
  paused: "En pausa",
  stalled: "Sin seeds conectados",
  done: "Completada",
  error: "Falló la descarga",
};

const pct = new Intl.NumberFormat("es-ES", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
/** 0.634 → "63,4 %". Never shows 100 % before it's done. */
export const formatPercent = (fraction: number) =>
  `${pct.format(Math.min(fraction < 1 ? 99.9 : 100, Math.max(0, fraction) * 100))} %`;

/** Whole percent for compact labels ("Descargando 45 %"). */
export const roundPercent = (fraction: number) =>
  `${Math.min(fraction < 1 ? 99 : 100, Math.floor(Math.max(0, fraction) * 100))} %`;

/** "quedan 1 h 4 min" / "quedan 3 min" / "menos de un minuto". */
export function formatEta(etaS: number): string {
  const min = Math.round(etaS / 60);
  return min < 1 ? "menos de un minuto" : `quedan ${formatRuntime(min)}`;
}
