import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const REPOSITORY = "sircalch/Ostrin";
export const EXTENSION = ".ostrin";
export const USAGE_THRESHOLD = 2_000;
// The repository is excluded explicitly. The report never scans examples/ or counts local files.
// Exclude forks explicitly so the snapshot matches the Linguist contribution gate.
export const QUERY = `NOT is:fork extension:ostrin -repo:${REPOSITORY}`;
export const API_ENDPOINT = `search/code?q=${encodeURIComponent(QUERY)}`;
export const SEARCH_URL = `https://github.com/search?q=${encodeURIComponent(QUERY)}&type=code`;

function fail(message) {
  throw new Error(`linguist-usage-report: ${message}`);
}

function runGh(args) {
  const result = spawnSync("gh", args, { encoding: "utf8", windowsHide: true });
  if (result.error?.code === "ENOENT") {
    fail("GitHub CLI (gh) no está instalado; instala gh desde https://cli.github.com/.");
  }
  if (result.error) fail(`no se pudo ejecutar gh: ${result.error.message}`);
  if (result.status !== 0) {
    const detail = String(result.stderr ?? "").trim().replaceAll(/\s+/g, " ");
    fail(`gh ${args[0] ?? ""} falló${detail ? `: ${detail}` : "."}`);
  }
  return result.stdout;
}

export function assertGhReady({ versionOutput = "", authStatus = 0 } = {}) {
  if (!versionOutput.trim()) fail("GitHub CLI (gh) no respondió a `gh --version`.");
  if (authStatus !== 0) {
    fail("gh no está autenticado; ejecuta `gh auth login` y vuelve a intentarlo.");
  }
}

function checkGhAuth() {
  const version = spawnSync("gh", ["--version"], { encoding: "utf8", windowsHide: true });
  if (version.error?.code === "ENOENT") {
    fail("GitHub CLI (gh) no está instalado; instala gh desde https://cli.github.com/.");
  }
  if (version.error) fail(`no se pudo ejecutar gh --version: ${version.error.message}`);
  if (version.status !== 0) fail("gh --version falló; verifica la instalación de GitHub CLI.");

  const auth = spawnSync("gh", ["auth", "status"], { encoding: "utf8", windowsHide: true });
  assertGhReady({ versionOutput: version.stdout, authStatus: auth.status ?? 1 });
}

export function buildUsageReport(response, generatedAt = new Date().toISOString()) {
  if (!response || !Number.isInteger(response.total_count) || response.total_count < 0) {
    fail("la respuesta de GitHub no contiene un total_count entero válido.");
  }
  if (Array.isArray(response.items) && response.items.some((item) => item?.repository?.full_name === REPOSITORY)) {
    fail(`la búsqueda devolvió ${REPOSITORY}; se aborta para no presentar uso interno como evidencia.`);
  }

  const incompleteResults = response.incomplete_results === true;
  const indexedPublicFiles = response.total_count;
  const countReady = !incompleteResults && indexedPublicFiles >= USAGE_THRESHOLD;

  return {
    schema: "ostrin.linguist-usage/v1",
    generatedAt,
    extension: EXTENSION,
    query: QUERY,
    queryUrl: SEARCH_URL,
    indexedPublicFiles,
    incompleteResults,
    internalRepositoryExcluded: true,
    threshold: {
      requiredFiles: USAGE_THRESHOLD,
      countReady,
      status: countReady ? "threshold-reached" : "below-threshold",
    },
    distribution: {
      status: "manual-review-required",
      uniqueRepositories: null,
      uniqueUsers: null,
      note: "GitHub Linguist requiere distribución entre repositorios y usuarios; este contador no la infiere del repositorio de Ostrin.",
    },
    readyForUpstreamPullRequest: false,
    status: countReady ? "distribution-and-license-review-pending" : "public-usage-pending",
  };
}

export function renderHuman(report) {
  return [
    "Ostrin · GitHub Linguist usage report",
    `generated: ${report.generatedAt}`,
    `query: ${report.query}`,
    `indexed public ${report.extension} files: ${report.indexedPublicFiles}`,
    `required for common extensions: ${report.threshold.requiredFiles}`,
    `incomplete GitHub results: ${report.incompleteResults ? "yes" : "no"}`,
    "internal Ostrin repository counted: no",
    `distribution review: ${report.distribution.status}`,
    `upstream PR ready: ${report.readyForUpstreamPullRequest ? "yes" : "no"}`,
    `status: ${report.status}`,
    `search URL: ${report.queryUrl}`,
  ].join("\n") + "\n";
}

export function parseArgs(argv) {
  let json = false;
  let output = null;
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--json") {
      json = true;
    } else if (argument === "--output") {
      output = argv[++index];
      if (!output || output.startsWith("--")) fail("--output requiere una ruta de archivo.");
    } else if (argument === "--help" || argument === "-h") {
      return { help: true, json, output };
    } else {
      fail(`opción desconocida: ${argument}; usa --help para ver el uso.`);
    }
  }
  return { help: false, json, output };
}

export const HELP = `Uso: node scripts/linguist-usage-report.mjs [--json] [--output <archivo>]

Consulta GitHub Code Search mediante gh api search/code y no cuenta archivos del
repositorio sircalch/Ostrin. --json produce un artefacto machine-readable; --output
escribe el formato elegido en un archivo. El reporte no autoriza por sí solo un PR upstream.
`;

function collectReport() {
  checkGhAuth();
  let response;
  try {
    response = JSON.parse(runGh([
      "api",
      API_ENDPOINT,
      "--header",
      "Accept: application/vnd.github+json",
    ]));
  } catch (error) {
    if (error instanceof SyntaxError) fail("gh api devolvió una respuesta que no es JSON.");
    throw error;
  }
  return buildUsageReport(response);
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write(HELP);
    return;
  }
  const report = collectReport();
  const content = options.json ? `${JSON.stringify(report, null, 2)}\n` : renderHuman(report);
  if (options.output) {
    const outputPath = path.resolve(options.output);
    mkdirSync(path.dirname(outputPath), { recursive: true });
    writeFileSync(outputPath, content, "utf8");
  } else {
    process.stdout.write(content);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
