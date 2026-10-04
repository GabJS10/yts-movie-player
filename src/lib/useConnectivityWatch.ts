import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { getApiStatus } from "../api/tauri";
import { setOffline, useConnectivity } from "../store/connectivity";

/** While offline, how often the catalog servers are probed again. */
export const OFFLINE_PROBE_MS = 30_000;

/**
 * Leaves "Sin conexión" by itself: when the browser reports the link back, and every 30 s meanwhile,
 * it probes the catalog servers (get_api_status); if one answers, the failed queries run again.
 * Mounted once, in the app shell.
 */
export function useConnectivityWatch() {
  const qc = useQueryClient();
  const offline = useConnectivity((s) => s.offline);

  useEffect(() => {
    const probe = async () => {
      try {
        const servers = await getApiStatus();
        if (!servers.some((s) => s.ok)) return;
      } catch {
        return;
      }
      setOffline(false);
      void qc.refetchQueries({ predicate: (q) => q.state.status === "error" });
    };
    const onOnline = () => void probe();
    const onOffline = () => setOffline(true);
    window.addEventListener("online", onOnline);
    window.addEventListener("offline", onOffline);
    const timer = offline ? window.setInterval(() => void probe(), OFFLINE_PROBE_MS) : undefined;
    return () => {
      window.removeEventListener("online", onOnline);
      window.removeEventListener("offline", onOffline);
      window.clearInterval(timer);
    };
  }, [qc, offline]);
}
