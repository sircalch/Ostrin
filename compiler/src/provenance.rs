//! Experimental, conservative provenance artifacts for Ostrin programs.
//!
//! The report records what the compiler can prove from the current source and
//! effect inventory. It deliberately does not claim replay or purity: input
//! snapshots, generated outputs, package locks and platform guarantees still
//! need to be supplied by a future reproducibility pipeline.

use std::path::Path;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::effects::EffectReport;

pub fn build_report(path: &Path, source: &[u8], target: &str, effects: &EffectReport) -> Value {
    let normalized_source = String::from_utf8_lossy(source).replace("\r\n", "\n");
    let mut hasher = Sha256::new();
    hasher.update(normalized_source.as_bytes());
    let source_hash = format!("sha256:{:x}", hasher.finalize());
    let effect_names: Vec<String> = effects.effects.iter().cloned().collect();
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
        "activity": {
            "effects": effect_names,
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
            "A future R2/R3 pipeline will add input snapshots, output hashes, lockfiles and runtime metadata.",
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
    format!(
        "provenance-report: experimental artifact\nentry: {}\nsource-hash: {}\ncompiler: {} {} ({})\ntarget: {}\neffects: {}\nreproducibility: {}\nnote: {}",
        program["entry"].as_str().unwrap_or("<unknown>"),
        program["source_hash"].as_str().unwrap_or("<unknown>"),
        compiler["name"].as_str().unwrap_or("ostrinc"),
        compiler["version"].as_str().unwrap_or("<unknown>"),
        compiler["commit"].as_str().unwrap_or("unknown"),
        compiler["target"].as_str().unwrap_or("native"),
        if effects.is_empty() { "none known" } else { &effects },
        report["reproducibility"]["level"].as_str().unwrap_or("unverified"),
        report["reproducibility"]["reason"].as_str().unwrap_or(""),
    )
}
