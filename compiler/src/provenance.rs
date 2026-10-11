//! Experimental, conservative provenance artifacts for Ostrin programs.
//!
//! The report records what the compiler can prove from the current source and
//! effect inventory. It deliberately does not claim replay or purity: input
//! snapshots, generated outputs, complete package locks and captured platform
//! metadata still need to be supplied by a future reproducibility pipeline.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::effects::EffectReport;

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn normalized_file_hash(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let normalized = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    Some(sha256_bytes(normalized.as_bytes()))
}

fn ancestor_file(start: &Path, file_name: &str) -> Option<PathBuf> {
    let mut directory = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent()?.to_path_buf()
    };
    loop {
        let candidate = directory.join(file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !directory.pop() {
            return None;
        }
    }
}

fn project_file(path: &Path, file_name: &str) -> Option<Value> {
    let file = ancestor_file(path, file_name)?;
    Some(json!({
        "path": file_name,
        "sha256": normalized_file_hash(&file)?,
    }))
}

fn runtime_snapshot(path: &Path, target: &str) -> Value {
    // The compiler lockfile is part of the source tree used to build this
    // executable. Keep only a repository-relative label in the artifact so a
    // report never leaks the absolute checkout path.
    let compiler_lockfile = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let compiler_lock = normalized_file_hash(&compiler_lockfile).map(|sha256| {
        json!({
            "path": "compiler/Cargo.lock",
            "sha256": sha256,
        })
    });
    let project_manifest = project_file(path, "ostrin.toml");
    let project_lockfile = project_file(path, "ostrin.lock");
    let wasi = if target.contains("wasi") {
        Some("preview1")
    } else {
        None
    };

    json!({
        "schema": "ostrin.runtime/v1",
        "status": "partial",
        "target": target,
        "wasi": wasi,
        "runner": if wasi.is_some() { "WASI host" } else { "native process" },
        "compiler_lockfile": compiler_lock,
        "project": {
            "manifest": project_manifest,
            "lockfile": project_lockfile,
        },
        "platform_policy": {
            "output": "target-specific; byte exact only when the replay gate verifies it",
            "numeric": "tolerances must be declared for cross-platform comparisons",
        },
        "missing": [
            "input snapshots",
            "output hashes from this invocation",
            "captured host and hardware metadata",
        ],
    })
}

pub fn build_report(path: &Path, source: &[u8], target: &str, effects: &EffectReport) -> Value {
    let normalized_source = String::from_utf8_lossy(source).replace("\r\n", "\n");
    let source_hash = sha256_bytes(normalized_source.as_bytes());
    let effect_names: Vec<String> = effects.effects.iter().cloned().collect();
    let functions_without_known_effects: Vec<String> = effects
        .function_names
        .iter()
        .filter(|function| {
            effects
                .function_kinds
                .get(*function)
                .is_none_or(std::collections::BTreeSet::is_empty)
        })
        .cloned()
        .collect();
    let sites: Vec<Value> = effects
        .sites
        .iter()
        .map(|site| {
            json!({
                "effect": site.effect,
                "operation": site.operation,
                "function": site.function,
                "file": site.file.as_deref().map(|file| file.replace('\\', "/")),
                "line": site.line,
                "col": site.col,
            })
        })
        .collect();

    let (level, reason) = if effect_names.is_empty() {
        (
            "R0-candidate",
            "no known effect sites were found; this inventory is not a purity proof",
        )
    } else {
        (
            "unverified",
            "input snapshots, output hashes, package locks and platform replay are not captured yet",
        )
    };

    json!({
        "schema": "ostrin.provenance/v0",
        "experimental": true,
        "verified": false,
        "program": {
            "entry": path.display().to_string(),
            "source_hash": source_hash,
        },
        "compiler": {
            "name": "ostrinc",
            "version": env!("CARGO_PKG_VERSION"),
            "commit": option_env!("OSTRIN_COMMIT").unwrap_or("unknown"),
            "target": target,
        },
        "runtime": runtime_snapshot(path, target),
        "activity": {
            "effects": effect_names,
            "functions_without_known_effects": functions_without_known_effects,
            "sites": sites,
        },
        "inputs": [],
        "outputs": [],
        "reproducibility": {
            "level": level,
            "reason": reason,
        },
        "limitations": [
            "This artifact is a conservative source inventory, not a static effect guarantee.",
            "The runtime snapshot is partial; input snapshots, invocation output hashes and captured host metadata are still required for R2/R3.",
        ],
    })
}

pub fn text_summary(report: &Value) -> String {
    let program = &report["program"];
    let compiler = &report["compiler"];
    let effects = report["activity"]["effects"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let no_known_effects = report["activity"]["functions_without_known_effects"]
        .as_array()
        .map(|values| values.len().to_string())
        .unwrap_or_else(|| "0".to_string());
    format!(
        "provenance-report: experimental artifact\nentry: {}\nsource-hash: {}\ncompiler: {} {} ({})\ntarget: {}\neffects: {}\nfunctions-without-known-effects: {}\nreproducibility: {}\nnote: {}",
        program["entry"].as_str().unwrap_or("<unknown>"),
        program["source_hash"].as_str().unwrap_or("<unknown>"),
        compiler["name"].as_str().unwrap_or("ostrinc"),
        compiler["version"].as_str().unwrap_or("<unknown>"),
        compiler["commit"].as_str().unwrap_or("unknown"),
        compiler["target"].as_str().unwrap_or("native"),
        if effects.is_empty() { "none known" } else { &effects },
        no_known_effects,
        report["reproducibility"]["level"].as_str().unwrap_or("unverified"),
        report["reproducibility"]["reason"].as_str().unwrap_or(""),
    )
}
