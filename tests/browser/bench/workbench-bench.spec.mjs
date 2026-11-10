import { expect, test } from "@playwright/test";
import { execSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPS_COLD = Number(process.env.BENCH_COLD ?? 5);
const REPS_WARM = Number(process.env.BENCH_WARM ?? 10);
const REPS_CANCEL = Number(process.env.BENCH_CANCEL ?? 5);
const here = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(here, "..", "..", "..", "docs", "benchmarks");

const LOOP = JSON.stringify({
  format: "ostrin.project", version: 1, name: "endless loop", entry: "main.ostrin",
  files: { "main.ostrin": "fn main() -> Void {\n    mut i = 0\n    while true {\n        i = i + 1\n    }\n}\n" },
});

function stats(values) {
  const xs = values.filter((v) => Number.isFinite(v)).sort((a, b) => a - b);
  if (xs.length === 0) return { n: 0 };
  const mean = xs.reduce((s, v) => s + v, 0) / xs.length;
  const sd = Math.sqrt(xs.reduce((s, v) => s + (v - mean) ** 2, 0) / Math.max(1, xs.length - 1));
  const q = (p) => {
    const i = (xs.length - 1) * p;
    const lo = Math.floor(i);
    return xs[lo] + (xs[Math.ceil(i)] - xs[lo]) * (i - lo);
  };
  return { n: xs.length, median: q(0.5), min: xs[0], max: xs[xs.length - 1], mean, sd, q1: q(0.25), q3: q(0.75) };
}

function environment(browserVersion) {
  let commit = "unknown";
  try { commit = execSync("git rev-parse HEAD", { cwd: path.join(here, "..", "..", "..") }).toString().trim(); } catch {}
  return { os: `${os.type()} ${os.release()} ${os.arch()}`, cpus: os.cpus()[0]?.model ?? "unknown", cpuCount: os.cpus().length, browser: `chromium ${browserVersion}`, commit, measuredAt: new Date().toISOString() };
}

async function loadPlayground(page) {
  const started = Date.now();
  await page.goto("./playground.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("#run")).toBeEnabled({ timeout: 90_000 });
  return Date.now() - started;
}

async function runSweep(page) {
  await page.locator("#demo-sweep").click();
  await expect(page.locator("#file-explorer")).toContainText("4 files");
  await page.locator("#run").click();
  await expect(page.locator("#output")).toContainText("all checks passed", { timeout: 120_000 });
  return page.evaluate(() => window.__ostrinLastRun);
}

test("Workbench Delta measurements: cold and warm Worker runs, cancellation and editor responsiveness", async ({ browser }) => {
  test.setTimeout(3_600_000);
  const version = browser.version();
  const cold = { loadMs: [], compileMs: [], instantiateMs: [], runMs: [], transferMs: [], totalMs: [], workerCreateMs: [], wallMs: [] };
  for (let i = 0; i < REPS_COLD; i += 1) {
    // A new browser context per cold run, so ostrinc.wasm is not served from a previous run's HTTP cache.
    const coldContext = await browser.newContext();
    const page = await coldContext.newPage();
    cold.loadMs.push(await loadPlayground(page));
    const timing = await runSweep(page);
    cold.compileMs.push(timing.compileMs);
    cold.instantiateMs.push(timing.instantiateMs);
    cold.runMs.push(timing.runMs);
    cold.transferMs.push(timing.transferMs);
    cold.totalMs.push(timing.totalMs);
    cold.workerCreateMs.push(timing.workerCreateMs);
    cold.wallMs.push(timing.wallMs);
    await coldContext.close();
  }
  const context = await browser.newContext();

  const warm = { compileMs: [], instantiateMs: [], runMs: [], transferMs: [], totalMs: [], wallMs: [] };
  const page = await context.newPage();
  // The loop below imports over an edited project; the benchmark accepts that discard prompt by design.
  page.on("dialog", (dialog) => dialog.accept());
  await loadPlayground(page);
  await runSweep(page); // the first run warms the module; it is not counted
  for (let i = 0; i < REPS_WARM; i += 1) {
    const timing = await runSweep(page);
    if (!timing.workerReused) throw new Error("a warm run must reuse the Worker");
    warm.compileMs.push(timing.compileMs);
    warm.instantiateMs.push(timing.instantiateMs);
    warm.runMs.push(timing.runMs);
    warm.transferMs.push(timing.transferMs);
    warm.totalMs.push(timing.totalMs);
    warm.wallMs.push(timing.wallMs);
  }

  const cancel = { cancelToIdleMs: [], recoveryRunMs: [] };
  const responsiveness = { editorUpdateMs: [] };
  for (let i = 0; i < REPS_CANCEL; i += 1) {
    await page.locator("#import-project-file").setInputFiles({ name: "loop.ostrin.project.json", mimeType: "application/json", buffer: Buffer.from(LOOP) });
    await expect(page.locator("#file-explorer")).toContainText("1 file");
    await page.locator("#run").click();
    await expect(page.locator("#cancel-run")).toBeEnabled({ timeout: 30_000 });
    const edit = await page.evaluate(() => {
      const source = document.getElementById("source");
      const start = performance.now();
      source.value = "fn main() -> Void {\n    print(\"typing\")\n}\n";
      source.dispatchEvent(new Event("input"));
      return performance.now() - start;
    });
    responsiveness.editorUpdateMs.push(edit);
    const started = Date.now();
    await page.locator("#cancel-run").click();
    await expect(page.locator("#status")).toHaveText("cancelled", { timeout: 15_000 });
    cancel.cancelToIdleMs.push(Date.now() - started);
    const recovery = await runSweep(page);
    cancel.recoveryRunMs.push(recovery.wallMs);
  }
  await context.close();

  const result = {
    schema: "ostrin.benchmark/v1",
    subject: "Workbench Delta: Worker execution, cold/warm start, cancellation, editor responsiveness",
    methodology: {
      cold: `${REPS_COLD} runs, each in a new browser context (no HTTP cache); each loads the page, then runs the parameter sweep once in a new Worker`,
      warm: `${REPS_WARM} runs in one page after one warm-up run; the Worker is reused`,
      cancel: `${REPS_CANCEL} cancellations of an endless program; each followed by a sweep run`,
      responsiveness: "time of a synchronous editor update dispatched while a program runs",
      notes: "compileMs is the Worker's own download and compile of ostrinc.wasm (0 when cached). The ping message does not compile and is not reported as preparation time. Times are wall-clock in the browser; values are not compared with other machines.",
    },
    environment: environment(version),
    cold: Object.fromEntries(Object.entries(cold).map(([k, v]) => [k, stats(v)])),
    warm: Object.fromEntries(Object.entries(warm).map(([k, v]) => [k, stats(v)])),
    cancel: Object.fromEntries(Object.entries(cancel).map(([k, v]) => [k, stats(v)])),
    responsiveness: Object.fromEntries(Object.entries(responsiveness).map(([k, v]) => [k, stats(v)])),
  };
  mkdirSync(outDir, { recursive: true });
  const file = path.join(outDir, `workbench-delta-${os.platform()}.json`);
  writeFileSync(file, JSON.stringify(result, null, 2) + "\n");
  console.log(`benchmark written: ${file}`);
});
