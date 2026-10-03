import { create } from "zustand";
import { onTorrentStats } from "../api/tauri";

const STALE_MS = 2500;

type SwarmState = {
  /** Latest download speed (bytes/s) per active torrent. */
  speeds: Record<string, { bps: number; at: number }>;
  /** Total download speed across active torrents, bytes/s. */
  totalBps: number;
};

export const useSwarmStore = create<SwarmState>()(() => ({ speeds: {}, totalBps: 0 }));

let started = false;

/** Subscribes once, app-wide, to `torrent://stats`. Called from main.tsx after IPC is ready. */
export async function startSwarmListener(): Promise<void> {
  if (started) return;
  started = true;
  await onTorrentStats((s) => {
    const now = Date.now();
    const speeds = { ...useSwarmStore.getState().speeds, [s.infohash]: { bps: s.downSpeedBps, at: now } };
    let totalBps = 0;
    for (const [hash, v] of Object.entries(speeds)) {
      if (now - v.at > STALE_MS) delete speeds[hash];
      else totalBps += v.bps;
    }
    useSwarmStore.setState({ speeds, totalBps });
  });
}
