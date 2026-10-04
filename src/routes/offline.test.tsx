import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Download } from "../api/types";
import { renderApp } from "../test/render";

// Mock offline: the catalog fails with `network`; downloaded movies (any state) answer from their saved
// copy. The Godfather is the finished one.
const offline = { before: (b: { setOffline: (on: boolean) => void }) => b.setOffline(true) };

describe("Sin conexión", () => {
  it("Inicio hides the catalog rows and shows the library, Continuar viendo and Mi lista", async () => {
    await renderApp("/", offline);
    expect(await screen.findByRole("heading", { level: 1, name: "Sin conexión" })).toBeInTheDocument();
    const banner = screen.getByRole("banner");
    expect(within(banner).getByRole("link", { name: "Sin conexión" })).toHaveAttribute("href", "/downloads");

    const library = await screen.findByRole("region", { name: "En tu biblioteca" });
    expect(within(library).getByText("The Godfather")).toBeInTheDocument();
    expect(await screen.findByRole("region", { name: "Mi lista" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Destacada" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Tendencias en YTS" })).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("the movie page shows the saved copy, only the downloaded version, and no Similares", async () => {
    const { backend } = await renderApp("/movie/3304", offline);
    const done = (backend.handle("list_downloads") as Download[]).find((d) => d.movie.id === 3304)!;
    expect(await screen.findByRole("heading", { level: 1, name: "The Godfather" })).toBeInTheDocument();
    expect(screen.getByText(/es la copia guardada al descargarla/)).toBeInTheDocument();
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(1);
    expect(radios[0]).toHaveTextContent("1080p");
    expect(screen.getByRole("link", { name: /Reproducir 1080p/ })).toHaveAttribute(
      "href",
      `/play/3304?infohash=${done.infohash}`,
    );
    expect(screen.queryByRole("heading", { name: "Similares" })).not.toBeInTheDocument();
    expect(
      within(screen.getByRole("banner")).getByRole("link", { name: "Sin conexión" }),
    ).toBeInTheDocument();
  });

  it("a movie that isn't downloaded explains it's offline", async () => {
    await renderApp("/movie/1632", offline);
    expect(await screen.findByRole("heading", { name: "Sin conexión", level: 2 })).toBeInTheDocument();
    expect(screen.getByText("Revisa tu conexión a internet y vuelve a intentarlo.")).toBeInTheDocument();
  });

  it("Descargas works offline and leads to the saved copy", async () => {
    await renderApp("/downloads", offline);
    expect(await screen.findByRole("heading", { name: "En la biblioteca" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "The Godfather" })).toHaveAttribute("href", "/movie/3304");
  });

  it("comes back by itself when the network returns", async () => {
    const { backend } = await renderApp("/", offline);
    expect(await screen.findByRole("heading", { level: 1, name: "Sin conexión" })).toBeInTheDocument();
    backend.setOffline(false);
    act(() => {
      window.dispatchEvent(new Event("online"));
    });
    await waitFor(() =>
      expect(screen.queryByRole("heading", { level: 1, name: "Sin conexión" })).not.toBeInTheDocument(),
    );
    expect(await screen.findByRole("region", { name: "Destacada" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { name: "Tendencias en YTS" })).toBeInTheDocument();
    expect(within(screen.getByRole("banner")).queryByRole("link", { name: "Sin conexión" })).toBeNull();
  });
});
