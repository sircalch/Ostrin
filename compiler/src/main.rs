mod ast;
mod interpreter;
mod lexer;
mod modules;
mod package;
mod parser;
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
    let run = args.iter().any(|a| a == "--run");
    let Some(path) = args.iter().skip(1).find(|a| !a.starts_with("--")) else {
        eprintln!("usage: ostrinc [--tokens|--ast|--run] <entry_file.ostrin>");
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
                eprintln!("lex error at {}:{}: {}", e.line, e.col, e.message);
                ExitCode::FAILURE
            }
        };
    }

    let entry_path = Path::new(path);
    let manifest_path = entry_path.parent().unwrap_or_else(|| Path::new(".")).join("ostrin.toml");
    let deps = if manifest_path.is_file() {
        let manifest = match package::load_manifest(&manifest_path) {
            Ok(m) => m,
            Err(e) => { eprintln!("{e}"); return ExitCode::FAILURE; }
        };
        let roots = match package::resolve_dependency_roots(&manifest) {
            Ok(r) => r,
            Err(e) => { eprintln!("{e}"); return ExitCode::FAILURE; }
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
            for msg in &messages {
                eprintln!("{msg}");
            }
            eprintln!("\n{} error(es) de análisis", messages.len());
            return ExitCode::FAILURE;
        }
    };

    if ast_only {
        for item in &items {
            println!("{item:#?}");
        }
        return ExitCode::SUCCESS;
    }

    let errors = typeck::Checker::new().check_program(&items);
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("error OSTRIN-{}: {}", e.code, e.message);
        }
        eprintln!("\n{} error(es)", errors.len());
        return ExitCode::FAILURE;
    }

    if !run {
        println!("OK — no se encontraron errores de tipo ({} elemento(s)).", items.len());
        return ExitCode::SUCCESS;
    }

    match interpreter::Interpreter::new(&items).run_main() {
        Ok(_) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("runtime error: {msg}");
            ExitCode::FAILURE
        }
    }
}
