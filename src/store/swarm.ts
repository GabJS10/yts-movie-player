import { create } from "zustand";
import { onTorrentStats } from "../api/tauri";
import type { TorrentStats } from "../api/types";

const STALE_MS = 2500;

type SwarmState = {
  /** Latest torrent://stats per infohash, with the time it arrived. */
  stats: Record<string, TorrentStats & { at: number }>;
  /** Total download speed across torrents heard from in the last 2.5 s, bytes/s. */
  totalBps: number;
  push: (s: TorrentStats, now?: number) => void;
};

export const useSwarmStore = create<SwarmState>()((set, get) => ({
  stats: {},
  totalBps: 0,
  push: (s, now = Date.now()) => {
    const stats = { ...get().stats, [s.infohash]: { ...s, at: now } };
    let totalBps = 0;
    for (const v of Object.values(stats)) if (now - v.at <= STALE_MS) totalBps += v.downSpeedBps;
    set({ stats, totalBps });
  },
}));

let started = false;

/**
 * Subscribes once, app-wide, to `torrent://stats` (called from main.tsx after IPC is ready).
 * Components read from the store instead of listening themselves.
 */
export async function startSwarmListener(): Promise<void> {
  if (started) return;
  started = true;
  await onTorrentStats((s) => useSwarmStore.getState().push(s));
}
