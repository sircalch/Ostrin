import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const SAMPLE_MANIFEST = "docs/linguist-samples.yml";

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

function unquote(value) {
  const trimmed = value.trim();
  return trimmed.replace(/^("|')(.*)\1$/, "$2");
}

// This intentionally parses only the small, documented YAML shape used by the
// sample manifest. Keeping the parser dependency-free lets the same gate run
// in the website action and on a clean checkout of the repository.
export function parseSampleManifest(text) {
  const manifest = { schema: "", license: "", samples: [] };
  let currentSample = null;
  for (const rawLine of text.replaceAll("\r\n", "\n").split("\n")) {
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const indent = rawLine.length - rawLine.trimStart().length;
    const sampleStart = trimmed.match(/^-\s+path:\s*(.+)$/);
    if (sampleStart) {
      currentSample = { path: unquote(sampleStart[1]) };
      manifest.samples.push(currentSample);
      continue;
    }
    const field = trimmed.match(/^([A-Za-z][A-Za-z0-9_-]*):\s*(.*)$/);
    if (!field) continue;
    const [, key, value] = field;
    if (indent === 0) {
      if (key === "schema" || key === "license") manifest[key] = unquote(value);
    } else if (currentSample) {
      currentSample[key] = unquote(value);
    }
  }
  return manifest;
}

export function validateLinguistPreparation(root = repositoryRoot) {
  const errors = [];
  const proposal = read("docs/linguist-language.yml", root).replaceAll("\r\n", "\n");
  const grammarText = read("vscode-ostrin/syntaxes/ostrin.tmLanguage.json", root);
  const documentation = read("docs/linguist.md", root);
  const sampleManifestText = read(SAMPLE_MANIFEST, root);
  const license = read("LICENSE", root);
  let grammar;
  const sampleManifest = parseSampleManifest(sampleManifestText);

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
  if (!documentation.includes(SAMPLE_MANIFEST)) {
    errors.push(`docs/linguist.md must link ${SAMPLE_MANIFEST}`);
  }
  if (!/^MIT License/m.test(license)) {
    errors.push("LICENSE must be an MIT license before samples are proposed upstream");
  }

  if (!sampleManifestText) {
    errors.push(`${SAMPLE_MANIFEST} is missing`);
  } else {
    if (sampleManifest.schema !== "ostrin.linguist-samples/v1") {
      errors.push(`${SAMPLE_MANIFEST}: schema must be ostrin.linguist-samples/v1`);
    }
    if (sampleManifest.license !== "MIT") {
      errors.push(`${SAMPLE_MANIFEST}: top-level license must be MIT`);
    }
    if (sampleManifest.samples.length === 0) {
      errors.push(`${SAMPLE_MANIFEST}: at least one sample is required`);
    }
    const paths = new Set();
    for (const sample of sampleManifest.samples) {
      if (!sample.path) {
        errors.push(`${SAMPLE_MANIFEST}: every sample needs a path`);
        continue;
      }
      if (paths.has(sample.path)) {
        errors.push(`${SAMPLE_MANIFEST}: duplicate sample path ${sample.path}`);
      }
      paths.add(sample.path);
      if (sample.path.startsWith("/") || sample.path.includes("..")) {
        errors.push(`${SAMPLE_MANIFEST}: sample path must stay inside the repository: ${sample.path}`);
      }
      if (!sample.path.endsWith(".ostrin")) {
        errors.push(`${SAMPLE_MANIFEST}: sample path must end in .ostrin: ${sample.path}`);
      }
      if (!sample.role) {
        errors.push(`${SAMPLE_MANIFEST}: sample ${sample.path} needs a role`);
      }
      if (sample.license !== "MIT") {
        errors.push(`${SAMPLE_MANIFEST}: sample ${sample.path} must declare MIT licensing`);
      }
      if (!/^https:\/\/github\.com\/sircalch\/Ostrin\/blob\/main\/.+\.ostrin$/.test(sample.source ?? "")) {
        errors.push(`${SAMPLE_MANIFEST}: sample ${sample.path} needs its canonical source URL`);
      }
      if (!existsSync(path.join(root, sample.path))) {
        errors.push(`missing Linguist sample candidate: ${sample.path}`);
      }
    }
  }
  const examples = filesUnder("examples", root).filter((file) => file.endsWith(".ostrin"));

  return {
    errors,
    examples: examples.length,
    sampleCandidates: sampleManifest.samples.length,
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
