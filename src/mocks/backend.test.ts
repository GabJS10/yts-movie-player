import { describe, expect, it } from "vitest";
import type { MovieDetail, StreamSession } from "../api/types";
import { createMockBackend } from "./backend";

describe("mock backend streams (IPC v0.5)", () => {
  it("goes connecting → buffering → ready, with static YTS seeds and no '3' cells", () => {
    const b = createMockBackend();
    const movie = b.handle("get_movie", { movieId: 1632 }) as MovieDetail;
    const t = movie.torrents.find((x) => x.quality === "1080p" && x.videoCodec === "x264")!;
    const session = b.handle("start_stream", { movieId: 1632, infohash: t.infohash }) as StreamSession;

    const phases: string[] = [];
    for (let i = 0; i < 12; i++) {
      const s = b.tick().find((x) => x.infohash === t.infohash)!;
      phases.push(s.phase);
      expect(s.seeds).toBe(t.seeds);
      expect(s.pieceMap).toHaveLength(200);
      expect(s.pieceMap).not.toContain("3");
    }
    const distinct = phases.filter((p, i) => p !== phases[i - 1]);
    expect(distinct).toEqual(["connecting", "buffering", "ready"]);
    expect(phases).not.toContain("metadata");
    expect(session.bufferTargetBytes).toBeGreaterThan(0);
  });

  it("samples the pieceMap over a 64 MB window: the 8 MB buffer fills ~25 cells", () => {
    const b = createMockBackend();
    const movie = b.handle("get_movie", { movieId: 1632 }) as MovieDetail;
    const t = movie.torrents.find((x) => x.quality === "1080p" && x.videoCodec === "x264")!;
    b.handle("start_stream", { movieId: 1632, infohash: t.infohash });
    let s = b.tick().find((x) => x.infohash === t.infohash)!;
    while (s.phase !== "ready") s = b.tick().find((x) => x.infohash === t.infohash)!;
    expect(s.pieceMapWindow).toEqual({ startByte: 0, endByte: 64 * 1024 * 1024 });
    const cellBytes = (64 * 1024 * 1024) / 200;
    const ready = [...s.pieceMap!].filter((c) => c === "1").length;
    expect(ready).toBe(Math.floor(s.bufferedAheadBytes / cellBytes));
    expect(ready).toBeGreaterThanOrEqual(24); // ≥ 8 MB
    expect([...s.pieceMap!].filter((c) => c === "2").length).toBe(Math.max(0, 100 - ready));
  });
});
