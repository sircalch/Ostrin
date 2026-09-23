import { expect, test } from "@playwright/test";

const publicPages = [
  "./",
  "./community.html",
  "./docs.html",
  "./ecosystem.html",
  "./examples.html",
  "./language.html",
  "./playground.html",
  "./roadmap.html",
  "./showcase.html",
];

async function expectNoHorizontalOverflow(page, route, width) {
  const dimensions = await page.evaluate(() => ({
    viewport: document.documentElement.clientWidth,
    document: document.documentElement.scrollWidth,
    body: document.body.scrollWidth,
  }));
  expect(dimensions.document, `${route} overflows at ${width}px`).toBeLessThanOrEqual(dimensions.viewport);
  expect(dimensions.body, `${route} body overflows at ${width}px`).toBeLessThanOrEqual(dimensions.viewport);
}

test("homepage runs the real compiler and renders its diagnostics", async ({ page }) => {
  const runtimeErrors = [];
  page.on("pageerror", (error) => runtimeErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") runtimeErrors.push(message.text());
  });

  await page.goto("./", { waitUntil: "domcontentloaded" });
  await expect(page).toHaveTitle(/Ostrin Programming Language/);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Make the meaning of code");
  await expect(page.getByLabel("Example program")).toBeVisible();
  await expectNoHorizontalOverflow(page, "homepage desktop", 1280);
  await expect(page.locator("vite-error-overlay, .nextjs-portal, #webpack-dev-server-client-overlay")).toHaveCount(0);

  const runButton = page.getByRole("button", { name: "Run" });
  await expect(runButton).toBeEnabled({ timeout: 45_000 });
  await runButton.click();
  await expect(page.locator("#output")).toContainText("5 m/s", { timeout: 30_000 });

  const source = page.getByLabel("Ostrin source");
  await source.fill("fn main() -> Void {\n    print(1 + true)\n}\n");
  await page.getByRole("button", { name: "Check" }).click();
  await expect(page.locator("#output .diagnostic.error")).toBeVisible();
  await expect(source).toHaveClass(/has-diagnostic/);
  await expect(page.locator("#source-status")).toContainText(/line \d+/);

  expect(runtimeErrors).toEqual([]);
});

test("mobile navigation is operable and labelled", async ({ page }) => {
  for (const width of [390, 768]) {
    await page.setViewportSize({ width, height: 844 });
    await page.goto("./", { waitUntil: "domcontentloaded" });
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expectNoHorizontalOverflow(page, "homepage", width);

    const menuButton = page.getByRole("button", { name: "Open navigation" });
    const navigation = page.getByRole("navigation", { name: "Main navigation" });
    await expect(menuButton).toBeVisible();
    await expect(menuButton).toHaveAttribute("aria-expanded", "false");
    await expect(navigation).toBeHidden();
    await menuButton.click();
    await expect(menuButton).toHaveAttribute("aria-expanded", "true");
    const docsLink = navigation.getByRole("link", { name: "Docs" });
    await expect(docsLink).toBeVisible();
    await docsLink.click();

    await expect(page).toHaveURL(/\/Ostrin\/docs\.html$/);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expectNoHorizontalOverflow(page, "docs.html", width);
  }
});

test("header switches at its tablet breakpoint without overflow", async ({ page }) => {
  for (const [width, collapsed] of [[980, true], [981, false]]) {
    await page.setViewportSize({ width, height: 844 });
    await page.goto("./", { waitUntil: "domcontentloaded" });
    await expectNoHorizontalOverflow(page, "homepage header", width);
    if (collapsed) {
      await expect(page.getByRole("button", { name: "Open navigation" })).toBeVisible();
      await expect(page.getByRole("navigation", { name: "Main navigation" })).toBeHidden();
    } else {
      await expect(page.getByRole("button", { name: "Open navigation" })).toBeHidden();
      await expect(page.getByRole("navigation", { name: "Main navigation" })).toBeVisible();
    }
  }
});

test("all public pages fit phone and tablet viewports", async ({ page }) => {
  for (const width of [390, 768]) {
    await page.setViewportSize({ width, height: 900 });
    for (const route of publicPages) {
      await page.goto(route, { waitUntil: "domcontentloaded" });
      await expect(page.locator("main")).toBeVisible();
      await expect(page.getByRole("heading", { level: 1 }).first()).toBeVisible();
      await expectNoHorizontalOverflow(page, route, width);
    }
  }
});
