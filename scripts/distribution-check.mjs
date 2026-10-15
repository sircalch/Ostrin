import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];

function read(relativePath) {
  const absolutePath = path.join(root, relativePath);
  if (!existsSync(absolutePath)) {
    failures.push("missing " + relativePath);
    return "";
  }
  return readFileSync(absolutePath, "utf8");
}

function requireText(text, marker, label) {
  if (!text.includes(marker)) failures.push(label + ": missing " + marker);
}

const release = read(".github/workflows/release.yml");
const wasi = read(".github/workflows/wasi.yml");
const compiler = read("compiler/src/main.rs");
for (const target of [
  "x86_64-unknown-linux-gnu",
  "aarch64-apple-darwin",
  "x86_64-pc-windows-msvc",
]) {
  requireText(release, target, "release workflow");
}
for (const marker of ["sha256sum", "shasum", "ARCHIVE=", "gh release create", "--notes-file", "--verify-tag"]) {
  requireText(release, marker, "release workflow");
}
for (const marker of ["safe_ref=", 'package_ref="v${version}-rc.${GITHUB_SHA:0:7}"', "matrix.target", "archive=\"$name.tar.gz\"", "archive=\"$name.zip\""]) {
  requireText(release, marker, "release archive naming");
}
for (const marker of [
  "scripts/wasi-program-check.mjs",
  "all 28 entries from scripts/wasi-program-check.mjs",
  "sha256sum ./*.wasm > SHA256SUMS",
  "tar -czf ../ostrinc-wasm32-wasip1.tar.gz ./*.wasm SHA256SUMS README.txt",
]) {
  requireText(wasi, marker, "WASI program matrix");
}
requireText(compiler, 'command.arg("-lsetjmp")', "WASI setjmp link support");
requireText(release, "scripts/v0.2-readiness-check.mjs", "v0.2 release gate");
requireText(release, "if: startsWith(github.ref, 'refs/tags/v0.2.')", "v0.2 release tag guard");
read("docs/release-readiness-v0.2.json");
read("scripts/v0.2-readiness-check.mjs");

const shell = read("scripts/install.sh");
for (const marker of [
  "releases/download/",
  ".sha256",
  "sha256sum --check",
  "shasum -a 256 --check",
  "x86_64-unknown-linux-gnu",
  "aarch64-apple-darwin",
  "the repository has no published release",
]) {
  requireText(shell, marker, "Unix installer");
}

const powershell = read("scripts/install.ps1");
for (const marker of [
  "releases/download/",
  ".sha256",
  "Get-FileHash -Algorithm SHA256",
  "Expand-Archive",
  "x86_64-pc-windows-msvc",
  "the repository has no published release",
]) {
  requireText(powershell, marker, "Windows installer");
}

requireText(powershell, "$Repository -notmatch", "Windows installer repository validation");

const cargo = read("compiler/Cargo.toml");
const version = (cargo.match(/^version\s*=\s*"([^"]+)"/m) || [])[1];
if (!version) failures.push("compiler/Cargo.toml: missing package version");

const changelog = read("CHANGELOG.md");
const latestPublishedVersion = [...changelog.matchAll(/^## (\d+\.\d+\.\d+) .*\((\d{4}-\d{2}-\d{2})\)\s*$/gm)]
  .map(([, releaseVersion, date]) => ({ version: releaseVersion, date }))
  .filter((release) => existsSync(path.join(root, "docs", "releases", `v${release.version}.md`)))
  .sort((a, b) => b.date.localeCompare(a.date))[0]?.version || version;
const readme = read("README.md");
for (const marker of ["scripts/install.sh", "scripts/install.ps1", "checksum", "--version " + latestPublishedVersion, "Invoke-WebRequest -UseBasicParsing"]) {
  requireText(readme, marker, "README installation documentation");
}

// The release workflow publishes docs/releases/v<version>.md as the release body.
const notesPath = "docs/releases/v" + version + ".md";
const notes = read(notesPath);
for (const marker of [
  "## Supported platforms",
  "## Install",
  "## Compatibility and known limitations",
  "## Examples",
  "## Reporting problems",
  "experimental 0.x release",
  `ostrinc-v${version}-x86_64-unknown-linux-gnu.tar.gz`,
  `ostrinc-v${version}-aarch64-apple-darwin.tar.gz`,
  `ostrinc-v${version}-x86_64-pc-windows-msvc.zip`,
  `ostrinc-v${version}-wasm32-wasip1.tar.gz`,
  "sha256",
  "ostrinc --version",
]) {
  requireText(notes, marker, notesPath);
}
if (notes.includes("release candidate") || notes.includes("-rc.")) {
  failures.push(`${notesPath}: final release notes still describe candidate artifacts`);
}
for (const marker of [
  "build-wasi:",
  'name="ostrinc-${package_ref}-wasm32-wasip1"',
  'test "$(ls dist | wc -l)" -eq 8',
  "sha256sum --check SHA256SUMS",
]) {
  requireText(release, marker, "release WASI asset");
}
if (/^\s*push:\s*$/m.test(wasi) && /tags:/m.test(wasi)) {
  failures.push("WASI workflow still publishes a separate tag artifact instead of the release asset");
}
requireText(changelog, "## " + version + " ", "CHANGELOG.md");

if (failures.length) {
  console.error(failures.map((failure) => "distribution-check: " + failure).join("\n"));
  process.exitCode = 1;
} else {
  console.log("distribution-check: ok (release matrix, checksums and installers)");
}
