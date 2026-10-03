import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { isTauri } from "./lib/runtime";
import { createQueryClient } from "./lib/queryClient";
import { createAppRouter } from "./router";
import { startSwarmListener } from "./store/swarm";
import "./styles/app.css";

async function bootstrap() {
  // Outside Tauri (plain browser during development) the IPC is served by an in-memory mock.
  // The dynamic import sits behind import.meta.env.DEV, so production builds drop it entirely.
  if (import.meta.env.DEV && !isTauri()) {
    const { installMocks } = await import("./mocks/setup");
    installMocks();
  }

  void startSwarmListener().catch((err: unknown) => console.error("torrent://stats listener failed", err));

  const queryClient = createQueryClient();
  const router = createAppRouter(queryClient);
  const root = document.getElementById("root");
  if (!root) throw new Error("#root element missing in index.html");
  createRoot(root).render(
    <StrictMode>
      <App queryClient={queryClient} router={router} />
    </StrictMode>,
  );
}

void bootstrap();
