import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Download } from "../api/types";
import { renderApp } from "../test/render";

describe("AppShell", () => {
  it("renders the navigation with Inicio active", async () => {
    await renderApp("/");
    const nav = screen.getByRole("navigation", { name: "Principal" });
    for (const label of ["Inicio", "Buscar", "Mi lista", "Descargas"]) {
      expect(within(nav).getByRole("link", { name: label })).toBeInTheDocument();
    }
    expect(within(nav).getByRole("link", { name: "Inicio" })).toHaveAttribute("aria-current", "page");
    expect(within(screen.getByRole("banner")).getByRole("link", { name: "Ajustes" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Destacada" })).toBeInTheDocument();
  });

  it("navigates between routes", async () => {
    const user = userEvent.setup();
    const { router } = await renderApp("/");
    const nav = screen.getByRole("navigation", { name: "Principal" });

    await user.click(within(nav).getByRole("link", { name: "Mi lista" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/my-list"));
    expect(await screen.findByRole("heading", { level: 1, name: "Mi lista" })).toBeInTheDocument();

    await user.click(within(screen.getByRole("banner")).getByRole("link", { name: "Ajustes" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Ajustes" })).toBeInTheDocument();
  });

  it("counts active downloads (queued, downloading, stalled) and follows download://changed", async () => {
    const { backend } = await renderApp("/");
    // Mock: two downloading, one stalled, one paused, one done.
    expect(await screen.findByRole("link", { name: "Descargas, 3 activas" })).toBeInTheDocument();
    const list = backend.handle("list_downloads") as Download[];
    const stalled = list.find((d) => d.state === "stalled")!;
    // Changed elsewhere (no command from this screen): only the event can tell the counter.
    backend.handle("pause_download", { infohash: stalled.infohash });
    expect(await screen.findByRole("link", { name: "Descargas, 2 activas" })).toBeInTheDocument();
    backend.handle("remove_download", { infohash: list.find((d) => d.state === "active")!.infohash });
    expect(await screen.findByRole("link", { name: "Descargas, 1 activa" })).toBeInTheDocument();
  });

  it("hides the navigation on the player", async () => {
    await renderApp("/play/1632");
    expect(screen.queryByRole("navigation", { name: "Principal" })).not.toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Interstellar" })).toBeInTheDocument();
  });

  it("passes validated search params to /search", async () => {
    const { router } = await renderApp("/search?query=matrix&quality=1080p&minimumRating=7");
    const match = router.state.matches.find((m) => m.routeId === "/search");
    expect(match?.search).toMatchObject({ query: "matrix", quality: "1080p", minimumRating: 7 });
  });
});
