import { test, expect, openMock } from "../fixtures";

test("Ficha sin red muestra el error con su acción", async ({ page }) => {
  await openMock(page, "/movie/8462", { fail: "get_movie=network" });
  const err = page.getByTestId("error-state");
  await expect(err).toBeVisible();
  await expect(err).toHaveAttribute("data-code", "network");
  await expect(err.getByRole("button").or(err.getByRole("link")).first()).toBeVisible();
});

test("película inexistente → not_found", async ({ page }) => {
  await openMock(page, "/movie/1");
  await expect(page.getByTestId("error-state")).toHaveAttribute("data-code", "not_found");
});

test("API caída al buscar → api_unavailable", async ({ page }) => {
  await openMock(page, "/search");
  await page.getByTestId("search-input").fill("!api");
  await expect(page.getByTestId("error-state")).toHaveAttribute("data-code", "api_unavailable");
});

test("modo sin conexión muestra el indicador", async ({ page }) => {
  await openMock(page, "/", { offline: true });
  await expect(page.getByTestId("offline-indicator")).toBeVisible();
});
