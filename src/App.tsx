import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import type { createAppRouter } from "./router";

type Props = { queryClient: QueryClient; router: ReturnType<typeof createAppRouter> };

export function App({ queryClient, router }: Props) {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}
