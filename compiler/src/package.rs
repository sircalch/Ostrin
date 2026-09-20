use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct PackageManifest {
    pub name: String,
    pub version: String,
    /// Reservado para cuando la CLI acepte un directorio de proyecto en vez
    /// de un archivo de entrada explícito (documento 14, §1); hoy el punto
    /// de entrada siempre lo indica el usuario en la línea de comandos.
    #[allow(dead_code)]
    pub entry: String,
    pub dependencies: HashMap<String, DependencySpec>,
}

pub enum DependencySpec {
    Path(PathBuf),
    Git { url: String, ratchet: String },
}

pub fn load_manifest(manifest_path: &Path) -> Result<PackageManifest, String> {
    let source = fs::read_to_string(manifest_path)
        .map_err(|e| format!("could not read '{}': {e}", manifest_path.display()))?;
    let table: toml::Table = source
        .parse()
        .map_err(|e| format!("'{}' is not valid TOML: {e}", manifest_path.display()))?;

    let package = table
        .get("package")
        .and_then(|v| v.as_table())
        .ok_or_else(|| format!("'{}' is missing a [package] section", manifest_path.display()))?;

    let name = package.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    let version = package.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0").to_string();
    let entry = package.get("entry").and_then(|v| v.as_str()).unwrap_or("main.ostrin").to_string();

    let mut dependencies = HashMap::new();
    if let Some(deps) = table.get("dependencies").and_then(|v| v.as_table()) {
        let base = manifest_path.parent().unwrap_or_else(|| Path::new("."));
        for (dep_name, spec) in deps {
            let Some(spec_table) = spec.as_table() else {
                return Err(format!("dependency '{dep_name}' must be a table, e.g. {{ path = \"...\" }}"));
            };
            if let Some(path) = spec_table.get("path").and_then(|v| v.as_str()) {
                dependencies.insert(dep_name.clone(), DependencySpec::Path(base.join(path)));
            } else if let Some(url) = spec_table.get("git").and_then(|v| v.as_str()) {
                let ratchet = spec_table
                    .get("tag")
                    .or_else(|| spec_table.get("rev"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("HEAD")
                    .to_string();
                dependencies.insert(dep_name.clone(), DependencySpec::Git { url: url.to_string(), ratchet });
            } else {
                return Err(format!("dependency '{dep_name}' needs either 'path' or 'git'"));
            }
        }
    }

    Ok(PackageManifest { name, version, entry, dependencies })
}

/// Resuelve cada dependencia a un directorio raíz local.
///
/// Las dependencias Git siguen siendo opt-in: una compilación normal no hace
/// red. `--fetch` habilita el clon/actualización explícitos y deja el checkout
/// en `.ostrin/packages/`, de modo que el lockfile puede apuntar a una ruta
/// estable dentro del proyecto.
pub fn resolve_dependency_roots(
    manifest: &PackageManifest,
    manifest_dir: &Path,
    fetch_git: bool,
) -> Result<HashMap<String, PathBuf>, String> {
    let mut roots = HashMap::new();
    for (name, spec) in &manifest.dependencies {
        match spec {
            DependencySpec::Path(p) => {
                if !p.is_dir() {
                    return Err(format!(
                        "dependency '{name}': path '{}' does not exist or is not a directory",
                        p.display()
                    ));
                }
                roots.insert(name.clone(), p.clone());
            }
            DependencySpec::Git { url, ratchet } if !fetch_git => {
                return Err(format!(
                    "dependency '{name}' uses 'git = \"{url}\"' (@ {ratchet}), but ostrinc does not fetch git dependencies automatically.\nPass --fetch explicitly to allow the compiler to clone/update it, or clone it yourself and reference it with a 'path' dependency instead."
                ));
            }
            DependencySpec::Git { url, ratchet } => {
                let root = fetch_git_dependency(manifest_dir, name, url, ratchet)?;
                roots.insert(name.clone(), root);
            }
        }
    }
    Ok(roots)
}

fn fetch_git_dependency(manifest_dir: &Path, name: &str, url: &str, ratchet: &str) -> Result<PathBuf, String> {
    let cache = manifest_dir.join(".ostrin").join("packages");
    fs::create_dir_all(&cache).map_err(|e| format!("could not create package cache '{}': {e}", cache.display()))?;
    let key = stable_key(&format!("{name}\n{url}\n{ratchet}"));
    let safe_name = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect::<String>();
    let destination = cache.join(format!("{safe_name}-{key:016x}"));

    if destination.join(".git").is_dir() {
        run_git(&destination, &["fetch", "--tags", "--prune", "origin"])
            .map_err(|e| format!("could not update git dependency '{name}': {e}"))?;
    } else {
        if destination.exists() {
            return Err(format!(
                "git dependency '{name}' cache path '{}' exists but is not a Git checkout; remove it and retry",
                destination.display()
            ));
        }
        let destination_text = destination.to_string_lossy().into_owned();
        let output = Command::new("git")
            .args(["clone", "--no-checkout", url, &destination_text])
            .output()
            .map_err(|e| format!("could not start git: {e}"))?;
        if !output.status.success() {
            return Err(format!("git clone failed: {}", command_error(&output.stderr, output.status.code())));
        }
    }

    run_git(&destination, &["checkout", "--detach", ratchet])
        .map_err(|e| format!("could not checkout '{ratchet}' for git dependency '{name}': {e}"))?;
    let revision = git_output(&destination, &["rev-parse", "HEAD"])
        .map_err(|e| format!("could not resolve the commit for git dependency '{name}': {e}"))?;
    if revision.is_empty() {
        return Err(format!("git dependency '{name}' resolved to an empty commit"));
    }
    Ok(destination)
}

fn run_git(directory: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .map_err(|e| format!("could not start git: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(&output.stderr, output.status.code()))
    }
}

fn git_output(directory: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .map_err(|e| format!("could not start git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(command_error(&output.stderr, output.status.code()))
    }
}

fn command_error(stderr: &[u8], code: Option<i32>) -> String {
    let detail = String::from_utf8_lossy(stderr).trim().to_string();
    if detail.is_empty() {
        format!("process exited with {}", code.map_or_else(|| "no status".to_string(), |n| n.to_string()))
    } else {
        detail
    }
}

/// FNV-1a is only a cache-key namespace, not a package integrity claim.
fn stable_key(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}

fn package_version(root: &Path) -> Result<String, String> {
    let manifest = root.join("ostrin.toml");
    if !manifest.is_file() {
        return Ok("0.0.0".to_string());
    }
    Ok(load_manifest(&manifest)?.version)
}

pub fn write_lockfile(manifest_dir: &Path, manifest: &PackageManifest, roots: &HashMap<String, PathBuf>) -> Result<(), String> {
    let mut out = String::new();
    out.push_str(&format!("# generado por ostrinc — no editar a mano\nlockfile_version = 1\npackage = {}\nversion = {}\n\n", toml_string(&manifest.name), toml_string(&manifest.version)));
    let base = manifest_dir.canonicalize().unwrap_or_else(|_| manifest_dir.to_path_buf());
    let mut names: Vec<&String> = roots.keys().collect();
    names.sort();
    for name in names {
        let root = &roots[name];
        let abs = root.canonicalize().unwrap_or_else(|_| root.clone());
        let relative = root.strip_prefix(manifest_dir).ok();
        let display = relative
            .or_else(|| abs.strip_prefix(&base).ok())
            .unwrap_or(&abs)
            .display()
            .to_string()
            .replace('\\', "/");
        let display = if display.is_empty() { "." } else { &display };
        out.push_str(&format!("[[dependency]]\nname = {}\n", toml_string(name)));
        match manifest.dependencies.get(name) {
            Some(DependencySpec::Path(_)) => {
                out.push_str("source = \"path\"\n");
                out.push_str(&format!("resolved_path = {}\n", toml_string(&display)));
            }
            Some(DependencySpec::Git { url, ratchet }) => {
                let revision = git_output(root, &["rev-parse", "HEAD"])
                    .map_err(|e| format!("could not record resolved commit for dependency '{name}': {e}"))?;
                out.push_str("source = \"git\"\n");
                out.push_str(&format!("git = {}\nrequested = {}\nresolved_rev = {}\nresolved_path = {}\n", toml_string(url), toml_string(ratchet), toml_string(&revision), toml_string(&display)));
            }
            None => return Err(format!("dependency '{name}' was resolved but is not in the manifest")),
        }
        out.push_str(&format!("package_version = {}\n\n", toml_string(&package_version(root)?)));
    }
    let lock_path = manifest_dir.join("ostrin.lock");
    fs::write(&lock_path, out).map_err(|e| format!("could not write '{}': {e}", lock_path.display()))
}
