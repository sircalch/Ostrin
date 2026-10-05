#!/usr/bin/env node

// Execute an input-file experiment bundle in an isolated WASI filesystem.
//
// R0 bundles remain the byte-for-byte publication contract. This command is a
// first executable input-driven slice: the source reads data.json, the host
// verifies the bundle hashes, and a sensitivity run proves that the declared
// parameter changes the rendered figure. It deliberately does not claim the
// seeded-randomness R1 guarantee for fixtures that have not opted in.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { experimentFixtures, normalizeFigureMetadata, outputPathFor, repositoryRoot } from "./experiment-bundle.mjs";
import { runWasm } from "./lab-data.mjs";

const schema = "ostrin.experiment/v0";

function sha256(text) {
  return `sha256:${createHash("sha256").update(Buffer.from(text, "utf8")).digest("hex")}`;
}

function bytes(text) {
  return Buffer.byteLength(text, "utf8");
}

function canonicalJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function fixtureArgument(argv) {
  const inline = argv.find((argument) => argument.startsWith("--fixture="));
  if (inline) return inline.slice("--fixture=".length);
  const index = argv.indexOf("--fixture");
  if (index >= 0 && argv[index + 1]) return argv[index + 1];
  return "";
}

function extractSvg(lines, id) {
  const start = lines.findIndex((line) => line.startsWith("<svg"));
  const end = lines.findIndex((line, index) => index >= start && line === "</svg>");
  if (start < 0 || end < 0) throw new Error(`${id}: executable replay did not print one complete SVG`);
  return `${lines.slice(start, end + 1).join("\n")}\n`;
}

function bundleErrors(bundle, id) {
  const errors = [];
  if (!bundle || typeof bundle !== "object") return [`${id}: bundle is not an object`];
  if (bundle.schema !== schema) errors.push(`${id}: expected ${schema}`);
  if (bundle.id !== id) errors.push(`${id}: bundle id does not match the requested fixture`);
  if (bundle.reproducibility?.level !== "R0") errors.push(`${id}: executable replay only accepts an explicit R0 bundle`);
  if (!bundle.files || typeof bundle.files !== "object") errors.push(`${id}: files object is missing`);
  if (!Array.isArray(bundle.manifest?.files) || bundle.manifest.files.length !== 4) errors.push(`${id}: manifest must contain four files`);
  for (const entry of bundle.manifest?.files ?? []) {
    const content = bundle.files?.[entry.path];
    if (typeof content !== "string") {
      errors.push(`${id}: ${entry.path} is missing from the bundle`);
      continue;
    }
    if (entry.bytes !== bytes(content)) errors.push(`${id}: ${entry.path} byte count differs from the manifest`);
    if (entry.sha256 !== sha256(content)) errors.push(`${id}: ${entry.path} hash differs from the manifest`);
  }
  let embedded;
  try {
    embedded = JSON.parse(bundle.files?.["provenance.json"] ?? "null");
  } catch {
    embedded = null;
  }
  if (JSON.stringify(embedded) !== JSON.stringify(bundle.provenance)) errors.push(`${id}: provenance.json is not the embedded provenance object`);
  const execution = bundle.provenance?.execution;
  if (!execution || execution.mode !== "input-file" || execution.input_file !== "data.json") {
    errors.push(`${id}: bundle does not declare an input-file executable replay contract`);
  }
  if (execution?.status !== "verified-in-ci") errors.push(`${id}: executable replay contract is not CI-verified`);
  if (!Array.isArray(execution?.consumes) || execution.consumes.length === 0) errors.push(`${id}: executable replay does not declare consumed inputs`);
  return errors;
}

function alternateInput(bundle, id) {
  let input;
  try {
    input = JSON.parse(bundle.files["data.json"]);
  } catch (error) {
    throw new Error(`${id}: data.json is invalid JSON (${error.message})`);
  }
  const consumed = bundle.provenance.execution.consumes;
  if (!consumed.includes("parameters.scale") || !Number.isFinite(input.parameters?.scale)) {
    throw new Error(`${id}: this replay slice requires numeric parameters.scale sensitivity`);
  }
  const alternate = structuredClone(input);
  const original = alternate.parameters.scale;
  alternate.parameters.scale = original >= 1 ? original - 0.25 : original + 0.25;
  if (alternate.parameters.scale === original) throw new Error(`${id}: could not construct a distinct sensitivity input`);
  return canonicalJson(alternate);
}

async function replay(id) {
  const fixture = experimentFixtures[id];
  if (!fixture) throw new Error(`unknown executable replay fixture ${JSON.stringify(id)}; available: ${Object.keys(experimentFixtures).join(", ")}`);
  const bundlePath = outputPathFor(id);
  if (!existsSync(bundlePath)) throw new Error(`${id}: missing ${path.relative(repositoryRoot, bundlePath)}`);
  const bundle = JSON.parse(readFileSync(bundlePath, "utf8"));
  const errors = bundleErrors(bundle, id);
  if (errors.length) throw new Error(errors.join("\n"));

  const source = bundle.files["source.ostrin"];
  const input = bundle.files["data.json"];
  const inputFile = bundle.provenance.execution.input_file;
  const readPattern = new RegExp(`read_file\\(\\s*"${inputFile.replace(/[.*+?^${}()|[\\]\\]/g, "\\$&")}"\\s*\\)`);
  if (!readPattern.test(source)) throw new Error(`${id}: source does not read ${inputFile}`);

  const modulePath = path.join(repositoryRoot, "website", "ostrinc.wasm");
  if (!existsSync(modulePath)) throw new Error(`${id}: website/ostrinc.wasm is missing`);
  const module = await WebAssembly.compile(readFileSync(modulePath));
  const files = { "main.ostrin": source, [inputFile]: input };
  const output = await runWasm(module, files, ["--run", "main.ostrin"]);
  const recorded = bundle.provenance.figure_metadata;
  const metadata = {
    sourceHash: recorded.source_hash,
    dataHash: recorded.data_hash,
    seed: String(recorded.seed ?? "").replace(/^seed=/, ""),
    compiler: recorded.compiler,
  };
  const rendered = normalizeFigureMetadata(extractSvg(output, id), metadata);
  if (rendered !== bundle.files["figure.svg"]) throw new Error(`${id}: input-driven replay SVG differs from the published bundle`);

  const changedInput = alternateInput(bundle, id);
  const changedOutput = await runWasm(module, { "main.ostrin": source, [inputFile]: changedInput }, ["--run", "main.ostrin"]);
  const changedSvg = normalizeFigureMetadata(extractSvg(changedOutput, id), metadata);
  if (changedSvg === rendered) throw new Error(`${id}: changing a declared input did not change the rendered figure`);

  return { id, input: inputFile, consumed: bundle.provenance.execution.consumes, baseline: "match", sensitivity: "changed-output" };
}

async function main() {
  const id = fixtureArgument(process.argv.slice(2));
  if (!id) throw new Error("usage: node scripts/experiment-replay.mjs --fixture <id>");
  const result = await replay(id);
  console.log(`experiment-replay: verified ${result.id} (${result.input}; ${result.sensitivity})`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(`experiment-replay: ${error.message}`);
    process.exitCode = 1;
  });
}

export { bundleErrors, extractSvg, replay };
