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
const buttons = ["run", "check", "test", "format"].map($);

let modulePromise;
function loadModule() {
  modulePromise ??= WebAssembly.compileStreaming(fetch("ostrinc.wasm"));
  return modulePromise;
}

function escapeHtml(text) {
  return text.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
}

async function invoke(flag) {
  const lines = [];
  const files = new Map([["main.ostrin", new File(new TextEncoder().encode(source.value))]]);
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

async function execute(flag) {
  buttons.forEach((b) => (b.disabled = true));
  status.textContent = "running…";
  const started = performance.now();
  try {
    const { code, lines } = await invoke(flag);
    let html = lines.map(([kind, text]) => `<span class="${kind === "err" ? "err" : ""}">${escapeHtml(text)}</span>`).join("\n");
    if (flag === "--fmt" && code === 0) {
      // --fmt prints the formatted source: put it back in the editor.
      source.value = lines.filter(([kind]) => kind === "out").map(([, text]) => text).join("\n") + "\n";
      html = '<span class="ok">formatted</span>';
    } else if (code === 0 && !html && flag === "--check") {
      html = '<span class="ok">no errors</span>';
    }
    output.innerHTML = html || '<span class="dim">(no output)</span>';
    const elapsed = Math.round(performance.now() - started);
    status.textContent = `${code === 0 ? "ok" : "exit " + code} · ${elapsed} ms`;
  } catch (error) {
    output.innerHTML = `<span class="err">${escapeHtml(String(error))}</span>`;
    status.textContent = "failed to start";
  } finally {
    buttons.forEach((b) => (b.disabled = false));
  }
}

const select = $("example");
for (const name of Object.keys(EXAMPLES)) select.append(new Option(name, name));
select.addEventListener("change", () => {
  source.value = EXAMPLES[select.value];
  output.textContent = "";
  status.textContent = "";
});
source.value = EXAMPLES[select.value];

$("run").addEventListener("click", () => execute("--run"));
$("check").addEventListener("click", () => execute("--check"));
$("test").addEventListener("click", () => execute("--test"));
$("format").addEventListener("click", () => execute("--fmt"));
source.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
    event.preventDefault();
    execute("--run");
  } else if (event.key === "Tab") {
    event.preventDefault();
    const { selectionStart: start, selectionEnd: end } = source;
    source.setRangeText("    ", start, end, "end");
  }
});

buttons.forEach((b) => (b.disabled = true));
loadModule().then(
  () => {
    output.innerHTML = '<span class="dim">Ready. Press Run (Ctrl + Enter).</span>';
    buttons.forEach((b) => (b.disabled = false));
  },
  (error) => {
    output.innerHTML = `<span class="err">Could not load ostrinc.wasm: ${escapeHtml(String(error))}\nBuild it with: cargo build --manifest-path compiler/Cargo.toml --target wasm32-wasip1 --release</span>`;
  },
);
