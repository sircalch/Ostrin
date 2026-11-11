import { expect, test } from "@playwright/test";

// Persistence of the Run History in IndexedDB. Every test uses the real compiler and the real Workbench; storage
// faults are injected in the page before its scripts run (addInitScript), never by replacing the Run History itself.

const DB_NAME = "ostrin-workbench-history";
const LOOP = JSON.stringify({
  format: "ostrin.project", version: 1, name: "endless loop", entry: "main.ostrin",
  files: { "main.ostrin": "fn main() -> Void {\n    mut i = 0\n    while true {\n        i = i + 1\n    }\n}\n" },
});

async function openPage(page) {
  await page.goto("./playground.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 45_000 });
  // The saved runs are read in the background; the panel says so until they are.
  await expect(page.locator("#run-history")).not.toContainText("Loading saved runs", { timeout: 20_000 });
}

async function reload(page) {
  await page.reload({ waitUntil: "domcontentloaded" });
  await openPage(page);
}

async function loadSweep(page, button = "#demo-sweep") {
  await page.locator(button).click();
  await expect(page.locator("#file-explorer")).toContainText("4 files");
}

async function runAndWaitForSaved(page, rowCount) {
  await page.locator("#run").click();
  await expect(page.locator("#run-history tbody tr")).toHaveCount(rowCount, { timeout: 90_000 });
  await expect(page.locator("#run-history .history-saved-saved")).toHaveCount(rowCount, { timeout: 20_000 });
}

test("a real run is saved, survives a reload with its id, results, figure and verified content", async ({ page }) => {
  test.setTimeout(150_000);
  await openPage(page);
  await loadSweep(page);
  const source = await page.locator("#source").inputValue();
  await runAndWaitForSaved(page, 1);
  const id = await page.locator("#run-history tbody tr td:nth-child(2)").first().textContent();

  await reload(page);
  const rows = page.locator("#run-history tbody tr");
  await expect(rows).toHaveCount(1);
  await expect(rows.first().locator("td:nth-child(2)")).toHaveText(id);
  await expect(rows.first()).toContainText("completed");
  await expect(rows.first()).toContainText("validated");
  await rows.first().click();
  const details = page.locator("#run-history .history-details");
  await expect(details).toContainText("validated structured results");
  await expect(details.locator(".history-results-table tbody tr")).toHaveCount(4);
  await expect(details.locator(".figure-preview img")).toBeVisible();
  await expect(details).toContainText("Storage");
  await expect(details).toContainText("source main.ostrin: verified now");
  await expect(details).toContainText("figure: verified now");
  await expect(details).not.toContainText("digest only");
  // The open project is the one the page loaded; recovery never replaced it.
  await expect(page.locator("#source")).toHaveValue(source);
  await expect(page.locator("#file-explorer")).toContainText("4 files");
  await details.getByRole("button", { name: "Back to the open project" }).click();
  await expect(page.locator("#run-history .history-details")).toHaveCount(0);
});

test("two independent runs are both recovered after a reload and compare with the same reasons", async ({ page }) => {
  test.setTimeout(240_000);
  await openPage(page);
  await loadSweep(page);
  await runAndWaitForSaved(page, 1);
  await loadSweep(page, "#demo-sweep-even");
  await runAndWaitForSaved(page, 2);

  await reload(page);
  const rows = page.locator("#run-history tbody tr");
  await expect(rows).toHaveCount(2);
  await expect(page.locator("#run-history .history-saved-saved")).toHaveCount(2);
  await rows.nth(0).locator("input[type=checkbox]").check();
  await rows.nth(1).locator("input[type=checkbox]").check();
  await page.getByRole("button", { name: "Compare the two selected runs" }).click();
  const comparison = page.locator("#run-history .history-compare");
  await expect(comparison).toContainText("Comparable");
  await expect(comparison).toContainText("No parameter is shared");
  await expect(comparison).toContainText("Only in A: c = 1, 3, 5, 7");
  await expect(comparison).toContainText("Only in B: c = 2, 4, 6, 8");
  await expect(comparison.locator(".history-figure-card img")).toHaveCount(2);
});

test("a cancelled run is recovered as incomplete evidence and is never shown as validated", async ({ page }) => {
  test.setTimeout(150_000);
  await openPage(page);
  await page.locator("#import-project-file").setInputFiles({ name: "loop.json", mimeType: "application/json", buffer: Buffer.from(LOOP) });
  await expect(page.locator("#file-explorer")).toContainText("1 file");
  await page.locator("#run").click();
  await expect(page.locator("#cancel-run")).toBeEnabled({ timeout: 15_000 });
  await page.locator("#cancel-run").click();
  await expect(page.locator("#run-history .history-saved-saved")).toHaveCount(1, { timeout: 20_000 });

  await reload(page);
  const row = page.locator("#run-history tbody tr").first();
  await expect(row).toContainText("cancelled");
  await expect(row).toContainText("incomplete");
  await expect(row).not.toContainText("validated");
  await row.click();
  await expect(page.locator("#run-history .history-details")).toContainText("incomplete: no validated results");
  // The source bytes of a cancelled run exist and are stored; no stdout or figure was produced, so none is claimed.
  await expect(page.locator("#run-history .history-details")).toContainText("source main.ostrin: verified now");
  await expect(page.locator("#run-history .history-details")).not.toContainText("figure:");
  await expect(page.locator("#run-history .history-details .figure-preview")).toHaveCount(0);
});

test("when IndexedDB cannot be opened the page keeps working, says so, and marks each run as not saved", async ({ page }) => {
  test.setTimeout(150_000);
  await page.addInitScript(() => {
    IDBFactory.prototype.open = function () {
      throw new DOMException("storage blocked for this test", "SecurityError");
    };
  });
  await openPage(page);
  await expect(page.locator("#run-history")).toContainText("Saved history is unavailable");
  await loadSweep(page);
  const source = await page.locator("#source").inputValue();
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 90_000 });
  await expect(page.locator("#run-history tbody tr")).toHaveCount(1, { timeout: 20_000 });
  await expect(page.locator("#run-history .history-saved-not-saved")).toHaveCount(1);
  await expect(page.locator("#run-history")).toContainText("not saved");
  await expect(page.locator("#source")).toHaveValue(source);
  await expect(page.locator("#file-explorer")).toContainText("4 files");
});

test("a write refused for quota is shown as not saved, the run stays visible, and nothing is saved behind it", async ({ page }) => {
  test.setTimeout(150_000);
  await page.addInitScript(() => {
    for (const method of ["add", "put"]) {
      const real = IDBObjectStore.prototype[method];
      IDBObjectStore.prototype[method] = function (...args) {
        if (window.__ostrinQuotaFault) throw new DOMException("quota is full for this test", "QuotaExceededError");
        return real.apply(this, args);
      };
    }
  });
  await openPage(page);
  await loadSweep(page);
  const source = await page.locator("#source").inputValue();
  await page.evaluate(() => {
    window.__ostrinQuotaFault = true;
  });
  await page.locator("#run").click();
  await expect(page.locator("#run-history tbody tr")).toHaveCount(1, { timeout: 90_000 });
  const savedCell = page.locator("#run-history .history-saved-not-saved");
  await expect(savedCell).toHaveCount(1, { timeout: 20_000 });
  await expect(savedCell).toHaveAttribute("title", "the browser storage quota is full");
  await expect(page.locator("#run-history")).toContainText("was not saved permanently: the browser storage quota is full");
  await expect(page.locator("#source")).toHaveValue(source);
  await expect(page.locator("#file-explorer")).toContainText("4 files");

  // After a reload with storage working, the refused run is not reported as saved, because it never was.
  await reload(page);
  await expect(page.locator("#run-history")).toContainText("No runs recorded yet");
});

test("a damaged saved record is isolated: the other runs load and the damaged one is reported, not deleted", async ({ page }) => {
  test.setTimeout(150_000);
  await openPage(page);
  await loadSweep(page);
  await runAndWaitForSaved(page, 1);
  await page.evaluate(async (name) => {
    const db = await new Promise((resolve, reject) => {
      const request = indexedDB.open(name, 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const tx = db.transaction(["records"], "readwrite");
    tx.objectStore("records").put({ schema: "ostrin.history-record/v1", entry: { id: "run-broken", status: "completed" } }, "run-broken");
    await new Promise((resolve, reject) => {
      tx.oncomplete = resolve;
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
    db.close();
  }, DB_NAME);

  await reload(page);
  await expect(page.locator("#run-history tbody tr")).toHaveCount(1);
  await expect(page.locator("#run-history tbody tr").first()).toContainText("validated");
  await expect(page.locator("#run-history")).toContainText("1 saved record(s) could not be read and were not loaded (run-broken)");
  await expect(page.locator("#run-history")).toContainText("nothing was deleted");
});

test("a stored figure whose bytes were altered is detected on recovery and is not shown", async ({ page }) => {
  test.setTimeout(150_000);
  await openPage(page);
  await loadSweep(page);
  await runAndWaitForSaved(page, 1);
  await page.evaluate(async (name) => {
    const db = await new Promise((resolve, reject) => {
      const request = indexedDB.open(name, 1);
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const read = db.transaction(["records"], "readonly").objectStore("records").getAll();
    const records = await new Promise((resolve) => (read.onsuccess = () => resolve(read.result)));
    const digest = records[0].entry.digests.figure;
    const write = db.transaction(["contents"], "readwrite");
    write.objectStore("contents").put({ bytes: new TextEncoder().encode("<svg>altered</svg>"), size: 18 }, digest);
    await new Promise((resolve, reject) => {
      write.oncomplete = resolve;
      write.onerror = () => reject(write.error);
      write.onabort = () => reject(write.error);
    });
    db.close();
  }, DB_NAME);

  await reload(page);
  const row = page.locator("#run-history tbody tr").first();
  await expect(row).toContainText("completed");
  await row.click();
  const details = page.locator("#run-history .history-details");
  await expect(details).toContainText("figure: altered: failed verification, not used");
  await expect(details).toContainText("Figure not available: its stored bytes are altered");
  await expect(details.locator(".figure-preview img")).toHaveCount(0);
  await expect(page.locator("#run-history")).toContainText("1 stored content(s) failed verification");
});

test("the store refuses a duplicate id without overwriting, keeps concurrent writes in order, and leaves no partial run after a failed write", async ({ page }) => {
  test.setTimeout(120_000);
  await openPage(page);
  const outcome = await page.evaluate(async () => {
    const idb = await import(new URL("./workbench-history-idb.js", location.href).href);
    const history = await import(new URL("./workbench-history.js", location.href).href);
    const hex = (buffer) => [...new Uint8Array(buffer)].map((b) => b.toString(16).padStart(2, "0")).join("");
    const encoder = new TextEncoder();
    const digestOf = async (bytes) => hex(await crypto.subtle.digest("SHA-256", bytes));
    const makeRun = async (id, text) => ({
      entry: history.runRecord({
        id, projectId: "t", projectName: "t", entry: "main.ostrin", command: "--run main.ostrin", status: "cancelled",
        startedAt: "2026-11-10T12:00:00.000Z",
        digests: { sources: { "main.ostrin": await digestOf(encoder.encode(text)) }, stdout: null, stderr: null, figure: null, manifest: null },
      }),
      contents: [{ bytes: encoder.encode(text) }],
    });
    const name = "ostrin-test-" + Date.now() + "-" + Math.random().toString(36).slice(2);
    const store = idb.createHistoryStore({ name, digestOf });
    const readContents = async () => {
      const db = await new Promise((resolve, reject) => {
        const request = indexedDB.open(name, 1);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      const count = await new Promise((resolve) => {
        const r = db.transaction(["contents"], "readonly").objectStore("contents").count();
        r.onsuccess = () => resolve(r.result);
      });
      db.close();
      return count;
    };

    // Concurrent writes: all six are committed, and none is lost.
    const runs = await Promise.all(["run-a", "run-b", "run-c", "run-d", "run-e", "run-f"].map(async (id, i) => makeRun(id, "source " + i)));
    await Promise.all(runs.map((r) => store.record(r.entry, r.contents)));
    const loaded = await store.load();
    const idsAfterWrites = loaded.entries.map((e) => e.entry.id).sort();

    // A duplicate id is refused, and the stored run keeps its original content.
    const duplicate = await makeRun("run-a", "a different text under the same id");
    let duplicateCode = null;
    try {
      await store.record(duplicate.entry, duplicate.contents);
    } catch (error) {
      duplicateCode = error.code;
    }
    const afterDuplicate = await store.load();
    const originalStillStored = afterDuplicate.entries.filter((e) => e.entry.id === "run-a").length;

    // A quota failure during a write leaves no record and no orphaned content behind.
    const before = await readContents();
    const failing = await makeRun("run-quota", "content that must not stay behind");
    const realAdd = IDBObjectStore.prototype.add;
    const realPut = IDBObjectStore.prototype.put;
    let quotaCode = null;
    try {
      IDBObjectStore.prototype.add = function () { throw new DOMException("quota", "QuotaExceededError"); };
      IDBObjectStore.prototype.put = function () { throw new DOMException("quota", "QuotaExceededError"); };
      await store.record(failing.entry, failing.contents);
    } catch (error) {
      quotaCode = error.code;
    } finally {
      IDBObjectStore.prototype.add = realAdd;
      IDBObjectStore.prototype.put = realPut;
    }
    const after = await readContents();
    const finalLoad = await store.load();
    const quotaRunStored = finalLoad.entries.some((e) => e.entry.id === "run-quota");

    indexedDB.deleteDatabase(name);
    return { idsAfterWrites, duplicateCode, originalStillStored, quotaCode, contentsBefore: before, contentsAfter: after, quotaRunStored };
  });

  expect(outcome.idsAfterWrites).toEqual(["run-a", "run-b", "run-c", "run-d", "run-e", "run-f"]);
  expect(outcome.duplicateCode).toBe("duplicate");
  expect(outcome.originalStillStored).toBe(1);
  expect(outcome.quotaCode).toBe("quota");
  expect(outcome.contentsAfter).toBe(outcome.contentsBefore);
  expect(outcome.quotaRunStored).toBe(false);
});
