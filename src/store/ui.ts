import { create } from "zustand";

// Client-only UI state. Remote data lives in TanStack Query, not here.
type UiState = {
  volume: number; // 0–1
  muted: boolean;
  setVolume: (volume: number) => void;
  toggleMuted: () => void;
  /** Subtitle delay in seconds per movie id, remembered for the session only. */
  subtitleDelay: Record<number, number>;
  setSubtitleDelay: (movieId: number, delay: number) => void;
};

export const useUiStore = create<UiState>()((set) => ({
  volume: 0.8,
  muted: false,
  setVolume: (volume) => set({ volume: Math.min(1, Math.max(0, volume)), muted: false }),
  toggleMuted: () => set((s) => ({ muted: !s.muted })),
  subtitleDelay: {},
  setSubtitleDelay: (movieId, delay) =>
    set((s) => ({ subtitleDelay: { ...s.subtitleDelay, [movieId]: delay } })),
}));
