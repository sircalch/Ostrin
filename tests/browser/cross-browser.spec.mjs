import { expect, test } from "@playwright/test";

const publicPages = [
  "./",
  "./community.html",
  "./cookbook.html",
  "./benchmarks.html",
  "./docs.html",
  "./ecosystem.html",
  "./examples.html",
  "./guides.html",
  "./language.html",
  "./playground.html",
  "./provenance.html",
  "./reference.html",
  "./roadmap.html",
  "./showcase.html",
  "./viz.html",
];

async function expectNoHorizontalOverflow(page, route, width) {
  await page.evaluate(async () => {
    if (document.fonts?.ready) await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  });
  const dimensions = await page.evaluate(() => ({
    viewport: document.documentElement.clientWidth,
    document: document.documentElement.scrollWidth,
    body: document.body.scrollWidth,
  }));
  expect(dimensions.document, `${route} overflows at ${width}px`).toBeLessThanOrEqual(dimensions.viewport);
  expect(dimensions.body, `${route} body overflows at ${width}px`).toBeLessThanOrEqual(dimensions.viewport);
}

test("all public pages render without horizontal overflow", async ({ page }) => {
  test.setTimeout(120_000);
  const runtimeErrors = [];
  page.on("pageerror", (error) => runtimeErrors.push(error.message));

  for (const width of [390, 1280]) {
    await page.setViewportSize({ width, height: 900 });
    for (const route of publicPages) {
      await page.goto(route, { waitUntil: "domcontentloaded" });
      await expect(page.locator("main")).toBeVisible();
      await expect(page.getByRole("heading", { level: 1 }).first()).toBeVisible();
      await expectNoHorizontalOverflow(page, route, width);
    }
  }

  expect(runtimeErrors).toEqual([]);
});

test("navigation and example tabs keep their keyboard semantics", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("./", { waitUntil: "domcontentloaded" });

  const menuButton = page.locator(".menu-toggle");
  const navigation = page.getByRole("navigation", { name: "Main navigation" });
  await menuButton.focus();
  await page.keyboard.press("Enter");
  await expect(navigation).toBeVisible();
  await expect(navigation.getByRole("link", { name: "Learn" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(navigation).toBeHidden();
  await expect(menuButton).toBeFocused();

  await page.goto("./examples.html", { waitUntil: "domcontentloaded" });
  const tabs = page.getByRole("tab");
  await expect(tabs).toHaveCount(3);
  await tabs.nth(0).focus();
  await page.keyboard.press("ArrowRight");
  await expect(tabs.nth(1)).toBeFocused();
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#concurrency-tab")).toBeVisible();
});
