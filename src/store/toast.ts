import { create } from "zustand";

// One transient message at a time, bottom-center (DESIGN.md: toast). A new one replaces the last.
export type Toast = { id: number; text: string; tone: "ok" | "error" };

type ToastState = {
  toast: Toast | null;
  show: (text: string, tone?: Toast["tone"]) => void;
  dismiss: (id: number) => void;
};

let nextId = 1;

export const useToastStore = create<ToastState>()((set, get) => ({
  toast: null,
  show: (text, tone = "ok") => set({ toast: { id: nextId++, text, tone } }),
  dismiss: (id) => {
    if (get().toast?.id === id) set({ toast: null });
  },
}));

/** Callable outside React (mutation callbacks). */
export const showToast = (text: string, tone?: Toast["tone"]) => useToastStore.getState().show(text, tone);
