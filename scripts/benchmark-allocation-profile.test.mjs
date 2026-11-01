import assert from "node:assert/strict";
import test from "node:test";
import { allocationScope, parseAllocationCounters, validateAllocationProfile } from "./benchmark-allocation-profile.mjs";

const raw = "ostrin memory: live_allocations=0 peak_allocations=12 total_allocations=93";
const hash = "a".repeat(64);
const valid = () => ({
  scope: allocationScope, timingIncluded: false, executableSha256: hash,
  samples: [{ ...parseAllocationCounters(raw), outputSha256: hash }],
});

test("native allocation reports preserve managed counts without inferring byte usage", () => {
  assert.deepEqual(parseAllocationCounters(raw + "\r\n"), { live: 0, peak: 12, total: 93, raw });
  assert.deepEqual(validateAllocationProfile(valid(), 1, hash), []);
});

test("missing, duplicated, malformed, leaking and inconsistent counters are rejected", () => {
  for (const value of ["", `${raw}\n${raw}`, raw + " trailing", raw.replace("=0 peak", "=1 peak"),
    raw.replace("=12 total", "=94 total"), raw.replace("=93", "=9007199254740992"),
    raw.replace("=12", "=-12"), raw.replace("=12", "=1.2")]) {
    assert.throws(() => parseAllocationCounters(value));
  }
});

test("profile requires all samples, original output identity and raw counter agreement", () => {
  assert.ok(validateAllocationProfile(undefined, 1, hash).length);
  assert.ok(validateAllocationProfile(valid(), 2, hash).length);
  const report = valid();
  report.samples[0].total = 92;
  assert.ok(validateAllocationProfile(report, 1, hash).some((error) => error.includes("raw report")));
  report.samples[0].total = 93;
  assert.ok(validateAllocationProfile(report, 1, "b".repeat(64)).some((error) => error.includes("output identity")));
  report.timingIncluded = true;
  assert.ok(validateAllocationProfile(report, 1, hash).some((error) => error.includes("timings")));
});
