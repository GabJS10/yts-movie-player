import { describe, expect, it } from "vitest";
import { genreLabel, genres } from "./genres";

describe("genres", () => {
  it("values are lowercase API filters with unique Spanish labels", () => {
    const GENRES = genres();
    for (const g of GENRES) expect(g.value).toMatch(/^[a-z-]+$/);
    expect(new Set(GENRES.map((g) => g.label)).size).toBe(GENRES.length);
  });

  it("labels API-cased genres and filter values alike", () => {
    expect(genreLabel("Sci-Fi")).toBe("Ciencia ficción");
    expect(genreLabel("sci-fi")).toBe("Ciencia ficción");
    expect(genreLabel("Reality-TV")).toBe("Reality-TV");
  });
});
