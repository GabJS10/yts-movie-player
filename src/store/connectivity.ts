import { create } from "zustand";

// "Sin conexión" mode. Every network call goes through Rust, so the UI learns about the network from
// the answers: a `network` error turns it on, a catalog answer turns it off. The browser's online/offline
// events only nudge it (WebKitGTK reports them, but YTS can be unreachable with the link up).

type ConnectivityState = {
  offline: boolean;
  setOffline: (offline: boolean) => void;
};

export const useConnectivity = create<ConnectivityState>()((set, get) => ({
  offline: false,
  setOffline: (offline) => {
    if (get().offline !== offline) set({ offline });
  },
}));

export const setOffline = (offline: boolean) => useConnectivity.getState().setOffline(offline);

export const resetConnectivity = () => useConnectivity.setState({ offline: false });
