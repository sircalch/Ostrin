import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function read(relativePath, root = repositoryRoot) {
  const absolutePath = path.join(root, relativePath);
  return existsSync(absolutePath) ? readFileSync(absolutePath, "utf8").replaceAll("\r\n", "\n") : "";
}

function numberPattern(value) {
  const digits = String(value);
  return digits.length > 3
    ? `${digits.slice(0, -3)}[ .,:]?${digits.slice(-3)}`
    : digits;
}

function parseNumber(value) {
  return Number(value.replaceAll(",", ""));
}

function sourceBaseline(source) {
  const sourceCode = source.replace(/^\s*\/\/ ?/gm, "");
  const minimum = sourceCode.match(/native_generated\s*>=\s*(\d+)/)?.[1];
  const maximumAst = sourceCode.match(/MAX_AST_FALLBACK_FUNCTIONS:\s*usize\s*=\s*(\d+)/)?.[1];
  const detail = sourceCode.match(
    /The verified baseline is\s+(\d[\d,]*)\s+AST fallbacks\s+\((\d[\d,]*)\s+HIR\/IR-generated:\s*(\d[\d,]*)\s+IR and\s+(\d[\d,]*)\s+HIR functions\)/,
  );
  if (!minimum || !maximumAst || !detail) return null;
  const [ast, total, ir, hir] = detail.slice(1).map(parseNumber);
  return { minimum: Number(minimum), maximumAst: Number(maximumAst), total, ast, ir, hir };
}

function check(errors, condition, message) {
  if (!condition) errors.push(message);
}

/**
 * Check the claims that tie the public plan to the repository's current gates.
 * This is deliberately small and source-backed: it does not invent feature
 * counts, and it fails when a known maturity or ownership boundary is erased.
 */
export function validateProjectPlan(root = repositoryRoot) {
  const errors = [];
  const status = read("ESTADO_Y_PLAN.md", root);
  const roadmap = read("ROADMAP.md", root);
  const design = read("docs/design/20-hir-y-ir.md", root);
  const changelog = read("CHANGELOG.md", root);
  const differential = read("compiler/tests/differential.rs", root);
  const linguist = read("docs/linguist.md", root);
  const linguistUsage = read("docs/linguist-usage.md", root);
  const websiteAudit = read("docs/website-audit.md", root);
  const viz = read("website/viz.html", root);

  for (const [name, text] of [
    ["ESTADO_Y_PLAN.md", status],
    ["ROADMAP.md", roadmap],
    ["docs/design/20-hir-y-ir.md", design],
    ["CHANGELOG.md", changelog],
    ["compiler/tests/differential.rs", differential],
  ]) check(errors, Boolean(text), `${name}: missing or empty`);

  const baseline = sourceBaseline(differential);
  check(errors, baseline !== null,
    "compiler/tests/differential.rs: native ratchet baseline is not parseable");
  if (baseline) {
    check(errors, baseline.total === baseline.minimum,
      `compiler/tests/differential.rs: detailed HIR/IR total ${baseline.total} differs from ratchet ${baseline.minimum}`);
    check(errors, baseline.ast === baseline.maximumAst,
      `compiler/tests/differential.rs: detailed AST total ${baseline.ast} differs from ratchet ${baseline.maximumAst}`);
    check(errors, baseline.ir + baseline.hir === baseline.total,
      "compiler/tests/differential.rs: HIR and IR detail does not add up");

    const total = numberPattern(baseline.total);
    const ir = numberPattern(baseline.ir);
    const hir = numberPattern(baseline.hir);
    const ast = numberPattern(baseline.ast);
    check(errors, new RegExp(`${ir}\\s+funciones\\s+generadas\\s+desde\\s+IR,\\s*${hir}\\s+desde\\s+HIR\\s+y\\s*${ast}\\s+en\\s+fallback\\s+AST`, "s").test(status),
      "ESTADO_Y_PLAN.md: native IR/HIR/AST inventory drifted from the ratchet source");
    check(errors, new RegExp(`${total}\\s+funciones\\s+generadas\\s+por\\s+HIR/IR[\\s\\S]{0,120}${ir}\\s+IR\\s*\\+\\s*${hir}\\s+HIR[\\s\\S]{0,80}${ast}\\s+AST`, "s").test(roadmap),
      "ROADMAP.md: native IR/HIR/AST inventory drifted from the ratchet source");
  }

  const quantityFallback = (text) => text.includes("Figure.unit_line")
    && text.includes("unit_scatter")
    && /(?:fallback\s+HIR\/AST|HIR\/AST\s+fallback)/i.test(text)
    && /ownership/i.test(text);
  for (const [name, text] of [
    ["ESTADO_Y_PLAN.md", status],
    ["ROADMAP.md", roadmap],
    ["docs/design/20-hir-y-ir.md", design],
    ["CHANGELOG.md", changelog],
  ]) check(errors, quantityFallback(text),
    `${name}: must preserve the Quantity visualization ownership boundary`);

  check(errors, /generic quantity plotting instantiations[\s\S]{0,180}remain[\s\S]{0,180}fallback/i.test(differential),
    "compiler/tests/differential.rs: missing explicit generic Quantity plotting fallback boundary");
  check(errors, !/generic quantity plotting instantiations now lower through IR\/C/i.test(differential),
    "compiler/tests/differential.rs: stale claim says generic Quantity plotting is already IR/C");

  check(errors, /todavía no aparece como lenguaje oficial/i.test(linguist),
    "docs/linguist.md: must state that upstream Linguist recognition is not yet accepted");
  check(errors, /node scripts\/linguist-usage-report\.mjs --json/.test(linguistUsage)
    && /indexedPublicFiles=0/.test(linguistUsage)
    && /readyForUpstreamPullRequest=false/.test(linguistUsage),
    "docs/linguist-usage.md: latest public-usage snapshot and command evidence are missing");
  check(errors, /<h3>Available<\/h3>[\s\S]*<h3>Planned<\/h3>/i.test(viz),
    "website/viz.html: Viz maturity labels Available and Planned are required");
  for (const marker of ["viz.animate", "WebM", "3D", "provenance"]) {
    check(errors, viz.includes(marker), `website/viz.html: missing north-star capability marker ${marker}`);
  }
  check(errors, /R0/.test(websiteAudit) && /R1/.test(websiteAudit) && /source-backed/i.test(websiteAudit),
    "docs/website-audit.md: reproducibility maturity and source-backed evidence are missing");

  return { errors, baseline };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = validateProjectPlan();
  if (result.errors.length > 0) {
    for (const error of result.errors) console.error(`project-plan-check: ${error}`);
    process.exitCode = 1;
  } else {
    const { total, ir, hir, ast } = result.baseline;
    console.log(`project-plan-check: ok (${total} HIR/IR: ${ir} IR + ${hir} HIR, ${ast} AST fallback; maturity and evidence markers aligned)`);
  }
}
