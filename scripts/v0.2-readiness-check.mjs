import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const readinessPath = "docs/release-readiness-v0.2.json";
export const requiredCriteria = [
  "ast-fallback", "ownership", "numerical-validation", "measurements", "reproducibility-r1", "documentation",
  "sanitizers", "benchmarks", "website-audit", "installation", "changelog", "artifacts",
];

function readJson(root, relativePath, errors) {
  try {
    return JSON.parse(readFileSync(path.join(root, relativePath), "utf8"));
  } catch (error) {
    errors.push(`cannot read ${relativePath}: ${error.message}`);
    return undefined;
  }
}

function readWebsiteStatus(root, errors) {
  const relativePath = "website/status-data.js";
  try {
    const source = readFileSync(path.join(root, relativePath), "utf8");
    const match = source.match(/globalThis\.OSTRIN_STATUS\s*=\s*Object\.freeze\((\{[\s\S]*\})\);/);
    if (!match) throw new Error("cannot parse OSTRIN_STATUS object");
    return JSON.parse(match[1]);
  } catch (error) {
    errors.push(`cannot read ${relativePath}: ${error.message}`);
    return undefined;
  }
}

export function validateV02Readiness(root = repositoryRoot, manifestPath = readinessPath) {
  const errors = [];
  let manifest;
  try {
    manifest = JSON.parse(readFileSync(path.join(root, manifestPath), "utf8"));
  } catch (error) {
    return { ready: false, errors: [`cannot read readiness manifest: ${error.message}`] };
  }

  if (manifest.release !== "0.2.0") errors.push("manifest release must be 0.2.0");

  const provenanceFiles = [
    [manifestPath, manifest.candidateProvenance],
    ["validation/release-candidate-v0.2.0.json", readJson(root, "validation/release-candidate-v0.2.0.json", errors)],
    ["validation/quality-scorecard.json", readJson(root, "validation/quality-scorecard.json", errors)],
    ["website/status-data.js", readWebsiteStatus(root, errors)],
  ];
  for (const field of ["sourceRevision", "evidenceRevision", "releaseTag", "finalReleaseSourceRevision"]) {
    const values = provenanceFiles.map(([file, value]) => [file, value?.[field]]);
    const reference = values[0][1];
    if (reference === undefined) errors.push(`${manifestPath}: candidateProvenance.${field} is missing`);
    for (const [file, value] of values.slice(1)) {
      if (value === undefined) errors.push(`${file}: ${field} is missing`);
      else if (value !== reference) errors.push(`${file}: ${field} does not match ${manifestPath}`);
    }
  }

  if (!Array.isArray(manifest.criteria)) {
    return { ready: false, errors: [...errors, "criteria must be an array"] };
  }

  const byId = new Map();
  for (const criterion of manifest.criteria) {
    if (!criterion || typeof criterion.id !== "string") {
      errors.push("every criterion must have a string id");
      continue;
    }
    if (byId.has(criterion.id)) errors.push(`duplicate criterion: ${criterion.id}`);
    byId.set(criterion.id, criterion);
  }
  for (const id of requiredCriteria) {
    const criterion = byId.get(id);
    if (!criterion) {
      errors.push(`missing required criterion: ${id}`);
      continue;
    }
    if (criterion.status !== "READY") errors.push(`${id}: status is ${criterion.status ?? "missing"}`);
    if (typeof criterion.acceptance !== "string" || criterion.acceptance.trim().length < 20) {
      errors.push(`${id}: acceptance criteria are missing or too short`);
    }
    if (!Array.isArray(criterion.evidence) || criterion.evidence.length === 0) {
      errors.push(`${id}: READY requires evidence`);
      continue;
    }
    let hasCiRunUrl = false;
    for (const item of criterion.evidence) {
      if (!item || typeof item.claim !== "string" || !item.claim.trim()) {
        errors.push(`${id}: evidence requires a claim`);
        continue;
      }
      let hasReference = false;
      if (typeof item.path === "string") {
        hasReference = true;
        const absolute = path.resolve(root, item.path);
        if (absolute !== root && !absolute.startsWith(`${root}${path.sep}`)) {
          errors.push(`${id}: evidence path escapes the repository: ${item.path}`);
        } else if (!existsSync(absolute)) {
          errors.push(`${id}: evidence file does not exist: ${item.path}`);
        }
      }
      if (typeof item.url === "string") {
        hasReference = true;
        try {
          const url = new URL(item.url);
          if (url.protocol === "https:") {
            hasCiRunUrl ||= url.hostname === "github.com"
              && /^\/sircalch\/Ostrin\/actions\/runs\/\d+\/?$/.test(url.pathname);
          }
          else errors.push(`${id}: evidence URL must use HTTPS: ${item.url}`);
        } catch {
          errors.push(`${id}: evidence URL is invalid: ${item.url}`);
        }
      }
      if (!hasReference) errors.push(`${id}: evidence requires a repository path or HTTPS URL`);
    }
    if (criterion.requires_ci_run_url && !hasCiRunUrl) errors.push(`${id}: READY requires a successful GitHub Actions run URL`);
  }
  for (const id of byId.keys()) {
    if (!requiredCriteria.includes(id)) errors.push(`unknown criterion: ${id}`);
  }

  const differentialPath = path.join(root, "compiler/tests/differential.rs");
  const differential = existsSync(differentialPath) ? readFileSync(differentialPath, "utf8") : "";
  const astLimit = differential.match(/MAX_AST_FALLBACK_FUNCTIONS:\s*usize\s*=\s*(\d+)/)?.[1];
  if (!astLimit) errors.push("cannot read the AST fallback ratchet from compiler/tests/differential.rs");
  else if (Number(astLimit) > 6) errors.push(`AST fallback ratchet ${astLimit} exceeds the v0.2 limit of 6`);

  const unresolved = requiredCriteria.filter((id) => byId.get(id)?.status !== "READY");
  if (manifest.status !== "READY") errors.push(`manifest status is ${manifest.status ?? "missing"}`);
  else if (unresolved.length > 0) errors.push(`manifest cannot be READY while criteria are open: ${unresolved.join(", ")}`);
  return { ready: errors.length === 0, errors };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = validateV02Readiness();
  if (!result.ready) {
    console.error(`v0.2-readiness-check: NOT READY (${result.errors.length} blockers)`);
    for (const error of result.errors) console.error(`- ${error}`);
    process.exitCode = 1;
  } else {
    console.log("v0.2-readiness-check: READY");
  }
}
