import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function read(relativePath, root = repositoryRoot) {
  const absolutePath = path.join(root, relativePath);
  return existsSync(absolutePath) ? readFileSync(absolutePath, "utf8").replaceAll("\r\n", "\n") : "";
}

function check(errors, condition, message) {
  if (!condition) errors.push(message);
}

/**
 * Keep the scalar Quantity migration evidence-backed. The unit-aware line and
 * scatter methods now use dimension-specialized IR/C instances; remaining plot
 * families and hosted sanitizer coverage stay explicit follow-up gates.
 */
export function validateQuantityOwnership(root = repositoryRoot) {
  const errors = [];
  const design = read("docs/design/27-ownership-de-quantity-escalar.md", root);
  const examplesTests = read("compiler/tests/examples.rs", root);
  const differential = read("compiler/tests/differential.rs", root);
  const ownership = read("compiler/src/ownership.rs", root);
  const irC = read("compiler/src/ir_c.rs", root);
  const qtyRuntime = read("compiler/src/qty_runtime.c", root);
  const status = read("ESTADO_Y_PLAN.md", root);
  const roadmap = read("ROADMAP.md", root);

  for (const [name, text] of [
    ["docs/design/27-ownership-de-quantity-escalar.md", design],
    ["compiler/tests/examples.rs", examplesTests],
    ["compiler/tests/differential.rs", differential],
    ["compiler/src/ownership.rs", ownership],
    ["compiler/src/ir_c.rs", irC],
    ["compiler/src/qty_runtime.c", qtyRuntime],
    ["ESTADO_Y_PLAN.md", status],
    ["ROADMAP.md", roadmap],
  ]) check(errors, Boolean(text), `${name}: missing or empty`);

  check(errors,
    /Estado: fase 1 implementada y validada[.;][\s\S]{0,220}fase 2 habilita `Figure\.unit_line`[\s\S]{0,160}emisión WASI/i.test(design),
    "design 27: phase 1/2 status or the evidenced Figure IR boundary is missing");
  for (const marker of [
    "BorrowedQty",
    "OwnedQty",
    "StaticQty",
    "ostrin_qty_retain",
    "ostrin_qty_release",
    "ostrin_qty_copy",
    "Array<Quantity<D>>",
    "Figure.unit_line",
    "unit_scatter",
    "live_allocations=0",
    "native_quantity_scalar_escape_retains_array_unit_labels",
    "3 m/s",
  ]) check(errors, design.includes(marker), `design 27: missing ownership contract marker ${marker}`);

  const plotTestStart = examplesTests.indexOf("fn viz_units_quantity_plots_lower_to_ir_and_preserve_ownership");
  const plotTestEnd = examplesTests.indexOf("\n#[test]", plotTestStart);
  const plotIrTest = plotTestStart < 0 ? "" : examplesTests.slice(plotTestStart, plotTestEnd < 0 ? undefined : plotTestEnd);
  check(errors, plotIrTest.includes('native-source: examples/viz_units.ostrin ir=1 hir=0 ast=0')
    && plotIrTest.includes("Figure__unit_line")
    && plotIrTest.includes("Figure__unit_scatter")
    && plotIrTest.includes("live_allocations=0"),
    "compiler/tests/examples.rs: viz_units IR/WASI/native zero-leak regression is missing");
  check(errors, /viz_units_quantity_plots_lower_to_ir_and_preserve_ownership[\s\S]{0,1200}?skip_if_no_c_compiler/i.test(examplesTests),
    "compiler/tests/examples.rs: Quantity plot compiler guard must handle hosts without a C compiler");
  check(errors, /viz_units_quantity_plots_lower_to_ir_and_preserve_ownership[\s\S]{0,2200}?--target[\s\S]{0,100}?wasm32-wasi/i.test(examplesTests),
    "compiler/tests/examples.rs: Quantity plot WASI emission check is missing");
  check(errors,
    /native_quantity_scalar_escape_retains_array_unit_labels[\s\S]{0,5000}?3 m\/s[\s\S]{0,5000}?--leak-check/i.test(examplesTests),
    "compiler/tests/examples.rs: scalar Quantity max escape regression is missing");

  check(errors, /Generic quantity plotting instantiations[\s\S]{0,220}now lower through IR\/C/i.test(differential),
    "compiler/tests/differential.rs: verified generic Quantity plot IR migration is missing");

  const quantityArm = /Ty::Quantity\(_\)[\s\S]{0,80}=>\s*true/.test(ownership);
  check(errors, quantityArm,
    "compiler/src/ownership.rs: Ty::Quantity must enter managed ownership only in phase 1 with typed helpers");
  for (const [name, text, markers] of [
    ["compiler/src/qty_runtime.c", qtyRuntime, ["ostrin_qty_retain", "ostrin_qty_release", "ostrin_qty_copy", "ostrin_qty_from_borrowed", "ostrin_qty_from_owned"]],
    ["compiler/src/ir_c.rs", irC, ["Ty::Quantity(_) =>", "ostrin_qty_retain", "ostrin_qty_release", "ostrin_qty_from_borrowed"]],
  ]) {
    for (const marker of markers) check(errors, text.includes(marker),
      `${name}: phase 1 typed Quantity helper marker ${marker} is missing`);
  }

  const designLink = "docs/design/27-ownership-de-quantity-escalar.md";
  check(errors, status.includes(designLink) && roadmap.includes(designLink),
    "status/roadmap: scalar Quantity ownership design link is missing");
  check(errors, /Figure\.unit_line[\s\S]{0,220}IR\/C[\s\S]{0,220}WASI/i.test(status)
    && /Figure\.unit_line[\s\S]{0,240}IR\/C[\s\S]{0,240}WASI/i.test(roadmap),
  "status/roadmap: verified viz_units IR/WASI evidence is missing");

  return { errors };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = validateQuantityOwnership();
  if (result.errors.length > 0) {
    for (const error of result.errors) console.error(`quantity-ownership-check: ${error}`);
    process.exitCode = 1;
  } else {
  console.log("quantity-ownership-check: ok (scalar labels, dimension-specialized plot IR/WASI/leak evidence aligned)");
  }
}
