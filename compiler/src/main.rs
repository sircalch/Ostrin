mod ast;
mod codegen;
mod dap;
mod fmt;
mod hir;
mod hir_c;
mod interpreter;
mod ir;
mod ir_c;
mod lexer;
mod lsp;
mod modules;
mod ownership;
mod package;
mod parser;
mod protocol;
mod symbols;
mod typeck;
mod types;

use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_C_SOURCE_ID: AtomicU64 = AtomicU64::new(0);

/// The tree-walking interpreter recurses once per nested expression, and
/// its frames are large (especially in debug builds), so deeply nested calls
/// such as `print(f(try g()).unwrap())` overflowed the default 1 MiB main
/// thread stack on Windows. Everything runs on a thread with a much larger
/// stack instead.
fn main() -> ExitCode {
    match std::thread::Builder::new().stack_size(512 * 1024 * 1024).spawn(real_main) {
        Ok(handle) => handle.join().unwrap_or(ExitCode::FAILURE),
        Err(_) => real_main(),
    }
}

fn real_main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let tokens_only = args.iter().any(|a| a == "--tokens");
    let fmt_mode = args.iter().any(|a| a == "--fmt");
    let ast_only = args.iter().any(|a| a == "--ast");
    let symbols_only = args.iter().any(|a| a == "--symbols");
    let members_only = args.iter().any(|a| a == "--members");
    let types_only = args.iter().any(|a| a == "--types");
    let typed_report = args.iter().any(|a| a == "--typed-report");
    let native_type_report = args.iter().any(|a| a == "--native-type-report");
    let hir_mode = args.iter().any(|a| a == "--hir");
    let ir_mode = args.iter().any(|a| a == "--ir");
    let ownership_report = args.iter().any(|a| a == "--ownership-report");
    let ownership_ir = args.iter().any(|a| a == "--ownership-ir");
    let ownership_check = args.iter().any(|a| a == "--ownership-check");
    let stdin_source = args.iter().any(|a| a == "--stdin");
    let lsp_server = args.iter().any(|a| a == "--lsp");
    let dap_server = args.iter().any(|a| a == "--dap");
    let run = args.iter().any(|a| a == "--run");
    let test_mode = args.iter().any(|a| a == "--test");
    let json = args.iter().any(|a| a == "--json");
    let fetch_packages = args.iter().any(|a| a == "--fetch");
    let locked_packages = args.iter().any(|a| a == "--locked");
    let help = args.iter().any(|a| a == "--help" || a == "-h");
    let version = args.iter().any(|a| a == "--version" || a == "-V");

    if help {
        print_help();
        return ExitCode::SUCCESS;
    }
    if version {
        println!("ostrinc 0.1.0");
        return ExitCode::SUCCESS;
    }

    if let Some(directory) = argument_value(&args, "--new") {
        return scaffold_project(&directory);
    }

    if lsp_server {
        return lsp::run();
    }

    if dap_server {
        return dap::run();
    }

    if stdin_source {
        let source_file = argument_value(&args, "--file").unwrap_or_else(|| "<stdin>".to_string());
        return check_stdin(&source_file, json);
    }

    let value_flags = ["--file", "--out", "--project", "--target", "--new"];
    let mut skip_next = false;
    let positional = args.iter().skip(1).find(|a| {
        if skip_next {
            skip_next = false;
            return false;
        }
        if value_flags.contains(&a.as_str()) {
            skip_next = true;
            return false;
        }
        !a.starts_with("--")
    }).cloned();
    let path_owned = if let Some(path) = positional {
        path
    } else if let Some(project) = argument_value(&args, "--project") {
        match project_entry_path(&project) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        eprintln!("usage: ostrinc [OPTIONS] [entry_file.ostrin] (or --project DIR)");
        return ExitCode::FAILURE;
    };
    let path = path_owned.as_str();

    if fmt_mode {
        return format_file(path, args.iter().any(|a| a == "--write"), args.iter().any(|a| a == "--check"));
    }

    if tokens_only {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: could not read '{path}': {e}");
                return ExitCode::FAILURE;
            }
        };
        return match lexer::Lexer::new(&source).tokenize() {
            Ok(tokens) => {
                for token in &tokens {
                    println!("{:4}:{:<4} {:<16} {:?}", token.line, token.col, format!("{:?}", token.kind), token.lexeme);
                }
                println!("\n{} tokens", tokens.len());
                ExitCode::SUCCESS
            }
            Err(e) => {
                if json {
                    emit_json_diagnostic(None, &format!("lex error: {}", e.message), Some(path), Some(e.line), Some(e.col));
                } else {
                    eprintln!("lex error at {}:{}: {}", e.line, e.col, e.message);
                }
                ExitCode::FAILURE
            }
        };
    }

    let entry_path = Path::new(path);
    let manifest_path = entry_path.parent().unwrap_or_else(|| Path::new(".")).join("ostrin.toml");
    let deps = if manifest_path.is_file() {
        let manifest = match package::load_manifest(&manifest_path) {
            Ok(m) => m,
            Err(e) => {
                if json { emit_json_diagnostic(None, &e, Some(path), None, None); } else { eprintln!("{e}"); }
                return ExitCode::FAILURE;
            }
        };
        let resolved = match package::resolve_dependency_graph(
            &manifest,
            manifest_path.parent().unwrap_or_else(|| Path::new(".")),
            fetch_packages,
            locked_packages,
        ) {
            Ok(r) => r,
            Err(e) => {
                if json { emit_json_diagnostic(None, &e, Some(path), None, None); } else { eprintln!("{e}"); }
                return ExitCode::FAILURE;
            }
        };
        if !locked_packages {
            if let Err(e) = package::write_lockfile(manifest_path.parent().unwrap(), &manifest, &resolved) {
                eprintln!("warning: could not write ostrin.lock: {e}");
            }
        }
        resolved.roots()
    } else {
        std::collections::HashMap::new()
    };

    let items = match modules::load_project_with_deps(entry_path, &deps) {
        Ok(items) => items,
        Err(messages) => {
            for diagnostic in &messages {
                if json {
                    // A leading `OSTRIN-E####: ` is the stable code, not message text.
                    let (code, message) = match diagnostic.message.split_once(": ") {
                        Some((code, rest)) if code.starts_with("OSTRIN-E") => (Some(code), rest),
                        _ => (None, diagnostic.message.as_str()),
                    };
                    emit_json_diagnostic(
                        code,
                        message,
                        diagnostic.file.as_deref().or(Some(path)),
                        diagnostic.line,
                        diagnostic.col,
                    );
                } else {
                    eprintln!("{diagnostic}");
                }
            }
            if !json { eprintln!("\n{} error(es) de análisis", messages.len()); }
            return ExitCode::FAILURE;
        }
    };

    if ast_only {
        for item in &items {
            println!("{item:#?}");
        }
        return ExitCode::SUCCESS;
    }

    if members_only {
        let (_errors, bindings) = typeck::Checker::new().check_program_with_bindings(&items);
        for member in symbols::collect_members(&items) {
            if json {
                emit_json_member(&member);
            } else {
                println!("{}.{} {}", member.owner, member.name, member.detail);
            }
        }
        for binding in bindings {
            if json {
                emit_json_binding(&binding, path);
            } else {
                println!(
                    "binding {}: {} ({}:{})",
                    binding.name, binding.type_name, binding.span.line, binding.span.col
                );
            }
        }
        return ExitCode::SUCCESS;
    }

    if native_type_report {
        // Compares the native backend's own type inference with the checker's.
        let typed = typeck::Checker::new().check_program_typed(&items);
        return match codegen::generate_with_report(&items, &typed) {
            Ok((_, report)) => {
                println!("agreed: {}", report.agreed);
                println!("node-agreed: {}", report.node_agreed);
                println!("node-unchecked: {}", report.node_unchecked);
                println!("ir-generated: {}", report.ir_generated);
                println!("hir-generated: {}", report.hir_generated);
                println!("partial: {}", report.partial);
                println!("completed: {}", report.completed);
                println!("generic-calls-from-checker: {}", report.calls_from_checker);
                println!("generic-calls-inferred: {}", report.calls_inferred);
                println!("unchecked: {}", report.unchecked);
                println!("divergences: {}", report.divergences.len());
                for line in &report.divergences {
                    println!("  {line}");
                }
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        };
    }

    if typed_report {
        // Audit of the checker's typed-expression table: how many expressions
        // have a fully determined type, and where the checker gave up.
        let typed = typeck::Checker::new().check_program_typed(&items);
        let total = typed.expr_types.len();
        let mut unknown: Vec<(String, usize, usize)> = typed
            .expr_types
            .iter()
            .filter(|(_, ty)| types::ty_contains_unknown(ty))
            .map(|(key, _)| (key.file.clone().unwrap_or_else(|| path.to_string()), key.start.line, key.start.col))
            .collect();
        unknown.sort();
        println!("expressions: {total}");
        println!("errors: {}", typed.errors.len());
        println!("unknown: {}", unknown.len());
        for (file, line, col) in &unknown {
            println!("  {file}:{line}:{col}");
        }
        return ExitCode::SUCCESS;
    }

    if types_only {
        let (_errors, _bindings, expressions) =
            typeck::Checker::new().check_program_with_editor_data(&items);
        for expression in expressions {
            let file = expression.source_file.as_deref().unwrap_or(path);
            if json {
                emit_json_expression(&expression, file);
            } else {
                println!(
                    "{}:{}:{} {} -> {}",
                    file,
                    expression.span.line,
                    expression.span.col,
                    expression.function,
                    expression.type_name
                );
            }
        }
        return ExitCode::SUCCESS;
    }

    if symbols_only {
        for symbol in symbols::collect(&items) {
            if json {
                emit_json_symbol(&symbol, path);
            } else {
                println!(
                    "{}:{} {} {}",
                    symbol.span.line, symbol.span.col, symbol.kind, symbol.detail
                );
            }
        }
        return ExitCode::SUCCESS;
    }

    let typed_program = typeck::Checker::new().check_program_typed(&items);
    let errors = typed_program.errors.clone();
    if !errors.is_empty() {
        for e in &errors {
            if json {
                emit_json_diagnostic(
                    Some(e.code),
                    &e.message,
                    e.source_file.as_deref().or(Some(path)),
                    e.span.map(|span| span.line),
                    e.span.map(|span| span.col),
                );
            } else {
                eprintln!("error OSTRIN-{}: {}", e.code, e.message);
            }
        }
        if !json { eprintln!("\n{} error(es)", errors.len()); }
        return ExitCode::FAILURE;
    }

    if hir_mode {
        // Lowers every function to the typed HIR, verifies its invariants and prints it.
        let program = hir::lower(&items, &typed_program);
        let generic_functions: std::collections::HashSet<String> = items
            .iter()
            .filter_map(|item| match item {
                ast::Item::Function(f) if !f.generics.is_empty() => Some(f.name.clone()),
                _ => None,
            })
            .collect();
        let report = hir::verify(&program, &generic_functions);
        if !args.iter().any(|a| a == "--quiet") {
            print!("{}", hir::dump(&program));
        }
        println!("hir functions: {}", program.functions.len());
        println!("hir nodes: {}", report.nodes);
        println!("hir unknown: {}", report.unknown);
        println!("hir violations: {}", report.violations.len());
        for v in &report.violations {
            println!("  {}: {}", v.function, v.message);
        }
        return ExitCode::SUCCESS;
    }

    if ir_mode {
        let hir_program = hir::lower(&items, &typed_program);
        let ir_program = ir::lower(&hir_program);
        let report = ir::verify(&ir_program);
        if !args.iter().any(|a| a == "--quiet") {
            print!("{}", ir::dump(&ir_program));
        }
        println!("ir functions: {}", ir_program.functions.len());
        println!("ir blocks: {}", report.blocks);
        println!("ir instructions: {}", report.instructions);
        println!("ir opaque: {}", report.opaque);
        println!("ir unterminated: {}", report.unterminated);
        println!("ir violations: {}", report.violations.len());
        for violation in &report.violations {
            println!("  {violation}");
        }
        return if report.violations.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }

    if ownership_report {
        let hir_program = hir::lower(&items, &typed_program);
        let ir_program = ir::lower(&hir_program);
        print!("{}", ownership::dump(&ownership::analyze(&ir_program)));
        return ExitCode::SUCCESS;
    }

    if ownership_check {
        let hir_program = hir::lower(&items, &typed_program);
        let ir_program = ir::lower(&hir_program);
        let movable_types = ownership::movable_types(&items);
        let violations = ownership::check_moves_for_types(&ir_program, &movable_types);
        print!("{}", ownership::dump_moves(&violations));
        return if violations.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }

    if ownership_ir {
        let hir_program = hir::lower(&items, &typed_program);
        let ir_program = ir::lower(&hir_program);
        let (lowered, summary) = ownership::lower_linear(&ir_program);
        print!("{}", ir::dump(&lowered));
        print!("{}", ownership::dump_lowering(&summary));
        return if ir::verify(&lowered).violations.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }

    // E1101 is part of the normal compiler contract now: every executable
    // entry point sees the same ownership check before code generation or
    // interpretation. The explicit --ownership-check mode above remains a
    // useful report-only inspection of the same facts.
    let hir_program = hir::lower(&items, &typed_program);
    let ir_program = ir::lower(&hir_program);
    let movable_types = ownership::movable_types(&items);
    let move_violations = ownership::check_moves_for_types(&ir_program, &movable_types);
    if !move_violations.is_empty() {
        for violation in &move_violations {
            let message = format!(
                "value %{} in function '{}' of type '{}' was sent through a channel at bb{}:{} and used after channel send at bb{}:{}",
                violation.value,
                violation.function,
                violation.ty.describe(),
                violation.send_block,
                violation.send_instruction,
                violation.use_block,
                violation.use_instruction,
            );
            if json {
                emit_json_diagnostic(Some("E1101"), &message, Some(path), None, None);
            } else {
                eprintln!("error OSTRIN-E1101: {message}");
            }
        }
        return ExitCode::FAILURE;
    }

    let emit_c = args.iter().any(|a| a == "--emit-c");
    let compile_native = args.iter().any(|a| a == "--compile");
    let leak_check = args.iter().any(|a| a == "--leak-check");
    let native_threads = args.iter().any(|a| a == "--native-threads");
    let target = argument_value(&args, "--target").unwrap_or_else(|| "native".to_string());
    if target != "native" && target != "wasm32-wasi" {
        eprintln!("error: unsupported compilation target '{target}' (expected native or wasm32-wasi)");
        return ExitCode::FAILURE;
    }
    if target != "native" && !(emit_c || compile_native) {
        eprintln!("error: --target requires --emit-c or --compile");
        return ExitCode::FAILURE;
    }
    if target == "wasm32-wasi" && native_threads {
        eprintln!("error: --native-threads is not supported for target wasm32-wasi");
        return ExitCode::FAILURE;
    }
    if emit_c || compile_native {
        return run_codegen(&items, &typed_program, entry_path, path, &args, emit_c, json, leak_check, native_threads, &target);
    }
    if leak_check || native_threads {
        eprintln!("error: --leak-check and --native-threads require --emit-c or --compile");
        return ExitCode::FAILURE;
    }

    if test_mode {
        // Runs every zero-argument `test_*` function, each in a fresh interpreter.
        let names = interpreter::Interpreter::new(&items).with_literal_kinds(typed_program.literal_kinds.clone()).test_function_names();
        if names.is_empty() {
            eprintln!("error: no 'test_*' functions found in '{path}'");
            return ExitCode::FAILURE;
        }
        let mut failed = 0usize;
        for name in &names {
            match interpreter::Interpreter::new(&items).with_literal_kinds(typed_program.literal_kinds.clone()).run_function(name) {
                Ok(_) => println!("test {name} ... ok"),
                Err(message) => {
                    failed += 1;
                    println!("test {name} ... FAILED ({message})");
                }
            }
        }
        println!("
test result: {}. {} passed; {failed} failed", if failed == 0 { "ok" } else { "FAILED" }, names.len() - failed);
        return if failed == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }

    if !run {
        println!("OK — no se encontraron errores de tipo ({} elemento(s)).", items.len());
        return ExitCode::SUCCESS;
    }

    match interpreter::Interpreter::new(&items).with_literal_kinds(typed_program.literal_kinds.clone()).run_main() {
        Ok(_) => ExitCode::SUCCESS,
        Err(msg) => {
            if json {
                emit_json_diagnostic(None, &format!("runtime error: {msg}"), Some(path), None, None);
            } else {
                eprintln!("runtime error: {msg}");
            }
            ExitCode::FAILURE
        }
    }
}

/// Handles `--emit-c` and `--compile` once the program has already
/// type-checked cleanly. `--emit-c` just writes the generated C (to `--out`
/// or stdout); `--compile` additionally hands that source to a target-specific
/// C compiler, producing either a native executable or a WASI command module.
fn run_codegen(
    items: &[ast::Item],
    typed: &typeck::TypedProgram,
    entry_path: &Path,
    display_path: &str,
    args: &[String],
    emit_c: bool,
    json: bool,
    leak_check: bool,
    native_threads: bool,
    target: &str,
) -> ExitCode {
    let source = match codegen::generate_with_native_options(items, typed, leak_check, native_threads) {
        Ok((source, _)) => source,
        Err(message) => {
            if json {
                emit_json_diagnostic(None, &message, Some(display_path), None, None);
            } else {
                eprintln!("error: {message}");
            }
            return ExitCode::FAILURE;
        }
    };

    if emit_c {
        return match argument_value(args, "--out") {
            Some(out_path) => match fs::write(&out_path, &source) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: could not write '{out_path}': {e}");
                    ExitCode::FAILURE
                }
            },
            None => {
                print!("{source}");
                ExitCode::SUCCESS
            }
        };
    }

    let Some(compiler) = codegen::find_c_compiler_for_target(target) else {
        if target == "wasm32-wasi" {
            eprintln!("error: no WASI C compiler found (set $OSTRIN_WASI_CC or install clang/wasi-sdk)");
        } else {
            eprintln!("error: no GNU-compatible C compiler found (checked $OSTRIN_CC, cc, gcc, clang)");
        }
        return ExitCode::FAILURE;
    };
    let output_path = argument_value(args, "--out").unwrap_or_else(|| {
        let stem = entry_path.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
        let dir = entry_path.parent().unwrap_or_else(|| Path::new("."));
        let exe_name = if target == "wasm32-wasi" {
            format!("{stem}.wasm")
        } else if cfg!(windows) {
            format!("{stem}.exe")
        } else {
            stem.to_string()
        };
        dir.join(exe_name).display().to_string()
    });
    let c_source_id = NEXT_C_SOURCE_ID.fetch_add(1, Ordering::Relaxed);
    let c_path = env::temp_dir().join(format!("ostrin_codegen_{}_{}.c", std::process::id(), c_source_id));
    if let Err(e) = fs::write(&c_path, &source) {
        eprintln!("error: could not write temporary C source: {e}");
        return ExitCode::FAILURE;
    }
    let mut command = std::process::Command::new(&compiler);
    command.arg(&c_path).arg("-o").arg(&output_path).arg("-O2");
    if target == "wasm32-wasi" {
        // wasi-sdk 34 no longer defaults the legacy `wasm32-wasi` triple to
        // the right sysroot layout. Keep the CLI spelling for compatibility,
        // but always ask Clang for the current WASI Preview 1 target. Ostrin's
        // cooperative task cancellation uses setjmp/longjmp, which wasi-libc
        // implements with the WebAssembly exception-handling proposal.
        command.arg("--target=wasm32-wasip1");
        command.arg("-Werror=shift-count-overflow");
        command.args(["-mllvm", "-wasm-enable-sjlj"]);
        if let Ok(sysroot) = env::var("OSTRIN_WASI_SYSROOT") {
            command.arg(format!("--sysroot={sysroot}"));
        }
    } else {
        command.args((!cfg!(windows)).then_some("-pthread"));
        // POSIX toolchains keep libm separate from libc.  The generated
        // runtime and package code use sqrt/round/floor, so native Unix
        // programs must link it explicitly (Windows math symbols are part
        // of the platform C runtime).
        command.args((!cfg!(windows)).then_some("-lm"));
    }
    // No fused multiply-add: results must match the interpreter bit for bit.
    let status = command.arg("-ffp-contract=off").status();
    let _ = fs::remove_file(&c_path);
    match status {
        Ok(status) if status.success() => {
            println!("compiled: {output_path}");
            ExitCode::SUCCESS
        }
        Ok(status) => {
            eprintln!("error: '{compiler}' exited with {status}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: could not run '{compiler}': {e}");
            ExitCode::FAILURE
        }
    }
}

/// `--fmt`: print the formatted file; `--write` rewrites it; `--check` fails if
/// it is not already formatted.
fn format_file(path: &str, write: bool, check: bool) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read '{path}': {e}");
            return ExitCode::FAILURE;
        }
    };
    let formatted = match fmt::format_source(&source) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("error: cannot format '{path}': {error}");
            return ExitCode::FAILURE;
        }
    };
    let changed = formatted != source.replace("\r\n", "\n");
    if check {
        if changed {
            eprintln!("{path}: needs formatting");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }
    if write {
        if changed {
            if let Err(e) = fs::write(path, &formatted) {
                eprintln!("error: could not write '{path}': {e}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }
    print!("{formatted}");
    ExitCode::SUCCESS
}

/// `--new DIR`: create a project (manifest, entry module with a test, `.gitignore`).
fn scaffold_project(directory: &str) -> ExitCode {
    let root = Path::new(directory);
    let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        eprintln!("error: '{directory}' does not end in a valid project name (letters, digits, '-' and '_')");
        return ExitCode::FAILURE;
    }
    if root.exists() && fs::read_dir(root).map(|mut entries| entries.next().is_some()).unwrap_or(true) {
        eprintln!("error: '{directory}' already exists and is not empty");
        return ExitCode::FAILURE;
    }
    let manifest = format!("[package]
name = \"{name}\"
version = \"0.1.0\"
entry = \"main.ostrin\"

[dependencies]
");
    let entry = "import std.math

fn greet(name: String) -> String {
    \"Hello, \" + name + \"!\"
}

fn main() -> Void {
    print(greet(\"Ostrin\"))
    print(math.max(2, 3))
}

fn test_greet() -> Void {
    assert_eq(greet(\"you\"), \"Hello, you!\")
}
";
    let files = [("ostrin.toml", manifest.as_str()), ("main.ostrin", entry), (".gitignore", "*.exe
*.c
")];
    if let Err(error) = fs::create_dir_all(root) {
        eprintln!("error: could not create '{directory}': {error}");
        return ExitCode::FAILURE;
    }
    for (file, content) in files {
        if let Err(error) = fs::write(root.join(file), content) {
            eprintln!("error: could not write '{}': {error}", root.join(file).display());
            return ExitCode::FAILURE;
        }
    }
    println!("created project '{name}' in {directory}");
    println!("  cd {directory} && ostrinc --project . --run");
    println!("  ostrinc --test {directory}/main.ostrin");
    ExitCode::SUCCESS
}

fn print_help() {
    println!("ostrinc 0.1.0 — compiler and interpreter for Ostrin");
    println!();
    println!("Usage:\n  ostrinc [OPTIONS] [entry_file.ostrin]");
    println!();
    println!("Options:");
    println!("  --check       Type-check the project (the default)");
    println!("  --run         Type-check and run the entry file");
    println!("  --ast         Print the parsed AST");
    println!("  --tokens      Print lexer tokens");
    println!("  --new DIR     Create a new project in DIR (manifest, main.ostrin with a test)");
    println!("  --fmt         Print the formatted file (--write to rewrite, --check to verify)");
    println!("  --symbols     Print source symbols and signatures");
    println!("  --members     Print type members and local bindings for editor tools");
    println!("  --types       Print inferred expression types for editor tools");
    println!("  --stdin       Read source from stdin for editor integrations");
    println!("  --file PATH   Associate stdin source with a source path");
    println!("  --lsp         Run the language server over stdio");
    println!("  --dap         Run the debug adapter over stdio");
    println!("  --hir         Print the typed, verified HIR");
    println!("  --ir          Lower HIR to explicit basic blocks and temporaries");
    println!("  --ownership-report  Report conservative managed values and last-use candidates");
    println!("  --ownership-ir      Insert proof-guided linear release markers into a cloned IR");
    println!("  --ownership-check   Detect managed values used after channel send (E1101)");
    println!("  --leak-check       Report native allocations before process cleanup (with --emit-c/--compile)");
    println!("  --native-threads   Use OS threads and blocking native channels (with --emit-c/--compile)");
    println!("  --emit-c      Transpile to C (a supported subset only; see docs) instead of running");
    println!("  --compile     Transpile to C and compile it to a native executable or WASI module");
    println!("  --target NAME Select native (default) or wasm32-wasi for --emit-c/--compile");
    println!("  --project DIR Compile the entry declared by DIR/ostrin.toml");
    println!("  --fetch       Explicitly clone/update Git dependencies for this project");
    println!("  --locked      Require the existing ostrin.lock without rewriting it");
    println!("  --out PATH    Output path for --emit-c/--compile (defaults: stdout / target-specific entry output)");
    println!("  --json        Emit machine-readable diagnostics as JSON Lines");
    println!("  -h, --help    Print this help");
    println!("  -V, --version Print the compiler version");
}

fn argument_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn project_entry_path(project: &str) -> Result<String, String> {
    let candidate = Path::new(project);
    let manifest_path = if candidate.is_file() { candidate.to_path_buf() } else { candidate.join("ostrin.toml") };
    let manifest = package::load_manifest(&manifest_path)?;
    let root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    Ok(root.join(manifest.entry).display().to_string())
}

fn check_stdin(source_file: &str, json: bool) -> ExitCode {
    let mut source = String::new();
    if let Err(error) = io::stdin().read_to_string(&mut source) {
        if json {
            emit_json_diagnostic(None, &format!("could not read stdin: {error}"), Some(source_file), None, None);
        } else {
            eprintln!("could not read stdin: {error}");
        }
        return ExitCode::FAILURE;
    }

    let tokens = match lexer::Lexer::new(&source).tokenize() {
        Ok(tokens) => tokens,
        Err(error) => {
            if json {
                emit_json_diagnostic(
                    Some("OSTRIN-E0002"),
                    &format!("lex error: {}", error.message),
                    Some(source_file),
                    Some(error.line),
                    Some(error.col),
                );
            } else {
                eprintln!("error OSTRIN-E0002: lex error at {}:{}: {}", error.line, error.col, error.message);
            }
            return ExitCode::FAILURE;
        }
    };

    let (items, parse_errors) = parser::Parser::new(tokens).parse_program();
    for error in &parse_errors {
        if json {
            emit_json_diagnostic(
                Some("OSTRIN-E0001"),
                &format!("parse error: {}", error.message),
                Some(source_file),
                Some(error.line),
                Some(error.col),
            );
        } else {
            eprintln!("error OSTRIN-E0001: parse error at {}:{}: {}", error.line, error.col, error.message);
        }
    }

    let errors = typeck::Checker::new().check_program(&items);
    for error in &errors {
        if json {
            emit_json_diagnostic(
                Some(error.code),
                &error.message,
                error.source_file.as_deref().or(Some(source_file)),
                error.span.map(|span| span.line),
                error.span.map(|span| span.col),
            );
        } else {
            eprintln!("error OSTRIN-{}: {}", error.code, error.message);
        }
    }

    let total_errors = parse_errors.len() + errors.len();
    if total_errors == 0 {
        if !json {
            println!("OK — no se encontraron errores de tipo.");
        }
        ExitCode::SUCCESS
    } else {
        if !json {
            eprintln!("\n{} error(es)", total_errors);
        }
        ExitCode::FAILURE
    }
}

fn emit_json_diagnostic(
    code: Option<&str>,
    message: &str,
    file: Option<&str>,
    line: Option<usize>,
    col: Option<usize>,
) {
    let code = code.map_or_else(|| "null".to_string(), json_string);
    let file = file.map_or_else(|| "null".to_string(), json_string);
    let line = line.map_or_else(|| "null".to_string(), |value| value.to_string());
    let col = col.map_or_else(|| "null".to_string(), |value| value.to_string());
    println!(
        "{{\"severity\":\"error\",\"code\":{},\"message\":{},\"file\":{},\"line\":{},\"column\":{}}}",
        code,
        json_string(message),
        file,
        line,
        col
    );
}

fn emit_json_symbol(symbol: &symbols::Symbol, fallback_file: &str) {
    let file = symbol.source_file.as_deref().unwrap_or(fallback_file);
    println!(
        "{{\"kind\":{},\"name\":{},\"detail\":{},\"file\":{},\"line\":{},\"column\":{}}}",
        json_string(symbol.kind),
        json_string(&symbol.name),
        json_string(&symbol.detail),
        json_string(file),
        symbol.span.line,
        symbol.span.col
    );
}

fn emit_json_member(member: &symbols::MemberSymbol) {
    let result_type = member
        .result_type
        .as_deref()
        .map_or_else(|| "null".to_string(), json_string);
    let owner_generics = format!(
        "[{}]",
        member
            .owner_generics
            .iter()
            .map(|generic| json_string(generic))
            .collect::<Vec<_>>()
            .join(",")
    );
    let file = member
        .source_file
        .as_deref()
        .map_or_else(|| "null".to_string(), json_string);
    let line = member
        .span
        .map_or_else(|| "null".to_string(), |span| span.line.to_string());
    let column = member
        .span
        .map_or_else(|| "null".to_string(), |span| span.col.to_string());
    println!(
        "{{\"kind\":\"member\",\"memberKind\":{},\"owner\":{},\"name\":{},\"detail\":{},\"resultType\":{},\"ownerGenerics\":{},\"file\":{},\"line\":{},\"column\":{}}}",
        json_string(member.kind),
        json_string(&member.owner),
        json_string(&member.name),
        json_string(&member.detail),
        result_type,
        owner_generics,
        file,
        line,
        column
    );
}

fn emit_json_binding(binding: &typeck::EditorBinding, fallback_file: &str) {
    let file = binding.source_file.as_deref().unwrap_or(fallback_file);
    println!(
        "{{\"kind\":\"binding\",\"name\":{},\"type\":{},\"function\":{},\"scopeDepth\":{},\"file\":{},\"line\":{},\"column\":{}}}",
        json_string(&binding.name),
        json_string(&binding.type_name),
        json_string(&binding.function),
        binding.scope_depth,
        json_string(file),
        binding.span.line,
        binding.span.col
    );
}

fn emit_json_expression(expression: &typeck::EditorExpression, fallback_file: &str) {
    let file = expression.source_file.as_deref().unwrap_or(fallback_file);
    println!(
        "{{\"kind\":\"expression\",\"type\":{},\"function\":{},\"file\":{},\"line\":{},\"column\":{},\"endLine\":{},\"endColumn\":{}}}",
        json_string(&expression.type_name),
        json_string(&expression.function),
        json_string(file),
        expression.span.line,
        expression.span.col,
        expression.end.line,
        expression.end.col
    );
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if c.is_control() => escaped.push_str(&format!("\\u{:04x}", c as u32)),
            c => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}
