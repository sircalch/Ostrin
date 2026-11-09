import { expect, test } from "@playwright/test";

// A program that never terminates: its only exit is the page terminating the Worker.
const LOOP_PROJECT = JSON.stringify({
  format: "ostrin.project",
  version: 1,
  name: "endless loop",
  entry: "main.ostrin",
  files: { "main.ostrin": "fn main() -> Void {\n    mut i = 0\n    while true {\n        i = i + 1\n    }\n}\n" },
});

async function openPlayground(page) {
  await page.goto("./playground.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
}

async function importLoop(page) {
  await page.locator("#import-project-file").setInputFiles({
    name: "loop.ostrin.project.json",
    mimeType: "application/json",
    buffer: Buffer.from(LOOP_PROJECT),
  });
  await expect(page.locator("#file-explorer")).toContainText("1 file");
}

function editor(page) {
  return page.getByLabel("Ostrin source", { exact: true });
}

test("Cancel stops a non-terminating program, keeps the editor's text, and the page keeps responding", async ({ page }) => {
  test.setTimeout(90_000);
  await openPlayground(page);
  await importLoop(page);
  await page.locator("#run").click();

  const cancel = page.locator("#cancel-run");
  await expect(cancel).toBeEnabled({ timeout: 15_000 });
  await expect(page.locator("#status")).toHaveText("running…");

  // The page must stay responsive while the program runs: the editor accepts new text.
  await editor(page).fill("fn main() -> Void {\n    print(\"edited while running\")\n}\n");
  await expect(editor(page)).toHaveValue(/edited while running/);

  await cancel.click();
  await expect(page.locator("#output")).toHaveText(/Run cancelled\. Your edits are kept\./, { timeout: 10_000 });
  await expect(page.locator("#status")).toHaveText("cancelled");
  await expect(cancel).toBeDisabled();
  await expect(editor(page)).toHaveValue(/edited while running/, { timeout: 1000 });
});

test("a new Run works after Cancel, from a fresh Worker", async ({ page }) => {
  test.setTimeout(90_000);
  await openPlayground(page);
  await importLoop(page);
  await page.locator("#run").click();
  await expect(page.locator("#cancel-run")).toBeEnabled({ timeout: 15_000 });
  await page.locator("#cancel-run").click();
  await expect(page.locator("#status")).toHaveText("cancelled", { timeout: 10_000 });

  await page.locator("#demo-project").click();
  await expect(page.locator("#file-explorer")).toContainText("4 files");
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 60_000 });
});

test("two consecutive Run clicks leave one active run and show one result", async ({ page }) => {
  test.setTimeout(90_000);
  await openPlayground(page);
  await page.locator("#demo-project").click();
  await expect(page.locator("#file-explorer")).toContainText("4 files");
  await page.locator("#run").click();
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 60_000 });
  const text = await page.locator("#output").textContent();
  expect(text.match(/all checks passed/g)?.length ?? 0).toBe(1);
});

test("loading another project during a run cancels that run and reports it", async ({ page }) => {
  test.setTimeout(90_000);
  await openPlayground(page);
  await importLoop(page);
  await page.locator("#run").click();
  await expect(page.locator("#cancel-run")).toBeEnabled({ timeout: 15_000 });
  await page.locator("#demo-project").click();
  await expect(page.locator("#output")).toContainText("Loaded the cubic root demonstration", { timeout: 15_000 });
  await expect(page.locator("#cancel-run")).toBeDisabled();
  await expect(page.locator("#file-explorer")).toContainText("4 files");
});

test("a program that exceeds the time limit is reported as timed out", async ({ page }) => {
  test.setTimeout(90_000);
  await openPlayground(page);
  await importLoop(page);
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("The run timed out", { timeout: 60_000 });
  await expect(page.locator("#status")).toHaveText("timed out");
  await expect(page.locator("#cancel-run")).toBeDisabled();
});
