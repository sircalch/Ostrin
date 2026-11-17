#!/usr/bin/env node
// Reproducibility check for scripts/experiment-bundle.mjs.
//
// For every registered fixture and for R repetitions, the bundle is rendered by a fresh Node process. Each repetition
// writes two copies into two independent directories (A and B). The processes differ on purpose: their working directory,
// time zone and locale are varied between repetitions, so any dependence on those is visible. Every output is compared
// byte for byte with the others and with the committed file under website/assets/experiments.
//
// Result (written to docs/experiment-bundle-determinism.json):
//   - byteIdentical: every output of a fixture has the same SHA-256 across all repetitions and both directories;
//   - committedMatches: the rendered bundle equals the committed file (line endings normalised, as verifyBundleFile does);
//   - for any difference, the JSON paths that differ, classified (see classify()).
// No field is excluded: a difference fails the check. The normalisation is only the CRLF-to-LF conversion that
// verifyBundleFile already applies to the committed file.
//
// Usage: node scripts/experiment-bundle-determinism.mjs [--reps 3] [--out docs/experiment-bundle-determinism.json]

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "..");
const args = process.argv.slice(2);
const option = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);

// ---------------------------------------------------------------- child mode: render one fixture to a file

if (args[0] === "--child") {
  const [, fixtureId, target] = args;
  const mod = await import(pathToFileURL(path.join(repo, "scripts", "experiment-bundle.mjs")).href);
  const text = mod.renderExperimentBundle(mod.buildExperimentBundle(fixtureId, mod.repositoryRoot));
  writeFileSync(target, text, "utf8");
  process.exit(0);
}

// ---------------------------------------------------------------- parent mode

const REPS = Number(option("--reps", "3"));
const outFile = path.join(repo, option("--out", "docs/experiment-bundle-determinism.json"));
const work = mkdtempSync(path.join(os.tmpdir(), "bundle-determinism-"));
const mod = await import(pathToFileURL(path.join(repo, "scripts", "experiment-bundle.mjs")).href);
const fixtureIds = Object.keys(mod.experimentFixtures);

const sha256 = (text) => createHash("sha256").update(text, "utf8").digest("hex");
const normalise = (text) => text.replaceAll("\r\n", "\n");

// The environment varies between repetitions: working directory, time zone, locale.
const ENVIRONMENTS = [
  { TZ: "UTC", LANG: "C", LC_ALL: "C" },
  { TZ: "Europe/Madrid", LANG: "es_ES.UTF-8", LC_ALL: "es_ES.UTF-8" },
  { TZ: "America/Los_Angeles", LANG: "en_US.UTF-8", LC_ALL: "en_US.UTF-8" },
];

function render(fixtureId, dir, rep, slot) {
  const cwd = path.join(work, `cwd-${rep}-${slot}`);
  mkdirSync(cwd, { recursive: true });
  mkdirSync(dir, { recursive: true });
  const target = path.join(dir, `${fixtureId}.json`);
  const env = { ...process.env, ...ENVIRONMENTS[(rep + (slot === "B" ? 1 : 0)) % ENVIRONMENTS.length] };
  const r = spawnSync(process.execPath, [fileURLToPath(import.meta.url), "--child", fixtureId, target], { cwd, env, encoding: "utf8", timeout: 600_000, windowsHide: true });
  if (r.status !== 0) throw new Error(`child ${fixtureId} rep ${rep} ${slot} exited ${r.status}: ${r.stderr.slice(0, 400)}`);
  return { target, text: readFileSync(target, "utf8") };
}

// Classification of a differing JSON path. Only used to explain a difference; nothing is excluded from the comparison.
function classify(pathText) {
  if (/(^|\.)(captured|generated|created|timestamp|time|date|at)$/i.test(pathText)) return "timestamp";
  if (/(hardware|runner|node|image|cpu|endianness|os)/i.test(pathText)) return "environment metadata";
  if (/(id|uuid|hash-id)$/i.test(pathText)) return "generated identifier";
  return "scientific or serialisation content";
}

function diffPaths(a, b, prefix = "", out = []) {
  if (typeof a !== typeof b || Array.isArray(a) !== Array.isArray(b)) { out.push({ path: prefix || "$", a: String(JSON.stringify(a)).slice(0, 80), b: String(JSON.stringify(b)).slice(0, 80) }); return out; }
  if (a && typeof a === "object") {
    const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
    for (const key of [...keys].sort()) diffPaths(a[key], b?.[key], prefix ? `${prefix}.${key}` : key, out);
  } else if (a !== b) {
    out.push({ path: prefix || "$", a: String(a).slice(0, 80), b: String(b).slice(0, 80) });
  }
  return out;
}

const report = {
  schema: "ostrin.experiment-determinism/v0",
  generated: new Date().toISOString(),
  node: process.version,
  platform: `${process.platform} ${os.release()} ${process.arch}`,
  repetitions: REPS,
  directories: ["A", "B"],
  environments: ENVIRONMENTS,
  fixtures: [],
};

let allByteIdentical = true;
let allCommitted = true;
for (const fixtureId of fixtureIds) {
  const outputs = [];
  for (let rep = 0; rep < REPS; rep += 1) {
    for (const slot of ["A", "B"]) {
      const dir = path.join(work, `out-${slot}`);
      outputs.push({ rep, slot, ...render(fixtureId, dir, rep, slot) });
    }
  }
  const hashes = outputs.map((o) => sha256(o.text));
  const byteIdentical = hashes.every((h) => h === hashes[0]);
  const committedPath = mod.outputPathFor(fixtureId, mod.repositoryRoot);
  const committed = existsSync(committedPath) ? normalise(readFileSync(committedPath, "utf8")) : null;
  const committedMatches = committed !== null && normalise(outputs[0].text) === committed;
  const differences = byteIdentical ? [] : outputs.slice(1).flatMap((o) => diffPaths(JSON.parse(outputs[0].text), JSON.parse(o.text)).map((d) => ({ ...d, rep: o.rep, slot: o.slot, class: classify(d.path) })));
  allByteIdentical &&= byteIdentical;
  allCommitted &&= committedMatches;
  report.fixtures.push({
    fixture: fixtureId,
    bytes: Buffer.byteLength(outputs[0].text, "utf8"),
    sha256: hashes[0],
    outputs: outputs.length,
    distinctHashes: new Set(hashes).size,
    byteIdentical,
    committedPath: path.relative(repo, committedPath).replaceAll("\\", "/"),
    committedMatches,
    differences,
  });
  console.log(`${fixtureId}: ${byteIdentical ? "byte-identical" : "DIFFERS"} across ${outputs.length} renders; committed ${committedMatches ? "matches" : "differs"}`);
}

// Negative control: the same fixture rendered with the host-capture switch on (as CI does) must differ, otherwise the
// comparison above could not detect environment-dependent fields. The difference is recorded, not treated as a failure.
const controlFixture = "lines";
const controlDir = path.join(work, "control");
mkdirSync(controlDir, { recursive: true });
const plain = render(controlFixture, path.join(controlDir, "plain"), 0, "A").text;
const captureCwd = path.join(work, "cwd-control-capture");
mkdirSync(captureCwd, { recursive: true });
const captureTarget = path.join(controlDir, "capture.json");
const captured = spawnSync(process.execPath, [fileURLToPath(import.meta.url), "--child", controlFixture, captureTarget], {
  cwd: captureCwd, encoding: "utf8", windowsHide: true, timeout: 600_000,
  env: { ...process.env, OSTRIN_CAPTURE_HOST: "1", GITHUB_RUN_ID: "control-run", GITHUB_ACTIONS: "true" },
});
if (captured.status !== 0) throw new Error(`control render failed: ${captured.stderr.slice(0, 300)}`);
const captureText = readFileSync(captureTarget, "utf8");
const controlDifferences = diffPaths(JSON.parse(plain), JSON.parse(captureText)).map((d) => ({ ...d, class: classify(d.path) }));
report.control = {
  fixture: controlFixture,
  switch: "OSTRIN_CAPTURE_HOST=1 (set by the CI workflow; embeds runner identity, hardware and node version)",
  differs: captureText !== plain,
  differingPaths: controlDifferences.length,
  examples: controlDifferences.slice(0, 6),
};
console.log(`control: host capture changes ${controlDifferences.length} JSON path(s) of ${controlFixture}; detected: ${captureText !== plain}`);

report.summary = { fixtures: fixtureIds.length, allByteIdentical, allCommittedMatch: allCommitted, controlDetectsEnvironment: captureText !== plain };
mkdirSync(path.dirname(outFile), { recursive: true });
writeFileSync(outFile, JSON.stringify(report, null, 2) + "\n", "utf8");
console.log(`summary: ${fixtureIds.length} fixtures, byte-identical across all renders: ${allByteIdentical}, committed files match: ${allCommitted}`);
process.exitCode = allByteIdentical && allCommitted ? 0 : 1;
