// README screenshots from the real app with the real YTS catalog (needs internet).
// Usage (from e2e/, after scripts/build-app.sh): xvfb-run -a -s "-screen 0 1440x900x24" node app/screenshots.mjs
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { remote } from "webdriverio";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const out = path.join(root, "docs/screenshots");
mkdirSync(out, { recursive: true });
process.env.XDG_DATA_HOME = mkdtempSync(path.join(os.tmpdir(), "yts-shots-"));
process.env.XDG_STATE_HOME = process.env.XDG_DATA_HOME;

const driver = spawn(path.join(os.homedir(), ".cargo/bin/tauri-driver"), [], { stdio: "inherit" });
await new Promise((r) => setTimeout(r, 1500));
const browser = await remote({
  hostname: "127.0.0.1",
  port: 4444,
  logLevel: "warn",
  capabilities: {
    "tauri:options": { application: path.join(root, "src-tauri/target/e2e/debug/yts-player") },
    "wdio:enforceWebDriverClassic": true,
  },
});
const go = (p) =>
  browser.execute((u) => {
    window.history.pushState({}, "", u);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, p);
const settle = (ms = 4000) => browser.pause(ms);
try {
  await browser.setWindowSize(1440, 900);
  await browser.$("[data-testid=hero]").waitForDisplayed({ timeout: 60000 });
  await settle(6000);
  await browser.saveScreenshot(path.join(out, "inicio.png"));

  const id = await browser.$("[data-testid=movie-card]").getAttribute("data-movie-id");
  await go(`/movie/${id}`);
  await browser.$("[data-testid=movie-title]").waitForDisplayed({ timeout: 30000 });
  await settle();
  await browser.saveScreenshot(path.join(out, "ficha.png"));

  await go("/search");
  await browser.$("[data-testid=search-input]").setValue("matrix");
  await settle(5000);
  await browser.saveScreenshot(path.join(out, "buscar.png"));

  await go(`/movie/${id}`);
  await browser.$("[data-testid=movie-play]").waitForDisplayed({ timeout: 30000 });
  await browser.$("[data-testid=movie-play]").click();
  await browser.$("[data-testid=video]").waitForExist({ timeout: 60000 });
  await browser
    .waitUntil(
      async () => (await browser.execute(() => document.querySelector("video")?.currentTime ?? 0)) > 20,
      { timeout: 180000, interval: 1000 },
    )
    .catch(() => {});
  await browser.execute(() => {
    const v = document.querySelector("video");
    v?.pause();
    document
      .querySelector("[data-testid=video]")
      ?.dispatchEvent(new MouseEvent("mousemove", { bubbles: true }));
  });
  await settle(1500);
  await browser.saveScreenshot(path.join(out, "reproductor.png"));
} finally {
  await browser.deleteSession();
  driver.kill();
}
console.log("screenshots in", out);
