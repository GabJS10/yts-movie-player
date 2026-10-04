import { describe, expect, it } from "vitest";
import type { MovieDetail, MovieSummary } from "../api/types";
import { createMockBackend } from "../mocks/backend";
import { sortFavorites, toSummary } from "./movie";
import { timeLeft } from "./format";
import { validateBaseUrl } from "./settings";

const detail = createMockBackend().handle("get_movie", { movieId: 1632 }) as MovieDetail;

describe("toSummary", () => {
  it("keeps exactly the MovieSummary fields", () => {
    const s = toSummary(detail);
    expect(Object.keys(s).sort()).toEqual(
      [
        "id",
        "imdbCode",
        "title",
        "year",
        "rating",
        "runtimeMin",
        "genres",
        "coverUrl",
        "coverLargeUrl",
        "backgroundUrl",
        "qualities",
        "hasX264",
        "maxSeeds",
      ].sort(),
    );
    expect(s.title).toBe("Interstellar");
  });
});

describe("sortFavorites", () => {
  const m = (id: number, title: string, year: number, rating: number) =>
    ({ ...toSummary(detail), id, title, year, rating }) as MovieSummary;
  const list = [m(1, "Coco", 2017, 8.4), m(2, "Alien", 1979, 8.5), m(3, "Ñandú", 2001, 6)];
  it.each([
    ["added", [1, 2, 3]],
    ["rating", [2, 1, 3]],
    ["year", [1, 3, 2]],
    ["title", [2, 1, 3]],
  ] as const)("%s", (sort, ids) => expect(sortFavorites(list, sort).map((x) => x.id)).toEqual(ids));
});

describe("timeLeft", () => {
  it("rounds to minutes and uses hours past 60", () => {
    expect(timeLeft(3720, 10140)).toBe("Quedan 1 h 47 min");
    expect(timeLeft(100, 400)).toBe("Quedan 5 min");
    expect(timeLeft(395, 400)).toBe("Queda menos de un minuto");
  });
});

describe("validateBaseUrl", () => {
  const existing = ["https://yts.gg/api/v2/"];
  it("accepts absolute http(s) URLs not in the list", () => {
    expect(validateBaseUrl(" https://mirror.example/api/v2/ ", existing)).toBeNull();
  });
  it("rejects empty, malformed, non-http and duplicates", () => {
    expect(validateBaseUrl("  ", existing)).toMatch(/Escribe/);
    expect(validateBaseUrl("yts.gg", existing)).toMatch(/no es una URL/i);
    expect(validateBaseUrl("ftp://yts.gg/", existing)).toMatch(/https/);
    expect(validateBaseUrl("https://yts.gg/api/v2/", existing)).toMatch(/ya está/);
    const full = Array.from({ length: 10 }, (_, i) => `https://m${i}.example/`);
    expect(validateBaseUrl("https://new.example/", full)).toMatch(/como máximo/);
  });
});
