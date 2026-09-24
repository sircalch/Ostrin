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
for (const marker of ["safe_ref=", "matrix.target", "archive=\"$name.tar.gz\"", "archive=\"$name.zip\""]) {
  requireText(release, marker, "release archive naming");
}
for (const marker of [
  "scripts/wasi-program-check.mjs",
  "wasi_io_contract.wasm",
  "native_ir_file_io.wasm",
  "native_ir_managed_consumers.wasm",
  "native_ir_nested_wrappers.wasm",
  "native_ir_function_values.wasm",
  "sha256sum dist/ostrinc.wasm dist/hello.wasm dist/pkg_project.wasm dist/wasi_io_contract.wasm",
  "dist/native_ir_managed_consumers.wasm dist/native_ir_nested_wrappers.wasm dist/native_ir_function_values.wasm > dist/SHA256SUMS",
  "native_ir_managed_consumers.wasm native_ir_nested_wrappers.wasm native_ir_function_values.wasm SHA256SUMS README.txt",
]) {
  requireText(wasi, marker, "WASI program matrix");
}

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

const readme = read("README.md");
for (const marker of ["scripts/install.sh", "scripts/install.ps1", "checksum", "--version " + version]) {
  requireText(readme, marker, "README installation documentation");
}

// The release workflow publishes docs/releases/v<version>.md as the release body.
const notesPath = "docs/releases/v" + version + ".md";
const notes = read(notesPath);
for (const marker of [
  "## Supported platforms",
  "## Install",
  "## Known limitations",
  "## Examples",
  "## Reporting problems",
  "experimental",
  "ostrinc-v" + version + "-x86_64-unknown-linux-gnu.tar.gz",
  "ostrinc-v" + version + "-aarch64-apple-darwin.tar.gz",
  "ostrinc-v" + version + "-x86_64-pc-windows-msvc.zip",
  "--version " + version,
]) {
  requireText(notes, marker, notesPath);
}
const changelog = read("CHANGELOG.md");
requireText(changelog, "## " + version + " ", "CHANGELOG.md");

if (failures.length) {
  console.error(failures.map((failure) => "distribution-check: " + failure).join("\n"));
  process.exitCode = 1;
} else {
  console.log("distribution-check: ok (release matrix, checksums and installers)");
}
