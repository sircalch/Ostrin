import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { demoProject } from "./workbench-demo.mjs";
import { filesForRun, validateProject } from "../website/workbench-project.js";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const compiler =
  process.env.OSTRINC_BIN ?? path.join(root, "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc");
if (!existsSync(compiler)) throw new Error(`compiler binary not found at ${compiler}; build it with cargo build --release or set OSTRINC_BIN`);

// Runs the project with the real compiler in a fresh directory. `edits` replaces text in given files first.
function runProject(edits = {}) {
  const project = demoProject();
  for (const [name, [from, to]] of Object.entries(edits)) {
    const before = project.files[name];
    assert.ok(before.includes(from), `edit target not found in ${name}: ${from}`);
    project.files[name] = before.replace(from, to);
  }
  const dir = mkdtempSync(path.join(os.tmpdir(), "ostrin-demo-"));
  try {
    for (const [name, text] of Object.entries(filesForRun(project))) writeFileSync(path.join(dir, name), text);
    return spawnSync(compiler, ["--run", project.entry], { cwd: dir, encoding: "utf8" });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function valueAfter(output, label) {
  const line = output.split("\n").find((l) => l.startsWith(label));
  assert.ok(line, `missing line "${label}" in:\n${output.slice(0, 800)}`);
  return Number(line.slice(label.length).trim());
}

function failLines(output) {
  return output.split("\n").filter((l) => l.startsWith("FAIL "));
}

test("the demo project is a valid ostrin.project v1 with four module files", () => {
  const project = demoProject();
  assert.deepEqual(validateProject(project), []);
  assert.deepEqual(Object.keys(project.files).sort(), ["analysis.ostrin", "graphics.ostrin", "main.ostrin", "model.ostrin"]);
  assert.equal(project.entry, "main.ostrin");
});

test("consistent model: root, residual, derivatives and symbolic representation agree within stated tolerances", () => {
  const result = runProject();
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const out = result.stdout;
  assert.deepEqual(failLines(out), []);

  const root = valueAfter(out, "root: ");
  assert.ok(Math.abs(root - 2.0945514815423265) <= 1e-12, `root ${root}`);

  const residual = valueAfter(out, "residual |f(root)|: ");
  const analytic = 3 * root * root - 2;
  // Brent's x tolerance is 1e-12, so |f(root)| is bounded by |f'(root)| * 1e-12 with a safety factor 10.
  assert.ok(residual <= 10 * Math.abs(analytic) * 1e-12, `residual ${residual}`);

  const symbolic = valueAfter(out, "symbolic derivative: ");
  const automatic = valueAfter(out, "autodiff derivative: ");
  const numerical = valueAfter(out, "finite-difference derivative: ");
  assert.ok(Math.abs(symbolic - analytic) <= 1e-12, `symbolic ${symbolic} vs analytic ${analytic}`);
  assert.ok(Math.abs(automatic - analytic) <= 1e-12, `autodiff ${automatic} vs analytic ${analytic}`);
  assert.ok(Math.abs(numerical - analytic) <= 1e-5, `finite difference ${numerical} vs analytic ${analytic}`);

  assert.match(out, /all checks passed/);
});

test("consistent model: the figure is one SVG whose caption carries the same root as the printed values", () => {
  const out = runProject().stdout;
  assert.equal(out.split("<svg").length - 1, 1);
  assert.equal(out.split("</svg>").length - 1, 1);
  const root = valueAfter(out, "root: ");
  assert.ok(out.includes(`root x = ${root}`), "the figure caption uses the same root the program printed");
  assert.ok(out.includes('class="pt"'), "the root marker is a scatter point");
});

test("edited constant: the numerical model changes, the symbolic representation does not, and the discrepancy is the failure", () => {
  // Only the model's constant changes. The symbolic representation in analysis.ostrin still says -5.
  const result = runProject({ "model.ostrin": ["- 5.0", "- 6.0"] });
  assert.equal(result.status, 1, "a symbolic/model mismatch must end with a failing exit status");
  const out = result.stdout;

  const fails = failLines(out);
  assert.equal(fails.length, 5, `expected one failure per reference point, got:\n${fails.join("\n")}`);
  for (const line of fails) {
    assert.match(line, /^FAIL symbolic value differs from model\.f at x = /, line);
  }
  // The residual, root and derivative checks still pass, so the cause is the representation mismatch alone.
  assert.match(out, /checks failed: 5/);
  assert.doesNotMatch(out, /all checks passed/);

  // The printed root belongs to the edited model: f(x) = x^3 - 2x - 6 has its root near 2.18.
  const root = valueAfter(out, "root: ");
  assert.ok(Math.abs(root - 2.179981072158062) <= 1e-9, `root ${root}`);
  // The figure is built from the same model: its caption carries that root.
  assert.ok(out.includes(`root x = ${root}`), "the figure caption carries the edited model's root");
});

test("non-finite model values make the checks fail; they never pass", () => {
  // sqrt of a negative number is NaN, so every model value and the root are non-finite.
  const result = runProject({ "model.ostrin": ["x * x * x - 2.0 * x - 5.0", "sqrt(0.0 - 1.0) + x"] });
  assert.notEqual(result.status, 0);
  assert.doesNotMatch(result.stdout, /all checks passed/);
  assert.ok(failLines(result.stdout).length > 0, result.stdout);
});

test("a root reported inside [0, 1] where there is none counts as a failed check", () => {
  // f(x) = x^3 - 2x + 0.5 changes sign on [0, 1], so root_in(0, 1) returns Ok and the check must fail.
  const result = runProject({ "model.ostrin": ["- 5.0", "+ 0.5"] });
  assert.equal(result.status, 1, "an unexpected Ok must end with a failing exit status");
  assert.match(result.stdout, /FAIL unexpected root in \[0, 1\]/);
});

test("the expected error case is reported by the method, not hidden", () => {
  const out = runProject().stdout;
  assert.match(out, /expected error for \[0, 1\]: /);
});
