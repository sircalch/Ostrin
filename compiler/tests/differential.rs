//! Safety net for the compiler pipeline (Stage 0 of the architecture plan).
//!
//! * `every_example_agrees_between_interpreter_and_native` — the tree-walking
//!   interpreter is the semantic oracle; every example in `examples/` that
//!   type-checks and runs must produce byte-identical output when compiled
//!   natively. New examples are picked up automatically.
//! * `compile_fail_examples_report_their_error_code` — every `*_errors.ostrin`
//!   style example must be rejected with a stable `OSTRIN-E….` code.
//! * `mutated_sources_never_crash_the_front_end` — deterministic mutation
//!   fuzzing of the lexer/parser/checker: bad input may produce errors but
//!   must never panic.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

fn ostrinc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ostrinc")).args(args).output().expect("failed to run ostrinc")
}

fn examples() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("examples directory")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|ext| ext == "ostrin"))
        .collect();
    files.sort();
    files
}

fn name_of(path: &PathBuf) -> String {
    path.file_name().unwrap().to_string_lossy().to_string()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

/// Examples the native backend deliberately does not reproduce, with the
/// reason. Each entry must *fail* to compile natively — when a gap is closed
/// the assertion below fires and the entry must be deleted.
const KNOWN_NATIVE_GAPS: &[(&str, &str)] = &[];

/// Examples that compile natively but whose output legitimately differs
/// (operating-system error text comes from `strerror` vs Rust's `io::Error`).
const KNOWN_OUTPUT_DIFFERENCES: &[(&str, &str)] = &[
    ("stdlib_io.ostrin", "OS-specific error message text"),
];

/// Files that are syntax showcases, not runnable programs.
const NOT_PROGRAMS: &[&str] = &["newlines.ostrin", "advanced.ostrin"];

#[test]
fn every_example_agrees_between_interpreter_and_native() {
    let mut compared = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for path in examples() {
        let name = name_of(&path);
        let file = path.to_string_lossy().to_string();
        if NOT_PROGRAMS.contains(&name.as_str()) {
            continue;
        }
        if !ostrinc(&["--check", &file]).status.success() {
            continue; // a negative example: covered by the compile-fail test
        }
        let interpreted = ostrinc(&["--run", &file]);
        if !interpreted.status.success() {
            continue; // rejected at run time by design (e.g. E1101)
        }
        let exe = std::env::temp_dir().join(format!("ostrin_diff_{}_{}.exe", std::process::id(), name));
        let exe_str = exe.to_string_lossy().to_string();
        let compile = ostrinc(&["--compile", "--out", &exe_str, &file]);
        if !compile.status.success() {
            let error = text(&compile.stderr);
            if error.contains("no GNU-compatible C compiler found") {
                // CI sets OSTRIN_REQUIRE_CC so a missing compiler can't silently
                // turn this whole safety net into a no-op.
                assert!(std::env::var_os("OSTRIN_REQUIRE_CC").is_none(), "OSTRIN_REQUIRE_CC is set but no C compiler was found");
                eprintln!("skipping the native comparison: no C compiler available");
                return;
            }
            if KNOWN_NATIVE_GAPS.iter().any(|(n, _)| *n == name) {
                continue;
            }
            failures.push(format!("{name}: native compilation failed: {}", error.lines().next().unwrap_or("")));
            continue;
        }
        if let Some((_, why)) = KNOWN_NATIVE_GAPS.iter().find(|(n, _)| *n == name) {
            failures.push(format!("{name}: now compiles natively — remove it from KNOWN_NATIVE_GAPS (was: {why})"));
            continue;
        }
        let native = Command::new(&exe).output().expect("failed to run the compiled program");
        let _ = fs::remove_file(&exe);
        if KNOWN_OUTPUT_DIFFERENCES.iter().any(|(n, _)| *n == name) {
            continue;
        }
        compared += 1;
        if text(&native.stdout) != text(&interpreted.stdout) {
            failures.push(format!("{name}: output differs between interpreter and native"));
        }
    }
    assert!(failures.is_empty(), "differential failures:\n  {}", failures.join("\n  "));
    assert!(compared >= 30, "the differential test compared only {compared} examples; is the examples directory intact?");
}

#[test]
fn compile_fail_examples_report_their_error_code() {
    let mut checked = 0usize;
    for path in examples() {
        let name = name_of(&path);
        if !(name.ends_with("_error.ostrin") || name.ends_with("_errors.ostrin")) {
            continue;
        }
        let out = ostrinc(&["--check", &path.to_string_lossy()]);
        assert!(!out.status.success(), "{name} should be rejected by the checker");
        let diagnostics = format!("{}{}", text(&out.stdout), text(&out.stderr));
        let has_code = diagnostics.split("OSTRIN-E").skip(1).any(|rest| rest.chars().take(4).all(|c| c.is_ascii_digit()));
        assert!(has_code, "{name} was rejected without a stable OSTRIN-E code:\n{diagnostics}");
        checked += 1;
    }
    assert!(checked >= 15, "only {checked} compile-fail examples were found");
}

/// Small deterministic generator (xorshift64*), so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn mutate(source: &str, rng: &mut Rng) -> String {
    let chars: Vec<char> = source.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let mut out = chars.clone();
    match rng.below(6) {
        0 => {
            // delete a random span
            let start = rng.below(out.len());
            let len = rng.below(12).min(out.len() - start);
            out.drain(start..start + len);
        }
        1 => {
            // truncate
            out.truncate(rng.below(out.len()));
        }
        2 => {
            // insert punctuation that stresses the parser
            const NOISE: &[&str] = &["{", "}", "(", ")", "<", ">", "=>", "::", "fn", "match", "\"", "->", ",", ".", "record", "enum", "spawn"];
            let at = rng.below(out.len());
            for (i, c) in NOISE[rng.below(NOISE.len())].chars().enumerate() {
                out.insert(at + i, c);
            }
        }
        3 => {
            // duplicate a random span
            let start = rng.below(out.len());
            let len = rng.below(40).min(out.len() - start);
            let piece: Vec<char> = out[start..start + len].to_vec();
            let at = rng.below(out.len());
            for (i, c) in piece.into_iter().enumerate() {
                out.insert(at + i, c);
            }
        }
        4 => {
            // swap two random characters
            let (a, b) = (rng.below(out.len()), rng.below(out.len()));
            out.swap(a, b);
        }
        _ => {
            // replace a character with a random one
            let at = rng.below(out.len());
            out[at] = ['@', '#', '\\', '`', '0', ' ', '\n', '\t', '"', '\'', '$'][rng.below(11)];
        }
    }
    out.into_iter().collect()
}

#[test]
fn mutated_sources_never_crash_the_front_end() {
    let mut rng = Rng(0x0057_5249_4E00_0001);
    let dir = std::env::temp_dir().join(format!("ostrin_fuzz_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut crashes: Vec<String> = Vec::new();
    // `OSTRIN_FUZZ_ROUNDS=50 cargo test` for a deeper local run.
    let rounds: usize = std::env::var("OSTRIN_FUZZ_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    for path in examples() {
        let source = fs::read_to_string(&path).unwrap();
        for round in 0..rounds {
            let mutated = mutate(&source, &mut rng);
            let file = dir.join(format!("case_{}_{round}.ostrin", name_of(&path)));
            fs::write(&file, &mutated).unwrap();
            for mode in ["--check", "--ast"] {
                let out = ostrinc(&[mode, &file.to_string_lossy()]);
                let stderr = String::from_utf8_lossy(&out.stderr);
                if out.status.code() == Some(101) || out.status.code().is_none() || stderr.contains("panicked") {
                    crashes.push(format!("{} {mode} (round {round}): {}", name_of(&path), stderr.lines().next().unwrap_or("")));
                    let keep = std::env::temp_dir().join(format!("ostrin_fuzz_crash_{}_{round}.ostrin", name_of(&path)));
                    let _ = fs::copy(&file, keep);
                }
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
    assert!(crashes.is_empty(), "the front end crashed on mutated input:\n  {}", crashes.join("\n  "));
}

/// Ratchet for the checker's typed-expression table (Stage 1 of the
/// architecture plan): across every example that type-checks, the number of
/// expressions whose type the checker could not determine must never grow.
/// When you close a gap, lower `MAX_UNKNOWN_EXPRESSIONS` to the new value.
#[test]
fn typed_expression_table_does_not_regress() {
    const MAX_UNKNOWN_EXPRESSIONS: usize = 11;
    let (mut total, mut unknown) = (0usize, 0usize);
    for path in examples() {
        let file = path.to_string_lossy().to_string();
        if !ostrinc(&["--check", &file]).status.success() {
            continue;
        }
        let report = text(&ostrinc(&["--typed-report", &file]).stdout);
        for line in report.lines() {
            if let Some(n) = line.strip_prefix("expressions: ") {
                total += n.trim().parse::<usize>().unwrap();
            } else if let Some(n) = line.strip_prefix("unknown: ") {
                unknown += n.trim().parse::<usize>().unwrap();
            }
        }
    }
    assert!(total > 1000, "typed report covered only {total} expressions");
    assert!(unknown <= MAX_UNKNOWN_EXPRESSIONS, "{unknown} of {total} expressions have an unknown type (limit {MAX_UNKNOWN_EXPRESSIONS})");
}

/// The native backend still infers its own types (it has not yet been moved
/// onto the checker's typed-expression table). Until it is, this test keeps
/// the two honest: on every program the backend compiles, the backend's type
/// for an expression must agree with the checker's — zero divergences.
#[test]
fn native_backend_types_agree_with_the_checker() {
    let mut hir_generated = 0usize;
    let mut ir_generated = 0usize;
    let (mut agreed, mut partial, mut completed, mut unchecked, mut node_agreed, mut divergences) = (0usize, 0usize, 0usize, 0usize, 0usize, Vec::<String>::new());
    for path in examples() {
        let file = path.to_string_lossy().to_string();
        if !ostrinc(&["--check", &file]).status.success() {
            continue;
        }
        let out = ostrinc(&["--native-type-report", &file]);
        if !out.status.success() {
            continue; // not compilable natively (already covered by the differential test)
        }
        for line in text(&out.stdout).lines() {
            if let Some(n) = line.strip_prefix("agreed: ") {
                agreed += n.trim().parse::<usize>().unwrap();
            } else if let Some(n) = line.strip_prefix("node-agreed: ") {
                node_agreed += n.trim().parse::<usize>().unwrap();
            } else if let Some(n) = line.strip_prefix("partial: ") {
                partial += n.trim().parse::<usize>().unwrap();
            } else if let Some(n) = line.strip_prefix("hir-generated: ") {
                hir_generated += n.trim().parse::<usize>().unwrap();
            } else if let Some(n) = line.strip_prefix("ir-generated: ") {
                ir_generated += n.trim().parse::<usize>().unwrap();
            } else if let Some(n) = line.strip_prefix("unchecked: ") {
                unchecked += n.trim().parse::<usize>().unwrap();
            } else if let Some(n) = line.strip_prefix("completed: ") {
                completed += n.trim().parse::<usize>().unwrap();
            } else if line.starts_with("  ") {
                divergences.push(format!("{}: {}", name_of(&path), line.trim()));
            }
        }
    }
    assert!(agreed > 1300, "only {agreed} expressions were compared");
    // The same comparison over *every* AST node (operands included), by node address.
    assert!(node_agreed > 3000, "only {node_agreed} AST nodes were compared with the checker's per-node types");
    // Ratchet: expressions the backend cannot compare (the checker has no type).
    // Ratchet: functions whose C is generated from the typed HIR or its
    // explicit IR lowering (the migration of the native backend).
    let native_generated = hir_generated + ir_generated;
    assert!(native_generated >= 131, "only {native_generated} functions were generated from HIR/IR (expected at least 131)");
    println!("functions generated from HIR/IR: {native_generated} (HIR {hir_generated}, IR {ir_generated})");
    assert!(unchecked <= 4, "{unchecked} expressions have no checker type (limit 4)");
    // Every partial literal the backend meets is completed from the checker's type.
    assert_eq!(partial, completed, "{} partial literal(s) were not completed from the checker's type", partial - completed);
    assert!(divergences.is_empty(), "the backend and the checker disagree on types:\n  {}", divergences.join("\n  "));
}

/// Ratchet for the HIR (Stage 2 of the architecture plan): every function of
/// every example that type-checks lowers to a typed HIR whose nodes carry
/// their types. The unknown-type count and the invariant violations
/// (`ostrinc --hir`) may only go down; lower the limits when a gap is closed.
#[test]
fn hir_covers_the_examples_with_known_types() {
    const MAX_UNKNOWN_NODES: usize = 26;
    const MAX_VIOLATIONS: usize = 0;
    let (mut nodes, mut unknown, mut violations) = (0usize, 0usize, Vec::<String>::new());
    for path in examples() {
        let file = path.to_string_lossy().to_string();
        if !ostrinc(&["--check", &file]).status.success() {
            continue;
        }
        let out = ostrinc(&["--hir", "--quiet", &file]);
        assert!(out.status.success(), "--hir failed on {}", name_of(&path));
        for line in text(&out.stdout).lines() {
            if let Some(n) = line.strip_prefix("hir nodes: ") {
                nodes += n.trim().parse::<usize>().unwrap();
            } else if let Some(n) = line.strip_prefix("hir unknown: ") {
                unknown += n.trim().parse::<usize>().unwrap();
            } else if line.starts_with("  ") {
                violations.push(format!("{}: {}", name_of(&path), line.trim()));
            }
        }
    }
    assert!(nodes > 4000, "the HIR covered only {nodes} nodes");
    assert!(unknown <= MAX_UNKNOWN_NODES, "{unknown} of {nodes} HIR nodes have no known type (limit {MAX_UNKNOWN_NODES})");
    assert!(violations.len() <= MAX_VIOLATIONS, "HIR invariant violations:\n  {}", violations.join("\n  "));
}
