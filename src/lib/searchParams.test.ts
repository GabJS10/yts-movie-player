import { describe, expect, it } from "vitest";
import { validateCatalogSearch, validatePlaySearch } from "./searchParams";

describe("validateCatalogSearch", () => {
  it("keeps valid params with their types", () => {
    expect(
      validateCatalogSearch({
        query: "  matrix ",
        genre: "sci-fi",
        quality: "1080p.x265",
        minimumRating: "7",
        sortBy: "rating",
        orderBy: "asc",
      }),
    ).toEqual({
      query: "matrix",
      genre: "sci-fi",
      quality: "1080p.x265",
      minimumRating: 7,
      sortBy: "rating",
      orderBy: "asc",
    });
  });

  it("drops invalid values", () => {
    expect(
      validateCatalogSearch({
        query: "  ",
        genre: "Sci Fi",
        quality: "4k",
        minimumRating: 12,
        sortBy: "nope",
        orderBy: "up",
      }),
    ).toEqual({});
  });
});

describe("validatePlaySearch", () => {
  const hash = "A".repeat(40);
  it("keeps a valid infohash (lowercased) and from=start", () => {
    expect(validatePlaySearch({ infohash: hash, from: "start" })).toEqual({
      infohash: "a".repeat(40),
      from: "start",
    });
  });
  it("drops anything else", () => {
    expect(validatePlaySearch({ infohash: "xyz", from: "middle" })).toEqual({});
  });
});
