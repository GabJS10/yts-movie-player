// Linux only: deleted-but-open /var/tmp/WebKit-Media-* files held by the app's WebKitWebProcess
// (GStreamer on-disk buffering of the whole <video> file). Empty list = no leak.
import { existsSync, readdirSync, readFileSync, readlinkSync, realpathSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
// Mirrors wdio.conf.ts (which has side effects, so it is not imported).
const application = process.env.E2E_APP_BINARY ?? path.join(root, "src-tauri/target/e2e/debug/yts-player");

const tryRead = <T>(fn: () => T): T | undefined => {
  try {
    return fn();
  } catch {
    return undefined; // process/fd vanished or not readable
  }
};

export function openWebKitMediaFiles(): string[] {
  if (process.platform !== "linux" || !existsSync("/proc")) return [];
  const appPath = tryRead(() => realpathSync(application));
  if (!appPath) return [];

  const pids = readdirSync("/proc").filter((d) => /^\d+$/.test(d));
  const appPids = new Set(
    pids.filter((pid) => {
      const exe = tryRead(() => readlinkSync(`/proc/${pid}/exe`));
      return exe !== undefined && tryRead(() => realpathSync(exe)) === appPath;
    }),
  );

  const found: string[] = [];
  for (const pid of pids) {
    // comm is truncated by the kernel to 15 chars: "WebKitWebProces".
    if (tryRead(() => readFileSync(`/proc/${pid}/comm`, "utf8").trim()) !== "WebKitWebProces") continue;
    const stat = tryRead(() => readFileSync(`/proc/${pid}/stat`, "utf8"));
    // Fields after the last ")": state, ppid, ...
    const ppid = stat?.slice(stat.lastIndexOf(")") + 2).split(" ")[1];
    if (!ppid || !appPids.has(ppid)) continue;
    for (const fd of tryRead(() => readdirSync(`/proc/${pid}/fd`)) ?? []) {
      const target = tryRead(() => readlinkSync(`/proc/${pid}/fd/${fd}`));
      if (target?.includes("/WebKit-Media-")) found.push(target);
    }
  }
  return found;
}
