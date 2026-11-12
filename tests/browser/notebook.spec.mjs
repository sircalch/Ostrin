import { expect, test } from "@playwright/test";
import { writeFileSync } from "node:fs";

// The Ostrin Scientific Notebook in a real browser, with the real compiler (WASM in a Worker). Every output checked
// here was produced by a run in the page; restored outputs are labelled as such.

const INPUT = 'input[type=file][aria-label="Notebook file to import"]';
const SAVE_STATE = "#nb-save-state";

async function open(page) {
  await page.goto("./notebook.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
}

// The less frequent actions live in the File menu; it is opened before they are used.
async function openFileMenu(page) {
  const menu = page.locator("details.nb-menu");
  if (!(await menu.evaluate((node) => node.open))) await menu.locator("summary").click();
}

async function addCode(page) {
  await page.getByRole("button", { name: "+ Code" }).click();
  return page.locator(".nb-cell-code").last().getAttribute("data-cell-id");
}

async function addText(page) {
  await page.getByRole("button", { name: "+ Text" }).click();
  return page.locator(".nb-cell-text").last().getAttribute("data-cell-id");
}

const cell = (page, id) => page.locator(`[data-cell-id="${id}"]`);

async function setSource(page, id, text) {
  await cell(page, id).locator("textarea.nb-source").fill(text);
}

async function runCell(page, id) {
  await cell(page, id).getByRole("button", { name: "Run", exact: true }).click();
}

// Waits until the document is stored: the state element says so, and the stored text is the current one.
async function expectSaved(page) {
  await expect(page.locator(SAVE_STATE)).toHaveAttribute("data-state", "saved", { timeout: 15_000 });
}

const HELLO = 'fn main() -> Void {\n    print("hello from a cell")\n}\n';

test("create a notebook, add code, run it, and see its output with a current status", async ({ page }) => {
  test.setTimeout(180_000);
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, HELLO);
  await runCell(page, id);
  await expect(cell(page, id).locator(".nb-stdout")).toContainText("hello from a cell", { timeout: 120_000 });
  await expect(cell(page, id).locator(".nb-chip")).toHaveText("current");
  await expect(cell(page, id).locator(".nb-meta")).toContainText("computed in this session");
});

test("editing the code makes the earlier output stale, and running again makes it current", async ({ page }) => {
  test.setTimeout(180_000);
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, HELLO);
  await runCell(page, id);
  await expect(cell(page, id).locator(".nb-chip")).toHaveText("current", { timeout: 120_000 });
  await setSource(page, id, HELLO.replace("hello from a cell", "second version"));
  await expect(cell(page, id).locator(".nb-chip")).toHaveText("stale: the code or its data changed since this ran");
  await expect(cell(page, id).locator(".nb-stale")).toBeVisible();
  await runCell(page, id);
  await expect(cell(page, id).locator(".nb-stdout")).toContainText("second version", { timeout: 120_000 });
  await expect(cell(page, id).locator(".nb-chip")).toHaveText("current");
  await expect(cell(page, id).locator(".nb-stale")).toHaveCount(0);
});

test("run all executes several cells in order, and a dependency's definitions are used by the cell that imports them", async ({ page }) => {
  test.setTimeout(240_000);
  await open(page);
  const defs = await addCode(page);
  await setSource(page, defs, "pub fn answer() -> Int {\n    42\n}\n");
  const use = await addCode(page);
  await setSource(page, use, "import " + defs + "\nfn main() -> Void {\n    print(\"the answer is \" + " + defs + ".answer().to_string())\n}\n");
  await cell(page, use).locator("input[type=text]").fill(defs);
  await cell(page, use).locator("input[type=text]").press("Enter");
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(cell(page, use).locator(".nb-stdout")).toContainText("the answer is 42", { timeout: 120_000 });
  await expect(cell(page, use).locator(".nb-chip")).toHaveText("current");
});

test("cancelling a run stops it, records it as cancelled, and the notebook keeps working", async ({ page }) => {
  test.setTimeout(180_000);
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, "fn main() -> Void {\n    mut i = 0\n    while true {\n        i = i + 1\n    }\n}\n");
  await runCell(page, id);
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(cell(page, id).locator(".nb-meta")).toContainText("cancelled", { timeout: 60_000 });
  await expect(cell(page, id).locator(".nb-error")).toContainText("cancelled");
  const next = await addCode(page);
  await setSource(page, next, HELLO);
  await runCell(page, next);
  await expect(cell(page, next).locator(".nb-stdout")).toContainText("hello from a cell", { timeout: 120_000 });
});

test("a program that fails to compile shows its diagnostics, and the notebook recovers on the next cell", async ({ page }) => {
  test.setTimeout(180_000);
  await open(page);
  const bad = await addCode(page);
  await setSource(page, bad, "fn main() -> Void {\n    print(undefined_name)\n}\n");
  await runCell(page, bad);
  await expect(cell(page, bad).locator(".nb-meta")).toContainText("failed", { timeout: 120_000 });
  await expect(cell(page, bad).locator(".nb-stderr")).toContainText("undefined_name");
  const good = await addCode(page);
  await setSource(page, good, HELLO);
  await runCell(page, good);
  await expect(cell(page, good).locator(".nb-stdout")).toContainText("hello from a cell", { timeout: 120_000 });
});

test("the experimental example runs end to end: a table and a figure from the attached data, then save and recover", async ({ page }) => {
  test.setTimeout(420_000);
  await open(page);
  await openFileMenu(page);
  await page.locator("#nb-example").selectOption("experimental");
  await expect(page.locator(".nb-cell")).toHaveCount(5, { timeout: 30_000 });
  await expectSaved(page);
  await expect(page.locator(".nb-resource")).toContainText("data/calibration.csv");
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(page.locator('[data-cell-id="c2"] .nb-table').first()).toBeVisible({ timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c2"] .nb-table caption').first()).toContainText("Descriptive statistics");
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("quadratic fit: sse");
  await expect(page.locator('[data-cell-id="c3"] .nb-figure img')).toBeVisible({ timeout: 120_000 });
  await expect(page.locator('[data-cell-id="c4"] .nb-figure img')).toBeVisible({ timeout: 120_000 });
  await expect(page.locator('[data-cell-id="c2"] .nb-chip')).toHaveText("current");
  await expectSaved(page);
  await page.waitForFunction(() => localStorage.getItem("ostrin.notebook.v1.draft")?.includes("calibration"), null, { timeout: 15_000 });
  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
  await expect(page.locator(".nb-cell")).toHaveCount(5);
  await expect(page.locator('[data-cell-id="c3"] .nb-figure img')).toBeVisible();
  await expect(page.locator('[data-cell-id="c2"] .nb-meta')).toContainText("restored from the document");
  await expect(page.locator('[data-cell-id="c2"] .nb-chip')).toHaveText("current");
});

test("export, then import into a clean browser context: the cells and outputs come back, and nothing runs by itself", async ({ page, browser }, testInfo) => {
  test.setTimeout(420_000);
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, HELLO);
  await runCell(page, id);
  await expect(cell(page, id).locator(".nb-stdout")).toContainText("hello from a cell", { timeout: 120_000 });
  await openFileMenu(page);
  const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Export…" }).click()]);
  const file = testInfo.outputPath(download.suggestedFilename());
  await download.saveAs(file);
  expect(download.suggestedFilename()).toMatch(/\.ostrin\.notebook\.json$/);

  const clean = await browser.newContext();
  const other = await clean.newPage();
  await open(other);
  await other.locator(INPUT).setInputFiles(file);
  await expect(other.locator(".nb-cell")).toHaveCount(1, { timeout: 30_000 });
  await expect(other.locator(`[data-cell-id="${id}"] .nb-stdout`)).toContainText("hello from a cell");
  await expect(other.locator(`[data-cell-id="${id}"] .nb-meta`)).toContainText("restored from the document");
  // Nothing ran on import: no output is marked as computed in this session.
  await expect(other.locator(".nb-meta", { hasText: "computed in this session" })).toHaveCount(0);
  await expect(other.locator("#nb-status")).not.toContainText("Running");
  await expectSaved(other);
  await clean.close();
});

test("a malformed or unsupported document is refused and the open notebook is unchanged", async ({ page }, testInfo) => {
  test.setTimeout(120_000);
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, HELLO);
  const future = testInfo.outputPath("future.ostrin.notebook.json");
  writeFileSync(future, JSON.stringify({ format: "ostrin.notebook", version: 2, id: "x", title: "x", created: "2026-11-11T09:00:00.000Z", updated: "2026-11-11T09:00:00.000Z", cells: [], resources: [], runs: {} }));
  await page.locator(INPUT).setInputFiles(future);
  await expect(page.locator("#nb-notice")).toContainText("unsupported version 2");
  await expect(page.locator(".nb-cell")).toHaveCount(1);
  const broken = testInfo.outputPath("broken.ostrin.notebook.json");
  writeFileSync(broken, "{ this is not json");
  await page.locator(INPUT).setInputFiles(broken);
  await expect(page.locator("#nb-notice")).toContainText("not valid JSON");
  await expect(page.locator(".nb-cell")).toHaveCount(1);
  const hostile = testInfo.outputPath("hostile.ostrin.notebook.json");
  writeFileSync(hostile, JSON.stringify({ format: "ostrin.notebook", version: 1, id: "x", title: "x", created: "2026-11-11T09:00:00.000Z", updated: "2026-11-11T09:00:00.000Z", cells: [{ id: "c1", kind: "code", source: "fn main() -> Void { }", deps: [] }], resources: [], runs: { c1: { hash: "a".repeat(64), state: "completed", startedAt: "2026-11-11T09:00:00.000Z", durationMs: 1, exitCode: 0, stdout: "<img src=x onerror=window.__evil=1>", stderr: "", tables: [], figures: ["<script>window.__evil=2</script>"], results: null, resultsRejected: null, error: null } } }));
  await page.locator(INPUT).setInputFiles(hostile);
  await expect(page.locator("#nb-notice")).toContainText("malformed figure");
  expect(await page.evaluate(() => window.__evil ?? null)).toBeNull();
});

test("an open document is kept in the browser, so a reload restores its cells", async ({ page }) => {
  test.setTimeout(120_000);
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, 'fn main() -> Void {\n    print("kept")\n}\n');
  await expectSaved(page);
  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
  await expect(page.locator(`[data-cell-id="${id}"] textarea.nb-source`)).toHaveValue(/kept/);
});

test("the physical example runs in the browser; changing a parameter makes the outputs stale and the re-run shows the new values", async ({ page }) => {
  test.setTimeout(420_000);
  await open(page);
  await openFileMenu(page);
  await page.locator("#nb-example").selectOption("physical");
  await expect(page.locator(".nb-cell")).toHaveCount(6);
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(page.locator('[data-cell-id="c2"] .nb-table').first()).toBeVisible({ timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("largest error over the 1201 nodes");
  await expect(page.locator('[data-cell-id="c3"] .nb-figure img')).toBeVisible({ timeout: 120_000 });
  await expect(page.locator('[data-cell-id="c5"] .nb-figure img')).toBeVisible({ timeout: 120_000 });
  await expect(page.locator('[data-cell-id="c2"] .nb-chip')).toHaveText("current");
  const before = await page.locator('[data-cell-id="c2"] .nb-table').first().textContent();
  const params = page.locator('[data-cell-id="c1"] textarea.nb-source');
  const text = await params.inputValue();
  await params.fill(text.replace("0.35 / (1 s)", "0.5 / (1 s)"));
  await expect(page.locator('[data-cell-id="c2"] .nb-chip')).toHaveText("stale: the code or its data changed since this ran");
  await expect(page.locator('[data-cell-id="c3"] .nb-chip')).toHaveText("stale: the code or its data changed since this ran");
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(page.locator('[data-cell-id="c2"] .nb-chip')).toHaveText("current", { timeout: 240_000 });
  const after = await page.locator('[data-cell-id="c2"] .nb-table').first().textContent();
  expect(after).not.toEqual(before);
});

test("the symbolic example runs in the browser: derivative and substitution, roots and factorization, and a figure", async ({ page }) => {
  test.setTimeout(420_000);
  await open(page);
  await openFileMenu(page);
  await page.locator("#nb-example").selectOption("symbolic");
  await expect(page.locator(".nb-cell")).toHaveCount(4);
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(page.locator('[data-cell-id="c1"] .nb-stdout')).toContainText("f(2) by substitution: 9", { timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c1"] .nb-table').first()).toBeVisible();
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("factorization: (-1 + x)*(-2 + x)", { timeout: 120_000 });
  await expect(page.locator('[data-cell-id="c3"] .nb-figure img')).toBeVisible({ timeout: 120_000 });
});

// ---------------------------------------------------------------- Markdown in text cells

test("text cells render Markdown, and their links and HTML cannot run anything", async ({ page }) => {
  test.setTimeout(120_000);
  await open(page);
  const id = await addText(page);
  await setSource(page, id, "# Heading\n\nA paragraph with **bold**, *soft*, `code` and [a link](https://example.org/x).\n\n- first\n- second\n\n<script>window.__evil = 3</script>\n\n[bad](javascript:alert(1))");
  await cell(page, id).getByRole("button", { name: /Edit text cell|Read text cell/ }).click();
  await expect(cell(page, id).locator("h3")).toHaveText("Heading");
  await expect(cell(page, id).locator("strong")).toHaveText("bold");
  await expect(cell(page, id).locator("ul li")).toHaveCount(2);
  await expect(cell(page, id).locator('a[href="https://example.org/x"]')).toHaveText("a link");
  await expect(cell(page, id).locator("a[href^='javascript']")).toHaveCount(0);
  await expect(cell(page, id).getByText("(link not allowed)")).toBeVisible();
  await expect(cell(page, id).locator("script")).toHaveCount(0);
  await expect(cell(page, id).getByText("<script>window.__evil = 3</script>")).toBeVisible();
  expect(await page.evaluate(() => window.__evil ?? null)).toBeNull();
});

test("the Edit and Read toggle switches between the original text and the rendered view", async ({ page }) => {
  test.setTimeout(120_000);
  await open(page);
  const id = await addText(page);
  await setSource(page, id, "## Notes\n\nSome *text*.");
  await cell(page, id).getByRole("button", { name: /Read text cell/ }).click();
  await expect(cell(page, id).locator(".nb-md h4")).toHaveText("Notes");
  await cell(page, id).getByRole("button", { name: /Edit text cell/ }).click();
  await expect(cell(page, id).locator("textarea.nb-source")).toHaveValue(/## Notes/);
  await expect(cell(page, id).locator(".nb-md")).toHaveCount(0);
});

// ---------------------------------------------------------------- persistence states

test("the save state moves through unsaved, saving and saved, and an edit during a save is not lost", async ({ page }) => {
  test.setTimeout(120_000);
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, 'fn main() -> Void {\n    print("one")\n}\n');
  await expect(page.locator(SAVE_STATE)).not.toHaveAttribute("data-state", "error");
  for (const word of ["two", "three", "four"]) {
    await setSource(page, id, `fn main() -> Void {\n    print("${word}")\n}\n`);
  }
  await expectSaved(page);
  const stored = await page.evaluate(() => localStorage.getItem("ostrin.notebook.v1.draft"));
  expect(stored).toContain('print(\\"four\\")');
  expect(stored).not.toContain('print(\\"one\\")');
});

test("the Save button stores the document now and reports it saved", async ({ page }) => {
  test.setTimeout(120_000);
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, 'fn main() -> Void {\n    print("now")\n}\n');
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expectSaved(page);
  const stored = await page.evaluate(() => localStorage.getItem("ostrin.notebook.v1.draft"));
  expect(stored).toContain('print(\\"now\\")');
});

test("when storage fails, the page says the changes are not saved, keeps the document open, and recovers when storage works again", async ({ page }) => {
  test.setTimeout(120_000);
  await page.addInitScript(() => {
    const real = Storage.prototype.setItem;
    Storage.prototype.setItem = function (key, value) {
      if (window.__failStorage && key === "ostrin.notebook.v1.draft") throw new DOMException("storage is full for this test", "QuotaExceededError");
      return real.call(this, key, value);
    };
  });
  await open(page);
  await page.evaluate(() => {
    window.__failStorage = true;
  });
  const id = await addCode(page);
  await setSource(page, id, 'fn main() -> Void {\n    print("in memory")\n}\n');
  await expect(page.locator(SAVE_STATE)).toHaveAttribute("data-state", "error", { timeout: 15_000 });
  await expect(page.locator(SAVE_STATE)).toHaveText("Not saved");
  await expect(page.locator("#nb-notice")).toContainText("they are not stored in this browser");
  // The document is still open, with its code, and nothing claims it is saved.
  await expect(cell(page, id).locator("textarea.nb-source")).toHaveValue(/in memory/);
  await expect(page.locator(SAVE_STATE)).not.toHaveAttribute("data-state", "saved");
  await page.evaluate(() => {
    window.__failStorage = false;
  });
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expectSaved(page);
  await expect(page.locator("#nb-notice")).toHaveText("");
});

test("a pending autosave is not reported as an error", async ({ page }) => {
  test.setTimeout(120_000);
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, 'fn main() -> Void {\n    print("pending")\n}\n');
  const state = await page.locator(SAVE_STATE).getAttribute("data-state");
  expect(["unsaved", "saving", "saved"]).toContain(state);
  await expectSaved(page);
  await expect(page.locator("#nb-notice")).toHaveText("");
});

test("the physical example keeps working after a reload, and running a cell after a reload does not change its stored text", async ({ page }) => {
  test.setTimeout(420_000);
  await open(page);
  await openFileMenu(page);
  await page.locator("#nb-example").selectOption("physical");
  await expect(page.locator(".nb-cell")).toHaveCount(6);
  await expectSaved(page);
  const before = await page.evaluate(() => localStorage.getItem("ostrin.notebook.v1.draft"));
  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
  await expect(page.locator(".nb-cell")).toHaveCount(6);
  const after = await page.evaluate(() => localStorage.getItem("ostrin.notebook.v1.draft"));
  expect(after).toEqual(before);
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(page.locator('[data-cell-id="c2"] .nb-table').first()).toBeVisible({ timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c2"] .nb-meta')).toContainText("computed in this session");
});

test("the toolbar keeps every action reachable at a narrow width, and its states are correct", async ({ page }) => {
  test.setTimeout(120_000);
  await page.setViewportSize({ width: 360, height: 780 });
  await open(page);
  await expect(page.getByRole("button", { name: "Run all" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Save", exact: true })).toBeDisabled();
  await openFileMenu(page);
  await expect(page.getByRole("button", { name: "Export…" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Import…" })).toBeVisible();
  await expect(page.locator("#nb-example")).toBeVisible();
});
