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
    /The verified baseline is\s+(\d[\d,]*)\s+AST\s+fallbacks?\s+\((\d[\d,]*)\s+HIR\/IR-generated:\s*(\d[\d,]*)\s+IR and\s+(\d[\d,]*)\s+HIR functions\)/,
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
  const ownershipMatrix = read("docs/ownership-matrix.md", root);
  const dynTraitDesign = read("docs/design/15-dyn-trait.md", root);
  const fallbackLedger = read("validation/ast-fallback-ledger.json", root);
  const reproduction = read("README-reproduce.md", root);

  for (const [name, text] of [
    ["ESTADO_Y_PLAN.md", status],
    ["ROADMAP.md", roadmap],
    ["docs/design/20-hir-y-ir.md", design],
    ["CHANGELOG.md", changelog],
    ["compiler/tests/differential.rs", differential],
    ["docs/ownership-matrix.md", ownershipMatrix],
    ["README-reproduce.md", reproduction],
  ]) check(errors, Boolean(text), `${name}: missing or empty`);

  const ownershipTypes = [
    "String",
    "Record concreto no recursivo",
    "Enum con payload gestionado",
    "List<T>",
    "Map<K,V>",
    "Set<T>",
    "Array<T>",
    "Quantity<D> con etiqueta dinámica",
    "Measurement<T>",
    "Option<T> con payload gestionado",
    "Result<T,E> con payload gestionado",
    "Closure con entorno",
    "Task<T>",
    "Channel<T>",
    "Iterator con estado gestionado",
    "Agregado anidado (`List`/wrapper/record)",
    "Record recursivo",
    "dyn Trait",
  ];
  const matrixRows = ownershipMatrix.split("\n")
    .filter((line) => line.startsWith("|") && !line.includes("---"))
    .map((line) => line.split("|").slice(1, -1).map((cell) => cell.trim()))
    .filter((cells) => cells.length === 17 && cells[0] !== "Tipo gestionado");
  const matrixHeader = ownershipMatrix.split("\n")
    .find((line) => line.startsWith("| Tipo gestionado |"))
    ?.split("|").slice(1, -1).map((cell) => cell.trim()) ?? [];
  const expectedContexts = [
    "Construcción", "Copia/alias", "Argumento", "Retorno", "Campo", "Colección",
    "Rama", "Loop", "Phi", "Error", "Captura closure", "Captura tarea",
    "Destrucción", "Nativo", "WASI",
  ];
  check(errors, matrixHeader.length === 17
    && matrixHeader[0] === "Tipo gestionado"
    && matrixHeader.slice(1, 16).every((context, index) => context === expectedContexts[index]),
  "docs/ownership-matrix.md: the 15 ownership contexts must match the required matrix columns");
  check(errors, /construction|construcción/i.test(ownershipMatrix)
    && /KNOWN LIMITATION/.test(ownershipMatrix)
    && /TESTED/.test(ownershipMatrix)
    && /SUPPORTED/.test(ownershipMatrix)
    && /NOT APPLICABLE/.test(ownershipMatrix),
  "docs/ownership-matrix.md: status legend or ownership contexts are incomplete");
  check(errors, matrixRows.length === ownershipTypes.length,
    `docs/ownership-matrix.md: expected ${ownershipTypes.length} typed ownership rows, found ${matrixRows.length}`);
  const rowNames = matrixRows.map((cells) => cells[0].replaceAll("`", ""));
  for (const type of ownershipTypes) {
    check(errors, rowNames.includes(type.replaceAll("`", "")),
      `docs/ownership-matrix.md: missing ownership row ${type}`);
  }
  for (const cells of matrixRows) {
    check(errors, cells.slice(1, 16).every((status) => ["T", "S", "A", "K"].includes(status)),
      `docs/ownership-matrix.md: ${cells[0]} must assign one valid state to each of 15 contexts`);
    check(errors, Boolean(cells[16]),
      `docs/ownership-matrix.md: ${cells[0]} must include evidence or a known boundary`);
  }
  check(errors, status.includes("docs/ownership-matrix.md") && roadmap.includes("docs/ownership-matrix.md"),
    "ESTADO_Y_PLAN.md and ROADMAP.md must link the ownership matrix");
  check(errors, status.includes("docs/design/15-dyn-trait.md")
    && /HIR\/IR/i.test(dynTraitDesign) && /ownership/i.test(dynTraitDesign)
    && /retain/.test(dynTraitDesign) && /release/.test(dynTraitDesign)
    && /live_allocations=0/.test(dynTraitDesign),
  "dyn Trait: public status and design contract must expose the pending HIR/IR ownership boundary");
  const dynamicTraitEntries = fallbackLedger.match(/"reason": "dynamic-trait-(?:dispatch|default-method)"/g) ?? [];
  check(errors, dynamicTraitEntries.length === 0,
    "validation/ast-fallback-ledger.json: dyn Trait default dispatch has migrated; stale dyn fallback entries must be removed");

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

  const quantityPlotIr = (text) => text.includes("Figure.unit_line")
    && text.includes("unit_scatter")
    && /IR\/C/.test(text)
    && /WASI/i.test(text)
    && /(?:live_allocations=0|cero\s+(?:fugas|allocations vivas))/i.test(text)
    && /paridad|parity|SVG diferencial|SVG differential/i.test(text);
  for (const [name, text] of [
    ["ESTADO_Y_PLAN.md", status],
    ["ROADMAP.md", roadmap],
    ["docs/design/20-hir-y-ir.md", design],
    ["CHANGELOG.md", changelog],
  ]) check(errors, quantityPlotIr(text),
    `${name}: must report evidence for IR Quantity visualization and ownership`);

  check(errors, /generic quantity plotting instantiations[\s\S]{0,220}now lower through IR\/C/i.test(differential),
    "compiler/tests/differential.rs: must document the tested generic Quantity plot IR migration");
  check(errors, !/generic quantity plotting instantiations[\s\S]{0,180}remain[\s\S]{0,180}fallback/i.test(differential),
    "compiler/tests/differential.rs: stale claim keeps generic Quantity plotting on fallback");

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
  check(errors, /scripts\/numeric-validation\.mjs/.test(reproduction)
    && /scripts\/experiment-replay\.mjs --all/.test(reproduction)
    && /R0/.test(reproduction) && /R1/.test(reproduction) && /R2/.test(reproduction) && /R3/.test(reproduction)
    && /OSTRIN_CAPTURE_HOST=1/.test(reproduction),
  "README-reproduce.md: reproduction commands, R0-R3 levels and CI host contract are incomplete");

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
