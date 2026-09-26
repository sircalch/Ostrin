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
  const end = lines.findIndex((line, index) => index >= start && line === "</svg>");
  if (start < 0 || end < 0) return { svg: null, printed: lines };
  return { svg: lines.slice(start, end + 1).join("\n"), printed: [...lines.slice(0, start), ...lines.slice(end + 1)] };
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

function videoMimeType() {
  if (!globalThis.MediaRecorder?.isTypeSupported) return "";
  return ["video/webm;codecs=vp9", "video/webm;codecs=vp8", "video/webm"].find((type) => MediaRecorder.isTypeSupported(type)) ?? "";
}

async function exportAnimatedWebm(source, duration, title, loops = 1) {
  const mime = videoMimeType();
  if (!mime || !HTMLCanvasElement.prototype.captureStream) throw new Error("WebM export needs MediaRecorder and canvas capture in this browser");
  const dimensions = svgDimensions(source);
  const document_ = new DOMParser().parseFromString(source, "image/svg+xml");
  const flipbookFrames = document_.querySelectorAll(".ostrin-frame").length;
  const fps = flipbookFrames ? Math.max(1, Math.round(flipbookFrames / duration)) : 15;
  const framesPerLoop = flipbookFrames || Math.min(180, Math.max(30, Math.ceil(duration * fps)));
  const loopCount = Math.max(1, Math.floor(Number(loops) || 1));
  const frameCount = framesPerLoop * loopCount;
  const canvas = document.createElement("canvas");
  canvas.width = dimensions.width;
  canvas.height = dimensions.height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("WebM export needs a 2D canvas context");
  const stream = canvas.captureStream(fps);
  const recorder = new MediaRecorder(stream, { mimeType: mime });
  const chunks = [];
  const finished = new Promise((resolve, reject) => {
    recorder.ondataavailable = (event) => { if (event.data.size) chunks.push(event.data); };
    recorder.onerror = () => reject(new Error("the browser stopped WebM recording"));
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
  link.download = `${(title || "ostrin-viz-animation").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "ostrin-viz-animation"}.webm`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return { frameCount, fps };
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
  const animationTools = el("div", { className: "viz-animation-tools", hidden: true }, [
    el("span", { className: "viz-animation-caption", text: "Animation" }),
    el("button", { type: "button", className: "button-quiet", text: "Play", "data-viz-play": "" }),
    el("button", { type: "button", className: "button-quiet", text: "Pause", "data-viz-pause": "" }),
    el("button", { type: "button", className: "button-quiet", text: "Restart", "data-viz-restart": "" }),
    el("button", { type: "button", className: "button-quiet", text: "Export WebM", "data-viz-export": "" }),
    animationSlider,
    animationLabel,
    el("label", { className: "viz-animation-speed-control" }, [el("span", { text: "Speed" }), animationSpeedInput, animationSpeedLabel]),
    el("label", { className: "viz-animation-loop-control" }, [el("span", { text: "Loops" }), animationLoop]),
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
  let zoom = 100;
  let svg = "";
  let cameraSource = "";
  let onCameraSvg = null;
  let cameraTimer = 0;
  let cameraGeneration = 0;
  let cameraRendering = false;
  let cameraQueued = false;
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
  const render = () => {
    zoomLabel.textContent = `${zoom}%`;
    frame.srcdoc = `<!doctype html><meta charset="utf-8"><style>html,body{margin:0;background:#fff}svg{display:block;width:${zoom}%;height:auto}</style>${svg}`;
  };
  const sourceWithCamera = (source, azimuth, elevation) => {
    const view = `.view(${azimuth.toFixed(1)}, ${elevation.toFixed(1)})`;
    if (/\.view\s*\([^)]*\)/.test(source)) return source.replace(/\.view\s*\([^)]*\)/, view);
    return source.replace(/(viz\.scene3d\("(?:\\.|[^"\\])*"\))/, `$1${view}`);
  };
  const renderCamera = async (generation) => {
    if (cameraRendering) { cameraQueued = true; return; }
    cameraRendering = true;
    const azimuth = cameraAzimuth.valueAsNumber;
    const elevation = cameraElevation.valueAsNumber;
    cameraStatus.textContent = "Rendering with Ostrin…";
    try {
      const { runOstrinc } = await runtime();
      const source = sourceWithCamera(cameraSource, azimuth, elevation);
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
      cameraStatus.textContent = `Rendered by Ostrin · azimuth ${azimuth}° · elevation ${elevation}°`;
    } catch (error) {
      cameraStatus.textContent = `Could not render: ${error.message ?? error}`;
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
    cameraStatus.textContent = "Camera changed · waiting to render…";
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
  const button = (text, label, action) => {
    const node = el("button", { type: "button", className: "button-quiet", "aria-label": label, text });
    node.addEventListener("click", action);
    return node;
  };
  const node = el("dialog", { className: "viz-dialog", "aria-label": "Figure explorer" }, [
    el("div", { className: "viz-dialog-bar" }, [
      heading,
      el("div", { className: "viz-dialog-tools" }, [
        button("−", "Zoom out", () => { zoom = Math.max(50, zoom - 25); render(); }),
        zoomLabel,
        button("+", "Zoom in", () => { zoom = Math.min(400, zoom + 50); render(); }),
        button("Close", "Close the explorer", () => node.close()),
      ]),
    ]),
    cameraTools,
    animationTools,
    tableTools,
    el("p", { className: "sl-provenance", text: "Hover a point or bar for its values (the SVG's own <title> tooltips). Zoom rescales the vector figure; scroll to pan." }),
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
      const result = await exportAnimatedWebm(svg, animationDuration, heading.textContent, animationLoopLimit || 1);
      animationStatus.textContent = `${result.frameCount} frames · ${result.fps} fps · downloaded`;
    } catch (error) {
      animationStatus.textContent = error.message ?? String(error);
    } finally {
      exportButton.disabled = false;
    }
  });
  animationSlider.addEventListener("input", () => setAnimationTime(animationSlider.value));
  animationSpeedInput.addEventListener("input", () => setAnimationSpeed(animationSpeedInput.value));
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
  cameraAzimuth.addEventListener("input", scheduleCameraRender);
  cameraElevation.addEventListener("input", scheduleCameraRender);
  cameraTools.querySelector("[data-viz-camera-reset]").addEventListener("click", () => {
    const view = cameraSource.match(/\.view\s*\(\s*(-?(?:\d+\.?\d*|\.\d+))\s*,\s*(-?(?:\d+\.?\d*|\.\d+))\s*\)/);
    cameraAzimuth.value = view ? String(Math.round(Number(view[1]))) : "-55";
    cameraElevation.value = view ? String(Math.round(Number(view[2]))) : "28";
    scheduleCameraRender();
  });
  frame.addEventListener("load", () => {
    captureAnimationFrames();
    prepareAnimationDocument();
    setAnimationSpeed(animationSpeedInput.value);
    prepareTable();
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
      onCameraSvg = camera?.onSvg ?? null;
      cameraTools.hidden = !cameraSource;
      clearTimeout(cameraTimer);
      cameraQueued = false;
      const view = cameraSource.match(/\.view\s*\(\s*(-?(?:\d+\.?\d*|\.\d+))\s*,\s*(-?(?:\d+\.?\d*|\.\d+))\s*\)/);
      cameraAzimuth.value = view ? String(Math.max(-180, Math.min(180, Math.round(Number(view[1]))))) : "-55";
      cameraElevation.value = view ? String(Math.max(-80, Math.min(80, Math.round(Number(view[2]))))) : "28";
      cameraAzimuthLabel.textContent = `${cameraAzimuth.value}°`;
      cameraElevationLabel.textContent = `${cameraElevation.value}°`;
      cameraStatus.textContent = cameraSource ? "Adjust the camera; Ostrin will recompute the SVG." : "";
      tableTools.hidden = true;
      prepareAnimation();
      render();
      node.showModal();
    },
  };
  return dialog;
}

function card(figure) {
  const image = el("img", { className: "viz-image", src: figure.svg, alt: `${figure.title}: SVG produced by the Ostrin program ${figure.source}`, loading: "lazy", decoding: "async" });
  const printed = el("pre", { className: "viz-printed", text: figure.printed.join("\n") });
  const provenance = el("p", { className: "sl-provenance", text: `Recorded by ostrinc ${LAB.compiler} from ${figure.source}.` });
  const run = el("button", { type: "button", className: "button", disabled: true, "data-viz-run": figure.id, text: "Run live" });
  const status = el("span", { className: "sl-status", text: "loading compiler…" });
  let liveSvg = null;
  const explore = el("button", { type: "button", className: "button-quiet", "data-viz-explore": figure.id, text: "Explore" });
  explore.addEventListener("click", async () => {
    const text = liveSvg ?? await fetch(figure.svg).then((response) => response.text());
    const is3d = figure.code.includes("viz.scene3d(");
    explorer().open(figure.title, text, is3d ? {
      source: figure.code,
      onSvg(nextSvg) {
        liveSvg = nextSvg;
        image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(nextSvg)}`;
        provenance.textContent = `Camera updated live in your browser by ostrinc.wasm ${LAB.compiler}.`;
        provenance.dataset.state = "live";
      },
    } : null);
  });
  const code = el("details", { className: "viz-code" }, [
    el("summary", { text: `Source · ${figure.source}` }),
    el("pre", { className: "sl-code", tabindex: "0" }, [el("code", { text: figure.code })]),
  ]);

  run.addEventListener("click", async () => {
    run.disabled = true;
    status.textContent = "running…";
    const started = performance.now();
    try {
      const { runOstrinc } = await runtime();
      const { code: exit, lines } = await runOstrinc({ "main.ostrin": figure.code }, ["--run", "main.ostrin"]);
      const stdout = lines.filter(([kind]) => kind === "out").map(([, text]) => text);
      const { svg, printed: text } = svgOf(stdout);
      const elapsed = Math.round(performance.now() - started);
      if (exit === 0 && svg) {
        liveSvg = svg;
        image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
        printed.textContent = text.join("\n");
        provenance.textContent = `Computed live in your browser by ostrinc.wasm ${LAB.compiler} in ${elapsed} ms.`;
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
    run,
    status,
    node: el("article", { className: "viz-card", id: `viz-${figure.id}` }, [
      el("a", { className: "viz-frame", href: figure.svg, "aria-label": `Open the ${figure.title} SVG` }, [image]),
      el("div", { className: "viz-body" }, [
        el("h3", { text: figure.title }),
        el("p", { className: "muted", text: figure.blurb }),
        figure.printed.length ? printed : null,
        el("div", { className: "sl-actions" }, [run, explore, status, el("a", { className: "text-link", href: figure.sourceUrl, text: "View source ↗" })]),
        provenance,
        code,
      ]),
    ]),
  };
}

function mountGallery(root) {
  const cards = LAB.gallery.map(card);
  root.replaceChildren(...cards.map((item) => item.node));
  runtime().then(({ loadCompiler }) => loadCompiler()).then(
    () => cards.forEach(({ run, status }) => { run.disabled = false; status.textContent = ""; }),
    (error) => cards.forEach(({ status }) => { status.textContent = `compiler unavailable: ${error.message ?? error}`; }),
  );
}

if (LAB?.gallery) document.querySelectorAll("[data-viz-gallery]").forEach(mountGallery);
