export const pendulumPhases = [
  { name: "process_entry", parent: null, function: "main" },
  { name: "application", parent: "process_entry", function: "ostrin_main" },
  { name: "input_and_analysis", parent: "application", function: "ostrin_fn_load_analysis" },
  { name: "analysis", parent: "input_and_analysis", function: "ostrin_fn_analyze" },
  { name: "render", parent: "application", function: "ostrin_fn_render" },
  { name: "text_report", parent: "application", function: "ostrin_fn_print_report" },
];

export function instrumentationPlan(cSource, phases = pendulumPhases) {
  const definitions = [...cSource.matchAll(/^(?:[A-Za-z_][\w* \t]*\b)([A-Za-z_]\w*)\([^;{}]*\)\s*\{/gm)].map((match) => match[1]);
  const selected = new Set(phases.map((phase) => phase.function));
  for (const name of selected) if (!definitions.includes(name)) throw new Error(`phase function definition missing: ${name}`);
  const excluded = [...new Set(definitions)].filter((name) => !selected.has(name));
  for (const name of excluded) {
    if ([...selected].some((target) => target.includes(name))) throw new Error(`GCC exclusion ${name} also matches a selected phase`);
  }
  return {
    definitions: definitions.length,
    flags: ["-finstrument-functions", `-finstrument-functions-exclude-function-list=${excluded.join(",")}`],
  };
}

export function profilerSource(template, phases = pendulumPhases) {
  return template.replace("@PHASE_COUNT@", String(phases.length)).replace("@PHASE_ENTRIES@", phases.map((phase) =>
    `{ (void*)&${phase.function}, ${JSON.stringify(phase.name)}, 0, 0, 0, 0, 0 }`).join(",\n"));
}

export function parsePhaseReport(stderr, phases = pendulumPhases) {
  const lines = stderr.replaceAll("\r\n", "\n").split("\n").filter((line) => line.startsWith("ostrin phases: "));
  if (lines.length !== 1) throw new Error("expected exactly one phase report");
  const report = JSON.parse(lines[0].slice("ostrin phases: ".length));
  if (report.schema !== 1 || report.unknownCalls !== 0 || report.error !== 0 || report.depth !== 0) {
    throw new Error("incomplete phase stack or unexpected instrumented calls");
  }
  if (!Array.isArray(report.phases) || report.phases.length !== phases.length) throw new Error("phase list differs from the selected functions");
  for (const [index, phase] of report.phases.entries()) {
    if (phase.name !== phases[index].name || phase.calls !== 1) throw new Error(`invalid phase identity at ${index}`);
    for (const [inclusive, exclusive] of [["inclusiveNs", "exclusiveNs"], ["inclusiveCpuNs", "exclusiveCpuNs"]]) {
      if (!Number.isSafeInteger(phase[inclusive]) || !Number.isSafeInteger(phase[exclusive])
        || phase[exclusive] < 0 || phase[inclusive] < phase[exclusive]) throw new Error(`invalid phase counters at ${index}`);
    }
  }
  const byName = Object.fromEntries(report.phases.map((phase) => [phase.name, phase]));
  for (const [inclusive, exclusive] of [["inclusiveNs", "exclusiveNs"], ["inclusiveCpuNs", "exclusiveCpuNs"]]) {
    if (report.phases.reduce((sum, phase) => sum + phase[exclusive], 0) !== report.phases[0][inclusive]) {
      throw new Error("exclusive phase times do not partition process-entry time");
    }
    for (const phase of phases) {
      const measured = byName[phase.name];
      const childTime = phases.filter((candidate) => candidate.parent === phase.name)
        .reduce((sum, child) => sum + byName[child.name][inclusive], 0);
      if (measured[inclusive] - childTime !== measured[exclusive]) throw new Error(`phase nesting is inconsistent for ${phase.name}`);
    }
  }
  return { ...report, raw: lines[0] };
}
