import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
    expect(calls.map((c) => c.cmd).sort()).toEqual(["get_movie", "get_settings", "list_movies"]);
    expect(await screen.findByText("1080p")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Más info/ })).toHaveAttribute("href", "/movie/8462");
  });

  it("♥ adds the featured movie to Mi lista", async () => {
    const user = userEvent.setup();
    const { calls } = await renderWithProviders(<HeroBanner />);
    const fav = await screen.findByRole("button", { name: "Añadir a Mi lista" });
    await waitFor(() => expect(fav).toBeEnabled()); // once get_movie says whether it's saved
    await user.click(fav);
    expect(screen.getByRole("button", { name: "Quitar de Mi lista" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await waitFor(() => expect(calls.some((c) => c.cmd === "add_favorite")).toBe(true));
  });
});
