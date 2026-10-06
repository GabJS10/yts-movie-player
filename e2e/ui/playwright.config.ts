// UI E2E: Playwright (WebKit, the engine closest to WebKitGTK) against the Vite dev
// server, which installs the frontend's mocked backend outside Tauri (see src/testids.md).
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./specs",
  outputDir: "../test-results",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI
    ? [["list"], ["html", { open: "never", outputFolder: "../playwright-report" }]]
    : "list",
  use: {
    baseURL: "http://localhost:1420",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    viewport: { width: 1440, height: 900 },
    // The specs assert the Spanish copy; the app follows the browser language unless one is chosen.
    locale: "es-ES",
  },
  projects: [
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"], viewport: { width: 1440, height: 900 }, locale: "es-ES" },
    },
  ],
  webServer: {
    command: "npm run dev",
    cwd: "../..",
    url: "http://localhost:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
