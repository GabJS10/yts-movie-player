import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { MovieSummary } from "../api/types";
import { renderWithProviders } from "../test/render";
import { MovieCard } from "./MovieCard";

const movie: MovieSummary = {
  id: 1632,
  imdbCode: "tt0816692",
  title: "Interstellar",
  year: 2014,
  rating: 8.7,
  runtimeMin: 169,
  genres: ["Adventure", "Drama", "Sci-Fi"],
  coverUrl: "http://127.0.0.1:1/img/a",
  coverLargeUrl: "http://127.0.0.1:1/img/b",
  backgroundUrl: null,
  qualities: ["3D", "2160p", "720p", "1080p"],
  hasX264: true,
};

describe("MovieCard", () => {
  it("links to the movie page with a lazy poster", async () => {
    const { container } = await renderWithProviders(<MovieCard movie={movie} />);
    const link = screen.getByRole("link", { name: "Interstellar (2014)" });
    expect(link).toHaveAttribute("href", "/movie/1632");
    const img = container.querySelector("img");
    expect(img).toHaveAttribute("loading", "lazy");
    expect(img).toHaveAttribute("src", movie.coverUrl);
  });

  it("shows rating, runtime and qualities in ascending order", async () => {
    const { container } = await renderWithProviders(<MovieCard movie={movie} />);
    expect(container.textContent).toContain("8,7");
    expect(container.textContent).toContain("2 h 49 min");
    const chips = [...container.querySelectorAll(".qchip")].map((c) => c.textContent);
    expect(chips).toEqual(["720p", "1080p", "2160p", "3D"]);
  });

  it("marks qualities dashed when the movie only has x265", async () => {
    const { container } = await renderWithProviders(<MovieCard movie={{ ...movie, hasX264: false }} />);
    expect(container.querySelectorAll(".qchip-hevc")).toHaveLength(4);
  });
});
