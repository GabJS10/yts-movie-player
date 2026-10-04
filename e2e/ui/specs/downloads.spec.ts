import { test, expect, openMock } from "../fixtures";

test("pausar y reanudar una descarga", async ({ page }) => {
  await openMock(page, "/downloads", { tick: 0 });
  // The Dark Knight (3175) starts downloading in the mock.
  const row = page
    .locator('[data-testid="download-row"]')
    .filter({ has: page.locator('[data-state="active"]') })
    .first()
    .or(page.locator('[data-testid="download-row"][data-state="active"]').first());
  await expect(row).toBeVisible();
  const infohash = await row.getAttribute("data-infohash");
  const sel = page.locator(`[data-testid="download-row"][data-infohash="${infohash}"]`);

  await sel.getByTestId("download-toggle").click();
  await expect(sel).toHaveAttribute("data-state", "paused");
  await sel.getByTestId("download-toggle").click();
  await expect(sel).not.toHaveAttribute("data-state", "paused");
});

test("quitar una descarga pide confirmación", async ({ page }) => {
  await openMock(page, "/downloads", { tick: 0 });
  const rows = page.getByTestId("download-row");
  await expect(rows.first()).toBeVisible();
  const count = await rows.count();
  await rows.first().getByTestId("download-remove").click();
  await expect(page.getByTestId("remove-dialog")).toBeVisible();
  await page.getByTestId("remove-cancel").click();
  await expect(rows).toHaveCount(count);

  await rows.first().getByTestId("download-remove").click();
  await page.getByTestId("remove-keep").click();
  await expect(rows).toHaveCount(count - 1);
});
