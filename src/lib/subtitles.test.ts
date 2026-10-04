import { describe, expect, it } from "vitest";
import {
  activeCues,
  cueSpans,
  formatDelay,
  formatResetTime,
  isSubtitleFile,
  parseTimestamp,
  parseVtt,
  stepDelay,
} from "./subtitles";

describe("parseVtt", () => {
  it("parses cues with ids, settings, multi-line text, CRLF and a BOM; skips NOTE blocks", () => {
    const vtt =
      "﻿WEBVTT - Interstellar\r\n\r\nNOTE made by hand\r\n\r\n1\r\n00:00:01.000 --> 00:00:03.500 line:90%\r\n<i>Hola</i>\r\nmundo\r\n\r\n" +
      "01:02:03.250 --> 01:02:05.000\r\nTarde\r\n\r\n\r\n00:10.000 --> 00:12.000\r\nCorto\r\n";
    expect(parseVtt(vtt)).toEqual([
      { start: 1, end: 3.5, text: "<i>Hola</i>\nmundo" },
      { start: 10, end: 12, text: "Corto" },
      { start: 3723.25, end: 3725, text: "Tarde" },
    ]);
  });

  it("tolerates SRT-style commas and drops cues without text", () => {
    expect(parseVtt("1\n00:00:01,500 --> 00:00:02,000\nUno\n\n2\n00:00:03,000 --> 00:00:04,000\n\n")).toEqual(
      [{ start: 1.5, end: 2, text: "Uno" }],
    );
  });

  it("parseTimestamp handles hours and short forms", () => {
    expect(parseTimestamp("01:00:00.5")).toBe(3600.5);
    expect(parseTimestamp("02:03.004")).toBeCloseTo(123.004);
  });
});

describe("activeCues", () => {
  const cues = parseVtt(
    "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nA\n\n00:00:03.000 --> 00:00:05.000\nB\n\n00:00:04.000 --> 00:00:06.000\nC\n",
  );
  const at = (t: number, d = 0) => activeCues(cues, t, d).map((c) => c.text);

  it("shows the cue whose span holds the time (end excluded)", () => {
    expect(at(1)).toEqual(["A"]);
    expect(at(2)).toEqual([]);
    expect(at(3)).toEqual(["B"]);
  });

  it("shows overlapping cues together, in order", () => {
    expect(at(4.5)).toEqual(["B", "C"]);
  });

  it("a positive delay shows them later, a negative one earlier", () => {
    expect(at(3.2, 0.5)).toEqual([]); // 2.7 in cue time
    expect(at(3.6, 0.5)).toEqual(["B"]);
    expect(at(2.6, -0.5)).toEqual(["B"]); // 3.1
  });
});

describe("cueSpans", () => {
  it("keeps i/b/u, drops other tags and decodes entities; never HTML", () => {
    expect(
      cueSpans("<v Cooper>Hola <i>mundo</i> &amp; <b><u>más</u></b><c.x>!</c> <script>x</script>"),
    ).toEqual([
      { text: "Hola ", i: false, b: false, u: false },
      { text: "mundo", i: true, b: false, u: false },
      { text: " & ", i: false, b: false, u: false },
      { text: "más", i: false, b: true, u: true },
      { text: "!", i: false, b: false, u: false },
      { text: " ", i: false, b: false, u: false },
      { text: "x", i: false, b: false, u: false },
    ]);
  });
});

describe("delay", () => {
  it("steps by tenths without drifting", () => {
    let d = 0;
    for (let i = 0; i < 3; i++) d = stepDelay(d, 1);
    expect(d).toBe(0.3);
    expect(stepDelay(-0.1, 1)).toBe(0);
  });
  it("formats with sign and comma", () => {
    expect(formatDelay(0)).toBe("0,0 s");
    expect(formatDelay(0.3)).toBe("+0,3 s");
    expect(formatDelay(-1.2)).toBe("−1,2 s");
  });
});

describe("misc", () => {
  it("formats the quota reset in local HH:MM", () => {
    const d = new Date(2026, 9, 4, 2, 5);
    expect(formatResetTime(d.toISOString())).toBe("02:05");
    expect(formatResetTime(null)).toBeNull();
    expect(formatResetTime("nope")).toBeNull();
  });
  it("accepts .srt and .vtt only", () => {
    expect(isSubtitleFile("/a/b/Movie.ES.SRT")).toBe(true);
    expect(isSubtitleFile("x.vtt")).toBe(true);
    expect(isSubtitleFile("x.ass")).toBe(false);
  });
});
