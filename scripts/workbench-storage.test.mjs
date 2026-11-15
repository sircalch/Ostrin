import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { runRecord } from "../website/workbench-history.js";
import {
  classifyStorageError,
  createSerialQueue,
  digestsOf,
  ERROR_CODES,
  mergeHistory,
  MAX_RUN_CONTENT_BYTES,
  prepareRecord,
  RECORD_SCHEMA,
  restoreRecords,
  StorageError,
} from "../website/workbench-history-storage.js";
import { createHistory } from "../website/workbench-history.js";

const TOL = "x: 1e-12 (Brent); residual <= 10*|f'(root)|*1e-12; derivative: |numeric - analytic| <= 1e-5";
const encoder = new TextEncoder();
const digestOf = async (bytes) => createHash("sha256").update(bytes).digest("hex");
const bytesOf = (text) => encoder.encode(text);
const results = (cs) => ({
  schema: "ostrin.results/v1",
  experiment: "cubic-parameter-sweep",
  family: "x^3 - 2x - c",
  bracket: [1, 3],
  method: "std.numeric.brent",
  tolerances: TOL,
  rows: cs.map((c) => ({ c, status: "converged", root: c + 0.5, residual: 0, residual_bound: 1e-11, analytic_derivative: 1, numeric_derivative: 1, passed: true, message: "" })),
});

// A complete run whose content is real: the source, stdout, stderr and figure are the bytes the digests stand for.
async function completedRun(id, { cs = [1, 3], figure = "<svg>one</svg>" } = {}) {
  const source = "fn main() -> Void {}\n";
  const stdout = "RESULTS {}\n";
  const stderr = "";
  const sourceBytes = bytesOf(source);
  const stdoutBytes = bytesOf(stdout);
  const figureBytes = bytesOf(figure);
  const entry = runRecord({
    id,
    projectId: "sweep",
    projectName: "sweep",
    entry: "main.ostrin",
    command: "--run main.ostrin",
    status: "completed",
    exitCode: 0,
    compilerIdentity: "ostrinc 0.2.0",
    startedAt: "2026-11-10T10:00:00.000Z",
    durationMs: 12,
    parameters: { c: cs },
    results: results(cs),
    digests: {
      sources: { "main.ostrin": await digestOf(sourceBytes) },
      stdout: await digestOf(stdoutBytes),
      stderr: null,
      figure: await digestOf(figureBytes),
      manifest: null,
    },
  });
  const contents = [{ bytes: sourceBytes }, { bytes: stdoutBytes }, { bytes: figureBytes }];
  return { entry, contents, figure };
}

function cancelledRun(id) {
  return runRecord({
    id,
    projectId: "loop",
    projectName: "loop",
    entry: "main.ostrin",
    command: "--run main.ostrin",
    status: "cancelled",
    startedAt: "2026-11-10T11:00:00.000Z",
    digests: { sources: { "main.ostrin": "e".repeat(64) }, stdout: null, stderr: null, figure: null, manifest: null },
  });
}

// Turns a prepared record into the raw rows an IndexedDB read would return.
function asRaw(prepared) {
  return {
    records: [{ key: prepared.key, value: structuredClone(prepared.record) }],
    contents: prepared.contents.map(({ key, value }) => ({ key, value: { bytes: value.bytes.slice(), size: value.size } })),
  };
}

test("preparing a run keeps every referenced content once, with its original bytes, and verifies each one", async () => {
  const { entry, contents } = await completedRun("run-1");
  const prepared = await prepareRecord(entry, [...contents, contents[0]], digestOf);
  assert.equal(prepared.key, "run-1");
  assert.equal(prepared.record.schema, RECORD_SCHEMA);
  assert.equal(prepared.contents.length, 3, "a repeated content is stored once");
  assert.deepEqual(prepared.record.missing, []);
  assert.equal(prepared.verification.sources["main.ostrin"], "verified");
  assert.equal(prepared.verification.stdout, "verified");
  assert.equal(prepared.verification.figure, "verified");
  assert.equal(prepared.verification.stderr, null, "no stderr digest was registered");
});

test("a digest without its bytes is recorded as missing, never invented", async () => {
  const { entry, contents } = await completedRun("run-2");
  const prepared = await prepareRecord(entry, contents.slice(0, 2), digestOf);
  assert.deepEqual(prepared.record.missing, ["figure"]);
  assert.equal(prepared.verification.figure, "missing");
  assert.equal(prepared.contents.length, 2);
});

test("content that the run does not reference is refused before anything is written", async () => {
  const { entry, contents } = await completedRun("run-3");
  await assert.rejects(
    prepareRecord(entry, [...contents, { bytes: bytesOf("not part of this run") }], digestOf),
    (error) => error instanceof StorageError && error.code === ERROR_CODES.invalid,
  );
});

test("content above the per-run limit is refused as too large, before anything is written", async () => {
  const { entry } = await completedRun("run-4");
  const huge = new Uint8Array(MAX_RUN_CONTENT_BYTES + 1);
  const hugeEntry = runRecord({
    ...entry,
    digests: { ...entry.digests, figure: await digestOf(huge) },
  });
  await assert.rejects(
    prepareRecord(hugeEntry, [{ bytes: huge }], digestOf),
    (error) => error.code === ERROR_CODES.tooLarge,
  );
});

test("a stored run comes back with its entry unchanged, its verified figure and its verification", async () => {
  const { entry, contents, figure } = await completedRun("run-5");
  const prepared = await prepareRecord(entry, contents, digestOf);
  const restored = await restoreRecords(asRaw(prepared).records, asRaw(prepared).contents, digestOf);
  assert.equal(restored.quarantined.length, 0);
  assert.equal(restored.entries.length, 1);
  const [{ entry: back, verification }] = restored.entries;
  assert.equal(JSON.stringify(back), JSON.stringify(entry));
  assert.equal(back.evidence, "validated");
  assert.equal(restored.figures[entry.digests.figure], figure);
  assert.equal(verification.figure, "verified");
  assert.equal(verification.sources["main.ostrin"], "verified");
});

test("altered stored bytes are detected by recomputing the digest: the figure is not used", async () => {
  const { entry, contents } = await completedRun("run-6");
  const prepared = await prepareRecord(entry, contents, digestOf);
  const raw = asRaw(prepared);
  const figureRow = raw.contents.find((row) => row.key === entry.digests.figure);
  figureRow.value.bytes = bytesOf("<svg>tampered</svg>");
  const restored = await restoreRecords(raw.records, raw.contents, digestOf);
  assert.equal(restored.entries[0].verification.figure, "altered");
  assert.equal(restored.figures[entry.digests.figure], undefined);
  assert.deepEqual(restored.altered, [entry.digests.figure]);
  assert.equal(restored.entries[0].entry.evidence, "validated", "the run itself is still recorded");
});

test("a damaged run is quarantined on its own and the others load", async () => {
  const good = await completedRun("run-good");
  const other = await completedRun("run-other", { cs: [5, 7] });
  const preparedGood = await prepareRecord(good.entry, good.contents, digestOf);
  const preparedOther = await prepareRecord(other.entry, other.contents, digestOf);
  const raw = { records: [...asRaw(preparedGood).records, ...asRaw(preparedOther).records], contents: [...asRaw(preparedGood).contents, ...asRaw(preparedOther).contents] };
  // Flip the evidence of one run: it no longer matches what runRecord would produce.
  raw.records[1].value.entry.evidence = "incomplete";
  // A record whose key is not the run id, and one with an unknown format.
  raw.records.push({ key: "run-keyed-wrong", value: { schema: RECORD_SCHEMA, entry: good.entry } });
  raw.records.push({ key: "run-unknown", value: { schema: "something-else" } });
  const restored = await restoreRecords(raw.records, raw.contents, digestOf);
  assert.deepEqual(restored.entries.map((e) => e.entry.id), ["run-good"]);
  assert.equal(restored.quarantined.length, 3);
  assert.ok(restored.quarantined.some((q) => q.key === "run-other" && /inconsistent/.test(q.reason)));
  assert.ok(restored.quarantined.some((q) => q.key === "run-keyed-wrong" && /key does not match/.test(q.reason)));
  assert.ok(restored.quarantined.some((q) => q.key === "run-unknown" && /unknown record format/.test(q.reason)));
});

test("a cancelled run restores as incomplete evidence and is never validated", async () => {
  const cancelled = cancelledRun("run-cancelled");
  const prepared = await prepareRecord(cancelled, [], digestOf);
  const restored = await restoreRecords(asRaw(prepared).records, asRaw(prepared).contents, digestOf);
  const [{ entry }] = restored.entries;
  assert.equal(entry.status, "cancelled");
  assert.equal(entry.evidence, "incomplete");
  assert.equal(entry.results, null);
  assert.equal(restored.entries[0].verification.sources["main.ostrin"], "missing", "the source bytes were not stored, so the digest is only registered");
});

test("merging restored runs keeps runs already on the page and never replaces them", async () => {
  const one = await completedRun("run-m1");
  const two = await completedRun("run-m2", { cs: [5, 7] });
  const preparedOne = await prepareRecord(one.entry, one.contents, digestOf);
  const preparedTwo = await prepareRecord(two.entry, two.contents, digestOf);
  const restored = await restoreRecords(
    [...asRaw(preparedOne).records, ...asRaw(preparedTwo).records],
    [...asRaw(preparedOne).contents, ...asRaw(preparedTwo).contents],
    digestOf,
  );
  const base = { ...createHistory(), entries: [one.entry], figures: { [one.entry.digests.figure]: one.figure } };
  const merged = mergeHistory(base, restored);
  assert.deepEqual(merged.history.entries.map((e) => e.id), ["run-m1", "run-m2"]);
  assert.equal(merged.history.entries[0], one.entry, "the run already on the page is the same object");
  assert.deepEqual(Object.keys(merged.verification), ["run-m2"]);
});

test("errors are classified by name, so the page can say what happened", () => {
  assert.equal(classifyStorageError({ name: "QuotaExceededError", message: "full" }).code, ERROR_CODES.quota);
  assert.equal(classifyStorageError({ name: "ConstraintError" }).code, ERROR_CODES.duplicate);
  assert.equal(classifyStorageError({ name: "SecurityError" }).code, ERROR_CODES.denied);
  assert.equal(classifyStorageError({ name: "AbortError", message: "aborted" }).code, ERROR_CODES.transaction);
  const original = new StorageError(ERROR_CODES.invalid, "bad");
  assert.equal(classifyStorageError(original), original);
});

test("serialised tasks run one at a time, in order, and a failure does not block the next", async () => {
  const queue = createSerialQueue();
  const order = [];
  const slow = (label, ms, fail = false) => queue(async () => {
    order.push("start " + label);
    await new Promise((resolve) => setTimeout(resolve, ms));
    order.push("end " + label);
    if (fail) throw new Error(label + " failed");
    return label;
  });
  const results2 = await Promise.allSettled([slow("a", 20), slow("b", 1, true), slow("c", 1)]);
  assert.deepEqual(order, ["start a", "end a", "start b", "end b", "start c", "end c"]);
  assert.equal(results2[0].value, "a");
  assert.equal(results2[1].status, "rejected");
  assert.equal(results2[2].value, "c");
});

test("the digests a run refers to are listed in a fixed order, one per stored content", async () => {
  const { entry } = await completedRun("run-d");
  assert.deepEqual(digestsOf(entry).map((r) => r.kind), ["source main.ostrin", "stdout", "figure"]);
});
