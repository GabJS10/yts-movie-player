// Folder paths come from the backend as the OS writes them: "/home/ana/…" on Linux, "C:\Users\ana\…"
// on Windows (or a UNC share, "\\nas\pelis\…"). Only display and comparison here; nothing touches disk.

/** Drive letter, UNC share, or backslashes with no forward slash: a Windows path. */
export const isWindowsPath = (path: string): boolean =>
  /^[A-Za-z]:([\\/]|$)/.test(path) || path.startsWith("\\\\") || (path.includes("\\") && !path.includes("/"));

export const pathSeparator = (path: string): "\\" | "/" => (isWindowsPath(path) ? "\\" : "/");

/** The root that can't be trimmed or cut: "/", "C:\", "\\nas\pelis\"; "" for a relative path. */
function rootOf(path: string): string {
  const drive = /^[A-Za-z]:[\\/]?/.exec(path);
  if (drive) return drive[0];
  const unc = /^\\\\[^\\/]+[\\/][^\\/]+[\\/]?/.exec(path);
  if (unc) return unc[0];
  return path.startsWith("/") ? "/" : "";
}

/** Without trailing separators, except the root itself ("/", "C:\"). */
export function trimTrailingSeparators(path: string): string {
  const root = rootOf(path);
  if (path.length <= root.length) return path;
  return root + path.slice(root.length).replace(/[\\/]+$/, "");
}

/**
 * Splits a path into the part that may be cut when it doesn't fit and the last folder or file, which
 * always shows: "C:\Users\ana\AppData\Local\yts-player\library" → ["C:\Users\ana\AppData\Local\yts-player\", "library"].
 * A bare root is all tail.
 */
export function splitPathTail(path: string): [head: string, tail: string] {
  const clean = trimTrailingSeparators(path);
  const root = rootOf(clean);
  const rest = clean.slice(root.length);
  const cut = Math.max(rest.lastIndexOf("/"), rest.lastIndexOf("\\"));
  if (!rest) return ["", clean];
  if (cut < 0) return [root, rest];
  return [root + rest.slice(0, cut + 1), rest.slice(cut + 1)];
}

/** Same folder? Ignores trailing separators; Windows paths also ignore case and "/" vs "\". */
export function samePath(a: string, b: string): boolean {
  const x = trimTrailingSeparators(a);
  const y = trimTrailingSeparators(b);
  if (x === y) return true;
  if (!isWindowsPath(x) || !isWindowsPath(y)) return false;
  const norm = (p: string) => trimTrailingSeparators(p.replace(/\//g, "\\")).toLowerCase();
  return norm(x) === norm(y);
}
