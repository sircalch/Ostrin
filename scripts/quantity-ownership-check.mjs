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
 * Keep the scalar Quantity migration staged. Phase 1 owns scalar labels in the
 * IR/C path, while generic Quantity visualization remains behind its verified
 * fallback until the complete ownership matrix is covered.
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
    /Estado: fase 1 implementada y validada\.[\s\S]{0,180}No habilita todavía la bajada de\s+`?Figure\.unit_line`?/i.test(design),
    "design 27: phase 1 status or the generic Figure fallback boundary is missing");
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

  const fallbackTest = /fn\s+viz_units_keeps_scalar_quantity_plots_on_verified_fallback\s*\([\s\S]{0,5000}?report_text\.contains\("native-source:\s*examples\/viz_units\.ostrin\s+ir=0\s+hir=0\s+ast=1"\)/;
  check(errors, fallbackTest.test(examplesTests),
    "compiler/tests/examples.rs: viz_units fallback regression or AST baseline guard is missing");
  check(errors, /viz_units_keeps_scalar_quantity_plots_on_verified_fallback[\s\S]{0,1200}?skip_if_no_c_compiler/i.test(examplesTests),
    "compiler/tests/examples.rs: Quantity fallback guard must remain runnable on hosts without a C compiler");
  check(errors, /viz_units_keeps_scalar_quantity_plots_on_verified_fallback[\s\S]{0,1200}?--native-type-report/i.test(examplesTests),
    "compiler/tests/examples.rs: Quantity fallback guard must inspect the native type report");
  check(errors,
    /native_quantity_scalar_escape_retains_array_unit_labels[\s\S]{0,5000}?3 m\/s[\s\S]{0,5000}?--leak-check/i.test(examplesTests),
    "compiler/tests/examples.rs: scalar Quantity max escape regression is missing");

  check(errors, /generic quantity plotting instantiations[\s\S]{0,220}remain[\s\S]{0,220}fallback/i.test(differential),
    "compiler/tests/differential.rs: generic Quantity plotting fallback boundary is missing");
  check(errors, /scalar unit-label ownership/i.test(differential),
    "compiler/tests/differential.rs: fallback boundary does not name scalar unit-label ownership");

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
  check(errors, /viz_units\.ostrin[\s\S]{0,180}fallback/i.test(status)
    && /viz_units\.ostrin[\s\S]{0,180}fallback/i.test(roadmap),
  "status/roadmap: viz_units fallback evidence is missing");

  return { errors };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = validateQuantityOwnership();
  if (result.errors.length > 0) {
    for (const error of result.errors) console.error(`quantity-ownership-check: ${error}`);
    process.exitCode = 1;
  } else {
    console.log("quantity-ownership-check: ok (phase 1 scalar labels, fallback regression and ownership helpers aligned)");
  }
}
