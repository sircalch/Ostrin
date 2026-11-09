import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

const sha256 = (text) => createHash("sha256").update(text, "utf8").digest("hex");
const demoFile = new URL("../../website/workbench-demos/cubic-root.ostrin.project.json", import.meta.url);

async function openDemo(page) {
  await page.goto("./playground.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
  await page.locator("#demo-project").click();
  await expect(page.locator("#file-explorer")).toContainText("4 files");
}

test("the cubic root demonstration loads, runs through the real compiler and shows its figure", async ({ page }) => {
  await openDemo(page);
  await expect(page.locator('#file-explorer [data-path="main.ostrin"] .file-badge')).toHaveText("entry");
  await expect(page.locator("#output")).toContainText("Nothing has run yet");

  await page.locator("#run").click();
  const output = page.locator("#output");
  await expect(output).toContainText("all checks passed", { timeout: 60_000 });
  await expect(output).toContainText("root: 2.0945514815423265");
  await expect(output).toContainText("symbolic derivative: 11.161437726493464");
  await expect(output).toContainText("autodiff derivative: 11.161437726493464");
  await expect(output).toContainText("expected error for [0, 1]");
  await expect(page.locator(".figure-preview img")).toBeVisible({ timeout: 60_000 });
  expect(await page.locator(".figure-preview img").evaluate((node) => node.naturalWidth)).toBeGreaterThan(100);
});

test("the demo manifest records the hashes of the exact sources that ran", async ({ page }) => {
  await openDemo(page);
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 60_000 });

  const expected = JSON.parse(await readFile(demoFile, "utf8"));
  const [download] = await Promise.all([page.waitForEvent("download"), page.locator("#download-manifest").click()]);
  const manifest = JSON.parse(await readFile(await download.path(), "utf8"));
  expect(manifest.project.entry).toBe("main.ostrin");
  for (const [name, text] of Object.entries(expected.files)) {
    expect(manifest.project.sources[name].sha256, name).toBe(sha256(text));
  }
  expect(manifest.outputs.stdout.sha256).toMatch(/^[0-9a-f]{64}$/);
  expect(manifest.claims.reproducibility).toBe("not assessed (no R3 claim)");
});

test("unsaved edits are kept when the user declines to discard them", async ({ page }) => {
  await openDemo(page);
  await page.locator('#file-explorer [data-path="model.ostrin"]').click();
  await page.getByLabel("Ostrin source", { exact: true }).fill("pub fn f(x: Float) -> Float {\n    x\n}\n");
  page.once("dialog", (dialog) => dialog.dismiss());
  await page.locator("#demo-project").click();
  await expect(page.locator("#file-explorer")).toContainText("4 files");
  await page.locator('#file-explorer [data-path="model.ostrin"]').click();
  await expect(page.getByLabel("Ostrin source", { exact: true })).toHaveValue(/pub fn f\(x: Float\) -> Float \{\n    x\n\}/);
});
