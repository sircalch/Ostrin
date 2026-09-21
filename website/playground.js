// Runs the real Ostrin compiler (ostrinc.wasm, wasm32-wasip1) in the page. The WASI layer comes from
// @bjorn3/browser_wasi_shim; the program is the only file in an in-memory preopened directory.
import { WASI, File, OpenFile, ConsoleStdout, PreopenDirectory } from "https://cdn.jsdelivr.net/npm/@bjorn3/browser_wasi_shim@0.3.0/+esm";

const EXAMPLES = {
  "Hello, quantities": `fn velocity(distance: Quantity<Length>, time: Quantity<Time>) -> Quantity<Length / Time> {
    distance / time
}

fn main() -> Void {
    speed = velocity(10 m, 2 s)
    print(speed)
}
`,
  "Standard library": `import std.math
import std.lists

fn main() -> Void {
    numbers = [5, 3, 9, 1]
    print(lists.sorted(numbers))
    print(math.max(2.5, 1.5))
    print(math.gcd(12, 18))
    print(17 % 5)
}
`,
  "Records and match": `record Point {
    x: Float
    y: Float
}

enum Shape {
    Circle(Float)
    Rect(Float, Float)
}

fn area(shape: Shape) -> Float {
    match shape {
        Circle(r) => 3.14159 * r * r,
        Rect(w, h) => w * h,
    }
}

fn main() -> Void {
    print(Point { x: 1.0, y: 2.0 })
    print(area(Circle(2.0)))
    print(area(Rect(3.0, 4.0)))
}
`,
  "Tests": `fn double(n: Int) -> Int {
    n * 2
}

fn test_double() -> Void {
    assert_eq(double(21), 42)
}

fn test_negative() -> Void {
    assert_eq(double(-3), -6)
}
`,
  "Concurrency": `fn main() -> Void {
    results = channel<Int>()
    producer = spawn {
        results.send(1)
        results.send(2)
        results.close()
    }
    for value in results {
        print(value)
    }
}
`,
};

const $ = (id) => document.getElementById(id);
const source = $("source");
const output = $("output");
const status = $("status");
const buttons = ["run", "check", "test", "format"].map($).filter(Boolean);
const shareButton = $("share");

let modulePromise;
function loadModule() {
  modulePromise ??= WebAssembly.compileStreaming(fetch("ostrinc.wasm"));
  return modulePromise;
}

function escapeHtml(text) {
  return text.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
}

async function invoke(sourceText, flag) {
  const lines = [];
  const files = new Map([["main.ostrin", new File(new TextEncoder().encode(sourceText))]]);
  const fds = [
    new OpenFile(new File([])),
    ConsoleStdout.lineBuffered((line) => lines.push(["out", line])),
    ConsoleStdout.lineBuffered((line) => lines.push(["err", line])),
    new PreopenDirectory(".", files),
  ];
  const wasi = new WASI(["ostrinc", flag, "main.ostrin"], [], fds);
  const instance = await WebAssembly.instantiate(await loadModule(), { wasi_snapshot_preview1: wasi.wasiImport });
  let code = 0;
  try {
    code = wasi.start(instance);
  } catch (error) {
    lines.push(["err", `runtime trap: ${error.message ?? error}`]);
    code = 1;
  }
  return { code, lines };
}

async function execute(flag, sourceNode = source, outputNode = output, statusNode = status, controls = buttons) {
  if (!sourceNode || !outputNode || !statusNode) return;
  controls.forEach((b) => (b.disabled = true));
  statusNode.textContent = "running…";
  const started = performance.now();
  try {
    const { code, lines } = await invoke(sourceNode.value, flag);
    let html = lines.map(([kind, text]) => `<span class="${kind === "err" ? "err" : ""}">${escapeHtml(text)}</span>`).join("\n");
    if (flag === "--fmt" && code === 0) {
      // --fmt prints the formatted source: put it back in the editor.
      sourceNode.value = lines.filter(([kind]) => kind === "out").map(([, text]) => text).join("\n") + "\n";
      html = '<span class="ok">formatted</span>';
    } else if (code === 0 && !html && flag === "--check") {
      html = '<span class="ok">no errors</span>';
    }
    outputNode.innerHTML = html || '<span class="dim">(no output)</span>';
    const elapsed = Math.round(performance.now() - started);
    statusNode.textContent = `${code === 0 ? "ok" : "exit " + code} · ${elapsed} ms`;
  } catch (error) {
    outputNode.innerHTML = `<span class="err">${escapeHtml(String(error))}</span>`;
    statusNode.textContent = "failed to start";
  } finally {
    controls.forEach((b) => (b.disabled = false));
  }
}

const select = $("example");
if (select && source) {
  for (const name of Object.keys(EXAMPLES)) select.append(new Option(name, name));
  select.addEventListener("change", () => {
    source.value = EXAMPLES[select.value];
    if (output) output.textContent = "";
    if (status) status.textContent = "";
  });
  const sharedCode = new URLSearchParams(location.search).get("code");
  if (sharedCode !== null) {
    source.value = sharedCode;
    if (output) output.textContent = "Shared source loaded. Press Run (Ctrl + Enter).";
  } else {
    source.value = EXAMPLES[select.value];
  }
}

$("run")?.addEventListener("click", () => execute("--run"));
$("check")?.addEventListener("click", () => execute("--check"));
$("test")?.addEventListener("click", () => execute("--test"));
$("format")?.addEventListener("click", () => execute("--fmt"));
shareButton?.addEventListener("click", async () => {
  if (!source) return;
  const url = new URL(location.href);
  url.search = "";
  url.searchParams.set("code", source.value);
  try {
    await navigator.clipboard.writeText(url.href);
    status.textContent = "link copied";
  } catch (_) {
    window.prompt("Copy this playground link", url.href);
  }
});
source?.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
    event.preventDefault();
    execute("--run");
  } else if (event.key === "Tab") {
    event.preventDefault();
    const { selectionStart: start, selectionEnd: end } = source;
    source.setRangeText("    ", start, end, "end");
  }
});

const liveExamples = {
  quantities: { code: `fn velocity(distance: Quantity<Length>, time: Quantity<Time>) -> Quantity<Length / Time> {
    distance / time
}

fn double<D: Dimension>(x: Quantity<D>) -> Quantity<D> {
    x * 2
}

fn main() -> Void {
    a = 5 nm
    b = 2 m
    total = a + b
    print(total)

    v = velocity(10 m, 2 s)
    print(v)

    doubled = double(3 kg)
    print(doubled)

    ratio = b / a
    print(ratio)

    mut n = 0
    for i in 1 to 5 {
        n = n + i
    }
    print(n)
}
` },
  standard: { code: `import std.math
import std.lists
import std.strings

fn main() -> Void {
    print(math.min(3, 9))
    print(math.max(2.5, 1.5))
    print(math.clamp(15, 0, 10))
    print(math.gcd(12, 18))
    print(math.lcm(4, 6))
    print(math.pow_int(2, 10))
    print(lists.contains([1, 2, 3], 2))
    print(lists.index_of(["a", "b"], "b"))
    print(lists.reversed([1, 2, 3]))
    print(lists.sorted([3, 1, 2, 5, 4]))
    print(lists.sorted(["pear", "apple", "fig"]))
    print(lists.take([1, 2, 3, 4], 2))
    print(lists.concat([1], [2, 3]))
    print(lists.range_list(2, 6))
    print(lists.max_of([4, 9, 2]))
    print(lists.min_of([4, 9, 2]))
    print(strings.repeat("ab", 3))
    print(strings.pad_left("7", 3, "0"))
    print(strings.count_of("a,b,c", ","))
}
` },
  records: { code: `enum Inner {
    Ready(Int)
    Empty
}

enum Outer {
    Wrapped(Inner)
    None
}

record Point {
    x: Int
    y: Int
}

fn inspect(value: Outer) -> Int {
    match value {
        Wrapped(Ready(value)) => value,
        _ => 0,
    }
}

fn read_x(point: Point) -> Int {
    match point {
        Point(x: x, y: _) => x,
    }
}

fn main() -> Void {
    print(inspect(Wrapped(Ready(42))))
    print(inspect(Wrapped(Empty)))
    print(read_x(Point { x: 5, y: 9 }))
}
` },
  concurrency: { code: `fn main() -> Void {
    ch = channel<Int>()

    producer = spawn {
        for i in 1 to 5 {
            ch.send(i)
        }
        ch.close()
    }

    for value in ch {
        print(value)
    }

    task = spawn {
        3 + 4
    }
    print(task.join())
}
` },
};

const liveControls = [];
for (const card of document.querySelectorAll("[data-live-example]")) {
  const definition = liveExamples[card.dataset.liveExample];
  const liveSource = card.querySelector("[data-live-source]");
  const liveOutput = card.querySelector("[data-live-output]");
  const liveStatus = card.querySelector("[data-live-status]");
  if (!definition || !liveSource || !liveOutput || !liveStatus) continue;
  const run = card.querySelector('[data-live-action="run"]');
  const check = card.querySelector('[data-live-action="check"]');
  const reset = card.querySelector('[data-live-action="reset"]');
  const copy = card.querySelector('[data-live-action="copy"]');
  liveSource.value = definition.code;
  const controls = [run, check, reset, copy].filter(Boolean);
  liveControls.push(...controls);
  run?.addEventListener("click", () => execute("--run", liveSource, liveOutput, liveStatus, controls));
  check?.addEventListener("click", () => execute("--check", liveSource, liveOutput, liveStatus, controls));
  reset?.addEventListener("click", () => {
    liveSource.value = definition.code;
    liveOutput.innerHTML = '<span class="dim">Ready. Press Run.</span>';
    liveStatus.textContent = "";
  });
  copy?.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(liveSource.value);
      liveStatus.textContent = "copied";
    } catch (_) {
      liveStatus.textContent = "select the source to copy";
    }
  });
  liveOutput.innerHTML = '<span class="dim">Ready. Press Run.</span>';
}

const allControls = [...buttons, ...liveControls];
if (allControls.length) {
  allControls.forEach((b) => (b.disabled = true));
  loadModule().then(
    () => {
      if (output) output.innerHTML = '<span class="dim">Ready. Press Run (Ctrl + Enter).</span>';
      document.querySelectorAll("[data-live-output]").forEach((node) => {
        node.innerHTML = '<span class="dim">Ready. Press Run.</span>';
      });
      allControls.forEach((b) => (b.disabled = false));
    },
    (error) => {
      const message = `Could not load ostrinc.wasm: ${escapeHtml(String(error))}`;
      if (output) output.innerHTML = `<span class="err">${message}\nBuild it with: cargo build --manifest-path compiler/Cargo.toml --target wasm32-wasip1 --release</span>`;
      document.querySelectorAll("[data-live-output]").forEach((node) => {
        node.innerHTML = `<span class="err">${message}</span>`;
      });
    },
  );
}
