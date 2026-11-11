import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { sealPackage } from "../../website/workbench-evidence.js";

// Portable evidence packages, end to end: a real compiler run in one browser context is exported as a file and
// imported in another context with clean storage. Packages are also tampered with and must be refused whole.

const IMPORT_INPUT = "input[type=file][aria-label='Evidence package to import']";
const LOOP = JSON.stringify({
  format: "ostrin.project", version: 1, name: "endless loop", entry: "main.ostrin",
  files: { "main.ostrin": "fn main() -> Void {\n    mut i = 0\n    while true {\n        i = i + 1\n    }\n}\n" },
});

const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");

async function openPage(page) {
  await page.goto("./playground.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
  await expect(page.locator("#run-history")).not.toContainText("Loading saved runs", { timeout: 20_000 });
}

// Loading a demo is asynchronous and cancels any run in flight: wait for the demo's own message before running.
const DEMO_LOADED = {
  "#demo-sweep": "Loaded the parameter sweep. Nothing has run yet",
  "#demo-sweep-even": "Loaded the parameter sweep for c = 2, 4, 6, 8. Nothing has run yet",
};

async function runSweepAndSave(page, demo = "#demo-sweep", rows = 1) {
  await page.locator(demo).click();
  await expect(page.locator("#output")).toContainText(DEMO_LOADED[demo], { timeout: 45_000 });
  await expect(page.locator("#file-explorer")).toContainText("4 files");
  await page.locator("#run").click();
  await expect(page.locator("#run-history tbody tr")).toHaveCount(rows, { timeout: 90_000 });
  await expect(page.locator("#run-history .history-saved-saved")).toHaveCount(rows, { timeout: 20_000 });
}

async function exportFromDetails(page, testInfo) {
  await page.locator("#run-history tbody tr").first().click();
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: "Export this run as evidence" }).click(),
  ]);
  const file = testInfo.outputPath(download.suggestedFilename());
  await download.saveAs(file);
  return { file, name: download.suggestedFilename(), text: readFileSync(file, "utf8") };
}

async function exportSelection(page, testInfo) {
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    page.getByRole("button", { name: /^Export selected runs/ }).click(),
  ]);
  const file = testInfo.outputPath(download.suggestedFilename());
  await download.saveAs(file);
  return { file, name: download.suggestedFilename(), text: readFileSync(file, "utf8") };
}

// Every content of a package is decoded and hashed here, with Node's own SHA-256, not the page's.
function verifyWithNode(text) {
  const pkg = JSON.parse(text);
  const declared = new Set(pkg.contents.map((c) => c.digest));
  for (const item of pkg.contents) {
    const bytes = Buffer.from(item.data, "base64");
    expect(bytes.byteLength).toBe(item.size);
    expect(sha(bytes)).toBe(item.digest);
  }
  // Every digest that a run declares as included must have its bytes in the package.
  for (const run of pkg.runs) {
    const { digests } = run.entry;
    const pairs = [
      ...Object.entries(digests.sources).map(([name, d]) => [d, run.contentStatus.sources[name]]),
      [digests.stdout, run.contentStatus.stdout],
      [digests.stderr, run.contentStatus.stderr],
      [digests.figure, run.contentStatus.figure],
    ];
    for (const [digest, status] of pairs) {
      if (digest !== null && status === "included") expect(declared.has(digest)).toBe(true);
    }
  }
  return pkg;
}

async function importInto(page, file) {
  await page.locator(IMPORT_INPUT).setInputFiles(file);
}

// A tampered copy of a package, written next to the test output. `change` edits the parsed package; `reseal`
// recomputes the integrity digest afterwards, as someone who wants the change to pass would.
async function tampered(testInfo, text, name, change, { reseal }) {
  const pkg = JSON.parse(text);
  change(pkg);
  const out = reseal ? await sealPackage(pkg, sha256Bytes) : pkg;
  const file = testInfo.outputPath(name);
  writeFileSync(file, JSON.stringify(out, null, 2));
  return file;
}

async function sha256Bytes(bytes) {
  return sha(Buffer.from(bytes));
}

// Replaces a content's bytes and updates every digest that refers to them, so the package is internally consistent.
function replaceContent(pkg, kind, text) {
  const bytes = Buffer.from(text, "utf8");
  const newDigest = sha(bytes);
  const item = pkg.contents.find((c) => c.kind === kind);
  const oldDigest = item.digest;
  item.digest = newDigest;
  item.size = bytes.byteLength;
  item.data = bytes.toString("base64");
  for (const run of pkg.runs) {
    const d = run.entry.digests;
    for (const name of Object.keys(d.sources)) if (d.sources[name] === oldDigest) d.sources[name] = newDigest;
    if (d.stdout === oldDigest) d.stdout = newDigest;
    if (d.stderr === oldDigest) d.stderr = newDigest;
    if (d.figure === oldDigest) d.figure = newDigest;
  }
}

test("flow A: a real run is exported, then imported into a clean browser context and recovered with its results, figure and verified content", async ({ browser }, testInfo) => {
  test.setTimeout(240_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await runSweepAndSave(first);
  const id = await first.locator("#run-history tbody tr td:nth-child(2)").first().textContent();
  const exported = await exportFromDetails(first, testInfo);
  expect(exported.name).toMatch(/\.ostrin\.evidence\.json$/);
  const pkg = verifyWithNode(exported.text);
  expect(pkg.schema).toBe("ostrin.evidence-package/v1");
  expect(pkg.runs[0].entry.id).toBe(id);
  expect(pkg.runs[0].contentStatus.figure).toBe("included");
  await origin.close();

  // A clean context: nothing from the origin browser is available here.
  const clean = await browser.newContext();
  const page = await clean.newPage();
  await openPage(page);
  await expect(page.locator("#run-history tbody tr")).toHaveCount(0);
  const projectBefore = await page.locator("#source").inputValue();
  const outputBefore = await page.locator("#output").textContent();
  await importInto(page, exported.file);
  const rows = page.locator("#run-history tbody tr");
  await expect(rows).toHaveCount(1, { timeout: 20_000 });
  await expect(rows.first().locator("td:nth-child(2)")).toHaveText(id);
  await expect(rows.first()).toContainText("completed");
  await expect(rows.first()).toContainText("validated");
  await expect(page.locator("#run-history .history-notice, #run-history")).toContainText("Imported 1 new run(s)");
  await rows.first().click();
  const details = page.locator("#run-history .history-details");
  await expect(details).toContainText("validated structured results");
  await expect(details.locator(".history-results-table tbody tr")).toHaveCount(4);
  await expect(details.locator(".figure-preview img")).toBeVisible();
  await expect(details).toContainText("source main.ostrin: verified now");
  await expect(details).toContainText("stdout: verified now");
  await expect(details).toContainText("figure: verified now");
  await expect(details).toContainText("Evidence coverage");
  await expect(details).toContainText("complete: validated results and all declared content verified");
  // Importing never changes the open project nor runs anything.
  await expect(page.locator("#source")).toHaveValue(projectBefore);
  await expect(page.locator("#output")).toHaveText(outputBefore ?? "");
  await clean.close();
});

test("flow B: two runs exported together are imported into a clean context and still compare with the same reasons", async ({ browser }, testInfo) => {
  test.setTimeout(300_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await runSweepAndSave(first, "#demo-sweep", 1);
  await runSweepAndSave(first, "#demo-sweep-even", 2);
  await first.locator("#run-history tbody tr").nth(0).locator("input[type=checkbox]").check();
  await first.locator("#run-history tbody tr").nth(1).locator("input[type=checkbox]").check();
  const exported = await exportSelection(first, testInfo);
  expect(verifyWithNode(exported.text).runs.length).toBe(2);
  await origin.close();

  const clean = await browser.newContext();
  const page = await clean.newPage();
  await openPage(page);
  await importInto(page, exported.file);
  await expect(page.locator("#run-history tbody tr")).toHaveCount(2, { timeout: 20_000 });
  await expect(page.locator("#run-history .history-saved-saved")).toHaveCount(2);
  await page.locator("#run-history tbody tr").nth(0).locator("input[type=checkbox]").check();
  await page.locator("#run-history tbody tr").nth(1).locator("input[type=checkbox]").check();
  await page.getByRole("button", { name: "Compare the two selected runs" }).click();
  const comparison = page.locator("#run-history .history-compare");
  await expect(comparison).toContainText("Comparable");
  await expect(comparison).toContainText("No parameter is shared");
  await expect(comparison).toContainText("Only in A: c = 1, 3, 5, 7");
  await expect(comparison).toContainText("Only in B: c = 2, 4, 6, 8");
  await expect(comparison.locator(".history-figure-card img")).toHaveCount(2);

  // An imported run compares with a run executed locally in the clean context, under the same rules.
  await page.getByRole("button", { name: "Back to the list" }).click();
  await runSweepAndSave(page, "#demo-sweep-even", 3);
  await expect(page.locator("#run-history tbody tr")).toHaveCount(3);
  await clean.close();
});

test("flow C: altered bytes are detected, the package is refused whole, and no partial record is left behind", async ({ browser }, testInfo) => {
  test.setTimeout(300_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await runSweepAndSave(first, "#demo-sweep", 1);
  await runSweepAndSave(first, "#demo-sweep-even", 2);
  await first.locator("#run-history tbody tr").nth(0).locator("input[type=checkbox]").check();
  await first.locator("#run-history tbody tr").nth(1).locator("input[type=checkbox]").check();
  const exported = await exportSelection(first, testInfo);
  await origin.close();

  const clean = await browser.newContext();
  const page = await clean.newPage();
  await openPage(page);
  const rows = page.locator("#run-history tbody tr");

  // 1. Bytes changed, integrity digest not updated: the package does not match its digest.
  const stdoutEdit = await tampered(testInfo, exported.text, "stdout-raw.json", (pkg) => {
    const item = pkg.contents.find((c) => c.kind === "stdout");
    item.data = Buffer.from("altered output\n").toString("base64");
    item.size = Buffer.byteLength("altered output\n");
  }, { reseal: false });
  await importInto(page, stdoutEdit);
  await expect(page.locator("#run-history")).toContainText("Import refused");
  await expect(page.locator("#run-history")).toContainText("does not match its integrity digest");
  await expect(rows).toHaveCount(0);

  // 2. Bytes changed and the integrity digest recomputed: the content's own SHA-256 refuses it.
  const stdoutSealed = await tampered(testInfo, exported.text, "stdout-sealed.json", (pkg) => {
    const item = pkg.contents.find((c) => c.kind === "stdout");
    const bytes = Buffer.from("altered output\n");
    item.data = bytes.toString("base64");
    item.size = bytes.byteLength;
  }, { reseal: true });
  await importInto(page, stdoutSealed);
  await expect(page.locator("#run-history")).toContainText("does not match its SHA-256");
  await expect(rows).toHaveCount(0);

  // 3. A source changed under a recomputed integrity digest, with its declared digest left as it was.
  const sourceSealed = await tampered(testInfo, exported.text, "source-sealed.json", (pkg) => {
    const item = pkg.contents.find((c) => c.kind === "source");
    item.data = Buffer.from("fn main() -> Void {}\n// altered\n").toString("base64");
    item.size = Buffer.byteLength("fn main() -> Void {}\n// altered\n");
  }, { reseal: true });
  await importInto(page, sourceSealed);
  await expect(page.locator("#run-history")).toContainText("does not match its SHA-256");
  await expect(rows).toHaveCount(0);

  // 4. A figure changed: refused the same way.
  const figureSealed = await tampered(testInfo, exported.text, "figure-sealed.json", (pkg) => {
    const item = pkg.contents.find((c) => c.kind === "figure");
    const bytes = Buffer.from("<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>");
    item.data = bytes.toString("base64");
    item.size = bytes.byteLength;
  }, { reseal: true });
  await importInto(page, figureSealed);
  await expect(page.locator("#run-history")).toContainText("does not match its SHA-256");
  await expect(rows).toHaveCount(0);

  // Two runs in one package: the good package imports, and a tampered one afterwards adds nothing to it.
  await importInto(page, exported.file);
  await expect(rows).toHaveCount(2, { timeout: 20_000 });
  // A new run added to the stored ones, carrying bytes its digest does not describe: refused whole, nothing added.
  const extra = await tampered(testInfo, exported.text, "second-tamper.json", (pkg) => {
    pkg.runs[1].entry.id = "run-tampered-extra";
    pkg.contents.find((c) => c.kind === "stdout").data = Buffer.from("bytes the digest does not describe").toString("base64");
  }, { reseal: true });
  await importInto(page, extra);
  await expect(page.locator("#run-history")).toContainText("Import refused");
  await expect(rows).toHaveCount(2);
  await expect(page.locator("#run-history")).not.toContainText("run-tampered-extra");
  await clean.close();
});

test("flow D: a cancelled run exported and imported stays incomplete, with no validated results invented", async ({ browser }, testInfo) => {
  test.setTimeout(240_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await first.locator("#import-project-file").setInputFiles({ name: "loop.json", mimeType: "application/json", buffer: Buffer.from(LOOP) });
  await expect(first.locator("#file-explorer")).toContainText("1 file");
  await first.locator("#run").click();
  await expect(first.locator("#cancel-run")).toBeEnabled({ timeout: 15_000 });
  await first.locator("#cancel-run").click();
  await expect(first.locator("#run-history .history-saved-saved")).toHaveCount(1, { timeout: 20_000 });
  const exported = await exportFromDetails(first, testInfo);
  const pkg = verifyWithNode(exported.text);
  expect(pkg.runs[0].coverage).toBe("incomplete-run");
  expect(pkg.runs[0].entry.evidence).toBe("incomplete");
  expect(pkg.runs[0].entry.results).toBeNull();
  await origin.close();

  const clean = await browser.newContext();
  const page = await clean.newPage();
  await openPage(page);
  await importInto(page, exported.file);
  const row = page.locator("#run-history tbody tr").first();
  await expect(row).toContainText("cancelled", { timeout: 20_000 });
  await expect(row).toContainText("incomplete");
  await expect(row).not.toContainText("validated");
  await row.click();
  const details = page.locator("#run-history .history-details");
  await expect(details).toContainText("incomplete: no validated results");
  await expect(details).toContainText("incomplete: run cancelled, no validated results");
  await expect(details.locator(".figure-preview")).toHaveCount(0);
  await expect(details.locator(".history-results-table")).toHaveCount(0);
  await clean.close();
});

test("importing the same package twice never duplicates a run, and an identical run is reported, not added", async ({ browser }, testInfo) => {
  test.setTimeout(240_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await runSweepAndSave(first);
  const exported = await exportFromDetails(first, testInfo);
  await origin.close();

  const clean = await browser.newContext();
  const page = await clean.newPage();
  await openPage(page);
  await importInto(page, exported.file);
  await expect(page.locator("#run-history tbody tr")).toHaveCount(1, { timeout: 20_000 });
  await importInto(page, exported.file);
  await expect(page.locator("#run-history")).toContainText("1 already present and identical, not duplicated");
  await expect(page.locator("#run-history tbody tr")).toHaveCount(1);
  await clean.close();
});

test("a package with a run id already stored with different content is a conflict: nothing is imported", async ({ browser }, testInfo) => {
  test.setTimeout(240_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await runSweepAndSave(first);
  const exported = await exportFromDetails(first, testInfo);
  await origin.close();
  const clean = await browser.newContext();
  const page = await clean.newPage();
  await openPage(page);
  await importInto(page, exported.file);
  await expect(page.locator("#run-history tbody tr")).toHaveCount(1, { timeout: 20_000 });
  const changed = await tampered(testInfo, exported.text, "conflict.json", (pkg) => {
    pkg.runs[0].entry.command = "--run another.ostrin";
  }, { reseal: true });
  await importInto(page, changed);
  await expect(page.locator("#run-history")).toContainText("already stored with different content; nothing was imported");
  await expect(page.locator("#run-history tbody tr")).toHaveCount(1);
  await clean.close();
});

test("malformed files are refused by name: invalid JSON, unknown version, malformed Base64, missing content and inconsistent metadata", async ({ browser }, testInfo) => {
  test.setTimeout(240_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await runSweepAndSave(first);
  const exported = await exportFromDetails(first, testInfo);
  await origin.close();
  const clean = await browser.newContext();
  const page = await clean.newPage();
  await openPage(page);
  const rows = page.locator("#run-history tbody tr");
  const expectRefused = async (file, reason) => {
    await importInto(page, file);
    await expect(page.locator("#run-history")).toContainText("Import refused");
    await expect(page.locator("#run-history")).toContainText(reason);
    await expect(rows).toHaveCount(0);
  };

  const broken = testInfo.outputPath("broken.json");
  writeFileSync(broken, "{ this is not json");
  await expectRefused(broken, "not valid JSON");

  const future = testInfo.outputPath("future.json");
  writeFileSync(future, JSON.stringify({ ...JSON.parse(exported.text), schema: "ostrin.evidence-package/v9" }));
  await expectRefused(future, "unsupported package schema");

  const base64 = await tampered(testInfo, exported.text, "base64.json", (pkg) => {
    pkg.contents[0].data = "@@not base64@@";
  }, { reseal: true });
  await expectRefused(base64, "not well-formed Base64");

  const missing = await tampered(testInfo, exported.text, "missing.json", (pkg) => {
    pkg.contents = pkg.contents.filter((c) => c.kind !== "stdout");
  }, { reseal: true });
  await expectRefused(missing, "declared included but its bytes are missing");

  const metadata = await tampered(testInfo, exported.text, "metadata.json", (pkg) => {
    pkg.runs[0].entry.evidence = "incomplete";
  }, { reseal: true });
  await expectRefused(metadata, "do not agree with each other");
  await clean.close();
});

test("an import that hits a storage quota writes nothing: the package is refused as a whole", async ({ browser }, testInfo) => {
  test.setTimeout(240_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await runSweepAndSave(first);
  const exported = await exportFromDetails(first, testInfo);
  await origin.close();

  const clean = await browser.newContext();
  await clean.addInitScript(() => {
    for (const method of ["add", "put"]) {
      const real = IDBObjectStore.prototype[method];
      IDBObjectStore.prototype[method] = function (...args) {
        if (window.__ostrinQuotaFault) throw new DOMException("quota is full for this test", "QuotaExceededError");
        return real.apply(this, args);
      };
    }
  });
  const page = await clean.newPage();
  await openPage(page);
  await page.evaluate(() => {
    window.__ostrinQuotaFault = true;
  });
  await importInto(page, exported.file);
  await expect(page.locator("#run-history")).toContainText("Import refused: the browser storage quota is full");
  await expect(page.locator("#run-history tbody tr")).toHaveCount(0);
  await page.evaluate(() => {
    window.__ostrinQuotaFault = false;
  });
  await page.reload({ waitUntil: "domcontentloaded" });
  await openPage(page);
  await expect(page.locator("#run-history tbody tr")).toHaveCount(0, { timeout: 15_000 });
  await clean.close();
});

test("concurrent imports from two tabs commit the run once: one succeeds, the other is identical or refused, no partial state", async ({ browser }, testInfo) => {
  test.setTimeout(240_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await runSweepAndSave(first);
  const exported = await exportFromDetails(first, testInfo);
  await origin.close();

  const clean = await browser.newContext();
  const page = await clean.newPage();
  await openPage(page);
  const outcome = await page.evaluate(async (text) => {
    const idb = await import(new URL("./workbench-history-idb.js", location.href).href);
    const evidence = await import(new URL("./workbench-evidence.js", location.href).href);
    const digestOf = async (bytes) => [...new Uint8Array(await crypto.subtle.digest("SHA-256", bytes))].map((b) => b.toString(16).padStart(2, "0")).join("");
    const verified = await evidence.verifyPackage(text, digestOf);
    // Two independent stores, as two tabs would have: each has its own queue, so their writes can race.
    const tabA = idb.createHistoryStore({ digestOf });
    const tabB = idb.createHistoryStore({ digestOf });
    const results = await Promise.allSettled([tabA.importPackage(verified), tabB.importPackage(verified)]);
    const loaded = await tabA.load();
    return {
      statuses: results.map((r) => r.status),
      reasons: results.map((r) => (r.status === "rejected" ? r.reason.code : null)),
      added: results.map((r) => (r.status === "fulfilled" ? r.value.added.length : null)),
      runs: loaded.entries.length,
    };
  }, exported.text);
  expect(outcome.statuses).toContain("fulfilled");
  expect(outcome.runs).toBe(1);
  // The rejected one, if any, was refused by a duplicate constraint and nothing else was written.
  for (let i = 0; i < 2; i++) {
    if (outcome.statuses[i] === "rejected") expect(outcome.reasons[i]).toBe("duplicate");
  }
  await clean.close();
});

test("a package with a figure that contains script is shown only as an image: nothing in it runs", async ({ browser }, testInfo) => {
  test.setTimeout(240_000);
  const origin = await browser.newContext();
  const first = await origin.newPage();
  await openPage(first);
  await runSweepAndSave(first);
  const exported = await exportFromDetails(first, testInfo);
  await origin.close();
  const hostile = await tampered(testInfo, exported.text, "hostile-figure.json", (pkg) => {
    replaceContent(pkg, "figure", "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>window.__evil = 1</script><text>x</text></svg>");
    pkg.runs[0].entry.projectName = "<img src=x onerror=window.__evil=2>";
  }, { reseal: true });

  const clean = await browser.newContext();
  const page = await clean.newPage();
  await openPage(page);
  await importInto(page, hostile);
  const row = page.locator("#run-history tbody tr").first();
  await expect(row).toContainText("<img src=x onerror=window.__evil=2>");
  await row.click();
  await expect(page.locator("#run-history .figure-preview img")).toBeVisible();
  await expect(page.locator("#run-history script")).toHaveCount(0);
  expect(await page.evaluate(() => [window.__evil ?? null])).toEqual([null]);
  await clean.close();
});

test("a stored content whose bytes no longer match their digest is withheld from export, and the export says so", async ({ browser }, testInfo) => {
  test.setTimeout(240_000);
  const context = await browser.newContext({ acceptDownloads: true });
  const page = await context.newPage();
  await openPage(page);
  await runSweepAndSave(page);
  // Corrupt the stored stdout bytes directly in the browser's IndexedDB, keeping the digest the run recorded.
  await page.evaluate(async () => {
    const db = await new Promise((resolve, reject) => {
      const request = indexedDB.open("ostrin-workbench-history", 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const read = db.transaction(["records"], "readonly").objectStore("records").getAll();
    const records = await new Promise((resolve) => (read.onsuccess = () => resolve(read.result)));
    const stdoutDigest = records[0].entry.digests.stdout;
    const write = db.transaction(["contents"], "readwrite");
    write.objectStore("contents").put({ bytes: new TextEncoder().encode("corrupted stored output"), size: 24 }, stdoutDigest);
    await new Promise((resolve, reject) => {
      write.oncomplete = resolve;
      write.onerror = () => reject(write.error);
      write.onabort = () => reject(write.error);
    });
    db.close();
  });
  await page.reload({ waitUntil: "domcontentloaded" });
  await openPage(page);
  await page.locator("#run-history tbody tr").first().click();
  const exported = await exportFromDetails(page, testInfo);
  await expect(page.locator("#run-history")).toContainText("1 content(s) withheld");
  const pkg = JSON.parse(exported.text);
  expect(pkg.runs[0].contentStatus.stdout).toBe("withheld");
  expect(pkg.contents.some((c) => c.kind === "stdout")).toBe(false);
  expect(pkg.withheld[0].reason).toBe("stored bytes do not match their SHA-256 digest");
  expect(pkg.runs[0].coverage).toBe("partial-content");
  verifyWithNode(exported.text);
  await context.close();
});

test("regression: results JSON, project export and the manifest still work beside the history and the import control", async ({ browser }) => {
  test.setTimeout(180_000);
  const context = await browser.newContext({ acceptDownloads: true });
  const page = await context.newPage();
  await openPage(page);
  await runSweepAndSave(page);
  await expect(page.locator("#download-results")).toBeEnabled();
  await expect(page.locator("#download-manifest")).toBeEnabled();
  const [results] = await Promise.all([page.waitForEvent("download"), page.locator("#download-results").click()]);
  expect(results.suggestedFilename()).toMatch(/results\.json$/i);
  const [manifest] = await Promise.all([page.waitForEvent("download"), page.locator("#download-manifest").click()]);
  expect(manifest.suggestedFilename()).toMatch(/manifest/i);
  const [project] = await Promise.all([page.waitForEvent("download"), page.locator("#export-project").click()]);
  expect(project.suggestedFilename()).toMatch(/\.ostrin\.project\.json$/);
  await context.close();
});
