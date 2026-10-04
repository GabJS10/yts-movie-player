// Real app, offline: Inicio → Ficha → Reproducir (torrent from the local seeder)
// → progreso guardado → Mi lista → Descargar → reproducir desde la biblioteca.
import { browser, $, $$, expect } from "@wdio/globals";

const movieId = process.env.E2E_PLAYABLE_MOVIE_ID!;
const byId = (id: string) => $(`[data-testid="${id}"]`);

async function go(path: string) {
  // The app is a single page; navigate through the router like a link would.
  await browser.execute((p) => {
    window.history.pushState({}, "", p);
    window.dispatchEvent(new PopStateEvent("popstate"));
  }, path);
}

describe("YTS Player (app real, sin internet)", () => {
  it("muestra el catálogo del servidor falso", async () => {
    await expect(byId("hero")).toBeDisplayed();
    await expect($(`[data-testid="movie-card"][data-movie-id="${movieId}"]`)).toExist();
  });

  it("abre la Ficha y reproduce desde el seeder local", async () => {
    await go(`/movie/${movieId}`);
    await expect(byId("movie-title")).toBeDisplayed();
    await byId("movie-play").click();

    const video = byId("video");
    await video.waitForExist({ timeout: 60_000 });
    await browser.waitUntil(
      async () => (await browser.execute(() => document.querySelector("video")?.currentTime ?? 0)) > 7, // progress under 5 s is not saved
      { timeout: 90_000, timeoutMsg: "el video no avanzó: ¿llega el torrent del seeder?" },
    );
  });

  it("guarda el progreso al salir y la Ficha ofrece Continuar", async () => {
    await byId("player-back").click();
    await expect(byId("movie-title")).toBeDisplayed();
    await expect(byId("movie-restart")).toBeDisplayed();
  });

  it("agrega a Mi lista", async () => {
    const fav = byId("movie-favorite");
    if ((await fav.getAttribute("aria-pressed")) !== "true") await fav.click();
    await expect(fav).toHaveAttribute("aria-pressed", "true");
    await go("/my-list");
    await expect($(`[data-testid="movie-card"][data-movie-id="${movieId}"]`)).toBeDisplayed();
  });

  it("descarga la película y la reproduce desde la biblioteca", async () => {
    await go(`/movie/${movieId}`);
    const dl = byId("movie-download");
    await dl.waitForDisplayed();
    if ((await dl.getAttribute("data-state")) === "none") await dl.click();

    await go("/downloads");
    const row = $('[data-testid="download-row"]');
    await row.waitForExist({ timeout: 30_000 });
    await browser.waitUntil(async () => (await row.getAttribute("data-state")) === "done", {
      timeout: 120_000,
      timeoutMsg: "la descarga no terminó",
    });

    await row.$('[data-testid="download-play"]').click();
    await browser.waitUntil(
      async () => (await browser.execute(() => document.querySelector("video")?.currentTime ?? 0)) > 1,
      { timeout: 30_000, timeoutMsg: "no se reproduce desde la biblioteca" },
    );
    expect(await $$('[data-testid="error-state"]').length).toBe(0);
  });
});
