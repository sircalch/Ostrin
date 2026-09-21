import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const websiteRoot = path.join(repositoryRoot, "website");
const failures = [];

function check(condition, message) {
  if (!condition) failures.push(message);
}

function read(relativePath) {
  const absolutePath = path.join(repositoryRoot, relativePath);
  check(existsSync(absolutePath), `missing ${relativePath}`);
  return existsSync(absolutePath) ? readFileSync(absolutePath, "utf8") : "";
}

const pages = readdirSync(websiteRoot)
  .filter((name) => name.endsWith(".html"))
  .sort();
const publicPages = pages.filter((name) => name !== "404.html");

for (const page of publicPages) {
  const html = read(`website/${page}`);
  check(/<title>[^<]+<\/title>/i.test(html), `${page}: missing title`);
  check(/<link rel="canonical" href="[^"]+">/i.test(html), `${page}: missing canonical`);
  check(/property="og:title"/i.test(html), `${page}: missing og:title`);
  check(/name="twitter:card"/i.test(html), `${page}: missing twitter card`);

  for (const match of html.matchAll(/(?:href|src)="([^"]+)"/gi)) {
    const reference = match[1];
    if (/^(?:[a-z]+:|\/\/)/i.test(reference)) continue;
    const [pathAndQuery, fragment] = reference.split("#", 2);
    const localReference = pathAndQuery.split("?", 1)[0];
    const targetName = localReference === "/" ? "index.html" : (localReference || page);
    const target = path.resolve(websiteRoot, targetName);
    check(target.startsWith(`${websiteRoot}${path.sep}`), `${page}: unsafe local reference ${reference}`);
    check(existsSync(target), `${page}: missing local reference ${reference}`);
    if (fragment && existsSync(target) && target.endsWith(".html")) {
      const targetHtml = readFileSync(target, "utf8");
      check(targetHtml.includes(`id="${fragment}"`) || targetHtml.includes(`name="${fragment}"`), `${page}: missing local anchor ${reference}`);
    }
  }
}

const homepage = read("website/index.html");
check(homepage.includes('id="try-ostrin"'), "index.html: missing homepage playground anchor");
check(homepage.includes('type="module" src="playground.js"'), "index.html: missing real playground module");
check(homepage.includes('data-site-value="examples"'), "index.html: missing centralized project facts");
check(homepage.includes('id="output" class="output" role="status"'), "index.html: output is missing accessible status semantics");

const docs = read("website/docs.html");
check(docs.includes('id="learn"'), "docs.html: missing guided learning path");
for (const marker of ["examples/hello.ostrin", "examples/collections.ostrin", "examples/option_result.ostrin", "examples/statistics.ostrin", "examples/match_nested.ostrin"]) {
  check(docs.includes(marker), `docs.html: missing learning source ${marker}`);
}
for (const marker of ["language.html#quantities", "language.html#errors", "language.html#concurrency", "showcase.html#tables"]) {
  check(docs.includes(`href="${marker}"`), `docs.html: missing learning link ${marker}`);
}

const examples = read("website/examples.html");
check(examples.includes('type="module" src="playground.js"'), "examples.html: missing live example module");
for (const key of ["quantities", "standard", "records", "concurrency"]) {
  check(examples.includes(`data-live-example="${key}"`), `examples.html: missing live example ${key}`);
}

const playgroundScript = read("website/playground.js").replaceAll("\r\n", "\n");
for (const marker of ["[flag, \"--json\"]", "parseDiagnostic", "diagnostic-location"]) {
  check(playgroundScript.includes(marker), `playground.js: missing structured diagnostics marker ${marker}`);
}
for (const [key, sourceFile] of Object.entries({
  quantities: "examples/physics.ostrin",
  standard: "examples/std_library.ostrin",
  records: "examples/match_nested.ostrin",
  concurrency: "examples/concurrency.ostrin",
})) {
  const source = read(sourceFile).replaceAll("\r\n", "\n").trim();
  check(playgroundScript.includes(source), `playground.js: live example ${key} drifted from ${sourceFile}`);
}

const playground = read("website/playground.html");
for (const id of ["run", "check", "test", "format", "share", "source", "output"]) {
  check(playground.includes(`id="${id}"`), `playground.html: missing control ${id}`);
}
check(playground.includes('id="output" class="output" role="status"'), "playground.html: output is missing accessible status semantics");

const showcase = read("website/showcase.html");
for (const marker of ["examples/physics.ostrin", "examples/data_project", "examples/plot_project", "examples/autodiff_project"]) {
  check(showcase.includes(marker), `showcase.html: missing evidence link ${marker}`);
}

const community = read("website/community.html");
for (const marker of ["issues/new/choose", "CONTRIBUTING.md", "CODE_OF_CONDUCT.md", "community-labels.md"]) {
  check(community.includes(marker), `community.html: missing contribution link ${marker}`);
}

const sitemap = read("website/sitemap.xml");
for (const page of publicPages) {
  const location = page === "index.html"
    ? "https://sircalch.github.io/Ostrin/"
    : `https://sircalch.github.io/Ostrin/${page}`;
  check(sitemap.includes(`<loc>${location}</loc>`), `sitemap.xml: missing ${location}`);
}

const robots = read("website/robots.txt");
check(robots.includes("Sitemap: https://sircalch.github.io/Ostrin/sitemap.xml"), "robots.txt: missing sitemap declaration");

if (process.argv.includes("--wasm")) {
  const wasmPath = path.join(websiteRoot, "ostrinc.wasm");
  check(existsSync(wasmPath), "website/ostrinc.wasm: missing generated artifact");
  if (existsSync(wasmPath)) check(statSync(wasmPath).size > 100_000, "website/ostrinc.wasm: artifact is unexpectedly small");
}

if (failures.length) {
  console.error(failures.map((failure) => `website-check: ${failure}`).join("\n"));
  process.exitCode = 1;
} else {
  console.log(`website-check: ok (${publicPages.length} public pages${process.argv.includes("--wasm") ? ", WASM artifact" : ""})`);
}
