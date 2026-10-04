import { test, expect, openMock } from "../fixtures";

test("Inicio → Ficha → Reproductor", async ({ page }) => {
  await openMock(page, "/");
  await expect(page.getByTestId("hero")).toBeVisible();
  await expect(page.getByTestId("movie-row").first()).toBeVisible();

  const card = page.getByTestId("movie-card").first();
  const movieId = await card.getAttribute("data-movie-id");
  await card.click();
  await expect(page).toHaveURL(new RegExp(`/movie/${movieId}`));
  await expect(page.getByTestId("movie-title")).toBeVisible();

  await page.getByTestId("movie-play").click();
  await expect(page).toHaveURL(/\/play\//);
  await expect(page.getByTestId("phase")).toBeVisible();
});

test("Mi lista: el ♥ de la Ficha se refleja en la página", async ({ page }) => {
  await openMock(page, "/movie/8462");
  const fav = page.getByTestId("movie-favorite");
  const before = await fav.getAttribute("aria-pressed");
  await fav.click();
  await expect(fav).toHaveAttribute("aria-pressed", before === "true" ? "false" : "true");

  await page.getByTestId("nav-my-list").click();
  const inList = page.locator('[data-testid="movie-card"][data-movie-id="8462"]');
  if (before === "true") await expect(inList).toHaveCount(0);
  else await expect(inList).toHaveCount(1);
});

test("la barra de navegación marca la sección activa", async ({ page }) => {
  await openMock(page, "/downloads");
  await expect(page.getByTestId("nav-downloads")).toHaveAttribute("aria-current", "page");
  await page.getByTestId("nav-search").click();
  await expect(page.getByTestId("nav-search")).toHaveAttribute("aria-current", "page");
});
