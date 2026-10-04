import { test as base, expect, type Page } from "@playwright/test";

type MockOptions = {
  fail?: string; // "cmd[=code],…"
  offline?: boolean;
  tick?: number;
  settings?: Record<string, unknown>;
};

/** Opens a route with the mocked backend configured (latency 0 by default). */
export async function openMock(page: Page, path: string, opts: MockOptions = {}) {
  const params = new URLSearchParams({ "mock:latency": "0" });
  if (opts.fail) params.set("mock:fail", opts.fail);
  if (opts.offline) params.set("mock:offline", "1");
  if (opts.tick !== undefined) params.set("mock:tick", String(opts.tick));
  if (opts.settings) params.set("mock:settings", JSON.stringify(opts.settings));
  await page.goto(`${path}${path.includes("?") ? "&" : "?"}${params}`);
}

export const test = base.extend({
  page: async ({ page }, use) => {
    // The mock player's video lives on the internet; keep the tests offline.
    await page.route("https://test-videos.co.uk/**", (route) => route.abort());
    await use(page);
  },
});

export { expect };
