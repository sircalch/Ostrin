// Scientific Lab, hero example, compiler pipeline and Cookbook.
//
// Everything shown here comes from website/lab-data.js, which scripts/lab-data.mjs generates from
// Ostrin programs under examples/ and the output of running them with ostrinc.wasm. Run and the
// parameter controls recompute the result with the same compiler in this page. JavaScript only
// substitutes parameter values into the source, draws charts from the numbers Ostrin prints and
// displays the SVG that Ostrin produces; it never computes a scientific result itself.
import { loadCompiler, runOstrinc } from "./ostrin-runtime.js";

const LAB = globalThis.OSTRIN_LAB;
const SVG_NS = "http://www.w3.org/2000/svg";

function el(tag, attributes = {}, children = []) {
  const node = document.createElement(tag);
  for (const [name, value] of Object.entries(attributes)) {
    if (value === undefined || value === null || value === false) continue;
    if (name === "text") node.textContent = value;
    else if (name === "className") node.className = value;
    else node.setAttribute(name, value === true ? "" : value);
  }
  for (const child of [].concat(children)) if (child) node.append(child);
  return node;
}

function svgEl(tag, attributes = {}, text) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [name, value] of Object.entries(attributes)) node.setAttribute(name, value);
  if (text !== undefined) node.textContent = text;
  return node;
}

// ---- parameters -----------------------------------------------------------------------------

function paramPattern(name) {
  return new RegExp(`^(\\s*)${name} = (-?[0-9.]+)$`, "m");
}

function initialValue(source, param) {
  return source.match(paramPattern(param.name))?.[2] ?? String(param.min);
}

// Keeps the literal's type: a Float default stays a Float literal (1 -> 1.0).
function literal(value, original) {
  const text = String(Number(value));
  return original.includes(".") && !text.includes(".") ? `${text}.0` : text;
}

function applyParams(source, params, values) {
  let text = source;
  for (const param of params) {
    const original = initialValue(source, param);
    text = text.replace(paramPattern(param.name), (_, indent) => `${indent}${param.name} = ${literal(values[param.name], original)}`);
  }
  return text;
}

// ---- output rendering -------------------------------------------------------------------------

function splitSvg(lines) {
  const start = lines.findIndex((line) => line.startsWith("<svg"));
  const end = lines.findIndex((line, index) => index >= start && line.startsWith("</svg>"));
  if (start < 0 || end < 0) return { svg: null, rest: lines };
  return { svg: lines.slice(start, end + 1).join("\n"), rest: [...lines.slice(0, start), ...lines.slice(end + 1)] };
}

function chartPoints(lines, chart) {
  const points = [];
  const rest = [];
  for (const line of lines) {
    const parts = line.split(" ");
    if (parts[0] === chart.prefix && parts.length === chart.series.length + 2 && parts.slice(1).every((part) => Number.isFinite(Number(part)))) {
      points.push(parts.slice(1).map(Number));
    } else {
      rest.push(line);
    }
  }
  return { points, rest };
}

function formatTick(value) {
  return Math.abs(value) >= 1000 ? String(Math.round(value)) : String(Math.round(value * 1000) / 1000);
}

function drawChart(points, chart, kind) {
  const width = 560;
  const height = 250;
  const pad = { left: 52, right: 16, top: 16, bottom: 36 };
  const xs = points.map((point) => point[0]);
  const ys = points.flatMap((point) => point.slice(1));
  const [xMin, xMax] = [Math.min(...xs), Math.max(...xs)];
  let [yMin, yMax] = [Math.min(...ys), Math.max(...ys)];
  if (kind === "bars") yMin = Math.min(0, yMin);
  if (yMin === yMax) { yMin -= 1; yMax += 1; }
  const x = (value) => pad.left + (xMax === xMin ? 0.5 : (value - xMin) / (xMax - xMin)) * (width - pad.left - pad.right);
  const y = (value) => height - pad.bottom - (value - yMin) / (yMax - yMin) * (height - pad.top - pad.bottom);

  const svg = svgEl("svg", { viewBox: `0 0 ${width} ${height}`, class: "sl-chart", role: "img", "aria-label": `${chart.series.join(" and ")} by ${chart.x}, from the Ostrin output` });
  svg.append(svgEl("line", { x1: pad.left, y1: height - pad.bottom, x2: width - pad.right, y2: height - pad.bottom, class: "axis" }));
  svg.append(svgEl("line", { x1: pad.left, y1: pad.top, x2: pad.left, y2: height - pad.bottom, class: "axis" }));
  if (yMin < 0 && yMax > 0) svg.append(svgEl("line", { x1: pad.left, y1: y(0), x2: width - pad.right, y2: y(0), class: "zero" }));
  svg.append(svgEl("text", { x: pad.left - 8, y: y(yMax) + 4, class: "tick end" }, formatTick(yMax)));
  svg.append(svgEl("text", { x: pad.left - 8, y: y(yMin) + 4, class: "tick end" }, formatTick(yMin)));
  svg.append(svgEl("text", { x: pad.left, y: height - pad.bottom + 16, class: "tick" }, formatTick(xMin)));
  svg.append(svgEl("text", { x: width - pad.right, y: height - pad.bottom + 16, class: "tick end" }, formatTick(xMax)));
  svg.append(svgEl("text", { x: (pad.left + width - pad.right) / 2, y: height - 6, class: "tick middle" }, chart.x));

  if (kind === "bars") {
    const slot = (width - pad.left - pad.right) / points.length;
    points.forEach(([, count], index) => {
      const top = y(count);
      svg.append(svgEl("rect", { x: pad.left + index * slot + slot * 0.12, y: top, width: slot * 0.76, height: y(yMin) - top, class: "bar" }));
    });
  } else {
    chart.series.forEach((_, series) => {
      const line = points.map((point) => `${x(point[0]).toFixed(1)},${y(point[series + 1]).toFixed(1)}`).join(" ");
      svg.append(svgEl("polyline", { points: line, class: `series series-${series}` }));
      for (const point of points) svg.append(svgEl("circle", { cx: x(point[0]).toFixed(1), cy: y(point[series + 1]).toFixed(1), r: 2.6, class: `dot series-${series}` }));
    });
  }

  const legend = el("div", { className: "sl-legend" }, chart.series.map((name, index) => el("span", { className: `sl-series-${kind === "bars" ? "bar" : index}`, text: name })));
  return el("figure", { className: "sl-figure" }, [svg, legend]);
}

function renderResult(container, demo, lines, errors) {
  container.replaceChildren();
  let text = lines;
  if (demo.render === "svg") {
    const { svg, rest } = splitSvg(lines);
    text = rest;
    if (svg) {
      // Shown as an image: the SVG text comes from the Ostrin program, and an <img> never runs scripts.
      const image = el("img", { className: "sl-svg", alt: `SVG produced by the Ostrin program ${demo.source}`, src: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}` });
      container.append(el("figure", { className: "sl-figure" }, [image, el("figcaption", { text: "SVG generated by std.viz, written in Ostrin" })]));
    }
  } else if (demo.chart) {
    const { points, rest } = chartPoints(lines, demo.chart);
    text = rest;
    if (points.length > 1) container.append(drawChart(points, demo.chart, demo.render === "bars" ? "bars" : "series"));
  }
  const pre = el("pre", { className: "sl-output-text" });
  pre.textContent = text.join("\n");
  if (errors.length) pre.append(el("span", { className: "err", text: (text.length ? "\n" : "") + errors.join("\n") }));
  if (text.length || errors.length) container.append(pre);
  container.append(el("details", { className: "sl-raw" }, [
    el("summary", { text: `Raw program output (${lines.length} line${lines.length === 1 ? "" : "s"})` }),
    el("pre", { text: lines.join("\n") }),
  ]));
}

function codeBlock(text, label) {
  return el("pre", { className: "sl-code", tabindex: "0", "aria-label": label }, [el("code", { text })]);
}

// ---- Scientific Lab ------------------------------------------------------------------------

function buildDemoPanel(demo, index) {
  const values = Object.fromEntries(demo.params.map((param) => [param.name, initialValue(demo.files[demo.main], param)]));
  const code = codeBlock(demo.files[demo.main], `Program file ${demo.source}`);
  const result = el("div", { className: "sl-result", "data-lab-result": demo.id, role: "status", "aria-live": "polite" });
  const provenance = el("p", { className: "sl-provenance", "data-lab-provenance": demo.id });
  const run = el("button", { type: "button", className: "button", "data-lab-run": demo.id, disabled: true, text: "Run" });
  const reset = el("button", { type: "button", className: "button-quiet", text: "Reset" });
  const status = el("span", { className: "sl-status", text: "loading compiler…" });

  const packages = Object.keys(demo.files).filter((name) => name !== demo.main && name.endsWith(".ostrin"));
  const controls = demo.params.map((param) => {
    const id = `lab-${demo.id}-${param.name}`;
    const output = el("output", { for: id, text: values[param.name] });
    const input = el("input", { id, type: "range", min: param.min, max: param.max, step: param.step, value: Number(values[param.name]), "data-lab-param": param.name });
    input.addEventListener("input", () => {
      values[param.name] = input.value;
      output.textContent = literal(input.value, initialValue(demo.files[demo.main], param));
      refreshSource();
      scheduleRun();
    });
    return el("label", { className: "sl-param", for: id }, [el("span", { text: param.label }), input, output]);
  });

  function currentFiles() {
    return { ...demo.files, [demo.main]: applyParams(demo.files[demo.main], demo.params, values) };
  }
  function refreshSource() {
    code.firstChild.textContent = currentFiles()[demo.main];
  }
  function showRecorded() {
    renderResult(result, demo, demo.output, []);
    provenance.textContent = `Recorded output: ${demo.source} run by ostrinc ${LAB.compiler} (${LAB.recordedWith}) when the site was built. Press Run to recompute it in your browser.`;
    provenance.dataset.state = "recorded";
  }

  let timer;
  let running = false;
  let pending = false;
  async function execute() {
    if (running) { pending = true; return; }
    running = true;
    run.disabled = true;
    status.textContent = "running…";
    const started = performance.now();
    try {
      const { code: exit, lines } = await runOstrinc(currentFiles(), demo.args);
      const stdout = lines.filter(([kind]) => kind === "out").map(([, text]) => text);
      const stderr = lines.filter(([kind]) => kind === "err").map(([, text]) => text);
      renderResult(result, demo, stdout, stderr);
      const elapsed = Math.round(performance.now() - started);
      provenance.textContent = `Computed live in your browser by ostrinc.wasm ${LAB.compiler} in ${elapsed} ms${exit === 0 ? "" : ` (exit ${exit})`}.`;
      provenance.dataset.state = exit === 0 ? "live" : "error";
      status.textContent = exit === 0 ? `ok · ${elapsed} ms` : `exit ${exit}`;
    } catch (error) {
      status.textContent = `could not run: ${error.message ?? error}`;
    } finally {
      running = false;
      run.disabled = false;
      if (pending) { pending = false; execute(); }
    }
  }
  function scheduleRun() {
    if (run.dataset.ready !== "true") return;
    clearTimeout(timer);
    timer = setTimeout(execute, 350);
  }

  run.addEventListener("click", execute);
  reset.addEventListener("click", () => {
    for (const param of demo.params) values[param.name] = initialValue(demo.files[demo.main], param);
    controls.forEach((label) => {
      const input = label.querySelector("input");
      input.value = Number(values[input.dataset.labParam]);
      label.querySelector("output").textContent = values[input.dataset.labParam];
    });
    refreshSource();
    showRecorded();
    status.textContent = "";
  });
  showRecorded();

  const panel = el("div", { className: "sl-panel", role: "tabpanel", id: `lab-panel-${demo.id}`, "aria-labelledby": `lab-tab-${demo.id}`, hidden: index !== 0 }, [
    el("div", { className: "sl-panel-head" }, [
      el("div", {}, [el("h3", { text: demo.headline }), el("p", { className: "sl-how", text: demo.how })]),
      el("span", { className: "sl-status-pill", text: "live · real compiler" }),
    ]),
    el("div", { className: "sl-grid" }, [
      el("div", { className: "sl-source" }, [
        el("div", { className: "code-title" }, [el("span", { className: "mono", text: demo.source }), packages.length ? el("span", { className: "dim", text: `+ ${packages.join(", ")}` }) : null]),
        code,
        controls.length ? el("div", { className: "sl-params" }, controls) : null,
        el("div", { className: "sl-actions" }, [run, reset, status]),
      ]),
      el("div", { className: "sl-out" }, [el("div", { className: "code-title" }, [el("span", { text: "result" })]), result, provenance]),
    ]),
    el("div", { className: "sl-links" }, [
      el("a", { className: "text-link", href: demo.sourceUrl, text: "View source ↗" }),
      el("a", { className: "text-link violet", href: demo.docs.href, text: `Documentation: ${demo.docs.label} ↗` }),
      el("a", { className: "text-link", href: `cookbook.html#${demo.id}`, text: "Cookbook recipe →" }),
    ]),
    el("p", { className: "sl-limits" }, [el("strong", { text: "Current limits: " }), demo.limits]),
  ]);
  return { panel, run };
}

function mountLab(root) {
  const tabs = el("div", { className: "sl-tabs", role: "tablist", "aria-label": "Scientific Lab demos" });
  const panels = el("div", { className: "sl-panels" });
  const runs = [];
  const buttons = LAB.demos.map((demo, index) => {
    const button = el("button", { type: "button", role: "tab", id: `lab-tab-${demo.id}`, "aria-controls": `lab-panel-${demo.id}`, "aria-selected": String(index === 0), tabindex: index === 0 ? "0" : "-1", className: "sl-tab", text: demo.title });
    const { panel, run } = buildDemoPanel(demo, index);
    panels.append(panel);
    runs.push(run);
    return button;
  });
  function select(index, focus) {
    buttons.forEach((button, current) => {
      const active = current === index;
      button.setAttribute("aria-selected", String(active));
      button.tabIndex = active ? 0 : -1;
      panels.children[current].hidden = !active;
    });
    if (focus) buttons[index].focus();
  }
  buttons.forEach((button, index) => {
    button.addEventListener("click", () => select(index, false));
    button.addEventListener("keydown", (event) => {
      const moves = { ArrowRight: 1, ArrowLeft: -1 };
      if (event.key in moves) {
        event.preventDefault();
        select((index + moves[event.key] + buttons.length) % buttons.length, true);
      }
    });
  });
  tabs.append(...buttons);
  root.replaceChildren(tabs, panels);
  const fromHash = LAB.demos.findIndex((demo) => location.hash === `#lab-${demo.id}`);
  if (fromHash >= 0) select(fromHash, false);

  loadCompiler().then(
    () => runs.forEach((run) => {
      run.disabled = false;
      run.dataset.ready = "true";
      run.parentElement.querySelector(".sl-status").textContent = "";
    }),
    (error) => runs.forEach((run) => {
      run.parentElement.querySelector(".sl-status").textContent = `compiler unavailable: ${error.message ?? error}`;
    }),
  );
}

// ---- hero, pipeline and cookbook -----------------------------------------------------------

function mountHero(root) {
  root.querySelector("[data-hero-code]")?.replaceChildren(el("code", { text: LAB.hero.code }));
  root.querySelector("[data-hero-output]")?.replaceChildren(document.createTextNode(LAB.hero.output.join("\n")));
  const source = root.querySelector("[data-hero-source]");
  if (source) { source.href = LAB.hero.sourceUrl; source.textContent = LAB.hero.source; }
}

function mountPipeline(root) {
  const { pipeline } = LAB;
  const stages = [
    ["Source", pipeline.code, "What you write."],
    ["Typed HIR", pipeline.hir, "Every node carries a checked type (ostrinc --hir)."],
    ["IR", pipeline.ir, "Basic blocks and explicit temporaries (ostrinc --ir)."],
    ["C", pipeline.c, "Emitted from the IR and compiled by gcc/clang (ostrinc --emit-c)."],
  ];
  root.replaceChildren(...stages.map(([title, text, note], index) => el("article", { className: "pipeline-stage" }, [
    el("span", { className: "feature-number", text: `0${index + 1} / ${title.toUpperCase()}` }),
    el("p", { className: "muted", text: note }),
    codeBlock(text, `${title} for ${pipeline.source}`),
  ])));
  const source = document.querySelector("[data-pipeline-source]");
  if (source) { source.href = pipeline.sourceUrl; source.textContent = pipeline.source; }
  const output = document.querySelector("[data-pipeline-output]");
  if (output) output.textContent = pipeline.output;
}

function mountCookbook(root) {
  root.replaceChildren(...LAB.demos.map((demo) => {
    const result = el("div", { className: "sl-result" });
    renderResult(result, demo, demo.output, []);
    return el("article", { className: "recipe", id: demo.id }, [
      el("p", { className: "eyebrow", text: demo.title }),
      el("h2", { text: demo.headline }),
      el("p", { className: "lede", text: demo.how }),
      el("div", { className: "sl-grid" }, [
        el("div", { className: "sl-source" }, [el("div", { className: "code-title" }, [el("span", { className: "mono", text: demo.source })]), codeBlock(demo.files[demo.main], `Program file ${demo.source}`)]),
        el("div", { className: "sl-out" }, [el("div", { className: "code-title" }, [el("span", { text: "recorded output" })]), result,
          el("p", { className: "sl-provenance", text: `Recorded by ostrinc ${LAB.compiler} (${LAB.recordedWith}) from ${demo.source}.` })]),
      ]),
      el("div", { className: "sl-links" }, [
        el("a", { className: "text-link violet", href: `index.html#lab-${demo.id}`, text: "Run and change it in the Lab →" }),
        el("a", { className: "text-link", href: demo.sourceUrl, text: "View source ↗" }),
        el("a", { className: "text-link", href: demo.docs.href, text: `${demo.docs.label} ↗` }),
      ]),
      el("p", { className: "sl-limits" }, [el("strong", { text: "Current limits: " }), demo.limits]),
    ]);
  }));
}

if (LAB) {
  document.querySelectorAll("[data-lab]").forEach(mountLab);
  document.querySelectorAll("[data-hero]").forEach(mountHero);
  document.querySelectorAll("[data-pipeline]").forEach(mountPipeline);
  document.querySelectorAll("[data-cookbook]").forEach(mountCookbook);
}
