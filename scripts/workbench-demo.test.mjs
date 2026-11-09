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
const compiler = process.env.OSTRINC_BIN ?? path.join(root, "compiler", "target", "release", process.platform === "win32" ? "ostrinc.exe" : "ostrinc");
if (!existsSync(compiler)) throw new Error(`compiler binary not found at ${compiler}; build it with cargo build --release or set OSTRINC_BIN`);

function runProject(project, entry = project.entry) {
  const dir = mkdtempSync(path.join(os.tmpdir(), "ostrin-demo-"));
  try {
    for (const [name, text] of Object.entries(filesForRun(project))) writeFileSync(path.join(dir, name), text);
    return spawnSync(compiler, ["--run", entry], { cwd: dir, encoding: "utf8" });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function valueAfter(output, label) {
  const line = output.split("\n").find((l) => l.startsWith(label));
  assert.ok(line, `missing line "${label}" in:\n${output.slice(0, 800)}`);
  return Number(line.slice(label.length).trim());
}

test("the demo project is a valid ostrin.project v1 with four module files", () => {
  const project = demoProject();
  assert.deepEqual(validateProject(project), []);
  assert.deepEqual(Object.keys(project.files).sort(), ["analysis.ostrin", "graphics.ostrin", "main.ostrin", "model.ostrin"]);
  assert.equal(project.entry, "main.ostrin");
});

test("run through the real compiler: root, residual and the three derivatives agree within stated tolerances", () => {
  const result = runProject(demoProject());
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const out = result.stdout;

  const root = valueAfter(out, "root: ");
  // Positive root of x^3 - 2x - 5 (reference value). Compared at 1e-12, the tolerance used by the program.
  assert.ok(Math.abs(root - 2.0945514815423265) <= 1e-12, `root ${root}`);

  const residual = valueAfter(out, "residual |f(root)|: ");
  assert.ok(residual <= 1e-12, `residual ${residual}`);

  const analytic = 3 * root * root - 2;
  const symbolic = valueAfter(out, "symbolic derivative: ");
  const automatic = valueAfter(out, "autodiff derivative: ");
  const numerical = valueAfter(out, "finite-difference derivative: ");
  assert.ok(Math.abs(symbolic - analytic) <= 1e-12, `symbolic ${symbolic} vs analytic ${analytic}`);
  assert.ok(Math.abs(automatic - analytic) <= 1e-12, `autodiff ${automatic} vs analytic ${analytic}`);
  assert.ok(Math.abs(numerical - analytic) <= 1e-5, `finite difference ${numerical} vs analytic ${analytic}`);

  assert.match(out, /all checks passed/);
});

test("the figure is one standalone SVG document with the root marked", () => {
  const out = runProject(demoProject()).stdout;
  assert.equal(out.split("<svg").length - 1, 1);
  assert.equal(out.split("</svg>").length - 1, 1);
  assert.ok(out.includes("x^3 - 2x - 5 and its root"));
  assert.ok(out.includes('class="pt"'), "the root marker is a scatter point");
});

test("the expected error case is reported by the method, not hidden", () => {
  const out = runProject(demoProject()).stdout;
  assert.match(out, /expected error for \[0, 1\]: /);
});

test("an edited module changes the result the compiler reports", () => {
  const project = demoProject();
  // Changing the model's constant must break the residual check, so the test can fail.
  project.files["model.ostrin"] = project.files["model.ostrin"].replace("- 5.0", "- 6.0");
  const result = runProject(project);
  assert.notEqual(result.status, 0, "a wrong model must make the program fail its own checks");
  assert.match(result.stdout, /checks failed/);
});
