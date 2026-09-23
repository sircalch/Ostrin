import { copyFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const directory = path.dirname(fileURLToPath(import.meta.url));
const repositoryRoot = path.resolve(directory, "../..");
const compilerManifest = path.join(repositoryRoot, "compiler", "Cargo.toml");
const targetDirectory = process.env.CARGO_TARGET_DIR
  ? path.resolve(repositoryRoot, process.env.CARGO_TARGET_DIR)
  : path.join(repositoryRoot, "compiler", "target");
const compilerArtifact = path.join(targetDirectory, "wasm32-wasip1", "release", "ostrinc.wasm");
const websiteArtifact = path.join(repositoryRoot, "website", "ostrinc.wasm");

execFileSync("cargo", [
  "build",
  "--manifest-path",
  compilerManifest,
  "--target",
  "wasm32-wasip1",
  "--release",
], { cwd: repositoryRoot, stdio: "inherit" });

copyFileSync(compilerArtifact, websiteArtifact);
console.log(`Prepared real browser compiler at ${path.relative(repositoryRoot, websiteArtifact)}`);
