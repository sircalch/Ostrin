use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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

/// Resuelve cada dependencia a un directorio raíz local ya existente en disco.
/// Las dependencias 'git' se reconocen y quedan registradas en el manifiesto,
/// pero esta build de ostrinc no clona repositorios automáticamente — sería
/// una operación de red no solicitada explícitamente en cada compilación. Usa
/// una dependencia 'path' apuntando a un clon ya hecho a mano mientras tanto.
pub fn resolve_dependency_roots(manifest: &PackageManifest) -> Result<HashMap<String, PathBuf>, String> {
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
            DependencySpec::Git { url, ratchet } => {
                return Err(format!(
                    "dependency '{name}' uses 'git = \"{url}\"' (@ {ratchet}), but this build of ostrinc does not fetch git dependencies automatically (no network access is performed as a side effect of compiling).\nClone it yourself and reference it with a 'path' dependency instead."
                ));
            }
        }
    }
    Ok(roots)
}

pub fn write_lockfile(manifest_dir: &Path, manifest: &PackageManifest, roots: &HashMap<String, PathBuf>) -> Result<(), String> {
    let mut out = String::new();
    out.push_str(&format!("# generado por ostrinc — no editar a mano\npackage = \"{}\"\nversion = \"{}\"\n\n", manifest.name, manifest.version));
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
        out.push_str(&format!("[[dependency]]\nname = \"{name}\"\nresolved_path = \"{display}\"\n\n"));
    }
    let lock_path = manifest_dir.join("ostrin.lock");
    fs::write(&lock_path, out).map_err(|e| format!("could not write '{}': {e}", lock_path.display()))
}
