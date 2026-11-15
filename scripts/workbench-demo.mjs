// Builds the Workbench demonstration projects from their .ostrin sources, or checks that the committed
// project documents still match them.
//   node scripts/workbench-demo.mjs          check (exit 1 on drift)
//   node scripts/workbench-demo.mjs --write  regenerate the project documents
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createProject, serializeProject, validateProject } from "../website/workbench-project.js";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const demosDir = path.join(root, "website", "workbench-demos");

// Every demonstration project: its source folder, its document, and its display name.
export const DEMOS = Object.freeze([
  { id: "cubic-root", name: "Cubic root: x^3 - 2x - 5" },
  { id: "parameter-sweep", name: "Parameter sweep: x^3 - 2x - c for c = 1, 3, 5, 7" },
  { id: "parameter-sweep-even", name: "Parameter sweep: x^3 - 2x - c for c = 2, 4, 6, 8" },
]);

export function demoProject(id = "cubic-root") {
  const demo = DEMOS.find((d) => d.id === id);
  if (!demo) throw new Error(`unknown demo ${JSON.stringify(id)}`);
  const sourceDir = path.join(demosDir, demo.id);
  const files = {};
  for (const name of readdirSync(sourceDir).filter((n) => n.endsWith(".ostrin")).sort()) {
    files[name] = readFileSync(path.join(sourceDir, name), "utf8").replace(/\r\n/g, "\n");
  }
  const project = { ...createProject(demo.name), files, entry: "main.ostrin" };
  const problems = validateProject(project);
  if (problems.length > 0) throw new Error(`demo project ${id} is invalid: ${problems.join("; ")}`);
  return project;
}

export function documentPath(id) {
  return path.join(demosDir, `${id}.ostrin.project.json`);
}

function main() {
  const write = process.argv.includes("--write");
  let drift = false;
  for (const { id } of DEMOS) {
    const text = serializeProject(demoProject(id));
    if (write) {
      writeFileSync(documentPath(id), text);
      console.log(`workbench-demo: wrote ${path.relative(root, documentPath(id))}`);
      continue;
    }
    const committed = readFileSync(documentPath(id), "utf8").replace(/\r\n/g, "\n");
    if (committed !== text) {
      console.error(`workbench-demo: ${id} document is stale; run node scripts/workbench-demo.mjs --write`);
      drift = true;
    }
  }
  if (!write) {
    if (drift) process.exit(1);
    console.log("workbench-demo: ok (project documents match their sources)");
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) main();
