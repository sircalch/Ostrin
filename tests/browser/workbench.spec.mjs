import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

const MAIN = "import helpers\n\nfn main() -> Void {\n    print(helpers.square(7).to_string())\n}\n";
const HELPERS = "pub fn square(x: Int) -> Int {\n    x * x\n}\n";
const sha256 = (text) => createHash("sha256").update(text, "utf8").digest("hex");

async function openPlayground(page) {
  await page.goto("./playground.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
}

async function addFile(page, name) {
  page.once("dialog", (dialog) => dialog.accept(name));
  await page.locator("#file-explorer").getByRole("button", { name: "New file" }).click();
}

async function selectFile(page, name) {
  await page.locator(`#file-explorer [data-path="${name}"]`).click();
}

function editor(page) {
  return page.getByLabel("Ostrin source", { exact: true });
}

// Builds the two-file project through the real UI: helpers.ostrin is created and edited, then
// main.ostrin imports it.
async function buildTwoFileProject(page) {
  await openPlayground(page);
  await page.locator("#example").selectOption("Symbolic derivatives");
  await addFile(page, "helpers.ostrin");
  await expect(page.locator('#file-explorer [data-path="helpers.ostrin"]')).toBeVisible();
  await editor(page).fill(HELPERS);
  await selectFile(page, "main.ostrin");
  await editor(page).fill(MAIN);
  await expect(page.locator("#file-explorer")).toContainText("2 files");
}

test("a two-file project runs through the real compiler and prints 49", async ({ page }) => {
  await buildTwoFileProject(page);
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("49", { timeout: 60_000 });
  await expect(page.locator('#file-explorer [data-path="main.ostrin"] .file-badge')).toHaveText("entry");
});

test("switching files keeps each file's unsaved text", async ({ page }) => {
  await buildTwoFileProject(page);
  await selectFile(page, "helpers.ostrin");
  await expect(editor(page)).toHaveValue(/pub fn square/);
  await editor(page).fill("pub fn square(x: Int) -> Int {\n    x * x * 1\n}\n");
  await selectFile(page, "main.ostrin");
  await expect(editor(page)).toHaveValue(/import helpers/);
  await selectFile(page, "helpers.ostrin");
  await expect(editor(page)).toHaveValue(/x \* x \* 1/);
  await expect(page.locator('#file-explorer [data-path="helpers.ostrin"] .file-dirty')).toBeVisible();
});

test("export, reload, import and rerun reproduce the same output and manifest hashes", async ({ page }) => {
  await buildTwoFileProject(page);
  const [projectDownload] = await Promise.all([
    page.waitForEvent("download"),
    page.locator("#export-project").click(),
  ]);
  expect(projectDownload.suggestedFilename()).toMatch(/\.ostrin\.project\.json$/);
  const projectPath = await projectDownload.path();
  const savedText = await readFile(projectPath, "utf8");
  const saved = JSON.parse(savedText);
  expect(saved.format).toBe("ostrin.project");
  expect(saved.version).toBe(1);
  expect(saved.files["helpers.ostrin"]).toBe(HELPERS);
  expect(saved.files["main.ostrin"]).toBe(MAIN);

  await page.reload({ waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
  await page.locator("#import-project-file").setInputFiles(projectPath);
  await expect(page.locator('#file-explorer [data-path="helpers.ostrin"]')).toBeVisible();
  await selectFile(page, "main.ostrin");
  await expect(editor(page)).toHaveValue(/helpers\.square\(7\)/);

  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("49", { timeout: 60_000 });

  const [manifestDownload] = await Promise.all([
    page.waitForEvent("download"),
    page.locator("#download-manifest").click(),
  ]);
  const manifest = JSON.parse(await readFile(await manifestDownload.path(), "utf8"));
  expect(manifest.format).toBe("ostrin.run-manifest");
  expect(manifest.project.entry).toBe("main.ostrin");
  expect(manifest.project.sources["main.ostrin"].sha256).toBe(sha256(MAIN));
  expect(manifest.project.sources["helpers.ostrin"].sha256).toBe(sha256(HELPERS));
  expect(manifest.outputs.stdout.sha256).toBe(sha256("49\n"));
  expect(manifest.run.compiler).toMatch(/^ostrinc /);
  expect(manifest.run.compilerSource).toBe("reported by the runtime (--version)");
  expect(manifest.claims.reproducibility).toBe("not assessed (no R3 claim)");
});

test("a project file with an invalid path is refused and the current project is kept", async ({ page }) => {
  await buildTwoFileProject(page);
  const invalid = JSON.stringify({ format: "ostrin.project", version: 1, name: "bad", entry: "main.ostrin", files: { "../escape.ostrin": "x", "main.ostrin": "x" } });
  await page.locator("#import-project-file").setInputFiles({ name: "bad.ostrin.project.json", mimeType: "application/json", buffer: Buffer.from(invalid) });
  await expect(page.locator("#status")).toContainText("Import refused");
  await expect(page.locator('#file-explorer [data-path="helpers.ostrin"]')).toBeVisible();
  await expect(page.locator("#file-explorer")).toContainText("2 files");
});

test("malformed JSON and oversized projects are refused without changing the project", async ({ page }) => {
  await buildTwoFileProject(page);
  await page.locator("#import-project-file").setInputFiles({ name: "broken.json", mimeType: "application/json", buffer: Buffer.from("{not json") });
  await expect(page.locator("#status")).toContainText("Import refused");
  await expect(page.locator("#file-explorer")).toContainText("2 files");

  const oversized = Buffer.alloc(2 * 1024 * 1024 + 1, 32);
  await page.locator("#import-project-file").setInputFiles({ name: "big.json", mimeType: "application/json", buffer: oversized });
  await expect(page.locator("#status")).toContainText("too large");
  await expect(page.locator("#file-explorer")).toContainText("2 files");
});

test("compiler errors are shown from the real compiler, not simulated", async ({ page }) => {
  await openPlayground(page);
  await editor(page).fill('fn main() -> Void {\n    print(1 + "a")\n}\n');
  await page.locator("#check").click();
  await expect(page.locator("#output")).toContainText("OSTRIN-E1041", { timeout: 60_000 });
});

test("single-file examples still run without the project controls changing their result", async ({ page }) => {
  await openPlayground(page);
  await page.locator("#example").selectOption("Symbolic derivatives");
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("d(x^3)/dx = 3*x^2", { timeout: 60_000 });
});

test("the page keeps working when browser storage throws", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      get() {
        throw new Error("storage blocked");
      },
    });
  });
  await openPlayground(page);
  await page.locator("#example").selectOption("Symbolic derivatives");
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("d(x^3)/dx = 3*x^2", { timeout: 60_000 });
});

test("a figure from the Workbench can be downloaded as SVG", async ({ page }) => {
  await openPlayground(page);
  await page.locator("#example").selectOption("Symbolic model to figure");
  await page.locator("#run").click();
  await expect(page.locator(".figure-preview img")).toBeVisible({ timeout: 60_000 });
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.locator("[data-download-svg]").click(),
  ]);
  expect(download.suggestedFilename()).toBe("figure.svg");
});

test("the manifest is unavailable until a run has been recorded", async ({ page }) => {
  await openPlayground(page);
  await expect(page.locator("#download-manifest")).toBeDisabled();
});
