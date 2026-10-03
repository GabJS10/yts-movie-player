import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderWithProviders } from "../test/render";
import { HeroBanner } from "./HeroBanner";

describe("HeroBanner", () => {
  it("uses the movie's sharp still and shows its default version", async () => {
    const { container, calls } = await renderWithProviders(<HeroBanner />);
    expect(
      await screen.findByRole("heading", { level: 1, name: "Avengers: Infinity War" }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(container.querySelector("section > img")).toHaveAttribute(
        "src",
        "/design/prototype/bg/infinity-war.jpg",
      ),
    );
    expect(calls.map((c) => c.cmd)).toEqual(["list_movies", "get_movie"]);
    expect(await screen.findByText("1080p")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Más info/ })).toHaveAttribute("href", "/movie/8462");
  });
});
