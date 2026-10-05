// Ostrin Viz gallery. Every figure is an Ostrin program under examples/; scripts/lab-data.mjs
// records the SVG it printed (website/assets/viz/*.svg) with ostrinc.wasm. "Run live" runs the
// same program with the compiler in this page and shows the SVG it prints. JavaScript never draws
// a figure itself: images are the SVG text Ostrin produced, shown through <img>.
const LAB = globalThis.OSTRIN_LAB;

// The compiler runtime (and its WASI shim from the CDN) loads on demand, so the recorded
// figures show even when it cannot be reached.
const runtime = () => import("./ostrin-runtime.js");

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

function svgOf(lines) {
  const start = lines.findIndex((line) => line.startsWith("<svg"));
  // `viz.grid` returns one outer SVG containing nested panel SVGs. The first
  // closing tag belongs to the first panel; keep the final tag so the iframe
  // receives the complete composition and can link its table and plot nodes.
  const end = lines.findLastIndex((line, index) => index >= start && line === "</svg>");
  if (start < 0 || end < 0) return { svg: null, printed: lines };
  return { svg: lines.slice(start, end + 1).join("\n"), printed: [...lines.slice(0, start), ...lines.slice(end + 1)] };
}

function provenanceOf(source) {
  if (!source) return null;
  const root = new DOMParser().parseFromString(source, "image/svg+xml").documentElement;
  const node = root.querySelector("ostrin-provenance");
  if (!node) return null;
  return {
    sourceHash: node.getAttribute("source-hash") ?? "",
    dataHash: node.getAttribute("data-hash") ?? "",
    seed: node.getAttribute("seed") ?? "",
    compiler: node.getAttribute("compiler") ?? "",
  };
}

function provenanceText(metadata, prefix = "Provenance") {
  if (!metadata) return `${prefix}: no reproducibility metadata recorded.`;
  const values = [
    metadata.sourceHash ? `source ${metadata.sourceHash}` : "source hash unavailable",
    metadata.dataHash ? `data ${metadata.dataHash}` : "data hash unavailable",
    metadata.seed ? `seed ${metadata.seed}` : "seed unavailable",
    metadata.compiler ? metadata.compiler : "compiler unavailable",
  ];
  return `${prefix}: ${values.join(" · ")}.`;
}

function figureManifest(figure, source = null) {
  const metadata = provenanceOf(source) ?? (figure.provenance && Object.keys(figure.provenance).length
    ? {
        sourceHash: figure.provenance["source-hash"] ?? "",
        dataHash: figure.provenance["data-hash"] ?? "",
        seed: figure.provenance.seed ?? "",
        compiler: figure.provenance.compiler ?? "",
      }
    : null);
  return {
    schema: "ostrin.figure/v0",
    id: figure.id,
    title: figure.title,
    source: figure.source,
    source_url: figure.sourceUrl,
    svg: figure.svg,
    capabilities: [...(figure.capabilities ?? [])],
    recorded_with: LAB.recordedWith,
    selected_state: selectedFigureState(figure),
    provenance: metadata
      ? {
          source_hash: metadata.sourceHash || null,
          data_hash: metadata.dataHash || null,
          seed: metadata.seed || null,
          compiler: metadata.compiler || null,
        }
      : null,
  };
}

function downloadFigureManifest(figure, source = null) {
  const manifest = JSON.stringify(figureManifest(figure, source), null, 2) + "\n";
  downloadBlob(new Blob([manifest], { type: "application/json;charset=utf-8" }), `${fileStem(figure.title)}.ostrin-figure.json`);
}

const SVG_NUMBER = /-?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/g;

function interpolateAnimatedValue(first, second, amount) {
  const a = first.match(SVG_NUMBER) ?? [];
  const b = second.match(SVG_NUMBER) ?? [];
  if (a.length !== b.length) return amount < 0.5 ? first : second;
  let index = 0;
  return first.replace(SVG_NUMBER, () => {
    const value = Number(a[index]) + (Number(b[index]) - Number(a[index])) * amount;
    index += 1;
    return String(Number(value.toFixed(5)));
  });
}

// Turn one instant of Ostrin's SVG animation into a static SVG. Flip-book
// animations select one computed frame; SMIL animations interpolate their
// numeric attributes (including morphing paths) and remove <animate> nodes.
function staticSvgAt(source, fraction) {
  const document_ = new DOMParser().parseFromString(source, "image/svg+xml");
  const root = document_.documentElement;
  const frames = [...root.querySelectorAll(".ostrin-frame")];
  if (frames.length) {
    const chosen = frames[Math.min(frames.length - 1, Math.floor(fraction * frames.length))];
    for (const frame of frames) {
      if (frame !== chosen) frame.remove();
    }
    chosen.style.cssText += ";animation:none!important;opacity:1!important;animation-delay:0ms!important";
  } else {
    for (const animation of [...root.querySelectorAll("animate")]) {
      const target = animation.parentElement;
      const values = (animation.getAttribute("values") ?? "").split(";");
      const attribute = animation.getAttribute("attributeName");
      if (!target || !attribute || !values.length) continue;
      const position = Math.min(values.length - 1, fraction * (values.length - 1));
      const lower = Math.floor(position);
      const upper = Math.min(values.length - 1, lower + 1);
      target.setAttribute(attribute, interpolateAnimatedValue(values[lower], values[upper], position - lower));
      animation.remove();
    }
  }
  return new XMLSerializer().serializeToString(root);
}

function svgDimensions(source) {
  const root = new DOMParser().parseFromString(source, "image/svg+xml").documentElement;
  const viewBox = (root.getAttribute("viewBox") ?? "").trim().split(/[ ,]+/).map(Number);
  const width = Number.parseFloat(root.getAttribute("width") ?? "") || viewBox[2] || 640;
  const height = Number.parseFloat(root.getAttribute("height") ?? "") || viewBox[3] || 400;
  return { width: Math.max(1, Math.round(width)), height: Math.max(1, Math.round(height)) };
}

function imageFromSvg(source) {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("the browser could not rasterize an SVG frame"));
    image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(source)}`;
  });
}

function fileStem(title, fallback = "ostrin-figure") {
  return (title || fallback).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || fallback;
}

const SOURCE_NUMBER = "-?(?:\\d+(?:\\.\\d*)?|\\.\\d+)(?:[eE][+-]?\\d+)?";

function sourceParameterPattern(name) {
  const escaped = String(name).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^(\\s*)${escaped}\\s*=\\s*(${SOURCE_NUMBER})$`, "m");
}

function sourceParameterValue(source, parameter) {
  return source.match(sourceParameterPattern(parameter.name))?.[2] ?? String(parameter.min);
}

function sourceParameterLiteral(value, original) {
  const text = String(Number(value));
  return original.includes(".") && !text.includes(".") ? `${text}.0` : text;
}

function applySourceParameters(source, parameters, values) {
  let text = source;
  for (const parameter of parameters) {
    const original = sourceParameterValue(source, parameter);
    text = text.replace(sourceParameterPattern(parameter.name), (_, indent) => `${indent}${parameter.name} = ${sourceParameterLiteral(values[parameter.name], original)}`);
  }
  return text;
}

// A gallery URL identifies a real Ostrin program and the inputs that produced its current
// frame.  This keeps a figure shareable without uploading source, data or rendered output.
const VIZ_PARAMETER_NAMES = new Set((LAB?.gallery ?? []).flatMap((figure) => (figure.controls ?? []).map((parameter) => parameter.name)));

function readVizState() {
  const url = new URL(window.location.href);
  const hashFigure = url.hash.startsWith("#viz-") ? url.hash.slice("#viz-".length) : "";
  const requested = url.searchParams.get("figure") || hashFigure;
  const figure = LAB.gallery.find((candidate) => candidate.id === requested);
  if (!figure) return null;
  const parameters = {};
  for (const parameter of figure.controls ?? []) {
    const raw = url.searchParams.get(parameter.name);
    if (raw === null || !Number.isFinite(Number(raw))) continue;
    parameters[parameter.name] = String(Math.max(parameter.min, Math.min(parameter.max, Number(raw))));
  }
  const readAngle = (name, min, max) => {
    const raw = Number(url.searchParams.get(name));
    return Number.isFinite(raw) ? Math.max(min, Math.min(max, raw)) : null;
  };
  return { figureId: figure.id, parameters, azimuth: readAngle("azimuth", -180, 180), elevation: readAngle("elevation", -80, 80) };
}

function selectedFigureState(figure) {
  const state = readVizState();
  if (!state || state.figureId !== figure.id) return null;
  const parameters = Object.fromEntries(Object.entries(state.parameters).map(([name, value]) => [name, Number(value)]));
  const selected = { parameters, share_url: window.location.href };
  if (state.azimuth !== null || state.elevation !== null) {
    selected.camera = { azimuth: state.azimuth, elevation: state.elevation };
  }
  return selected;
}

function readWorkflowState() {
  const id = new URL(window.location.href).searchParams.get("workflow");
  return (LAB?.workflows ?? []).find((workflow) => workflow.id === id) ?? null;
}

function readGalleryCapability() {
  const id = new URL(window.location.href).searchParams.get("capability");
  return VIZ_CAPABILITY_DEFINITIONS.some((definition) => definition.id === id) ? id : "all";
}

function writeGalleryCapability(capability, mode = "replace") {
  const url = new URL(window.location.href);
  url.searchParams.delete("capability");
  if (capability !== "all") url.searchParams.set("capability", capability);
  const method = mode === "push" ? "pushState" : "replaceState";
  window.history[method]({ capability }, "", url.href);
}

function writeWorkflowState(workflowId, mode = "replace") {
  const url = new URL(window.location.href);
  url.searchParams.set("workflow", workflowId);
  url.searchParams.delete("figure");
  for (const parameter of VIZ_PARAMETER_NAMES) url.searchParams.delete(parameter);
  url.searchParams.delete("azimuth");
  url.searchParams.delete("elevation");
  url.searchParams.delete("capability");
  url.hash = `workflow-${workflowId}`;
  const method = mode === "push" ? "pushState" : "replaceState";
  window.history[method]({ workflow: workflowId }, "", url.href);
}

function writeVizState(figureId, parameters = {}, azimuth = null, elevation = null, mode = "replace") {
  const url = new URL(window.location.href);
  url.searchParams.set("figure", figureId);
  for (const name of VIZ_PARAMETER_NAMES) url.searchParams.delete(name);
  url.searchParams.delete("azimuth");
  url.searchParams.delete("elevation");
  for (const [name, value] of Object.entries(parameters)) url.searchParams.set(name, String(value));
  if (azimuth !== null && azimuth !== undefined) url.searchParams.set("azimuth", String(azimuth));
  if (elevation !== null && elevation !== undefined) url.searchParams.set("elevation", String(elevation));
  url.hash = `viz-${figureId}`;
  const method = mode === "push" ? "pushState" : "replaceState";
  window.history[method]({ figure: figureId }, "", url.href);
}

function downloadBlob(blob, filename) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function downloadSvg(source, title) {
  downloadBlob(new Blob([source], { type: "image/svg+xml;charset=utf-8" }), `${fileStem(title)}.svg`);
}

function playgroundHref(source) {
  const url = new URL("playground.html", window.location.href);
  url.searchParams.set("code", source);
  return url.href;
}

async function downloadExperimentBundle(url, title) {
  const response = await fetch(url, { cache: "no-store" });
  if (!response.ok) throw new Error(`experiment bundle unavailable (HTTP ${response.status})`);
  const blob = await response.blob();
  downloadBlob(blob, `${fileStem(title)}.ostrin-experiment.json`);
}

async function exportPng(source, title, fraction = 0, scale = 2) {
  const staticSource = staticSvgAt(source, fraction);
  const dimensions = svgDimensions(staticSource);
  const image = await imageFromSvg(staticSource);
  const canvas = document.createElement("canvas");
  canvas.width = dimensions.width * scale;
  canvas.height = dimensions.height * scale;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("PNG export needs a 2D canvas context");
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.drawImage(image, 0, 0, canvas.width, canvas.height);
  const blob = await new Promise((resolve) => canvas.toBlob(resolve, "image/png"));
  if (!blob) throw new Error("the browser could not encode a PNG");
  downloadBlob(blob, `${fileStem(title)}.png`);
  return { width: canvas.width, height: canvas.height };
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function sanitizeSvg(source) {
  const document_ = new DOMParser().parseFromString(source, "image/svg+xml");
  const root = document_.documentElement;
  if (!root || root.localName !== "svg") throw new Error("the figure did not produce a valid SVG");
  root.querySelectorAll("script,foreignObject").forEach((node) => node.remove());
  for (const element of [root, ...root.querySelectorAll("*")]) {
    for (const attribute of [...element.attributes]) {
      const name = attribute.name.toLowerCase();
      const value = attribute.value.trim().toLowerCase();
      if (name.startsWith("on") || ((name === "href" || name === "xlink:href") && value.startsWith("javascript:"))) {
        element.removeAttribute(attribute.name);
      }
    }
  }
  return new XMLSerializer().serializeToString(root);
}

function exportInteractiveHtml(source, title, fraction = 0) {
  const staticSource = sanitizeSvg(staticSvgAt(source, fraction));
  const dimensions = svgDimensions(staticSource);
  const metadata = provenanceOf(staticSource);
  const safeTitle = escapeHtml(title || "Ostrin figure");
  const provenance = escapeHtml(provenanceText(metadata, "Ostrin provenance"));
  const html = [
    "<!doctype html>",
    '<html lang="en">',
    "<head>",
    '<meta charset="utf-8">',
    '<meta name="viewport" content="width=device-width, initial-scale=1">',
    "<title>" + safeTitle + " · Ostrin interactive figure</title>",
    "<style>",
    ":root{color-scheme:dark;--bg:#080a10;--panel:#111522;--line:#293145;--text:#f4f7fb;--muted:#a9b4c7;--accent:#a78bfa}",
    "*{box-sizing:border-box}",
    "html,body{margin:0;min-height:100%;background:var(--bg);color:var(--text);font:14px system-ui,sans-serif}",
    "main{display:grid;gap:14px;width:min(1200px,100% - 32px);margin:24px auto}",
    "h1{margin:0;font-size:1.25rem}",
    ".provenance{margin:0;color:var(--muted);font:12px ui-monospace,SFMono-Regular,Consolas,monospace;overflow-wrap:anywhere}",
    ".toolbar{display:flex;flex-wrap:wrap;align-items:center;gap:8px;padding:10px;border:1px solid var(--line);border-radius:6px;background:var(--panel)}",
    "button{min-height:32px;padding:5px 10px;border:1px solid var(--line);border-radius:4px;background:#171d2d;color:var(--text);cursor:pointer;font:12px ui-monospace,SFMono-Regular,Consolas,monospace}",
    "button:hover,button:focus-visible{border-color:var(--accent);outline:2px solid color-mix(in srgb,var(--accent) 45%,transparent);outline-offset:1px}",
    "button[aria-pressed=false]{opacity:.72}",
    ".zoom{min-width:4rem;color:var(--muted);font:12px ui-monospace,SFMono-Regular,Consolas,monospace;text-align:center}",
    ".status{flex:1 1 260px;color:var(--muted);font:12px ui-monospace,SFMono-Regular,Consolas,monospace}",
    ".viewport{min-height:240px;max-height:calc(100vh - 190px);overflow:auto;padding:18px;border:1px solid var(--line);border-radius:6px;background:#fff;touch-action:none}",
    ".art{width:max-content;transform-origin:top left;will-change:transform}",
    ".art svg{display:block;width:" + dimensions.width + "px;height:" + dimensions.height + "px;max-width:none}",
    ".crosshair-line{stroke:#111827;stroke-width:1;stroke-dasharray:5 4;vector-effect:non-scaling-stroke;pointer-events:none}",
    ".crosshair-marker{fill:#fff;stroke:#111827;stroke-width:1.5;vector-effect:non-scaling-stroke;pointer-events:none}",
    ".crosshair-label{fill:#111827;font:12px ui-monospace,SFMono-Regular,Consolas,monospace;paint-order:stroke;stroke:#fff;stroke-width:4px;stroke-linejoin:round;pointer-events:none}",
    "@media (prefers-reduced-motion:reduce){*,*::before,*::after{scroll-behavior:auto!important;transition:none!important;animation-duration:.001ms!important;animation-iteration-count:1!important}}",
    "</style>",
    "</head>",
    '<body><main data-ostrin-export="interactive">',
    "<h1>" + safeTitle + "</h1>",
    '<p class="provenance" data-ostrin-provenance="true">' + provenance + "</p>",
    '<div class="toolbar" role="toolbar" aria-label="Figure controls">',
    '<button type="button" id="zoom-out" aria-label="Zoom out">−</button>',
    '<output class="zoom" id="zoom" aria-live="polite">100%</output>',
    '<button type="button" id="zoom-in" aria-label="Zoom in">+</button>',
    '<button type="button" id="reset" aria-label="Reset view">Reset view</button>',
    '<button type="button" id="crosshair" aria-pressed="false">Enable crosshair</button>',
    '<span class="status" id="status" aria-live="polite">Drag to pan · scroll to zoom · hover a mark to inspect values.</span>',
    "</div>",
    '<div class="viewport" id="viewport" tabindex="0" aria-label="Interactive figure viewport"><div class="art" id="art">',
    staticSource,
    "</div></div>",
    "<script>",
    "(() => {",
    '  const viewport = document.getElementById("viewport");',
    '  const art = document.getElementById("art");',
    '  const svg = art.querySelector("svg");',
    '  const zoomOutput = document.getElementById("zoom");',
    '  const status = document.getElementById("status");',
    '  const crosshairButton = document.getElementById("crosshair");',
    "  let scale = 1, panX = 0, panY = 0, dragging = null, crosshairEnabled = false;",
    "  const applyTransform = () => {",
    '    art.style.transform = "translate(" + panX + "px," + panY + "px) scale(" + scale + ")";',
    '    zoomOutput.textContent = Math.round(scale * 100) + "%";',
    "  };",
    '  const setZoom = (value) => { scale = Math.max(.5, Math.min(4, value)); applyTransform(); };',
    '  document.getElementById("zoom-out").addEventListener("click", () => setZoom(scale - .25));',
    '  document.getElementById("zoom-in").addEventListener("click", () => setZoom(scale + .25));',
    '  document.getElementById("reset").addEventListener("click", () => { scale = 1; panX = 0; panY = 0; applyTransform(); status.textContent = "View reset · drag to pan · scroll to zoom."; });',
    '  viewport.addEventListener("wheel", (event) => { event.preventDefault(); setZoom(scale * (event.deltaY < 0 ? 1.1 : .9)); }, { passive: false });',
    '  viewport.addEventListener("pointerdown", (event) => { if (event.button !== 0) return; dragging = { x: event.clientX, y: event.clientY, panX, panY }; viewport.setPointerCapture(event.pointerId); });',
    '  viewport.addEventListener("pointermove", (event) => { if (!dragging) return; panX = dragging.panX + event.clientX - dragging.x; panY = dragging.panY + event.clientY - dragging.y; applyTransform(); });',
    '  viewport.addEventListener("pointerup", () => { dragging = null; });',
    '  viewport.addEventListener("pointercancel", () => { dragging = null; });',
    '  const inspectableSelector = "[data-viz-index],.bar,.boxplot,.violin,.hexbin-cell,.contourf-cell,.quiver,.streamplot,.viz-series";',
    '  const namespace = "http://www.w3.org/2000/svg";',
    "  let overlay = null;",
    '  const hideCrosshair = () => { if (overlay) overlay.setAttribute("visibility", "hidden"); };',
    "  if (svg && svg.querySelector(inspectableSelector)) {",
    '    overlay = document.createElementNS(namespace, "g");',
    '    overlay.setAttribute("aria-hidden", "true");',
    '    overlay.setAttribute("class", "ostrin-export-crosshair");',
    '    const makeLine = () => { const line = document.createElementNS(namespace, "line"); line.setAttribute("class", "crosshair-line"); overlay.append(line); return line; };',
    "    const vertical = makeLine(), horizontal = makeLine();",
    '    const marker = document.createElementNS(namespace, "circle"); marker.setAttribute("class", "crosshair-marker"); marker.setAttribute("r", "4"); overlay.append(marker);',
    '    const label = document.createElementNS(namespace, "text"); label.setAttribute("class", "crosshair-label"); overlay.append(label);',
    "    svg.append(overlay);",
    '    const showCrosshair = (event) => { if (!crosshairEnabled) return; const mark = event.target instanceof Element ? event.target.closest(inspectableSelector) : null; if (!mark || mark.closest(".ostrin-export-crosshair")) { hideCrosshair(); return; } let bounds; try { bounds = mark.getBBox(); } catch { hideCrosshair(); return; } if (!bounds || (!bounds.width && !bounds.height)) { hideCrosshair(); return; } const viewBox = svg.viewBox.baseVal; const x0 = viewBox.x || 0, y0 = viewBox.y || 0; const width = viewBox.width || Number(svg.getAttribute("width")) || 1, height = viewBox.height || Number(svg.getAttribute("height")) || 1; const x = bounds.x + bounds.width / 2, y = bounds.y + bounds.height / 2; vertical.setAttribute("x1", x); vertical.setAttribute("x2", x); vertical.setAttribute("y1", y0); vertical.setAttribute("y2", y0 + height); horizontal.setAttribute("x1", x0); horizontal.setAttribute("x2", x0 + width); horizontal.setAttribute("y1", y); horizontal.setAttribute("y2", y); marker.setAttribute("cx", x); marker.setAttribute("cy", y); label.setAttribute("x", Math.min(x + 8, x0 + width - 12)); label.setAttribute("y", Math.max(y - 8, y0 + 14)); label.textContent = mark.querySelector("title")?.textContent?.trim() || mark.getAttribute("aria-label") || "Selected mark"; overlay.setAttribute("visibility", "visible"); status.textContent = label.textContent; };',
    '    svg.addEventListener("pointermove", showCrosshair);',
    '    svg.addEventListener("pointerleave", hideCrosshair);',
    '    overlay.setAttribute("visibility", "hidden");',
    '  } else { crosshairButton.disabled = true; status.textContent = "No inspectable marks in this figure."; }',
    '  crosshairButton.addEventListener("click", () => { crosshairEnabled = !crosshairEnabled; crosshairButton.setAttribute("aria-pressed", String(crosshairEnabled)); crosshairButton.textContent = crosshairEnabled ? "Disable crosshair" : "Enable crosshair"; hideCrosshair(); status.textContent = crosshairEnabled ? "Move over a mark to inspect its values." : "Crosshair disabled."; });',
    "  applyTransform();",
    "})();",
    "</script>",
    "</body></html>",
  ].join("\n");
  downloadBlob(new Blob([html], { type: "text/html;charset=utf-8" }), fileStem(title) + ".html");
  return { width: dimensions.width, height: dimensions.height };
}

function printPdf(source, title, fraction = 0) {
  const staticSource = sanitizeSvg(staticSvgAt(source, fraction));
  const dimensions = svgDimensions(staticSource);
  const metadata = provenanceOf(staticSource);
  const popup = window.open("", "_blank");
  if (!popup) throw new Error("PDF export was blocked; allow pop-ups for this site");
  const safeTitle = escapeHtml(title || "Ostrin figure");
  const provenance = metadata
    ? escapeHtml(provenanceText(metadata, "Provenance"))
    : "No reproducibility metadata recorded.";
  // The browser's print pipeline preserves the original SVG vectors and metadata.
  // Users can choose “Save as PDF” in the native dialog; no raster or server is involved.
  popup.document.open();
  popup.document.write(`<!doctype html><html lang="en"><head><meta charset="utf-8"><title>${safeTitle} · Ostrin PDF</title><style>@page{size:${dimensions.width}px ${dimensions.height}px;margin:12mm}*{box-sizing:border-box}html,body{margin:0;color:#172033;background:#fff;font:14px system-ui,sans-serif}main{display:grid;gap:12px;max-width:${dimensions.width}px;margin:0 auto}h1{font-size:18px;margin:0}p{margin:0;color:#4b5563;font-size:11px;overflow-wrap:anywhere}svg{display:block;width:100%;height:auto;max-height:calc(100vh - 78px)}@media print{p{color:#172033}}</style></head><body><main><h1>${safeTitle}</h1><p>${provenance}</p>${staticSource}</main></body></html>`);
  popup.document.close();
  popup.focus();
  const print = () => popup.print();
  if (popup.document.readyState === "complete") setTimeout(print, 0);
  else popup.addEventListener("load", print, { once: true });
  return { width: dimensions.width, height: dimensions.height };
}

const VIDEO_MIME_CANDIDATES = {
  webm: ["video/webm;codecs=vp9", "video/webm;codecs=vp8", "video/webm"],
  // MP4 is deliberately capability-detected. Chromium builds commonly expose
  // WebM only, while Safari and some installed codecs expose H.264 through the
  // same MediaRecorder API. Never offer a file that the browser cannot encode.
  mp4: ["video/mp4;codecs=avc1.42E01E,mp4a.40.2", "video/mp4;codecs=avc1.4D401E", "video/mp4"],
};

function videoMimeType(format = "webm") {
  if (!globalThis.MediaRecorder?.isTypeSupported) return "";
  const candidates = VIDEO_MIME_CANDIDATES[format] ?? VIDEO_MIME_CANDIDATES.webm;
  return candidates.find((type) => {
    try {
      return MediaRecorder.isTypeSupported(type);
    } catch {
      return false;
    }
  }) ?? "";
}

async function exportAnimatedVideo(source, duration, title, format = "webm", loops = 1, scale = 1) {
  const mime = videoMimeType(format);
  const label = format.toUpperCase();
  if (!mime || !HTMLCanvasElement.prototype.captureStream) {
    throw new Error(`${label} export needs a compatible MediaRecorder codec and canvas capture in this browser`);
  }
  const dimensions = svgDimensions(source);
  const renderScale = Math.max(1, Math.min(2, Number(scale) || 1));
  const document_ = new DOMParser().parseFromString(source, "image/svg+xml");
  const flipbookFrames = document_.querySelectorAll(".ostrin-frame").length;
  const fps = flipbookFrames ? Math.max(1, Math.round(flipbookFrames / duration)) : 15;
  const framesPerLoop = flipbookFrames || Math.min(180, Math.max(30, Math.ceil(duration * fps)));
  const loopCount = Math.max(1, Math.floor(Number(loops) || 1));
  const frameCount = framesPerLoop * loopCount;
  const canvas = document.createElement("canvas");
  canvas.width = dimensions.width * renderScale;
  canvas.height = dimensions.height * renderScale;
  const context = canvas.getContext("2d");
  if (!context) throw new Error(`${label} export needs a 2D canvas context`);
  const stream = canvas.captureStream(fps);
  const recorder = new MediaRecorder(stream, { mimeType: mime });
  const chunks = [];
  const finished = new Promise((resolve, reject) => {
    recorder.ondataavailable = (event) => { if (event.data.size) chunks.push(event.data); };
    recorder.onerror = () => reject(new Error(`the browser stopped ${label} recording`));
    recorder.onstop = resolve;
  });
  recorder.start();
  try {
    for (let index = 0; index < frameCount; index += 1) {
      const frameIndex = index % framesPerLoop;
      const frame = await imageFromSvg(staticSvgAt(source, framesPerLoop === 1 ? 0 : frameIndex / (framesPerLoop - 1)));
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.drawImage(frame, 0, 0, canvas.width, canvas.height);
      await new Promise((resolve) => setTimeout(resolve, Math.max(8, 1000 / fps)));
    }
  } finally {
    recorder.stop();
    stream.getTracks().forEach((track) => track.stop());
  }
  await finished;
  const blob = new Blob(chunks, { type: mime });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  const stem = (title || "ostrin-viz-animation").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "ostrin-viz-animation";
  link.download = stem + (renderScale === 1 ? "" : "-" + renderScale + "x") + `.${format}`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return { frameCount, fps, scale: renderScale, format };
}

// GIF is intentionally encoded here instead of adding a large runtime dependency. The fixed
// 6×6×6 cube plus grayscale palette is deterministic, and the SVG itself remains the lossless
// publication route. The comment extension carries the reproducibility record when one exists.
const GIF_PALETTE = (() => {
  const palette = [];
  for (let red = 0; red < 6; red += 1) {
    for (let green = 0; green < 6; green += 1) {
      for (let blue = 0; blue < 6; blue += 1) palette.push([red * 51, green * 51, blue * 51]);
    }
  }
  for (let gray = 0; palette.length < 256; gray += 1) {
    const value = Math.round(gray * 255 / 39);
    palette.push([value, value, value]);
  }
  return palette;
})();

function gifPaletteIndex(red, green, blue) {
  const redLevel = Math.round(red / 51);
  const greenLevel = Math.round(green / 51);
  const blueLevel = Math.round(blue / 51);
  const cubeIndex = redLevel * 36 + greenLevel * 6 + blueLevel;
  const cube = GIF_PALETTE[cubeIndex];
  const cubeDistance = (red - cube[0]) ** 2 + (green - cube[1]) ** 2 + (blue - cube[2]) ** 2;
  const gray = Math.round((red * 299 + green * 587 + blue * 114) / 1000);
  const grayLevel = Math.round(gray * 39 / 255);
  const grayValue = Math.round(grayLevel * 255 / 39);
  const grayDistance = (red - grayValue) ** 2 + (green - grayValue) ** 2 + (blue - grayValue) ** 2;
  return grayDistance < cubeDistance ? 216 + grayLevel : cubeIndex;
}

function indexedGifPixels(imageData) {
  const indices = new Uint8Array(imageData.data.length / 4);
  for (let pixel = 0, offset = 0; offset < imageData.data.length; pixel += 1, offset += 4) {
    const alpha = imageData.data[offset + 3] / 255;
    const red = imageData.data[offset] * alpha + 255 * (1 - alpha);
    const green = imageData.data[offset + 1] * alpha + 255 * (1 - alpha);
    const blue = imageData.data[offset + 2] * alpha + 255 * (1 - alpha);
    indices[pixel] = gifPaletteIndex(red, green, blue);
  }
  return indices;
}

function gifLzw(indices) {
  const clearCode = 256;
  const endCode = 257;
  let nextCode = 258;
  let codeSize = 9;
  let bitBuffer = 0;
  let bitCount = 0;
  const bytes = [];
  const dictionary = new Map();
  const emit = (code) => {
    bitBuffer |= code << bitCount;
    bitCount += codeSize;
    while (bitCount >= 8) {
      bytes.push(bitBuffer & 0xff);
      bitBuffer >>>= 8;
      bitCount -= 8;
    }
  };
  emit(clearCode);
  let current = indices[0] ?? 0;
  for (let index = 1; index < indices.length; index += 1) {
    const next = indices[index];
    const key = current * 256 + next;
    const joined = dictionary.get(key);
    if (joined !== undefined) {
      current = joined;
      continue;
    }
    emit(current);
    if (nextCode < 4096) {
      dictionary.set(key, nextCode);
      nextCode += 1;
      if (nextCode === (1 << codeSize) && codeSize < 12) codeSize += 1;
    } else {
      emit(clearCode);
      dictionary.clear();
      nextCode = 258;
      codeSize = 9;
    }
    current = next;
  }
  emit(current);
  emit(endCode);
  if (bitCount > 0) bytes.push(bitBuffer & 0xff);
  return bytes;
}

function gifAscii(output, text) {
  for (const character of text) output.push(character.charCodeAt(0));
}

function gifWord(output, value) {
  output.push(value & 0xff, (value >>> 8) & 0xff);
}

function gifSubBlocks(output, bytes) {
  for (let offset = 0; offset < bytes.length; offset += 255) {
    const block = bytes.slice(offset, offset + 255);
    output.push(block.length, ...block);
  }
  output.push(0);
}

function encodeGif(frames, width, height, fps, loops, comment) {
  if (width > 65535 || height > 65535) throw new Error("GIF export supports figures up to 65535×65535 pixels");
  const output = [];
  gifAscii(output, "GIF89a");
  gifWord(output, width);
  gifWord(output, height);
  output.push(0xf7, 0, 0); // global table, 8-bit color resolution, 256 colors
  for (const [red, green, blue] of GIF_PALETTE) output.push(red, green, blue);
  output.push(0x21, 0xff, 0x0b);
  gifAscii(output, "NETSCAPE2.0");
  output.push(0x03, 0x01);
  gifWord(output, Math.max(0, (Number(loops) || 1) - 1));
  output.push(0);
  if (comment) {
    const commentBytes = new TextEncoder().encode(comment).slice(0, 2048);
    output.push(0x21, 0xfe);
    gifSubBlocks(output, [...commentBytes]);
  }
  const delay = Math.max(1, Math.min(65535, Math.round(100 / fps)));
  for (const frame of frames) {
    output.push(0x21, 0xf9, 0x04, 0x00);
    gifWord(output, delay);
    output.push(0, 0, 0x2c, 0, 0, 0, 0);
    gifWord(output, width);
    gifWord(output, height);
    output.push(0x00, 0x08);
    gifSubBlocks(output, gifLzw(frame));
  }
  output.push(0x3b);
  return new Uint8Array(output);
}

async function exportAnimatedGif(source, duration, title, loops = 1, scale = 1) {
  const dimensions = svgDimensions(source);
  const renderScale = Math.max(1, Math.min(2, Number(scale) || 1));
  const document_ = new DOMParser().parseFromString(source, "image/svg+xml");
  const flipbookFrames = document_.querySelectorAll(".ostrin-frame").length;
  const fps = flipbookFrames ? Math.max(1, Math.round(flipbookFrames / duration)) : 15;
  const framesPerLoop = flipbookFrames || Math.min(180, Math.max(30, Math.ceil(duration * fps)));
  const loopCount = Math.max(1, Math.floor(Number(loops) || 1));
  const frameCount = framesPerLoop * loopCount;
  const canvas = document.createElement("canvas");
  canvas.width = dimensions.width * renderScale;
  canvas.height = dimensions.height * renderScale;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) throw new Error("GIF export needs a 2D canvas context");
  const frames = [];
  for (let index = 0; index < frameCount; index += 1) {
    const frameIndex = index % framesPerLoop;
    const image = await imageFromSvg(staticSvgAt(source, framesPerLoop === 1 ? 0 : frameIndex / (framesPerLoop - 1)));
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    frames.push(indexedGifPixels(context.getImageData(0, 0, canvas.width, canvas.height)));
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  const metadata = provenanceOf(source);
  const comment = metadata ? provenanceText(metadata, "Ostrin provenance") : "Ostrin SVG animation";
  const blob = new Blob([encodeGif(frames, canvas.width, canvas.height, fps, loopCount, comment)], { type: "image/gif" });
  downloadBlob(blob, fileStem(title || "ostrin-animation") + (renderScale === 1 ? "" : "-" + renderScale + "x") + ".gif");
  return { frameCount, fps, scale: renderScale };
}

// ---- explorer: the SVG in a sandboxed frame (no scripts) where its own hover styles and
// <title> tooltips work; zoom rescales the vector figure and the frame scrolls to pan.
let dialog;
function explorer() {
  if (dialog) return dialog;
  const frame = el("iframe", { className: "viz-frame-live", sandbox: "allow-same-origin", title: "Interactive figure" });
  const heading = el("h2", { className: "viz-dialog-title" });
  const zoomLabel = el("output", { className: "viz-zoom", text: "100%" });
  const animationLabel = el("output", { className: "viz-animation-time", text: "0%" });
  const animationSlider = el("input", { type: "range", className: "viz-animation-slider", min: "0", max: "1000", value: "0", step: "1", "aria-label": "Animation position" });
  const animationSpeedInput = el("input", { type: "range", className: "viz-animation-speed", min: "25", max: "400", value: "100", step: "25", "aria-label": "Animation speed" });
  const animationSpeedLabel = el("output", { className: "viz-animation-speed-label", text: "1×" });
  const animationStatus = el("span", { className: "viz-animation-status", "aria-live": "polite", text: "" });
  const animationLoop = el("select", { className: "viz-animation-loop", "data-viz-loop": "", "aria-label": "Animation loop count" }, [
    el("option", { value: "0", text: "∞" }),
    el("option", { value: "1", text: "1×" }),
    el("option", { value: "3", text: "3×" }),
    el("option", { value: "5", text: "5×" }),
  ]);
  const animationQuality = el("select", { className: "viz-animation-loop", "data-viz-quality": "", "aria-label": "Animation export quality" }, [
    el("option", { value: "1", text: "1×" }),
    el("option", { value: "2", text: "2×" }),
  ]);
  const mp4Supported = Boolean(globalThis.HTMLCanvasElement?.prototype?.captureStream && videoMimeType("mp4"));
  const mp4Button = el("button", {
    type: "button",
    className: "button-quiet",
    text: "Export MP4",
    "data-viz-mp4-export": "",
    disabled: !mp4Supported,
    title: mp4Supported ? "Export an H.264 MP4 when supported by this browser" : "This browser does not expose an MP4 MediaRecorder codec",
  });
  const animationTools = el("div", { className: "viz-animation-tools", hidden: true }, [
    el("span", { className: "viz-animation-caption", text: "Animation" }),
    el("button", { type: "button", className: "button-quiet", text: "Play", "data-viz-play": "" }),
    el("button", { type: "button", className: "button-quiet", text: "Pause", "data-viz-pause": "" }),
    el("button", { type: "button", className: "button-quiet", text: "Restart", "data-viz-restart": "" }),
    el("button", { type: "button", className: "button-quiet", text: "Export WebM", "data-viz-export": "" }),
    mp4Button,
    el("button", { type: "button", className: "button-quiet", text: "Export GIF", "data-viz-gif-export": "" }),
    animationSlider,
    animationLabel,
    el("label", { className: "viz-animation-speed-control" }, [el("span", { text: "Speed" }), animationSpeedInput, animationSpeedLabel]),
    el("label", { className: "viz-animation-loop-control" }, [el("span", { text: "Loops" }), animationLoop]),
    el("label", { className: "viz-animation-loop-control" }, [el("span", { text: "Quality" }), animationQuality]),
    animationStatus,
  ]);
  const tableFilter = el("input", { id: "viz-table-filter", type: "search", placeholder: "Search rows", "aria-label": "Filter table rows" });
  const tableSort = el("select", { id: "viz-table-sort", "aria-label": "Sort table by" });
  const tableDirection = el("button", { type: "button", className: "button-quiet", text: "Ascending", "data-viz-table-direction": "" });
  const tableStatus = el("output", { className: "viz-table-status", "aria-live": "polite", text: "" });
  const tableTools = el("div", { className: "viz-table-tools", hidden: true }, [
    el("span", { className: "viz-animation-caption", text: "Table" }),
    el("label", { className: "viz-table-control" }, [el("span", { text: "Filter" }), tableFilter]),
    el("label", { className: "viz-table-control" }, [el("span", { text: "Sort by" }), tableSort]),
    tableDirection,
    tableStatus,
  ]);
  const selectionStatus = el("span", { className: "viz-selection-status", "aria-live": "polite", text: "" });
  const selectionTools = el("div", { className: "viz-selection-tools", hidden: true }, [
    el("span", { className: "viz-animation-caption", text: "Linked selection" }),
    selectionStatus,
    el("button", { type: "button", className: "button-quiet", text: "Clear selection", "data-viz-selection-clear": "" }),
  ]);
  const contractStatus = el("span", { className: "viz-contract-status", "aria-live": "polite", text: "" });
  const contractTools = el("div", { className: "viz-contract-tools", hidden: true }, [
    el("span", { className: "viz-animation-caption", text: "Event contracts" }),
    contractStatus,
    el("button", { type: "button", className: "button-quiet", text: "Clear linked events", "data-viz-contract-clear": "" }),
  ]);
  const cameraAzimuth = el("input", { type: "range", className: "viz-camera-slider", min: "-180", max: "180", step: "1", value: "-55", "aria-label": "3D camera azimuth", "data-viz-camera-azimuth": "" });
  const cameraAzimuthLabel = el("output", { className: "viz-camera-angle", text: "-55°" });
  const cameraElevation = el("input", { type: "range", className: "viz-camera-slider", min: "-80", max: "80", step: "1", value: "28", "aria-label": "3D camera elevation", "data-viz-camera-elevation": "" });
  const cameraElevationLabel = el("output", { className: "viz-camera-angle", text: "28°" });
  const cameraStatus = el("span", { className: "viz-camera-status", "aria-live": "polite", text: "" });
  const cameraTools = el("div", { className: "viz-camera-tools", hidden: true }, [
    el("span", { className: "viz-animation-caption", text: "3D camera" }),
    el("label", { className: "viz-camera-control" }, [el("span", { text: "Azimuth" }), cameraAzimuth, cameraAzimuthLabel]),
    el("label", { className: "viz-camera-control" }, [el("span", { text: "Elevation" }), cameraElevation, cameraElevationLabel]),
    el("button", { type: "button", className: "button-quiet", text: "Reset view", "data-viz-camera-reset": "" }),
    cameraStatus,
  ]);
  const parameterStatus = el("span", { className: "viz-parameter-status", "aria-live": "polite", text: "" });
  const parameterControls = el("div", { className: "viz-parameter-controls" });
  const parameterTools = el("div", { className: "viz-parameter-tools", hidden: true }, [
    el("span", { className: "viz-animation-caption", text: "Ostrin parameters" }),
    parameterControls,
    parameterStatus,
  ]);
  const legendStatus = el("span", { className: "viz-legend-status", "aria-live": "polite", text: "" });
  const legendControls = el("div", { className: "viz-legend-controls" });
  const legendTools = el("div", { className: "viz-legend-tools", hidden: true }, [
    el("span", { className: "viz-animation-caption", text: "Series" }),
    legendControls,
    legendStatus,
  ]);
  const crosshairStatus = el("span", { className: "viz-crosshair-status", "aria-live": "polite", text: "" });
  const crosshairToggle = el("button", { type: "button", className: "button-quiet", text: "Enable crosshair", "data-viz-crosshair-toggle": "", "aria-pressed": "false" });
  const crosshairTools = el("div", { className: "viz-crosshair-tools", hidden: true }, [
    el("span", { className: "viz-animation-caption", text: "Inspect" }),
    crosshairToggle,
    crosshairStatus,
  ]);
  let zoom = 100;
  let svg = "";
  let cameraSource = "";
  let cameraEnabled = false;
  let sourceControls = [];
  let sourceValues = {};
  let onCameraSvg = null;
  let cameraTimer = 0;
  let cameraGeneration = 0;
  let cameraRendering = false;
  let cameraQueued = false;
  let selectedIndex = null;
  let eventContracts = [];
  let contractState = new Map();
  let animationDuration = 0;
  let animationPaused = false;
  let animationRate = 1;
  let animationLoopLimit = 0;
  let animationLoopCount = 0;
  let animationFrameRequest = 0;
  let animationClockStartedAt = 0;
  let animationClockStartSeconds = 0;
  let tableRows = [];
  let tableSortColumn = -1;
  let tableAscending = true;
  let legendItems = [];
  let crosshairEnabled = false;
  let crosshairOverlay = null;
  let shareState = null;
  const render = () => {
    zoomLabel.textContent = `${zoom}%`;
    frame.srcdoc = `<!doctype html><meta charset="utf-8"><style>html,body{margin:0;background:#fff}body>svg{display:block;width:${zoom}%;height:auto}body>svg svg{display:block;width:auto;height:auto}[data-viz-index]{cursor:pointer}[data-viz-index]:focus{outline:2px solid #7c3aed;outline-offset:2px}.pt.viz-linked-selected{stroke:#7c3aed!important;stroke-width:3px!important;stroke-opacity:1!important}.table-row.viz-linked-selected .table-cell{stroke:#7c3aed;stroke-width:2}.table-row.viz-linked-selected .table-value{font-weight:700}.viz-contract-hover{filter:drop-shadow(0 0 3px #f59e0b)}.viz-contract-focus{filter:drop-shadow(0 0 3px #0ea5e9)}.viz-contract-selected{filter:drop-shadow(0 0 4px #db2777)}.table-row.viz-contract-hover .table-cell,.table-row.viz-contract-focus .table-cell,.table-row.viz-contract-selected .table-cell{stroke-width:2.5}.table-row.viz-contract-selected .table-value{font-weight:700}</style>${svg}`;
  };
  const sourceWithCamera = (source, azimuth, elevation) => {
    const view = `.view(${azimuth.toFixed(1)}, ${elevation.toFixed(1)})`;
    if (/\.view\s*\([^)]*\)/.test(source)) return source.replace(/\.view\s*\([^)]*\)/, view);
    return source.replace(/(viz\.scene3d\("(?:\\.|[^"\\])*"\))/, `$1${view}`);
  };
  const sourceForRun = () => {
    const parameterized = applySourceParameters(cameraSource, sourceControls, sourceValues);
    return cameraEnabled ? sourceWithCamera(parameterized, cameraAzimuth.valueAsNumber, cameraElevation.valueAsNumber) : parameterized;
  };
  const renderCamera = async (generation) => {
    if (cameraRendering) { cameraQueued = true; return; }
    cameraRendering = true;
    const azimuth = cameraAzimuth.valueAsNumber;
    const elevation = cameraElevation.valueAsNumber;
    const status = cameraEnabled ? cameraStatus : parameterStatus;
    status.textContent = "Rendering with Ostrin…";
    try {
      const { runOstrinc } = await runtime();
      const source = sourceForRun();
      const result = await runOstrinc({ "main.ostrin": source }, ["--run", "main.ostrin"]);
      const stdout = result.lines.filter(([kind]) => kind === "out").map(([, text]) => text);
      const rendered = svgOf(stdout);
      if (generation !== cameraGeneration) return;
      if (result.code !== 0 || !rendered.svg) {
        const errors = result.lines.filter(([kind]) => kind === "err").map(([, text]) => text);
        cameraStatus.textContent = errors.join(" ") || `Ostrin exited with ${result.code}`;
        return;
      }
      svg = rendered.svg;
      render();
      onCameraSvg?.(svg);
      const parameterSummary = sourceControls.map((parameter) => `${parameter.label ?? parameter.name} ${sourceValues[parameter.name]}`).join(" · ");
      status.textContent = cameraEnabled
        ? `Rendered by Ostrin · azimuth ${azimuth}° · elevation ${elevation}°${parameterSummary ? ` · ${parameterSummary}` : ""}`
        : `Rendered by Ostrin${parameterSummary ? ` · ${parameterSummary}` : ""}`;
    } catch (error) {
      status.textContent = `Could not render: ${error.message ?? error}`;
    } finally {
      cameraRendering = false;
      if (cameraQueued || generation !== cameraGeneration) {
        cameraQueued = false;
        clearTimeout(cameraTimer);
        cameraTimer = setTimeout(() => renderCamera(cameraGeneration), 0);
      }
    }
  };
  const scheduleCameraRender = () => {
    cameraGeneration += 1;
    cameraAzimuthLabel.textContent = `${cameraAzimuth.value}°`;
    cameraElevationLabel.textContent = `${cameraElevation.value}°`;
    const status = cameraEnabled ? cameraStatus : parameterStatus;
    status.textContent = cameraEnabled ? "Camera or parameter changed · waiting to render…" : "Parameter changed · waiting to render…";
    clearTimeout(cameraTimer);
    cameraTimer = setTimeout(() => renderCamera(cameraGeneration), 180);
  };
  const animationDocument = () => frame.contentDocument;
  function stopAnimationClock() {
    if (animationFrameRequest) cancelAnimationFrame(animationFrameRequest);
    animationFrameRequest = 0;
  }
  function freezeAnimationAtEnd() {
    const document_ = animationDocument();
    const frames = [...(document_?.querySelectorAll(".ostrin-frame") ?? [])];
    if (frames.length) {
      frames.forEach((node, index) => {
        node.style.animation = "none";
        node.style.opacity = index === frames.length - 1 ? "1" : "0";
      });
    } else {
      const root = document_?.documentElement;
      if (root?.setCurrentTime && animationDuration) root.setCurrentTime(Math.max(0, animationDuration / animationRate - 0.0001));
    }
    animationSlider.value = "1000";
    animationLabel.textContent = "100%";
  }
  function animationTick(now) {
    if (animationPaused || !animationDuration) return;
    const elapsed = Math.max(0, now - animationClockStartedAt) / 1000 * animationRate;
    const logicalSeconds = animationClockStartSeconds + elapsed;
    const completedLoops = Math.floor(logicalSeconds / animationDuration);
    if (animationLoopLimit > 0 && logicalSeconds >= animationDuration * animationLoopLimit) {
      animationLoopCount = animationLoopLimit;
      freezeAnimationAtEnd();
      setAnimationState(true);
      animationStatus.textContent = `completed ${animationLoopLimit} ${animationLoopLimit === 1 ? "loop" : "loops"}`;
      return;
    }
    animationLoopCount = completedLoops;
    const fraction = (logicalSeconds % animationDuration) / animationDuration;
    animationSlider.value = String(Math.round(fraction * 1000));
    setAnimationTime(animationSlider.value, false);
    animationFrameRequest = requestAnimationFrame(animationTick);
  }
  function startAnimationClock() {
    stopAnimationClock();
    if (animationPaused || !animationDuration) return;
    if (Number(animationSlider.value) >= 1000) {
      animationLoopCount = 0;
      setAnimationTime(0, false);
    }
    animationClockStartSeconds = Number(animationSlider.value) / 1000 * animationDuration;
    animationClockStartedAt = performance.now();
    animationFrameRequest = requestAnimationFrame(animationTick);
  }
  const captureAnimationFrames = () => {
    animationDocument()?.querySelectorAll(".ostrin-frame").forEach((node) => {
      node.dataset.baseDelay = String(parseFloat(node.style.animationDelay) || 0);
    });
  };
  const setAnimationState = (paused) => {
    if (paused) stopAnimationClock();
    const document_ = animationDocument();
    const root = document_?.documentElement;
    if (root?.pauseAnimations && root?.unpauseAnimations) {
      if (paused) root.pauseAnimations();
      else root.unpauseAnimations();
    }
    document_?.querySelectorAll(".ostrin-frame").forEach((node) => {
      if (!paused) node.style.animation = "";
      node.style.animationPlayState = paused ? "paused" : "running";
    });
    animationPaused = paused;
    if (!paused) startAnimationClock();
  };
  const setAnimationTime = (value, pause = true) => {
    const fraction = Number(value) / 1000;
    const seconds = fraction * animationDuration / animationRate;
    const document_ = animationDocument();
    const root = document_?.documentElement;
    if (root?.setCurrentTime && animationDuration) root.setCurrentTime(seconds);
    document_?.querySelectorAll(".ostrin-frame").forEach((node) => {
      const base = Number(node.dataset.baseDelay ?? 0) / animationRate;
      node.style.animationDelay = `${base - seconds * 1000}ms`;
    });
    if (pause) setAnimationState(true);
    animationLabel.textContent = `${Math.round(fraction * 100)}%`;
  };
  const setAnimationSpeed = (value) => {
    animationRate = Number(value) / 100;
    animationSpeedLabel.textContent = `${animationRate.toFixed(2).replace(/\.00$/, "")}×`;
    const document_ = animationDocument();
    document_?.querySelectorAll(".ostrin-frame").forEach((node) => {
      node.style.animationDuration = `${animationDuration / animationRate}s`;
      node.style.animationDelay = `${Number(node.dataset.baseDelay ?? 0) / animationRate}ms`;
    });
    document_?.querySelectorAll("animate").forEach((node) => {
      const base = Number(node.dataset.baseDurationSeconds ?? 0);
      if (base > 0) node.setAttribute("dur", `${base / animationRate}s`);
    });
    if (document_?.documentElement?.setCurrentTime && animationDuration) setAnimationTime(animationSlider.value, false);
    if (!animationPaused) {
      animationClockStartSeconds = Number(animationSlider.value) / 1000 * animationDuration;
      animationClockStartedAt = performance.now();
    }
  };
  const prepareAnimation = () => {
    const animated = /<animate\b|ostrin-frame|@keyframes/.test(svg);
    animationTools.hidden = !animated;
    if (!animated) return;
    const durations = [...svg.matchAll(/(?:dur="|animation:[^;]*?\s)([\d.]+)(ms|s)/g)].map((match) => Number(match[1]) * (match[2] === "ms" ? 0.001 : 1));
    animationDuration = Math.max(...durations, 1);
    animationSlider.value = "0";
    animationSpeedInput.value = "100";
    animationRate = 1;
    animationSpeedLabel.textContent = "1×";
    animationLoop.value = "0";
    animationQuality.value = "1";
    animationLoopLimit = 0;
    animationLoopCount = 0;
    animationStatus.textContent = "";
    animationPaused = false;
    stopAnimationClock();
    requestAnimationFrame(captureAnimationFrames);
  };
  const prepareAnimationDocument = () => {
    animationDocument()?.querySelectorAll("animate").forEach((node) => {
      const duration = node.getAttribute("dur") ?? "";
      const match = duration.match(/^([\d.]+)(ms|s)$/);
      if (match) node.dataset.baseDurationSeconds = String(Number(match[1]) * (match[2] === "ms" ? 0.001 : 1));
    });
  };
  const applyReducedMotion = () => {
    if (!animationTools.hidden && globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
      setAnimationTime(0, true);
      animationStatus.textContent = "reduced motion";
    }
  };
  const numericTableValue = (value) => {
    const match = value.replace(/,/g, "").match(/[-+]?(?:\d+\.?\d*|\.\d+)(?:e[-+]?\d+)?/i);
    return match ? Number(match[0]) : Number.NaN;
  };
  const compareTableRows = (left, right) => {
    const a = left.values[tableSortColumn] ?? "";
    const b = right.values[tableSortColumn] ?? "";
    const na = numericTableValue(a);
    const nb = numericTableValue(b);
    let result;
    if (Number.isFinite(na) && Number.isFinite(nb)) return tableAscending ? na - nb : nb - na;
    if (Number.isFinite(na)) return -1;
    if (Number.isFinite(nb)) return 1;
    result = a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });
    return tableAscending ? result : -result;
  };
  const applyTableState = () => {
    if (!tableRows.length) return;
    const query = tableFilter.value.trim().toLocaleLowerCase();
    const matches = (row) => !query || row.values.some((value) => value.toLocaleLowerCase().includes(query));
    const visible = tableRows.filter(matches);
    const orderedVisible = tableSortColumn < 0 ? [...visible] : [...visible].sort(compareTableRows);
    const hidden = tableRows.filter((row) => !matches(row));
    const ordered = [...orderedVisible, ...hidden];
    const firstY = Math.min(...tableRows.map((row) => row.baseY));
    const rowHeight = Number(tableRows[0].node.querySelector(".table-cell")?.getAttribute("height")) || 30;
    const stripes = tableRows.slice(0, 2).map((row) => row.node.querySelector(".table-cell")?.getAttribute("fill")).filter(Boolean);
    const body = animationDocument()?.querySelector(".table-body");
    ordered.forEach((row, index) => {
      const isVisible = matches(row);
      row.node.style.display = isVisible ? "" : "none";
      if (isVisible) {
        const slot = orderedVisible.indexOf(row);
        row.node.setAttribute("transform", `translate(0 ${firstY + slot * rowHeight - row.baseY})`);
        if (stripes.length) row.node.querySelectorAll(".table-cell").forEach((cell) => cell.setAttribute("fill", stripes[slot % stripes.length]));
      } else {
        row.node.removeAttribute("transform");
      }
      body?.append(row.node);
    });
    const footer = animationDocument()?.querySelector("[data-table-footer]");
    const columns = tableRows[0].values.length;
    if (footer) footer.textContent = `${visible.length} of ${tableRows.length} rows · ${columns} columns`;
    tableStatus.textContent = `${visible.length} of ${tableRows.length} rows shown`;
  };
  const prepareTable = () => {
    const document_ = animationDocument();
    const nodes = [...(document_?.querySelectorAll("g.table-row") ?? [])];
    tableRows = nodes.map((node, index) => ({
      node,
      index,
      values: [...node.querySelectorAll(".table-cell")].map((cell) => cell.querySelector("title")?.textContent ?? ""),
      baseY: Number(node.getAttribute("data-table-y") ?? node.querySelector(".table-cell")?.getAttribute("y") ?? 0),
    }));
    tableTools.hidden = !tableRows.length;
    if (!tableRows.length) return;
    tableFilter.value = "";
    tableSortColumn = -1;
    tableAscending = true;
    tableDirection.textContent = "Ascending";
    tableSort.replaceChildren(el("option", { value: "-1", text: "Original order" }));
    [...document_.querySelectorAll(".table-heading")].forEach((heading, index) => {
      tableSort.append(el("option", { value: String(index), text: heading.textContent?.trim() || `Column ${index + 1}` }));
    });
    tableSort.value = "-1";
    applyTableState();
  };
  const prepareEventContracts = () => {
    const document_ = animationDocument();
    eventContracts = [];
    contractState = new Map();
    const channels = new Map();
    for (const root of document_?.querySelectorAll("svg[data-ostrin-bind]") ?? []) {
      const nodes = [...root.querySelectorAll("[data-viz-index]")];
      if (!nodes.length) continue;
      for (const raw of (root.getAttribute("data-ostrin-bind") ?? "").split(",")) {
        const [channel, event = "select"] = raw.split(":", 2).map((part) => part.trim());
        if (!channel || !["hover", "focus", "select"].includes(event)) continue;
        const contract = channels.get(channel) ?? { channel, events: new Set(), nodes: new Map() };
        contract.events.add(event);
        for (const node of nodes) {
          const index = node.getAttribute("data-viz-index");
          if (!index) continue;
          const linked = contract.nodes.get(index) ?? [];
          linked.push(node);
          contract.nodes.set(index, linked);
        }
        channels.set(channel, contract);
      }
    }
    eventContracts = [...channels.values()].filter((contract) => contract.nodes.size > 0);
    contractTools.hidden = eventContracts.length === 0;
    const apply = (contract) => {
      const state = contractState.get(contract.channel) ?? {};
      for (const [index, nodes] of contract.nodes) {
        nodes.forEach((node) => {
          node.classList.toggle("viz-contract-hover", state.hoverIndex === index);
          node.classList.toggle("viz-contract-focus", state.focusIndex === index);
          node.classList.toggle("viz-contract-selected", state.selectedIndex === index);
        });
      }
    };
    const clear = () => {
      contractState = new Map();
      eventContracts.forEach(apply);
      contractStatus.textContent = eventContracts.length ? `${eventContracts.length} linked channel${eventContracts.length === 1 ? "" : "s"} ready.` : "";
    };
    const update = (contract, kind, index) => {
      const state = contractState.get(contract.channel) ?? {};
      if (kind === "select") state.selectedIndex = index;
      else state[`${kind}Index`] = index;
      contractState.set(contract.channel, state);
      apply(contract);
      const ordinal = Number(index) + 1;
      contractStatus.textContent = `${contract.channel} · ${kind} row ${ordinal}`;
    };
    const clearTransient = (contract, kind, index, relatedTarget) => {
      if (relatedTarget && [...(contract.nodes.get(index) ?? [])].some((node) => node.contains?.(relatedTarget))) return;
      const state = contractState.get(contract.channel) ?? {};
      if (state[`${kind}Index`] !== index) return;
      delete state[`${kind}Index`];
      contractState.set(contract.channel, state);
      apply(contract);
      contractStatus.textContent = `${contract.channel} · linked channel ready.`;
    };
    for (const contract of eventContracts) {
      for (const event of contract.events) {
        for (const [index, nodes] of contract.nodes) {
          nodes.forEach((node) => {
            if (event === "hover") {
              node.addEventListener("pointerover", () => update(contract, "hover", index));
              node.addEventListener("pointerout", (event_) => clearTransient(contract, "hover", index, event_.relatedTarget));
            } else if (event === "focus") {
              node.addEventListener("focusin", () => update(contract, "focus", index));
              node.addEventListener("focusout", (event_) => clearTransient(contract, "focus", index, event_.relatedTarget));
            } else {
              node.setAttribute("tabindex", "0");
              node.addEventListener("click", () => update(contract, "select", index));
              node.addEventListener("keydown", (event_) => {
                if (event_.key === "Enter" || event_.key === " ") {
                  event_.preventDefault();
                  update(contract, "select", index);
                }
              });
            }
          });
        }
      }
    }
    contractTools.querySelector("[data-viz-contract-clear]").onclick = clear;
    clear();
  };
  const prepareLinkedSelection = () => {
    const document_ = animationDocument();
    const points = [...(document_?.querySelectorAll(".pt[data-viz-index]") ?? [])];
    const rows = [...(document_?.querySelectorAll("g.table-row[data-viz-index]") ?? [])];
    const pointByIndex = new Map(points.map((node) => [node.getAttribute("data-viz-index"), node]));
    const rowByIndex = new Map(rows.map((node) => [node.getAttribute("data-viz-index"), node]));
    const indices = [...pointByIndex.keys()].filter((index) => rowByIndex.has(index));
    selectionTools.hidden = indices.length === 0;
    selectedIndex = null;
    if (!indices.length) return;
    const select = (index) => {
      selectedIndex = index;
      points.forEach((node) => node.classList.toggle("viz-linked-selected", node.getAttribute("data-viz-index") === index));
      rows.forEach((node) => node.classList.toggle("viz-linked-selected", node.getAttribute("data-viz-index") === index));
      selectionStatus.textContent = `Selected row ${Number(index) + 1} of ${indices.length}`;
    };
    const clear = () => {
      selectedIndex = null;
      points.forEach((node) => node.classList.remove("viz-linked-selected"));
      rows.forEach((node) => node.classList.remove("viz-linked-selected"));
      selectionStatus.textContent = "Click a point or table row to link them.";
    };
    selectionStatus.textContent = "Click a point or table row to link them.";
    [...pointByIndex.entries(), ...rowByIndex.entries()].forEach(([index, node]) => {
      node.setAttribute("tabindex", "0");
      node.setAttribute("aria-label", `Select linked row ${Number(index) + 1}`);
      if (node.matches(".pt")) node.setAttribute("role", "button");
      node.addEventListener("click", () => select(index));
      node.addEventListener("keydown", (event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          select(index);
        }
      });
    });
    selectionTools.querySelector("[data-viz-selection-clear]").onclick = clear;
    if (selectedIndex !== null) select(selectedIndex);
  };
  const prepareLegend = () => {
    const document_ = animationDocument();
    const items = [...(document_?.querySelectorAll(".legend-item[data-viz-series-id]") ?? [])];
    const seriesById = new Map([...document_?.querySelectorAll(".viz-series[data-viz-series-id]") ?? []].map((node) => [node.getAttribute("data-viz-series-id"), node]));
    legendItems = items.map((item) => {
      const id = item.getAttribute("data-viz-series-id");
      const label = item.querySelector("text")?.textContent?.trim() || `Series ${Number(id) + 1}`;
      return { id, label, series: seriesById.get(id) };
    }).filter(({ series }) => series);
    legendTools.hidden = legendItems.length === 0;
    legendControls.replaceChildren(...legendItems.map(({ id, label, series }) => {
      const toggle = el("button", {
        type: "button",
        className: "button-quiet viz-legend-toggle",
        text: label,
        "data-viz-legend-toggle": id,
        "aria-pressed": "true",
        "aria-label": `Hide ${label}`,
      });
      toggle.addEventListener("click", () => {
        const nextVisible = toggle.getAttribute("aria-pressed") !== "true";
        series.style.display = nextVisible ? "" : "none";
        toggle.setAttribute("aria-pressed", String(nextVisible));
        toggle.setAttribute("aria-label", `${nextVisible ? "Hide" : "Show"} ${label}`);
        toggle.classList.toggle("is-hidden", !nextVisible);
        const visibleCount = legendItems.filter(({ series: node }) => node.style.display !== "none").length;
        legendStatus.textContent = `${visibleCount} of ${legendItems.length} series visible`;
      });
      return toggle;
    }));
    legendStatus.textContent = legendItems.length ? `${legendItems.length} series visible` : "";
  };
  const prepareCrosshair = () => {
    const document_ = animationDocument();
    const root = document_?.querySelector("svg");
    const inspectableSelector = "[data-viz-index],.bar,.boxplot,.violin,.hexbin-cell,.contourf-cell,.quiver,.streamplot,.viz-series";
    const inspectable = root?.querySelector(inspectableSelector);
    crosshairTools.hidden = !root || !inspectable;
    crosshairOverlay = null;
    if (!root || !inspectable) return;
    const namespace = "http://www.w3.org/2000/svg";
    const overlay = document_.createElementNS(namespace, "g");
    overlay.setAttribute("class", "viz-crosshair-overlay");
    overlay.setAttribute("aria-hidden", "true");
    overlay.style.pointerEvents = "none";
    const vertical = document_.createElementNS(namespace, "line");
    const horizontal = document_.createElementNS(namespace, "line");
    const marker = document_.createElementNS(namespace, "circle");
    const label = document_.createElementNS(namespace, "text");
    [vertical, horizontal].forEach((line) => {
      line.setAttribute("class", "viz-crosshair-line");
      line.setAttribute("stroke", "#7c3aed");
      line.setAttribute("stroke-width", "1");
      line.setAttribute("stroke-dasharray", "4 4");
    });
    marker.setAttribute("class", "viz-crosshair-marker");
    marker.setAttribute("r", "4");
    marker.setAttribute("fill", "#7c3aed");
    marker.setAttribute("stroke", "#fff");
    marker.setAttribute("stroke-width", "1.5");
    label.setAttribute("class", "viz-crosshair-label");
    label.setAttribute("font-size", "11");
    label.setAttribute("font-family", "ui-monospace, SFMono-Regular, Menlo, monospace");
    label.setAttribute("fill", "#4c1d95");
    label.setAttribute("paint-order", "stroke");
    label.setAttribute("stroke", "#fff");
    label.setAttribute("stroke-width", "3");
    label.setAttribute("stroke-linejoin", "round");
    overlay.append(vertical, horizontal, marker, label);
    root.append(overlay);
    crosshairOverlay = { overlay, vertical, horizontal, marker, label, root };
    const hide = () => {
      overlay.setAttribute("visibility", "hidden");
      if (crosshairEnabled) crosshairStatus.textContent = "Move over a mark to inspect its values.";
    };
    const show = (event) => {
      if (!crosshairEnabled) return;
      const target = event.target;
      const mark = typeof target?.closest === "function"
        ? target.closest(inspectableSelector)
        : null;
      if (!mark || mark.closest?.(".viz-crosshair-overlay")) {
        hide();
        return;
      }
      const title = mark.querySelector("title")?.textContent?.trim() || mark.getAttribute("aria-label") || "Selected mark";
      let bounds;
      try { bounds = mark.getBBox(); } catch { hide(); return; }
      if (!bounds || (!bounds.width && !bounds.height)) { hide(); return; }
      const viewBox = root.viewBox?.baseVal;
      const width = viewBox?.width || Number(root.getAttribute("width")) || 1;
      const height = viewBox?.height || Number(root.getAttribute("height")) || 1;
      const x = bounds.x + bounds.width / 2;
      const y = bounds.y + bounds.height / 2;
      vertical.setAttribute("x1", String(x));
      vertical.setAttribute("x2", String(x));
      vertical.setAttribute("y1", String(viewBox?.y || 0));
      vertical.setAttribute("y2", String((viewBox?.y || 0) + height));
      horizontal.setAttribute("x1", String(viewBox?.x || 0));
      horizontal.setAttribute("x2", String((viewBox?.x || 0) + width));
      horizontal.setAttribute("y1", String(y));
      horizontal.setAttribute("y2", String(y));
      marker.setAttribute("cx", String(x));
      marker.setAttribute("cy", String(y));
      label.setAttribute("x", String(Math.min(x + 8, (viewBox?.x || 0) + width - 12)));
      label.setAttribute("y", String(Math.max(y - 8, (viewBox?.y || 0) + 14)));
      label.textContent = title;
      overlay.setAttribute("visibility", "visible");
      crosshairStatus.textContent = title;
    };
    root.addEventListener("pointermove", show);
    root.addEventListener("pointerleave", hide);
    overlay.setAttribute("visibility", "hidden");
    crosshairStatus.textContent = crosshairEnabled ? "Move over a mark to inspect its values." : "Enable the crosshair to inspect marks.";
  };
  const setCrosshairEnabled = (enabled) => {
    crosshairEnabled = enabled;
    crosshairToggle.setAttribute("aria-pressed", String(enabled));
    crosshairToggle.textContent = enabled ? "Disable crosshair" : "Enable crosshair";
    crosshairOverlay?.overlay?.setAttribute("visibility", "hidden");
    crosshairStatus.textContent = enabled ? "Move over a mark to inspect its values." : "Crosshair disabled.";
  };
  const button = (text, label, action) => {
    const node = el("button", { type: "button", className: "button-quiet", "aria-label": label, text });
    node.addEventListener("click", action);
    return node;
  };
  const exportStatus = el("span", { className: "viz-export-status", "aria-live": "polite", text: "" });
  const shareStatus = el("span", { className: "viz-share-status", "aria-live": "polite", text: "" });
  const syncShareState = (mode = "replace") => {
    if (!shareState?.id) return;
    writeVizState(shareState.id, sourceValues, cameraEnabled ? cameraAzimuth.value : null, cameraEnabled ? cameraElevation.value : null, mode);
  };
  const shareButton = button("Link", "Copy share link", async () => {
    syncShareState("replace");
    try {
      if (!navigator.clipboard?.writeText) throw new Error("Clipboard API unavailable");
      await navigator.clipboard.writeText(window.location.href);
      shareStatus.textContent = "Link copied.";
    } catch {
      shareStatus.textContent = "Share URL ready — copy it from the address bar.";
    }
  });
  const exportSvgButton = button("SVG", "Download SVG", () => {
    downloadSvg(svg, heading.textContent);
    exportStatus.textContent = "SVG downloaded";
  });
  const exportPngButton = button("PNG", "Export PNG", async () => {
    exportPngButton.disabled = true;
    exportStatus.textContent = "exporting PNG…";
    try {
      const fraction = animationDuration ? Number(animationSlider.value) / 1000 : 0;
      const result = await exportPng(svg, heading.textContent, fraction);
      exportStatus.textContent = `PNG downloaded · ${result.width}×${result.height}`;
    } catch (error) {
      exportStatus.textContent = error.message ?? String(error);
    } finally {
      exportPngButton.disabled = false;
    }
  });
  const exportPdfButton = button("PDF", "Print figure as PDF", () => {
    try {
      const fraction = animationDuration ? Number(animationSlider.value) / 1000 : 0;
      const result = printPdf(svg, heading.textContent, fraction);
      exportStatus.textContent = `PDF print view opened · ${result.width}×${result.height}`;
    } catch (error) {
      exportStatus.textContent = error.message ?? String(error);
    }
  });
  const exportHtmlButton = button("HTML", "Export interactive HTML", () => {
    try {
      const fraction = animationDuration ? Number(animationSlider.value) / 1000 : 0;
      const result = exportInteractiveHtml(svg, heading.textContent, fraction);
      exportStatus.textContent = "HTML downloaded · interactive · " + result.width + "×" + result.height;
    } catch (error) {
      exportStatus.textContent = error.message ?? String(error);
    }
  });
  const prepareSourceParameters = (parameters = [], initialValues = {}) => {
    sourceControls = parameters;
    sourceValues = Object.fromEntries(parameters.map((parameter) => {
      const fallback = sourceParameterValue(cameraSource, parameter);
      const candidate = Number(initialValues[parameter.name]);
      const value = Number.isFinite(candidate) ? Math.max(parameter.min, Math.min(parameter.max, candidate)) : Number(fallback);
      return [parameter.name, String(value)];
    }));
    parameterControls.replaceChildren(...parameters.map((parameter) => {
      const id = `viz-parameter-${parameter.name}`;
      const original = sourceParameterValue(cameraSource, parameter);
      const output = el("output", { for: id, text: sourceParameterLiteral(sourceValues[parameter.name], original) });
      const input = el("input", {
        id,
        type: "range",
        min: parameter.min,
        max: parameter.max,
        step: parameter.step,
        value: Number(sourceValues[parameter.name]),
        "aria-label": parameter.label ?? parameter.name,
        "data-viz-parameter": parameter.name,
      });
      input.addEventListener("input", () => {
        sourceValues[parameter.name] = input.value;
        output.textContent = sourceParameterLiteral(input.value, original);
        syncShareState("replace");
        scheduleCameraRender();
      });
      return el("label", { className: "viz-parameter-control", for: id }, [el("span", { text: parameter.label ?? parameter.name }), input, output]);
    }));
    parameterTools.hidden = parameters.length === 0;
    parameterStatus.textContent = parameters.length ? "Parameters are evaluated by Ostrin." : "";
  };
  const node = el("dialog", { className: "viz-dialog", "aria-label": "Figure explorer" }, [
    el("div", { className: "viz-dialog-bar" }, [
      heading,
      el("div", { className: "viz-dialog-tools" }, [
        button("−", "Zoom out", () => { zoom = Math.max(50, zoom - 25); render(); }),
        zoomLabel,
        button("+", "Zoom in", () => { zoom = Math.min(400, zoom + 50); render(); }),
        exportSvgButton,
        exportPngButton,
        exportPdfButton,
        exportHtmlButton,
        shareButton,
        exportStatus,
        shareStatus,
        button("Close", "Close the explorer", () => node.close()),
      ]),
    ]),
    parameterTools,
    legendTools,
    crosshairTools,
    cameraTools,
    animationTools,
    tableTools,
    selectionTools,
    contractTools,
    el("p", { className: "sl-provenance", text: "Hover a point or bar for its values (the SVG's own <title> tooltips). Toggle legend series or enable the crosshair to inspect data. Zoom rescales the vector figure; scroll to pan." }),
    frame,
  ]);
  animationTools.querySelector("[data-viz-play]").addEventListener("click", () => setAnimationState(false));
  animationTools.querySelector("[data-viz-pause]").addEventListener("click", () => setAnimationState(true));
  animationTools.querySelector("[data-viz-restart]").addEventListener("click", () => {
    animationLoopCount = 0;
    animationSlider.value = "0";
    setAnimationTime(0, false);
    setAnimationState(false);
  });
  animationLoop.addEventListener("change", () => {
    animationLoopLimit = Number(animationLoop.value);
    animationLoopCount = 0;
    animationStatus.textContent = animationLoopLimit ? `up to ${animationLoopLimit} ${animationLoopLimit === 1 ? "loop" : "loops"}` : "looping";
    if (!animationPaused) startAnimationClock();
  });
  animationTools.querySelector("[data-viz-export]").addEventListener("click", async (event) => {
    const exportButton = event.currentTarget;
    exportButton.disabled = true;
    animationStatus.textContent = "exporting…";
    try {
      const result = await exportAnimatedVideo(svg, animationDuration, heading.textContent, "webm", animationLoopLimit || 1, animationQuality.value);
      const qualitySuffix = result.scale > 1 ? " · " + result.scale + "×" : "";
      animationStatus.textContent = result.frameCount + " frames · " + result.fps + " fps · downloaded" + qualitySuffix;
    } catch (error) {
      animationStatus.textContent = error.message ?? String(error);
    } finally {
      exportButton.disabled = false;
    }
  });
  mp4Button.addEventListener("click", async () => {
    mp4Button.disabled = true;
    animationStatus.textContent = "encoding MP4…";
    try {
      const result = await exportAnimatedVideo(svg, animationDuration, heading.textContent, "mp4", animationLoopLimit || 1, animationQuality.value);
      const qualitySuffix = result.scale > 1 ? " · " + result.scale + "×" : "";
      animationStatus.textContent = result.frameCount + " frames · " + result.fps + " fps · downloaded" + qualitySuffix;
    } catch (error) {
      animationStatus.textContent = error.message ?? String(error);
    } finally {
      mp4Button.disabled = !mp4Supported;
    }
  });
  animationTools.querySelector("[data-viz-gif-export]").addEventListener("click", async (event) => {
    const exportButton = event.currentTarget;
    exportButton.disabled = true;
    animationStatus.textContent = "encoding GIF…";
    try {
      const result = await exportAnimatedGif(svg, animationDuration, heading.textContent, animationLoopLimit || 1, animationQuality.value);
      const qualitySuffix = result.scale > 1 ? " · " + result.scale + "×" : "";
      animationStatus.textContent = result.frameCount + " frames · " + result.fps + " fps · downloaded" + qualitySuffix;
    } catch (error) {
      animationStatus.textContent = error.message ?? String(error);
    } finally {
      exportButton.disabled = false;
    }
  });
  animationSlider.addEventListener("input", () => setAnimationTime(animationSlider.value));
  animationSpeedInput.addEventListener("input", () => setAnimationSpeed(animationSpeedInput.value));
  crosshairToggle.addEventListener("click", () => setCrosshairEnabled(!crosshairEnabled));
  tableFilter.addEventListener("input", applyTableState);
  tableSort.addEventListener("change", () => {
    tableSortColumn = Number(tableSort.value);
    applyTableState();
  });
  tableDirection.addEventListener("click", () => {
    tableAscending = !tableAscending;
    tableDirection.textContent = tableAscending ? "Ascending" : "Descending";
    applyTableState();
  });
  cameraAzimuth.addEventListener("input", () => { syncShareState("replace"); scheduleCameraRender(); });
  cameraElevation.addEventListener("input", () => { syncShareState("replace"); scheduleCameraRender(); });
  cameraTools.querySelector("[data-viz-camera-reset]").addEventListener("click", () => {
    const view = cameraSource.match(/\.view\s*\(\s*(-?(?:\d+\.?\d*|\.\d+))\s*,\s*(-?(?:\d+\.?\d*|\.\d+))\s*\)/);
    cameraAzimuth.value = view ? String(Math.round(Number(view[1]))) : "-55";
    cameraElevation.value = view ? String(Math.round(Number(view[2]))) : "28";
    syncShareState("replace");
    scheduleCameraRender();
  });
  frame.addEventListener("load", () => {
    captureAnimationFrames();
    prepareAnimationDocument();
    setAnimationSpeed(animationSpeedInput.value);
    prepareLegend();
    prepareCrosshair();
    prepareTable();
    prepareEventContracts();
    prepareLinkedSelection();
    applyReducedMotion();
    if (!animationPaused) startAnimationClock();
  });
  node.addEventListener("close", () => {
    stopAnimationClock();
    clearTimeout(cameraTimer);
    cameraGeneration += 1;
    cameraQueued = false;
  });
  document.body.append(node);
  dialog = {
    open(title, text, camera = null) {
      heading.textContent = title;
      svg = text;
      zoom = 100;
      cameraSource = camera?.source ?? "";
      cameraEnabled = Boolean(camera?.camera);
      shareState = camera?.id ? { id: camera.id, initial: camera.initialState ?? null } : null;
      prepareSourceParameters(camera?.controls ?? [], camera?.initialState?.parameters ?? {});
      onCameraSvg = camera?.onSvg ?? null;
      cameraTools.hidden = !cameraEnabled;
      clearTimeout(cameraTimer);
      cameraQueued = false;
      const view = cameraSource.match(/\.view\s*\(\s*(-?(?:\d+\.?\d*|\.\d+))\s*,\s*(-?(?:\d+\.?\d*|\.\d+))\s*\)/);
      cameraAzimuth.value = camera?.initialState?.azimuth !== null && camera?.initialState?.azimuth !== undefined
        ? String(camera.initialState.azimuth)
        : (view ? String(Math.max(-180, Math.min(180, Math.round(Number(view[1]))))) : "-55");
      cameraElevation.value = camera?.initialState?.elevation !== null && camera?.initialState?.elevation !== undefined
        ? String(camera.initialState.elevation)
        : (view ? String(Math.max(-80, Math.min(80, Math.round(Number(view[2]))))) : "28");
      cameraAzimuthLabel.textContent = `${cameraAzimuth.value}°`;
      cameraElevationLabel.textContent = `${cameraElevation.value}°`;
      cameraStatus.textContent = cameraEnabled ? "Adjust the camera; Ostrin will recompute the SVG." : "";
      tableTools.hidden = true;
      selectionTools.hidden = true;
      contractTools.hidden = true;
      contractStatus.textContent = "";
      eventContracts = [];
      contractState = new Map();
      legendTools.hidden = true;
      legendControls.replaceChildren();
      legendItems = [];
      legendStatus.textContent = "";
      setCrosshairEnabled(false);
      crosshairTools.hidden = true;
      crosshairStatus.textContent = "";
      selectionStatus.textContent = "";
      exportStatus.textContent = "";
      prepareAnimation();
      render();
      node.showModal();
      syncShareState(camera?.initialState ? "replace" : "push");
      if (camera?.initialState) scheduleCameraRender();
    },
  };
  return dialog;
}

function card(figure) {
  const image = el("img", { className: "viz-image", src: figure.svg, alt: `${figure.title}: SVG produced by the Ostrin program ${figure.source}`, loading: "lazy", decoding: "async" });
  const printed = el("pre", { className: "viz-printed", text: figure.printed.join("\n") });
  const recordedProvenance = figure.provenance && Object.keys(figure.provenance).length
    ? {
        sourceHash: figure.provenance["source-hash"] ?? "",
        dataHash: figure.provenance["data-hash"] ?? "",
        seed: figure.provenance.seed ?? "",
        compiler: figure.provenance.compiler ?? "",
      }
    : null;
  const provenance = el("p", { className: "sl-provenance", text: provenanceText(recordedProvenance, `Recorded from ${figure.source}`) });
  const run = el("button", { type: "button", className: "button", "data-viz-run": figure.id, text: "Run live" });
  const status = el("span", { className: "sl-status", text: "" });
  let liveSvg = null;
  const explore = el("button", { type: "button", className: "button-quiet", "data-viz-explore": figure.id, text: "Explore" });
  const manifest = el("button", {
    type: "button",
    className: "button-quiet",
    "aria-label": `Download ${figure.title} figure manifest`,
    "data-viz-manifest": figure.id,
    text: "Manifest",
  });
  manifest.addEventListener("click", () => {
    try {
      downloadFigureManifest(figure, liveSvg);
      status.textContent = "manifest downloaded";
    } catch (error) {
      status.textContent = error.message ?? String(error);
    }
  });
  const playground = el("a", {
    className: "button-quiet",
    href: playgroundHref(figure.code),
    "data-viz-playground": figure.id,
    "aria-label": `Open ${figure.title} in the Playground`,
    text: "Open in Playground",
  });
  const bundle = figure.bundle
    ? el("button", { type: "button", className: "button-quiet", "aria-label": "Download experiment bundle", "data-viz-bundle": figure.id, text: "Bundle" })
    : null;
  bundle?.addEventListener("click", async () => {
    bundle.disabled = true;
    status.textContent = "preparing bundle…";
    try {
      await downloadExperimentBundle(figure.bundle, figure.title);
      status.textContent = "bundle downloaded · R0 replay verified";
    } catch (error) {
      status.textContent = error.message ?? String(error);
    } finally {
      bundle.disabled = false;
    }
  });
  explore.addEventListener("click", async () => {
    const text = liveSvg ?? await fetch(figure.svg).then((response) => response.text());
    const is3d = figure.code.includes("viz.scene3d(");
    const state = readVizState();
    const explorerConfig = {
      id: figure.id,
      source: figure.code,
      camera: is3d,
      controls: figure.controls ?? [],
      initialState: state?.figureId === figure.id ? state : null,
      onSvg(nextSvg) {
        liveSvg = nextSvg;
        image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(nextSvg)}`;
        provenance.textContent = provenanceText(provenanceOf(nextSvg), "Source updated live");
        provenance.dataset.state = "live";
      },
    };
    explorer().open(figure.title, text, explorerConfig);
  });
  const code = el("details", { className: "viz-code" }, [
    el("summary", { text: `Source · ${figure.source}` }),
    el("pre", { className: "sl-code", tabindex: "0" }, [el("code", { text: figure.code })]),
  ]);
  const capabilities = el("ul", { className: "viz-capability-tags", "aria-label": "Recorded capabilities" }, (figure.capabilities ?? []).map((capability) => el("li", { text: capabilityLabel(capability) })));

  run.addEventListener("click", async () => {
    run.disabled = true;
    status.textContent = "loading compiler…";
    const started = performance.now();
    try {
      const { runOstrinc } = await runtime();
      status.textContent = "running…";
      const { code: exit, lines } = await runOstrinc({ "main.ostrin": figure.code }, ["--run", "main.ostrin"]);
      const stdout = lines.filter(([kind]) => kind === "out").map(([, text]) => text);
      const { svg, printed: text } = svgOf(stdout);
      const elapsed = Math.round(performance.now() - started);
      if (exit === 0 && svg) {
        liveSvg = svg;
        image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
        printed.textContent = text.join("\n");
        provenance.textContent = provenanceText(provenanceOf(svg), `Computed live in your browser by ostrinc.wasm ${LAB.compiler} in ${elapsed} ms`);
        provenance.dataset.state = "live";
        status.textContent = `ok · ${elapsed} ms`;
      } else {
        const errors = lines.filter(([kind]) => kind === "err").map(([, text]) => text);
        printed.textContent = errors.join("\n") || `exit ${exit}`;
        provenance.dataset.state = "error";
        status.textContent = `exit ${exit}`;
      }
    } catch (error) {
      status.textContent = `could not run: ${error.message ?? error}`;
    } finally {
      run.disabled = false;
    }
  });

  return {
    id: figure.id,
    capabilities: figure.capabilities ?? [],
    searchText: [figure.id, figure.title, figure.blurb, figure.source, ...(figure.capabilities ?? []).map(capabilityLabel)].join(" ").toLocaleLowerCase(),
    run,
    status,
    explore,
    node: el("article", { className: "viz-card", id: `viz-${figure.id}` }, [
      el("a", { className: "viz-frame", href: figure.svg, "aria-label": `Open the ${figure.title} SVG` }, [image]),
      el("div", { className: "viz-body" }, [
        el("h3", { text: figure.title }),
        el("p", { className: "muted", text: figure.blurb }),
        capabilities,
        figure.printed.length ? printed : null,
        el("div", { className: "sl-actions" }, [run, explore, manifest, playground, bundle, status, el("a", { className: "text-link", href: figure.sourceUrl, text: "View source ↗" })]),
        provenance,
        code,
      ]),
    ]),
  };
}

const VIZ_CAPABILITY_DEFINITIONS = [
  { id: "all", label: "All outputs", description: "Every recorded figure" },
  { id: "2d", label: "2D plots", description: "Lines, fields and statistical views" },
  { id: "3d", label: "3D scenes", description: "Surfaces, volumes and trajectories" },
  { id: "animation", label: "Animation", description: "Motion or frame sequences" },
  { id: "tables", label: "Tables", description: "Structured rows rendered as SVG" },
  { id: "reproducibility", label: "Provenance", description: "Source and data metadata" },
  { id: "units", label: "Units", description: "Quantity-aware source programs" },
];

function capabilityLabel(id) {
  return VIZ_CAPABILITY_DEFINITIONS.find((definition) => definition.id === id)?.label ?? id;
}

const WORKFLOW_KIND_LABELS = {
  figure: "figure",
  table: "table",
  animation: "animation · video",
  provenance: "provenance",
};

function workflowFigureHref(workflow, step) {
  return `viz.html?workflow=${encodeURIComponent(workflow.id)}&figure=${encodeURIComponent(step.figure)}#viz-${encodeURIComponent(step.figure)}`;
}

function mountWorkflows(root) {
  const galleryById = new Map((LAB?.gallery ?? []).map((figure) => [figure.id, figure]));
  const workflowCards = [];
  const selectionStatus = el("p", { className: "viz-workflow-selection", "aria-live": "polite", text: "" });

  const updateSelection = (selectedId) => {
    for (const { workflow, node, select } of workflowCards) {
      const selected = workflow.id === selectedId;
      node.dataset.active = String(selected);
      select.setAttribute("aria-pressed", String(selected));
      select.textContent = selected ? "Selected" : "Select workflow";
    }
    const selected = (LAB?.workflows ?? []).find((workflow) => workflow.id === selectedId);
    selectionStatus.textContent = selected
      ? `${selected.title} selected · shareable as ?workflow=${selected.id}`
      : "Choose a workflow to highlight its path.";
  };

  for (const workflow of LAB?.workflows ?? []) {
    const select = el("button", {
      type: "button",
      className: "button-quiet",
      "data-viz-workflow-select": workflow.id,
      "aria-pressed": "false",
      text: "Select workflow",
    });
    const stepNodes = workflow.steps.map((step, index) => {
      const figure = galleryById.get(step.figure);
      if (!figure) return null;
      const kind = WORKFLOW_KIND_LABELS[step.kind] ?? step.kind;
      const source = el("a", {
        className: "text-link",
        href: step.sourceUrl,
        "data-viz-workflow-source": step.figure,
        text: `Source · ${step.source}`,
      });
      const figureLink = el("a", {
        className: "viz-workflow-step-link",
        href: workflowFigureHref(workflow, step),
        "data-viz-workflow-step": step.figure,
        "aria-label": `Open ${step.title} in the Viz explorer`,
      }, [
        el("span", { className: "viz-workflow-step-index", text: String(index + 1).padStart(2, "0") }),
        el("span", { className: "viz-workflow-step-copy" }, [
          el("span", { className: "viz-workflow-step-kind", text: kind }),
          el("strong", { text: step.title }),
          el("span", { className: "muted", text: step.detail }),
        ]),
        el("span", { className: "viz-workflow-step-arrow", "aria-hidden": "true", text: "→" }),
      ]);
      return el("li", { className: "viz-workflow-step" }, [figureLink, source]);
    }).filter(Boolean);
    const node = el("article", { className: "viz-workflow-card", "data-viz-workflow": workflow.id, "data-active": "false" }, [
      el("div", { className: "viz-workflow-card-head" }, [
        el("div", {}, [el("span", { className: "viz-workflow-number", text: workflow.id === "explore-3d" ? "03" : workflow.id === "analyze" ? "02" : "01" }), el("span", { className: `viz-workflow-status ${workflow.status}`, text: workflow.status })]),
        select,
      ]),
      el("h3", { text: workflow.title }),
      el("p", { className: "muted", text: workflow.summary }),
      el("ol", { className: "viz-workflow-steps" }, stepNodes),
    ]);
    select.addEventListener("click", () => {
      writeWorkflowState(workflow.id, "push");
      updateSelection(workflow.id);
      node.scrollIntoView({ block: "nearest", behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
    });
    workflowCards.push({ workflow, node, select });
  }

  root.replaceChildren(...workflowCards.map(({ node }) => node), selectionStatus);
  const initial = readWorkflowState();
  updateSelection(initial?.id ?? "");
}

function mountGallery(root) {
  const cards = LAB.gallery.map(card);
  const tools = document.querySelector("[data-viz-gallery-tools]");
  const search = tools?.querySelector("#viz-gallery-search");
  const status = tools?.querySelector("[data-viz-gallery-status]");
  const capabilityPanel = document.querySelector("[data-viz-capabilities]");
  const capabilityFilters = capabilityPanel?.querySelector("[data-viz-capability-filters]");
  const capabilityStatus = capabilityPanel?.querySelector("[data-viz-capability-status]");
  let selectedCapability = readGalleryCapability();
  const empty = el("p", { className: "viz-gallery-empty dim", hidden: true, text: "No figures match this search." });
  const capabilityCounts = new Map(VIZ_CAPABILITY_DEFINITIONS.map(({ id }) => [id, id === "all"
    ? cards.length
    : cards.filter((item) => item.capabilities.includes(id)).length]));
  const capabilityButtons = VIZ_CAPABILITY_DEFINITIONS.map((definition) => {
    const button = el("button", {
      type: "button",
      className: "viz-capability-filter",
      "data-viz-capability": definition.id,
      "aria-controls": "viz-gallery",
      "aria-pressed": definition.id === "all" ? "true" : "false",
      title: definition.description,
    }, [
      el("strong", { text: String(capabilityCounts.get(definition.id)) }),
      el("span", { text: definition.label }),
    ]);
    button.addEventListener("click", () => {
      selectedCapability = definition.id;
      writeGalleryCapability(selectedCapability, "push");
      for (const candidate of capabilityButtons) candidate.setAttribute("aria-pressed", String(candidate === button));
      updateSearch();
    });
    return button;
  });
  capabilityFilters?.replaceChildren(...capabilityButtons);
  for (const button of capabilityButtons) {
    button.setAttribute("aria-pressed", String(button.dataset.vizCapability === selectedCapability));
  }
  if (capabilityPanel) capabilityPanel.hidden = false;
  const updateSearch = () => {
    const query = search?.value.trim().toLocaleLowerCase() ?? "";
    let visible = 0;
    for (const item of cards) {
      const matchesCapability = selectedCapability === "all" || item.capabilities.includes(selectedCapability);
      const matches = matchesCapability && (!query || item.searchText.includes(query));
      item.node.hidden = !matches;
      if (matches) visible += 1;
    }
    empty.hidden = visible !== 0;
    if (status) status.textContent = query || selectedCapability !== "all" ? `${visible} of ${cards.length} figures match` : `${cards.length} figures`;
    if (capabilityStatus) {
      const active = VIZ_CAPABILITY_DEFINITIONS.find((definition) => definition.id === selectedCapability);
      capabilityStatus.textContent = `${visible} ${active?.label.toLocaleLowerCase() ?? "outputs"} shown`;
    }
  };
  if (tools && search) {
    tools.hidden = false;
    search.setAttribute("aria-controls", "viz-gallery");
    search.addEventListener("input", updateSearch);
  }
  root.replaceChildren(empty, ...cards.map((item) => item.node));
  updateSearch();
  const initial = readVizState();
  if (initial) {
    const target = cards.find((item) => item.id === initial.figureId);
    if (target) queueMicrotask(() => target.explore.click());
  }
}

if (LAB?.workflows) document.querySelectorAll("[data-viz-workflows]").forEach(mountWorkflows);
if (LAB?.gallery) document.querySelectorAll("[data-viz-gallery]").forEach(mountGallery);
