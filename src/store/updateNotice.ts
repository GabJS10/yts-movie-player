import { create } from "zustand";

// "Nueva versión disponible": shown at most once per session; closing it hides it until the next start.
type UpdateNotice = { dismissed: boolean; dismiss: () => void };

export const useUpdateNotice = create<UpdateNotice>()((set) => ({
  dismissed: false,
  dismiss: () => set({ dismissed: true }),
}));

export const resetUpdateNotice = () => useUpdateNotice.setState({ dismissed: false });
