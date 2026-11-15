import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { runRecord } from "../website/workbench-history.js";
import {
  buildPackage,
  canonicalJson,
  COVERAGE,
  decodeBase64,
  encodeBase64,
  LIMITS,
  packageFileName,
  PackageError,
  PACKAGE_SCHEMA,
  referencesOf,
  sealPackage,
  verifyPackage,
} from "../website/workbench-evidence.js";
import { planImport, restoreRecords } from "../website/workbench-history-storage.js";

const TOL = "x: 1e-12 (Brent); residual <= 10*|f'(root)|*1e-12; derivative: |numeric - analytic| <= 1e-5";
const encoder = new TextEncoder();
const digestOf = async (bytes) => createHash("sha256").update(bytes).digest("hex");
const results = (cs) => ({
  schema: "ostrin.results/v1",
  experiment: "cubic-parameter-sweep",
  family: "x^3 - 2x - c",
  bracket: [1, 3],
  method: "std.numeric.brent",
  tolerances: TOL,
  rows: cs.map((c) => ({ c, status: "converged", root: c + 0.5, residual: 0, residual_bound: 1e-11, analytic_derivative: 1, numeric_derivative: 1, passed: true, message: "" })),
});
const CREATED = "2026-11-10T12:00:00Z";
const PRODUCER = { name: "ostrin playground" };

// A validated run with its real bytes, and a cancelled run with only its source.
async function fixtures() {
  const source = encoder.encode("fn main() -> Void {}\n");
  const stdout = encoder.encode("RESULTS {}\nok\n");
  const figure = encoder.encode("<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>");
  const complete = runRecord({
    id: "run-complete",
    projectId: "sweep",
    projectName: "sweep",
    entry: "main.ostrin",
    command: "--run main.ostrin",
    status: "completed",
    exitCode: 0,
    compilerIdentity: "ostrinc 0.2.0",
    startedAt: "2026-11-10T10:00:00.000Z",
    durationMs: 12,
    parameters: { c: [1, 3] },
    results: results([1, 3]),
    digests: {
      sources: { "main.ostrin": await digestOf(source) },
      stdout: await digestOf(stdout),
      stderr: null,
      figure: await digestOf(figure),
      manifest: null,
    },
  });
  const cancelledSource = encoder.encode("while true {}\n");
  const cancelled = runRecord({
    id: "run-cancelled",
    projectId: "loop",
    projectName: "loop",
    entry: "main.ostrin",
    command: "--run main.ostrin",
    status: "cancelled",
    startedAt: "2026-11-10T11:00:00.000Z",
    digests: { sources: { "main.ostrin": await digestOf(cancelledSource) }, stdout: null, stderr: null, figure: null, manifest: null },
  });
  const contents = new Map([
    [await digestOf(source), { kind: "source", bytes: source }],
    [await digestOf(stdout), { kind: "stdout", bytes: stdout }],
    [await digestOf(figure), { kind: "figure", bytes: figure }],
    [await digestOf(cancelledSource), { kind: "source", bytes: cancelledSource }],
  ]);
  return { complete, cancelled, contents, source, stdout, figure };
}

async function packageText(runs = null) {
  const f = await fixtures();
  const built = await buildPackage({
    runs: (runs ?? [{ entry: f.complete }, { entry: f.cancelled }]).map((entry) => ({ entry: entry.entry ?? entry })),
    contents: f.contents,
    createdAt: CREATED,
    producer: PRODUCER,
    digestOf,
  });
  return { text: built.text, pkg: built.pkg, f };
}

// Edits the package as a parsed object, then either reseals it (an attacker who recomputes integrity) or not.
async function edited(text, change, { reseal }) {
  const pkg = JSON.parse(text);
  change(pkg);
  const out = reseal ? await sealPackage(pkg, digestOf) : pkg;
  return JSON.stringify(out);
}

async function rejects(text, code) {
  await assert.rejects(verifyPackage(text, digestOf), (error) => error instanceof PackageError && error.code === code, `expected ${code}`);
}

test("canonical JSON is the same for any key order and rejects non-finite numbers", () => {
  assert.equal(canonicalJson({ b: 1, a: [true, null, "x"] }), canonicalJson(JSON.parse('{"a":[true,null,"x"],"b":1}')));
  assert.equal(canonicalJson({ b: 1, a: 2 }), '{"a":2,"b":1}');
  assert.throws(() => canonicalJson({ n: Number.NaN }), (e) => e.code === "invalid-value");
});

test("Base64 round-trips exact bytes and rejects malformed text", () => {
  const bytes = Uint8Array.from([0, 1, 2, 254, 255, 10]);
  assert.deepEqual(decodeBase64(encodeBase64(bytes)), bytes);
  for (const bad of ["abc", "ab!d", "====", "Zm9v\n", 42]) {
    assert.throws(() => decodeBase64(bad), (e) => e.code === "bad-encoding", String(bad));
  }
});

test("a sealed package verifies: every byte decodes, every digest and cross-reference holds", async () => {
  const { text, f } = await packageText();
  const verified = await verifyPackage(text, digestOf);
  assert.equal(verified.runs.length, 2);
  assert.equal(verified.contents.length, 4);
  const byDigest = Object.fromEntries(verified.contents.map((c) => [c.digest, c.bytes]));
  assert.deepEqual(byDigest[await digestOf(f.stdout)], f.stdout, "stdout bytes come back exactly");
  assert.deepEqual(verified.coverage, { runs: 2, complete: 1, partialContent: 0, incompleteRun: 1, incompleteResults: 0 });
});

test("coverage is stated per run: a cancelled run is incomplete, with no validated results", async () => {
  const { text } = await packageText();
  const verified = await verifyPackage(text, digestOf);
  const cancelled = verified.runs.find((r) => r.entry.id === "run-cancelled");
  assert.equal(cancelled.coverage, COVERAGE.incompleteRun);
  assert.equal(cancelled.entry.evidence, "incomplete");
  assert.equal(cancelled.entry.results, null);
  const complete = verified.runs.find((r) => r.entry.id === "run-complete");
  assert.equal(complete.coverage, COVERAGE.complete);
});

test("a changed byte without resealing is caught by the package integrity digest", async () => {
  const { text } = await packageText();
  const pkg = JSON.parse(text);
  pkg.contents[0].data = encodeBase64(encoder.encode("tampered"));
  pkg.contents[0].size = 8;
  await rejects(JSON.stringify(pkg), "package-digest-mismatch");
});

test("a changed byte with a recomputed integrity digest is caught by the content's SHA-256", async () => {
  const { text } = await packageText();
  const tampered = await edited(text, (pkg) => {
    const item = pkg.contents.find((c) => c.kind === "stdout");
    const bytes = decodeBase64(item.data);
    bytes[0] ^= 0xff;
    item.data = encodeBase64(bytes);
  }, { reseal: true });
  await rejects(tampered, "digest-mismatch");
});

test("resealing does not authenticate metadata: the digest proves consistency, not who wrote it", async () => {
  const { text } = await packageText();
  const renamed = await edited(text, (pkg) => {
    pkg.runs[0].entry.projectName = "another project";
  }, { reseal: true });
  const verified = await verifyPackage(renamed, digestOf);
  assert.equal(verified.runs[0].entry.projectName, "another project", "an attacker who reseals passes; this is the documented limit");
});

test("unknown schema versions, invalid JSON and non-object documents are refused by name", async () => {
  await rejects('{"schema":"ostrin.evidence-package/v9"}', "unknown-schema");
  await rejects("{not json", "invalid-json");
  await rejects("[1,2,3]", "bad-structure");
});

test("oversized, deeply nested and unexpectedly shaped packages are refused before interpretation", async () => {
  await rejects("x".repeat(LIMITS.maxPackageChars + 1), "too-large");
  const deep = "[".repeat(LIMITS.maxDepth + 5) + "]".repeat(LIMITS.maxDepth + 5);
  await rejects(`{"schema":"${PACKAGE_SCHEMA}","x":${deep}}`, "too-deep");
  await rejects(`{"schema":"${PACKAGE_SCHEMA}","__proto__":{"polluted":true}}`, "bad-structure");
});

test("a run whose fields contradict each other is inconsistent, even when resealed", async () => {
  const { text } = await packageText();
  const bad = await edited(text, (pkg) => {
    pkg.runs[1].entry.evidence = "validated";
  }, { reseal: true });
  await rejects(bad, "inconsistent-run");
});

test("declared coverage must match the evidence", async () => {
  const { text } = await packageText();
  const bad = await edited(text, (pkg) => {
    pkg.runs[1].coverage = COVERAGE.complete;
  }, { reseal: true });
  await rejects(bad, "coverage-mismatch");
});

test("a declared-included content with no bytes is missing; present bytes not declared included are inconsistent", async () => {
  const { text } = await packageText();
  const missing = await edited(text, (pkg) => {
    pkg.contents = pkg.contents.filter((c) => c.kind !== "figure");
  }, { reseal: true });
  await rejects(missing, "missing-content");
  const inconsistent = await edited(text, (pkg) => {
    pkg.runs[0].contentStatus.stdout = "digest-only";
  }, { reseal: true });
  await rejects(inconsistent, "inconsistent-content");
});

test("duplicate contents, unreferenced contents, kind mismatches and wrong sizes are refused", async () => {
  const { text } = await packageText();
  const dup = await edited(text, (pkg) => pkg.contents.push({ ...pkg.contents[0] }), { reseal: true });
  await rejects(dup, "duplicate-content");
  const kind = await edited(text, (pkg) => {
    pkg.contents.find((c) => c.kind === "stdout").kind = "stderr";
  }, { reseal: true });
  await rejects(kind, "kind-mismatch");
  const size = await edited(text, (pkg) => {
    pkg.contents[0].size += 1;
  }, { reseal: true });
  await rejects(size, "size-mismatch");
});

test("a content referred to by no run is refused as unreferenced", async () => {
  const { text } = await packageText();
  const bytes = encoder.encode("orphan");
  const orphan = {
    digest: await digestOf(bytes),
    kind: "source",
    encoding: "base64",
    size: bytes.byteLength,
    data: encodeBase64(bytes),
  };
  const bad = await edited(text, (pkg) => pkg.contents.push(orphan), { reseal: true });
  await rejects(bad, "unreferenced-content");
});

test("two runs with the same id in one package are refused", async () => {
  const { text } = await packageText();
  const bad = await edited(text, (pkg) => {
    pkg.runs[1].entry.id = "run-complete";
  }, { reseal: true });
  await rejects(bad, "duplicate-run");
});

test("content that does not match its digest is withheld at export, never exported as verified", async () => {
  const f = await fixtures();
  const forged = new Map(f.contents);
  forged.set(await digestOf(f.stdout), { kind: "stdout", bytes: encoder.encode("altered stdout") });
  const built = await buildPackage({
    runs: [{ entry: f.complete }],
    contents: forged,
    createdAt: CREATED,
    producer: PRODUCER,
    digestOf,
  });
  const stdoutStatus = built.pkg.runs[0].contentStatus.stdout;
  assert.equal(stdoutStatus, "withheld");
  assert.equal(built.pkg.withheld[0].reason, "stored bytes do not match their SHA-256 digest");
  const verified = await verifyPackage(built.text, digestOf);
  assert.equal(verified.contents.some((c) => c.kind === "stdout"), false);
  assert.equal(verified.runs[0].coverage, COVERAGE.partialContent);
});

test("digest-only runs are declared as such and never counted as complete", async () => {
  const f = await fixtures();
  const onlyEntry = await buildPackage({
    runs: [{ entry: f.complete }],
    contents: new Map(),
    createdAt: CREATED,
    producer: PRODUCER,
    digestOf,
  });
  assert.equal(onlyEntry.pkg.runs[0].contentStatus.sources["main.ostrin"], "digest-only");
  const verified = await verifyPackage(onlyEntry.text, digestOf);
  assert.equal(verified.runs[0].coverage, COVERAGE.partialContent);
  assert.equal(verified.contents.length, 0);
});

test("importing a package plans new runs, reuses identical content, and refuses conflicts without writing", async () => {
  const { text, f } = await packageText();
  const verified = await verifyPackage(text, digestOf);
  const empty = { records: new Map(), contents: new Map() };
  const fresh = planImport(verified, empty, CREATED);
  assert.deepEqual(fresh.summary.added, ["run-complete", "run-cancelled"]);
  assert.equal(fresh.summary.contentsAdded, 4);

  const storedComplete = { entry: verified.runs[0].entry };
  const identical = planImport(verified, { records: new Map([["run-complete", storedComplete]]), contents: new Map() }, CREATED);
  assert.deepEqual(identical.summary.identical, ["run-complete"]);
  assert.deepEqual(identical.summary.added, ["run-cancelled"]);

  const altered = { entry: { ...verified.runs[0].entry, command: "another command" } };
  assert.throws(() => planImport(verified, { records: new Map([["run-complete", altered]]), contents: new Map() }, CREATED), (e) => e.code === "conflict");

  const storedSource = await digestOf(f.source);
  const contradictory = { bytes: encoder.encode("different bytes under the same digest") };
  assert.throws(
    () => planImport(verified, { records: new Map(), contents: new Map([[storedSource, contradictory]]) }, CREATED),
    (e) => e.code === "conflict",
  );
  const reused = planImport(verified, { records: new Map(), contents: new Map([[storedSource, { bytes: f.source }]]) }, CREATED);
  assert.equal(reused.summary.contentsReused, 1);
  assert.equal(reused.summary.contentsAdded, 3);
});

test("package file names are safe and bounded", () => {
  const name = packageFileName(["run ../../etc/passwd<script>"], new Date("2026-11-10T12:34:56Z"));
  assert.match(name, /^ostrin-evidence-[A-Za-z0-9_.-]+\.ostrin\.evidence\.json$/);
  assert.ok(!name.includes("/") && !name.includes("<"));
  assert.match(packageFileName(["a", "b"]), /^ostrin-evidence-2-runs-/);
});

test("references list every digest a run carries, in the Run History order", async () => {
  const f = await fixtures();
  assert.deepEqual(referencesOf(f.complete).map((r) => r.kind), ["source", "stdout", "figure"]);
});

test("a package whose result rows are not finite numbers or booleans is refused, even when resealed", async () => {
  const { text } = await packageText();
  const bad = await edited(text, (pkg) => {
    pkg.runs[0].entry.results.rows[0].root = "not a number";
    pkg.runs[0].entry.results.rows[0].passed = "yes";
  }, { reseal: true });
  await rejects(bad, "inconsistent-run");
});

test("two result rows for the same parameter c are refused: a comparison would silently keep one of them", async () => {
  const { text } = await packageText();
  const bad = await edited(text, (pkg) => {
    const rows = pkg.runs[0].entry.results.rows;
    rows.push({ ...rows[0] });
    pkg.runs[0].entry.parameters.c = [...pkg.runs[0].entry.parameters.c, rows[0].c];
  }, { reseal: true });
  await rejects(bad, "inconsistent-run");
});

test("a damaged stored record with the same id is named as such and never overwritten by an import", async () => {
  const { text } = await packageText();
  const verified = await verifyPackage(text, digestOf);
  const damaged = { entry: undefined };
  assert.throws(
    () => planImport(verified, { records: new Map([["run-complete", damaged]]), contents: new Map() }, CREATED),
    (e) => e.code === "conflict" && /damaged record/.test(e.message),
  );
});

test("the origin of a restored run is kept: local runs and imported runs are told apart", async () => {
  const { text } = await packageText();
  const verified = await verifyPackage(text, digestOf);
  const plan = planImport(verified, { records: new Map(), contents: new Map() }, CREATED);
  const imported = plan.newRuns.find((r) => r.id === "run-cancelled").record;
  const local = { schema: "ostrin.history-record/v1", entry: verified.runs[0].entry, missing: [], storedAt: CREATED };
  const restored = await restoreRecords(
    [{ key: "run-complete", value: local }, { key: "run-cancelled", value: imported }],
    [],
    digestOf,
  );
  const origins = Object.fromEntries(restored.entries.map((e) => [e.entry.id, e.origin]));
  assert.deepEqual(origins["run-complete"], { kind: "local" }, "a record without an origin was run in this browser");
  assert.deepEqual(origins["run-cancelled"], { kind: "imported", source: "evidence package", at: CREATED });
});

test("a package whose declared parameters disagree with its result rows is refused, even when resealed", async () => {
  const { text } = await packageText();
  const bad = await edited(text, (pkg) => {
    pkg.runs[0].entry.parameters.c = [1, 3, 5];
  }, { reseal: true });
  await rejects(bad, "inconsistent-run");
});

test("incomplete evidence never carries parameters", async () => {
  const { text } = await packageText();
  const bad = await edited(text, (pkg) => {
    pkg.runs[1].entry.parameters = { c: [1] };
  }, { reseal: true });
  await rejects(bad, "inconsistent-run");
});
