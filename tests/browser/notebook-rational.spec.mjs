import { expect, test } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// Notebook F (exact algebra and rational equations) in a real browser with the real compiler (WASM in a Worker). Every
// output checked here was printed by a run in the page. Expected values were worked out by hand from the definitions.

const INPUT = 'input[type=file][aria-label="Notebook file to import"]';
const SAVE_STATE = "#nb-save-state";

async function open(page) {
  await page.goto("./notebook.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
}

async function openFileMenu(page) {
  const menu = page.locator("details.nb-menu");
  if (!(await menu.evaluate((node) => node.open))) await menu.locator("summary").click();
}

const cell = (page, id) => page.locator(`[data-cell-id="${id}"]`);

async function runCell(page, id) {
  await cell(page, id).getByRole("button", { name: "Run", exact: true }).click();
}

async function loadRational(page) {
  await open(page);
  await openFileMenu(page);
  await page.locator("#nb-example").selectOption("rational");
  await expect(page.locator(".nb-cell")).toHaveCount(14);
}

test("notebook F runs in the browser: the polynomials, the division, the simplification and the equations", async ({ page }) => {
  test.setTimeout(900_000);
  await loadRational(page);
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(cell(page, "c3").locator(".nb-stdout")).toContainText("holds exactly: true", { timeout: 300_000 });
  await expect(cell(page, "c3").locator(".nb-stdout")).toContainText("q1 = x + y");
  await expect(cell(page, "c3").locator(".nb-stdout")).toContainText("r = x + y + 1");
  await expect(cell(page, "c4").locator(".nb-stdout")).toContainText("domain requirements kept: [-1, 1] != 0", { timeout: 300_000 });
  await expect(cell(page, "c5").locator(".nb-stdout")).toContainText("f(1) is not evaluated");
  await expect(cell(page, "c6").locator(".nb-stdout")).toContainText("= 3: solved", { timeout: 300_000 });
  await expect(cell(page, "c6").locator(".nb-stdout")).toContainText("excluded: x = 1");
  await expect(cell(page, "c7").locator(".nb-stdout")).toContainText("= 2: no_solution", { timeout: 300_000 });
  await expect(cell(page, "c8").locator(".nb-stdout")).toContainText("(x^2 - 2)/(x^2 - 2) = 1: identity", { timeout: 300_000 });
  await expect(cell(page, "c9").locator(".nb-stdout")).toContainText("x^3 + x + 1 = 0: partial", { timeout: 300_000 });
  await expect(cell(page, "c10").locator(".nb-stdout")).toContainText("equal: true", { timeout: 300_000 });
});

test("notebook F: the table marks x = 1 as excluded, and the figure is an SVG with separate segments", async ({ page }) => {
  test.setTimeout(900_000);
  await loadRational(page);
  await runCell(page, "c1");
  await runCell(page, "c11");
  const table = cell(page, "c11").locator(".nb-table").first();
  await expect(table).toBeVisible({ timeout: 300_000 });
  await expect(table).toContainText("excluded");
  await expect(table).toContainText("-3/2");
  await runCell(page, "c12");
  await expect(cell(page, "c12").locator(".nb-figure img")).toBeVisible({ timeout: 300_000 });
});

test("notebook F: changing the parameter in c1 makes the dependent outputs stale, and running them again shows the new values", async ({ page }) => {
  test.setTimeout(900_000);
  await loadRational(page);
  await page.getByRole("button", { name: "Run all" }).click();
  await expect(cell(page, "c6").locator(".nb-stdout")).toContainText("= 3: solved", { timeout: 300_000 });
  const source = cell(page, "c1").locator("textarea.nb-source");
  await source.fill((await source.inputValue()).replace('return "3"', 'return "5"'));
  // c1 only defines functions, so it records no run and its status stays "not run"
  await expect(cell(page, "c6").locator(".nb-chip")).toHaveText(/stale/);
  await expect(cell(page, "c2").locator(".nb-chip")).toHaveText(/current/);
  await runCell(page, "c6");
  await expect(cell(page, "c6").locator(".nb-stdout")).toContainText("= 5: solved", { timeout: 300_000 });
  await expect(cell(page, "c6").locator(".nb-stdout")).toContainText("solutions: 4");
});

test("notebook F: a broken cell reports its diagnostic, and the notebook still runs once the cell is repaired", async ({ page }) => {
  test.setTimeout(900_000);
  await loadRational(page);
  const source = cell(page, "c8").locator("textarea.nb-source");
  const original = await source.inputValue();
  await source.fill(original.replace("rational.rational_solve(f, g).unwrap()", "rational.rational_solve(f, g).unwrap() +"));
  await runCell(page, "c8");
  await expect(cell(page, "c8").locator(".nb-diagnostic, .nb-error, .nb-stderr").first()).toBeVisible({ timeout: 300_000 });
  await source.fill(original);
  await runCell(page, "c8");
  await expect(cell(page, "c8").locator(".nb-stdout")).toContainText("identity", { timeout: 300_000 });
});

test("notebook F: save, reload and recover the edited parameter and its outputs; an imported file is not run by itself", async ({ page, browser }, testInfo) => {
  test.setTimeout(900_000);
  await loadRational(page);
  await runCell(page, "c1");
  const source = cell(page, "c1").locator("textarea.nb-source");
  await source.fill((await source.inputValue()).replace('return "3"', 'return "7"'));
  await openFileMenu(page);
  const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name: "Export…" }).click()]);
  const exported = Buffer.concat(await (await download.createReadStream()).toArray()).toString("utf8");
  const saved = JSON.parse(exported);
  expect(JSON.stringify(saved.cells)).toContain("return \\\"7\\\"");
  await expect(page.locator(SAVE_STATE)).toHaveAttribute("data-state", "saved", { timeout: 15_000 });
  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
  await expect(cell(page, "c1").locator("textarea.nb-source")).toHaveValue(/return "7"/);

  // import the exported file into a clean context: its cells come back, and nothing runs by itself
  const other = await browser.newContext();
  const clean = await other.newPage();
  await open(clean);
  const file = join(testInfo.outputDir, "rational-exported.ostrin.notebook.json");
  mkdirSync(testInfo.outputDir, { recursive: true });
  writeFileSync(file, exported);
  await clean.locator(INPUT).setInputFiles(file);
  await expect(clean.locator(".nb-cell")).toHaveCount(14, { timeout: 60_000 });
  await expect(clean.locator(".nb-chip", { hasText: "computed in this session" })).toHaveCount(0);
  await other.close();
});
