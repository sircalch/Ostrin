import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function read(relativePath, root = repositoryRoot) {
  const absolutePath = path.join(root, relativePath);
  return existsSync(absolutePath) ? readFileSync(absolutePath, "utf8") : "";
}

function filesUnder(relativePath, root = repositoryRoot) {
  const absolutePath = path.join(root, relativePath);
  if (!existsSync(absolutePath)) return [];
  return readdirSync(absolutePath, { withFileTypes: true }).flatMap((entry) => {
    const child = path.join(relativePath, entry.name);
    return entry.isDirectory() ? filesUnder(child, root) : [child];
  });
}

function hasYamlValue(yaml, key, value) {
  return new RegExp(`^\\s*${key}:\\s*${value}\\s*$`, "m").test(yaml);
}

const sampleCandidates = [
  "examples/physics.ostrin",
  "examples/quantity_arrays.ostrin",
  "examples/arrays.ostrin",
  "examples/numeric_methods.ostrin",
  "examples/native_concurrency.ostrin",
  "examples/concurrency.ostrin",
  "examples/native_records.ostrin",
];

export function validateLinguistPreparation(root = repositoryRoot) {
  const errors = [];
  const proposal = read("docs/linguist-language.yml", root).replaceAll("\r\n", "\n");
  const grammarText = read("vscode-ostrin/syntaxes/ostrin.tmLanguage.json", root);
  const documentation = read("docs/linguist.md", root);
  const license = read("LICENSE", root);
  let grammar;

  if (!proposal) {
    errors.push("docs/linguist-language.yml is missing");
  } else {
    const required = [
      ["type", "programming"],
      ["color", '"#5B3DF5"'],
      ["tm_scope", "source.ostrin"],
      ["ace_mode", "text"],
    ];
    for (const [key, value] of required) {
      if (!hasYamlValue(proposal, key, value)) {
        errors.push(`docs/linguist-language.yml: ${key} must be ${value}`);
      }
    }
    if (!/^\s*-\s*ostrin\s*$/m.test(proposal)) {
      errors.push("docs/linguist-language.yml: missing ostrin alias");
    }
    if (!/^\s*-\s*"?\.ostrin"?\s*$/m.test(proposal)) {
      errors.push("docs/linguist-language.yml: missing .ostrin extension");
    }
    if (!/^\s*-\s*ostrinc\s*$/m.test(proposal)) {
      errors.push("docs/linguist-language.yml: missing ostrinc interpreter");
    }
  }

  if (!grammarText) {
    errors.push("vscode-ostrin/syntaxes/ostrin.tmLanguage.json is missing");
  } else {
    try {
      grammar = JSON.parse(grammarText);
    } catch (error) {
      errors.push(`TextMate grammar is not valid JSON: ${error.message}`);
    }
    if (grammar?.scopeName !== "source.ostrin") {
      errors.push("TextMate grammar scopeName must be source.ostrin");
    }
    if (!grammar?.fileTypes?.includes("ostrin")) {
      errors.push("TextMate grammar must declare the ostrin file type");
    }
  }

  if (!documentation.includes("docs/linguist-language.yml")) {
    errors.push("docs/linguist.md must link the machine-readable proposal");
  }
  if (!/^MIT License/m.test(license)) {
    errors.push("LICENSE must be an MIT license before samples are proposed upstream");
  }

  const missingSamples = sampleCandidates.filter((sample) => !existsSync(path.join(root, sample)));
  if (missingSamples.length > 0) {
    errors.push(`missing Linguist sample candidates: ${missingSamples.join(", ")}`);
  }
  const examples = filesUnder("examples", root).filter((file) => file.endsWith(".ostrin"));

  return {
    errors,
    examples: examples.length,
    sampleCandidates: sampleCandidates.length,
    grammarScope: grammar?.scopeName ?? "",
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = validateLinguistPreparation();
  if (result.errors.length > 0) {
    for (const error of result.errors) console.error(`linguist-check: ${error}`);
    process.exitCode = 1;
  } else {
    console.log(`linguist-check: ok (proposal, TextMate grammar, ${result.sampleCandidates} sample candidates, ${result.examples} Ostrin examples)`);
  }
}
