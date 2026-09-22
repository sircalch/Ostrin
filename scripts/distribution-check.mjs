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
for (const marker of ["sha256sum", "shasum", "ARCHIVE=", "gh release create"]) {
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
  "sha256sum dist/ostrinc.wasm dist/hello.wasm dist/pkg_project.wasm dist/wasi_io_contract.wasm",
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

const readme = read("README.md");
for (const marker of ["scripts/install.sh", "scripts/install.ps1", "checksum"]) {
  requireText(readme, marker, "README installation documentation");
}

if (failures.length) {
  console.error(failures.map((failure) => "distribution-check: " + failure).join("\n"));
  process.exitCode = 1;
} else {
  console.log("distribution-check: ok (release matrix, checksums and installers)");
}
