import type { Quality, Torrent } from "../api/types";

const QUALITY_RANK: Record<Quality, number> = { "480p": 1, "720p": 2, "1080p": 3, "2160p": 4, "3D": 0 };

export const qualityRank = (q: Quality) => QUALITY_RANK[q];

/** Below this many seeds a version is flagged as slow to start. */
export const LOW_SEEDS = 15;

/**
 * Default version on the movie page: the highest-quality x264 torrent that has seeds
 * (x265/HEVC often doesn't play in WebKitGTK). 3D only when nothing else exists.
 * Ties go to the one with more seeds. Falls back to the best version with seeds, then to any.
 */
export function pickDefaultTorrent(torrents: readonly Torrent[]): Torrent | null {
  const better = (a: Torrent, b: Torrent) =>
    qualityRank(b.quality) - qualityRank(a.quality) || b.seeds - a.seeds;
  const best = (list: Torrent[]) => [...list].sort(better)[0] ?? null;
  const flat = torrents.filter((t) => t.quality !== "3D");
  return (
    best(flat.filter((t) => t.videoCodec === "x264" && t.seeds > 0)) ??
    best(flat.filter((t) => t.seeds > 0)) ??
    best(flat) ??
    best([...torrents])
  );
}

/** Display order for the versions table: quality ascending, x264 before x265, 3D last. */
export function sortForDisplay(torrents: readonly Torrent[]): Torrent[] {
  const key = (t: Torrent) => (t.quality === "3D" ? 99 : qualityRank(t.quality));
  return [...torrents].sort(
    (a, b) => key(a) - key(b) || (a.videoCodec === "x264" ? 0 : 1) - (b.videoCodec === "x264" ? 0 : 1),
  );
}

/** Swarm health in 0–5 bars. 0 = dead (no seeds). */
export function signalLevel(seeds: number): number {
  if (seeds <= 0) return 0;
  if (seeds < 5) return 1;
  if (seeds < LOW_SEEDS) return 2;
  if (seeds < 40) return 3;
  if (seeds < 90) return 4;
  return 5;
}

export type TorrentNote = { kind: "hevc" | "3d" | "no-seeds" | "low-seeds"; text: string };

/** Warnings shown next to a version, most important first. */
export function torrentNotes(t: Torrent): TorrentNote[] {
  const notes: TorrentNote[] = [];
  if (t.videoCodec === "x265") notes.push({ kind: "hevc", text: "HEVC: puede necesitar VLC" });
  if (t.quality === "3D") notes.push({ kind: "3d", text: "Requiere pantalla 3D" });
  if (t.seeds <= 0) notes.push({ kind: "no-seeds", text: "Sin seeds: puede no arrancar" });
  else if (t.seeds < LOW_SEEDS) notes.push({ kind: "low-seeds", text: "Pocos seeds: arranque lento" });
  return notes;
}

export const SOURCE_LABEL: Record<Torrent["source"], string> = { bluray: "BluRay", web: "WEB" };
