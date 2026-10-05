import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

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

test("public pages expose a keyboard skip link", async ({ page }) => {
  for (const route of publicPages) {
    await page.goto(route, { waitUntil: "domcontentloaded" });
    const skipLink = page.locator("a.skip-link");
    await expect(skipLink).toHaveAttribute("href", "#main-content");
    await skipLink.focus();
    await page.keyboard.press("Enter");
    await expect(page.locator("#main-content")).toBeFocused();
    await expect(page.locator("#main-content")).toHaveCSS("outline-width", "2px");
  }
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
    const openMenuButton = page.locator(".menu-toggle");
    await expect(openMenuButton).toHaveAttribute("aria-expanded", "true");
    const docsLink = navigation.getByRole("link", { name: "Learn" });
    await expect(docsLink).toBeVisible();
    await docsLink.click();

    await expect(page).toHaveURL(/\/Ostrin\/docs\.html$/);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expectNoHorizontalOverflow(page, "docs.html", width);
  }
});

test("navigation closes with Escape and restores focus", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("./", { waitUntil: "domcontentloaded" });

  const menuButton = page.locator(".menu-toggle");
  const navigation = page.getByRole("navigation", { name: "Main navigation" });
  await menuButton.focus();
  await page.keyboard.press("Enter");
  await expect(menuButton).toHaveAttribute("aria-expanded", "true");
  await expect(menuButton).toHaveAccessibleName("Close navigation");
  await expect(navigation.getByRole("link", { name: "Learn" })).toBeFocused();

  await page.keyboard.press("Escape");
  await expect(menuButton).toHaveAttribute("aria-expanded", "false");
  await expect(menuButton).toHaveAccessibleName("Open navigation");
  await expect(menuButton).toBeFocused();
});

test("example tabs expose selection and keyboard navigation", async ({ page }) => {
  await page.goto("./examples.html", { waitUntil: "domcontentloaded" });

  const tabs = page.getByRole("tab");
  await expect(tabs).toHaveCount(3);
  await expect(tabs.nth(0)).toHaveAttribute("aria-selected", "true");
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "false");
  await expect(page.locator("#physics-tab")).toBeVisible();
  await expect(page.locator("#concurrency-tab")).toBeHidden();

  await tabs.nth(0).focus();
  await page.keyboard.press("ArrowRight");
  await expect(tabs.nth(1)).toBeFocused();
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#concurrency-tab")).toBeVisible();
  await expect(page.locator("#physics-tab")).toBeHidden();

  await page.keyboard.press("End");
  await expect(tabs.nth(2)).toBeFocused();
  await expect(tabs.nth(2)).toHaveAttribute("aria-selected", "true");
});

test("example filters expose the active state", async ({ page }) => {
  await page.goto("./examples.html", { waitUntil: "domcontentloaded" });
  const filterToolbar = page.getByRole("toolbar", { name: "Filter examples" });
  const filters = filterToolbar.getByRole("button");
  await expect(filters.nth(0)).toHaveAttribute("aria-pressed", "true");
  await filterToolbar.getByRole("button", { name: "Types" }).click();
  await expect(filterToolbar.getByRole("button", { name: "Types" })).toHaveAttribute("aria-pressed", "true");
  await expect(filterToolbar.getByRole("button", { name: "All" })).toHaveAttribute("aria-pressed", "false");
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

test("benchmark page renders the recorded workload report", async ({ page }) => {
  const runtimeErrors = [];
  page.on("pageerror", (error) => runtimeErrors.push(error.message));
  await page.goto("./benchmarks.html", { waitUntil: "domcontentloaded" });
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Measure the work");
  await expect(page.locator("[data-benchmark-state]")).toContainText("Recorded benchmark");
  await expect(page.locator("[data-benchmark-rows] tr")).toHaveCount(8);
  await expect(page.locator("[data-benchmark-commit]")).not.toHaveText("—");
  await expectNoHorizontalOverflow(page, "benchmarks.html", 1280);
  expect(runtimeErrors).toEqual([]);
});

test("provenance page renders the generated reproducibility artifact", async ({ page }) => {
  const runtimeErrors = [];
  page.on("pageerror", (error) => runtimeErrors.push(error.message));
  await page.goto("./provenance.html", { waitUntil: "domcontentloaded" });
  await expect(page.getByRole("heading", { level: 1 })).toContainText("inspectable");
  await expect(page.locator("[data-provenance-schema]")).toHaveText("ostrin.provenance/v0");
  await expect(page.locator("[data-provenance-source]")).toContainText("sha256:");
  await expect(page.locator("[data-provenance-effects]")).toContainText("measurement");
  await expect(page.locator("[data-provenance-level]")).toHaveText("unverified");
  await expect(page.locator("[data-provenance-json]")).toContainText('"source_hash"');
  await expectNoHorizontalOverflow(page, "provenance.html", 1280);
  expect(runtimeErrors).toEqual([]);
});

test("Scientific Lab recomputes its demos with the real compiler", async ({ page }) => {
  const runtimeErrors = [];
  page.on("pageerror", (error) => runtimeErrors.push(error.message));
  await page.goto("./#lab", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-release-line]")).toHaveAttribute("data-state", /published|unreleased/);
  const tabs = page.getByRole("tab");
  await expect(tabs).toHaveCount(12);

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

  await page.getByRole("tab", { name: "Linear Algebra" }).click();
  await expect(page.locator("#lab-panel-linear-algebra .sl-raw pre")).toContainText("QR residual");
  await expect(page.locator("#lab-panel-linear-algebra .sl-raw pre")).toContainText("Q orthogonality");
  await page.locator('[data-lab-run="linear-algebra"]').click();
  await expect(page.locator('[data-lab-provenance="linear-algebra"]')).toHaveAttribute("data-state", "live", { timeout: 30_000 });
  await expect(page.locator("#lab-panel-linear-algebra .sl-raw pre")).toContainText("QR residual = 0");
  await expect(page.locator("#lab-panel-linear-algebra .sl-raw pre")).toContainText("Cholesky residual = 0");
  await expect(page.locator("#lab-panel-linear-algebra .sl-raw pre")).toContainText("Cholesky solve residual = 0");
  await page.getByRole("tab", { name: "Complex" }).click();
  await expect(page.locator("#lab-panel-complex .sl-raw pre")).toContainText("magnitude = 2");
  await page.locator('[data-lab-run="complex"]').click();
  await expect(page.locator('[data-lab-provenance="complex"]')).toHaveAttribute("data-state", "live", { timeout: 30_000 });
  await expect(page.locator("#lab-panel-complex .sl-raw pre")).toContainText("z² =");

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

test("Scientific Lab links preserve the selected demo and parameters", async ({ page }) => {
  await page.goto("./?lab=surface&waves=1.6&azimuth=-30&elevation=24#lab-surface", { waitUntil: "domcontentloaded" });
  await expect(page.getByRole("tab", { name: "3D" })).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#lab-surface-waves")).toHaveValue("1.6");
  await expect(page.locator("#lab-surface-azimuth")).toHaveValue("-30");
  await expect(page.locator("#lab-panel-surface .sl-code")).toContainText("waves = 1.6");

  await page.getByRole("tab", { name: "Plot" }).click();
  await expect(page).toHaveURL(/\?lab=plot[^#]*#lab-plot$/);
  await page.goBack();
  await expect(page.getByRole("tab", { name: "3D" })).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#lab-surface-waves")).toHaveValue("1.6");

  await page.locator('[data-lab-share="surface"]').click();
  await expect(page.locator('#lab-panel-surface .sl-status[aria-live]')).toHaveText(/Link copied|Share URL ready/);
});

test("Cookbook renders every recipe with source and recorded output", async ({ page }) => {
  await page.goto("./cookbook.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-cookbook] .recipe")).toHaveCount(12);
  for (const recipe of await page.locator("[data-cookbook] .recipe").all()) {
    await expect(recipe.locator(".sl-code")).not.toBeEmpty();
    await expect(recipe.locator(".sl-raw pre")).not.toBeEmpty();
    const source = await recipe.getByRole("link", { name: /View source/ }).getAttribute("href");
    expect(source.startsWith("https://github.com/sircalch/Ostrin/blob/main/examples/")).toBe(true);
  }
});

test("Viz gallery shows recorded figures and reruns them with the real compiler", async ({ page }) => {
  test.setTimeout(60_000);
  const runtimeErrors = [];
  page.on("pageerror", (error) => runtimeErrors.push(error.message));
  await page.goto("./viz.html", { waitUntil: "domcontentloaded" });
  const cards = page.locator("[data-viz-gallery] .viz-card");
  await expect(cards).toHaveCount(29);
  await expect(page.getByRole("heading", { name: "Data table" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Linked data selection" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "3D vector field" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "3D volume slices" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "3D isosurface" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Kernel-density violins" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Hexbin density" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Filled contour bands" })).toBeVisible();
  await expect(page.locator(".viz-status")).toContainText("print-ready PDF export");
  await expect(page.locator("#viz-provenance .sl-provenance")).toContainText("source sha256:");
  const bundleDownloadPromise = page.waitForEvent("download");
  await page.locator("#viz-provenance").getByRole("button", { name: "Download experiment bundle" }).click();
  const bundleDownload = await bundleDownloadPromise;
  expect(bundleDownload.suggestedFilename()).toBe("reproducible-provenance.ostrin-experiment.json");
  const bundle = JSON.parse(await readFile(await bundleDownload.path(), "utf8"));
  expect(bundle.schema).toBe("ostrin.experiment/v0");
  expect(bundle.reproducibility.level).toBe("R0");
  for (const entry of bundle.manifest.files) {
    expect(entry.sha256).toBe(`sha256:${createHash("sha256").update(bundle.files[entry.path], "utf8").digest("hex")}`);
  }
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
  const tableRun = page.locator('[data-viz-run="table"]');
  await expect(tableRun).toBeEnabled();
  await tableRun.click();
  await expect(page.locator("#viz-table .sl-provenance")).toHaveAttribute("data-state", "live", { timeout: 30_000 });
  await expect(page.locator("#viz-table img.viz-image")).toHaveAttribute("src", /^data:image\/svg\+xml/);

  // The explorer shows the SVG in a sandboxed frame where its own tooltips and hover styles work.
  await page.locator('[data-viz-explore="scatter-fit"]').click();
  const scatterFrame = page.frameLocator(".viz-frame-live");
  const point = scatterFrame.locator("circle.pt").first();
  await expect(point.locator("title")).toHaveText(/^measurements: \(0\.5, /);
  await expect(page.locator(".viz-legend-tools")).toBeVisible();
  const firstLegendToggle = page.locator("[data-viz-legend-toggle]").first();
  const firstSeriesId = await firstLegendToggle.getAttribute("data-viz-legend-toggle");
  const firstSeries = scatterFrame.locator(`.viz-series[data-viz-series-id="${firstSeriesId}"]`);
  await expect(firstSeries).toBeVisible();
  await firstLegendToggle.click();
  await expect(firstSeries).toHaveCSS("display", "none");
  await expect(firstLegendToggle).toHaveAttribute("aria-pressed", "false");
  await expect(page.locator(".viz-legend-status")).toHaveText(/2 of 3 series visible/);
  await firstLegendToggle.click();
  await expect(firstSeries).toBeVisible();
  await expect(firstLegendToggle).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".viz-crosshair-tools")).toBeVisible();
  const crosshairToggle = page.locator("[data-viz-crosshair-toggle]");
  await crosshairToggle.press("Enter");
  await expect(crosshairToggle).toHaveAttribute("aria-pressed", "true");
  await point.hover();
  await expect(page.locator(".viz-crosshair-status")).toHaveText(/measurements: \(/);
  await expect(scatterFrame.locator(".viz-crosshair-overlay")).toHaveAttribute("visibility", "visible");
  await crosshairToggle.press("Enter");
  await expect(crosshairToggle).toHaveAttribute("aria-pressed", "false");
  await expect(page.locator(".viz-crosshair-status")).toHaveText("Crosshair disabled.");
  const svgDownloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download SVG" }).click();
  const svgDownload = await svgDownloadPromise;
  expect(svgDownload.suggestedFilename()).toMatch(/\.svg$/);
  const pngDownloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export PNG" }).click();
  const pngDownload = await pngDownloadPromise;
  expect(pngDownload.suggestedFilename()).toMatch(/\.png$/);
  const pngBytes = await readFile(await pngDownload.path());
  expect([...pngBytes.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
  expect(pngBytes.readUInt32BE(16)).toBe(1280);
  expect(pngBytes.readUInt32BE(20)).toBe(800);
  await expect(page.locator(".viz-export-status")).toHaveText(/PNG downloaded · \d+×\d+/);
  const htmlDownloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export interactive HTML" }).click();
  const htmlDownload = await htmlDownloadPromise;
  expect(htmlDownload.suggestedFilename()).toMatch(/\.html$/);
  const htmlText = await readFile(await htmlDownload.path(), "utf8");
  expect(htmlText).toContain('data-ostrin-export="interactive"');
  expect(htmlText).toContain('data-ostrin-provenance="true"');
  expect(htmlText).toContain("ostrin-export-crosshair");
  expect(htmlText).toContain("<svg");
  await expect(page.locator(".viz-export-status")).toHaveText(/HTML downloaded · interactive · \d+×\d+/);
  const exportedPage = await page.context().newPage();
  await exportedPage.setContent(htmlText);
  await exportedPage.getByRole("button", { name: "Zoom in" }).click();
  await expect(exportedPage.locator("#zoom")).toHaveText("125%");
  await exportedPage.getByRole("button", { name: "Enable crosshair" }).click();
  await exportedPage.locator("circle.pt").first().hover();
  await expect(exportedPage.locator("#status")).toHaveText(/measurements: \(/);
  await expect(exportedPage.locator(".ostrin-export-crosshair")).toHaveAttribute("visibility", "visible");
  await exportedPage.close();
  const pdfPopupPromise = page.waitForEvent("popup");
  await page.getByRole("button", { name: "Print figure as PDF" }).click();
  const pdfPopup = await pdfPopupPromise;
  await pdfPopup.waitForLoadState("domcontentloaded");
  await expect(pdfPopup.locator("svg")).toBeVisible();
  await expect(pdfPopup.locator("h1")).toHaveText("Scatter and fit");
  await expect(pdfPopup.locator("p")).toContainText("No reproducibility metadata recorded.");
  await expect(page.locator(".viz-export-status")).toHaveText(/PDF print view opened · \d+×\d+/);
  await pdfPopup.close();
  await page.getByRole("button", { name: "Zoom in" }).click();
  await expect(page.locator(".viz-zoom")).toHaveText("150%");
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="table"]').click();
  await expect(page.locator(".viz-table-tools")).toBeVisible();
  const tableFrame = page.frameLocator(".viz-frame-live");
  await expect(tableFrame.locator("rect.table-cell")).toHaveCount(16);
  await expect(tableFrame.locator("text=ODE solver comparison")).toHaveCount(1);
  await page.locator("#viz-table-filter").fill("BDF");
  await expect(tableFrame.locator('g.table-row:not([style*="display: none"])')).toHaveCount(1);
  await expect(tableFrame.locator("[data-table-footer]")).toHaveText("1 of 4 rows · 4 columns");
  await page.locator("#viz-table-filter").fill("");
  await page.locator("#viz-table-sort").selectOption("1");
  await expect(tableFrame.locator("g.table-row").first().locator("title").first()).toHaveText("BDF");
  await page.locator("[data-viz-table-direction]").click();
  await expect(tableFrame.locator("g.table-row").first().locator("title").first()).toHaveText("RK4");
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="surface"]').click();
  const surfaceScale = page.locator('[data-viz-parameter="scale"]');
  await expect(surfaceScale).toBeVisible();
  await surfaceScale.fill("1.6");
  const surfaceFrame = page.frameLocator(".viz-frame-live");
  await expect(page.locator(".viz-camera-status")).toContainText("Spatial scale 1.6", { timeout: 30_000 });
  await expect(surfaceFrame.locator("text").filter({ hasText: "scale = 1.6" })).toHaveCount(1);
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="lorenz"]').click();
  const lorenzRho = page.locator('[data-viz-parameter="rho"]');
  await expect(lorenzRho).toBeVisible();
  await lorenzRho.fill("34");
  const lorenzFrame = page.frameLocator(".viz-frame-live");
  await expect(page.locator(".viz-camera-status")).toContainText("ρ (Rayleigh parameter) 34", { timeout: 30_000 });
  await expect(lorenzFrame.locator("text").filter({ hasText: "ρ = 34" })).toHaveCount(1);
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="linked-data"]').click();
  await expect(page.locator(".viz-selection-tools")).toBeVisible();
  await expect(page.locator(".viz-contract-tools")).toBeVisible();
  const linkedFrame = page.frameLocator(".viz-frame-live");
  await expect(linkedFrame.locator("circle.pt[data-viz-index]")).toHaveCount(5);
  await expect(linkedFrame.locator("g.table-row[data-viz-index]")).toHaveCount(5);
  await linkedFrame.locator('circle.pt[data-viz-index="2"]').hover();
  await expect(linkedFrame.locator('g.table-row[data-viz-index="2"]')).toHaveClass(/viz-contract-hover/);
  await expect(page.locator(".viz-contract-status")).toContainText("calibration · hover row 3");
  await linkedFrame.locator('g.table-row[data-viz-index="2"]').click();
  await expect(linkedFrame.locator('circle.pt[data-viz-index="2"]')).toHaveClass(/viz-linked-selected/);
  await expect(linkedFrame.locator('g.table-row[data-viz-index="2"]')).toHaveClass(/viz-linked-selected/);
  await expect(page.locator(".viz-selection-status")).toHaveText("Selected row 3 of 5");
  await linkedFrame.locator('circle.pt[data-viz-index="1"]').press("Enter");
  await expect(page.locator(".viz-selection-status")).toHaveText("Selected row 2 of 5");
  await expect(linkedFrame.locator('g.table-row[data-viz-index="1"]')).toHaveClass(/viz-linked-selected/);
  await page.getByRole("button", { name: "Clear selection" }).click();
  await expect(linkedFrame.locator('circle.pt[data-viz-index="2"]')).not.toHaveClass(/viz-linked-selected/);
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="animation"]').click();
  await expect(page.locator(".viz-animation-tools")).toBeVisible();
  const mp4Button = page.locator("[data-viz-mp4-export]");
  await expect(mp4Button).toHaveCount(1);
  if (await mp4Button.isDisabled()) {
    await expect(mp4Button).toHaveAttribute("title", /does not expose an MP4 MediaRecorder codec/);
  } else {
    await expect(mp4Button).not.toHaveAttribute("title", /does not expose an MP4 MediaRecorder codec/);
  }
  await page.getByRole("button", { name: "Pause" }).click();
  await page.locator("[data-viz-loop]").selectOption("1");
  await expect(page.locator("[data-viz-loop]")).toHaveValue("1");
  await page.getByRole("button", { name: "Restart" }).click();
  await expect(page.locator(".viz-animation-status")).toHaveText("completed 1 loop", { timeout: 10_000 });
  const animationQuality = page.locator("[data-viz-quality]");
  await expect(animationQuality).toHaveValue("1");
  await expect(animationQuality.locator("option")).toHaveCount(2);
  await page.locator("[data-viz-loop]").selectOption("3");
  await expect(page.locator(".viz-animation-status")).toHaveText("up to 3 loops");
  await page.locator(".viz-animation-speed").fill("200");
  await expect(page.locator(".viz-animation-speed-label")).toHaveText("2×");
  await page.locator(".viz-animation-speed").fill("100");
  await page.locator(".viz-animation-slider").fill("500");
  await expect(page.locator(".viz-animation-time")).toHaveText("50%");
  const downloadPromise = page.waitForEvent("download", { timeout: 30_000 });
  await page.getByRole("button", { name: "Export WebM" }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("animation.webm");
  await expect(page.locator(".viz-animation-status")).toHaveText("72 frames · 8 fps · downloaded");
  await page.locator("[data-viz-loop]").selectOption("1");
  await animationQuality.selectOption("2");
  const gifDownloadPromise = page.waitForEvent("download", { timeout: 30_000 });
  await page.getByRole("button", { name: "Export GIF" }).click();
  const gifDownload = await gifDownloadPromise;
  expect(gifDownload.suggestedFilename()).toBe("animation-2x.gif");
  const gifBytes = await readFile(await gifDownload.path());
  expect(gifBytes.subarray(0, 6).toString("ascii")).toBe("GIF89a");
  expect(gifBytes.readUInt16LE(6)).toBe(1280);
  expect(gifBytes.readUInt16LE(8)).toBe(800);
  const decodedGif = await page.evaluate((base64) => new Promise((resolve) => {
    const image = new Image();
    image.onload = () => resolve({ width: image.naturalWidth, height: image.naturalHeight });
    image.onerror = () => resolve({ error: "browser could not decode GIF" });
    image.src = `data:image/gif;base64,${base64}`;
  }), gifBytes.toString("base64"));
  expect(decodedGif).toEqual({ width: 1280, height: 800 });
  await expect(page.locator(".viz-animation-status")).toHaveText("24 frames · 8 fps · downloaded · 2×");
  await page.getByRole("button", { name: "Close" }).click();
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.locator('[data-viz-explore="double-pendulum"]').click();
  const motionFrame = page.frameLocator(".viz-frame-live");
  await expect(motionFrame.locator('svg[data-ostrin-motion] animate')).not.toHaveCount(0);
  await expect(page.locator(".viz-animation-status")).toHaveText("reduced motion");
  await expect(motionFrame.locator("animate").first()).toHaveCSS("display", "none");
  await page.getByRole("button", { name: "Close" }).click();
  await page.emulateMedia({ reducedMotion: null });
  await page.locator('[data-viz-explore="vector-field"]').click();
  const vectorFrame = page.frameLocator(".viz-frame-live");
  await expect(vectorFrame.locator("g.vector3")).toHaveCount(25);
  await expect(vectorFrame.locator("g.vector3 line title").first()).toContainText("→");
  const azimuth = page.locator("[data-viz-camera-azimuth]");
  await expect(azimuth).toBeVisible();
  const originalX = await vectorFrame.locator("g.vector3 line").first().getAttribute("x1");
  await azimuth.evaluate((input) => {
    input.value = "30";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await expect(page.locator(".viz-camera-status")).toContainText("Rendered by Ostrin · azimuth 30°", { timeout: 30_000 });
  await expect(vectorFrame.locator("g.vector3")).toHaveCount(25);
  await expect.poll(() => vectorFrame.locator("g.vector3 line").first().getAttribute("x1")).not.toBe(originalX);
  await page.getByRole("button", { name: "Reset view" }).click();
  await expect(page.locator(".viz-camera-status")).toContainText("azimuth -48°", { timeout: 30_000 });
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="volume-slices"]').click();
  const volumeFrame = page.frameLocator(".viz-frame-live");
  await expect(volumeFrame.locator("polygon.slice-cell")).toHaveCount(768);
  await expect(volumeFrame.locator("polygon.slice-cell title").first()).toContainText("value");
  await expect(page.locator("[data-viz-camera-azimuth]")).toBeVisible();
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="isosurface"]').click();
  const isoFrame = page.frameLocator(".viz-frame-live");
  await expect(isoFrame.locator("polygon.isosurface-cell")).toHaveCount(4536);
  await expect(isoFrame.locator("polygon.isosurface-cell title").first()).toContainText("isosurface level");
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="violin"]').click();
  const violinFrame = page.frameLocator(".viz-frame-live");
  await expect(violinFrame.locator("path.violin")).toHaveCount(3);
  await expect(violinFrame.locator("path.violin title").first()).toContainText("median");
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="contourf"]').click();
  const contourfFrame = page.frameLocator(".viz-frame-live");
  await expect(contourfFrame.locator("rect.contourf-cell")).toHaveCount(47 * 47);
  await expect(contourfFrame.locator("rect.contourf-cell title").first()).toContainText("band");
  await expect(contourfFrame.locator("path")).toHaveCount(9);
  await expect(contourfFrame.locator("linearGradient")).toHaveCount(1);
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="quiver"]').click();
  const quiverFrame = page.frameLocator(".viz-frame-live");
  await expect(quiverFrame.locator(".quiver")).toHaveCount(17 * 13);
  await expect(quiverFrame.locator("g.quiver polygon")).toHaveCount(17 * 13 - 1);
  await expect(quiverFrame.locator(".quiver title").first()).toContainText("velocity");
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="streamplot"]').click();
  const streamFrame = page.frameLocator(".viz-frame-live");
  await expect(streamFrame.locator("path.streamplot")).toHaveCount(9 * 7);
  await expect(streamFrame.locator("path.streamplot title").first()).toContainText("flow streamline");
  await page.getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-explore="hexbin"]').click();
  const hexbinFrame = page.frameLocator(".viz-frame-live");
  await expect(hexbinFrame.locator("polygon.hexbin-cell")).toHaveCount(252);
  await expect(hexbinFrame.locator("polygon.hexbin-cell title").first()).toContainText("count");
  await expect(hexbinFrame.locator("linearGradient")).toHaveCount(1);
  await page.getByRole("button", { name: "Close" }).click();
  expect(runtimeErrors).toEqual([]);
});

test("Viz gallery defers the compiler download until a live action", async ({ page }) => {
  const wasmRequests = [];
  page.on("request", (request) => {
    if (request.url().includes("/ostrinc.wasm")) wasmRequests.push(request.url());
  });
  await page.goto("./viz.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-viz-gallery] .viz-card")).toHaveCount(29);
  await expect(page.locator(".viz-runtime-note")).toContainText("Recorded SVGs load immediately");
  expect(wasmRequests).toEqual([]);
});

test("Viz figure manifests preserve source and recorded provenance", async ({ page }) => {
  await page.goto("./viz.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("[data-viz-gallery] .viz-card")).toHaveCount(29);
  const manifestDownloadPromise = page.waitForEvent("download");
  await page.locator('[data-viz-manifest="provenance"]').click();
  const manifestDownload = await manifestDownloadPromise;
  expect(manifestDownload.suggestedFilename()).toBe("reproducible-provenance.ostrin-figure.json");
  const manifest = JSON.parse(await readFile(await manifestDownload.path(), "utf8"));
  expect(manifest.schema).toBe("ostrin.figure/v0");
  expect(manifest.id).toBe("provenance");
  expect(manifest.source).toBe("examples/viz_provenance.ostrin");
  expect(manifest.svg).toBe("assets/viz/provenance.svg");
  expect(manifest.capabilities).toContain("reproducibility");
  expect(manifest.selected_state).toBeNull();
  expect(manifest.citation).toMatchObject({
    type: "software-figure",
    author: "Ostrin project",
    container: "Ostrin Viz",
    title: "Reproducible provenance",
    source: "examples/viz_provenance.ostrin",
  });
  expect(manifest.citation.url).toContain("viz.html?figure=provenance");
  expect(manifest.provenance.source_hash).toMatch(/^sha256:/);
});

test("Viz figure manifests preserve the selected explorer state", async ({ page }) => {
  await page.goto("./viz.html?figure=surface&scale=1.6&azimuth=30&elevation=36#viz-surface", { waitUntil: "domcontentloaded" });
  await expect(page.locator(".viz-dialog[open]")).toBeVisible();
  await expect(page.locator('[data-viz-parameter="scale"]')).toHaveValue("1.6");
  await expect(page.locator("[data-viz-camera-azimuth]")).toHaveValue("30");
  await expect(page.locator("[data-viz-camera-elevation]")).toHaveValue("36");
  await page.getByRole("button", { name: "Close" }).click();

  const manifestDownloadPromise = page.waitForEvent("download");
  await page.locator('[data-viz-manifest="surface"]').click();
  const manifestDownload = await manifestDownloadPromise;
  const manifest = JSON.parse(await readFile(await manifestDownload.path(), "utf8"));
  expect(manifest.selected_state.parameters).toEqual({ scale: 1.6 });
  expect(manifest.selected_state.camera).toEqual({ azimuth: 30, elevation: 36 });
  expect(manifest.selected_state.share_url).toContain("figure=surface");
  expect(manifest.selected_state.share_url).toContain("scale=1.6");
});

test("Viz exposes a citation handoff for the selected figure state", async ({ page }) => {
  await page.goto("./viz.html?figure=surface&scale=1.6&azimuth=30&elevation=36#viz-surface", { waitUntil: "domcontentloaded" });
  await expect(page.locator(".viz-dialog[open]")).toBeVisible();
  await page.locator(".viz-dialog[open]").getByRole("button", { name: "Close" }).click();
  await page.locator('[data-viz-citation="surface"]').click();

  const citation = page.locator('.viz-citation-dialog[open]');
  await expect(citation).toBeVisible();
  await expect(citation).toContainText("@misc{ostrin_surface");
  await expect(citation).toContainText("examples/viz_surface.ostrin");
  await expect(citation).toContainText("scale=1.6");
  await expect(citation).toContainText("azimuth=30");
});

test("Viz capability filters are shareable and expose their gallery target", async ({ page }) => {
  await page.goto("./viz.html?capability=3d", { waitUntil: "domcontentloaded" });
  const gallery = page.locator("[data-viz-gallery]");
  const activeCards = gallery.locator('.viz-card:not([hidden])');
  const threeD = page.locator('[data-viz-capability="3d"]');
  await expect(activeCards).toHaveCount(7);
  await expect(threeD).toHaveAttribute("aria-pressed", "true");
  await expect(threeD).toHaveAttribute("aria-controls", "viz-gallery");
  await expect(page.locator("[data-viz-capability-status]")).toHaveText("7 3d scenes shown");
  await page.locator('[data-viz-capability="all"]').click();
  await expect(activeCards).toHaveCount(29);
  await expect(page).toHaveURL(/viz\.html$/);
});

test("Viz exposes the recorded 3D surface experiment bundle", async ({ page }) => {
  await page.goto("./viz.html", { waitUntil: "domcontentloaded" });
  const bundleDownloadPromise = page.waitForEvent("download");
  await page.locator('[data-viz-bundle="surface"]').click();
  const bundleDownload = await bundleDownloadPromise;
  expect(bundleDownload.suggestedFilename()).toBe("shaded-3d-surface.ostrin-experiment.json");
  const bundle = JSON.parse(await readFile(await bundleDownload.path(), "utf8"));
  expect(bundle.id).toBe("surface");
  expect(bundle.provenance.parameters).toEqual({ scale: 1 });
  expect(bundle.provenance.camera).toEqual({ azimuth: -55, elevation: 28 });
  expect(bundle.reproducibility.level).toBe("R0");
  expect(bundle.files["figure.svg"]).toContain("<ostrin-provenance");
  for (const entry of bundle.manifest.files) {
    expect(entry.sha256).toBe(`sha256:${createHash("sha256").update(bundle.files[entry.path], "utf8").digest("hex")}`);
  }
});

test("Viz exposes recorded animation, table and volume experiment bundles", async ({ page }) => {
  await page.goto("./viz.html", { waitUntil: "domcontentloaded" });

  const animationDownloadPromise = page.waitForEvent("download");
  await page.locator('[data-viz-bundle="animation"]').click();
  const animationDownload = await animationDownloadPromise;
  expect(animationDownload.suggestedFilename()).toBe("animation.ostrin-experiment.json");
  const animationBundle = JSON.parse(await readFile(await animationDownload.path(), "utf8"));
  expect(animationBundle.id).toBe("animation");
  expect(animationBundle.provenance.parameters).toEqual({ frames: 24, fps: 8, samples: 160 });
  expect(animationBundle.reproducibility.level).toBe("R0");

  const tableDownloadPromise = page.waitForEvent("download");
  await page.locator('[data-viz-bundle="table"]').click();
  const tableDownload = await tableDownloadPromise;
  expect(tableDownload.suggestedFilename()).toBe("data-table.ostrin-experiment.json");
  const tableBundle = JSON.parse(await readFile(await tableDownload.path(), "utf8"));
  expect(tableBundle.id).toBe("table");
  expect(tableBundle.provenance.parameters).toEqual({ rows: 4, columns: 4 });
  expect(tableBundle.reproducibility.level).toBe("R0");

  const volumeDownloadPromise = page.waitForEvent("download");
  await page.locator('[data-viz-bundle="volume-slices"]').click();
  const volumeDownload = await volumeDownloadPromise;
  expect(volumeDownload.suggestedFilename()).toBe("3d-volume-slices.ostrin-experiment.json");
  const volumeBundle = JSON.parse(await readFile(await volumeDownload.path(), "utf8"));
  expect(volumeBundle.id).toBe("volume-slices");
  expect(volumeBundle.provenance.parameters).toEqual({ grid: 17, slice: 8 });
  expect(volumeBundle.provenance.camera).toEqual({ azimuth: -48, elevation: 28 });
  expect(volumeBundle.reproducibility.level).toBe("R0");
});

test("Viz publishes structured data for its scientific source", async ({ page }) => {
  await page.goto("./viz.html", { waitUntil: "domcontentloaded" });
  const structuredData = await page.locator('script[type="application/ld+json"]').evaluateAll((scripts) =>
    scripts.map((script) => JSON.parse(script.textContent)),
  );
  const pageSchema = structuredData.find((entry) => entry["@type"] === "CollectionPage");
  expect(pageSchema).toMatchObject({
    "@context": "https://schema.org",
    "@id": "https://sircalch.github.io/Ostrin/viz.html#page",
    url: "https://sircalch.github.io/Ostrin/viz.html",
    about: {
      "@type": "SoftwareSourceCode",
      name: "std.viz",
      codeRepository: "https://github.com/sircalch/Ostrin/blob/main/compiler/std/viz.ostrin",
      programmingLanguage: { "@type": "ComputerLanguage", name: "Ostrin" },
    },
    mainEntity: { "@id": "https://sircalch.github.io/Ostrin/viz.html#gallery" },
  });
  const gallerySchema = structuredData.find((entry) => entry["@type"] === "ItemList");
  expect(gallerySchema).toMatchObject({
    "@context": "https://schema.org",
    "@id": "https://sircalch.github.io/Ostrin/viz.html#gallery",
    numberOfItems: 29,
  });
  expect(gallerySchema.itemListElement).toHaveLength(29);
  expect(gallerySchema.itemListElement[1]).toMatchObject({
    position: 2,
    item: {
      "@type": "ImageObject",
      name: "Shaded 3D surface",
      contentUrl: "https://sircalch.github.io/Ostrin/assets/viz/surface.svg",
      isBasedOn: "https://github.com/sircalch/Ostrin/blob/main/examples/viz_surface.ostrin",
    },
  });
});

test("Viz explorer links restore parameters and 3D camera", async ({ page }) => {
  await page.goto("./viz.html?figure=surface&scale=1.6&azimuth=30&elevation=36#viz-surface", { waitUntil: "domcontentloaded" });
  await expect(page.locator(".viz-dialog[open]")).toBeVisible();
  await expect(page.locator('[data-viz-parameter="scale"]')).toHaveValue("1.6");
  await expect(page.locator("[data-viz-camera-azimuth]")).toHaveValue("30");
  await expect(page.locator("[data-viz-camera-elevation]")).toHaveValue("36");
  await expect(page.locator(".viz-camera-status")).toContainText("Spatial scale 1.6", { timeout: 30_000 });

  await page.getByRole("button", { name: "Copy share link" }).click();
  await expect(page.locator(".viz-dialog .viz-share-status")).toHaveText(/Link copied|Share URL ready/);
});

test("Featured Viz workflows select source-backed paths", async ({ page }) => {
  await page.goto("./viz.html?workflow=analyze", { waitUntil: "domcontentloaded" });
  const workflows = page.locator("[data-viz-workflows] [data-viz-workflow]");
  await expect(workflows).toHaveCount(3);
  await expect(page.locator('[data-viz-workflow="analyze"]')).toHaveAttribute("data-active", "true");
  await expect(page.locator('[data-viz-workflow="analyze"] .viz-workflow-status')).toHaveText("experimental");
  await expect(page.locator('[data-viz-workflow="analyze"] .viz-workflow-step')).toHaveCount(4);
  await expect(page.locator('[data-viz-workflow="analyze"] [data-viz-workflow-source="scatter-fit"]')).toHaveAttribute(
    "href",
    "https://github.com/sircalch/Ostrin/blob/main/examples/viz_scatter_fit.ostrin",
  );
  await expect(page.locator('[data-viz-workflow="analyze"] [data-viz-workflow-step="table"]')).toHaveAttribute(
    "href",
    /workflow=analyze&figure=table#viz-table$/,
  );
  await expect(page.locator(".viz-workflow-selection")).toHaveText(/Analyze data selected/);

  await page.locator('[data-viz-workflow-select="simulate"]').click();
  await expect(page).toHaveURL(/workflow=simulate#workflow-simulate$/);
  await expect(page.locator('[data-viz-workflow="simulate"]')).toHaveAttribute("data-active", "true");
  await expect(page.locator('[data-viz-workflow="analyze"]')).toHaveAttribute("data-active", "false");
  await expect(page.locator(".viz-workflow-selection")).toHaveText(/Simulate a system selected/);
});
