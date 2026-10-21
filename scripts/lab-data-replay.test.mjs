import test from "node:test";
import assert from "node:assert/strict";
import { compareReplayFigure, spawnBoundedChild } from "./lab-data.mjs";

test("WASI child processes terminate when they exceed their timeout", () => {
  assert.throws(
    () => spawnBoundedChild(process.execPath, ["-e", "while (true) {}"], { encoding: "utf8" }, 100),
    /timed out after 100 ms/,
  );
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
