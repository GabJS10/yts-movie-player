import type { ErrorCode, SettingsPatch } from "../api/types";

// Mock scenario knobs for `npm run dev` and the Playwright e2e (e2e/ui). Each one can come from the page
// URL (`?mock:offline=1&mock:fail=add_favorite`, read once at start; wins) or from localStorage (same
// keys; persists across reloads). Documented in src/testids.md.

export type MockOptions = {
  /** Fixed IPC latency in ms (0 = answer on the next tick); null = random 150–450 ms. */
  latencyMs: number | null;
  /** Commands that reject, with the AppError code to reject with (default `db`). */
  fail: Map<string, ErrorCode>;
  /** No network from the start. */
  offline: boolean;
  /** Simulation step (stream buffer, downloads, moves) in ms; 0 = never on its own (drive it by hand). */
  tickMs: number;
  /** Settings patch applied before the first render (e.g. {"openSubtitlesApiKey":"quota"}). */
  settings: SettingsPatch | null;
};

export const MOCK_KEYS = ["mock:latency", "mock:fail", "mock:offline", "mock:tick", "mock:settings"] as const;

/** `add_favorite,start_stream=no_peers` → add_favorite: db, start_stream: no_peers. */
export function parseFail(raw: string | null): Map<string, ErrorCode> {
  const out = new Map<string, ErrorCode>();
  for (const item of (raw ?? "").split(",")) {
    const [cmd, code] = item.split("=").map((x) => x.trim());
    if (cmd) out.set(cmd, (code || "db") as ErrorCode);
  }
  return out;
}

export function readMockOptions(search: string, storage: Pick<Storage, "getItem"> | null): MockOptions {
  const url = new URLSearchParams(search);
  const get = (key: (typeof MOCK_KEYS)[number]): string | null => {
    if (url.has(key)) return url.get(key);
    try {
      return storage?.getItem(key) ?? null;
    } catch {
      return null;
    }
  };
  const num = (raw: string | null) =>
    raw !== null && raw.trim() !== "" && Number.isFinite(Number(raw)) ? Number(raw) : null;
  let settings: SettingsPatch | null = null;
  const rawSettings = get("mock:settings");
  if (rawSettings) {
    try {
      settings = JSON.parse(rawSettings) as SettingsPatch;
    } catch {
      console.warn("[yts-player] mock:settings is not valid JSON; ignored");
    }
  }
  const latency = num(get("mock:latency"));
  return {
    latencyMs: latency !== null && latency >= 0 ? latency : null,
    fail: parseFail(get("mock:fail")),
    offline: ["1", "true"].includes(get("mock:offline") ?? ""),
    tickMs: Math.max(0, num(get("mock:tick")) ?? 1000),
    settings,
  };
}

/** The `mock:*` params are for the mock only: drop them from the URL so the router never sees them. */
export function stripMockParams(href: string): string | null {
  const url = new URL(href);
  let changed = false;
  for (const key of MOCK_KEYS)
    if (url.searchParams.has(key)) {
      url.searchParams.delete(key);
      changed = true;
    }
  return changed ? `${url.pathname}${url.search}${url.hash}` : null;
}
