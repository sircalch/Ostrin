import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { compareLines } from "../../scripts/oracle-tokens.mjs";

// The linear-algebra examples in a real browser, through the same compiler the website ships: ostrinc.wasm loaded by
// website/ostrin-runtime.js in the page. The browser runs the compiler's interpreter route (`--run`); native code is not
// generated in the browser, so these results do not cover the native backend (see the sanitizer evidence for that).
// Nothing is mocked: every output below was printed by the WASM compiler in the page.

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const read = (path) => readFileSync(join(root, path), "utf8").replace(/\r\n/g, "\n");

async function openRuntime(page) {
  await page.goto("./notebook.html", { waitUntil: "domcontentloaded" });
  await expect(page.locator("html")).toHaveAttribute("data-notebook-ready", "true", { timeout: 30_000 });
}

// Runs `ostrinc <args>` on one source file in the page. Returns the exit code and the stdout / stderr lines.
async function runInBrowser(page, source) {
  return page.evaluate(async (text) => {
    const runtime = await import("./ostrin-runtime.js");
    const result = await runtime.runOstrinc({ "main.ostrin": text }, ["--run", "main.ostrin"]);
    return {
      code: result.code,
      out: result.lines.filter(([stream]) => stream === "out").map(([, line]) => line),
      err: result.lines.filter(([stream]) => stream === "err").map(([, line]) => line),
    };
  }, source);
}

const stdoutOf = (run) => run.out.join("\n");
const stderrOf = (run) => run.err.join("\n");

test.describe("std.linalg in the browser (WASM compiler, interpreter route)", () => {
  test.setTimeout(300_000);

  test.beforeEach(async ({ page }) => {
    await openRuntime(page);
  });

  test("the oracle program agrees with the exact-rational references", async ({ page }) => {
    const run = await runInBrowser(page, read("examples/linalg_oracle.ostrin"));
    expect(run.code, stderrOf(run)).toBe(0);
    const check = compareLines(stdoutOf(run) + "\n", read("tests/oracle/linalg-expected.txt"));
    expect(check.ok, check.failure).toBe(true);
    expect(check.lines).toBe(61);
  });

  test("the property program agrees with its references", async ({ page }) => {
    const run = await runInBrowser(page, read("examples/linalg_properties.ostrin"));
    expect(run.code, stderrOf(run)).toBe(0);
    const check = compareLines(stdoutOf(run) + "\n", read("tests/oracle/linalg-properties-expected.txt"));
    expect(check.ok, check.failure).toBe(true);
    expect(check.lines).toBe(44);
  });

  test("the quadratic fit recovers 1, 2, 0.5 and reports a status", async ({ page }) => {
    const run = await runInBrowser(page, read("examples/linalg_regression.ostrin"));
    expect(run.code, stderrOf(run)).toBe(0);
    expect(stdoutOf(run)).toBe("c0 1\nc1 2\nc2 0.5\nstatus ok\nresidual below 1e-9: true");
  });

  test("unit_dot and unit_matvec keep their units; a length mismatch is an Err, not a value", async ({ page }) => {
    const run = await runInBrowser(page, read("examples/linalg_units.ostrin"));
    expect(run.code, stderrOf(run)).toBe(0);
    expect(stdoutOf(run)).toBe("work: 10 J\nforce:\n[2, 2] N\nerror: unit_dot needs vectors of the same length");
  });

  test("LU solve gives the exact solution of a 2 by 2 system (within rounding)", async ({ page }) => {
    const source = `import std.numeric

fn main() -> Void {
    d = array([[4.0, 1.0], [2.0, 3.0]])
    b = array([1.0, 2.0])
    x = numeric.lu(d, 0.0).unwrap().solve(b)
    print(x[0].to_string())
    print(x[1].to_string())
}
`;
    const run = await runInBrowser(page, source);
    expect(run.code, stderrOf(run)).toBe(0);
    const [x0, x1] = run.out.map(Number);
    expect(Math.abs(x0 - 0.1)).toBeLessThanOrEqual(1e-14);
    expect(Math.abs(x1 - 0.6)).toBeLessThanOrEqual(1e-14);
  });

  test("solve_checked returns a solution with status ok for a well-conditioned system", async ({ page }) => {
    const source = `import std.linalg

fn main() -> Void {
    a = array([[4.0, 1.0], [2.0, 3.0]])
    b = array([1.0, 2.0])
    match linalg.solve_checked(a, b) {
        Ok(s) => print("status " + s.status + " x0 " + s.x[0].to_string() + " x1 " + s.x[1].to_string()),
        Err(message) => print("error: " + message),
    }
}
`;
    const run = await runInBrowser(page, source);
    expect(run.code, stderrOf(run)).toBe(0);
    const match = stdoutOf(run).match(/^status (\S+) x0 (\S+) x1 (\S+)$/);
    expect(match, stdoutOf(run)).not.toBeNull();
    expect(match[1]).toBe("ok");
    expect(Math.abs(Number(match[2]) - 0.1)).toBeLessThanOrEqual(1e-14);
    expect(Math.abs(Number(match[3]) - 0.6)).toBeLessThanOrEqual(1e-14);
  });

  test("solve_checked reports a nearly singular matrix as ill_conditioned with a large condition estimate", async ({ page }) => {
    const source = `import std.linalg

fn main() -> Void {
    a = array([[1.0, 1.0], [1.0, 1.00000000000001]])
    b = array([2.0, 2.0])
    match linalg.solve_checked(a, b) {
        Ok(s) => print("status " + s.status + " condition " + s.condition.to_string()),
        Err(message) => print("error: " + message),
    }
}
`;
    const run = await runInBrowser(page, source);
    expect(run.code, stderrOf(run)).toBe(0);
    const match = stdoutOf(run).match(/^status (\S+) condition (\S+)$/);
    expect(match, stdoutOf(run)).not.toBeNull();
    expect(match[1]).toBe("ill_conditioned");
    expect(Number(match[2])).toBeGreaterThan(1e12);
  });

  test("solve_checked refuses singular, non-finite, non-square and mismatched inputs with an Err and no solution", async ({ page }) => {
    const cases = [
      { name: "singular", a: "[[1.0, 2.0], [2.0, 4.0]]", b: "[1.0, 2.0]", message: "solve_checked: the matrix is singular" },
      { name: "non-finite right-hand side", a: "[[1.0, 0.0], [0.0, 1.0]]", b: "[0.0 / 0.0, 2.0]", message: "solve_checked: the right-hand side has a non-finite entry" },
      { name: "non-square", a: "[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]", b: "[1.0, 2.0]", message: "solve_checked needs a square matrix" },
      { name: "right-hand side length", a: "[[1.0, 0.0], [0.0, 1.0]]", b: "[1.0, 2.0, 3.0]", message: "solve_checked: the right-hand side needs one entry per row of the matrix" },
    ];
    for (const c of cases) {
      const source = `import std.linalg

fn main() -> Void {
    a = array(${c.a})
    b = array(${c.b})
    match linalg.solve_checked(a, b) {
        Ok(s) => print("unexpected status " + s.status),
        Err(message) => print("error: " + message),
    }
}
`;
      const run = await runInBrowser(page, source);
      expect(run.code, `${c.name}: ${stderrOf(run)}`).toBe(0);
      expect(stdoutOf(run), c.name).toBe(`error: ${c.message}`);
    }
  });

  test("unit_matvec refuses a vector of the wrong length with an Err", async ({ page }) => {
    const source = `import std.linalg

fn main() -> Void {
    stiffness = array([2.0, 0.0, 0.0, 4.0]) as N/m
    x = array([1.0, 0.5, 2.0]) as m
    match linalg.unit_matvec(stiffness, 2, 2, x) {
        Ok(f) => print((f as N).to_string()),
        Err(message) => print("error: " + message),
    }
}
`;
    const run = await runInBrowser(page, source);
    expect(run.code, stderrOf(run)).toBe(0);
    expect(stdoutOf(run)).toBe("error: unit_matvec: the vector needs one entry per column of the matrix");
  });

  test("adding a force to a length is a compile-time diagnostic: no program output", async ({ page }) => {
    const source = `fn main() -> Void {
    a = array([1.0, 2.0]) as N
    b = array([1.0, 2.0]) as m
    print((a + b).to_string())
}
`;
    const run = await runInBrowser(page, source);
    expect(run.code).not.toBe(0);
    expect(stderrOf(run)).toContain("OSTRIN-E1024");
    expect(run.out, "a refused program must not print anything").toEqual([]);
  });

  test("a matrix product with incompatible shapes stops with the runtime message and prints nothing", async ({ page }) => {
    const source = `fn main() -> Void {
    a = array([[1.0, 2.0], [3.0, 4.0]])
    w = array([1.0, 2.0, 3.0])
    print(a @ w)
}
`;
    const run = await runInBrowser(page, source);
    expect(run.code).not.toBe(0);
    expect(stderrOf(run) || stdoutOf(run)).toContain("matmul needs");
    expect(run.out, "no partial result may be printed").toEqual([]);
  });
});
