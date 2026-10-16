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

use crate::effects::{EffectKind, EffectReport, EffectSignature};

fn reproducibility_requirements(
    signature: &EffectSignature,
) -> std::collections::BTreeSet<&'static str> {
    let mut requirements = std::collections::BTreeSet::new();
    for effect in &signature.effects {
        requirements.insert(match effect {
            EffectKind::RandomSeed => "seed_and_rng_algorithm",
            EffectKind::Clock => "captured_clock_or_fixed_time",
            EffectKind::Filesystem => "input_snapshots_and_hashes",
            EffectKind::Network => "captured_responses_or_offline_replay",
            EffectKind::External => "captured_environment_and_arguments",
            EffectKind::Concurrency => "scheduler_policy_and_deterministic_order",
            EffectKind::Io => "captured_outputs_and_hashes",
            EffectKind::Unsafe => "trusted_unsafe_boundary_and_platform",
            // Local mutation is represented in the effect row, but it does
            // not by itself imply an external input that provenance must
            // capture. Unknown and descriptive effects likewise add no
            // specific evidence requirement until their contract is defined.
            EffectKind::LocalState
            | EffectKind::Measurement
            | EffectKind::Provenance
            | EffectKind::Unknown(_) => continue,
        });
    }
    if !signature.variables.is_empty() {
        requirements.insert("callback_effect_rows");
    }
    if !signature.open_calls.is_empty() {
        requirements.insert("resolve_or_capture_unknown_calls");
    }
    requirements
}

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

/// Return a repository-relative path only when the resolved file is inside
/// the current checkout. External absolute paths and relative paths that walk
/// above the checkout are omitted from portable provenance artifacts.
fn portable_path(path: &Path) -> Option<String> {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?;
    let repository_root = fs::canonicalize(repository_root).ok()?;
    let current_dir = fs::canonicalize(std::env::current_dir().ok()?).ok()?;
    let resolved = fs::canonicalize(if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    })
    .ok()?;
    let relative = resolved.strip_prefix(&repository_root).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
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
                "file": site.file.as_deref().and_then(|file| portable_path(Path::new(file))),
                "line": site.line,
                "col": site.col,
            })
        })
        .collect();
    let unresolved_calls: Vec<Value> = effects
        .unresolved_calls
        .iter()
        .map(|(function, calls)| {
            json!({
                "function": function,
                "calls": calls.iter().collect::<Vec<_>>(),
            })
        })
        .collect();
    let unresolved_call_sites: Vec<Value> = effects
        .unresolved_call_sites
        .iter()
        .map(|site| {
            json!({
                "operation": site.operation,
                "function": site.function,
                "file": site.file.as_deref().and_then(|file| portable_path(Path::new(file))),
                "line": site.line,
                "col": site.col,
            })
        })
        .collect();
    let indirect_calls: Vec<Value> = effects
        .indirect_calls
        .iter()
        .map(|(function, calls)| {
            json!({
                "function": function,
                "calls": calls.iter().collect::<Vec<_>>(),
            })
        })
        .collect();
    let symbolic_effect_calls: Vec<Value> = effects
        .symbolic_effect_calls
        .iter()
        .filter(|(_, calls)| !calls.is_empty())
        .map(|(function, calls)| {
            json!({
                "function": function,
                "calls": calls.iter().collect::<Vec<_>>(),
            })
        })
        .collect();
    let open_effect_calls: Vec<Value> = effects
        .open_effect_calls
        .iter()
        .filter(|(_, calls)| !calls.is_empty())
        .map(|(function, calls)| {
            json!({
                "function": function,
                "calls": calls.iter().collect::<Vec<_>>(),
            })
        })
        .collect();
    let function_effect_signatures: serde_json::Map<String, Value> = effects
        .function_effect_signatures
        .iter()
        .map(|(function, signature)| {
            (
                function.clone(),
                json!({
                    "effects": signature.effects.iter().map(|effect| effect.label()).collect::<Vec<_>>(),
                    "variables": signature.variables.iter().map(|name| format!("ε_{name}")).collect::<Vec<_>>(),
                    "open_calls": signature.open_calls,
                    "row_closed": signature.is_closed(),
                    "purity_proven": false,
                }),
            )
        })
        .collect();
    let function_reproducibility_requirements = effects
        .function_effect_signatures
        .iter()
        .filter_map(|(function, signature)| {
            let requirements = reproducibility_requirements(signature);
            (!requirements.is_empty()).then(|| (function.clone(), requirements))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let reproducibility_requirements = function_reproducibility_requirements
        .values()
        .flat_map(|requirements| requirements.iter().copied())
        .collect::<std::collections::BTreeSet<_>>();
    let (known_calls, symbolic_call_count, unresolved_call_count) = effects.call_coverage();
    let effect_analysis_closed = effects
        .function_effect_signatures
        .values()
        .all(EffectSignature::is_closed)
        && effects
            .open_effect_calls
            .values()
            .all(|calls| calls.is_empty())
        && effects
            .indirect_calls
            .values()
            .all(|calls| calls.is_empty())
        && unresolved_call_count == 0;

    let (level, reason) = if effect_names.is_empty() && effect_analysis_closed {
        (
            "R0-candidate",
            "no known effects or open effect rows were found; this is not a purity or reproducibility proof",
        )
    } else if effect_names.is_empty() {
        (
            "unverified",
            "the effect analysis contains open calls or callback variables, so no reproducibility candidate can be assessed",
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
            "entry": portable_path(path),
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
            "function_effect_signatures": function_effect_signatures,
            "unresolved_calls": unresolved_calls,
            "unresolved_call_sites": unresolved_call_sites,
            "symbolic_effect_calls": symbolic_effect_calls,
            "indirect_calls": indirect_calls,
            "open_effect_calls": open_effect_calls,
            "effect_coverage": {
                "count_unit": "distinct_function_operation_edges",
                "known_calls": known_calls,
                "symbolic_calls": symbolic_call_count,
                "unresolved_calls": unresolved_call_count,
                "complete": unresolved_call_count == 0,
            },
            "sites": sites,
        },
        "inputs": [],
        "outputs": [],
        "reproducibility": {
            "level": level,
            "reason": reason,
            "evidence_requirements": {
                "model": "constraints-only",
                "guarantee": false,
                "requirements": reproducibility_requirements,
                "functions": function_reproducibility_requirements,
            },
        },
        "limitations": [
            "This artifact includes experimental typed effect inference, not a static effect guarantee.",
            "Unresolved and indirect calls are listed separately because an empty known-effect row does not imply purity.",
            "Reproducibility evidence requirements are derived constraints only; they do not establish an R level.",
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
