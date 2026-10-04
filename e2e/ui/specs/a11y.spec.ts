import AxeBuilder from "@axe-core/playwright";
import { test, expect, openMock } from "../fixtures";

const pages = ["/", "/search", "/movie/8462", "/my-list", "/downloads", "/settings"];

for (const path of pages) {
  test(`accesibilidad (axe, serious/critical): ${path}`, async ({ page }) => {
    await openMock(page, path, { tick: 0 });
    // Not "networkidle": the rotating banner keeps preloading images.
    await page.locator("main").waitFor();
    await page.waitForTimeout(500);
    const { violations } = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).analyze();
    const serious = violations
      .filter((v) => v.impact === "serious" || v.impact === "critical")
      .map((v) => `${v.id}: ${v.help} (${v.nodes.length})`);
    expect(serious, serious.join("\n")).toEqual([]);
  });
}
