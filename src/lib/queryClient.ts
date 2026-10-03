import { QueryClient } from "@tanstack/react-query";
import type { AppError } from "../api/types";

// Every query/mutation error is a contract AppError (src/api/tauri.ts normalizes rejections).
declare module "@tanstack/react-query" {
  interface Register {
    defaultError: AppError;
  }
}

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // The backend already caches the catalog (~30 min TTL); avoid refetch storms on focus.
        staleTime: 5 * 60_000,
        refetchOnWindowFocus: false,
        retry: 1,
      },
    },
  });
}
