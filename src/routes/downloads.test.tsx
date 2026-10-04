import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Download, MovieDetail } from "../api/types";
import { formatBytes } from "../lib/format";
import { toSummary } from "../lib/movie";
import { MOCK_DATA_DIR } from "../mocks/backend";
import { renderApp } from "../test/render";

// Mock: The Dark Knight and Coco downloading, The Shawshank Redemption paused, Spider-Verse (4K) stalled,
// The Godfather finished.
const count = (calls: { cmd: string }[], cmd: string) => calls.filter((c) => c.cmd === cmd).length;
const row = (title: RegExp) => screen.getByRole("progressbar", { name: title }).closest("li") as HTMLElement;

async function open(options?: Parameters<typeof renderApp>[1]) {
  const r = await renderApp("/downloads", options);
  await screen.findByRole("heading", { name: "En curso" });
  const list = r.backend.handle("list_downloads") as Download[];
  const byTitle = (title: string) => list.find((d) => d.movie.title === title)!;
  return { ...r, byTitle };
}

describe("/downloads", () => {
  it("groups in progress and library downloads with their state, progress, speed, peers and ETA", async () => {
    await open();
    const groups = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(groups).toEqual(expect.arrayContaining(["En curso", "En la biblioteca"]));

    const dark = row(/The Dark Knight/);
    expect(within(dark).getByText("Descargando")).toBeInTheDocument();
    expect(within(dark).getByText("63,4 %")).toBeInTheDocument();
    expect(within(dark).getByText("5,2 MB/s")).toBeInTheDocument();
    expect(within(dark).getByText("41 peers")).toBeInTheDocument();
    expect(within(dark).getByText("quedan 4 min")).toBeInTheDocument();
    expect(within(dark).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "63");

    expect(within(row(/Shawshank/)).getByText("En pausa")).toBeInTheDocument();
    const stalled = row(/Spider-Verse/);
    expect(within(stalled).getByText("Sin seeds conectados")).toBeInTheDocument();
    expect(within(stalled).getByText(/Prueba otra calidad/)).toBeInTheDocument();
    expect(within(stalled).getByText("x265 · HEVC")).toBeInTheDocument();

    const done = row(/The Godfather/);
    expect(within(done).getByText("Completada")).toBeInTheDocument();
    expect(within(done).getByText(/sin conexión desde la biblioteca/)).toBeInTheDocument();
  });

  it("polls list_downloads about once a second while open, and shows the new progress", async () => {
    const { calls, backend } = await open();
    const before = count(calls, "list_downloads");
    backend.tick();
    await waitFor(() => expect(count(calls, "list_downloads")).toBeGreaterThan(before), { timeout: 2500 });
    await waitFor(() => expect(within(row(/The Dark Knight/)).queryByText("63,4 %")).not.toBeInTheDocument());
  });

  it("stops polling when the page closes", async () => {
    const user = userEvent.setup();
    const { calls, router } = await open();
    await user.click(
      within(screen.getByRole("navigation", { name: "Principal" })).getByRole("link", { name: "Mi lista" }),
    );
    await waitFor(() => expect(router.state.location.pathname).toBe("/my-list"));
    const after = count(calls, "list_downloads");
    await new Promise((r) => setTimeout(r, 1500));
    expect(count(calls, "list_downloads")).toBe(after);
  });

  it("pauses and resumes", async () => {
    const user = userEvent.setup();
    const { calls, byTitle } = await open();
    await user.click(screen.getByRole("button", { name: "Pausar The Dark Knight (1080p)" }));
    expect(calls.find((c) => c.cmd === "pause_download")?.args).toEqual({
      infohash: byTitle("The Dark Knight").infohash,
    });
    expect(await within(row(/The Dark Knight/)).findByText("En pausa")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Reanudar The Shawshank Redemption (720p)" }));
    expect(await within(row(/Shawshank/)).findByText("Descargando")).toBeInTheDocument();
  });

  it("removes after asking whether to delete the files too", async () => {
    const user = userEvent.setup();
    const { calls, byTitle } = await open();
    const trash = screen.getByRole("button", { name: "Quitar The Godfather (1080p)" });

    await user.click(trash);
    let dialog = screen.getByRole("dialog", { name: /¿Quitar The Godfather \(1080p\)\?/ });
    expect(dialog).toHaveTextContent("¿Borrar también los archivos? Liberarías 2,4 GB");
    expect(within(dialog).getByRole("button", { name: "Cancelar" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trash).toHaveFocus();
    expect(count(calls, "remove_download")).toBe(0);

    await user.click(trash);
    dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Borrar archivos" }));
    expect(calls.find((c) => c.cmd === "remove_download")?.args).toEqual({
      infohash: byTitle("The Godfather").infohash,
      deleteFiles: true,
    });
    expect(await screen.findByText("The Godfather borrada del disco")).toBeInTheDocument();
    expect(screen.queryByRole("progressbar", { name: /The Godfather/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "En la biblioteca" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Quitar Coco (1080p)" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Conservar archivos" }));
    expect(calls.filter((c) => c.cmd === "remove_download").at(-1)?.args).toMatchObject({
      deleteFiles: false,
    });
    expect(await screen.findByText(/Coco quitada; los archivos siguen/)).toBeInTheDocument();
  });

  it("plays and opens the folder of a finished download; the folder waits until it exists", async () => {
    const user = userEvent.setup();
    const { calls, byTitle } = await open();
    const godfather = byTitle("The Godfather");
    expect(screen.getByRole("link", { name: "Reproducir The Godfather (1080p)" })).toHaveAttribute(
      "href",
      `/play/3304?infohash=${godfather.infohash}`,
    );
    await user.click(screen.getByRole("button", { name: "Abrir la carpeta de The Godfather (1080p)" }));
    expect(calls.find((c) => c.cmd === "open_download_folder")?.args).toEqual({
      infohash: godfather.infohash,
    });
  });

  it("the folder button waits until the folder exists", async () => {
    const { backend } = await renderApp("/downloads", {
      before: (b) => {
        const movie = b.handle("get_movie", { movieId: 1632 }) as MovieDetail;
        b.handle("start_download", { movie: toSummary(movie), infohash: movie.torrents[0]!.infohash });
      },
    });
    const folder = await screen.findByRole("button", { name: "Abrir la carpeta de Interstellar (720p)" });
    expect(folder).toBeDisabled();
    act(() => void backend.tick()); // the queue starts and its folder appears
    await waitFor(() => expect(folder).toBeEnabled());
  });

  it("a download whose folder is missing, or being moved, can't be played, paused or opened", async () => {
    const { byTitle, backend } = await open();
    act(() => {
      backend.patchDownload(byTitle("The Godfather").infohash, { state: "unavailable" });
      backend.patchDownload(byTitle("Coco").infohash, { state: "moving" });
    });
    const godfather = await waitFor(() => {
      const r = row(/The Godfather/);
      expect(within(r).getByText("Carpeta no disponible")).toBeInTheDocument();
      return r;
    });
    // Still a complete download: it stays in the library group.
    expect(screen.getByRole("region", { name: "En la biblioteca" })).toContainElement(godfather);
    expect(within(godfather).getByText(/Sigue sola cuando la carpeta vuelva/)).toBeInTheDocument();
    expect(within(godfather).getByRole("button", { name: /^Reproducir/ })).toBeDisabled();
    expect(within(godfather).getByRole("button", { name: /^Abrir la carpeta/ })).toBeDisabled();
    expect(within(godfather).getByRole("button", { name: /^Quitar/ })).toBeEnabled();

    const coco = row(/Coco/);
    expect(within(coco).getByText("Moviendo a la carpeta nueva…")).toBeInTheDocument();
    expect(within(coco).getByRole("button", { name: /^(Pausar|Reanudar)/ })).toBeDisabled();
    expect(within(coco).getByRole("button", { name: /^Quitar/ })).toBeDisabled();
    expect(screen.getByRole("link", { name: "Descargas, 2 activas" })).toBeInTheDocument();
  });

  it("explains a failed action with a next step", async () => {
    const user = userEvent.setup();
    await open({ fail: { open_download_folder: { code: "io", message: "xdg-open failed" } } });
    await user.click(screen.getByRole("button", { name: "Abrir la carpeta de The Godfather (1080p)" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "No se pudo abrir la carpeta. Libera espacio o revisa la carpeta de datos.",
    );
  });

  it("shows the library, cache and speed summary", async () => {
    const { backend } = await open();
    const summary = screen.getByText("Biblioteca").closest("dl") as HTMLElement;
    const total = (backend.handle("list_downloads") as Download[]).reduce((n, d) => n + d.downloadedBytes, 0);
    expect(await within(summary).findByText(`${formatBytes(total)} · 1 película`)).toBeInTheDocument();
    expect(within(summary).getByText(`${MOCK_DATA_DIR}/library`)).toBeInTheDocument();
    expect(within(summary).getByRole("meter", { name: "Uso de la caché" })).toBeInTheDocument();
    expect(within(summary).getByText("No se comparte al terminar")).toBeInTheDocument();
  });

  it("has an empty state", async () => {
    await renderApp("/downloads", {
      before: (b) => {
        for (const d of b.handle("list_downloads") as Download[])
          b.handle("remove_download", { infohash: d.infohash, deleteFiles: true });
      },
    });
    expect(await screen.findByRole("heading", { name: "No hay descargas" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Explorar el catálogo" })).toHaveAttribute("href", "/");
  });
});
