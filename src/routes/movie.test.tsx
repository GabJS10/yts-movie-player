import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { renderApp } from "../test/render";

describe("/movie/$movieId", () => {
  it("shows details, preselects the best x264 version and links Reproducir to it", async () => {
    await renderApp("/movie/1632");
    expect(await screen.findByRole("heading", { level: 1, name: "Interstellar" })).toBeInTheDocument();
    // Sharp still (screenshotUrls[0]) as backdrop, not the blurred backgroundUrl.
    expect(document.querySelector("article img")).toHaveAttribute(
      "src",
      "/design/prototype/bg/interstellar-endurance.jpg",
    );
    const radios = await screen.findAllByRole("radio");
    const checked = radios.filter((r) => r.getAttribute("aria-checked") === "true");
    expect(checked).toHaveLength(1);
    expect(checked[0]).toHaveTextContent("1080p");
    expect(screen.getByText("HEVC: puede necesitar VLC")).toBeInTheDocument();
    const play = screen.getByRole("link", { name: /Reproducir 1080p/ });
    expect(play.getAttribute("href")).toMatch(/^\/play\/1632\?infohash=[0-9a-f]{40}$/);
    expect(screen.getByRole("button", { name: /Descargar/ })).toBeDisabled(); // phase 6
    expect(screen.getByText(/Matthew McConaughey/)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("heading", { name: "Similares" })).toBeInTheDocument());
  });

  it("explains a missing movie and offers a way home", async () => {
    await renderApp("/movie/1");
    expect(await screen.findByText("No encontramos esta película")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Volver al inicio" })).toHaveAttribute("href", "/");
  });

  describe("Mi lista (♥)", () => {
    // Mock: Spider-Verse (10960) is not in Mi lista; Interstellar (1632) is.
    it("adds optimistically and confirms with a toast", async () => {
      const user = userEvent.setup();
      const { calls } = await renderApp("/movie/10960");
      const fav = await screen.findByRole("button", { name: "Mi lista" });
      expect(fav).toHaveAttribute("aria-pressed", "false");
      await user.click(fav);
      // Flipped before the backend answers.
      expect(screen.getByRole("button", { name: "En Mi lista" })).toHaveAttribute("aria-pressed", "true");
      expect(await screen.findByText("Añadida a Mi lista")).toBeInTheDocument();
      const add = calls.find((c) => c.cmd === "add_favorite")?.args as { movie: Record<string, unknown> };
      expect(add.movie).toMatchObject({ id: 10960, title: "Spider-Man: Into the Spider-Verse" });
      expect(add.movie).not.toHaveProperty("torrents"); // a MovieSummary, not the detail
    });

    it("rolls back and explains when the backend fails", async () => {
      const user = userEvent.setup();
      await renderApp("/movie/1632", { fail: { remove_favorite: { code: "db", message: "disk full" } } });
      await user.click(await screen.findByRole("button", { name: "En Mi lista" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("No se pudo quitar de Mi lista");
      expect(screen.getByRole("button", { name: "En Mi lista" })).toHaveAttribute("aria-pressed", "true");
    });
  });

  it("offers Continuar (h:mm:ss) and Desde el principio when there's progress", async () => {
    await renderApp("/movie/10960");
    const resume = await screen.findByRole("link", { name: /Continuar \(1:02:00\)/ });
    expect(resume.getAttribute("href")).toMatch(/^\/play\/10960\?infohash=[0-9a-f]{40}$/);
    expect(screen.getByRole("link", { name: /Desde el principio/ }).getAttribute("href")).toMatch(
      /^\/play\/10960\?infohash=[0-9a-f]{40}&from=start$/,
    );
  });

  it("without progress there's only Reproducir", async () => {
    await renderApp("/movie/1632");
    expect(await screen.findByRole("link", { name: /Reproducir 1080p/ })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: /Desde el principio/ })).not.toBeInTheDocument();
  });

  it("preselects the version from the settings (preferred quality, x264)", async () => {
    await renderApp("/movie/1632", {
      before: (b) => b.handle("update_settings", { patch: { preferredQuality: "720p" } }),
    });
    expect(await screen.findByRole("link", { name: /Reproducir 720p/ })).toBeInTheDocument();
  });

  it("with x264 off and 4K preferred, picks the 2160p x265", async () => {
    await renderApp("/movie/1632", {
      before: (b) => b.handle("update_settings", { patch: { preferredQuality: "2160p", preferX264: false } }),
    });
    expect(await screen.findByRole("link", { name: /Reproducir 2160p/ })).toBeInTheDocument();
  });
});
