import { describe, expect, it } from "vitest";
import { parseFail, readMockOptions, stripMockParams } from "./options";

const storage = (values: Record<string, string>) => ({ getItem: (k: string) => values[k] ?? null });

describe("mock options (e2e scenarios)", () => {
  it("defaults: random latency, nothing failing, online, 1 s ticks", () => {
    expect(readMockOptions("", null)).toEqual({
      latencyMs: null,
      fail: new Map(),
      offline: false,
      tickMs: 1000,
      settings: null,
    });
  });

  it("reads localStorage, and the URL wins", () => {
    const o = readMockOptions(
      "?mock:latency=0&mock:fail=start_stream=no_peers,add_favorite",
      storage({
        "mock:latency": "2000",
        "mock:offline": "1",
        "mock:tick": "0",
        "mock:settings": '{"openSubtitlesApiKey":"quota"}',
      }),
    );
    expect(o.latencyMs).toBe(0);
    expect(o.fail).toEqual(
      new Map([
        ["start_stream", "no_peers"],
        ["add_favorite", "db"],
      ]),
    );
    expect(o.offline).toBe(true);
    expect(o.tickMs).toBe(0);
    expect(o.settings).toEqual({ openSubtitlesApiKey: "quota" });
  });

  it("ignores broken values", () => {
    const o = readMockOptions("?mock:latency=rápido&mock:settings={nope", null);
    expect(o.latencyMs).toBeNull();
    expect(o.settings).toBeNull();
    expect(parseFail(" , ")).toEqual(new Map());
  });

  it("strips only the mock params from the URL", () => {
    expect(stripMockParams("http://localhost:1420/search?query=matrix&mock:offline=1#x")).toBe(
      "/search?query=matrix#x",
    );
    expect(stripMockParams("http://localhost:1420/search?query=matrix")).toBeNull();
  });
});
