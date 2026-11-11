import { expect, test } from "@playwright/test";

const LOOP = JSON.stringify({
  format: "ostrin.project", version: 1, name: "endless loop", entry: "main.ostrin",
  files: { "main.ostrin": "fn main() -> Void {\n    mut i = 0\n    while true {\n        i = i + 1\n    }\n}\n" },
});

async function openSweep(page) {
  await page.goto("./playground.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
  await page.locator("#demo-sweep").click();
  await expect(page.locator("#file-explorer")).toContainText("4 files");
}

test("a real sweep run creates a history entry with validated results, its figure and its digests", async ({ page }) => {
  test.setTimeout(120_000);
  await openSweep(page);
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 90_000 });
  const rows = page.locator("#run-history tbody tr");
  await expect(rows).toHaveCount(1, { timeout: 20_000 });
  await expect(rows.first()).toContainText("completed");
  await expect(rows.first()).toContainText("validated");
  await rows.first().click();
  const details = page.locator("#run-history .history-details");
  await expect(details).toContainText("validated structured results");
  await expect(details.locator(".history-results-table tbody tr")).toHaveCount(4);
  await expect(details.locator(".figure-preview img")).toBeVisible();
  await expect(details).toContainText("source main.ostrin");
  await expect(details).toContainText("Integrity: these digests identify the recorded bytes");
  await details.getByRole("button", { name: "Back to the open project" }).click();
  await expect(page.locator("#run-history .history-details")).toHaveCount(0);
  await expect(page.locator("#file-explorer")).toContainText("4 files", { timeout: 5_000 });
});

test("a second run is an independent entry, and each entry shows its own id", async ({ page }) => {
  test.setTimeout(180_000);
  await openSweep(page);
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 90_000 });
  await expect(page.locator("#run-history tbody tr")).toHaveCount(1, { timeout: 20_000 });
  await page.locator("#run").click();
  await expect(page.locator("#run-history tbody tr")).toHaveCount(2, { timeout: 90_000 });
  const ids = await page.locator("#run-history tbody tr td:first-child").allTextContents();
  expect(new Set(ids).size).toBe(2);
});

test("a cancelled run is recorded as incomplete evidence, with no results shown as validated", async ({ page }) => {
  test.setTimeout(120_000);
  await page.goto("./playground.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
  await page.locator("#import-project-file").setInputFiles({ name: "loop.json", mimeType: "application/json", buffer: Buffer.from(LOOP) });
  await expect(page.locator("#file-explorer")).toContainText("1 file");
  await page.locator("#run").click();
  await expect(page.locator("#cancel-run")).toBeEnabled({ timeout: 15_000 });
  await page.locator("#cancel-run").click();
  const row = page.locator("#run-history tbody tr").first();
  await expect(row).toContainText("cancelled", { timeout: 20_000 });
  await expect(row).toContainText("incomplete");
  await row.click();
  await expect(page.locator("#run-history .history-details")).toContainText("incomplete: no validated results");
  await expect(page.locator("#run-history .history-results-table")).toHaveCount(0);
});
