// Real-app E2E: WebdriverIO → tauri-driver → WebKitWebDriver → YTS Player (debug build).
//
// Everything runs offline and isolated:
// - a local seeder (cargo example e2e_seeder) shares a short H.264 MP4,
// - a fake YTS API (fake-yts.mjs) points the first movie at that torrent,
// - the app gets its own XDG_DATA_HOME, so the user's data is never touched.
//
// Prerequisites: `npm run tauri build -- --debug --no-bundle` (from the repo root),
// `cargo install tauri-driver --locked`, WebKitWebDriver (apt: webkit2gtk-driver), ffmpeg.
import { spawn, execFileSync, type ChildProcess } from "node:child_process";
import { mkdtempSync, statSync, rmSync } from "node:fs";
import { createInterface } from "node:readline";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { startFakeYts } from "./fake-yts.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const application = path.join(root, "src-tauri/target/debug/yts-player");
const tauriDriverBin = path.join(os.homedir(), ".cargo/bin/tauri-driver");

let seeder: ChildProcess | undefined;
let tauriDriver: ChildProcess | undefined;
let fakeYts: { close: () => void } | undefined;
let dataHome: string | undefined;

type SeederInfo = { infohash: string; torrentPath: string; port: number };

function startSeeder(video: string): Promise<SeederInfo> {
  seeder = spawn("cargo", ["run", "--quiet", "--example", "e2e_seeder", "--", video], {
    cwd: path.join(root, "src-tauri"),
    stdio: ["ignore", "pipe", "inherit"],
  });
  const lines = createInterface({ input: seeder.stdout! });
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("e2e_seeder did not start in 300 s")), 300_000);
    lines.on("line", (line) => {
      try {
        const info = JSON.parse(line) as SeederInfo;
        clearTimeout(timer);
        resolve(info);
      } catch {
        // not the JSON line yet
      }
    });
    seeder!.on("exit", (code) => reject(new Error(`e2e_seeder exited with ${code}`)));
  });
}

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: ["./specs/**/*.e2e.ts"],
  maxInstances: 1,
  hostname: "127.0.0.1",
  port: 4444,
  capabilities: [
    {
      // @ts-expect-error tauri-driver specific capability
      "tauri:options": { application },
      browserName: "wry",
    },
  ],
  logLevel: "warn",
  waitforTimeout: 15_000,
  connectionRetryTimeout: 120_000,
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: { ui: "bdd", timeout: 180_000 },

  async onPrepare() {
    const video = execFileSync(path.join(root, "e2e/scripts/make-video.sh"), { encoding: "utf8" }).trim();
    const info = await startSeeder(video);
    const yts = await startFakeYts({
      seeder: { ...info, sizeBytes: statSync(video).size },
    });
    fakeYts = yts;
    dataHome = mkdtempSync(path.join(os.tmpdir(), "yts-e2e-"));
    // Inherited by the workers, then by tauri-driver and the app.
    process.env.XDG_DATA_HOME = dataHome;
    process.env.YTS_PLAYER_API_BASE_URLS = yts.baseUrl;
    process.env.YTS_PLAYER_NO_DHT = "1";
    process.env.E2E_PLAYABLE_MOVIE_ID = String(yts.playableId);
  },

  beforeSession() {
    tauriDriver = spawn(tauriDriverBin, [], { stdio: [null, process.stdout, process.stderr] });
  },

  afterSession() {
    tauriDriver?.kill();
  },

  onComplete() {
    fakeYts?.close();
    seeder?.kill("SIGTERM");
    if (dataHome) rmSync(dataHome, { recursive: true, force: true });
  },
};
