// Registers the mock backend when the app runs outside Tauri (plain browser, `npm run dev`).
// Imported dynamically from main.tsx behind `import.meta.env.DEV`, so it never reaches a production build.
//
// Exercising states by hand:
// - Search "!api" or "!net" → api_unavailable / network errors.
// - /movie/1 → not_found.
// - localStorage.setItem("mock:latency", "2000") → slower responses (skeletons); default 150–450 ms.
// - localStorage.setItem("mock:fail", "add_favorite,update_settings") → those commands fail with `db`
//   (optimistic rollback, error toasts).
// - Subtitles: the mock starts with an OpenSubtitles key, so they load by themselves. In Ajustes, set the
//   key to "invalid" (subtitles_auth) or "quota" (quota exhausted), or clear it ("no key" notice).
//   The Shawshank Redemption has no Spanish subtitles (English fallback offer).
// - localStorage.setItem("mock:offline", "1") → no network: the catalog fails with `network`, downloaded
//   movies open from their saved copy and play from the library (reload to apply).

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { createMockBackend } from "./backend";

function failing(cmd: string): boolean {
  try {
    return (localStorage.getItem("mock:fail") ?? "").split(",").includes(cmd);
  } catch {
    return false;
  }
}

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
  backend.onDownloadChanged((payload) => void emit("download://changed", payload));
  try {
    backend.setOffline(localStorage.getItem("mock:offline") === "1");
  } catch {
    // storage unavailable: online
  }
  mockIPC(
    (cmd, args) =>
      new Promise((resolve, reject) => {
        window.setTimeout(() => {
          try {
            if (failing(cmd)) throw { code: "db", message: `mock: ${cmd} forced to fail` };
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
