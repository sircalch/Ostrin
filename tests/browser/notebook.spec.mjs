import { expect, test } from "@playwright/test";
import { writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

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

// ---------------------------------------------------------------- symbolic to numeric, end to end

test("the numeric example runs from expression to figure, and a changed parameter updates every output after a run", async ({ page }) => {
  test.setTimeout(480_000);
  await open(page);
  await openFileMenu(page);
  await page.locator("#nb-example").selectOption("numeric");
  await expect(page.locator(".nb-cell")).toHaveCount(6);
  await page.getByRole("button", { name: "Run all" }).click();
  // transformation and derivation
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("f'(x) = -3 + 6*x^2", { timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("f(1/2) exactly: -1/4");
  // numeric table of f, f' and the central difference
  await expect(page.locator('[data-cell-id="c3"] .nb-table').first()).toBeVisible({ timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c3"] .nb-table').first()).toContainText("21");
  // checks against the independent quotient and a refused step
  await expect(page.locator('[data-cell-id="c4"] .nb-stdout')).toContainText("0 mismatched points of 41", { timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c4"] .nb-stdout')).toContainText("is refused, as it should be");
  // figure
  await expect(page.locator('[data-cell-id="c5"] .nb-figure img')).toBeVisible({ timeout: 240_000 });

  // change the parameter a from 2 to 5 in c1: the cells that depend on it become stale
  const parameters = page.locator('[data-cell-id="c1"] textarea.nb-source');
  await parameters.fill((await parameters.inputValue()).replace("return 2", "return 5"));
  for (const id of ["c2", "c3", "c4", "c5"]) {
    await expect(page.locator(`[data-cell-id="${id}"] .nb-chip`)).toHaveText(/stale/);
  }
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("f'(x) = -3 + 15*x^2", { timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("f(1/2) exactly: 1/8");
  // f'(1.5) = -3 + 15 * 1.5^2 = 30.75 exists only with the new parameter. (The earlier check, "15", also matched
  // digits of the old table, 21.00000000115898, so it passed before the table was refreshed and failed after.)
  await expect(page.locator('[data-cell-id="c3"] .nb-table').first()).toContainText("30.75", { timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c5"] .nb-figure img')).toBeVisible({ timeout: 240_000 });

  // the changed parameter is part of the document: a reload keeps it and does not run anything by itself
  await expect(page.locator(SAVE_STATE)).toHaveAttribute("data-state", "saved", { timeout: 15_000 });
  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
  await expect(page.locator('[data-cell-id="c1"] textarea.nb-source')).toHaveValue(/return 5/);
  // The outputs are stored with the document and shown as restored, not as computed in this page.
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("f'(x) = -3 + 15*x^2");
  await expect(page.locator('[data-cell-id="c2"] .nb-meta').first()).toContainText("restored from the document");
});

// ---------------------------------------------------------------- assumptions and conditions, end to end

test("the conditional example shows how a result changes with its context, and a changed context is run again", async ({ page }) => {
  test.setTimeout(480_000);
  await open(page);
  await openFileMenu(page);
  await page.locator("#nb-example").selectOption("conditional");
  await expect(page.locator(".nb-cell")).toHaveCount(5);
  await page.getByRole("button", { name: "Run all" }).click();
  // the table of results under the five contexts
  const table = page.locator('[data-cell-id="c1"] .nb-table').first();
  await expect(table).toBeVisible({ timeout: 240_000 });
  await expect(table).toContainText("abs(x)");
  await expect(table).toContainText("x != 1");
  await expect(table).toContainText("refused");
  // the checks, computed by the program
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("largest difference 0", { timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("20 counterexamples on the grid");
  await expect(page.locator('[data-cell-id="c2"] .nb-stdout')).toContainText("the original at x = 1 is undefined");
  await expect(page.locator('[data-cell-id="c3"] .nb-figure img')).toBeVisible({ timeout: 240_000 });

  // the derivation of E step by step: refused outside the domain, and refused by the context at x = -2 under x positive
  const derivation = page.locator('[data-cell-id="c4"] .nb-table').first();
  await expect(derivation).toBeVisible({ timeout: 240_000 });
  await expect(derivation).toContainText("no assumptions255");
  await expect(derivation).toContainText("outside the domain: x != 1 fails");
  await expect(derivation).toContainText("x positive-21refused: outside the context: x positive fails at x = -2");

  // edit the points in c4 and run it again: the new point appears with the original and the derivation
  const points = page.locator('[data-cell-id="c4"] textarea.nb-source');
  await points.fill((await points.inputValue()).replace("points = [2.0, 0.0 - 2.0, 1.0]", "points = [2.0, 0.0 - 2.0, 3.0]"));
  await expect(page.locator('[data-cell-id="c4"] .nb-chip')).toHaveText(/stale/);
  await page.locator('[data-cell-id="c4"]').getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator('[data-cell-id="c4"] .nb-table').first()).toContainText("no assumptions377", { timeout: 240_000 });

  // export the document: the downloaded file keeps the edited points
  await openFileMenu(page);
  const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Export…" }).click()]);
  const exported = await (await download.createReadStream()).toArray();
  expect(Buffer.concat(exported).toString("utf8")).toContain("points = [2.0, 0.0 - 2.0, 3.0]");

  // change the context from nonpositive to negative in c1, and its label: the result of the radical changes to -x
  const source = page.locator('[data-cell-id="c1"] textarea.nb-source');
  const edited = (await source.inputValue())
    .replace('symbolic.assume(none, "x", "nonpositive")', 'symbolic.assume(none, "x", "negative")')
    .replace('"x nonpositive"', '"x negative"');
  await source.fill(edited);
  // Each cell of this example builds its own context, so only c1 depends on the edit.
  await expect(page.locator('[data-cell-id="c1"] .nb-chip')).toHaveText(/stale/);
  await expect(page.locator('[data-cell-id="c2"] .nb-chip')).toHaveText(/current/);
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(page.locator('[data-cell-id="c1"] .nb-table').first()).toContainText("x negative", { timeout: 240_000 });
  await expect(page.locator('[data-cell-id="c1"] .nb-table').first()).toContainText("-x");
  await expect(page.locator('[data-cell-id="c1"] .nb-table').first()).not.toContainText("x nonpositive");
  // the saved document keeps the changed context and its outputs
  await expect(page.locator(SAVE_STATE)).toHaveAttribute("data-state", "saved", { timeout: 15_000 });
  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
  await expect(page.locator('[data-cell-id="c1"] textarea.nb-source')).toHaveValue(/"x", "negative"/);
  await expect(page.locator('[data-cell-id="c4"] textarea.nb-source')).toHaveValue(/points = \[2\.0, 0\.0 - 2\.0, 3\.0\]/);
  await expect(page.locator('[data-cell-id="c4"] .nb-table').first()).toContainText("no assumptions377");
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

test("a write already in progress cannot overwrite a newer change made during it, even when the unload save runs", async ({ page }) => {
  test.setTimeout(60_000);
  await open(page);
  // In one task: a Save starts a write that captures the title "first" and yields; then a newer title is typed and the
  // page's unload save runs before the yielded write resumes.
  await page.evaluate(async () => {
    const title = document.getElementById("notebook-title");
    title.value = "first";
    title.dispatchEvent(new Event("input"));
    document.getElementById("nb-save").click();
    // The write starts after a microtask: let it capture "first" and reach its yield.
    for (let i = 0; i < 5; i += 1) await Promise.resolve();
    title.value = "second";
    title.dispatchEvent(new Event("input"));
    window.dispatchEvent(new Event("beforeunload", { cancelable: true }));
  });
  await page.waitForTimeout(1_000);
  const stored = JSON.parse(await page.evaluate(() => localStorage.getItem("ostrin.notebook.v1.draft")));
  expect(stored.title).toBe("second");
  await expectSaved(page);
  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
  await expect(page.locator("#notebook-title")).toHaveValue("second");
});

const FAIL_INIT = () => {
  const real = Storage.prototype.setItem;
  Storage.prototype.setItem = function (key, value) {
    if (window.__failStorage && key === "ostrin.notebook.v1.draft") throw new DOMException("storage is full for this test", "QuotaExceededError");
    return real.call(this, key, value);
  };
};

test("edits made while storage is failing are all stored by the first write that succeeds", async ({ page }) => {
  test.setTimeout(120_000);
  await page.addInitScript(FAIL_INIT);
  await open(page);
  await page.evaluate(() => {
    window.__failStorage = true;
  });
  const id = await addCode(page);
  await setSource(page, id, 'fn main() -> Void {\n    print("one")\n}\n');
  await expect(page.locator(SAVE_STATE)).toHaveAttribute("data-state", "error", { timeout: 15_000 });
  // A newer edit while the failure is still in place: it must not be reported as saved, and must not be lost.
  await setSource(page, id, 'fn main() -> Void {\n    print("two")\n}\n');
  await expect(page.locator(SAVE_STATE)).toHaveAttribute("data-state", "error", { timeout: 15_000 });
  await page.evaluate(() => {
    window.__failStorage = false;
  });
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expectSaved(page);
  const stored = await page.evaluate(() => localStorage.getItem("ostrin.notebook.v1.draft"));
  expect(stored).toContain('print(\\"two\\")');
  expect(stored).not.toContain('print(\\"one\\")');
  await expect(page.locator("#nb-notice")).toHaveText("");
});

test("a failed write does not clear the error or mark a later change saved until a write succeeds", async ({ page }) => {
  test.setTimeout(120_000);
  await page.addInitScript(FAIL_INIT);
  await open(page);
  await page.evaluate(() => {
    window.__failStorage = true;
  });
  // The title edit starts an autosave; a Save click queues a second write behind it. Both fail.
  await page.locator("#notebook-title").fill("Failing title");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.locator(SAVE_STATE)).toHaveAttribute("data-state", "error", { timeout: 15_000 });
  await page.waitForTimeout(1_000);
  await expect(page.locator(SAVE_STATE)).toHaveAttribute("data-state", "error");
  await page.evaluate(() => {
    window.__failStorage = false;
  });
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expectSaved(page);
  const stored = JSON.parse(await page.evaluate(() => localStorage.getItem("ostrin.notebook.v1.draft")));
  expect(stored.title).toBe("Failing title");
});

test("an imported document is the one stored, even when a save of the previous document is in progress", async ({ page }) => {
  test.setTimeout(120_000);
  page.on("dialog", (dialog) => dialog.accept());
  await open(page);
  const id = await addCode(page);
  await setSource(page, id, 'fn main() -> Void {\n    print("before import")\n}\n');
  const file = join(tmpdir(), `ostrin-import-race-${Date.now()}.json`);
  const stamp = "2026-11-11T09:00:00.000Z";
  writeFileSync(file, JSON.stringify({ format: "ostrin.notebook", version: 1, id: "importedrace", title: "Imported race", created: stamp, updated: stamp, cells: [{ id: "c1", kind: "code", source: 'fn main() -> Void {\n    print("imported")\n}\n', deps: [] }], resources: [], runs: {} }));
  // A Save of the previous document starts; the import is chosen while that write may still be in progress.
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await page.locator(INPUT).setInputFiles(file);
  await expect(page.locator("#notebook-title")).toHaveValue("Imported race");
  await expectSaved(page);
  const stored = JSON.parse(await page.evaluate(() => localStorage.getItem("ostrin.notebook.v1.draft")));
  expect(stored.title).toBe("Imported race");
  expect(stored.cells[0].source).toContain("imported");
  expect(stored.cells[0].source).not.toContain("before import");
});
