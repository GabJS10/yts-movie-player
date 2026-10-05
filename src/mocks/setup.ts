// Registers the mock backend when the app runs outside Tauri (plain browser, `npm run dev`).
// Imported dynamically from main.tsx behind `import.meta.env.DEV`, so it never reaches a production build.
//
// Exercising states by hand:
// - Search "!api" or "!net" → api_unavailable / network errors.
// - /movie/1 → not_found.
// - Scenario knobs (src/mocks/options.ts; all documented in src/testids.md), from the URL or localStorage:
//   mock:latency, mock:fail (e.g. "add_favorite,start_stream=no_peers"), mock:offline, mock:tick, mock:settings.
// - Subtitles: the mock starts with an OpenSubtitles key, so they load by themselves. In Ajustes, set the
//   key to "invalid" (subtitles_auth) or "quota" (quota exhausted), or clear it ("no key" notice).
//   The Shawshank Redemption has no Spanish subtitles (English fallback offer).
// - mock:offline=1 → no network: the catalog fails with `network`, downloaded movies open from their saved
//   copy and play from the library.
// - Ajustes › Almacenamiento: "Cambiar…" picks /media/usb/Películas (another disk), which offers to move the
//   existing downloads (progress dialog, Cancelar). Folder paths with "sin-permiso" are refused, with
//   "desconectado" show as unavailable, and with "lleno" the downloads over 2 GB fail to move.

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { createMockBackend, setMockTrailerMode, setMockUpdate, type MockBackend } from "./backend";
import { readMockOptions, stripMockParams } from "./options";

declare global {
  interface Window {
    /** Dev/e2e handle on the mock backend (never in production builds). */
    __ytsMock?: { backend: MockBackend; tick: () => void };
  }
}

export function installMocks(): void {
  const options = readMockOptions(window.location.search, window.localStorage ?? null);
  const clean = stripMockParams(window.location.href);
  if (clean) window.history.replaceState(window.history.state, "", clean);

  setMockTrailerMode(options.trailer);
  setMockUpdate(
    options.update
      ? {
          version: options.update,
          url: `https://github.com/GabJS10/yts-movie-player/releases/tag/v${options.update}`,
          publishedAt: new Date().toISOString(),
        }
      : null,
  );
  const backend = createMockBackend({ platform: options.platform });
  backend.onEvent((event, payload) => void emit(event, payload));
  backend.setOffline(options.offline);
  if (options.settings) backend.handle("update_settings", { patch: options.settings });

  const latency = () => options.latencyMs ?? 150 + Math.random() * 300;
  mockIPC(
    (cmd, args) =>
      new Promise((resolve, reject) => {
        window.setTimeout(() => {
          try {
            const code = options.fail.get(cmd);
            if (code) throw { code, message: `mock: ${cmd} forced to fail` };
            resolve(backend.handle(cmd, args));
          } catch (err) {
            reject(err);
          }
        }, latency());
      }),
    { shouldMockEvents: true },
  );
  const tick = () => {
    for (const stats of backend.tick()) void emit("torrent://stats", stats);
  };
  if (options.tickMs > 0) window.setInterval(tick, options.tickMs);
  window.__ytsMock = { backend, tick };
  console.info("[yts-player] Running in the browser with the mock backend (src/mocks).");
}
