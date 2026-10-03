// Registers the mock backend when the app runs outside Tauri (plain browser, `npm run dev`).
// Imported dynamically from main.tsx behind `import.meta.env.DEV`, so it never reaches a production build.
//
// Exercising states by hand:
// - Search "!api" or "!net" → api_unavailable / network errors.
// - /movie/1 → not_found.
// - localStorage.setItem("mock:latency", "2000") → slower responses (skeletons); default 150–450 ms.

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { createMockBackend } from "./backend";

function latency(): number {
  try {
    const fixed = Number(localStorage.getItem("mock:latency"));
    if (fixed > 0) return fixed;
  } catch {
    // storage unavailable: fall through to the default
  }
  return 150 + Math.random() * 300;
}

export function installMocks(): void {
  const backend = createMockBackend();
  mockIPC(
    (cmd, args) =>
      new Promise((resolve, reject) => {
        window.setTimeout(() => {
          try {
            resolve(backend.handle(cmd, args));
          } catch (err) {
            reject(err);
          }
        }, latency());
      }),
    { shouldMockEvents: true },
  );
  window.setInterval(() => {
    for (const stats of backend.tick()) void emit("torrent://stats", stats);
  }, 1000);
  console.info("[yts-player] Running in the browser with the mock backend (src/mocks).");
}
