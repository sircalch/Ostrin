use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

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

#[derive(Debug, Clone)]
pub enum DependencySpec {
    Path(PathBuf),
    Git { url: String, ratchet: String },
}

#[derive(Debug, Clone)]
pub struct ResolvedDependency {
    pub spec: DependencySpec,
    pub root: PathBuf,
}

#[derive(Debug, Default)]
pub struct ResolvedDependencies {
    pub dependencies: HashMap<String, ResolvedDependency>,
}

impl ResolvedDependencies {
    pub fn roots(&self) -> HashMap<String, PathBuf> {
        self.dependencies
            .iter()
            .map(|(name, dependency)| (name.clone(), dependency.root.clone()))
            .collect()
    }
}

#[derive(Debug, Clone)]
struct LockedDependency {
    source: String,
    resolved_path: PathBuf,
    git: Option<String>,
    requested: Option<String>,
    resolved_rev: Option<String>,
    package_version: Option<String>,
    content_sha256: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct Lockfile {
    dependencies: HashMap<String, LockedDependency>,
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
    locked_mode: bool,
) -> Result<HashMap<String, PathBuf>, String> {
    Ok(resolve_dependency_graph(manifest, manifest_dir, fetch_git, locked_mode)?.roots())
}

/// Resuelve el grafo completo de dependencias y aplana los alias que pueden
/// importar los módulos. Cada paquete puede declarar sus propias
/// dependencias; los alias deben ser globalmente únicos dentro del proyecto,
/// porque el cargador de módulos mantiene un espacio de nombres plano.
pub fn resolve_dependency_graph(
    manifest: &PackageManifest,
    manifest_dir: &Path,
    fetch_git: bool,
    locked_mode: bool,
) -> Result<ResolvedDependencies, String> {
    let lockfile = load_lockfile(manifest_dir)?;
    if locked_mode && lockfile.is_none() && !manifest.dependencies.is_empty() {
        return Err(format!(
            "project '{}' has dependencies but no ostrin.lock; run without --locked once to resolve them",
            manifest.name
        ));
    }

    let mut resolved = ResolvedDependencies::default();
    let mut visiting = Vec::new();
    for (name, spec) in &manifest.dependencies {
        resolve_dependency(
            name,
            spec,
            manifest_dir,
            fetch_git,
            locked_mode,
            lockfile.as_ref(),
            &mut resolved,
            &mut visiting,
        )?;
    }
    if let Some(lockfile) = &lockfile {
        for name in lockfile.dependencies.keys() {
            if !resolved.dependencies.contains_key(name) {
                return Err(format!(
                    "ostrin.lock contains dependency '{name}' which is not present in the resolved dependency graph; regenerate the lockfile"
                ));
            }
        }
    }
    Ok(resolved)
}

fn resolve_dependency(
    name: &str,
    spec: &DependencySpec,
    project_dir: &Path,
    fetch_git: bool,
    locked_mode: bool,
    lockfile: Option<&Lockfile>,
    resolved: &mut ResolvedDependencies,
    visiting: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if resolved.dependencies.contains_key(name) {
        return Err(format!(
            "dependency alias '{name}' is declared more than once in the resolved package graph; use a unique dependency alias"
        ));
    }

    let locked = lockfile.and_then(|file| file.dependencies.get(name));
    let root = match spec {
        DependencySpec::Path(path) => {
            let root = if let Some(entry) = locked {
                validate_locked_path(name, path, entry, project_dir)?
            } else {
                if locked_mode {
                    return Err(format!(
                        "dependency '{name}' is missing from ostrin.lock; run without --locked to regenerate it"
                    ));
                }
                path.clone()
            };
            if !root.is_dir() {
                return Err(format!(
                    "dependency '{name}': path '{}' does not exist or is not a directory",
                    root.display()
                ));
            }
            validate_package_version(name, &root, locked)?;
            root
        }
        DependencySpec::Git { url, ratchet } if !fetch_git => {
            if let Some(entry) = locked {
                let root = validate_locked_git(name, url, ratchet, entry, project_dir)?;
                validate_package_version(name, &root, Some(entry))?;
                root
            } else {
                return Err(format!(
                    "dependency '{name}' uses 'git = \"{url}\"' (@ {ratchet}), but ostrinc does not fetch git dependencies automatically.\nPass --fetch explicitly to resolve it, or clone it yourself and reference it with a 'path' dependency instead."
                ));
            }
        }
        DependencySpec::Git { url, ratchet } => {
            let root = if let Some(entry) = locked {
                match validate_locked_git(name, url, ratchet, entry, project_dir) {
                    Ok(root) => root,
                    Err(error) if !locked_mode => fetch_git_dependency(project_dir, name, url, ratchet)
                        .map_err(|fetch_error| format!("{error}; fetching dependency also failed: {fetch_error}"))?,
                    Err(error) => return Err(error),
                }
            } else {
                if locked_mode {
                    return Err(format!(
                        "dependency '{name}' is missing from ostrin.lock; run without --locked to resolve it"
                    ));
                }
                fetch_git_dependency(project_dir, name, url, ratchet)?
            };
            validate_package_version(name, &root, locked)?;
            root
        }
    };

    let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
    if visiting.iter().any(|ancestor| ancestor == &canonical) {
        return Err(format!(
            "dependency graph contains a cycle through '{}'",
            root.display()
        ));
    }
    resolved.dependencies.insert(
        name.to_string(),
        ResolvedDependency { spec: spec.clone(), root: root.clone() },
    );

    let nested_manifest_path = root.join("ostrin.toml");
    if nested_manifest_path.is_file() {
        let nested_manifest = load_manifest(&nested_manifest_path)?;
        visiting.push(canonical);
        for (child_name, child_spec) in &nested_manifest.dependencies {
            resolve_dependency(
                child_name,
                child_spec,
                project_dir,
                fetch_git,
                locked_mode,
                lockfile,
                resolved,
                visiting,
            )?;
        }
        visiting.pop();
    }
    Ok(())
}

fn load_lockfile(manifest_dir: &Path) -> Result<Option<Lockfile>, String> {
    let path = manifest_dir.join("ostrin.lock");
    if !path.is_file() {
        return Ok(None);
    }
    let source = fs::read_to_string(&path)
        .map_err(|e| format!("could not read '{}': {e}", path.display()))?;
    let table: toml::Table = source
        .parse()
        .map_err(|e| format!("'{}' is not valid TOML: {e}", path.display()))?;
    let version = table
        .get("lockfile_version")
        .and_then(|value| value.as_integer())
        .ok_or_else(|| format!("'{}' has no supported lockfile_version", path.display()))?;
    if version != 1 {
        return Err(format!(
            "'{}' uses unsupported lockfile_version {version}; expected 1",
            path.display()
        ));
    }

    let mut dependencies = HashMap::new();
    if let Some(entries) = table.get("dependency").and_then(|value| value.as_array()) {
        for entry in entries {
            let Some(entry) = entry.as_table() else {
                return Err(format!("'{}' contains a non-table dependency entry", path.display()));
            };
            let name = required_lock_string(entry, "name", &path)?;
            if dependencies.contains_key(&name) {
                return Err(format!("'{}' contains duplicate dependency '{name}'", path.display()));
            }
            let source = required_lock_string(entry, "source", &path)?;
            let resolved_path = PathBuf::from(required_lock_string(entry, "resolved_path", &path)?);
            dependencies.insert(
                name,
                LockedDependency {
                    source,
                    resolved_path,
                    git: optional_lock_string(entry, "git"),
                    requested: optional_lock_string(entry, "requested"),
                    resolved_rev: optional_lock_string(entry, "resolved_rev"),
                    package_version: optional_lock_string(entry, "package_version"),
                    content_sha256: optional_lock_string(entry, "content_sha256"),
                },
            );
        }
    }
    Ok(Some(Lockfile { dependencies }))
}

fn required_lock_string(table: &toml::value::Table, key: &str, path: &Path) -> Result<String, String> {
    table
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("'{}' has a dependency entry without string '{key}'", path.display()))
}

fn optional_lock_string(table: &toml::value::Table, key: &str) -> Option<String> {
    table.get(key).and_then(|value| value.as_str()).map(str::to_string)
}

fn lock_root(manifest_dir: &Path, entry: &LockedDependency) -> PathBuf {
    if entry.resolved_path.is_absolute() {
        entry.resolved_path.clone()
    } else {
        manifest_dir.join(&entry.resolved_path)
    }
}

fn validate_locked_path(
    name: &str,
    declared: &Path,
    entry: &LockedDependency,
    manifest_dir: &Path,
) -> Result<PathBuf, String> {
    if entry.source != "path" {
        return Err(format!(
            "dependency '{name}' is declared as path but ostrin.lock records source '{}'; regenerate the lockfile",
            entry.source
        ));
    }
    let locked = lock_root(manifest_dir, entry);
    let declared = declared.canonicalize().unwrap_or_else(|_| declared.to_path_buf());
    let locked_canonical = locked.canonicalize().unwrap_or_else(|_| locked.clone());
    if declared != locked_canonical {
        return Err(format!(
            "dependency '{name}' path changed from '{}' to '{}'; regenerate the lockfile",
            locked.display(),
            declared.display()
        ));
    }
    validate_content_hash(name, &locked, entry)?;
    Ok(locked)
}

fn validate_locked_git(
    name: &str,
    url: &str,
    ratchet: &str,
    entry: &LockedDependency,
    manifest_dir: &Path,
) -> Result<PathBuf, String> {
    if entry.source != "git"
        || entry.git.as_deref() != Some(url)
        || entry.requested.as_deref() != Some(ratchet)
    {
        return Err(format!(
            "dependency '{name}' no longer matches its ostrin.lock Git source or requested revision; regenerate the lockfile"
        ));
    }
    let expected = entry.resolved_rev.as_deref().ok_or_else(|| {
        format!("dependency '{name}' has no resolved_rev in ostrin.lock; regenerate the lockfile")
    })?;
    let root = lock_root(manifest_dir, entry);
    if !root.join(".git").is_dir() {
        return Err(format!(
            "dependency '{name}' checkout '{}' is missing; run with --fetch to restore it",
            root.display()
        ));
    }
    let actual = git_output(&root, &["rev-parse", "HEAD"])
        .map_err(|error| format!("could not inspect locked dependency '{name}': {error}"))?;
    if actual != expected {
        return Err(format!(
            "dependency '{name}' checkout is at {actual}, but ostrin.lock requires {expected}; run with --fetch to restore it"
        ));
    }
    validate_content_hash(name, &root, entry)?;
    Ok(root)
}

fn validate_package_version(
    name: &str,
    root: &Path,
    entry: Option<&LockedDependency>,
) -> Result<(), String> {
    let Some(entry) = entry else { return Ok(()); };
    let Some(expected) = &entry.package_version else { return Ok(()); };
    let actual = package_version(root)?;
    if &actual != expected {
        return Err(format!(
            "dependency '{name}' changed package version from '{expected}' to '{actual}'; regenerate the lockfile"
        ));
    }
    Ok(())
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

fn validate_content_hash(name: &str, root: &Path, entry: &LockedDependency) -> Result<(), String> {
    let expected = entry.content_sha256.as_deref().ok_or_else(|| {
        format!("dependency '{name}' has no content_sha256 in ostrin.lock; regenerate the lockfile")
    })?;
    let actual = package_content_sha256(root)?;
    if actual != expected {
        return Err(format!(
            "dependency '{name}' content hash changed from '{expected}' to '{actual}'; regenerate the lockfile"
        ));
    }
    Ok(())
}

fn package_content_sha256(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect_package_sources(root, root, &mut files)?;
    files.sort();

    let mut hasher = Sha256::new();
    for relative in files {
        let normalized = relative.to_string_lossy().replace('\\', "/");
        let bytes = fs::read(root.join(&relative)).map_err(|e| {
            format!(
                "could not read package source '{}' for integrity hashing: {e}",
                root.join(&relative).display()
            )
        })?;
        let bytes = canonical_package_bytes(&bytes);
        hasher.update(b"ostrin-package-file\0");
        hasher.update(normalized.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn canonical_package_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut canonical = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            if bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            canonical.push(b'\n');
        } else {
            canonical.push(bytes[index]);
        }
        index += 1;
    }
    canonical
}

fn collect_package_sources(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|e| format!("could not read package directory '{}': {e}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("could not inspect package directory '{}': {e}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("could not inspect package path '{}': {e}", path.display()))?;
        if file_type.is_dir() {
            if matches!(entry.file_name().to_str(), Some(".git" | ".ostrin")) {
                continue;
            }
            collect_package_sources(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| format!("package source '{}' is outside package root", path.display()))?
                .to_path_buf();
            let is_manifest = relative.file_name().and_then(|name| name.to_str()) == Some("ostrin.toml");
            let is_source = relative.extension().and_then(|extension| extension.to_str()) == Some("ostrin");
            if is_manifest || is_source {
                files.push(relative);
            }
        }
    }
    Ok(())
}

fn portable_relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from = from.canonicalize().unwrap_or_else(|_| from.to_path_buf());
    let to = to.canonicalize().unwrap_or_else(|_| to.to_path_buf());
    let from: Vec<Component<'_>> = from.components().collect();
    let to: Vec<Component<'_>> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(left, right)| left == right).count();
    if common == 0 {
        return None;
    }

    let mut relative = PathBuf::new();
    for component in &from[common..] {
        if matches!(component, Component::Normal(_)) {
            relative.push("..");
        }
    }
    for component in &to[common..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

pub fn write_lockfile(manifest_dir: &Path, manifest: &PackageManifest, resolved: &ResolvedDependencies) -> Result<(), String> {
    let mut out = String::new();
    out.push_str(&format!("# generado por ostrinc — no editar a mano\nlockfile_version = 1\npackage = {}\nversion = {}\n\n", toml_string(&manifest.name), toml_string(&manifest.version)));
    let base = manifest_dir.canonicalize().unwrap_or_else(|_| manifest_dir.to_path_buf());
    let mut names: Vec<&String> = resolved.dependencies.keys().collect();
    names.sort();
    for name in names {
        let dependency = &resolved.dependencies[name];
        let root = &dependency.root;
        let abs = root.canonicalize().unwrap_or_else(|_| root.clone());
        let display = portable_relative_path(manifest_dir, root)
            .or_else(|| root.strip_prefix(manifest_dir).ok().map(PathBuf::from))
            .or_else(|| abs.strip_prefix(&base).ok().map(PathBuf::from))
            .unwrap_or(abs)
            .display()
            .to_string()
            .replace('\\', "/");
        let display = if display.is_empty() { "." } else { &display };
        let content_sha256 = package_content_sha256(root)?;
        out.push_str(&format!("[[dependency]]\nname = {}\n", toml_string(name)));
        match &dependency.spec {
            DependencySpec::Path(_) => {
                out.push_str("source = \"path\"\n");
                out.push_str(&format!("resolved_path = {}\n", toml_string(&display)));
            }
            DependencySpec::Git { url, ratchet } => {
                let revision = git_output(root, &["rev-parse", "HEAD"])
                    .map_err(|e| format!("could not record resolved commit for dependency '{name}': {e}"))?;
                out.push_str("source = \"git\"\n");
                out.push_str(&format!("git = {}\nrequested = {}\nresolved_rev = {}\nresolved_path = {}\n", toml_string(url), toml_string(ratchet), toml_string(&revision), toml_string(&display)));
            }
        }
        out.push_str(&format!(
            "package_version = {}\ncontent_sha256 = {}\n\n",
            toml_string(&package_version(root)?),
            toml_string(&content_sha256)
        ));
    }
    let lock_path = manifest_dir.join("ostrin.lock");
    fs::write(&lock_path, out).map_err(|e| format!("could not write '{}': {e}", lock_path.display()))
}
