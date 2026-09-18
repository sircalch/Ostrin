mod ast;
mod interpreter;
mod lexer;
mod modules;
mod package;
mod parser;
mod symbols;
mod typeck;
mod types;

use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let tokens_only = args.iter().any(|a| a == "--tokens");
    let ast_only = args.iter().any(|a| a == "--ast");
    let symbols_only = args.iter().any(|a| a == "--symbols");
    let members_only = args.iter().any(|a| a == "--members");
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

    let Some(path) = args.iter().skip(1).find(|a| !a.starts_with("--")) else {
        eprintln!("usage: ostrinc [--check|--ast|--tokens|--symbols|--members|--run] [--json] <entry_file.ostrin>");
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
                    emit_json_diagnostic(
                        None,
                        &diagnostic.message,
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
    println!("  --json        Emit machine-readable diagnostics as JSON Lines");
    println!("  -h, --help    Print this help");
    println!("  -V, --version Print the compiler version");
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
    println!(
        "{{\"kind\":\"member\",\"memberKind\":{},\"owner\":{},\"name\":{},\"detail\":{}}}",
        json_string(member.kind),
        json_string(&member.owner),
        json_string(&member.name),
        json_string(&member.detail)
    );
}

fn emit_json_binding(binding: &typeck::EditorBinding, fallback_file: &str) {
    let file = binding.source_file.as_deref().unwrap_or(fallback_file);
    println!(
        "{{\"kind\":\"binding\",\"name\":{},\"type\":{},\"function\":{},\"file\":{},\"line\":{},\"column\":{}}}",
        json_string(&binding.name),
        json_string(&binding.type_name),
        json_string(&binding.function),
        json_string(file),
        binding.span.line,
        binding.span.col
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
