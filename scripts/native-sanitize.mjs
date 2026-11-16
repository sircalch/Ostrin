// Runs Ostrin native programs under AddressSanitizer, LeakSanitizer and UndefinedBehaviorSanitizer.
//
// For every program and every codegen route (IR, HIR, AST) the script:
//   1. takes the expected stdout from the interpreter (`ostrinc --run`);
//   2. emits C with `ostrinc --emit-c --leak-check` under the route's environment;
//   3. compiles that C with gcc -fsanitize=address,undefined inside WSL (no sanitizer runtime exists for the
//      MinGW toolchain on Windows, so the sanitizer build runs in the Linux distribution);
//   4. runs the binary and requires: exit 0, no sanitizer report, `live_allocations=0`, and stdout equal to step 1.
//
// Usage: node scripts/native-sanitize.mjs [--distro Ubuntu-24.04] [--report docs/sanitizer-evidence.md] [files...]
// Without files, every tests/ownership and compiler/tests/fixtures/ownership program is used.
// The script writes intermediate files to a temporary directory and never modifies the fixtures.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : fallback;
};
const distro = flag("--distro", "Ubuntu-24.04");
const reportPath = flag("--report", null);
const compiler = process.env.OSTRIN_BIN ?? join(repo, "compiler", "target", "release", "ostrinc.exe");
const positional = [];
for (let i = 0; i < args.length; i += 1) {
  if (args[i] === "--distro" || args[i] === "--report") i += 1;
  else positional.push(args[i]);
}
const fixtureDir = join(repo, "compiler", "tests", "fixtures", "ownership");
const programs = positional.length
  ? positional.map((p) => resolve(p))
  : readdirSync(fixtureDir).filter((f) => f.endsWith(".ostrin")).sort().map((f) => join(fixtureDir, f));

const ROUTES = [
  { name: "ir", env: {} },
  { name: "hir", env: { OSTRIN_NO_IR_CODEGEN: "1" } },
  { name: "ast", env: { OSTRIN_NO_IR_CODEGEN: "1", OSTRIN_NO_HIR_CODEGEN: "1" } },
];

// Every program is built at each optimisation level: at -O1 gcc removes some redundant allocations (for example a second
// free of the same pointer), so -O0 is needed to keep every instrumented access and release visible to the sanitizers.
const OPT_LEVELS = (args.includes("--opt") ? args[args.indexOf("--opt") + 1] : "-O0,-O1").split(",");
const sanitizerFlags = (opt) => ["-std=gnu11", "-g", opt, "-fno-omit-frame-pointer", "-fsanitize=address,undefined", "-fno-sanitize-recover=all"];

const work = join(tmpdir(), "ostrin-native-sanitize");
mkdirSync(work, { recursive: true });

// Windows path -> WSL path (C:\a\b -> /mnt/c/a/b).
const wslPath = (p) => p.replace(/^([A-Za-z]):/, (_, d) => `/mnt/${d.toLowerCase()}`).replace(/\\/g, "/");
const wsl = (script) => spawnSync("wsl", ["-d", distro, "--", "bash", "-c", script], { encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 600_000 });

function versionOf(cmd, a = ["--version"]) {
  const r = spawnSync(cmd, a, { encoding: "utf8" });
  return (r.stdout || r.stderr || "").split("\n")[0].trim();
}

const toolchain = {
  ostrinc: versionOf(compiler),
  gccWsl: wsl("gcc --version | head -1").stdout.trim(),
  flags: OPT_LEVELS.map((opt) => sanitizerFlags(opt).join(" ")).join(" | "),
  distro,
};

const SANITIZER_REPORT = /AddressSanitizer|LeakSanitizer|runtime error:|UndefinedBehaviorSanitizer/;

function interpret(program) {
  const r = spawnSync(compiler, ["--run", program], { encoding: "utf8", timeout: 120_000 });
  return { status: r.status, stdout: r.stdout ?? "" };
}

function checkProgram(program) {
  const name = basename(program, ".ostrin");
  const expected = interpret(program);
  const rows = [];
  for (const opt of OPT_LEVELS)
  for (const route of ROUTES) {
    const tag = `${route.name}${opt.replace("-", "")}`;
    const c = join(work, `${name}.${tag}.c`);
    const exe = join(work, `${name}.${tag}`);
    const emit = spawnSync(compiler, ["--emit-c", "--leak-check", "--out", c, program], {
      encoding: "utf8",
      env: { ...process.env, ...route.env },
      timeout: 300_000,
    });
    if (emit.status !== 0) {
      // A refusal by the compiler is not a sanitizer result: the program never reached the C compiler. Type-inference limits
      // are labelled REJECTED_TYPE; any other refusal is REJECTED_COMPILER. Neither is counted as memory-safe.
      const message = (emit.stderr || emit.stdout).trim();
      const typeLimit = /cannot determine the type|cannot infer|add type annotations|cannot determine the parameter types/.test(message);
      rows.push({ program: name, route: `${route.name} ${opt}`, verdict: typeLimit ? "REJECTED_TYPE" : "REJECTED_COMPILER", detail: `compiler refused: ${message.split("\n").slice(-2).join(" | ")}` });
      continue;
    }
    const build = wsl(`cd ${wslPath(work)} && gcc ${sanitizerFlags(opt).join(" ")} ${wslPath(c)} -o ${wslPath(exe)} -lm 2>&1`);
    if (build.status !== 0) {
      rows.push({ program: name, route: `${route.name} ${opt}`, verdict: "FAILED", detail: `generated C does not compile: ${build.stdout.trim().split("\n").slice(-3).join(" | ")}` });
      continue;
    }
    const run = wsl(
      `cd ${wslPath(work)} && ASAN_OPTIONS=detect_leaks=1:abort_on_error=0 UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1 ${wslPath(exe)} 2>err.txt; echo "__STATUS__$?"; cat err.txt`,
    );
    // The shell prints: program stdout, then "__STATUS__<exit>", then the program's stderr (sanitizer reports included).
    const m = run.stdout.match(/^([\s\S]*?)__STATUS__(\d+)\n?([\s\S]*)$/);
    if (!m) {
      rows.push({ program: name, route: `${route.name} ${opt}`, verdict: "BLOCKED", detail: "run output was not captured" });
      continue;
    }
    const nativeStdout = m[1];
    const status = Number(m[2]);
    const stderr = m[3];
    const sanitizerLines = stderr.split("\n").filter((l) => SANITIZER_REPORT.test(l));
    const leakLine = stderr.match(/live_allocations=(\d+)/);
    const sameOutput = nativeStdout.trimEnd() === expected.stdout.trimEnd();
    let verdict = "VERIFIED";
    let detail = "";
    if (status !== 0) {
      verdict = "FAILED";
      detail = `exit ${status}; ${sanitizerLines[0] ?? stderr.trim().slice(0, 200)}`;
    } else if (sanitizerLines.length) {
      verdict = "FAILED";
      detail = sanitizerLines[0];
    } else if (!leakLine || leakLine[1] !== "0") {
      verdict = "FAILED";
      detail = `leak report: ${leakLine ? "live_allocations=" + leakLine[1] : "missing"}`;
    } else if (!sameOutput) {
      verdict = "FAILED";
      detail = "native output differs from the interpreter";
    }
    rows.push({ program: name, route: `${route.name} ${opt}`, verdict, detail, liveAllocations: leakLine ? Number(leakLine[1]) : null });
  }
  return rows;
}

const rows = [];
for (const program of programs) {
  if (!existsSync(program)) throw new Error(`missing program ${program}`);
  rows.push(...checkProgram(program));
}

const failed = rows.filter((r) => r.verdict !== "VERIFIED");
const summary = { toolchain, total: rows.length, verified: rows.length - failed.length, failed: failed.length, rows };

if (reportPath) {
  const lines = [];
  lines.push("# Native sanitizer evidence");
  lines.push("");
  lines.push("Generated by `scripts/native-sanitize.mjs`. Each row is one program on one codegen route.");
  lines.push("");
  lines.push("## Toolchain");
  lines.push("");
  lines.push(`- ostrinc: \`${toolchain.ostrinc}\``);
  lines.push(`- gcc (WSL ${distro}): \`${toolchain.gccWsl}\``);
  lines.push(`- Flags: \`${toolchain.flags}\``);
  lines.push(`- Runtime options: \`ASAN_OPTIONS=detect_leaks=1:abort_on_error=0\`, \`UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1\``);
  lines.push("");
  const tally = (v) => rows.filter((r) => r.verdict === v).length;
  lines.push(`## Result: ${tally("VERIFIED")} VERIFIED, ${tally("FAILED")} FAILED (memory or output), ${tally("REJECTED_TYPE")} REJECTED_TYPE (type inference limit), ${tally("REJECTED_COMPILER")} REJECTED_COMPILER, ${tally("BLOCKED")} BLOCKED (environment)`);
  lines.push("");
  lines.push("A REJECTED row was never compiled to C, so it says nothing about memory safety. Only VERIFIED rows passed the sanitizers.");
  lines.push("");
  lines.push("| Program | Route | Verdict | live_allocations | Detail |");
  lines.push("|---|---|---|---|---|");
  for (const r of rows) {
    lines.push(`| ${r.program} | ${r.route} | ${r.verdict} | ${r.liveAllocations ?? "n/a"} | ${(r.detail || "").replace(/\|/g, "/")} |`);
  }
  lines.push("");
  writeFileSync(reportPath, lines.join("\n"), "utf8");
}

console.log(JSON.stringify(summary, null, 2));
process.exitCode = failed.length ? 1 : 0;
