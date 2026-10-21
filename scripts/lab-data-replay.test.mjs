import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { captureWasiFileReads, captureWasiRandom, compareReplayFigure, runWasmWithFilesystemEvidence, spawnBoundedChild } from "./lab-data.mjs";
import { experimentFixtures, outputPathFor } from "./experiment-bundle.mjs";

test("WASI child processes terminate when they exceed their timeout", () => {
  assert.throws(
    () => spawnBoundedChild(process.execPath, ["-e", "while (true) {}"], { encoding: "utf8" }, 100),
    /timed out after 100 ms/,
  );
});

test("WASI filesystem evidence hashes bytes actually read from a declared input", async () => {
  const fixture = experimentFixtures.surface;
  const bundle = JSON.parse(readFileSync(outputPathFor(fixture.id), "utf8"));
  const source = bundle.files[bundle.experiment.source];
  const input = bundle.files["data.json"];
  const deterministicRandomSeed = "ab".repeat(32);
  const result = await runWasmWithFilesystemEvidence({
    "main.ostrin": source,
    "data.json": input,
  }, ["--run", "main.ostrin"], undefined, deterministicRandomSeed);

  const inputReads = result.filesystemReads.filter((event) => event.path === "data.json");
  assert.ok(inputReads.length > 0, JSON.stringify(result.filesystemReads));
  assert.ok(inputReads.every((event) => event.openStatus === "opened" && event.bytesRead === Buffer.byteLength(input)));
  const expected = `sha256:${createHash("sha256").update(input, "utf8").digest("hex")}`;
  assert.ok(inputReads.every((event) => event.contentSha256 === expected));
  assert.equal(result.wasiRandomEvidence.schema, "ostrin.wasi-random-evidence/v1");
  assert.equal(result.wasiRandomEvidence.algorithm, "sha256-counter-test-only/v1");
  assert.ok(result.wasiRandomEvidence.requests.length > 0);
  assert.ok(result.wasiRandomEvidence.requests.every((request) => /^sha256:[a-f0-9]{64}$/.test(request.contentSha256)));
});

test("WASI filesystem evidence records a failed open for an omitted input", async () => {
  const fixture = experimentFixtures.surface;
  const bundle = JSON.parse(readFileSync(outputPathFor(fixture.id), "utf8"));
  const result = await runWasmWithFilesystemEvidence({
    "main.ostrin": bundle.files[bundle.experiment.source],
  }, ["--run", "main.ostrin"], undefined, "cd".repeat(32));
  const failed = result.filesystemReads.find((event) => event.path === "data.json");
  assert.ok(failed, JSON.stringify(result.filesystemReads));
  assert.equal(failed.openStatus, "open-failed");
  assert.equal(failed.bytesRead, 0);
  assert.equal(failed.contentSha256, null);
});

test("WASI filesystem evidence marks a nonzero fd_read errno as incomplete", () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const memoryBytes = new Uint8Array(memory.buffer);
  memoryBytes.set(new TextEncoder().encode("data.json"), 8);
  const importObject = {
    wasi_snapshot_preview1: {
      path_open(...args) {
        new DataView(memory.buffer).setInt32(args[8], 7, true);
        return 0;
      },
      fd_read() { return 5; },
      fd_close() { return 0; },
    },
  };
  const capture = captureWasiFileReads(importObject);
  capture.attachInstance({ exports: { memory } });
  assert.equal(importObject.wasi_snapshot_preview1.path_open(3, 0, 8, 9, 0, 2n, 0n, 0, 64), 0);
  assert.equal(importObject.wasi_snapshot_preview1.fd_read(7, 100, 1, 80), 5);
  const [event] = capture.finish();
  assert.equal(event.openStatus, "opened");
  assert.equal(event.readStatus, "fd_read_errno_5");
  assert.equal(event.contentSha256, "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
});

test("WASI random_get uses reproducible bundle-controlled bytes and rejects invalid memory ranges", () => {
  const seed = "12".repeat(32);
  const run = (selectedSeed) => {
    const memory = new WebAssembly.Memory({ initial: 1 });
    const importObject = { wasi_snapshot_preview1: { random_get() { return 99; } } };
    const capture = captureWasiRandom(importObject, selectedSeed);
    capture.attachInstance({ exports: { memory } });
    assert.equal(importObject.wasi_snapshot_preview1.random_get(64, 32), 0);
    return {
      bytes: Array.from(new Uint8Array(memory.buffer, 64, 32)),
      evidence: capture.finish(),
    };
  };

  const first = run(seed);
  const repeated = run(seed);
  const changedSeed = run("34".repeat(32));
  assert.deepEqual(repeated, first);
  assert.notDeepEqual(changedSeed.bytes, first.bytes);
  const expectedBlock = createHash("sha256")
    .update("ostrin.wasi.random.sha256-counter-test-only.v1\0")
    .update(Buffer.from(seed, "hex"))
    .update(Buffer.alloc(8))
    .digest();
  assert.deepEqual(Buffer.from(first.bytes), expectedBlock);
  assert.equal(first.evidence.algorithm, "sha256-counter-test-only/v1");
  assert.equal(first.evidence.requests[0].byteLength, 32);

  const memory = new WebAssembly.Memory({ initial: 1 });
  const importObject = { wasi_snapshot_preview1: { random_get() { return 0; } } };
  const capture = captureWasiRandom(importObject, seed);
  capture.attachInstance({ exports: { memory } });
  assert.equal(importObject.wasi_snapshot_preview1.random_get(memory.buffer.byteLength - 2, 8), 28);
  assert.equal(capture.finish().requests[0].status, "invalid-memory-range");
});

function bundle(svg = "<svg/>") {
  return {
    files: { "figure.svg": svg },
    provenance: {
      replay: {
        level: "R0",
        status: "verified",
        backend: "ostrinc.wasm",
        target: "wasm32-wasip1",
        compares: "figure.svg",
      },
    },
  };
}

function seededBundle(svg = "<svg/>") {
  return {
    files: { "figure.svg": svg },
    provenance: {
      replay: {
        level: "R1",
        status: "verified",
        backend: "ostrinc.wasm",
        target: "wasm32-wasip1",
        compares: "figure.svg",
      },
    },
  };
}

function snapshotBundle(svg = "<svg/>") {
  return {
    files: { "figure.svg": svg },
    provenance: {
      replay: {
        level: "R2",
        status: "verified",
        backend: "ostrinc.wasm",
        target: "wasm32-wasip1",
        compares: "figure.svg",
      },
    },
  };
}

test("the replay contract accepts an exact bundled SVG", () => {
  assert.deepEqual(compareReplayFigure("surface", "<svg/>", bundle()), []);
});

test("the replay contract rejects renderer drift", () => {
  const errors = compareReplayFigure("surface", "<svg><path/></svg>", bundle());
  assert.ok(errors.some((error) => error.includes("replayed SVG differs")));
});

test("the replay contract rejects a bundle without verified R0 evidence", () => {
  const errors = compareReplayFigure("provenance", "<svg/>", { files: { "figure.svg": "<svg/>" } });
  assert.ok(errors.some((error) => error.includes("verified R0/R1/R2 replay contract")));
});

test("the replay contract accepts an exact bundled SVG with verified R1 evidence", () => {
  assert.deepEqual(compareReplayFigure("histogram", "<svg/>", seededBundle()), []);
});

test("the replay contract accepts an exact bundled SVG with verified R2 evidence", () => {
  assert.deepEqual(compareReplayFigure("surface", "<svg/>", snapshotBundle()), []);
});
