import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Download, MovieDetail } from "../api/types";
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
    expect(screen.getByRole("button", { name: /Descargar/ })).toBeEnabled();
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

  describe("Descargar", () => {
    it("Descargar → En cola → Descargando N % (polled) → Descargada", async () => {
      const user = userEvent.setup();
      const { calls, backend } = await renderApp("/movie/1632");
      await user.click(await screen.findByRole("button", { name: /Descargar/ }));
      const start = calls.find((c) => c.cmd === "start_download")?.args as {
        movie: Record<string, unknown>;
        infohash: string;
      };
      const chosen = (backend.handle("get_movie", { movieId: 1632 }) as MovieDetail).torrents.find(
        (t) => t.quality === "1080p",
      )!;
      expect(start.infohash).toBe(chosen.infohash);
      expect(start.movie).toMatchObject({ id: 1632, title: "Interstellar" });
      expect(start.movie).not.toHaveProperty("torrents");
      expect(await screen.findByText("Descargando Interstellar · 1080p")).toBeInTheDocument();
      expect(screen.getByRole("link", { name: "En cola (1080p), ver en Descargas" })).toHaveAttribute(
        "href",
        "/downloads",
      );

      // Progress arrives by polling list_downloads while the download is in progress.
      backend.tick();
      expect(await screen.findByRole("link", { name: /^Descargando 0 %/ })).toBeInTheDocument();
      const polls = calls.filter((c) => c.cmd === "list_downloads").length;
      backend.completeDownload(chosen.infohash);
      expect(
        await screen.findByRole("link", { name: "Descargada (1080p), ver en Descargas" }),
      ).toBeInTheDocument();
      expect(calls.filter((c) => c.cmd === "list_downloads").length).toBeGreaterThanOrEqual(polls);
    });

    it("shows a download in progress with its percentage", async () => {
      await renderApp("/movie/3175");
      expect(await screen.findByRole("link", { name: /^Descargando 63 % \(1080p\)/ })).toBeInTheDocument();
    });

    it("shows a paused download", async () => {
      await renderApp("/movie/3709");
      expect(await screen.findByRole("link", { name: /^En pausa 47 % \(720p\)/ })).toBeInTheDocument();
    });

    it("a downloaded movie plays its downloaded version (library) whatever the settings prefer, without polling", async () => {
      const { backend, calls } = await renderApp("/movie/3304", {
        before: (b) => b.handle("update_settings", { patch: { preferredQuality: "720p" } }),
      });
      const done = (backend.handle("list_downloads") as Download[]).find((d) => d.movie.id === 3304)!;
      expect(
        await screen.findByRole("link", { name: "Descargada (1080p), ver en Descargas" }),
      ).toBeInTheDocument();
      expect(screen.getByRole("link", { name: /Reproducir 1080p/ })).toHaveAttribute(
        "href",
        `/play/3304?infohash=${done.infohash}`,
      );
      const polls = calls.filter((c) => c.cmd === "list_downloads").length;
      await new Promise((r) => setTimeout(r, 1300));
      expect(calls.filter((c) => c.cmd === "list_downloads").length).toBe(polls);
    });

    it("explains why a download couldn't start (no space)", async () => {
      const user = userEvent.setup();
      await renderApp("/movie/1632", {
        fail: { start_download: { code: "io", message: "not enough free space: need 2.3 GB" } },
      });
      await user.click(await screen.findByRole("button", { name: /Descargar/ }));
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "No se pudo descargar. Libera espacio o revisa la carpeta de datos.",
      );
      expect(screen.getByRole("button", { name: /Descargar/ })).toBeEnabled();
    });
  });
});
