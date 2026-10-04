import { describe, expect, it } from "vitest";
import type { Download } from "../api/types";
import {
  activeCount,
  downloadForMovie,
  formatEta,
  formatPercent,
  groupDownloads,
  roundPercent,
} from "./downloads";
import { parseOptionalInt } from "./settings";

const dl = (over: Partial<Download> & { id?: number }): Download => ({
  infohash: over.infohash ?? "a".repeat(40),
  movie: { id: over.id ?? 1 } as Download["movie"],
  quality: "1080p",
  videoCodec: "x264",
  state: "active",
  progress: 0.5,
  sizeBytes: 100,
  downloadedBytes: 50,
  downSpeedBps: 0,
  peers: 0,
  etaS: null,
  path: null,
  error: null,
  addedAt: "2026-10-04T10:00:00Z",
  ...over,
});

describe("downloads helpers", () => {
  it("counts queued, active and stalled as active", () => {
    const list = (["queued", "active", "stalled", "paused", "done", "error"] as const).map((state, i) =>
      dl({ state, infohash: String(i).repeat(40) }),
    );
    expect(activeCount(list)).toBe(3);
    expect(activeCount(undefined)).toBe(0);
  });

  it("picks a finished version of the movie first", () => {
    const a = dl({ id: 7, infohash: "a".repeat(40), state: "active" });
    const b = dl({ id: 7, infohash: "b".repeat(40), state: "done" });
    expect(downloadForMovie([a, b], 7)).toBe(b);
    expect(downloadForMovie([a], 7)).toBe(a);
    expect(downloadForMovie([a, b], 8)).toBeNull();
  });

  it("groups newest first: in progress (errors included) and library", () => {
    const old = dl({ infohash: "1".repeat(40), addedAt: "2026-10-01T00:00:00Z", state: "error" });
    const recent = dl({ infohash: "2".repeat(40), addedAt: "2026-10-03T00:00:00Z" });
    const done = dl({ infohash: "3".repeat(40), state: "done" });
    expect(groupDownloads([old, done, recent])).toEqual({ inProgress: [recent, old], library: [done] });
  });

  it("formats percentages without reaching 100 % early", () => {
    expect(formatPercent(0.634)).toBe("63,4 %");
    expect(formatPercent(0.99999)).toBe("99,9 %");
    expect(formatPercent(1)).toBe("100,0 %");
    expect(roundPercent(0.456)).toBe("45 %");
    expect(roundPercent(0.999)).toBe("99 %");
    expect(roundPercent(1)).toBe("100 %");
  });

  it("formats the ETA", () => {
    expect(formatEta(240)).toBe("quedan 4 min");
    expect(formatEta(3840)).toBe("quedan 1 h 4 min");
    expect(formatEta(20)).toBe("menos de un minuto");
  });
});

describe("parseOptionalInt (Ajustes › Torrent)", () => {
  it("empty is null; integers within range pass; anything else explains itself", () => {
    expect(parseOptionalInt("  ", 1, 10)).toBeNull();
    expect(parseOptionalInt("7", 1, 10)).toBe(7);
    expect(parseOptionalInt("7.5", 1, 10)).toBe("Escribe un número entero o déjalo vacío.");
    expect(parseOptionalInt("-3", 1, 10)).toBe("Escribe un número entero o déjalo vacío.");
    expect(parseOptionalInt("80", 1024, 65535)).toBe("Entre 1024 y 65535.");
  });
});
