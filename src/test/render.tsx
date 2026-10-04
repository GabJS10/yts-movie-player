import { QueryClientProvider } from "@tanstack/react-query";
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from "@tanstack/react-router";
import { render } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import type { ReactNode } from "react";
import { App } from "../App";
import { createMockBackend, type MockBackend } from "../mocks/backend";
import { createQueryClient } from "../lib/queryClient";
import { createAppRouter } from "../router";

type Options = {
  /** Seed the mock backend before the first render (e.g. empty Mi lista, other settings). */
  before?: (backend: MockBackend) => void;
  /** Commands that reject with this AppError instead of reaching the mock. */
  fail?: Partial<Record<string, { code: string; message: string }>>;
};

/** Mock backend behind IPC; returns it so tests can inspect calls. */
export function installBackend(options: Options = {}) {
  const backend = createMockBackend();
  options.before?.(backend);
  const calls: { cmd: string; args: unknown }[] = [];
  const fail = { ...options.fail };
  mockIPC(
    (cmd, args) => {
      calls.push({ cmd, args });
      const error = fail[cmd];
      if (error) return Promise.reject(error);
      return backend.handle(cmd, args);
    },
    { shouldMockEvents: true },
  );
  // After `before` (seeding is not an event) and once IPC exists; a late emit after teardown is dropped.
  backend.onEvent((event, payload) => void emit(event, payload).catch(() => undefined));
  /** Make `cmd` fail from now on (or succeed again with `null`). */
  const setFailure = (cmd: string, error: { code: string; message: string } | null) => {
    if (error) fail[cmd] = error;
    else delete fail[cmd];
  };
  return { backend, calls, setFailure };
}

export function testQueryClient() {
  const qc = createQueryClient();
  qc.setDefaultOptions({ queries: { ...qc.getDefaultOptions().queries, retry: false } });
  return qc;
}

/** Renders the full app at `path`, with the mock backend behind IPC. */
export async function renderApp(path = "/", options: Options = {}) {
  const ipc = installBackend(options);
  const queryClient = testQueryClient();
  const router = createAppRouter(queryClient, createMemoryHistory({ initialEntries: [path] }));
  await router.load();
  const utils = render(<App queryClient={queryClient} router={router} />);
  return { ...utils, router, queryClient, ...ipc };
}

/** Renders a single component inside a router (for <Link>) and a QueryClient. */
export async function renderWithProviders(ui: ReactNode, options: Options = {}) {
  const ipc = installBackend(options);
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
    defaultNotFoundComponent: () => null,
  });
  await router.load();
  const utils = render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { ...utils, router, queryClient, ...ipc };
}
