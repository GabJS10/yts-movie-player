import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { intersectAll } from "../test/intersection";
import { renderWithProviders } from "../test/render";
import { MovieRow } from "./MovieRow";

const cards = () => document.querySelectorAll("[data-card]").length;

describe("MovieRow", () => {
  it("renders the first page, then loads the next one when the end comes into view", async () => {
    const { calls } = await renderWithProviders(
      <MovieRow title="Terror" params={{ genre: "horror", sortBy: "download_count" }} />,
    );
    expect(screen.getByRole("heading", { name: "Terror" })).toBeInTheDocument();
    await waitFor(() => expect(cards()).toBe(20));
    expect(calls.filter((c) => c.cmd === "list_movies")).toHaveLength(1);

    act(() => intersectAll());
    await waitFor(() => expect(cards()).toBeGreaterThanOrEqual(40));
    act(() => intersectAll(false));
    const pages = calls
      .filter((c) => c.cmd === "list_movies")
      .map((c) => (c.args as { params: { page: number; genre: string } }).params);
    // Keeps loading while the end stays in view; pages are requested in order, never twice.
    expect(pages.map((p) => p.page)).toEqual(pages.map((_, i) => i + 1));
    expect(pages.every((p) => p.genre === "horror")).toBe(true);
  });

  it("hides itself when the API returns no movies", async () => {
    await renderWithProviders(<MovieRow title="Nada" params={{ query: "zzzz-no-such-title" }} />);
    await waitFor(() => expect(screen.queryByRole("heading", { name: "Nada" })).not.toBeInTheDocument());
  });

  it("shows the error with a retry action", async () => {
    const user = userEvent.setup();
    const { calls } = await renderWithProviders(<MovieRow title="Caída" params={{ query: "!api" }} />);
    // The hook retries once (1 s) before giving up.
    expect(
      await screen.findByText("El catálogo de YTS no responde", {}, { timeout: 6000 }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Reintentar" }));
    await waitFor(() => expect(calls.filter((c) => c.cmd === "list_movies").length).toBeGreaterThan(2));
  });

  it("moves focus between cards with the arrow keys", async () => {
    const user = userEvent.setup();
    await renderWithProviders(<MovieRow title="Tendencias" params={{ sortBy: "download_count" }} />);
    await waitFor(() => expect(cards()).toBe(20));
    const [first, second] = document.querySelectorAll<HTMLElement>("[data-card]");
    first?.focus();
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(second);
    await user.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(first);
  });
});
