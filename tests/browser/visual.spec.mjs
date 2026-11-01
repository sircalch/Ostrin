import { expect, test } from "@playwright/test";

const visualSurfaces = [
  { id: "homepage", route: "./" },
  { id: "scientific-lab", route: "./", selector: "#lab" },
  { id: "viz-gallery", route: "./viz.html" },
  { id: "playground", route: "./playground.html" },
  { id: "provenance", route: "./provenance.html" },
  { id: "benchmarks", route: "./benchmarks.html" },
];

const viewportProfiles = [
  { id: "desktop", width: 1440, height: 960 },
  { id: "tablet", width: 768, height: 1024 },
  { id: "mobile", width: 390, height: 844 },
];

const screenshotOptions = {
  animations: "disabled",
  caret: "hide",
  scale: "css",
  threshold: 0.3,
  maxDiffPixelRatio: 0.015,
};

async function stabilizePage(page) {
  await page.emulateMedia({ reducedMotion: "reduce", colorScheme: "light" });
  await page.addStyleTag({
    content: "*,*::before,*::after{caret-color:transparent!important;scroll-behavior:auto!important}",
  });
  await page.evaluate(async () => {
    if (document.fonts?.ready) await document.fonts.ready;
    const visibleImages = [...document.images].filter((image) => {
      const bounds = image.getBoundingClientRect();
      return bounds.bottom > 0 && bounds.right > 0
        && bounds.top < window.innerHeight && bounds.left < window.innerWidth;
    });
    await Promise.all(visibleImages.map((image) => image.decode().catch(() => {})));
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  });
}

test("representative public surfaces match desktop, tablet and mobile visual baselines", async ({ page }) => {
  test.setTimeout(120_000);

  for (const viewport of viewportProfiles) {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    for (const surface of visualSurfaces) {
      await page.goto(surface.route, { waitUntil: "domcontentloaded" });
      await stabilizePage(page);
      await expect(page.locator("main")).toBeVisible();
      if (surface.selector) {
        const target = page.locator(surface.selector);
        await expect(target.getByRole("tab").first()).toBeVisible();
        await expect(target).toBeVisible();
        await expect(target).toHaveScreenshot(`${surface.id}-${viewport.id}.png`, screenshotOptions);
      } else {
        await expect(page.locator("main")).toBeVisible();
        await expect(page).toHaveScreenshot(`${surface.id}-${viewport.id}.png`, screenshotOptions);
      }
    }
  }
});

test("scientific plot visual fixtures match their published gallery cards", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.goto("./viz.html", { waitUntil: "domcontentloaded" });
  await stabilizePage(page);

  const visualMismatches = [];
  for (const id of ["lines", "scatter-fit", "histogram", "heatmap", "bars", "dashboard", "surface"]) {
    const card = page.locator(`#viz-${id}`);
    await expect(card, `missing visual fixture ${id}`).toBeVisible();
    await card.scrollIntoViewIfNeeded();
    const image = card.locator("img.viz-image");
    await image.evaluate((image) => {
      image.loading = "eager";
      image.src = image.currentSrc || image.src;
    });
    await expect.poll(() => image.evaluate((image) => image.naturalWidth), {
      message: `plot image ${id} did not load`,
      timeout: 15_000,
    }).toBeGreaterThan(0);
    try {
      await expect(image).toHaveScreenshot(`plot-${id}.png`, screenshotOptions);
    } catch (error) {
      visualMismatches.push({ id, message: error.message });
    }
  }
  expect(visualMismatches, "published scientific plot images differ from their baselines").toEqual([]);
});
