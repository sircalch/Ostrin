// Structural check for generated Viz SVG resources: every url(#id) and href="#id" must resolve to
// a definition in the same file, and no id may be defined twice within one file.
// Regex-based on the Ostrin SVG subset (no XML parser is available to Node without a dependency).
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..");

export function checkSvgReferences(text, label) {
  const errors = [];
  const defined = new Map();
  for (const m of text.matchAll(/\sid="([^"]+)"/g)) {
    defined.set(m[1], (defined.get(m[1]) ?? 0) + 1);
  }
  for (const [id, count] of defined) {
    if (count > 1) errors.push(`${label}: id "${id}" is defined ${count} times`);
  }
  const refs = new Set();
  for (const m of text.matchAll(/url\(#([^)\s"']+)\)/g)) refs.add(m[1]);
  for (const m of text.matchAll(/(?:href|xlink:href)="#([^"]+)"/g)) refs.add(m[1]);
  for (const ref of refs) {
    if (!defined.has(ref)) errors.push(`${label}: reference "#${ref}" has no definition`);
  }
  return errors;
}

function main() {
  const dir = join(root, "website", "assets", "viz");
  const errors = [];
  let files = 0;
  for (const name of readdirSync(dir).filter((n) => n.endsWith(".svg")).sort()) {
    files += 1;
    errors.push(...checkSvgReferences(readFileSync(join(dir, name), "utf8"), relative(root, join(dir, name)).replaceAll("\\", "/")));
  }
  if (errors.length > 0) {
    console.error(errors.join("\n"));
    process.exit(1);
  }
  console.log(`svg-references-check: ok (${files} Viz SVG assets)`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main();
