import assert from "node:assert/strict";
import test from "node:test";
import { instrumentationPlan, parsePhaseReport, pendulumPhases, profilerSource } from "./benchmark-native-phases.mjs";

const valid = () => ({ schema: 1, unknownCalls: 0, error: 0, depth: 0, phases: [
  { name: "process_entry", calls: 1, inclusiveNs: 100, exclusiveNs: 10 },
  { name: "application", calls: 1, inclusiveNs: 90, exclusiveNs: 20 },
  { name: "input_and_analysis", calls: 1, inclusiveNs: 50, exclusiveNs: 15 },
  { name: "analysis", calls: 1, inclusiveNs: 35, exclusiveNs: 35 },
  { name: "render", calls: 1, inclusiveNs: 15, exclusiveNs: 15 },
  { name: "text_report", calls: 1, inclusiveNs: 5, exclusiveNs: 5 },
] });
const line = (value) => `ostrin phases: ${JSON.stringify(value)}\n`;

test("GCC plan instruments all declared boundaries and excludes unrelated definitions", () => {
  const c = pendulumPhases.map((phase) => `void ${phase.function}(void) {}`).join("\n") + "\nstatic inline double helper(double x) { return x; }\n";
  const plan = instrumentationPlan(c);
  assert.equal(plan.definitions, 7);
  assert.equal(plan.flags[1], "-finstrument-functions-exclude-function-list=helper");
  assert.throws(() => instrumentationPlan(c.replace("void ostrin_fn_render(void) {}", "")), /missing/);
  assert.throws(() => instrumentationPlan(c + "\nint analyze(void) { return 1; }"), /also matches/);
  const generated = profilerSource("count=@PHASE_COUNT@; entries=@PHASE_ENTRIES@");
  assert.ok(generated.includes("count=6") && generated.includes("&ostrin_fn_analyze"));
  assert.ok(!generated.includes("@PHASE_"));
});

test("phase receipt partitions entry time and preserves input/analysis nesting", () => {
  assert.deepEqual(parsePhaseReport(line(valid())).phases, valid().phases);
});

test("missing, duplicated, incomplete and inconsistent phase evidence is rejected", () => {
  assert.throws(() => parsePhaseReport(""));
  assert.throws(() => parsePhaseReport(line(valid()) + line(valid())));
  for (const mutate of [
    (r) => { r.unknownCalls = 1; }, (r) => { r.error = 1; }, (r) => { r.depth = 1; },
    (r) => { r.phases.pop(); }, (r) => { r.phases[2].calls = 2; },
    (r) => { r.phases[2].exclusiveNs = 51; }, (r) => { r.phases[2].exclusiveNs = -1; },
    (r) => { r.phases[2].inclusiveNs = 2 ** 54; },
    (r) => { r.phases[0].inclusiveNs += 1; },
    (r) => { r.phases[2].inclusiveNs += 1; },
  ]) {
    const report = valid(); mutate(report);
    assert.throws(() => parsePhaseReport(line(report)));
  }
});
