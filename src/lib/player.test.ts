import { describe, expect, it } from "vitest";
import type { StreamSession, TorrentStats } from "../api/types";
import {
  availableToSpans,
  formatClock,
  initialPlayerState,
  isBufferReady,
  isCodecError,
  parsePieceMap,
  playerReducer,
  scrubLayers,
  shortcutAction,
  type PlayerEvent,
  type PlayerState,
} from "./player";

const MB = 1024 * 1024;
const session: StreamSession = {
  infohash: "a".repeat(40),
  movieId: 1,
  streamUrl: "http://127.0.0.1:1/stream/a/0",
  fileName: "m.mp4",
  fileSizeBytes: 2000 * MB,
  videoCodec: "x264",
  likelyPlayable: true,
  bufferTargetBytes: 8 * MB,
  resumeAtS: null,
  source: "network",
};
const stats = (over: Partial<TorrentStats>): TorrentStats => ({
  infohash: session.infohash,
  phase: "buffering",
  peers: 10,
  seeds: 5,
  downSpeedBps: MB,
  upSpeedBps: 0,
  progress: 0,
  downloadedBytes: 0,
  bufferedAheadBytes: 0,
  availableRanges: [],
  pieceMap: null,
  pieceMapWindow: null,
  ...over,
});
const run = (events: PlayerEvent[], from: PlayerState = initialPlayerState) =>
  events.reduce(playerReducer, from);

describe("playerReducer", () => {
  it("connecting → buffering → playing → waiting → playing", () => {
    let s = run([
      { type: "session", session },
      { type: "stats", stats: stats({ phase: "connecting" }) },
    ]);
    expect(s.status).toBe("buffering");
    s = run([{ type: "stats", stats: stats({ bufferedAheadBytes: 4 * MB }) }], s);
    expect(s.status).toBe("buffering");
    s = run([{ type: "stats", stats: stats({ bufferedAheadBytes: 8 * MB }) }], s);
    expect(s.status).toBe("playing");
    s = run([{ type: "video-waiting" }], s);
    expect(s.status).toBe("waiting");
    s = run([{ type: "video-playing" }], s);
    expect(s.status).toBe("playing");
    s = run([{ type: "stats", stats: stats({ phase: "stalled" }) }], s);
    expect(s.status).toBe("waiting");
    s = run([{ type: "video-pause" }], s);
    expect(s.status).toBe("paused");
  });

  it("starts on phase ready, on Enter, and immediately from the library", () => {
    expect(
      run([
        { type: "session", session },
        { type: "stats", stats: stats({ phase: "ready" }) },
      ]).status,
    ).toBe("playing");
    expect(run([{ type: "session", session }, { type: "force-start" }]).status).toBe("playing");
    expect(run([{ type: "force-start" }]).status).toBe("starting"); // no session yet
    expect(run([{ type: "session", session: { ...session, source: "library" } }]).status).toBe("playing");
  });

  it("ignores video events before playback and keeps terminal states", () => {
    let s = run([{ type: "session", session }, { type: "video-playing" }, { type: "video-waiting" }]);
    expect(s.status).toBe("buffering");
    s = run([{ type: "force-start" }, { type: "codec-error" }, { type: "video-playing" }], s);
    expect(s.status).toBe("codec-error");
    const failed = run([{ type: "start-failed", error: { code: "no_peers", message: "x" } }]);
    expect(failed).toMatchObject({ status: "failed", error: { code: "no_peers" } });
    expect(run([{ type: "retry" }], failed)).toEqual(initialPlayerState);
  });

  it("isBufferReady uses bufferedAheadBytes against bufferTargetBytes", () => {
    expect(isBufferReady(session, null)).toBe(false);
    expect(isBufferReady(session, stats({ bufferedAheadBytes: 8 * MB - 1 }))).toBe(false);
    expect(isBufferReady(session, stats({ bufferedAheadBytes: 8 * MB }))).toBe(true);
    expect(isBufferReady(session, stats({ phase: "done" }))).toBe(true);
  });
});

describe("progress bar layers", () => {
  it("merges, sorts and clamps availableRanges", () => {
    expect(
      availableToSpans([
        [0.5, 0.6],
        [0, 0.1],
        [0.08, 0.2],
        [0.7, 0.7],
        [0.9, 1.4],
      ]),
    ).toEqual([
      { start: 0, end: 0.2 },
      { start: 0.5, end: 0.6 },
      { start: 0.9, end: 1 },
    ]);
  });

  it("maps currentTime, <video>.buffered and availableRanges into fractions", () => {
    const buffered = { length: 2, start: (i: number) => [0, 60][i]!, end: (i: number) => [30, 90][i]! };
    const layers = scrubLayers({ currentTime: 15, duration: 120, buffered, availableRanges: [[0.25, 0.5]] });
    expect(layers.played).toBe(0.125);
    expect(layers.buffered).toEqual([
      { start: 0, end: 0.25 },
      { start: 0.5, end: 0.75 },
    ]);
    expect(layers.available).toEqual([{ start: 0.25, end: 0.5 }]);
    expect(scrubLayers({ currentTime: 5, duration: NaN, buffered, availableRanges: null })).toEqual({
      played: 0,
      buffered: [],
      available: [],
    });
  });
});

describe("helpers", () => {
  it("parses a 200-cell piece map, padding missing data", () => {
    const cells = parsePieceMap("1123");
    expect(cells).toHaveLength(200);
    expect(cells.slice(0, 5)).toEqual(["ready", "ready", "priority", "arriving", "missing"]);
    expect(parsePieceMap(null).every((c) => c === "missing")).toBe(true);
  });

  it("maps keys to player actions", () => {
    expect([" ", "ArrowLeft", "ArrowRight", "f", "M", "Escape", "x"].map(shortcutAction)).toEqual([
      "toggle",
      "back10",
      "forward10",
      "fullscreen",
      "mute",
      "escape",
      null,
    ]);
  });

  it("detects codec errors and formats clocks", () => {
    expect([1, 2, 3, 4].map(isCodecError)).toEqual([false, false, true, true]);
    expect(formatClock(65)).toBe("1:05");
    expect(formatClock(3725)).toBe("1:02:05");
    expect(formatClock(NaN)).toBe("0:00");
  });
});
