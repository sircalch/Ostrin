const report = globalThis.OSTRIN_BENCHMARK;

function text(selector, value) {
  const element = document.querySelector(selector);
  if (element) element.textContent = value;
}

if (!report) {
  text("[data-benchmark-state]", "No recorded benchmark is published yet.");
} else {
  text("[data-benchmark-state]", "Recorded benchmark · deterministic output verified");
  text("[data-benchmark-commit]", report.commit.slice(0, 12));
  text("[data-benchmark-compiler]", report.compiler);
  text("[data-benchmark-environment]", `${report.platform} / ${report.arch} · ${report.node}`);
  text("[data-benchmark-samples]", `${report.iterations} samples · ${report.warmups} warmup`);

  const body = document.querySelector("[data-benchmark-rows]");
  for (const workload of report.workloads) {
    const row = document.createElement("tr");
    const name = document.createElement("th");
    name.scope = "row";
    name.textContent = workload.workload.replace(/^examples\//, "").replace(/\.ostrin$/, "");
    const interpreter = document.createElement("td");
    interpreter.textContent = `${workload.interpreterMedianMs.toFixed(3)} ms`;
    const native = document.createElement("td");
    native.textContent = `${workload.nativeMedianMs.toFixed(3)} ms`;
    const ratio = document.createElement("td");
    ratio.textContent = `${workload.interpreterToNativeMedianRatio.toFixed(3)}×`;
    const compile = document.createElement("td");
    compile.textContent = `${workload.nativeCompileMs.toFixed(3)} ms`;
    for (const cell of [name, interpreter, native, ratio, compile]) row.append(cell);
    body?.append(row);
  }

  const pairChart = document.querySelector("[data-benchmark-pair-chart]");
  const pairSummary = document.querySelector("[data-benchmark-pair-summary]");
  const pairDefinitions = [
    ["Float array", "examples/benchmark_float_array.ostrin"],
    ["Quantity array", "examples/benchmark_quantity_array.ostrin"],
  ];
  const pair = pairDefinitions.map(([label, workload]) => ({
    label,
    result: report.workloads.find((candidate) => candidate.workload === workload),
  }));
  if (pairChart && pair.every((entry) => entry.result)) {
    const maximum = Math.max(...pair.flatMap((entry) => [
      entry.result.nativeMedianMs,
      entry.result.interpreterMedianMs,
    ]));
    for (const entry of pair) {
      const row = document.createElement("div");
      row.className = "benchmark-bar-row";
      const label = document.createElement("strong");
      label.textContent = entry.label;
      const tracks = document.createElement("div");
      tracks.className = "benchmark-bar-tracks";
      for (const [kind, value] of [["native", entry.result.nativeMedianMs], ["interpreter", entry.result.interpreterMedianMs]]) {
        const track = document.createElement("div");
        track.className = "benchmark-bar-track";
        track.setAttribute("aria-label", `${entry.label} ${kind}: ${value.toFixed(3)} ms`);
        const bar = document.createElement("span");
        bar.className = `benchmark-bar benchmark-bar-${kind}`;
        bar.style.width = `${(value / maximum) * 100}%`;
        bar.textContent = `${value.toFixed(3)} ms`;
        track.append(bar);
        tracks.append(track);
      }
      row.append(label, tracks);
      pairChart.append(row);
    }
    const floatResult = pair[0].result;
    const quantityResult = pair[1].result;
    const nativeDelta = ((quantityResult.nativeMedianMs / floatResult.nativeMedianMs) - 1) * 100;
    const direction = nativeDelta <= 0 ? "más rápido" : "más lento";
    const parity = report.methodology?.quantityFloatPair;
    const paritySummary = parity
      ? ` Los valores numéricos coincidieron en ${parity.comparedValues} resultados (diferencia máxima ${parity.maximumAbsoluteDifference}; tolerancias ${parity.absoluteTolerance} absoluta y ${parity.relativeTolerance} relativa).`
      : " No se registró la comparación numérica cruzada para este par.";
    pairSummary.textContent = `En esta ejecución, Quantity fue ${Math.abs(nativeDelta).toFixed(1)}% ${direction} que Float en native (${quantityResult.nativeMedianMs.toFixed(3)} frente a ${floatResult.nativeMedianMs.toFixed(3)} ms).${paritySummary} Los tiempos pueden variar con el entorno.`;
  } else if (pairChart) {
    pairChart.textContent = "El par Float/Quantity todavía no está disponible en este snapshot.";
  }
}
