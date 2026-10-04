import { create } from "zustand";
import type { MoveProgress } from "../api/types";

// "Mover también las descargas existentes": the move runs in the backend; this keeps its latest
// downloads://move-progress so the dialog can close and reopen without losing it.

type MoveState = {
  /** Latest progress of the current (or just finished) move; null = none. */
  progress: MoveProgress | null;
  /** The progress dialog is showing. */
  open: boolean;
  /** move_downloads was asked for; the first event hasn't arrived yet. */
  starting: boolean;
};

export const useMoveStore = create<MoveState>()(() => ({ progress: null, open: false, starting: false }));

export const moveRunning = (s: MoveState) => s.starting || (!!s.progress && !s.progress.finished);

export const resetMove = () => useMoveStore.setState({ progress: null, open: false, starting: false });
