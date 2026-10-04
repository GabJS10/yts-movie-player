import type { Quality, Torrent } from "../api/types";

const QUALITY_RANK: Record<Quality, number> = { "480p": 1, "720p": 2, "1080p": 3, "2160p": 4, "3D": 0 };

export const qualityRank = (q: Quality) => QUALITY_RANK[q];

/** Below this many seeds a version is flagged as slow to start. */
export const LOW_SEEDS = 15;

/** The two settings that steer the default version (Ajustes › Reproducción). */
export type TorrentPrefs = { preferredQuality: Quality; preferX264: boolean };

/**
 * Default version on the movie page and in the player. Tiers, in order: x264 with seeds (x265/HEVC
 * often doesn't play in WebKitGTK; skipped when `preferX264` is off), anything with seeds, anything
 * but 3D, anything. Inside a tier the preferred quality wins, then the closest one below it, then the
 * closest above; ties go to the one with more seeds. Without prefs, the highest quality wins.
 */
export function pickDefaultTorrent(
  torrents: readonly Torrent[],
  prefs?: TorrentPrefs | null,
): Torrent | null {
  const target = prefs && prefs.preferredQuality !== "3D" ? qualityRank(prefs.preferredQuality) : 4;
  // Lower is better: at or below the target, nearest first; above it, after every lower one.
  const distance = (t: Torrent) => {
    const r = qualityRank(t.quality);
    return r <= target ? target - r : 10 + r;
  };
  const better = (a: Torrent, b: Torrent) => distance(a) - distance(b) || b.seeds - a.seeds;
  const best = (list: Torrent[]) => [...list].sort(better)[0] ?? null;
  const flat = torrents.filter((t) => t.quality !== "3D");
  const seeded = flat.filter((t) => t.seeds > 0);
  const x264First = prefs?.preferX264 ?? true;
  return (
    (x264First ? best(seeded.filter((t) => t.videoCodec === "x264")) : null) ??
    best(seeded) ??
    best(flat) ??
    best([...torrents])
  );
}

/**
 * When nobody shares this version (no_peers): the best-seeded other one, x264 first (it plays here), and
 * only if it has more seeds than this one. 3D is never offered.
 */
export function betterSwarm(torrents: readonly Torrent[], current: Torrent): Torrent | null {
  const others = torrents.filter(
    (t) => t.infohash !== current.infohash && t.quality !== "3D" && t.seeds > current.seeds,
  );
  const x264 = (t: Torrent) => (t.videoCodec === "x264" ? 0 : 1);
  return [...others].sort((a, b) => x264(a) - x264(b) || b.seeds - a.seeds)[0] ?? null;
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
