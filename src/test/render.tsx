import { QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from "@tanstack/react-router";
import { render } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import type { ReactNode } from "react";
import { App } from "../App";
import { createMockBackend } from "../mocks/backend";
import { createQueryClient } from "../lib/queryClient";
import { createAppRouter } from "../router";

/** Mock backend behind IPC; returns it so tests can inspect calls. */
export function installBackend() {
  const backend = createMockBackend();
  const calls: { cmd: string; args: unknown }[] = [];
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      return backend.handle(cmd, args);
    },
    { shouldMockEvents: true },
  );
  return { backend, calls };
}

export function testQueryClient() {
  const qc = createQueryClient();
  qc.setDefaultOptions({ queries: { ...qc.getDefaultOptions().queries, retry: false } });
  return qc;
}

/** Renders the full app at `path`, with the mock backend behind IPC. */
export async function renderApp(path = "/") {
  const ipc = installBackend();
  const queryClient = testQueryClient();
  const router = createAppRouter(queryClient, createMemoryHistory({ initialEntries: [path] }));
  await router.load();
  const utils = render(<App queryClient={queryClient} router={router} />);
  return { ...utils, router, queryClient, ...ipc };
}

/** Renders a single component inside a router (for <Link>) and a QueryClient. */
export async function renderWithProviders(ui: ReactNode) {
  const ipc = installBackend();
  const queryClient = testQueryClient();
  const rootRoute = createRootRoute({
    component: () => (
      <>
        {ui}
        <Outlet />
      </>
    ),
  });
  const router = createRouter({
    routeTree: rootRoute,
    history: createMemoryHistory({ initialEntries: ["/"] }),
  });
  await router.load();
  const utils = render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { ...utils, router, queryClient, ...ipc };
}
