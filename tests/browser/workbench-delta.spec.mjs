import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

const sha256 = (text) => createHash("sha256").update(text, "utf8").digest("hex");
const documentFile = new URL("../../website/workbench-demos/parameter-sweep.ostrin.project.json", import.meta.url);

async function openSweep(page) {
  await page.goto("./playground.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
  await page.locator("#demo-sweep").click();
  await expect(page.locator("#file-explorer")).toContainText("4 files");
}

test("the parameter sweep runs in the Worker and shows a structured table, a figure and exportable results", async ({ page }) => {
  test.setTimeout(120_000);
  await openSweep(page);
  await page.locator("#run").click();
  const output = page.locator("#output");
  await expect(output).toContainText("all checks passed", { timeout: 90_000 });
  // The table is built from the structured line; the raw RESULTS line is not shown in the log.
  const rows = page.locator(".results-table tbody tr");
  await expect(rows).toHaveCount(4);
  const cValues = await rows.locator("td:first-child").allTextContents();
  expect(cValues).toEqual(["1", "3", "5", "7"]);
  await expect(page.locator(".results-table tbody tr td:last-child")).toHaveText(["yes", "yes", "yes", "yes"]);
  await expect(output).not.toContainText("RESULTS {");
  await expect(page.locator(".figure-preview img")).toBeVisible({ timeout: 60_000 });
  await expect(page.locator("#download-results")).toBeEnabled();

  const [download] = await Promise.all([page.waitForEvent("download"), page.locator("#download-results").click()]);
  expect(download.suggestedFilename()).toBe("results.json");
  const results = JSON.parse(await readFile(await download.path(), "utf8"));
  expect(results.schema).toBe("ostrin.results/v1");
  expect(results.rows.map((r) => r.c)).toEqual([1, 3, 5, 7]);
  const five = results.rows.find((r) => r.c === 5);
  expect(Math.abs(five.root - 2.0945514815423265)).toBeLessThanOrEqual(1e-12);
  expect(Math.abs(five.residual)).toBeLessThanOrEqual(five.residual_bound);
});

test("the sweep manifest hashes the committed sources and the raw captured output", async ({ page }) => {
  test.setTimeout(120_000);
  await openSweep(page);
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 90_000 });
  const expected = JSON.parse(await readFile(documentFile, "utf8"));
  const [download] = await Promise.all([page.waitForEvent("download"), page.locator("#download-manifest").click()]);
  const manifest = JSON.parse(await readFile(await download.path(), "utf8"));
  expect(manifest.project.entry).toBe("main.ostrin");
  for (const [name, text] of Object.entries(expected.files)) {
    expect(manifest.project.sources[name].sha256, name).toBe(sha256(text));
  }
  expect(manifest.outputs.stdout.source).toBe("raw runtime bytes");
  expect(manifest.claims.reproducibility).toBe("not assessed (no R3 claim)");
});

test("export, reload, import and rerun give the same structured results", async ({ page }) => {
  test.setTimeout(180_000);
  await openSweep(page);
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 90_000 });
  const [resultsDownload] = await Promise.all([page.waitForEvent("download"), page.locator("#download-results").click()]);
  const firstResults = JSON.parse(await readFile(await resultsDownload.path(), "utf8"));

  const [projectDownload] = await Promise.all([page.waitForEvent("download"), page.locator("#export-project").click()]);
  const projectPath = await projectDownload.path();
  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
  await page.locator("#import-project-file").setInputFiles(projectPath);
  await expect(page.locator("#file-explorer")).toContainText("4 files");

  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 90_000 });
  const [secondDownload] = await Promise.all([page.waitForEvent("download"), page.locator("#download-results").click()]);
  const secondResults = JSON.parse(await readFile(await secondDownload.path(), "utf8"));
  expect(secondResults).toEqual(firstResults);
});
