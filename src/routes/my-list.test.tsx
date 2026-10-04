import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { renderApp } from "../test/render";

const cardNames = () =>
  [...document.querySelectorAll("[data-card]")].map((c) =>
    c.getAttribute("aria-label")?.replace(/ \(\d+\)$/, ""),
  );

describe("/my-list", () => {
  it("lists the saved movies, most recent first, and re-sorts them", async () => {
    const user = userEvent.setup();
    await renderApp("/my-list");
    expect(await screen.findByText("5 películas guardadas para después")).toBeInTheDocument();
    expect(cardNames()).toEqual([
      "The Dark Knight",
      "The Godfather",
      "The Shawshank Redemption",
      "Coco",
      "Interstellar",
    ]);
    await user.click(screen.getByRole("button", { name: "Año" }));
    expect(screen.getByRole("button", { name: "Año" })).toHaveAttribute("aria-pressed", "true");
    expect(cardNames()[0]).toBe("Coco");
    await user.click(screen.getByRole("button", { name: "Valoración" }));
    expect(cardNames()[0]).toBe("The Shawshank Redemption");
  });

  it("shows what was just added from the movie page", async () => {
    const user = userEvent.setup();
    const { router } = await renderApp("/movie/10960");
    await user.click(await screen.findByRole("button", { name: "Mi lista" }));
    await screen.findByText("Añadida a Mi lista");
    await router.navigate({ to: "/my-list" });
    await waitFor(() => expect(cardNames()[0]).toBe("Spider-Man: Into the Spider-Verse"));
  });

  it("has an empty state with a way out", async () => {
    await renderApp("/my-list", {
      before: (b) =>
        [3175, 3304, 3709, 7062, 1632].forEach((movieId) => b.handle("remove_favorite", { movieId })),
    });
    expect(await screen.findByRole("heading", { name: "Tu lista está vacía" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Explorar el catálogo" })).toHaveAttribute("href", "/");
  });
});
