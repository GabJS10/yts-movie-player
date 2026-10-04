// Pure subtitle logic: WebVTT parsing, the active cue with the user's delay, cue markup and copy.
// The player draws cues itself (see SubtitleLayer), so nothing here depends on how WebKitGTK renders tracks.

export type Cue = { start: number; end: number; text: string };

const TIMING = /^\s*((?:\d+:)?\d{1,2}:\d{2}[.,]\d{1,3})\s+-->\s+((?:\d+:)?\d{1,2}:\d{2}[.,]\d{1,3})/;

/** "01:02:03.456" / "02:03.456" (comma tolerated) → seconds. */
export function parseTimestamp(ts: string): number {
  const parts = ts.replace(",", ".").split(":").map(Number);
  return parts.reduce((acc, n) => acc * 60 + n, 0);
}

/**
 * WebVTT → cues sorted by start. Tolerant: optional header, CRLF, cue ids, NOTE/STYLE blocks, and
 * SRT-style commas (the backend already converts SRT, this is a safety net).
 */
export function parseVtt(text: string): Cue[] {
  const cues: Cue[] = [];
  const blocks = text
    .replace(/^\uFEFF/, "")
    .replace(/\r\n?/g, "\n")
    .split(/\n{2,}/);
  for (const block of blocks) {
    const lines = block.split("\n");
    const i = lines.findIndex((l) => TIMING.test(l));
    if (i < 0) continue;
    const m = TIMING.exec(lines[i] ?? "");
    if (!m?.[1] || !m[2]) continue;
    const body = lines
      .slice(i + 1)
      .join("\n")
      .trim();
    if (!body) continue;
    cues.push({ start: parseTimestamp(m[1]), end: parseTimestamp(m[2]), text: body });
  }
  return cues.sort((a, b) => a.start - b.start);
}

/**
 * Cues showing at `time` with `delay` applied: a positive delay shows subtitles later
 * (cue.start ≤ time − delay < cue.end). Overlapping cues all show, in start order.
 */
export function activeCues(cues: readonly Cue[], time: number, delay: number): Cue[] {
  const t = time - delay;
  // Binary search for the first cue starting after t; only earlier ones can be active.
  let lo = 0;
  let hi = cues.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if ((cues[mid]?.start ?? 0) <= t) lo = mid + 1;
    else hi = mid;
  }
  const out: Cue[] = [];
  // Subtitle cues are short; look back a bounded window for long overlapping ones.
  for (let i = lo - 1; i >= 0 && i >= lo - 8; i--) {
    const c = cues[i];
    if (c && t < c.end) out.unshift(c);
  }
  return out;
}

export type CueSpan = { text: string; i: boolean; b: boolean; u: boolean };

/**
 * Cue text → styled spans. Keeps <i>, <b>, <u> (subtitles use them for songs, off-screen voices);
 * drops any other tag (<c.x>, <v Name>, <font>) and decodes the few entities VTT allows. Never HTML.
 */
export function cueSpans(text: string): CueSpan[] {
  const out: CueSpan[] = [];
  const state = { i: 0, b: 0, u: 0 };
  const decode = (s: string) =>
    s
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">")
      .replace(/&nbsp;/g, " ")
      .replace(/&amp;/g, "&");
  for (const part of text.split(/(<[^>]*>)/)) {
    if (!part) continue;
    const tag = /^<(\/?)([a-z]+)[^>]*>$/i.exec(part);
    if (tag) {
      const name = (tag[2] ?? "").toLowerCase();
      if (name === "i" || name === "b" || name === "u")
        state[name] = Math.max(0, state[name] + (tag[1] ? -1 : 1));
      continue;
    }
    if (part.startsWith("<")) continue; // timestamp tags like <00:01.000>
    out.push({ text: decode(part), i: state.i > 0, b: state.b > 0, u: state.u > 0 });
  }
  return out;
}

export const DELAY_STEP = 0.1;

/** Rounded to tenths so repeated ±0,1 never drifts (0.1 + 0.2). */
export const stepDelay = (delay: number, dir: 1 | -1) => Math.round((delay + dir * DELAY_STEP) * 10) / 10;

const nf1 = new Intl.NumberFormat("es-ES", { minimumFractionDigits: 1, maximumFractionDigits: 1 });

/** "+0,3 s" / "−1,2 s" / "0,0 s". */
export function formatDelay(delay: number): string {
  if (Math.abs(delay) < 0.05) return "0,0 s";
  return `${delay > 0 ? "+" : "−"}${nf1.format(Math.abs(delay))} s`;
}

/** Quota renewal in local time, "HH:MM". */
export function formatResetTime(iso: string | null): string | null {
  if (!iso) return null;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return null;
  return new Intl.DateTimeFormat("es-ES", { hour: "2-digit", minute: "2-digit" }).format(d);
}

/** Subtitle languages offered in Ajustes and the player (ISO 639-1 → name in its own language). */
export const SUBTITLE_LANGS: { code: string; label: string }[] = [
  { code: "es", label: "Español" },
  { code: "en", label: "English" },
  { code: "pt", label: "Português" },
  { code: "fr", label: "Français" },
  { code: "it", label: "Italiano" },
  { code: "de", label: "Deutsch" },
];

export const langLabel = (code: string | null) =>
  (code && SUBTITLE_LANGS.find((l) => l.code === code)?.label) ?? code?.toUpperCase() ?? "Subtítulos";

export const FALLBACK_LANG = "en";

/** Files the player accepts by drag & drop or "Cargar archivo…". */
export const isSubtitleFile = (path: string) => /\.(srt|vtt)$/i.test(path);

export const OPENSUBTITLES_KEYS_URL = "https://www.opensubtitles.com/en/consumers";
