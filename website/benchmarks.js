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
}
