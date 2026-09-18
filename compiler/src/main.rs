mod ast;
mod codegen;
mod dap;
mod interpreter;
mod lexer;
mod lsp;
mod modules;
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
    let ast_only = args.iter().any(|a| a == "--ast");
    let symbols_only = args.iter().any(|a| a == "--symbols");
    let members_only = args.iter().any(|a| a == "--members");
    let types_only = args.iter().any(|a| a == "--types");
    let typed_report = args.iter().any(|a| a == "--typed-report");
    let stdin_source = args.iter().any(|a| a == "--stdin");
    let lsp_server = args.iter().any(|a| a == "--lsp");
    let dap_server = args.iter().any(|a| a == "--dap");
    let run = args.iter().any(|a| a == "--run");
    let json = args.iter().any(|a| a == "--json");
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

    let value_flags = ["--file", "--out"];
    let mut skip_next = false;
    let Some(path) = args.iter().skip(1).find(|a| {
        if skip_next {
            skip_next = false;
            return false;
        }
        if value_flags.contains(&a.as_str()) {
            skip_next = true;
            return false;
        }
        !a.starts_with("--")
    }) else {
        eprintln!("usage: ostrinc [--check|--ast|--tokens|--symbols|--members|--types|--run] [--json] <entry_file.ostrin>");
        return ExitCode::FAILURE;
    };

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
        let roots = match package::resolve_dependency_roots(&manifest) {
            Ok(r) => r,
            Err(e) => {
                if json { emit_json_diagnostic(None, &e, Some(path), None, None); } else { eprintln!("{e}"); }
                return ExitCode::FAILURE;
            }
        };
        if let Err(e) = package::write_lockfile(manifest_path.parent().unwrap(), &manifest, &roots) {
            eprintln!("warning: could not write ostrin.lock: {e}");
        }
        roots
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

    let errors = typeck::Checker::new().check_program(&items);
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

    let emit_c = args.iter().any(|a| a == "--emit-c");
    let compile_native = args.iter().any(|a| a == "--compile");
    if emit_c || compile_native {
        return run_codegen(&items, entry_path, path, &args, emit_c, json);
    }

    if !run {
        println!("OK — no se encontraron errores de tipo ({} elemento(s)).", items.len());
        return ExitCode::SUCCESS;
    }

    match interpreter::Interpreter::new(&items).run_main() {
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
/// or stdout); `--compile` additionally hands that source to whatever C
/// compiler `codegen::find_c_compiler` finds, producing a real native
/// executable.
fn run_codegen(items: &[ast::Item], entry_path: &Path, display_path: &str, args: &[String], emit_c: bool, json: bool) -> ExitCode {
    let source = match codegen::generate(items) {
        Ok(source) => source,
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

    let Some(compiler) = codegen::find_c_compiler() else {
        eprintln!("error: no GNU-compatible C compiler found (checked $OSTRIN_CC, cc, gcc, clang)");
        return ExitCode::FAILURE;
    };
    let output_path = argument_value(args, "--out").unwrap_or_else(|| {
        let stem = entry_path.file_stem().and_then(|s| s.to_str()).unwrap_or("a");
        let dir = entry_path.parent().unwrap_or_else(|| Path::new("."));
        let exe_name = if cfg!(windows) { format!("{stem}.exe") } else { stem.to_string() };
        dir.join(exe_name).display().to_string()
    });
    let c_path = env::temp_dir().join(format!("ostrin_codegen_{}.c", std::process::id()));
    if let Err(e) = fs::write(&c_path, &source) {
        eprintln!("error: could not write temporary C source: {e}");
        return ExitCode::FAILURE;
    }
    let status = std::process::Command::new(&compiler)
        .arg(&c_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-O2")
        .status();
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

fn print_help() {
    println!("ostrinc 0.1.0 — compiler and interpreter for Ostrin");
    println!();
    println!("Usage:\n  ostrinc [OPTIONS] <entry_file.ostrin>");
    println!();
    println!("Options:");
    println!("  --check       Type-check the project (the default)");
    println!("  --run         Type-check and run the entry file");
    println!("  --ast         Print the parsed AST");
    println!("  --tokens      Print lexer tokens");
    println!("  --symbols     Print source symbols and signatures");
    println!("  --members     Print type members and local bindings for editor tools");
    println!("  --types       Print inferred expression types for editor tools");
    println!("  --stdin       Read source from stdin for editor integrations");
    println!("  --file PATH   Associate stdin source with a source path");
    println!("  --lsp         Run the language server over stdio");
    println!("  --dap         Run the debug adapter over stdio");
    println!("  --emit-c      Transpile to C (a supported subset only; see docs) instead of running");
    println!("  --compile     Transpile to C and compile it to a native executable");
    println!("  --out PATH    Output path for --emit-c/--compile (defaults: stdout / <entry>.exe next to the source)");
    println!("  --json        Emit machine-readable diagnostics as JSON Lines");
    println!("  -h, --help    Print this help");
    println!("  -V, --version Print the compiler version");
}

fn argument_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
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
