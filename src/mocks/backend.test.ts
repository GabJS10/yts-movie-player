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
});
