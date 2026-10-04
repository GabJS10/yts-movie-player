import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { renderApp, renderWithProviders } from "../test/render";
import { ContinueRow } from "./ContinueRow";

describe("Continuar viendo", () => {
  // Renders the whole home (hero + 9 catalog rows): slow under a loaded parallel run.
  it("is the first row on Inicio, then 'Porque viste X' and Mi lista", { timeout: 15_000 }, async () => {
    await renderApp("/");
    await screen.findByRole("heading", { name: "Continuar viendo" }, { timeout: 5000 });
    await screen.findByRole("heading", { name: "Mi lista" });
    await screen.findByRole("heading", { name: /^Porque viste / });
    const rows = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    // Mock: the most recent progress is Spider-Verse.
    expect(rows.slice(0, 3)).toEqual([
      "Continuar viendo",
      "Porque viste Spider-Man: Into the Spider-Verse",
      "Mi lista",
    ]);
  });

  it("shows time left and progress, and links to the player", async () => {
    await renderWithProviders(<ContinueRow />);
    const card = await screen.findByRole("link", { name: /Continuar Spider-Man: Into the Spider-Verse/ });
    // Mock: 3720 s of 117 min.
    expect(card).toHaveAccessibleName("Continuar Spider-Man: Into the Spider-Verse, quedan 55 min");
    expect(card).toHaveAttribute("href", "/play/10960");
    expect(within(card).getByText(/1:02:00 de 1:57:00/)).toBeInTheDocument();
    expect(card.querySelector(".bg-green")).toHaveStyle({ width: "53.0%" });
    const titles = screen.getAllByRole("link").map((a) => a.getAttribute("href"));
    expect(titles).toEqual(["/play/10960", "/play/7062", "/play/8462"]);
  });

  it("removes an item at once and tells the backend", async () => {
    const user = userEvent.setup();
    const { calls } = await renderWithProviders(<ContinueRow />);
    await user.click(await screen.findByRole("button", { name: "Quitar Coco de Continuar viendo" }));
    expect(screen.queryByRole("link", { name: /Continuar Coco/ })).not.toBeInTheDocument();
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "remove_progress")?.args).toEqual({ movieId: 7062 }),
    );
  });

  it("puts it back if removing fails", async () => {
    const user = userEvent.setup();
    await renderWithProviders(<ContinueRow />, {
      fail: { remove_progress: { code: "db", message: "locked" } },
    });
    await user.click(await screen.findByRole("button", { name: "Quitar Coco de Continuar viendo" }));
    expect(await screen.findByRole("link", { name: /Continuar Coco/ })).toBeInTheDocument();
  });

  it("is hidden when there's nothing to continue", async () => {
    const { calls } = await renderWithProviders(<ContinueRow />, {
      before: (b) => [10960, 7062, 8462].forEach((movieId) => b.handle("remove_progress", { movieId })),
    });
    await waitFor(() => expect(calls.some((c) => c.cmd === "list_continue_watching")).toBe(true));
    expect(screen.queryByRole("heading", { name: "Continuar viendo" })).not.toBeInTheDocument();
  });

  it("finished movies (≥ 92 %) drop out", async () => {
    await renderWithProviders(<ContinueRow />, {
      before: (b) => {
        const movie = b.handle("get_movie", { movieId: 10960 });
        b.handle("save_progress", { movie, positionS: 6500, durationS: 7020 });
      },
    });
    await screen.findByRole("link", { name: /Continuar Coco/ });
    expect(screen.queryByRole("link", { name: /Spider-Verse/ })).not.toBeInTheDocument();
  });
});
