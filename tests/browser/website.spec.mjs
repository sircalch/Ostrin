import { expect, test } from "@playwright/test";

const publicPages = [
  "./",
  "./community.html",
  "./cookbook.html",
  "./docs.html",
  "./ecosystem.html",
  "./examples.html",
  "./guides.html",
  "./language.html",
  "./playground.html",
  "./reference.html",
  "./roadmap.html",
  "./showcase.html",
  "./viz.html",
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
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Scientific-first. General-purpose.");
  await expect(page.getByLabel("Example program")).toBeVisible();
  await expectNoHorizontalOverflow(page, "homepage desktop", 1280);
  await expect(page.locator("vite-error-overlay, .nextjs-portal, #webpack-dev-server-client-overlay")).toHaveCount(0);

  const runButton = page.locator("#run");
  await expect(runButton).toBeEnabled({ timeout: 45_000 });
  await runButton.click();
  await expect(page.locator("#output")).toContainText("5 m/s", { timeout: 30_000 });

  const source = page.getByLabel("Ostrin source");
  await source.fill("fn main() -> Void {\n    print(1 + true)\n}\n");
  await page.locator("#check").click();
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
    const docsLink = navigation.getByRole("link", { name: "Learn" });
    await expect(docsLink).toBeVisible();
    await docsLink.click();

    await expect(page).toHaveURL(/\/Ostrin\/docs\.html$/);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expectNoHorizontalOverflow(page, "docs.html", width);
  }
});

test("header switches at its tablet breakpoint without overflow", async ({ page }) => {
  for (const [width, collapsed] of [[1000, true], [1001, false]]) {
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

test("Scientific Lab recomputes its demos with the real compiler", async ({ page }) => {
  const runtimeErrors = [];
  page.on("pageerror", (error) => runtimeErrors.push(error.message));
  await page.goto("./#lab", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-release-line]")).toHaveAttribute("data-state", /published|unreleased/);
  const tabs = page.getByRole("tab");
  await expect(tabs).toHaveCount(9);

  // Recorded output first, then a live run with the same compiler must reproduce it exactly.
  await page.getByRole("tab", { name: "Units" }).click();
  const provenance = page.locator('[data-lab-provenance="units"]');
  await expect(provenance).toHaveAttribute("data-state", "recorded");
  const recorded = await page.locator('#lab-panel-units .sl-raw pre').textContent();
  const run = page.locator('[data-lab-run="units"]');
  await expect(run).toBeEnabled({ timeout: 45_000 });
  await run.click();
  await expect(provenance).toHaveAttribute("data-state", "live", { timeout: 30_000 });
  await expect(page.locator('#lab-panel-units .sl-raw pre')).toHaveText(recorded);

  // A control rewrites the source and Ostrin recomputes the result.
  await page.getByRole("tab", { name: "Monte Carlo" }).click();
  const samples = page.locator("#lab-monte-carlo-samples");
  await expect(page.locator('[data-lab-run="monte-carlo"]')).toBeEnabled();
  await samples.fill("40000");
  await expect(page.locator("#lab-panel-monte-carlo .sl-code")).toContainText("samples = 40000");
  await expect(page.locator('[data-lab-provenance="monte-carlo"]')).toHaveAttribute("data-state", "live", { timeout: 30_000 });
  await expect(page.locator('#lab-panel-monte-carlo .sl-raw pre')).toContainText("estimate 40000 ");
  await expect(page.locator("#lab-panel-monte-carlo .sl-chart")).toBeVisible();

  // std.viz's SVG is shown as an image; the 3D view recomputes when the camera moves.
  await page.getByRole("tab", { name: "Plot" }).click();
  await page.locator('[data-lab-run="plot"]').click();
  await expect(page.locator('[data-lab-provenance="plot"]')).toHaveAttribute("data-state", "live", { timeout: 30_000 });
  await expect(page.locator('#lab-panel-plot img.sl-svg')).toBeVisible();
  await page.getByRole("tab", { name: "3D" }).click();
  await page.locator("#lab-surface-azimuth").fill("-20");
  await expect(page.locator("#lab-panel-surface .sl-code")).toContainText("azimuth = -20.0");
  await expect(page.locator('[data-lab-provenance="surface"]')).toHaveAttribute("data-state", "live", { timeout: 45_000 });
  await expect(page.locator('#lab-panel-surface img.sl-svg')).toBeVisible();

  await expect(page.locator("[data-pipeline] .pipeline-stage")).toHaveCount(4);
  await expect(page.locator("[data-pipeline]")).toContainText("ostrin_fn_kinetic");
  expect(runtimeErrors).toEqual([]);
});

test("Cookbook renders every recipe with source and recorded output", async ({ page }) => {
  await page.goto("./cookbook.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-cookbook] .recipe")).toHaveCount(9);
  for (const recipe of await page.locator("[data-cookbook] .recipe").all()) {
    await expect(recipe.locator(".sl-code")).not.toBeEmpty();
    await expect(recipe.locator(".sl-raw pre")).not.toBeEmpty();
    const source = await recipe.getByRole("link", { name: /View source/ }).getAttribute("href");
    expect(source.startsWith("https://github.com/sircalch/Ostrin/blob/main/examples/")).toBe(true);
  }
});

test("Viz gallery shows recorded figures and reruns them with the real compiler", async ({ page }) => {
  const runtimeErrors = [];
  page.on("pageerror", (error) => runtimeErrors.push(error.message));
  await page.goto("./viz.html", { waitUntil: "domcontentloaded" });
  const cards = page.locator("[data-viz-gallery] .viz-card");
  await expect(cards).toHaveCount(10);
  for (const card of await cards.all()) {
    const image = card.locator("img.viz-image");
    await expect(image).toHaveAttribute("src", /^assets\/viz\/[a-z-]+\.svg$/);
    const source = await card.getByRole("link", { name: /View source/ }).getAttribute("href");
    expect(source).toMatch(/^https:\/\/github\.com\/sircalch\/Ostrin\/blob\/main\/examples\/viz_[a-z_]+\.ostrin$/);
  }
  const run = page.locator('[data-viz-run="units"]');
  await expect(run).toBeEnabled({ timeout: 45_000 });
  await run.click();
  await expect(page.locator("#viz-units .sl-provenance")).toHaveAttribute("data-state", "live", { timeout: 30_000 });
  await expect(page.locator("#viz-units img.viz-image")).toHaveAttribute("src", /^data:image\/svg\+xml/);
  await expect(page.locator("#viz-units .viz-printed")).toHaveText("top speed 97.91999999999999 km/h, distance 0.764 km");
  expect(runtimeErrors).toEqual([]);
});
