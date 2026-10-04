import { describe, expect, it } from "vitest";
import type { Torrent } from "../api/types";
import { pickDefaultTorrent, signalLevel, sortForDisplay, torrentNotes } from "./versions";

const t = (over: Partial<Torrent>): Torrent => ({
  infohash: Math.random().toString(16).slice(2).padEnd(40, "0"),
  quality: "1080p",
  source: "bluray",
  videoCodec: "x264",
  bitDepth: 8,
  audioChannels: "2.0",
  sizeBytes: 2e9,
  seeds: 50,
  peers: 10,
  uploadedAt: null,
  ...over,
});

describe("pickDefaultTorrent", () => {
  it("prefers the best x264 with seeds over a better x265", () => {
    const x265 = t({ quality: "2160p", videoCodec: "x265", seeds: 100 });
    const p1080 = t({ quality: "1080p", seeds: 80 });
    const p720 = t({ quality: "720p", seeds: 100 });
    expect(pickDefaultTorrent([p720, x265, p1080])).toBe(p1080);
  });

  it("skips x264 versions without seeds", () => {
    const dead1080 = t({ quality: "1080p", seeds: 0 });
    const p720 = t({ quality: "720p", seeds: 4 });
    expect(pickDefaultTorrent([dead1080, p720])).toBe(p720);
  });

  it("breaks ties by seeds", () => {
    const a = t({ source: "web", seeds: 12 });
    const b = t({ source: "bluray", seeds: 60 });
    expect(pickDefaultTorrent([a, b])).toBe(b);
  });

  it("falls back to x265 with seeds, then to anything; 3D only as last resort", () => {
    const x265 = t({ quality: "2160p", videoCodec: "x265", seeds: 3 });
    const dead = t({ quality: "1080p", seeds: 0 });
    const threeD = t({ quality: "3D", seeds: 90 });
    expect(pickDefaultTorrent([dead, x265, threeD])).toBe(x265);
    expect(pickDefaultTorrent([dead, threeD])).toBe(dead);
    expect(pickDefaultTorrent([threeD])).toBe(threeD);
    expect(pickDefaultTorrent([])).toBeNull();
  });
});

describe("pickDefaultTorrent with settings", () => {
  const p720 = t({ quality: "720p", seeds: 40 });
  const p1080 = t({ quality: "1080p", seeds: 80 });
  const x1080 = t({ quality: "1080p", videoCodec: "x265", seeds: 100 });
  const x2160 = t({ quality: "2160p", videoCodec: "x265", seeds: 100 });
  const all = [p720, p1080, x1080, x2160];

  it("picks the preferred quality, x264 first", () => {
    expect(pickDefaultTorrent(all, { preferredQuality: "1080p", preferX264: true })).toBe(p1080);
    expect(pickDefaultTorrent(all, { preferredQuality: "720p", preferX264: true })).toBe(p720);
  });

  it("without the preferred quality in x264, falls to the closest one below it", () => {
    expect(pickDefaultTorrent(all, { preferredQuality: "2160p", preferX264: true })).toBe(p1080);
    const only720and2160 = [p720, t({ quality: "2160p", seeds: 9 })];
    expect(pickDefaultTorrent(only720and2160, { preferredQuality: "1080p", preferX264: true })).toBe(
      only720and2160[0],
    );
    // Nothing at or below: the closest above.
    const high = [t({ quality: "2160p", seeds: 5 }), p1080];
    expect(pickDefaultTorrent(high, { preferredQuality: "720p", preferX264: true })).toBe(p1080);
  });

  it("with preferX264 off, the codec doesn't matter: quality, then seeds", () => {
    expect(pickDefaultTorrent(all, { preferredQuality: "2160p", preferX264: false })).toBe(x2160);
    expect(pickDefaultTorrent(all, { preferredQuality: "1080p", preferX264: false })).toBe(x1080);
  });

  it("still skips dead versions and 3D", () => {
    const dead = t({ quality: "1080p", seeds: 0 });
    const threeD = t({ quality: "3D", seeds: 90 });
    expect(pickDefaultTorrent([dead, threeD, p720], { preferredQuality: "1080p", preferX264: true })).toBe(
      p720,
    );
  });
});

describe("signalLevel", () => {
  it.each([
    [0, 0],
    [1, 1],
    [4, 1],
    [5, 2],
    [14, 2],
    [15, 3],
    [39, 3],
    [40, 4],
    [89, 4],
    [90, 5],
    [100, 5],
  ])("%i seeds → %i bars", (seeds, bars) => expect(signalLevel(seeds)).toBe(bars));
});

describe("torrentNotes / sortForDisplay", () => {
  it("flags HEVC, 3D and low or no seeds", () => {
    expect(torrentNotes(t({ videoCodec: "x265" })).map((n) => n.kind)).toEqual(["hevc"]);
    expect(torrentNotes(t({ seeds: 7 })).map((n) => n.text)).toEqual(["Pocos seeds: arranque lento"]);
    expect(torrentNotes(t({ seeds: 0 }))[0]?.kind).toBe("no-seeds");
    expect(torrentNotes(t({ quality: "3D" }))[0]?.kind).toBe("3d");
    expect(torrentNotes(t({}))).toEqual([]);
  });

  it("orders by quality, x264 first, 3D last", () => {
    const list = [
      t({ quality: "3D" }),
      t({ quality: "2160p", videoCodec: "x265" }),
      t({ quality: "1080p", videoCodec: "x265" }),
      t({ quality: "720p" }),
      t({ quality: "1080p" }),
    ];
    expect(sortForDisplay(list).map((x) => `${x.quality}/${x.videoCodec}`)).toEqual([
      "720p/x264",
      "1080p/x264",
      "1080p/x265",
      "2160p/x265",
      "3D/x264",
    ]);
  });
});
