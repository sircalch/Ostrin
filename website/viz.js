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

// ---- explorer: the SVG in a sandboxed frame (no scripts) where its own hover styles and
// <title> tooltips work; zoom rescales the vector figure and the frame scrolls to pan.
let dialog;
function explorer() {
  if (dialog) return dialog;
  const frame = el("iframe", { className: "viz-frame-live", sandbox: "", title: "Interactive figure" });
  const heading = el("h2", { className: "viz-dialog-title" });
  const zoomLabel = el("output", { className: "viz-zoom", text: "100%" });
  let zoom = 100;
  let svg = "";
  const render = () => {
    zoomLabel.textContent = `${zoom}%`;
    frame.srcdoc = `<!doctype html><meta charset="utf-8"><style>html,body{margin:0;background:#fff}svg{display:block;width:${zoom}%;height:auto}</style>${svg}`;
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
    el("p", { className: "sl-provenance", text: "Hover a point or bar for its values (the SVG's own <title> tooltips). Zoom rescales the vector figure; scroll to pan." }),
    frame,
  ]);
  document.body.append(node);
  dialog = {
    open(title, text) {
      heading.textContent = title;
      svg = text;
      zoom = 100;
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
    explorer().open(figure.title, text);
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
