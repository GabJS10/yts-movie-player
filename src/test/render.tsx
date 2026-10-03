import { createMemoryHistory } from "@tanstack/react-router";
import { render } from "@testing-library/react";
import { mockIPC } from "@tauri-apps/api/mocks";
import { App } from "../App";
import { createMockBackend } from "../mocks/backend";
import { createQueryClient } from "../lib/queryClient";
import { createAppRouter } from "../router";

/** Renders the full app at `path`, with the mock backend behind IPC. */
export async function renderApp(path = "/") {
  const backend = createMockBackend();
  mockIPC((cmd, args) => backend.handle(cmd, args), { shouldMockEvents: true });
  const queryClient = createQueryClient();
  const router = createAppRouter(queryClient, createMemoryHistory({ initialEntries: [path] }));
  await router.load();
  const utils = render(<App queryClient={queryClient} router={router} />);
  return { ...utils, router, queryClient, backend };
}
