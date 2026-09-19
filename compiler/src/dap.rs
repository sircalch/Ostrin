//! `ostrinc --dap`: a Debug Adapter Protocol server over stdio for the same
//! tree-walking interpreter `--run` uses. There is no separate execution
//! thread and no bytecode VM — the interpreter's own call stack, advanced
//! statement by statement, *is* the debuggee's stack. Pausing at a
//! breakpoint means the interpreter blocks on a stdin read from right inside
//! its own recursive `eval_block` call, instead of returning; see
//! `interpreter::Debugger` and `Interpreter::enter_pause` for where that
//! actually happens. This module only handles the handshake that happens
//! *before* the program starts running: `initialize`, `launch`,
//! `setBreakpoints`, `configurationDone`.
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{json, Value};

use crate::interpreter::{Debugger, Interpreter};
use crate::modules;
use crate::package;
use crate::protocol::{read_message, write_message};
use crate::typeck::Checker;

pub fn run() -> ExitCode {
    let mut reader = BufReader::new(io::stdin());
    let mut writer = BufWriter::new(io::stdout());
    let mut seq: i64 = 0;

    let mut program: Option<PathBuf> = None;
    let mut stop_on_entry = false;
    let mut breakpoints: HashMap<String, HashSet<usize>> = HashMap::new();

    loop {
        let Some(payload) = read_message(&mut reader).unwrap_or(None) else {
            return ExitCode::SUCCESS;
        };
        let message: Value = match serde_json::from_slice(&payload) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let command = message.get("command").and_then(Value::as_str).unwrap_or_default().to_string();
        let request_seq = message.get("seq").and_then(Value::as_i64).unwrap_or(0);
        let arguments = message.get("arguments").cloned().unwrap_or(Value::Null);

        match command.as_str() {
            "initialize" => {
                send_response(
                    &mut writer,
                    &mut seq,
                    request_seq,
                    &command,
                    json!({
                        "supportsConfigurationDoneRequest": true,
                        "supportsEvaluateForHovers": true,
                        "supportsTerminateRequest": true
                    }),
                );
                send_event(&mut writer, &mut seq, "initialized", json!({}));
            }
            "launch" | "attach" => {
                program = arguments.get("program").and_then(Value::as_str).map(PathBuf::from);
                stop_on_entry = arguments.get("stopOnEntry").and_then(Value::as_bool).unwrap_or(false);
                send_response(&mut writer, &mut seq, request_seq, &command, json!({}));
            }
            "setBreakpoints" => {
                let path = arguments
                    .get("source")
                    .and_then(|source| source.get("path"))
                    .and_then(Value::as_str)
                    .map(canonical_string)
                    .unwrap_or_default();
                let requested = arguments.get("breakpoints").and_then(Value::as_array).cloned().unwrap_or_default();
                let lines: HashSet<usize> = requested
                    .iter()
                    .filter_map(|entry| entry.get("line").and_then(Value::as_u64))
                    .map(|line| line as usize)
                    .collect();
                breakpoints.insert(path, lines);
                let verified: Vec<Value> = requested
                    .iter()
                    .map(|entry| json!({ "verified": true, "line": entry.get("line").cloned().unwrap_or(Value::Null) }))
                    .collect();
                send_response(&mut writer, &mut seq, request_seq, &command, json!({ "breakpoints": verified }));
            }
            "setExceptionBreakpoints" => {
                send_response(&mut writer, &mut seq, request_seq, &command, json!({}));
            }
            "configurationDone" => {
                send_response(&mut writer, &mut seq, request_seq, &command, json!({}));
                break;
            }
            "disconnect" | "terminate" => {
                send_response(&mut writer, &mut seq, request_seq, &command, json!({}));
                return ExitCode::SUCCESS;
            }
            _ => send_response(&mut writer, &mut seq, request_seq, &command, json!({})),
        }
    }

    let Some(program) = program else {
        send_event(&mut writer, &mut seq, "output", json!({ "category": "stderr", "output": "no 'program' in the launch request\n" }));
        send_event(&mut writer, &mut seq, "terminated", json!({}));
        return ExitCode::FAILURE;
    };
    let entry_path = std::fs::canonicalize(&program).unwrap_or(program);

    let manifest_path = entry_path.parent().unwrap_or_else(|| std::path::Path::new(".")).join("ostrin.toml");
    let deps = if manifest_path.is_file() {
        package::load_manifest(&manifest_path)
            .and_then(|manifest| package::resolve_dependency_roots(&manifest))
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let items = match modules::load_project_with_deps(&entry_path, &deps) {
        Ok(items) => items,
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                send_event(&mut writer, &mut seq, "output", json!({ "category": "stderr", "output": format!("{diagnostic}\n") }));
            }
            send_event(&mut writer, &mut seq, "terminated", json!({}));
            return ExitCode::FAILURE;
        }
    };
    let typed = Checker::new().check_program_typed(&items);
    let errors = typed.errors;
    if !errors.is_empty() {
        for error in &errors {
            send_event(
                &mut writer,
                &mut seq,
                "output",
                json!({ "category": "stderr", "output": format!("error OSTRIN-{}: {}\n", error.code, error.message) }),
            );
        }
        send_event(&mut writer, &mut seq, "terminated", json!({}));
        return ExitCode::FAILURE;
    }

    let reader: Box<dyn BufRead> = Box::new(reader);
    let writer_box: Box<dyn Write> = Box::new(writer);
    let debugger = Debugger::new(reader, writer_box, breakpoints, stop_on_entry);

    let mut interpreter = Interpreter::new(&items).with_literal_kinds(typed.literal_kinds);
    interpreter.attach_debugger(debugger);
    let result = interpreter.run_main();

    let Some(mut debugger) = interpreter.take_debugger() else {
        // The debugger's transport is gone (stdin closed mid-session); there
        // is nowhere left to report the outcome to.
        return if result.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    };
    let exit_code = match result {
        Ok(_) => 0,
        Err(message) => {
            debugger.send_output(&format!("runtime error: {message}\n"));
            1
        }
    };
    debugger.send_event("exited", json!({ "exitCode": exit_code }));
    debugger.send_event("terminated", json!({}));
    if exit_code == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn canonical_string(path: &str) -> String {
    std::fs::canonicalize(path).map(|p| p.display().to_string()).unwrap_or_else(|_| path.to_string())
}

fn send_response<W: Write>(writer: &mut W, seq: &mut i64, request_seq: i64, command: &str, body: Value) {
    *seq += 1;
    let _ = write_message(
        writer,
        &json!({
            "seq": *seq, "type": "response", "request_seq": request_seq,
            "success": true, "command": command, "body": body
        }),
    );
}

fn send_event<W: Write>(writer: &mut W, seq: &mut i64, event: &str, body: Value) {
    *seq += 1;
    let _ = write_message(writer, &json!({ "seq": *seq, "type": "event", "event": event, "body": body }));
}
