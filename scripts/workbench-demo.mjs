// Builds the Workbench demonstration project from its .ostrin sources, or checks that the committed
// project document still matches them.
//   node scripts/workbench-demo.mjs          check (exit 1 on drift)
//   node scripts/workbench-demo.mjs --write  regenerate the project document
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createProject, serializeProject, validateProject } from "../website/workbench-project.js";

const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourceDir = path.join(root, "website", "workbench-demos", "cubic-root");
const outputPath = path.join(root, "website", "workbench-demos", "cubic-root.ostrin.project.json");

export function demoProject() {
  const files = {};
  for (const name of readdirSync(sourceDir).filter((n) => n.endsWith(".ostrin")).sort()) {
    files[name] = readFileSync(path.join(sourceDir, name), "utf8").replace(/\r\n/g, "\n");
  }
  const project = { ...createProject("Cubic root: x^3 - 2x - 5"), files, entry: "main.ostrin" };
  const problems = validateProject(project);
  if (problems.length > 0) throw new Error(`demo project is invalid: ${problems.join("; ")}`);
  return project;
}

function main() {
  const text = serializeProject(demoProject());
  if (process.argv.includes("--write")) {
    writeFileSync(outputPath, text);
    console.log(`workbench-demo: wrote ${path.relative(root, outputPath)}`);
    return;
  }
  const committed = readFileSync(outputPath, "utf8").replace(/\r\n/g, "\n");
  if (committed !== text) {
    console.error("workbench-demo: the project document is stale; run node scripts/workbench-demo.mjs --write");
    process.exit(1);
  }
  console.log("workbench-demo: ok (project document matches its sources)");
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) main();
