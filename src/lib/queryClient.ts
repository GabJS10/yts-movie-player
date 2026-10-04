import { MutationCache, QueryCache, QueryClient, type Query } from "@tanstack/react-query";
import type { AppError, MovieDetail } from "../api/types";
import { setOffline } from "../store/connectivity";

// Every query/mutation error is a contract AppError (src/api/tauri.ts normalizes rejections).
declare module "@tanstack/react-query" {
  interface Register {
    defaultError: AppError;
  }
}

/** Queries that only answer with the network up (the YTS catalog). */
const ONLINE_ONLY = new Set(["movies", "hero", "suggestions"]);

/** Connectivity follows the answers: see src/store/connectivity.ts. */
function track(query: Query<unknown, unknown, unknown, readonly unknown[]>) {
  const { status, data, error } = query.state;
  if (status === "error") {
    if ((error as AppError | null)?.code === "network") setOffline(true);
    return;
  }
  const kind = query.queryKey[0];
  if (kind === "movie") setOffline((data as MovieDetail | undefined)?.offline ?? false);
  else if (typeof kind === "string" && ONLINE_ONLY.has(kind)) setOffline(false);
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    queryCache: new QueryCache({
      onError: (_err, query) => track(query),
      onSuccess: (_data, query) => track(query),
    }),
    mutationCache: new MutationCache({
      onError: (err) => {
        if (err.code === "network") setOffline(true);
      },
    }),
    defaultOptions: {
      queries: {
        // The backend already caches the catalog (~30 min TTL); avoid refetch storms on focus.
        staleTime: 5 * 60_000,
        refetchOnWindowFocus: false,
        retry: 1,
        // Every query is local IPC: without network the backend still answers (downloads, saved copies,
        // or a `network` error), so never pause on navigator.onLine.
        networkMode: "always",
      },
      mutations: { networkMode: "always" },
    },
  });
}
