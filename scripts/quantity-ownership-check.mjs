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
 * Keep the scalar Quantity migration conservative until its ownership design
 * has a native/WASI/sanitizer implementation. This gate protects the evidence
 * that justifies the current fallback; it must be changed in the same PR that
 * adds the complete ownership matrix.
 */
export function validateQuantityOwnership(root = repositoryRoot) {
  const errors = [];
  const design = read("docs/design/27-ownership-de-quantity-escalar.md", root);
  const examplesTests = read("compiler/tests/examples.rs", root);
  const differential = read("compiler/tests/differential.rs", root);
  const ownership = read("compiler/src/ownership.rs", root);
  const status = read("ESTADO_Y_PLAN.md", root);
  const roadmap = read("ROADMAP.md", root);

  for (const [name, text] of [
    ["docs/design/27-ownership-de-quantity-escalar.md", design],
    ["compiler/tests/examples.rs", examplesTests],
    ["compiler/tests/differential.rs", differential],
    ["compiler/src/ownership.rs", ownership],
    ["ESTADO_Y_PLAN.md", status],
    ["ROADMAP.md", roadmap],
  ]) check(errors, Boolean(text), `${name}: missing or empty`);

  check(errors,
    /Estado: diseño técnico para revisión\.[\s\S]{0,180}No habilita todavía la bajada de\s+`?Figure\.unit_line`?/i.test(design),
    "design 27: must remain explicitly unimplemented until the ownership contract is delivered");
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
  ]) check(errors, design.includes(marker), `design 27: missing ownership contract marker ${marker}`);

  const fallbackTest = /fn\s+viz_units_keeps_scalar_quantity_plots_on_verified_fallback\s*\([\s\S]{0,5000}?report_text\.contains\("native-source:\s*examples\/viz_units\.ostrin\s+ir=0\s+hir=0\s+ast=1"\)/;
  check(errors, fallbackTest.test(examplesTests),
    "compiler/tests/examples.rs: viz_units fallback regression or AST baseline guard is missing");
  check(errors, /viz_units_keeps_scalar_quantity_plots_on_verified_fallback[\s\S]{0,1200}?skip_if_no_c_compiler/i.test(examplesTests),
    "compiler/tests/examples.rs: Quantity fallback guard must remain runnable on hosts without a C compiler");
  check(errors, /viz_units_keeps_scalar_quantity_plots_on_verified_fallback[\s\S]{0,1200}?--native-type-report/i.test(examplesTests),
    "compiler/tests/examples.rs: Quantity fallback guard must inspect the native type report");

  check(errors, /generic quantity plotting instantiations[\s\S]{0,220}remain[\s\S]{0,220}fallback/i.test(differential),
    "compiler/tests/differential.rs: generic Quantity plotting fallback boundary is missing");
  check(errors, /scalar unit-label ownership/i.test(differential),
    "compiler/tests/differential.rs: fallback boundary does not name scalar unit-label ownership");

  const quantityArm = /Ty::Quantity\(_\)[\s\S]{0,260}=>\s*false/.test(ownership);
  check(errors, quantityArm,
    "compiler/src/ownership.rs: Ty::Quantity must remain outside managed ownership until typed helpers exist");
  check(errors, !/Ty::Quantity\(_\)\s*=>\s*true/.test(ownership),
    "compiler/src/ownership.rs: Ty::Quantity was enabled without the design-27 implementation gate");

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
    console.log("quantity-ownership-check: ok (design, fallback regression and ownership classifier aligned)");
  }
}
