// Registers the mock backend when the app runs outside Tauri (plain browser, `npm run dev`).
// Imported dynamically from main.tsx behind `import.meta.env.DEV`, so it never reaches a production build.

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { createMockBackend } from "./backend";

export function installMocks(): void {
  const backend = createMockBackend();
  mockIPC((cmd, args) => backend.handle(cmd, args), { shouldMockEvents: true });
  window.setInterval(() => {
    for (const stats of backend.tick()) void emit("torrent://stats", stats);
  }, 1000);
  console.info("[yts-player] Running in the browser with the mock backend (src/mocks).");
}
