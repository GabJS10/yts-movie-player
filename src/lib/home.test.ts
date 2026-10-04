import { describe, expect, it } from "vitest";
import { reasonLabel } from "./featured";
import { orderRows, type RowDef } from "./home";

const rows: RowDef[] = [
  { title: "Tendencias", params: { sortBy: "download_count" } },
  { title: "Acción", params: { genre: "action" } },
  { title: "Comedia", params: { genre: "comedy" } },
  { title: "Terror", params: { genre: "horror" } },
  { title: "Drama", params: { genre: "drama" } },
];

describe("Inicio personalizado", () => {
  it("orders the genre rows by affinity; unranked keep their order after; general rows first", () => {
    expect(orderRows(rows, ["drama", "comedy", "western"]).map((r) => r.title)).toEqual([
      "Tendencias",
      "Drama",
      "Comedia",
      "Acción",
      "Terror",
    ]);
    expect(orderRows(rows, []).map((r) => r.title)).toEqual(rows.map((r) => r.title));
  });

  it("explains each banner reason in Spanish", () => {
    expect(reasonLabel({ kind: "because_watched", sourceMovieId: 1, sourceTitle: "Coco" })).toBe(
      "Porque viste Coco",
    );
    expect(reasonLabel({ kind: "because_list", sourceMovieId: 1, sourceTitle: "Coco" })).toBe(
      "Porque tienes Coco en Mi lista",
    );
    expect(reasonLabel({ kind: "genre", genre: "sci-fi" })).toBe("Para ti: Ciencia ficción");
    expect(reasonLabel({ kind: "trending" })).toBe("Tendencia en YTS");
  });
});
