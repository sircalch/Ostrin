use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};

use serde_json::json;

fn example_path(rel: &str) -> String {
    format!("{}/../examples/{}", env!("CARGO_MANIFEST_DIR"), rel)
}

/// Builds the same `file://` URI shape an editor would send for a real file
/// on disk, percent-encoding the parts (like the space in this repo's own
/// "Lenguaje nuevo" directory name) the way `lsp.rs`'s `path_to_uri` does.
fn file_uri(rel: &str) -> String {
    let path = fs::canonicalize(example_path(rel)).expect("example file must exist on disk");
    let normalized = path.display().to_string().replace('\\', "/");
    let mut out = String::from("file://");
    if !normalized.starts_with('/') {
        out.push('/');
    }
    for ch in normalized.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' | ':' => out.push(ch),
            other => out.push_str(&format!("%{:02X}", other as u32)),
        }
    }
    out
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ostrinc"))
        .args(args)
        .output()
        .expect("failed to run ostrinc")
}

fn run_with_env(args: &[&str], key: &str, value: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ostrinc"))
        .args(args)
        .env(key, value)
        .output()
        .expect("failed to run ostrinc with environment")
}

fn run_stdin(args: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ostrinc"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run ostrinc with stdin");
    child
        .stdin
        .take()
        .expect("missing stdin pipe")
        .write_all(source.as_bytes())
        .expect("failed to write source to ostrinc");
    child.wait_with_output().expect("failed to collect ostrinc output")
}

fn lsp_frame(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body).into_bytes()
}

fn run_lsp(messages: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ostrinc"))
        .arg("--lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run ostrinc LSP");
    {
        let mut stdin = child.stdin.take().expect("missing LSP stdin pipe");
        for message in messages {
            stdin.write_all(&lsp_frame(message)).expect("failed to write LSP message");
        }
    }
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .expect("missing LSP stdout pipe")
        .read_to_end(&mut output)
        .expect("failed to read LSP output");
    let status = child.wait().expect("failed to wait for LSP");
    Output {
        status,
        stdout: output,
        stderr: Vec::new(),
    }
}

fn run_dap(messages: &[String]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ostrinc"))
        .arg("--dap")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run ostrinc DAP");
    {
        let mut stdin = child.stdin.take().expect("missing DAP stdin pipe");
        for message in messages {
            stdin.write_all(&lsp_frame(message)).expect("failed to write DAP message");
        }
    }
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .expect("missing DAP stdout pipe")
        .read_to_end(&mut output)
        .expect("failed to read DAP output");
    let status = child.wait().expect("failed to wait for DAP");
    Output { status, stdout: output, stderr: Vec::new() }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn hello_type_checks_ok() {
    let out = run(&[&example_path("hello.ostrin")]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("OK"));
}

#[test]
fn record_fields_are_checked_and_generic_fields_are_substituted() {
    let out = run(&["--run", &example_path("field_access.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["9", "seven"]);
}

#[test]
fn invalid_record_field_access_and_assignment_are_rejected() {
    let out = run(&[&example_path("field_access_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1043"), "missing field diagnostic: {err}");
    assert!(err.contains("missing"), "missing field name: {err}");
    assert!(err.contains("E1041"), "missing field assignment diagnostic: {err}");
    assert!(err.contains("String") && err.contains("Int"), "missing field types: {err}");
}

#[test]
fn named_and_default_function_arguments_work() {
    let out = run(&["--run", &example_path("function_arguments.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["Ostrin!", "Ostrin?"]);
}

#[test]
fn function_argument_count_names_and_types_are_checked() {
    let out = run(&[&example_path("function_argument_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing argument diagnostic: {err}");
    assert!(err.contains("Missing required argument 'value'"), "missing arity diagnostic: {err}");
    assert!(err.contains("String") && err.contains("Int"), "missing argument types: {err}");
    assert!(err.contains("no parameter named 'extra'"), "missing named argument diagnostic: {err}");
    assert!(err.contains("Positional arguments must come before named arguments"), "missing ordering diagnostic: {err}");
}

#[test]
fn collection_lookup_results_are_option_types() {
    let out = run(&[&example_path("collection_types_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing Option argument diagnostic: {err}");
    assert!(err.contains("Option<Int>"), "collection lookup did not preserve Option<Int>: {err}");
}

#[test]
fn task_channel_and_iterator_types_are_checked() {
    let out = run(&[&example_path("concurrency_types_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing concurrency type diagnostic: {err}");
    assert!(err.contains("Method 'send' expects 'Int', got 'String'"), "missing channel send diagnostic: {err}");
    assert!(err.contains("Option<Int>"), "missing channel receive type: {err}");
    assert!(err.contains("String"), "missing task join type: {err}");
}

#[test]
fn collection_method_arguments_are_checked() {
    let out = run(&[&example_path("collection_argument_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing collection argument diagnostic: {err}");
    assert!(err.contains("push") && err.contains("expects 'Int'") && err.contains("String"), "missing List argument types: {err}");
    assert!(err.contains("get") && err.contains("expects 'String'") && err.contains("Int"), "missing Map key type: {err}");
    assert!(err.contains("add") && err.contains("expects 'Int'"), "missing Set element type: {err}");
}

#[test]
fn control_flow_conditions_and_explicit_returns_are_checked() {
    let out = run(&[&example_path("control_type_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("If condition expects 'Bool', got 'Int'"), "missing if condition diagnostic: {err}");
    assert!(err.contains("While condition expects 'Bool', got 'String'"), "missing while condition diagnostic: {err}");
    assert!(err.contains("Logical operators expect 'Bool' operands"), "missing logical operand diagnostic: {err}");
    assert!(err.contains("Return expression expects 'Int', got 'String'"), "missing return type diagnostic: {err}");
    assert!(err.contains("Empty return expects function return type 'Void'"), "missing empty return diagnostic: {err}");
    assert!(err.contains("Default value for 'value' expects 'Int', got 'String'"), "missing default value diagnostic: {err}");
}

#[test]
fn option_and_result_methods_run() {
    let out = run(&["--run", &example_path("option_result.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        stdout(&out).lines().collect::<Vec<_>>(),
        [
            "true", "Some(5)", "Some(5)", "4", "Ok(4)", "true", "9",
            "Err(missing)", "true", "Ok(8)", "Some(7)", "true", "Err(bad!)", "None"
        ]
    );
}

#[test]
fn option_and_result_lambdas_receive_contextual_types() {
    let out = run(&["--members", "--json", &example_path("option_result_types.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("\"name\":\"mapped\",\"type\":\"Option<Int>\""));
    assert!(text.contains("\"name\":\"chained\",\"type\":\"Option<Int>\""));
    assert!(text.contains("\"name\":\"result_mapped\",\"type\":\"Result<Int, String>\""));
    assert!(text.contains("\"name\":\"error_mapped\",\"type\":\"Result<Int, String>\""));
}

#[test]
fn option_and_result_method_arguments_are_checked() {
    let out = run(&[&example_path("option_result_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing Option/Result diagnostic: {err}");
    assert!(err.contains("unwrap_or") && err.contains("expects 'Int'") && err.contains("String"), "missing unwrap_or type diagnostic: {err}");
    assert!(err.contains("map") && err.contains("fn(Int)"), "missing map callback-shape diagnostic: {err}");
}

#[test]
fn try_unwraps_and_propagates_option_and_result() {
    let out = run(&["--run", &example_path("try_result.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        stdout(&out).lines().collect::<Vec<_>>(),
        ["Ok(4)", "Err(converted: bad)", "Some(10)", "None"]
    );
}

#[test]
fn try_requires_a_matching_enclosing_result_type() {
    let out = run(&[&example_path("try_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing try diagnostic: {err}");
    assert!(err.contains("enclosing Option/Result"), "missing container mismatch: {err}");
    assert!(err.contains("propagates error type") && err.contains("String") && err.contains("Int"), "missing error propagation mismatch: {err}");
    assert!(err.contains("try catch") && err.contains("fn(String)"), "missing catch signature mismatch: {err}");
}

#[test]
fn standard_library_file_io_and_parsing_work() {
    let out = run(&["--run", &example_path("stdlib_io.ostrin")]);
    let _ = fs::remove_file("target/ostrin-stdlib-test.txt");
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["true", "42", "hello from Ostrin"]);
}

#[test]
fn native_ir_file_io_preserves_results_and_ownership() {
    let file = example_path("native_ir_file_io.ostrin");
    let interpreted = run(&["--run", &file]);
    let _ = fs::remove_file("target/ostrin-ir-file-io.txt");
    let _ = fs::remove_file("target/ostrin-ir-file-io-missing.txt");
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(expected, "true\ntrue\nhello from IR\ntrue\n");

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|value| value.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 3, "file I/O example did not use the IR emitter: {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "file I/O example left a HIR fallback: {report_text}");

    let exe = temp_artifact("native-ir-file-io.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run native IR file I/O binary");
    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file("target/ostrin-ir-file-io.txt");
    let _ = fs::remove_file("target/ostrin-ir-file-io-missing.txt");
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "native file I/O leaked: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_source = run(&["--emit-c", "--native-threads", &file]);
    assert!(threaded_source.status.success(), "native-thread file I/O emission failed: {}", stderr(&threaded_source));
    let threaded_source = stdout(&threaded_source);
    assert!(threaded_source.contains("ostrin_file_read_cancelable"), "native file I/O did not lower read_file through the cancelable helper: {threaded_source}");
    assert!(threaded_source.contains("ostrin_file_write_cancelable"), "native file I/O did not lower write_file through the cancelable helper: {threaded_source}");
    assert!(threaded_source.contains("ostrin_file_request_wait"), "native file I/O did not use the cancelable request wait: {threaded_source}");
    assert!(threaded_source.contains("ostrin_thread_start_detached"), "native file I/O did not emit the detached worker runtime: {threaded_source}");

    let threaded_exe = temp_artifact("native-ir-file-io-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--leak-check", "--out", &threaded_exe, &file]);
    if skip_if_no_c_compiler(&threaded_compile) {
        return;
    }
    assert!(threaded_compile.status.success(), "native-thread compile failed: {}", stderr(&threaded_compile));
    let threaded = Command::new(&threaded_exe).output().expect("run native-thread IR file I/O binary");
    let _ = fs::remove_file(&threaded_exe);
    let _ = fs::remove_file("target/ostrin-ir-file-io.txt");
    let _ = fs::remove_file("target/ostrin-ir-file-io-missing.txt");
    assert!(threaded.status.success(), "native-thread binary failed: {}", String::from_utf8_lossy(&threaded.stderr));
    assert_eq!(String::from_utf8_lossy(&threaded.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&threaded.stderr).contains("live_allocations=0"), "native-thread file I/O leaked: {}", String::from_utf8_lossy(&threaded.stderr));
}

#[test]
fn standard_library_arguments_are_checked() {
    let out = run(&[&example_path("stdlib_io_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing standard library diagnostic: {err}");
    assert!(err.contains("read_file") && err.contains("String") && err.contains("Int"), "missing read_file type diagnostic: {err}");
    assert!(err.contains("write_file") && err.contains("String") && err.contains("Int"), "missing write_file type diagnostic: {err}");
    assert!(err.contains("parse_int") && err.contains("Bool"), "missing parse_int type diagnostic: {err}");
}

#[test]
fn json_diagnostics_are_editor_friendly_and_keep_source_locations() {
    let out = run(&["--json", &example_path("field_access_errors.ostrin")]);
    assert!(!out.status.success());
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "expected one JSON object per diagnostic: {text}");
    assert!(lines.iter().all(|line| line.starts_with('{') && line.ends_with('}')));
    assert!(text.contains("\"code\":\"E1043\""));
    assert!(text.contains("\"message\":\"Type 'Score' has no field 'missing'.\""));
    assert!(text.contains("\"line\":8"));
    assert!(text.contains("\"column\":5"));
}

#[test]
fn compiler_checks_unsaved_stdin_source_for_editor_integrations() {
    let source = "fn main() -> Void {\n    if 1 {\n        print(\"bad\")\n    }\n}\n";
    let out = run_stdin(&[
        "--stdin",
        "--check",
        "--json",
        "--file",
        "C:/workspace/unsaved.ostrin",
    ], source);
    assert!(!out.status.success());
    let text = stdout(&out);
    assert!(text.contains("\"severity\":\"error\""), "missing JSON diagnostic: {text}");
    assert!(text.contains("C:/workspace/unsaved.ostrin"), "missing source path: {text}");

    let valid = run_stdin(
        &["--stdin", "--check", "--json", "--file", "C:/workspace/unsaved.ostrin"],
        "fn main() -> Void {\n    print(\"ok\")\n}\n",
    );
    assert!(valid.status.success(), "stderr: {}", stderr(&valid));
    assert!(stdout(&valid).is_empty(), "JSON success should be silent: {}", stdout(&valid));
}

#[test]
fn compiler_lsp_negotiates_and_publishes_diagnostics() {
    let out = run_lsp(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
        r##"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///C:/workspace/lsp.ostrin","languageId":"ostrin","version":1,"text":"fn main() -> Void {\n    if 1 {\n        print(\"bad\")\n    }\n}\n"}}}"##,
        r#"{"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{"textDocument":{"uri":"file:///C:/workspace/lsp.ostrin"},"position":{"line":0,"character":4}}}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"textDocument/completion","params":{"textDocument":{"uri":"file:///C:/workspace/lsp.ostrin"},"position":{"line":0,"character":4}}}"#,
        r#"{"jsonrpc":"2.0","id":5,"method":"textDocument/definition","params":{"textDocument":{"uri":"file:///C:/workspace/lsp.ostrin"},"position":{"line":0,"character":4}}}"#,
        r##"{"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":"file:///C:/workspace/lsp.ostrin","version":2},"contentChanges":[{"text":"fn main() -> Void {\n    print(\"ok\")\n}\n"}]}}"##,
        r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#,
        r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
    ]);
    assert!(out.status.success(), "LSP exited unsuccessfully");
    let text = stdout(&out);
    assert!(text.contains("\"hoverProvider\":true"), "missing initialize capabilities: {text}");
    assert!(text.contains("textDocument/publishDiagnostics"), "missing diagnostics notification: {text}");
    assert!(text.contains("OSTRIN-E1041"), "missing type diagnostic: {text}");
    assert!(text.contains("\"line\":1"), "missing zero-based diagnostic range: {text}");
    assert!(text.contains("\"id\":3") && text.contains("Ostrin function"), "missing hover response: {text}");
    assert!(text.contains("\"id\":4") && text.contains("\"label\":\"main\""), "missing completion response: {text}");
    assert!(text.contains("\"id\":5") && text.contains("\"uri\":\"file:///C:/workspace/lsp.ostrin\""), "missing definition response: {text}");
    assert!(text.contains("\"diagnostics\":[]"), "didChange should clear diagnostics: {text}");
}

#[test]
fn compiler_lsp_resolves_imports_across_open_documents() {
    let main_uri = file_uri("proj1/main.ostrin");
    let units_uri = file_uri("proj1/physics/units.ostrin");
    let main_text = fs::read_to_string(example_path("proj1/main.ostrin")).unwrap();
    let units_text = fs::read_to_string(example_path("proj1/physics/units.ostrin")).unwrap();

    let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}).to_string();
    let open_units = json!({
        "jsonrpc":"2.0","method":"textDocument/didOpen",
        "params":{"textDocument":{"uri":units_uri,"languageId":"ostrin","version":1,"text":units_text}}
    }).to_string();
    let open_main = json!({
        "jsonrpc":"2.0","method":"textDocument/didOpen",
        "params":{"textDocument":{"uri":main_uri,"languageId":"ostrin","version":1,"text":main_text}}
    }).to_string();
    // `to_kelvin` is called as `units.to_kelvin(...)` on main.ostrin's line 5
    // (0-based): "    k = units.to_kelvin(25.0)".
    let hover = json!({
        "jsonrpc":"2.0","id":2,"method":"textDocument/hover",
        "params":{"textDocument":{"uri":main_uri},"position":{"line":5,"character":15}}
    }).to_string();
    let references = json!({
        "jsonrpc":"2.0","id":3,"method":"textDocument/references",
        "params":{"textDocument":{"uri":main_uri},"position":{"line":5,"character":15},"context":{"includeDeclaration":true}}
    }).to_string();
    let signature_help = json!({
        "jsonrpc":"2.0","id":4,"method":"textDocument/signatureHelp",
        "params":{"textDocument":{"uri":main_uri},"position":{"line":5,"character":25}}
    }).to_string();
    let semantic_tokens = json!({
        "jsonrpc":"2.0","id":5,"method":"textDocument/semanticTokens/full",
        "params":{"textDocument":{"uri":main_uri}}
    }).to_string();
    let rename = json!({
        "jsonrpc":"2.0","id":6,"method":"textDocument/rename",
        "params":{"textDocument":{"uri":main_uri},"position":{"line":5,"character":15},"newName":"to_kelvin_renamed"}
    }).to_string();
    // Break the import by editing units.ostrin in memory only (never written
    // to disk): this must be visible immediately in main.ostrin's diagnostics,
    // proving resolution uses the live buffer, not the file on disk.
    let broken_units_text = units_text.replace("pub fn meters_to_km", "fn meters_to_km");
    let break_units = json!({
        "jsonrpc":"2.0","method":"textDocument/didChange",
        "params":{"textDocument":{"uri":units_uri,"version":2},"contentChanges":[{"text":broken_units_text}]}
    }).to_string();
    let retouch_main = json!({
        "jsonrpc":"2.0","method":"textDocument/didChange",
        "params":{"textDocument":{"uri":main_uri,"version":2},"contentChanges":[{"text":main_text}]}
    }).to_string();

    let out = run_lsp(&[
        &initialize,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
        &open_units,
        &open_main,
        &hover,
        &references,
        &signature_help,
        &semantic_tokens,
        &rename,
        &break_units,
        &retouch_main,
        r#"{"jsonrpc":"2.0","id":7,"method":"shutdown","params":null}"#,
        r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
    ]);
    assert!(out.status.success(), "LSP exited unsuccessfully: {}", stderr(&out));
    let text = stdout(&out);

    assert!(
        text.contains("\"id\":2") && text.contains("to_kelvin") && text.contains("Ostrin function"),
        "hover should resolve a symbol declared in the imported file: {text}"
    );
    let references_reply = extract_result(&text, 3);
    let reference_count = references_reply.matches("\"range\"").count();
    assert!(
        reference_count >= 3,
        "references should span both the declaration and every call site across files: {references_reply}"
    );
    assert!(
        references_reply.contains(&units_uri.replace('\\', "\\\\")) || references_reply.contains("units.ostrin"),
        "references should include the declaration file: {references_reply}"
    );

    let signature_reply = extract_result(&text, 4);
    assert!(
        signature_reply.contains("celsius") && signature_reply.contains("Float"),
        "signature help should describe to_kelvin's parameter: {signature_reply}"
    );

    let tokens_reply = extract_result(&text, 5);
    assert!(tokens_reply.contains("\"data\":["), "semantic tokens should return a data array: {tokens_reply}");
    assert_ne!(tokens_reply, "{\"data\":[]}", "semantic tokens should not be empty for a resolved file: {tokens_reply}");

    let rename_reply = extract_result(&text, 6);
    assert!(rename_reply.contains("\"changes\""), "rename should produce a workspace edit: {rename_reply}");
    assert!(
        rename_reply.contains("to_kelvin_renamed"),
        "rename should carry the new name in the edit: {rename_reply}"
    );

    assert!(
        text.contains("OSTRIN") || text.contains("private") || text.contains("not found") || text.contains("Error"),
        "editing the imported file in memory should surface a diagnostic on the importer: {text}"
    );
}

#[test]
fn compiler_lsp_finds_references_in_unopened_workspace_files() {
    let root_uri = file_uri("proj1");
    let main_uri = file_uri("proj1/main.ostrin");
    let main_text = fs::read_to_string(example_path("proj1/main.ostrin")).unwrap();

    // `physics/units.ostrin` is never opened here — the server must read it
    // straight from disk (via the workspace root from `initialize`) to find
    // `to_kelvin`'s declaration for a references/rename request.
    let initialize = json!({
        "jsonrpc":"2.0","id":1,"method":"initialize",
        "params":{"rootUri":root_uri}
    }).to_string();
    let open_main = json!({
        "jsonrpc":"2.0","method":"textDocument/didOpen",
        "params":{"textDocument":{"uri":main_uri,"languageId":"ostrin","version":1,"text":main_text}}
    }).to_string();
    let references = json!({
        "jsonrpc":"2.0","id":2,"method":"textDocument/references",
        "params":{"textDocument":{"uri":main_uri},"position":{"line":5,"character":15},"context":{"includeDeclaration":true}}
    }).to_string();
    let rename = json!({
        "jsonrpc":"2.0","id":3,"method":"textDocument/rename",
        "params":{"textDocument":{"uri":main_uri},"position":{"line":5,"character":15},"newName":"to_kelvin_renamed"}
    }).to_string();

    let out = run_lsp(&[
        &initialize,
        r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
        &open_main,
        &references,
        &rename,
        r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":null}"#,
        r#"{"jsonrpc":"2.0","method":"exit","params":null}"#,
    ]);
    assert!(out.status.success(), "LSP exited unsuccessfully: {}", stderr(&out));
    let text = stdout(&out);

    let references_reply = extract_result(&text, 2);
    let reference_count = references_reply.matches("\"range\"").count();
    assert!(
        reference_count >= 3,
        "references should include occurrences from the never-opened units.ostrin: {references_reply}"
    );
    assert!(
        references_reply.contains("units.ostrin"),
        "references should point into the on-disk file that was never opened: {references_reply}"
    );

    let rename_reply = extract_result(&text, 3);
    assert!(
        rename_reply.contains("units.ostrin"),
        "rename should also produce an edit for the on-disk declaration file: {rename_reply}"
    );
}

fn extract_result(stream: &str, id: u64) -> String {
    let marker = format!("\"id\":{id},");
    let start = stream.find(&marker).unwrap_or_else(|| panic!("no reply for id {id} in: {stream}"));
    let tail = &stream[start..];
    let end = tail.find("Content-Length").unwrap_or(tail.len());
    tail[..end].to_string()
}

/// Splits raw DAP/LSP stdout back into individual JSON messages, using the
/// `Content-Length` framing byte-for-byte instead of text search — the
/// bodies below contain nested objects, so scanning for substrings like
/// `"request_seq":N` can't reliably find a message's own boundaries.
fn parse_framed_messages(out: &Output) -> Vec<serde_json::Value> {
    let bytes = &out.stdout;
    let mut offset = 0usize;
    let mut messages = Vec::new();
    while offset < bytes.len() {
        let header_end = match bytes[offset..].windows(4).position(|w| w == b"\r\n\r\n") {
            Some(pos) => offset + pos,
            None => break,
        };
        let header = String::from_utf8_lossy(&bytes[offset..header_end]);
        let length: usize = header
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length:").map(|value| value.trim().parse().unwrap()))
            .expect("frame missing Content-Length");
        let body_start = header_end + 4;
        let body = &bytes[body_start..body_start + length];
        messages.push(serde_json::from_slice(body).expect("frame body must be valid JSON"));
        offset = body_start + length;
    }
    messages
}

#[test]
fn compiler_dap_hits_breakpoints_and_reports_locals() {
    // `examples/fibonacci.ostrin` loops 10 times over `print(n)` at line 24;
    // a breakpoint there should pause once per iteration, with `n` and `fib`
    // (defined earlier in the same function) both visible as locals.
    let program = example_path("fibonacci.ostrin");
    let mut seq = 0u64;
    let mut next = |value: serde_json::Value| -> String {
        seq += 1;
        let mut object = value.as_object().cloned().unwrap();
        object.insert("seq".to_string(), json!(seq));
        object.insert("type".to_string(), json!("request"));
        json!(object).to_string()
    };

    let mut messages = vec![
        next(json!({"command":"initialize","arguments":{}})),
        next(json!({"command":"launch","arguments":{"program":program,"stopOnEntry":false}})),
        next(json!({
            "command":"setBreakpoints",
            "arguments":{"source":{"path":program},"breakpoints":[{"line":24}]}
        })),
        next(json!({"command":"configurationDone","arguments":{}})),
    ];
    // The loop runs exactly 10 times; inspect the first stop in full, then
    // just keep continuing through the remaining nine.
    messages.push(next(json!({"command":"stackTrace","arguments":{"threadId":1}})));
    messages.push(next(json!({"command":"scopes","arguments":{"frameId":0}})));
    messages.push(next(json!({"command":"variables","arguments":{"variablesReference":1}})));
    messages.push(next(json!({"command":"evaluate","arguments":{"expression":"n","frameId":0}})));
    for _ in 0..10 {
        messages.push(next(json!({"command":"continue","arguments":{"threadId":1}})));
    }

    let out = run_dap(&messages);
    assert!(out.status.success(), "DAP session exited unsuccessfully: {}", stderr(&out));
    let received = parse_framed_messages(&out);

    let events = |name: &str| -> Vec<&serde_json::Value> {
        received.iter().filter(|m| m["type"] == "event" && m["event"] == name).collect()
    };
    let response = |request_seq: u64| -> &serde_json::Value {
        received
            .iter()
            .find(|m| m["type"] == "response" && m["request_seq"] == request_seq)
            .unwrap_or_else(|| panic!("no response for request_seq {request_seq} in: {received:#?}"))
    };

    assert_eq!(events("initialized").len(), 1, "expected exactly one initialized event: {received:#?}");
    let stopped = events("stopped");
    assert_eq!(stopped.len(), 10, "the breakpoint on line 24 should be hit once per loop iteration: {received:#?}");
    assert!(stopped.iter().all(|event| event["body"]["reason"] == "breakpoint"));

    let stack_trace = &response(5)["body"];
    let top_frame = &stack_trace["stackFrames"][0];
    assert_eq!(top_frame["name"], "main");
    assert_eq!(top_frame["line"], 24);

    let variables = response(7)["body"]["variables"].as_array().expect("variables body");
    let names: Vec<&str> = variables.iter().filter_map(|v| v["name"].as_str()).collect();
    assert!(names.contains(&"n"), "locals should include the loop variable: {names:?}");
    assert!(names.contains(&"fib"), "locals should include a variable from an outer statement in the same function: {names:?}");

    assert_eq!(response(8)["body"]["result"], "0", "evaluating 'n' on the first iteration should be 0");

    let output_text: String = events("output").iter().filter_map(|event| event["body"]["output"].as_str()).collect();
    // Fibonacci(10) starting at 0,1: 0 1 1 2 3 5 8 13 21 34.
    assert_eq!(output_text, "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n", "unexpected printed sequence");

    assert_eq!(events("terminated").len(), 1, "expected a terminated event: {received:#?}");
    let exited = events("exited");
    assert_eq!(exited.len(), 1, "expected an exited event: {received:#?}");
    assert_eq!(exited[0]["body"]["exitCode"], 0, "program should exit cleanly");
}

#[test]
fn compiler_dap_stops_on_entry_steps_and_disconnects_cleanly() {
    // No breakpoints at all here — `stopOnEntry` should pause before the
    // very first statement of `main` runs, `next` should move exactly one
    // statement without touching anything inside a nested block, and
    // `disconnect` mid-run should stop the program before it reaches any of
    // its `print()` calls (all of which come later, inside the `for` loop).
    let program = example_path("hello.ostrin");
    let mut seq = 0u64;
    let mut next_msg = |value: serde_json::Value| -> String {
        seq += 1;
        let mut object = value.as_object().cloned().unwrap();
        object.insert("seq".to_string(), json!(seq));
        object.insert("type".to_string(), json!("request"));
        json!(object).to_string()
    };

    let messages = vec![
        next_msg(json!({"command":"initialize","arguments":{}})),
        next_msg(json!({"command":"launch","arguments":{"program":program,"stopOnEntry":true}})),
        next_msg(json!({"command":"configurationDone","arguments":{}})),
        next_msg(json!({"command":"stackTrace","arguments":{"threadId":1}})),
        next_msg(json!({"command":"next","arguments":{"threadId":1}})),
        next_msg(json!({"command":"stackTrace","arguments":{"threadId":1}})),
        next_msg(json!({"command":"disconnect","arguments":{}})),
    ];

    let out = run_dap(&messages);
    let received = parse_framed_messages(&out);
    let events = |name: &str| -> Vec<&serde_json::Value> {
        received.iter().filter(|m| m["type"] == "event" && m["event"] == name).collect()
    };
    let response = |request_seq: u64| -> &serde_json::Value {
        received
            .iter()
            .find(|m| m["type"] == "response" && m["request_seq"] == request_seq)
            .unwrap_or_else(|| panic!("no response for request_seq {request_seq} in: {received:#?}"))
    };

    let stopped = events("stopped");
    assert_eq!(stopped.len(), 2, "expected an entry stop and a step stop: {received:#?}");
    assert_eq!(stopped[0]["body"]["reason"], "entry");
    assert_eq!(stopped[1]["body"]["reason"], "step");

    let first_line = response(4)["body"]["stackFrames"][0]["line"].as_i64().unwrap();
    let second_line = response(6)["body"]["stackFrames"][0]["line"].as_i64().unwrap();
    assert_eq!(first_line, 8, "stopOnEntry should land on main's first statement");
    assert_eq!(second_line, 9, "'next' should move exactly one statement forward in the same frame");

    // Disconnecting here happens before the loop that calls print() ever
    // runs, so the only 'output' event should be the interpreter reporting
    // its own termination, not anything the Ostrin program printed.
    let output = events("output");
    assert_eq!(output.len(), 1, "no Ostrin print() should have run yet: {received:#?}");
    assert!(output[0]["body"]["output"].as_str().unwrap().contains("terminated"));
    assert_eq!(events("terminated").len(), 1);
}

/// Path for a temporary native artifact, unique per test process so parallel
/// `cargo test` runs never collide.
fn temp_artifact(name: &str) -> String {
    std::env::temp_dir()
        .join(format!("ostrin_native_test_{}_{name}", std::process::id()))
        .display()
        .to_string()
}

fn temp_source(name: &str, source: &str) -> String {
    let path = temp_artifact(name);
    fs::write(&path, source).expect("failed to write temporary Ostrin source");
    path
}

/// True when `--compile` failed only because no GNU-compatible C compiler is
/// installed — a real, environment-dependent condition (this suite can't
/// require every machine it runs on to have gcc/clang), not a codegen bug.
fn skip_if_no_c_compiler(compile: &Output) -> bool {
    if !compile.status.success() && stderr(compile).contains("no GNU-compatible C compiler found") {
        eprintln!("skipping: no GNU-compatible C compiler available on this machine");
        true
    } else {
        false
    }
}

#[test]
fn native_backend_compiles_and_runs_fibonacci() {
    let report = run(&["--native-type-report", &example_path("native_fibonacci.ostrin")]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 1, "recursive scalar function did not use the IR emitter: {}", stdout(&report));

    let emitted = run(&["--emit-c", &example_path("native_fibonacci.ostrin")]);
    assert!(emitted.status.success(), "IR C emission failed: {}", stderr(&emitted));
    assert!(stdout(&emitted).contains("__ostrin_ir_bb3:"), "expected CFG labels in IR output");
    assert!(stdout(&emitted).contains("__ostrin_ir_pred"), "expected predecessor tracking for phi lowering");

    let exe = temp_artifact("fibonacci.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_fibonacci.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    assert!(stdout(&compile).contains("compiled:"), "expected a confirmation message: {}", stdout(&compile));

    let run_output = Command::new(&exe).output().unwrap_or_else(|e| panic!("failed to run compiled binary '{exe}': {e}"));
    let _ = fs::remove_file(&exe);
    assert!(run_output.status.success(), "compiled binary exited unsuccessfully");
    // The MinGW C runtime's stdout is opened in text mode, so it rewrites
    // "\n" to "\r\n" on Windows; normalize before comparing.
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n",
        "native binary should print the same Fibonacci sequence the interpreter does"
    );
}

#[test]
fn native_backend_compiles_and_runs_strings_and_booleans() {
    let exe = temp_artifact("strings.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_strings.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));

    let run_output = Command::new(&exe).output().unwrap_or_else(|e| panic!("failed to run compiled binary '{exe}': {e}"));
    let _ = fs::remove_file(&exe);
    assert!(run_output.status.success(), "compiled binary exited unsuccessfully");
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "Hello, Ostrin\nHello, Ostrin\nHello, Ostrin\ntrue\nfalse\n",
        "native binary should exercise String concatenation, while-loops and Bool printing correctly"
    );
}

#[test]
fn native_backend_emit_c_writes_readable_c_source() {
    let out = run(&["--emit-c", &example_path("native_fibonacci.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert!(source.contains("#include <stdint.h>"));
    assert!(source.contains("int64_t ostrin_fn_fib(int64_t n)"));
    assert!(source.contains("int main(int argc, char** argv)"), "the generated file must supply its own C main: {source}");
}

#[test]
fn cooperative_c_runtime_guards_thread_only_headers() {
    let cooperative = run(&["--emit-c", &example_path("native_fibonacci.ostrin")]);
    assert!(cooperative.status.success(), "cooperative emit failed: {}", stderr(&cooperative));
    let source = stdout(&cooperative);
    assert!(source.contains("#if defined(OSTRIN_NATIVE_THREADS) && defined(_WIN32)"));
    assert!(source.contains("typedef int OstrinMutex;"), "cooperative runtime should provide no-op locks: {source}");
    assert!(!source.contains("#define OSTRIN_NATIVE_THREADS"));

    let threaded = run(&["--emit-c", "--native-threads", &example_path("native_threads.ostrin")]);
    assert!(threaded.status.success(), "threaded emit failed: {}", stderr(&threaded));
    assert!(stdout(&threaded).contains("#define OSTRIN_NATIVE_THREADS"));
    assert!(stdout(&threaded).contains("#include <pthread.h>") || stdout(&threaded).contains("#include <windows.h>"));
}

#[test]
fn wasm_target_emits_cooperative_c_and_rejects_native_threads() {
    let wasm = run(&["--emit-c", "--target", "wasm32-wasi", &example_path("hello.ostrin")]);
    assert!(wasm.status.success(), "WASI C emission failed: {}", stderr(&wasm));
    let source = stdout(&wasm);
    assert!(!source.contains("#define OSTRIN_NATIVE_THREADS"));
    assert!(source.contains("typedef int OstrinMutex;"));

    let threaded = run(&[
        "--emit-c",
        "--target",
        "wasm32-wasi",
        "--native-threads",
        &example_path("hello.ostrin"),
    ]);
    assert!(!threaded.status.success());
    assert!(stderr(&threaded).contains("--native-threads is not supported for target wasm32-wasi"));
}

#[test]
fn wasm_program_matrix_emits_without_native_thread_dependencies() {
    for file in [
        "hello.ostrin",
        "wasi_io_contract.ostrin",
        "native_ir_file_io.ostrin",
        "native_ir_managed_consumers.ostrin",
    ] {
        let wasm = run(&["--emit-c", "--target", "wasm32-wasi", &example_path(file)]);
        assert!(wasm.status.success(), "WASI C emission failed for {file}: {}", stderr(&wasm));
        let source = stdout(&wasm);
        assert!(!source.contains("#define OSTRIN_NATIVE_THREADS"), "WASI program {file} enabled native threads");
        assert!(source.contains("typedef int OstrinMutex;"), "WASI program {file} did not use the cooperative runtime");
    }

    let package = run(&[
        "--emit-c",
        "--target",
        "wasm32-wasi",
        "--project",
        &example_path("pkg_project/main_app"),
    ]);
    assert!(package.status.success(), "WASI package C emission failed: {}", stderr(&package));
    assert!(!stdout(&package).contains("#define OSTRIN_NATIVE_THREADS"));
}

#[test]
fn program_arguments_match_between_interpreter_and_native() {
    let file = example_path("args.ostrin");
    let interpreted = run(&["--run", &file, "--", "uno", "dos"]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), "2\nuno\ndos\n");

    let exe = temp_artifact("args.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).args(["uno", "dos"]).output().expect("run args binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "2\nuno\ndos\n");
}

#[test]
fn environment_and_paths_match_between_interpreter_and_native() {
    let file = example_path("env_path.ostrin");
    let interpreted = run_with_env(&["--run", &file], "OSTRIN_TEST_VALUE", "Ostrin");
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(expected, "Some(Ostrin)\nsrc/main.ostrin\n");

    let exe = temp_artifact("env-path.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe)
        .env("OSTRIN_TEST_VALUE", "Ostrin")
        .output()
        .expect("run environment/path binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn standard_args_and_env_modules_match_between_interpreter_and_native() {
    let file = example_path("std_args_env.ostrin");
    let interpreted = run_with_env(&["--run", &file, "--", "uno", "dos"], "OSTRIN_TEST_VALUE", "Ostrin");
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(
        expected,
        "2\nuno\ndos\ntrue\ntrue\nOstrin\ntrue\nsrc/main.ostrin\ntrue\n"
    );

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|value| value.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 8, "std args/env modules still fall back from IR: {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "std args/env modules left a HIR fallback: {report_text}");

    let exe = temp_artifact("std-args-env.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe)
        .env("OSTRIN_TEST_VALUE", "Ostrin")
        .args(["uno", "dos"])
        .output()
        .expect("run std args/env binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "std args/env native ownership leaked: {}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn structural_equality_matches_between_interpreter_and_native() {
    let file = example_path("structural_equality.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(expected, "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|value| value.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 1, "structural equality still falls back from IR: {}", stdout(&report));

    let exe = temp_artifact("structural-equality.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run structural equality binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "structural equality leaked: {}", String::from_utf8_lossy(&native.stderr));
}

#[test]
fn formatting_and_filesystem_builtins_match_between_interpreter_and_native() {
    let file = example_path("format_filesystem.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(expected, "2 + 3 = 5\ntrue\ntrue\n");

    let exe = temp_artifact("format-filesystem.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run formatting/filesystem binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn hash_map_scalars_match_between_interpreter_and_native() {
    let file = example_path("hash_map_stress.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(expected, "51\nSome(999)\nfalse\nSome(98)\n51\ntrue\nfalse\n");

    let exe = temp_artifact("hash-map-stress.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run hash map binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn hash_builtin_matches_between_interpreter_and_native() {
    let file = example_path("hash_builtin.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");

    let exe = temp_artifact("hash-builtin.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run hash builtin binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn composite_collection_keys_match_between_interpreter_and_native() {
    let file = example_path("hash_map_composite.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(expected, "Some(ok)\nfalse\nSome(updated)\n1\ntrue\nfalse\n1\nSome(eleven)\ntrue\n1\nSome(stable)\ntrue\n");

    let exe = temp_artifact("hash-map-composite.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run composite hash map binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn hash_rejects_unhashable_composite_payloads() {
    let out = run_stdin(
        &["--stdin", "--check", "--file", "C:/workspace/hash_error.ostrin"],
        "record NoHash {\n    value: Int\n}\nenum NoHashEnum {\n    Value(Int)\n}\nfn main() -> Void {\n    print(hash([1, 2]))\n    print(hash(NoHash { value: 1 }))\n    print(hash(Value(1)))\n}\n",
    );
    assert!(!out.status.success());
    let text = stderr(&out);
    assert!(text.contains("E1041"), "missing hash diagnostic: {text}");
    assert!(text.contains("derive(Hash)"), "missing hash contract: {text}");
}

#[test]
fn map_and_set_require_hash_and_eq_bounds() {
    let out = run_stdin(
        &["--stdin", "--check", "--file", "C:/workspace/collection_bounds.ostrin"],
        "record HashOnly: Hash {\n    value: Int\n}\nrecord EqOnly: Eq {\n    value: Int\n}\nfn main() -> Void {\n    mut values = Map<HashOnly, Int>()\n    mut seen = Set<EqOnly>()\n}\n",
    );
    assert!(!out.status.success());
    let text = stderr(&out);
    assert!(text.contains("Map key type 'HashOnly' must satisfy Hash + Eq"), "missing Map bound diagnostic: {text}");
    assert!(text.contains("Set element type 'EqOnly' must satisfy Hash + Eq"), "missing Set bound diagnostic: {text}");
    assert!(text.contains("missing Eq") && text.contains("missing Hash"), "missing individual bounds: {text}");
}

#[test]
fn native_backend_exposes_ownership_runtime_and_leak_check() {
    let out = run(&["--emit-c", "--leak-check", &example_path("native_fibonacci.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert!(source.contains("static void ostrin_retain(void* ptr)"));
    assert!(source.contains("static void ostrin_release(void* ptr)"));
    assert!(source.contains("static void ostrin_mem_report(void)"));
    assert!(source.contains("ostrin memory: live_allocations="));
    assert!(source.contains("ostrin_mem_report();"));
}

#[test]
fn native_ownership_primitives_release_composite_allocations() {
    let exe = temp_artifact("ownership_primitives.exe");
    let compile = run(&[
        "--compile",
        "--leak-check",
        "--out",
        &exe,
        &example_path("ownership_primitives.ostrin"),
    ]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "ownership compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run ownership primitive binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "ownership binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "3\n");
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "explicit clone/drop should release the list and its backing storage: {}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn native_ownership_automatically_releases_aliases_reassignments_and_returns() {
    let exe = temp_artifact("ownership-auto.exe");
    let compile = run(&[
        "--compile",
        "--leak-check",
        "--out",
        &exe,
        &example_path("ownership_auto.ostrin"),
    ]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "ownership auto compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run automatic ownership binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "ownership auto binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "3\n1\n1\n");
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "automatic ownership should release aliases, replaced values and returned values: {}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn native_ownership_releases_loop_and_branch_locals() {
    let exe = temp_artifact("ownership-loops.exe");
    let compile = run(&[
        "--compile",
        "--leak-check",
        "--out",
        &exe,
        &example_path("ownership_loops.ostrin"),
    ]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "ownership loop compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run loop ownership binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "ownership loop binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "3\n1\n");
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "loop and branch locals should be released per iteration: {}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn native_ownership_releases_ast_loop_locals() {
    let exe = temp_artifact("ownership-loops-ast.exe");
    let compile = run(&[
        "--compile",
        "--leak-check",
        "--out",
        &exe,
        &example_path("ownership_loops_ast.ostrin"),
    ]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "AST ownership loop compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run AST loop ownership binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "AST ownership loop binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "2\nNone\n");
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "AST loop locals should be released per iteration: {}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn native_threads_use_os_thread_and_blocking_channel_runtime() {
    let exe = temp_artifact("native-threads.exe");
    let compile = run(&[
        "--compile",
        "--native-threads",
        "--leak-check",
        "--out",
        &exe,
        &example_path("native_threads.ostrin"),
    ]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native thread compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run native thread binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native thread binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "7\n");
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "native thread runtime should release task, environment and channel allocations: {}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn native_threads_scope_drain_releases_nested_task_handles() {
    let exe = temp_artifact("native-scope-ownership.exe");
    let compile = run(&[
        "--compile",
        "--native-threads",
        "--leak-check",
        "--out",
        &exe,
        &example_path("concurrency_scheduler.ostrin"),
    ]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(
        compile.status.success(),
        "native scope compile failed: {}",
        stderr(&compile)
    );
    let native = Command::new(&exe)
        .output()
        .expect("run native scope binary");
    let _ = fs::remove_file(&exe);
    assert!(
        native.status.success(),
        "native scope binary failed: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    let output_lines: Vec<_> = String::from_utf8_lossy(&native.stdout)
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(output_lines.len(), 6, "unexpected task output: {output_lines:?}");
    let mut initial_lines = output_lines[..2].to_vec();
    initial_lines.sort();
    assert_eq!(initial_lines, vec!["main", "task"]);
    assert_eq!(output_lines.get(2).map(String::as_str), Some("42"));
    assert_eq!(output_lines.last().map(String::as_str), Some("7"));
    let mut concurrent_lines = output_lines[3..output_lines.len() - 1].to_vec();
    concurrent_lines.sort();
    assert_eq!(concurrent_lines, vec!["scope-body", "scope-task"]);
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "nested spawn_scope task handles should be released: {}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn compiler_lowers_hir_to_verified_cfg_ir() {
    let out = run(&["--ir", &example_path("native_fibonacci.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert!(source.contains("ir fn"));
    assert!(source.contains("bb0:"));
    assert!(source.contains("br %"), "the loop must become an explicit branch: {source}");
    assert!(source.contains("ret"), "the IR must terminate functions: {source}");
    assert!(source.contains("ir functions:"));
    assert!(source.contains("ir violations: 0"), "IR verifier reported a problem: {source}");
}

#[test]
fn compiler_lowers_match_and_try_to_explicit_ir_control_flow() {
    let match_ir = run(&["--ir", &example_path("native_enums.ostrin")]);
    assert!(match_ir.status.success(), "stderr: {}", stderr(&match_ir));
    let match_source = stdout(&match_ir);
    assert!(match_source.contains("pattern_test"));
    assert!(match_source.contains("pattern_bind"));
    assert!(match_source.contains("phi"));
    assert!(!match_source.contains("opaque match"), "match was left opaque: {match_source}");

    let try_ir = run(&["--ir", &example_path("native_result.ostrin")]);
    assert!(try_ir.status.success(), "stderr: {}", stderr(&try_ir));
    let try_source = stdout(&try_ir);
    assert!(try_source.contains("try_check"));
    assert!(try_source.contains("try_value"));
    assert!(try_source.contains("try_error"));
    assert!(!try_source.contains("opaque try"), "try was left opaque: {try_source}");
}

#[test]
fn compiler_lowers_concurrency_operations_to_explicit_ir() {
    let out = run(&["--ir", &example_path("native_concurrency.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert!(source.contains("channel("));
    assert!(source.contains("channel_send"));
    assert!(source.contains("channel_receive"));
    assert!(source.contains("channel_close"));
    assert!(source.contains("spawn"));
    assert!(source.contains("task_join"));
    assert!(source.contains("region_ret"));
    assert!(!source.contains("opaque concurrency"), "concurrency remained opaque: {source}");
}

#[test]
fn compiler_reports_conservative_ownership_facts() {
    let out = run(&["--ownership-report", &example_path("native_records.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert!(source.contains("ownership managed-values:"));
    assert!(source.contains("ownership last-use-candidates:"));
    assert!(source.contains("Point"), "record values should be classified as managed: {source}");
    assert!(source.contains("candidate=true"), "straight-line record uses should be reported: {source}");
}

#[test]
fn compiler_inserts_only_conservative_linear_releases() {
    let out = run(&["--ownership-ir", &example_path("ownership_linear.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert!(source.contains("release %"), "the channel transfer should have a release marker: {source}");
    assert!(source.contains("ownership-ir inserted-releases: 2"), "unexpected lowering summary: {source}");
}

#[test]
fn compiler_marks_reference_aliases_with_retains() {
    let out = run(&["--ownership-ir", &example_path("native_records.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert!(source.contains("retain %"), "managed aggregate aliases need retain markers: {source}");
    assert!(source.contains("ownership-ir inserted-retains:"), "missing retain summary: {source}");
}

#[test]
fn compiler_reports_static_channel_move_violations() {
    let out = run(&["--ownership-check", &example_path("moved_after_send.ostrin")]);
    assert!(!out.status.success(), "use-after-send must be rejected by the ownership check");
    let source = format!("{}{}", stdout(&out), stderr(&out));
    assert!(source.contains("OSTRIN-E1101"), "missing static E1101: {source}");
    assert!(source.contains("sent at") && source.contains("then used at"), "missing move locations: {source}");
}

#[test]
fn native_backend_emits_centralized_memory_cleanup() {
    let out = run(&["--emit-c", &example_path("native_hir_collections.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert!(source.contains("typedef struct OstrinAllocation"));
    assert!(source.contains("static void* ostrin_alloc(size_t size)"));
    assert!(source.contains("static void* ostrin_realloc(void* old_ptr, size_t size)"));
    assert!(source.contains("static void ostrin_free(void* ptr)"));
    assert!(source.contains("atexit(ostrin_mem_cleanup);"));
    assert!(source.contains("ostrin_realloc("), "collection growth must use the tracked allocator");
}

#[test]
fn native_backend_rejects_constructs_it_does_not_support_yet() {
    // `advanced.ostrin` is a syntax showcase that names a type (`Trajectory`)
    // the backend has no representation for: it must fail with a clear
    // message rather than silently emit something wrong.
    let out = run(&["--emit-c", &example_path("advanced.ostrin")]);
    assert!(!out.status.success(), "the native backend should refuse a program it can't fully compile");
    let error = stderr(&out);
    assert!(error.contains("supported by the native backend"), "unexpected error: {error}");
}

#[test]
fn moved_after_send_is_rejected_statically_in_both_entry_points() {
    // E1101 is now a compiler diagnostic shared by the interpreter and native
    // entry points. No backend should be generated for a use-after-send.
    let file = example_path("moved_after_send.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(!interpreted.status.success());
    let interpreted_error = format!("{}{}", stdout(&interpreted), stderr(&interpreted));
    assert!(interpreted_error.contains("OSTRIN-E1101"), "missing E1101: {interpreted_error}");
    assert!(interpreted_error.contains("used after channel send"), "unexpected diagnostic: {interpreted_error}");

    let exe = temp_artifact("moved.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    assert!(!compile.status.success(), "native compilation must stop at E1101");
    assert!(stderr(&compile).contains("OSTRIN-E1101"), "missing native E1101: {}", stderr(&compile));
    let _ = fs::remove_file(&exe);
}

#[test]
fn immutable_records_can_be_shared_through_channels() {
    let interpreted = run(&["--run", &example_path("immutable_record_channel.ostrin")]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).trim(), "1");

    let exe = temp_artifact("immutable-record.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("immutable_record_channel.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().unwrap();
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).trim(), "1");
}

#[test]
fn released_moved_records_do_not_poison_reused_allocation_addresses() {
    let file = temp_source(
        "e1101-released-record-reuse.ostrin",
        r#"
record Buffer {
    mut value: Int
}

fn retire_sent_record() -> Void {
    ch = channel<Buffer>()
    payload = Buffer { value: -1 }
    ch.send(payload)
    ch.receive()
    ch.close()
}

fn churn(seed: Int) -> Int {
    mut total = 0
    for index in 0 until 2048 {
        retire_sent_record()
        fresh = Buffer { value: seed + index }
        total = total + fresh.value
    }
    total
}

fn main() -> Void {
    first = spawn { churn(0) }
    second = spawn { churn(1) }
    print(first.join() + second.join())
}
"#,
    );
    let interpreted = run(&["--run", &file]);
    let exe = temp_artifact("e1101-released-record-reuse.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    let threaded_exe = temp_artifact("e1101-released-record-reuse-threads.exe");
    let threaded_compile = run(&[
        "--compile",
        "--native-threads",
        "--leak-check",
        "--out",
        &threaded_exe,
        &file,
    ]);
    let _ = fs::remove_file(&file);

    assert!(
        interpreted.status.success(),
        "a freed record address was treated as moved by the interpreter: {}",
        stderr(&interpreted)
    );
    assert_eq!(stdout(&interpreted).trim(), "4194304");
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "native compilation failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native allocation-reuse test");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).trim(), "4194304");
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "native move tracking retained released records: {}",
        String::from_utf8_lossy(&native.stderr)
    );

    if skip_if_no_c_compiler(&threaded_compile) {
        return;
    }
    assert!(
        threaded_compile.status.success(),
        "native-thread compilation failed: {}",
        stderr(&threaded_compile)
    );
    let threaded = Command::new(&threaded_exe)
        .output()
        .expect("run native-thread allocation-reuse test");
    let _ = fs::remove_file(&threaded_exe);
    assert!(
        threaded.status.success(),
        "native-thread run failed: {}",
        String::from_utf8_lossy(&threaded.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&threaded.stdout).trim(), "4194304");
    assert!(
        String::from_utf8_lossy(&threaded.stderr).contains("live_allocations=0"),
        "threaded move tracking retained released records: {}",
        String::from_utf8_lossy(&threaded.stderr)
    );
}

#[test]
fn channel_receiver_can_use_transferred_mutable_record() {
    let file = temp_source(
        "e1101-channel-record-transfer.ostrin",
        r#"
record Buffer {
    mut value: Int
}

fn main() -> Void {
    ch = channel<Buffer>()
    payload = Buffer { value: 7 }
    ch.send(payload)
    received = ch.receive()
    ch.close()
    match received {
        Some(buffer) => print(buffer.value),
        None => print(-1)
    }
}
"#,
    );
    let interpreted = run(&["--run", &file]);
    let exe = temp_artifact("e1101-channel-record-transfer.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    let threaded_exe = temp_artifact("e1101-channel-record-transfer-threads.exe");
    let threaded_compile = Command::new(env!("CARGO_BIN_EXE_ostrinc"))
        .args([
            "--compile",
            "--native-threads",
            "--leak-check",
            "--out",
            &threaded_exe,
            &file,
        ])
        .env("OSTRIN_NO_IR_CODEGEN", "1")
        .env("OSTRIN_NO_HIR_CODEGEN", "1")
        .output()
        .expect("compile transfer test through the AST backend");
    let _ = fs::remove_file(&file);

    assert!(
        interpreted.status.success(),
        "interpreter rejected use by the receiving binding: {}",
        stderr(&interpreted)
    );
    assert_eq!(stdout(&interpreted).trim(), "7");
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "native compilation failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native transfer test");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).trim(), "7");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"));

    if skip_if_no_c_compiler(&threaded_compile) {
        return;
    }
    assert!(threaded_compile.status.success(), "native-thread compilation failed: {}", stderr(&threaded_compile));
    let threaded = Command::new(&threaded_exe)
        .output()
        .expect("run native-thread transfer test");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded.status.success(), "native-thread run failed: {}", String::from_utf8_lossy(&threaded.stderr));
    assert_eq!(String::from_utf8_lossy(&threaded.stdout).trim(), "7");
    assert!(
        String::from_utf8_lossy(&threaded.stderr).contains("live_allocations=0"),
        "AST native-thread transfer left allocations live: {}",
        String::from_utf8_lossy(&threaded.stderr)
    );

    let invalid_file = temp_source(
        "e1101-channel-stale-alias.ostrin",
        r#"
record Buffer {
    mut value: Int
}

fn main() -> Void {
    ch = channel<Buffer>()
    payload = Buffer { value: 7 }
    stale_alias = payload
    ch.send(payload)
    received = ch.receive()
    ch.close()
    print(stale_alias.value)
}
"#,
    );
    let invalid = run(&["--run", &invalid_file]);
    let _ = fs::remove_file(&invalid_file);
    assert!(
        !invalid.status.success(),
        "receiving the value must not restore the sender's stale alias"
    );
    assert!(
        stderr(&invalid).contains("OSTRIN-E1101"),
        "stale sender alias was not rejected by static ownership checking: {}",
        stderr(&invalid)
    );
}

#[test]
fn native_backend_compiles_and_runs_records() {
    // Nested, heap-allocated records with a `mut` field mutated through its
    // binding, a record passed by identity into another function, and a
    // record literal nesting another record literal as one of its fields.
    let exe = temp_artifact("records.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_records.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));

    let run_output = Command::new(&exe).output().unwrap_or_else(|e| panic!("failed to run compiled binary '{exe}': {e}"));
    let _ = fs::remove_file(&exe);
    assert!(run_output.status.success(), "compiled binary exited unsuccessfully");
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "11\n2\n13\n",
        "native binary should match the interpreter's output for the same program"
    );
}

#[test]
fn native_backend_compiles_and_runs_record_methods() {
    // Static (compile-time-resolved) method dispatch: `Counter.increment`
    // mutates through a `mut self` receiver shared via the record's
    // pointer identity, and `Point.manhattan_distance` calls a plain
    // top-level function from inside a method body.
    let exe = temp_artifact("methods.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_methods.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));

    let run_output = Command::new(&exe).output().unwrap_or_else(|e| panic!("failed to run compiled binary '{exe}': {e}"));
    let _ = fs::remove_file(&exe);
    assert!(run_output.status.success(), "compiled binary exited unsuccessfully");
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "7\n7\n",
        "native binary should match the interpreter's output for the same program"
    );
}

#[test]
fn native_backend_compiles_and_runs_enums_and_match() {
    // A named-field variant (`Circle(radius: Int)`, constructed with a
    // named argument), a positional-field variant (`Rectangle(Int, Int)`,
    // whose fields the pattern `Rectangle(width, height)` must resolve by
    // position, not name), a unit variant, and a `match` over a plain Int
    // exercising a literal, a range, a guard that reads its own binding,
    // and a wildcard fallback.
    let exe = temp_artifact("enums.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_enums.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));

    let run_output = Command::new(&exe).output().unwrap_or_else(|e| panic!("failed to run compiled binary '{exe}': {e}"));
    let _ = fs::remove_file(&exe);
    assert!(run_output.status.success(), "compiled binary exited unsuccessfully");
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "27\n20\n0\nzero\nsmall\nnegative\nlarge\n",
        "native binary should match the interpreter's output for the same program"
    );
}

#[test]
fn native_backend_compiles_and_runs_generic_functions() {
    // Monomorphization: `identity<T>` is called with Int, String and a
    // record (Pair), so it must be emitted three times — once per concrete
    // type actually used, never a single generic C function — and `max<T>`
    // exercises a generic function whose body itself isn't trivial (an
    // `if`-expression comparing its own type parameter).
    let exe = temp_artifact("generics.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_generics.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));

    let run_output = Command::new(&exe).output().unwrap_or_else(|e| panic!("failed to run compiled binary '{exe}': {e}"));
    let _ = fs::remove_file(&exe);
    assert!(run_output.status.success(), "compiled binary exited unsuccessfully");
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "42\nhello\n7\n9\n9\n",
        "native binary should match the interpreter's output for the same program"
    );
}

#[test]
fn native_backend_emits_one_c_function_per_concrete_instantiation() {
    // Same source as above, inspected via --emit-c instead of run: there
    // must be exactly one definition per (function, concrete type) pair
    // actually called, reused across repeated calls with the same type
    // rather than duplicated.
    let out = run(&["--emit-c", &example_path("native_generics.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert_eq!(source.matches("identity__Int(int64_t value)").count(), 1);
    assert_eq!(source.matches("identity__String(const char* value)").count(), 1);
    assert_eq!(source.matches("identity__Pair(Pair* value)").count(), 1);
    assert_eq!(source.matches("max__Int(int64_t a, int64_t b)").count(), 1);
    assert!(!source.contains("<T>"), "no generic syntax should leak into the generated C: {source}");
}

#[test]
fn native_backend_compiles_and_runs_dyn_trait() {
    // A standalone `dyn Shape` (no `List`, which the native backend doesn't
    // support at all): a function parameter typed `dyn Shape` boxing two
    // different concrete records at two call sites, an explicitly-typed
    // `dyn Shape` binding, and a second trait method (`scale`) called
    // through the same dyn value — exercising the vtable dispatch itself,
    // not just a single trivial method.
    let exe = temp_artifact("dyn_trait.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_dyn_trait.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));

    let run_output = Command::new(&exe).output().unwrap_or_else(|e| panic!("failed to run compiled binary '{exe}': {e}"));
    let _ = fs::remove_file(&exe);
    assert!(run_output.status.success(), "compiled binary exited unsuccessfully");
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "12\n9\n3\n4\n",
        "native binary should match the interpreter's output for the same program"
    );
}

#[test]
fn native_backend_dedups_vtables_across_repeated_boxing() {
    // `Circle` is boxed into `dyn Shape` twice in native_dyn_trait.ostrin
    // (once via describe(), once via the explicit `boxed: dyn Shape =`
    // binding); its vtable must be emitted exactly once, not duplicated
    // (which would be a C "redefinition" compile error, not just wasteful).
    let out = run(&["--emit-c", &example_path("native_dyn_trait.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    assert_eq!(source.matches("Shape__Circle__vtable = {").count(), 1, "Circle's vtable should be emitted once: {source}");
    assert_eq!(source.matches("Shape__Square__vtable = {").count(), 1, "Square's vtable should be emitted once: {source}");
}

#[test]
fn native_backend_compiles_and_runs_lists() {
    // Three distinct monomorphized List instantiations in one program
    // (List<Int>, List<Point> and List<String>): literals, `.length()`,
    // `.push()`, indexing, `.remove_at()`, `for x in list`, and a plain
    // function taking `List<Int>` as a parameter (register_list_types'
    // reason to exist: nothing else in that function's own body
    // constructs a list, so its signature is the only thing that would
    // ever discover List_Int without it).
    let exe = temp_artifact("lists.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_lists.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));

    let run_output = Command::new(&exe).output().unwrap_or_else(|e| panic!("failed to run compiled binary '{exe}': {e}"));
    let _ = fs::remove_file(&exe);
    assert!(run_output.status.success(), "compiled binary exited unsuccessfully");
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "4\n5\n1\n5\n15\n1\n4\n14\n3\n7\na\nb\nc\n",
        "native binary should match the interpreter's output for the same program"
    );
}

#[test]
fn native_backend_compiles_and_runs_list_combinators_with_captures() {
    // map/filter/fold/any/all with lambdas that capture an enclosing local
    // (`offset`), including a `map` that changes the element type.
    let exe = temp_artifact("closures.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_closures.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let run_output = Command::new(&exe).output().unwrap();
    let _ = fs::remove_file(&exe);
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "11\n12\n13\n14\n15\n16\n3\n210\ntrue\nfalse\nn!\nn!\nn!\nn!\nn!\nn!\n"
    );
}

#[test]
fn native_backend_compiles_the_original_dyn_trait_example() {
    // The project's own dyn_trait.ostrin: List<dyn Shape>, .fold() with a
    // lambda calling a trait method through the vtable, and Float printing
    // that must match the interpreter digit for digit.
    let exe = temp_artifact("dyn_trait_original.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("dyn_trait.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let run_output = Command::new(&exe).output().unwrap();
    let _ = fs::remove_file(&exe);
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "12.56636\n9\n3.14159\n24.70795\n"
    );
}

#[test]
fn native_backend_compiles_and_runs_option() {
    // Some/None (a bare `None` typed by return position, `if` arms and call
    // arguments), match on Option, is_some/unwrap/unwrap_or, and List.find.
    let exe = temp_artifact("option.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("native_option.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let run_output = Command::new(&exe).output().unwrap();
    let _ = fs::remove_file(&exe);
    assert_eq!(
        String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"),
        "value\nnothing\ntrue\n3\n-1\n5\ntrue\n6\n"
    );
}

#[test]
fn native_backend_compiles_and_runs_result_and_try() {
    // Result<T, E> with Ok/Err typed from the return position, early return
    // through `try` for both Result and Option, a `catch` error handler that
    // maps the error type, match on Ok/Err, and a call as a `match`
    // scrutinee (which used to be misparsed as a trailing closure).
    for (file, expected) in [
        ("native_result.ostrin", "ok\nnegative\n10\ntrue\n0\n4\ntrue\n"),
        ("native_result_catch.ostrin", "2\nbad code\n"),
    ] {
        let interpreted = run(&["--run", &example_path(file)]);
        assert!(interpreted.status.success(), "interpreter failed on {file}: {}", stderr(&interpreted));
        assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected, "interpreter output for {file}");

        let exe = temp_artifact(&format!("{file}.exe"));
        let compile = run(&["--compile", "--out", &exe, &example_path(file)]);
        if skip_if_no_c_compiler(&compile) {
            return;
        }
        assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
        let run_output = Command::new(&exe).output().unwrap();
        let _ = fs::remove_file(&exe);
        assert_eq!(String::from_utf8_lossy(&run_output.stdout).replace("\r\n", "\n"), expected, "native output for {file}");
    }
}

#[test]
fn native_hir_handles_option_result_core() {
    // The structural Option/Result family now comes from HIR: constructors,
    // match, basic queries, unwrap/unwrap_or, ok/ok_or and propagation with
    // `try`. Result and scalar/String Option combinators with inline lambdas
    // are covered by the native IR tests; non-inline handlers remain fallback
    // paths.
    for (file, minimum_hir_functions) in [
        ("native_option.ostrin", 2usize),
        ("native_hir_option_locals.ostrin", 1usize),
        ("native_result.ostrin", 6usize),
        ("native_result_catch.ostrin", 3usize),
        ("native_ir_result_combinators.ostrin", 9usize),
        ("try_result.ostrin", 5usize),
    ] {
        let report = run(&["--native-type-report", &example_path(file)]);
        if skip_if_no_c_compiler(&report) {
            return;
        }
        assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
        // Functions migrate from HIR to the IR emitter over time; count both.
        let hir_functions: usize = stdout(&report)
            .lines()
            .filter_map(|line| {
                line.strip_prefix("hir-generated: ")
                    .or_else(|| line.strip_prefix("ir-generated: "))
                    .and_then(|n| n.trim().parse::<usize>().ok())
            })
            .sum();
        assert!(hir_functions >= minimum_hir_functions, "{file} generated only {hir_functions} HIR function(s), expected at least {minimum_hir_functions}");
    }
}

#[test]
fn native_hir_handles_scalar_widths() {
    // Float32 and fixed-width integer functions should use the HIR emitter
    // without losing single-precision rounding or checked integer overflow.
    for (file, expected) in [
        (
            "float32.ostrin",
            "0.3\ntrue\n0.30000000000000004\n0.33333334\n16777216\n3\n7\n0.10000000149011612\n25\n[1.5, 2.5]\n-0.1\n1.5\n0.1!\n",
        ),
        (
            "sized_ints.ostrin",
            "255\n145\n200\ntrue\ntrue\n-100\n100\n-120\n18446744073709551615\n300\n1000\n7\n10\n256\n1073741823\n2147483647\n2147483647!\n",
        ),
    ] {
        let path = example_path(file);
        let interpreted = run(&["--run", &path]);
        assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
        assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected, "interpreter output for {file}");

        let report = run(&["--native-type-report", &path]);
        if skip_if_no_c_compiler(&report) {
            return;
        }
        assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
        let report_text = stdout(&report);
        let hir_functions = report_text
            .lines()
            .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
            .unwrap_or(0);
        let ir_functions = report_text
            .lines()
            .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
            .unwrap_or(0);
        assert!(hir_functions + ir_functions >= 1, "{file} did not generate any function from HIR/IR");

        let exe = temp_artifact(&format!("hir_{file}.exe"));
        let compile = run(&["--compile", "--out", &exe, &path]);
        if skip_if_no_c_compiler(&compile) {
            return;
        }
        assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
        let native = Command::new(&exe).output().expect("failed to run scalar HIR binary");
        let _ = fs::remove_file(&exe);
        assert!(native.status.success(), "native run failed for {file}: {}", String::from_utf8_lossy(&native.stderr));
        assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected, "native output for {file}");
    }
}

#[test]
fn native_ir_emitter_handles_scalar_functions() {
    let file = example_path("int_division.ostrin");
    let expected = "3\n3\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 1, "scalar example did not exercise the IR native emitter: {}", stdout(&report));

    let exe = temp_artifact("native_ir_scalar.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run IR scalar binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn channel_move_analysis_respects_mutually_exclusive_cfg_paths() {
    let file = temp_source(
        "e1101-exclusive-paths.ostrin",
        r#"
record Buffer {
    mut value: Int
}

fn inspect(flag: Bool) -> Void {
    ch = channel<Buffer>()
    mut buffer = Buffer { value: 5 }
    if flag {
        ch.send(buffer)
    } else {
        print(buffer.value)
    }
    ch.close()
}

fn main() -> Void {
    inspect(false)
}
"#,
    );
    let checked = run(&["--ownership-check", &file]);
    let typechecked = run(&["--check", &file]);
    let emitted = run(&["--emit-c", &file]);
    let executed = run(&["--run", &file]);
    let compiled = run(&["--compile", &file]);
    let _ = fs::remove_file(&file);

    assert!(
        checked.status.success(),
        "exclusive paths were reported as a move violation: {}",
        stdout(&checked)
    );
    assert!(
        typechecked.status.success(),
        "--check rejected mutually exclusive paths: {}",
        stderr(&typechecked)
    );
    assert!(
        emitted.status.success(),
        "--emit-c rejected mutually exclusive paths: {}",
        stderr(&emitted)
    );
    assert!(
        executed.status.success(),
        "valid non-sending branch was rejected: {}",
        stderr(&executed)
    );
    assert_eq!(stdout(&executed).trim(), "5");
    if !skip_if_no_c_compiler(&compiled) {
        assert!(
            compiled.status.success(),
            "native compilation rejected exclusive paths: {}",
            stderr(&compiled)
        );
    }
}

#[test]
fn channel_move_analysis_checks_only_the_selected_phi_input() {
    let file = temp_source(
        "e1101-phi-exclusive-paths.ostrin",
        r#"
record Buffer {
    mut value: Int
}

fn choose_buffer(flag: Bool) -> Void {
    ch = channel<Buffer>()
    mut buffer = Buffer { value: 5 }
    mut selected = buffer
    if flag {
        ch.send(buffer)
        selected = Buffer { value: 9 }
    } else {
        selected = buffer
    }
    ch.close()
    print(selected.value)
}

fn main() -> Void {
    choose_buffer(true)
}
"#,
    );
    let checked = run(&["--ownership-check", &file]);
    let typechecked = run(&["--check", &file]);
    let emitted = run(&["--emit-c", &file]);
    let executed = run(&["--run", &file]);
    let _ = fs::remove_file(&file);

    assert!(
        checked.status.success(),
        "a non-selected Phi input was treated as a use: stdout={} stderr={}",
        stdout(&checked),
        stderr(&checked)
    );
    assert!(
        typechecked.status.success(),
        "--check rejected a non-selected Phi input: {}",
        stderr(&typechecked)
    );
    assert!(
        emitted.status.success(),
        "--emit-c rejected a non-selected Phi input: {}",
        stderr(&emitted)
    );
    assert!(
        executed.status.success(),
        "valid path-sensitive Phi was rejected: {}",
        stderr(&executed)
    );
    assert_eq!(stdout(&executed).trim(), "9");
}

#[test]
fn channel_move_analysis_follows_branch_joins_and_loop_backedges() {
    let file = temp_source(
        "e1101-cfg-joins.ostrin",
        r#"
record Buffer {
    mut value: Int
}

fn after_join(flag: Bool) -> Void {
    ch = channel<Buffer>()
    mut buffer = Buffer { value: 5 }
    if flag {
        ch.send(buffer)
    }
    print(buffer.value)
}

fn after_backedge() -> Void {
    ch = channel<Buffer>()
    mut buffer = Buffer { value: 5 }
    while buffer.value > 0 {
        ch.send(buffer)
    }
}

fn phi_after_move(flag: Bool) -> Void {
    ch = channel<Buffer>()
    mut buffer = Buffer { value: 5 }
    mut selected = Buffer { value: 11 }
    if flag {
        ch.send(buffer)
        selected = buffer
    } else {
        selected = Buffer { value: 13 }
    }
    print(selected.value)
}

fn main() -> Void {}
"#,
    );
    let checked = run(&["--ownership-check", &file]);
    let typechecked = run(&["--check", &file]);
    let emitted = run(&["--emit-c", &file]);
    let interpreted = run(&["--run", &file]);
    let compiled = run(&["--compile", &file]);
    let _ = fs::remove_file(&file);
    let report = stdout(&checked);

    assert!(
        !checked.status.success(),
        "a path that uses a moved value was not rejected: {report}"
    );
    assert!(
        report.matches("OSTRIN-E1101").count() >= 4,
        "expected join, loop and moved-phi diagnostics: {report}"
    );
    assert_eq!(
        report.matches("after_join").count(),
        1,
        "expected one post-join use: {report}"
    );
    assert_eq!(
        report.matches("after_backedge").count(),
        2,
        "expected condition and repeated-send uses: {report}"
    );
    assert!(
        report.contains("phi_after_move"),
        "missing moved Phi diagnostic: {report}"
    );
    assert!(
        report.contains("after_join"),
        "missing post-join diagnostic: {report}"
    );
    assert!(
        report.contains("after_backedge"),
        "missing loop-backedge diagnostic: {report}"
    );
    assert!(
        !typechecked.status.success(),
        "--check must reject a possible use after move"
    );
    assert!(
        stderr(&typechecked).contains("OSTRIN-E1101"),
        "missing --check E1101: {}",
        stderr(&typechecked)
    );
    assert!(
        !emitted.status.success(),
        "--emit-c must reject a possible use after move"
    );
    assert!(
        stderr(&emitted).contains("OSTRIN-E1101"),
        "missing --emit-c E1101: {}",
        stderr(&emitted)
    );
    assert!(
        !interpreted.status.success(),
        "interpreter entry point must reject E1101: {}",
        stderr(&interpreted)
    );
    assert!(
        stderr(&interpreted).contains("OSTRIN-E1101"),
        "missing interpreter diagnostic: {}",
        stderr(&interpreted)
    );
    assert!(
        !compiled.status.success(),
        "native entry point must reject E1101 before codegen"
    );
    assert!(
        stderr(&compiled).contains("OSTRIN-E1101"),
        "missing native diagnostic: {}",
        stderr(&compiled)
    );
}

#[test]
fn native_ir_emitter_handles_directional_integer_ranges() {
    let file = example_path("native_ir_ranges.ostrin");
    let expected = "25\n20\n19\n0\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert_eq!(ir_functions, 5, "all range functions should use the IR emitter: {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "range example fell back to HIR: {report_text}");

    let exe = temp_artifact("native_ir_ranges.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run range IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "range IR leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_user_iterator_protocol() {
    let file = example_path("fibonacci.ostrin");
    let expected = "0\n1\n1\n2\n3\n5\n8\n13\n21\n34\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 1, "iterator consumer did not use the IR emitter: {report_text}");

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "iterator IR emission failed: {}", stderr(&emitted));
    assert!(stdout(&emitted).contains("Fibonacci__next(__ir_v"), "iterator method call missing from IR C: {}", stdout(&emitted));

    let exe = temp_artifact("native_ir_user_iterator.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run user iterator IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "user iterator leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_generic_iterator_protocol() {
    let file = example_path("native_generic_iterator.ostrin");
    let expected = "7\n7\n7\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);
    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    assert!(report_text.contains("ir-generated: 2"), "generic iterator did not use IR: {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "generic iterator fell back to HIR: {report_text}");
    assert!(report_text.contains("divergences: 0"), "generic iterator diverged: {report_text}");
    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "generic iterator IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    assert!(source.contains("Cursor__Int__next(__ir_v"), "generic iterator call missing from IR C: {source}");
    assert!(!source.contains("iter_init"), "generic iterator used the legacy ABI: {source}");
    let exe = temp_artifact("native_ir_generic_iterator.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) { return; }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run generic iterator IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "generic iterator leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_channel_iterator_protocol() {
    let file = example_path("native_ir_channel_iterator.ostrin");
    let expected = "3\ntrue\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    assert!(report_text.contains("ir-generated: 1"), "channel iterator did not use the IR emitter: {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "channel iterator fell back to HIR: {report_text}");

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "channel IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    assert!(source.contains("Channel_Int_send"), "channel send missing from IR C: {source}");
    assert!(source.contains("Channel_Int_receive"), "channel receive missing from IR C: {source}");
    assert!(source.contains("Channel_Int_close"), "channel close missing from IR C: {source}");

    let exe = temp_artifact("native_ir_channel_iterator.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run channel iterator IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "channel iterator leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);

    let threaded_exe = temp_artifact("native_ir_channel_iterator_threads.exe");
    let threaded_compile = run(&["--native-threads", "--compile", "--leak-check", "--out", &threaded_exe, &file]);
    if skip_if_no_c_compiler(&threaded_compile) {
        return;
    }
    assert!(threaded_compile.status.success(), "native-threads compile failed: {}", stderr(&threaded_compile));
    let threaded = Command::new(&threaded_exe).output().expect("failed to run threaded channel iterator IR binary");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded.status.success(), "threaded native run failed: {}", String::from_utf8_lossy(&threaded.stderr));
    assert!(String::from_utf8_lossy(&threaded.stderr).contains("live_allocations=0"), "threaded channel iterator leaked: {}", String::from_utf8_lossy(&threaded.stderr));
    assert_eq!(String::from_utf8_lossy(&threaded.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_spawn_cfg_scope_and_nested_join() {
    let file = example_path("native_ir_spawn_join.ostrin");
    let expected = "7\n7\ncaptured\ninside task\n21\n6\n8\ncaptured!\n3\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    assert!(report_text.contains("ir-generated: 1"), "spawn/join did not use the IR emitter: {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "spawn/join fell back to HIR: {report_text}");

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "spawn/join IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    assert!(source.contains("ostrin_ir_task_ostrin_main_"), "spawn callback missing from IR C: {source}");
    assert!(source.contains("OstrinIrTaskEnv_ostrin_main_"), "captured task environment missing from IR C: {source}");
    assert!(source.contains("ostrin_ir_task_env_drop_ostrin_main_"), "captured task drop helper missing from IR C: {source}");
    assert!(source.contains("__e->__ir_v16"), "spawn CFG did not read its captured branch condition from the environment: {source}");
    assert!(source.contains("__ostrin_ir_bb6"), "spawn CFG branch target missing from IR C: {source}");
    assert!(source.contains("ostrin_scope_begin()"), "spawn_scope did not lower to the native scope runtime: {source}");
    assert!(source.contains("ostrin_scope_end(__ostrin_ir_scope_0)"), "spawn_scope cleanup missing from IR C: {source}");
    assert!(source.contains("ostrin_scope_end(__ostrin_ir_scope_1)"), "break path did not close its nested scope in IR C: {source}");
    assert!(source.matches("ostrin_ir_task_ostrin_main_").count() >= 8, "nested native task callbacks missing from IR C: {source}");
    assert!(source.contains("Task_Int_join"), "task join missing from IR C: {source}");
    assert!(source.contains("Task_String_join"), "managed task join missing from IR C: {source}");

    let exe = temp_artifact("native_ir_spawn_join.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run spawn/join IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "spawn/join leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);

    let threaded_exe = temp_artifact("native_ir_spawn_join_threads.exe");
    let threaded_compile = run(&["--native-threads", "--compile", "--leak-check", "--out", &threaded_exe, &file]);
    if skip_if_no_c_compiler(&threaded_compile) {
        return;
    }
    assert!(threaded_compile.status.success(), "native-threads compile failed: {}", stderr(&threaded_compile));
    let threaded = Command::new(&threaded_exe).output().expect("failed to run threaded spawn/join IR binary");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded.status.success(), "threaded native run failed: {}", String::from_utf8_lossy(&threaded.stderr));
    assert!(String::from_utf8_lossy(&threaded.stderr).contains("live_allocations=0"), "threaded spawn/join leaked: {}", String::from_utf8_lossy(&threaded.stderr));
    assert_eq!(String::from_utf8_lossy(&threaded.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_task_cancel() {
    let file = example_path("native_ir_task_cancel.ostrin");
    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "task cancel type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    assert!(report_text.contains("ir-generated: 1"), "task cancel did not use the IR emitter: {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "task cancel fell back to HIR: {report_text}");

    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter task cancel failed: {}", stderr(&interpreted));
    let expected = "true\nfalse\n";
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let exe = temp_artifact("native-ir-task-cancel.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "native task cancel compile failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native task cancel binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native task cancel failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "native task cancel leaked: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_exe = temp_artifact("native-ir-task-cancel-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--leak-check", "--out", &threaded_exe, &file]);
    assert!(threaded_compile.status.success(), "native-thread task cancel compile failed: {}", stderr(&threaded_compile));
    let threaded = Command::new(&threaded_exe).output().expect("run native-thread task cancel binary");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded.status.success(), "native-thread task cancel failed: {}", String::from_utf8_lossy(&threaded.stderr));
    let threaded_stdout = String::from_utf8_lossy(&threaded.stdout).replace("\r\n", "\n");
    assert!(
        threaded_stdout.lines().count() == 2
            && threaded_stdout.lines().all(|line| matches!(line, "true" | "false")),
        "native-thread task cancel returned an invalid result sequence: {threaded_stdout}"
    );
    assert!(String::from_utf8_lossy(&threaded.stderr).contains("live_allocations=0"), "native-thread task cancel leaked: {}", String::from_utf8_lossy(&threaded.stderr));

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "task cancel C emission failed: {}", stderr(&emitted));
    assert!(stdout(&emitted).contains("Task_Int_cancel"), "native C did not call the typed cancel helper");
}

#[test]
fn native_ir_emitter_handles_yield_with_task_runtime() {
    let file = example_path("native_ir_yield.ostrin");
    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "yield type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    assert!(report_text.contains("ir-generated: 1"), "yield did not use the IR emitter: {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "yield fell back to HIR: {report_text}");

    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter yield failed: {}", stderr(&interpreted));
    let expected = "7\n";
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let exe = temp_artifact("native-ir-yield.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "native yield compile failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native yield binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native yield failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "native yield leaked: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_exe = temp_artifact("native-ir-yield-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--leak-check", "--out", &threaded_exe, &file]);
    assert!(threaded_compile.status.success(), "native-thread yield compile failed: {}", stderr(&threaded_compile));
    let threaded = Command::new(&threaded_exe).output().expect("run native-thread yield binary");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded.status.success(), "native-thread yield failed: {}", String::from_utf8_lossy(&threaded.stderr));
    assert_eq!(String::from_utf8_lossy(&threaded.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&threaded.stderr).contains("live_allocations=0"), "native-thread yield leaked: {}", String::from_utf8_lossy(&threaded.stderr));

    let emitted = run(&["--emit-c", "--native-threads", &file]);
    assert!(emitted.status.success(), "yield C emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    assert!(source.contains("ostrin_select_wait()"));
    assert!(source.contains("ostrin_poll_one()"));
    assert!(source.contains("ostrin_task_checkpoint()"));
}

#[test]
fn native_ir_emitter_handles_select_over_channels() {
    let file = example_path("native_ir_select.ostrin");
    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "select type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    assert!(report_text.contains("ir-generated: 1"), "select did not use the IR emitter: {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "select fell back to HIR: {report_text}");

    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter select failed: {}", stderr(&interpreted));
    let expected = "7\n";
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let exe = temp_artifact("native-ir-select.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "native select compile failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native select binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native select failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "native select leaked: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_exe = temp_artifact("native-ir-select-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--leak-check", "--out", &threaded_exe, &file]);
    assert!(threaded_compile.status.success(), "native-thread select compile failed: {}", stderr(&threaded_compile));
    let threaded = Command::new(&threaded_exe).output().expect("run native-thread select binary");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded.status.success(), "native-thread select failed: {}", String::from_utf8_lossy(&threaded.stderr));
    assert_eq!(String::from_utf8_lossy(&threaded.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&threaded.stderr).contains("live_allocations=0"), "native-thread select leaked: {}", String::from_utf8_lossy(&threaded.stderr));

    let emitted = run(&["--emit-c", "--native-threads", &file]);
    assert!(emitted.status.success(), "select C emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    assert!(source.contains("List_Channel_Int_new_from_array"));
    assert!(source.contains("Channel_Int_try_receive"));
    assert!(source.contains("ostrin_task_checkpoint()"));
}

#[test]
fn native_ir_emitter_handles_strings_and_ownership_markers() {
    let file = example_path("native_ir_strings.ostrin");
    let expected = "true\nfalse\nHello, Ostrin\nfallback\nHello, Alias\nalias-fallback\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 6, "string example did not use the IR emitter: {}", stdout(&report));

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "string IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    assert!(source.contains("ostrin_str_concat(__ir_v"), "string concatenation did not come from IR: {source}");
    assert!(source.contains("strcmp(__ir_v"), "string equality did not come from IR: {source}");
    assert!(source.contains("ostrin_release((void*)__ir_v"), "IR ownership release marker was not emitted: {source}");
    assert!(!source.contains("ostrin_retain((void*)__ir_v"), "Phi ownership transfer should not retain the incoming branch value: {source}");

    let exe = temp_artifact("native_ir_strings.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run string IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "string IR ownership leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_releases_managed_loop_phi_values() {
    let file = example_path("native_ir_managed_loop.ostrin");
    let expected = "startxxx\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let ownership = run(&["--ownership-ir", &file]);
    assert!(ownership.status.success(), "ownership lowering failed: {}", stderr(&ownership));
    let ownership_source = stdout(&ownership);
    assert!(ownership_source.contains("phi"), "managed loop did not lower through a Phi: {ownership_source}");
    assert!(
        ownership_source.lines().filter(|line| line.trim_start().starts_with("release %")).count() >= 2,
        "loop-carried String was not released on its backedge: {ownership_source}"
    );

    let exe = temp_artifact("native_ir_managed_loop.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run managed loop binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "managed loop Phi ownership leaked: {}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn native_ir_emitter_handles_lists_and_ownership_markers() {
    let file = example_path("native_ir_lists.ostrin");
    let expected = "4\n4\n5\n4\n4\n0\n2\nA-one\n1\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 4, "list example did not use the IR emitter: {}", stdout(&report));

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "list IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    assert!(source.contains("List_Int_new_from_array"), "list construction did not come from IR: {source}");
    assert!(source.contains("List_String_new_from_array"), "managed string list construction did not come from IR: {source}");
    assert!(source.contains("List_Int_push"), "list push did not come from IR: {source}");
    assert!(source.contains("List_Int_get"), "list indexing did not come from IR: {source}");
    assert!(source.contains("List_Int_remove_at"), "list removal did not come from IR: {source}");
    assert!(source.contains("ostrin_release((void*)__ir_v"), "IR list ownership release marker was not emitted: {source}");

    let exe = temp_artifact("native_ir_lists.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run list IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "list IR ownership leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_record_lists() {
    let file = example_path("native_ir_record_lists.ostrin");
    let expected = "3\n2\npt\n1\n2\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 3, "record list example did not use the IR emitter: {}", stdout(&report));

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "record list IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    assert!(source.contains("List_Point_new_from_array"), "record list construction missing: {source}");
    assert!(source.contains("List_Point_remove_at"), "record list removal missing: {source}");

    let exe = temp_artifact("native_ir_record_lists.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run record list binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "record list leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_for_lists() {
    let file = example_path("native_ir_for_lists.ostrin");
    let expected = "6
ab
";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 3, "for list example did not use the IR emitter: {}", stdout(&report));

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "for list IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    assert!(source.contains("List_Int_length"), "for list construction missing: {source}");
    assert!(source.contains("List_String_get"), "for list removal missing: {source}");

    let exe = temp_artifact("native_ir_for_lists.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run for list binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "for list leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn fmt_normalizes_layout_and_supports_write_and_check() {
    let messy = "fn main() -> Void {\r\nprint(\"{\")   \n\n\n    if true {\nprint(1)\n}\n}\n\n";
    let clean = "fn main() -> Void {\n    print(\"{\")\n\n    if true {\n        print(1)\n    }\n}\n";
    let path = temp_artifact("fmt_messy.ostrin");
    fs::write(&path, messy).unwrap();

    let printed = run(&["--fmt", &path]);
    assert!(printed.status.success(), "fmt failed: {}", stderr(&printed));
    assert_eq!(stdout(&printed).replace("\r\n", "\n"), clean);

    let check = run(&["--fmt", "--check", &path]);
    assert!(!check.status.success(), "--check must fail on unformatted input");

    let write = run(&["--fmt", "--write", &path]);
    assert!(write.status.success(), "fmt --write failed: {}", stderr(&write));
    assert_eq!(fs::read_to_string(&path).unwrap(), clean);

    let check = run(&["--fmt", "--check", &path]);
    assert!(check.status.success(), "--check must pass after --write: {}", stderr(&check));
    let _ = fs::remove_file(&path);
}

#[test]
fn native_ir_merges_branch_and_loop_bindings_and_supports_rem() {
    // (file, expected stdout, minimum IR-generated functions)
    for (file, expected, minimum_ir) in [
        ("native_ir_branch_merge.ostrin", "5\n0\n5\n7\n", 3usize),
        ("native_ir_break_continue.ostrin", "9\n9\nab\n802\n", 4usize),
        ("rem_operator.ostrin", "1\n-1\n1\n1.5\n4\n12\n", 1usize),
    ] {
        let path = example_path(file);
        let interpreted = run(&["--run", &path]);
        assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
        assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected, "interpreter output for {file}");

        let report = run(&["--native-type-report", &path]);
        if skip_if_no_c_compiler(&report) {
            return;
        }
        let ir_functions = stdout(&report)
            .lines()
            .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
            .unwrap_or(0);
        assert!(ir_functions >= minimum_ir, "{file} generated only {ir_functions} IR function(s): {}", stdout(&report));

        let exe = temp_artifact(&format!("merge_{file}.exe"));
        let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
        assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
        let native = Command::new(&exe).output().expect("failed to run native binary");
        let _ = fs::remove_file(&exe);
        assert!(native.status.success(), "native run failed for {file}");
        assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "{file} leaked: {}", String::from_utf8_lossy(&native.stderr));
        assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected, "native output for {file}");
    }
}

#[test]
fn rem_operator_reports_zero_divisor_and_type_errors() {
    let zero = temp_artifact("rem_zero.ostrin");
    fs::write(&zero, "fn main() -> Void {\n    z = 0\n    print(5 % z)\n}\n").unwrap();
    let output = run(&["--run", &zero]);
    assert!(!output.status.success(), "division by zero must fail");
    assert!(stderr(&output).contains("division by zero"), "unexpected stderr: {}", stderr(&output));
    let _ = fs::remove_file(&zero);

    let bad = temp_artifact("rem_type.ostrin");
    fs::write(&bad, "fn main() -> Void {\n    print(\"a\" % 2)\n}\n").unwrap();
    let output = run(&[&bad]);
    assert!(!output.status.success());
    assert!(stdout(&output).contains("E1041") || stderr(&output).contains("E1041"), "expected E1041");
    let _ = fs::remove_file(&bad);
}

#[test]
fn native_ir_string_methods_cross_block_ownership_and_short_circuit() {
    // (file, expected stdout, minimum IR-generated functions)
    for (file, expected, minimum_ir) in [
        ("native_ir_string_methods.ostrin", "OSTRIN!\nmixed\na+b+c\n0\n30\n5\ntrue\n4\n2\n", 3usize),
        ("native_ir_string_results.ostrin", "41\nfalse\n0\n3.25\nfalse\n0\ntrue\ntrue\n9\n7\n8\n", 3usize),
        ("native_ir_try_strings.ostrin", "VALUE!\ntrue\nrecovered: FAILURE\n", 5usize),
        ("native_ir_cross_block_ownership.ostrin", "item-x\nitem-x!\n9\n2\n4\n24\n", 7usize),
        ("short_circuit.ostrin", "false\ntrue\ntrue\nfalse\n", 3usize),
        ("native_ir_param_ownership.ostrin", "abcd\nabcd\n", 3usize),
        ("native_ir_print_compound.ostrin", "[ab, c]\nPoint { x: 3, label: pt }\nSome(yes)\nNone\n[1, 2, 3]\n[k: vw]\n", 4usize),
    ] {
        let path = example_path(file);
        let interpreted = run(&["--run", &path]);
        assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
        assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected, "interpreter output for {file}");

        let report = run(&["--native-type-report", &path]);
        if skip_if_no_c_compiler(&report) {
            return;
        }
        let ir_functions = stdout(&report)
            .lines()
            .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
            .unwrap_or(0);
        assert!(ir_functions >= minimum_ir, "{file} generated only {ir_functions} IR function(s): {}", stdout(&report));

        let exe = temp_artifact(&format!("xblock_{file}.exe"));
        let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
        assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
        let native = Command::new(&exe).output().expect("failed to run native binary");
        let _ = fs::remove_file(&exe);
        assert!(native.status.success(), "native run failed for {file}");
        assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "{file} leaked: {}", String::from_utf8_lossy(&native.stderr));
        assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected, "native output for {file}");
    }
}

#[test]
fn native_ir_option_combinators_preserve_some_none_and_ownership() {
    let file = "native_ir_option_combinators.ostrin";
    let path = example_path(file);
    let expected = "mapped: VALUE\nnone\n5\nnone\n";
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &path]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 9, "{file} generated only {ir_functions} IR function(s): {}", stdout(&report));

    let exe = temp_artifact("native_ir_option_combinators.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run Option combinator binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed for {file}");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "{file} leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_option_list_preserves_payload_and_ownership() {
    let file = "native_ir_option_list.ostrin";
    let path = example_path(file);
    let expected = "2\nalpha\nnone\n2\ngamma\nbad\n";
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &path]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 5, "{file} generated only {ir_functions} IR function(s): {}", stdout(&report));

    let exe = temp_artifact("native_ir_option_list.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run Option<List> binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed for {file}: {}", stderr(&compile));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "{file} leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_compound_collections_preserve_payload_and_ownership() {
    let file = "native_ir_compound_collections.ostrin";
    let path = example_path(file);
    let expected = "2\nnone\n3\nbad set\n";
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &path]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 5, "{file} generated only {ir_functions} IR function(s): {}", stdout(&report));

    let exe = temp_artifact("native_ir_compound_collections.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run compound collection binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed for {file}: {}", stderr(&compile));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "{file} leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_nested_wrappers_preserve_recursive_ownership() {
    let file = "native_ir_nested_wrappers.ostrin";
    let path = example_path(file);
    let expected = "nested\ninner none\nok\nnested error\nnested\nouter fallback\nok\nok fallback\nnested\n";
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &path]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 10, "{file} generated only {ir_functions} IR function(s): {}", stdout(&report));
    assert!(stdout(&report).contains("hir-generated: 0"), "{file} unexpectedly used a HIR fallback: {}", stdout(&report));

    let exe = temp_artifact("native_ir_nested_wrappers.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run nested wrapper binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed for {file}: {}", stderr(&compile));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "{file} leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_try_global_handler_preserves_error_ownership() {
    let file = "native_ir_try_handler.ostrin";
    let path = example_path(file);
    let expected = "VALUE!\nhandled: FAILURE\n";
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &path]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 6, "{file} generated only {ir_functions} IR function(s): {}", stdout(&report));

    let exe = temp_artifact("native_ir_try_handler.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run global try handler binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed for {file}");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "{file} leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_try_local_handler_preserves_error_ownership() {
    let file = "native_ir_try_local_handler.ostrin";
    let path = example_path(file);
    let expected = "VALUE!\nhandled: FAILURE\n";
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &path]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 6, "{file} generated only {ir_functions} IR function(s): {}", stdout(&report));

    let exe = temp_artifact("native_ir_try_local_handler.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run local try handler binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed for {file}");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "{file} leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_try_captured_handler_preserves_closure_ownership() {
    let file = "native_ir_try_captured_handler.ostrin";
    let path = example_path(file);
    let expected = "VALUE!\nhandled: FAILURE\n";
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed for {file}: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &path]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
    let report_text = stdout(&report);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 6, "{file} generated only {ir_functions} IR function(s): {report_text}");
    assert!(report_text.contains("hir-generated: 0"), "{file} unexpectedly used a HIR fallback: {report_text}");

    let exe = temp_artifact("native_ir_try_captured_handler.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run captured try handler binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed for {file}");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "{file} leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

/// Deterministic generator of small programs (integer arithmetic, `%`, `if`, `while`, `for`,
/// `break`/`continue`, `and`/`or`, plus a `String` and a `List<Int>` per function) used to compare
/// the interpreter with the native backend, including leak checks on the managed values.
struct ProgramGen {
    state: u64,
    loop_id: usize,
}

impl ProgramGen {
    fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407), loop_id: 0 }
    }

    fn next(&mut self, bound: u64) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.state >> 33) % bound
    }

    fn int_expr(&mut self, vars: &[String], depth: u32) -> String {
        if depth == 0 || self.next(4) == 0 {
            return match self.next(6) {
                0 => format!("{}", self.next(30)),
                1 => "s.length()".to_string(),
                2 => "l.length()".to_string(),
                _ => vars[self.next(vars.len() as u64) as usize].clone(),
            };
        }
        let left = self.int_expr(vars, depth - 1);
        let right = self.int_expr(vars, depth - 1);
        match self.next(6) {
            0 | 1 => format!("({left} + {right})"),
            2 => format!("({left} - {right})"),
            3 => format!("(({left} * {right}) % 1000)"),
            4 => format!("({left} / {})", 1 + self.next(9)),
            _ => format!("({left} % {})", 1 + self.next(9)),
        }
    }

    fn bool_expr(&mut self, vars: &[String], depth: u32) -> String {
        if depth > 0 && self.next(3) == 0 {
            let left = self.bool_expr(vars, depth - 1);
            let right = self.bool_expr(vars, depth - 1);
            let op = if self.next(2) == 0 { "and" } else { "or" };
            return format!("({left} {op} {right})");
        }
        let ops = ["<", ">", "==", "!=", "<=", ">="];
        let left = self.int_expr(vars, 2);
        let right = self.int_expr(vars, 2);
        format!("{left} {} {right}", ops[self.next(6) as usize])
    }

    fn assignment(&mut self, vars: &[String], pad: &str) -> String {
        let target = ["a", "b", "c"][self.next(3) as usize];
        let value = self.int_expr(vars, 3);
        format!("{pad}{target} = ({value}) % 100000\n")
    }

    fn block(&mut self, vars: &mut Vec<String>, depth: u32, in_loop: bool, indent: usize) -> String {
        let pad = "    ".repeat(indent);
        let mut out = String::new();
        for _ in 0..(1 + self.next(3)) {
            let choices = if depth >= 3 { 4 } else { 10 };
            match self.next(choices) {
                0 | 1 => out.push_str(&self.assignment(vars, &pad)),
                2 => {
                    let piece = ["\"ab\"", "\"c\"", "\"xyz\""][self.next(3) as usize];
                    let line = match self.next(3) {
                        0 => format!("s = s + {piece}"),
                        1 => format!("s = {piece} + s"),
                        _ => "s = s.to_upper()".to_string(),
                    };
                    out.push_str(&format!("{pad}{line}\n"));
                }
                3 => {
                    let value = self.int_expr(vars, 2);
                    out.push_str(&format!("{pad}l.push({value})\n"));
                }
                4 if in_loop => {
                    let keyword = if self.next(2) == 0 { "break" } else { "continue" };
                    let cond = self.bool_expr(vars, 1);
                    out.push_str(&format!("{pad}if {cond} {{\n{pad}    {keyword}\n{pad}}}\n"));
                }
                4 | 5 | 6 => {
                    let cond = self.bool_expr(vars, 2);
                    let then_body = self.block(vars, depth + 1, in_loop, indent + 1);
                    out.push_str(&format!("{pad}if {cond} {{\n{then_body}{pad}}}"));
                    if self.next(2) == 0 {
                        let else_body = self.block(vars, depth + 1, in_loop, indent + 1);
                        out.push_str(&format!(" else {{\n{else_body}{pad}}}"));
                    }
                    out.push('\n');
                }
                7 | 8 => {
                    self.loop_id += 1;
                    let counter = format!("i{}", self.loop_id);
                    let limit = 1 + self.next(5);
                    out.push_str(&format!("{pad}mut {counter} = 0\n{pad}while {counter} < {limit} {{\n"));
                    out.push_str(&format!("{pad}    {counter} = {counter} + 1\n"));
                    vars.push(counter);
                    let body = self.block(vars, depth + 1, true, indent + 1);
                    vars.pop();
                    out.push_str(&format!("{body}{pad}}}\n"));
                }
                _ => {
                    self.loop_id += 1;
                    let item = format!("x{}", self.loop_id);
                    let items: Vec<String> = (0..(1 + self.next(4))).map(|_| format!("{}", self.next(20))).collect();
                    out.push_str(&format!("{pad}for {item} in [{}] {{\n", items.join(", ")));
                    vars.push(item);
                    let body = self.block(vars, depth + 1, true, indent + 1);
                    vars.pop();
                    out.push_str(&format!("{body}{pad}}}\n"));
                }
            }
        }
        out
    }

    fn program(&mut self, functions: usize) -> String {
        let mut source = String::new();
        for index in 0..functions {
            let mut vars: Vec<String> = ["a", "b", "c", "n"].iter().map(|v| v.to_string()).collect();
            let body = self.block(&mut vars, 0, false, 1);
            source.push_str(&format!(
                "fn f{index}(n: Int) -> Int {{\n    mut a = n\n    mut b = 1\n    mut c = 2\n    mut s = \"\"\n    mut l = [n]\n{body}    (a + b + c + s.length() + l.length()) % 1000003\n}}\n\n"
            ));
        }
        source.push_str("fn main() -> Void {\n");
        for index in 0..functions {
            for arg in ["0", "1", "7", "-3"] {
                source.push_str(&format!("    print(f{index}({arg}))\n"));
            }
        }
        source.push_str("}\n");
        source
    }
}

#[test]
fn generated_programs_agree_between_interpreter_and_native_backend() {
    // `OSTRIN_FUZZ_SEEDS=200 cargo test generated_programs` widens the search.
    let seed_count: u64 = std::env::var("OSTRIN_FUZZ_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    for seed in 1..=seed_count {
        let source = ProgramGen::new(seed).program(10);
        let path = temp_artifact(&format!("generated_{seed}.ostrin"));
        fs::write(&path, &source).unwrap();

        let interpreted = run(&["--run", &path]);
        assert!(interpreted.status.success(), "interpreter failed for seed {seed}: {}\n{source}", stderr(&interpreted));

        let exe = temp_artifact(&format!("generated_{seed}.exe"));
        let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
        if skip_if_no_c_compiler(&compile) {
            let _ = fs::remove_file(&path);
            return;
        }
        assert!(compile.status.success(), "native compile failed for seed {seed}: {}\n{source}", stderr(&compile));
        let native = Command::new(&exe).output().expect("failed to run generated binary");
        let _ = fs::remove_file(&exe);
        assert!(native.status.success(), "native run failed for seed {seed}: {}\n{source}", String::from_utf8_lossy(&native.stderr));
        assert_eq!(
            String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"),
            stdout(&interpreted).replace("\r\n", "\n"),
            "interpreter and native output differ for seed {seed}\n{source}"
        );
        assert!(
            String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
            "native binary leaked for seed {seed}: {}\n{source}",
            String::from_utf8_lossy(&native.stderr)
        );
        let _ = fs::remove_file(&path);
    }
}

/// Generates wrapper-heavy programs independently from the scalar generator above.  Keeping the
/// values behind functions and passing them through parameters exercises the ownership boundary
/// that a hand-written example can easily miss: the wrapper itself is consumed while its selected
/// `String` payload must survive the call and the final `print`.
struct ManagedWrapperGen {
    state: u64,
}

impl ManagedWrapperGen {
    fn new(seed: u64) -> Self {
        Self { state: seed.wrapping_mul(2862933555777941757).wrapping_add(3037000493) }
    }

    fn next(&mut self, bound: u64) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.state >> 33) % bound.max(1)
    }

    fn program(&mut self, seed: u64) -> String {
        let cases = 3 + self.next(3);
        let mut source = String::new();
        let mut option_flags = Vec::new();
        let mut result_flags = Vec::new();

        for case in 0..cases {
            let option_some = self.next(2) == 0;
            let result_ok = self.next(2) == 0;
            option_flags.push(option_some);
            result_flags.push(result_ok);
            let option_literal = format!("option-{seed}-{case}");
            let result_literal = format!("result-{seed}-{case}");
            source.push_str(&format!(
                "fn option_{case}() -> Option<String> {{\n\
    if {option_some} {{\n\
        Some(\"{option_literal}\")\n\
    }} else {{\n\
        None\n\
    }}\n\
}}\n\n\
fn result_{case}() -> Result<String, String> {{\n\
    if {result_ok} {{\n\
        Ok(\"{result_literal}\")\n\
    }} else {{\n\
        Err(\"error-{seed}-{case}\")\n\
    }}\n\
}}\n\n\
fn consume_option_{case}(value: Option<String>) -> String {{\n\
    value.unwrap_or(\"option parameter fallback\")\n\
}}\n\n\
fn consume_result_{case}(value: Result<String, String>) -> String {{\n\
    value.unwrap_or(\"result parameter fallback\")\n\
}}\n\n"
            ));
        }

        source.push_str("fn main() -> Void {\n");
        for case in 0..cases {
            source.push_str(&format!("    print(option_{case}().unwrap_or(\"option fallback\"))\n"));
            if option_flags[case as usize] {
                source.push_str(&format!("    print(option_{case}().unwrap())\n"));
            } else {
                source.push_str(&format!("    print(option_{case}().unwrap_or(\"option safe fallback\"))\n"));
            }
            source.push_str(&format!(
                "    match option_{case}().ok_or(\"option error\") {{\n\
        Ok(value) => print(value),\n\
        Err(error) => print(error),\n\
    }}\n"
            ));
            source.push_str(&format!("    print(result_{case}().unwrap_or(\"result fallback\"))\n"));
            if result_flags[case as usize] {
                source.push_str(&format!("    print(result_{case}().unwrap())\n"));
            } else {
                source.push_str(&format!("    print(result_{case}().unwrap_or(\"result safe fallback\"))\n"));
            }
            source.push_str(&format!("    print(result_{case}().ok().is_some())\n"));
            source.push_str(&format!("    print(consume_option_{case}(option_{case}()))\n"));
            source.push_str(&format!("    print(consume_result_{case}(result_{case}()))\n"));
        }
        source.push_str("}\n");
        source
    }
}

#[test]
fn generated_managed_wrappers_agree_between_interpreter_and_native_backend() {
    // `OSTRIN_FUZZ_SEEDS=50 cargo test generated_managed_wrappers` widens the search.
    let seed_count: u64 = std::env::var("OSTRIN_FUZZ_SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
    for seed in 1..=seed_count {
        let mut generator = ManagedWrapperGen::new(seed);
        let source = generator.program(seed);
        let path = temp_artifact(&format!("generated_managed_{seed}.ostrin"));
        fs::write(&path, &source).unwrap();

        let interpreted = run(&["--run", &path]);
        assert!(interpreted.status.success(), "interpreter failed for managed seed {seed}: {}\n{source}", stderr(&interpreted));

        let exe = temp_artifact(&format!("generated_managed_{seed}.exe"));
        let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
        if skip_if_no_c_compiler(&compile) {
            let _ = fs::remove_file(&path);
            return;
        }
        assert!(compile.status.success(), "native compile failed for managed seed {seed}: {}\n{source}", stderr(&compile));
        let native = Command::new(&exe).output().expect("failed to run generated managed binary");
        let _ = fs::remove_file(&exe);
        let _ = fs::remove_file(&path);
        assert!(native.status.success(), "native run failed for managed seed {seed}: {}\n{source}", String::from_utf8_lossy(&native.stderr));
        assert_eq!(
            String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"),
            stdout(&interpreted).replace("\r\n", "\n"),
            "interpreter and native output differ for managed seed {seed}\n{source}"
        );
        assert!(
            String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
            "native managed wrapper program leaked for seed {seed}: {}\n{source}",
            String::from_utf8_lossy(&native.stderr)
        );
    }
}

/// Mutates real programs (deleting, duplicating, truncating and swapping spans) and checks that the
/// front end reports diagnostics instead of crashing. A Rust panic exits with code 101 and prints
/// "panicked at"; a stack overflow kills the process without an exit code.
#[test]
fn front_end_never_panics_on_mutated_sources() {
    let mut state: u64 = 0x9E3779B97F4A7C15;
    let mut next = |bound: usize| -> usize {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((state >> 33) as usize) % bound.max(1)
    };
    let mut files: Vec<_> = fs::read_dir(example_path(""))
        .unwrap()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "ostrin"))
        .collect();
    files.sort();
    let path = temp_artifact("mutated.ostrin");
    let mut checked = 0;
    // `OSTRIN_FUZZ_SEEDS=N` mutates every example N times instead of every third one 3 times.
    let widened: Option<usize> = std::env::var("OSTRIN_FUZZ_SEEDS").ok().and_then(|v| v.parse().ok());
    let rounds = widened.unwrap_or(3);
    for file in files.iter().step_by(if widened.is_some() { 1 } else { 3 }) {
        let source: Vec<char> = fs::read_to_string(file).unwrap().chars().collect();
        if source.len() < 20 {
            continue;
        }
        for round in 0..rounds {
            let mut mutated = source.clone();
            let start = next(mutated.len() - 1);
            let span = 1 + next(40.min(mutated.len() - start));
            match (round + next(3)) % 4 {
                0 => {
                    mutated.drain(start..start + span);
                }
                1 => {
                    let copy: Vec<char> = mutated[start..start + span].to_vec();
                    let at = next(mutated.len());
                    for (offset, ch) in copy.into_iter().enumerate() {
                        mutated.insert(at + offset, ch);
                    }
                }
                2 => mutated.truncate(start),
                _ => {
                    let other = next(mutated.len() - span);
                    for offset in 0..span {
                        mutated.swap(start + offset, other + offset);
                    }
                }
            }
            let text: String = mutated.into_iter().collect();
            fs::write(&path, &text).unwrap();
            let output = run(&["--check", &path]);
            let code = output.status.code();
            assert!(
                matches!(code, Some(0) | Some(1)) && !stderr(&output).contains("panicked at"),
                "front end crashed (exit {code:?}) on a mutation of {}:\n{}\n--- stderr ---\n{}",
                file.display(),
                text,
                stderr(&output)
            );
            checked += 1;
        }
    }
    let _ = fs::remove_file(&path);
    assert!(checked > 60, "expected to check many mutations, checked {checked}");
}

#[test]
fn deeply_nested_input_does_not_crash_the_front_end() {
    let path = temp_artifact("deep_nesting.ostrin");
    for (label, source) in [
        ("parens", format!("fn main() -> Void {{\n    print({}1{})\n}}\n", "(".repeat(3000), ")".repeat(3000))),
        ("blocks", format!("fn main() -> Void {{\n{}{}}}\n", "    if true {\n".repeat(1500), "    }\n".repeat(1500))),
        ("unclosed", format!("fn main() -> Void {{\n    print({}\n", "[".repeat(3000))),
    ] {
        fs::write(&path, source).unwrap();
        let output = run(&["--check", &path]);
        let code = output.status.code();
        assert!(
            matches!(code, Some(0) | Some(1)) && !stderr(&output).contains("panicked at"),
            "nesting case '{label}' crashed the front end (exit {code:?}): {}",
            stderr(&output)
        );
    }
    let _ = fs::remove_file(&path);
}

#[test]
fn std_library_modules_agree_between_backends_and_pass_their_own_tests() {
    let tests = run(&["--test", &example_path("std_tests.ostrin")]);
    assert!(tests.status.success(), "std tests failed: {}{}", stdout(&tests), stderr(&tests));
    assert!(stdout(&tests).contains("10 passed"), "unexpected std test output: {}", stdout(&tests));

    let path = example_path("std_library.ostrin");
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = "3\n2.5\n10\n6\n12\n1024\ntrue\n1\n[3, 2, 1]\n[1, 2, 3, 4, 5]\n[apple, fig, pear]\n[1, 2]\n[1, 2, 3]\n[2, 3, 4, 5]\nSome(9)\nSome(2)\nababab\n007\n2\nOstrin\n[a, b, c]\n[a, b]\ntrue\n2 + 3 = 5\ns\nstr\n115\n115\n2024-02-29\n1\n29\n{\"ok\":true,\"items\":[1,2]}\n[ok, items]\n2\nfalse\ntrue\nfalse\n3\n0\n2\n2\n3.14\n-0.125\n2\ntrue\n";
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let emitted = run(&["--emit-c", &path]);
    assert!(emitted.status.success(), "std library C emission failed: {}", stderr(&emitted));
    assert!(stdout(&emitted).contains("ostrin_float_format"), "float formatter did not reach native C: {}", stdout(&emitted));

    let exe = temp_artifact("std_library.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run std binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "std library native ownership leaked: {}",
        String::from_utf8_lossy(&native.stderr)
    );

    let missing = temp_artifact("std_missing.ostrin");
    fs::write(&missing, "import std.nope\nfn main() -> Void {\n    print(1)\n}\n").unwrap();
    let output = run(&[&missing]);
    assert!(!output.status.success());
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        text.contains("no module 'nope'") && text.contains("math, lists, strings, time, json, args, env, maps"),
        "unhelpful message: {text}"
    );
    let _ = fs::remove_file(&missing);
}

#[test]
fn json_standard_library_matches_between_backends_and_is_leak_free() {
    let path = example_path("json_library.ostrin");
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = "{\"name\":\"Ostrin\",\"items\":[true,null,3.5],\"unicode\":\"😀\"}\nOstrin\n[name, items, unicode]\n[1,false]\n";
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let exe = temp_artifact("json_library.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native JSON compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run JSON binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native JSON run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
        "native JSON ownership leaked: {}",
        String::from_utf8_lossy(&native.stderr)
    );
}

#[test]
fn std_time_native_example_is_leak_free() {
    let path = example_path("time_library.ostrin");
    let exe = temp_artifact("time_library_leak.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run time binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "true\nfalse\ntrue\n60\n1\n2024-02-29\n29\n2\n");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "native leak report: {}", String::from_utf8_lossy(&native.stderr));
}

#[test]
fn functions_ending_in_return_type_check_and_run_natively() {
    let source = "fn f() -> Int {\n    return 1\n}\nfn g(x: Int) -> Int {\n    y = x + 1\n    return y + 2\n}\nfn h(c: Bool) -> Int {\n    if c {\n        return 1\n    } else {\n        return 2\n    }\n}\nfn main() -> Void {\n    print(f())\n    print(g(1))\n    print(h(true))\n    print(h(false))\n}\n";
    let path = temp_artifact("ends_in_return.ostrin");
    fs::write(&path, source).unwrap();
    let interpreted = run(&["--run", &path]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), "1\n4\n1\n2\n");
    let exe = temp_artifact("ends_in_return.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run binary");
    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(&path);
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "1\n4\n1\n2\n");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"));
}

#[test]
fn native_allocator_scales_linearly_with_live_allocations() {
    // The runtime used to keep live allocations in a linked list scanned on every retain/release,
    // making 60 000 live strings take ~15 s; a hash table makes it a few milliseconds.
    let source = "fn build(n: Int) -> Int {\n    mut words: List<String> = []\n    mut i = 0\n    while i < n {\n        words.push(\"w\" + \"x\")\n        i = i + 1\n    }\n    words.length()\n}\n\nfn main() -> Void {\n    print(build(60000))\n}\n";
    let path = temp_artifact("allocator_scale.ostrin");
    fs::write(&path, source).unwrap();
    let exe = temp_artifact("allocator_scale.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &path]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let started = std::time::Instant::now();
    let native = Command::new(&exe).output().expect("failed to run binary");
    let elapsed = started.elapsed();
    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(&path);
    assert!(native.status.success());
    assert_eq!(String::from_utf8_lossy(&native.stdout).trim(), "60000");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"));
    assert!(elapsed.as_secs_f64() < 5.0, "allocator is superlinear again: {elapsed:?}");
}

#[test]
fn new_scaffolds_a_project_that_runs_and_passes_its_own_test() {
    let root = temp_artifact("scaffold_project");
    let _ = fs::remove_dir_all(&root);
    let created = run(&["--new", &root]);
    assert!(created.status.success(), "--new failed: {}", stderr(&created));
    for file in ["ostrin.toml", "main.ostrin", ".gitignore"] {
        assert!(std::path::Path::new(&root).join(file).exists(), "missing {file}");
    }

    let output = run(&["--project", &root, "--run"]);
    assert!(output.status.success(), "project run failed: {}", stderr(&output));
    assert_eq!(stdout(&output).replace("\r\n", "\n"), "Hello, Ostrin!\n3\n");

    let entry = format!("{root}/main.ostrin");
    let tests = run(&["--test", &entry]);
    assert!(tests.status.success(), "project test failed: {}{}", stdout(&tests), stderr(&tests));
    assert!(stdout(&tests).contains("1 passed"), "unexpected test output: {}", stdout(&tests));

    let again = run(&["--new", &root]);
    assert!(!again.status.success(), "--new must not overwrite an existing project");
    let _ = fs::remove_dir_all(&root);

    let invalid = run(&["--new", &format!("{root}/bad name!")]);
    assert!(!invalid.status.success());
}

#[test]
fn native_ir_emitter_handles_scalar_maps_and_sets() {
    let file = example_path("native_ir_maps_sets.ostrin");
    let expected = "3\ntrue\nfalse\n3\n3\n1\n3\n3\n2\ntrue\nfalse\n1\n2\n2\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 3, "map/set example did not use the IR emitter: {}", stdout(&report));

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "map/set IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    for helper in [
        "Map_String_Int_new",
        "Map_String_Int_set",
        "Map_String_Int_contains_key",
        "Map_String_Int_keys",
        "Map_String_Int_values",
        "Set_String_new",
        "Set_String_add",
        "Set_String_remove",
        "Set_String_contains",
        "ostrin_release((void*)__ir_v",
    ] {
        assert!(source.contains(helper), "expected IR map/set helper '{helper}' in generated C: {source}");
    }

    let exe = temp_artifact("native_ir_maps_sets.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run map/set IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "map/set IR ownership leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_scalar_map_options() {
    let file = example_path("native_ir_map_options.ostrin");
    let expected = "3\n-1\ntrue\ntrue\n3\n5\n2\n5\n1\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 3, "map Option example did not use the IR emitter: {}", stdout(&report));

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "map Option IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    for helper in [
        "typedef struct { bool has; int64_t value; } Option_Int;",
        "Map_String_Int_get",
        "Map_String_Int_remove",
        ".has",
        "ostrin: unwrap on None",
    ] {
        assert!(source.contains(helper), "expected scalar Option support '{helper}' in generated C: {source}");
    }

    let exe = temp_artifact("native_ir_map_options.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run map Option IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "map Option IR ownership leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_managed_options_and_patterns() {
    let file = example_path("native_ir_managed_options.ostrin");
    let expected = "native\nfalse\nempty\nalpha\ntrue\nbeta\n1\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 3, "managed Option example did not use the IR emitter: {}", stdout(&report));

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "managed Option IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    for helper in [
        "typedef struct { bool has; const char* value; } Option_String;",
        "Map_String_String_get",
        "Map_String_String_remove",
        "ostrin_retain((void*)__ostrin_option.value)",
        ".has",
    ] {
        assert!(source.contains(helper), "expected managed Option support '{helper}' in generated C: {source}");
    }

    let exe = temp_artifact("native_ir_managed_options.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run managed Option IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "managed Option IR ownership leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_managed_option_result_consumers_preserve_ownership() {
    let file = example_path("native_ir_managed_consumers.ostrin");
    let expected = "native option\nnative option\noption fallback\nnative option\noption error\nnative result\nnative result\nresult fallback\ntrue\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 10, "managed Option/Result consumers did not migrate to IR: {report_text}");

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "managed consumer IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    for helper in [
        "ostrin: unwrap on None",
        "ostrin: unwrap on Err",
        "__ostrin_unwrapped",
        "__ostrin_result.ok = true",
        "__ostrin_result.ok = false",
    ] {
        assert!(source.contains(helper), "expected managed consumer support '{helper}' in generated C: {source}");
    }

    let exe = temp_artifact("native_ir_managed_consumers.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "managed consumer compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run managed consumer binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "managed consumer binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "managed Option/Result consumers leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_records_and_option_record_ownership() {
    let file = example_path("native_ir_records.ostrin");
    let expected = "3\nseven\nempty\nthree\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 5, "record example did not use the IR emitter: {report_text}");

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "record IR emission failed: {}", stderr(&emitted));
    let source = stdout(&emitted);
    for marker in [
        "typedef struct { bool has; Point* value; } Option_Point;",
        "ostrin_calloc_with_drop(1, sizeof(Point)",
        "ostrin_calloc_with_drop(1, sizeof(Box)",
        "ostrin_retain((void*)__ir_v",
        "ostrin_release((void*)__ir_v",
    ] {
        assert!(source.contains(marker), "expected record IR marker '{marker}' in generated C: {source}");
    }

    let exe = temp_artifact("native_ir_records.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run record IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "record IR ownership leaked: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_preserves_checked_fixed_width_arithmetic() {
    let file = example_path("native_ir_sized.ostrin");
    let expected = "120\n-4\n-7\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 4, "fixed-width scalar functions did not use IR: {}", stdout(&report));

    let exe = temp_artifact("native_ir_sized.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run fixed-width IR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_emitter_handles_cfg_control_flow() {
    let file = example_path("native_ir_control_flow.ostrin");
    let expected = "-1\n0\n1\n15\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let ir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 3, "branch/loop example did not use the IR emitter: {}", stdout(&report));

    let emitted = run(&["--emit-c", &file]);
    assert!(emitted.status.success(), "IR CFG emission failed: {}", stderr(&emitted));
    assert!(stdout(&emitted).contains("__ostrin_ir_pred"), "expected phi predecessor tracking");
    assert!(stdout(&emitted).contains("__ostrin_ir_bb3:"), "expected branch merge label");

    let exe = temp_artifact("native_ir_control_flow.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run IR CFG binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native run failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_hir_handles_collections_core() {
    // Collection literals, indexing, iteration and the non-closure methods
    // are now emitted directly from HIR; closure-family coverage is exercised
    // by the dedicated IR closure regressions below.
    let file = example_path("native_hir_collections.ostrin");
    let expected = "3\n2\n1\n9\n10\ntrue\n2\n2\ntrue\n2\n1\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let generated: usize = stdout(&report)
        .lines()
        .filter_map(|line| {
            line.strip_prefix("hir-generated: ")
                .or_else(|| line.strip_prefix("ir-generated: "))
                .and_then(|n| n.trim().parse::<usize>().ok())
        })
        .sum();
    assert!(generated >= 1, "collections example did not use the HIR/IR backends");

    let exe = temp_artifact("native_hir_collections.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run collections binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "collections binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_ir_handles_captured_closures_core() {
    // Captured closures, including a nested closure with a transitive
    // environment, a named function used as a value and the list combinators
    // all cross the IR boundary. Unsupported shapes keep the established
    // fallback.
    let file = example_path("native_hir_closures.ostrin");
    let expected = "15\nvalue!\n5\n23\nnested \n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let hir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 3, "closure example generated only {ir_functions} IR functions: {report_text}");
    assert_eq!(hir_functions, 0, "captured closure example fell back to HIR: {report_text}");

    let ir = run(&["--ir", &file]);
    assert!(ir.status.success(), "closure IR dump failed: {}", stderr(&ir));
    let ir_text = stdout(&ir);
    assert!(ir_text.contains("ir opaque: 0"), "nested closure lowering left an opaque IR node: {ir_text}");
    assert!(ir_text.contains("ir violations: 0"), "nested closure lowering violated IR invariants: {ir_text}");

    let exe = temp_artifact("native_hir_closures.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run closure binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "closure binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "captured closure leaked: {}", String::from_utf8_lossy(&native.stderr));
}

#[test]
fn native_ir_handles_named_function_values_and_indirect_calls() {
    // A named function used as a local value can now cross the IR boundary:
    // direct calls through the value and passing it to another function both
    // use the same closure ABI; the captured-closure regression covers
    // environment-backed lambdas separately.
    let file = example_path("native_ir_function_values.ostrin");
    let expected = "5\n10\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    let hir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(ir_functions >= 3, "named function value example generated only {ir_functions} IR functions");
    assert_eq!(hir_functions, 0, "named function values should not force a HIR fallback: {report_text}");

    let exe = temp_artifact("native_ir_function_values.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run function-value binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", stderr(&native));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(stderr(&native).contains("live_allocations=0"), "function values leaked: {}", stderr(&native));
}

#[test]
fn native_ir_handles_concrete_generic_instances() {
    // The generic declaration is lowered once, then specialized into concrete
    // IR for each call: scalar identity, List indexing, recursive calls and
    // Option methods share the same native representation as non-generic code.
    let file = example_path("native_hir_generics.ostrin");
    let expected = "4\nostrin\n42\n27\nloop\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let hir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert_eq!(hir_functions, 0, "generic instances unexpectedly fell back to HIR: {report_text}");
    assert!(ir_functions >= 6, "generic IR example generated only {ir_functions} functions: {report_text}");

    let exe = temp_artifact("native_hir_generics.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run generic HIR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "generic HIR binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "generic IR binary leaked: {}", String::from_utf8_lossy(&native.stderr));
}

#[test]
fn native_hir_handles_generic_records_and_enums() {
    // Applied record/enum types keep their HIR spelling (`Pair<Int, String>` /
    // `Maybe<Int>`) while the native backend resolves them to concrete C
    // instances. This specifically exercises generic fields, enum patterns,
    // constructors and methods inside specialized generic bodies.
    let file = example_path("native_generic_types.ostrin");
    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let hir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(hir_functions >= 8, "generic record/enum example generated only {hir_functions} HIR functions");

    let c = run(&["--emit-c", &file]);
    assert!(c.status.success(), "C emission failed: {}", stderr(&c));
    assert!(stdout(&c).contains("Pair__String_Int* __ir_v"), "generic record literal did not reach the IR C emitter");

    let exe = temp_artifact("native_generic_types.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run generic record/enum binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "generic record/enum binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(
        String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"),
        "one\n1\n1\ntrue\nhi\n3\n4\nb\n99\n1\n2\n1.5\n"
    );
}

#[test]
fn native_ir_handles_generic_methods_with_ir_for_generic_records() {
    // Generic method instances now lower through the IR pending queue,
    // including concrete generic-record returns.
    let file = example_path("native_generic_methods.ostrin");
    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let report_text = stdout(&report);
    let hir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    let ir_functions = report_text
        .lines()
        .find_map(|line| line.strip_prefix("ir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert_eq!(hir_functions, 0, "generic method unexpectedly fell back to HIR: {report_text}");
    assert!(ir_functions >= 5, "generic method example generated only {ir_functions} IR functions: {report_text}");

    let c = run(&["--emit-c", &file]);
    assert!(c.status.success(), "C emission failed: {}", stderr(&c));
    assert!(stdout(&c).contains("Box__String* __ir_v"), "generic method record return did not come from IR");

    let expected = run(&["--run", &file]);
    assert!(expected.status.success(), "interpreter failed: {}", stderr(&expected));
    let exe = temp_artifact("native_generic_methods_ir.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run generic method binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "generic method binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), stdout(&expected).replace("\r\n", "\n"));
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "generic method binary leaked: {}", String::from_utf8_lossy(&native.stderr));
}

#[test]
fn int_division_truncates_in_both_backends() {
    // The type checker types `Int / Int` as `Int`; the interpreter used to
    // return a Float (7 / 2 -> 3.5), contradicting it.
    let interpreted = run(&["--run", &example_path("int_division.ostrin")]);
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), "3\n3\n");

    let exe = temp_artifact("int_division.exe");
    let compile = run(&["--compile", "--out", &exe, &example_path("int_division.ostrin")]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    let native = Command::new(&exe).output().unwrap();
    let _ = fs::remove_file(&exe);
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "3\n3\n");
}

#[test]
fn native_backend_quantities_match_the_interpreter() {
    // Dimension is static, the unit is a runtime string (as in the
    // interpreter): mixed-unit addition, dimensionless division returning a
    // Float, a generic `<D: Dimension>` function, compound units built at
    // runtime (`m/s`, `kg*m/s*m/s`), comparisons across units, `as`,
    // `within`, `approximately`, unary minus and scalar/Quantity math, plus
    // `shapes.ostrin`: an `impl` on an enum, a list of enums and `to_string()`.
    for file in ["physics.ostrin", "native_units.ostrin", "shapes.ostrin"] {
        let interpreted = run(&["--run", &example_path(file)]);
        assert!(interpreted.status.success(), "interpreter failed on {file}: {}", stderr(&interpreted));
        let expected = stdout(&interpreted).replace("\r\n", "\n");

        let exe = temp_artifact(&format!("{file}.exe"));
        let compile = run(&["--compile", "--out", &exe, &example_path(file)]);
        if skip_if_no_c_compiler(&compile) {
            return;
        }
        assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
        let native = Command::new(&exe).output().unwrap();
        let _ = fs::remove_file(&exe);
        assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected, "output mismatch for {file}");
    }
}

#[test]
fn native_backend_monomorphizes_one_list_struct_per_element_type() {
    let out = run(&["--emit-c", &example_path("native_lists.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let source = stdout(&out);
    for struct_name in ["List_Int", "List_Point", "List_String"] {
        assert_eq!(
            source.matches(&format!("struct {struct_name} {{")).count(),
            1,
            "expected exactly one '{struct_name}' definition: {source}"
        );
    }
}

#[test]
fn cli_exposes_help_and_version() {
    let version = run(&["--version"]);
    assert!(version.status.success());
    assert!(stdout(&version).contains("ostrinc 0.1.0"));

    let help = run(&["--help"]);
    assert!(help.status.success());
    assert!(stdout(&help).contains("--json"));
    assert!(stdout(&help).contains("--check"));
    assert!(stdout(&help).contains("--symbols"));
    assert!(stdout(&help).contains("--members"));
    assert!(stdout(&help).contains("--types"));
    assert!(stdout(&help).contains("--project"));
    assert!(stdout(&help).contains("--native-threads"));
    assert!(stdout(&help).contains("wasm32-wasi"));
}

#[test]
fn compiler_exports_symbols_and_signatures_for_editor_tools() {
    let out = run(&["--symbols", "--json", &example_path("advanced.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("\"kind\":\"function\""));
    assert!(text.contains("\"name\":\"double\""));
    assert!(text.contains("fn double<D: Dimension>(x: Quantity<D>) -> Quantity<D>"));
    assert!(text.contains("\"line\":1"));
}

#[test]
fn compiler_exports_type_members_and_local_bindings_for_editor_tools() {
    let out = run(&["--members", "--json", &example_path("collections.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("\"memberKind\":\"method\",\"owner\":\"List\",\"name\":\"push\""));
    assert!(text.contains("\"owner\":\"Map\",\"name\":\"get\""));
    assert!(text.contains("\"kind\":\"binding\",\"name\":\"numbers\",\"type\":\"List<Int>\",\"function\":\"main\",\"scopeDepth\":1"));
    assert!(text.contains("\"name\":\"doubled\",\"type\":\"List<Int>\""));
    assert!(text.contains("\"name\":\"evens\",\"type\":\"List<Int>\""));
    assert!(text.contains("\"name\":\"total\",\"type\":\"Int\""));
    assert!(text.contains("\"name\":\"found\",\"type\":\"Option<Int>\""));

    let advanced = run(&["--members", "--json", &example_path("advanced.ostrin")]);
    assert!(advanced.status.success(), "stderr: {}", stderr(&advanced));
    let advanced_text = stdout(&advanced);
    assert!(advanced_text.contains("\"name\":\"x\",\"type\":\"Quantity<D>\",\"function\":\"double\",\"scopeDepth\":0"));

    let generics = run(&["--members", "--json", &example_path("generics.ostrin")]);
    assert!(generics.status.success(), "stderr: {}", stderr(&generics));
    let generics_text = stdout(&generics);
    assert!(generics_text.contains("\"name\":\"a\",\"type\":\"Score\",\"function\":\"main\",\"scopeDepth\":1"));
    assert!(generics_text.contains("\"owner\":\"Score\",\"name\":\"value\""));
    assert!(generics_text.contains("\"resultType\":\"Int\""));
    assert!(generics_text.contains("\"line\":5,\"column\":1"));
}

#[test]
fn compiler_exports_inferred_expression_types_for_editor_tools() {
    let out = run(&["--types", "--json", &example_path("option_result_types.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("\"kind\":\"expression\""));
    assert!(text.contains("\"type\":\"Option<Int>\""));
    assert!(text.contains("\"type\":\"Result<Int, String>\""));
    assert!(text.contains("\"line\":2,\"column\":13"));
    assert!(text.contains("\"endLine\":2,\"endColumn\":20"));
}

#[test]
fn advanced_type_checks_ok() {
    let out = run(&[&example_path("advanced.ostrin")]);
    assert!(out.status.success());
}

#[test]
fn newlines_type_checks_ok() {
    let out = run(&[&example_path("newlines.ostrin")]);
    assert!(out.status.success());
}

#[test]
fn shapes_type_checks_ok() {
    let out = run(&[&example_path("shapes.ostrin")]);
    assert!(out.status.success());
}

#[test]
fn dimensional_errors_are_all_reported() {
    let out = run(&[&example_path("errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1024"), "missing E1024 in: {err}");
    assert!(err.contains("E1025"), "missing E1025 in: {err}");
    assert!(err.contains("E1001"), "missing E1001 in: {err}");
    assert_eq!(err.matches("E1024").count(), 2, "expected E1024 to appear twice: {err}");
}

#[test]
fn physics_runs_with_correct_unit_arithmetic() {
    let out = run(&["--run", &example_path("physics.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[1], "5 m/s");
    assert_eq!(lines[2], "6 kg");
    assert_eq!(lines[3], "400000000");
    assert_eq!(lines[4], "15");
}

#[test]
fn fibonacci_iterator_protocol_works() {
    let out = run(&["--run", &example_path("fibonacci.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let expected = ["0", "1", "1", "2", "3", "5", "8", "13", "21", "34"];
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, expected);
}

#[test]
fn traits_dispatch_operators_via_impl() {
    let out = run(&["--run", &example_path("traits.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["Vector2 { x: 4, y: 6 }", "true", "false", "true", "false", "true"]);
}

#[test]
fn derive_eq_and_ord_work_without_manual_impl() {
    let out = run(&["--run", &example_path("derive.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["true", "false", "true", "true", "true"]);
}

#[test]
fn dyn_trait_dispatches_polymorphically() {
    let out = run(&["--run", &example_path("dyn_trait.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["12.56636", "9", "3.14159", "24.70795"]);
}

#[test]
fn concurrency_channel_and_task_work() {
    let out = run(&["--run", &example_path("concurrency.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["1", "2", "3", "4", "5", "7"]);
}

#[test]
fn concurrency_scheduler_defers_tasks_and_drains_scopes() {
    let out = run(&["--run", &example_path("concurrency_scheduler.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        ["main", "task", "42", "scope-body", "scope-task", "7"]
    );
}

#[test]
fn pending_tasks_can_be_cancelled_before_they_run() {
    let file = example_path("concurrency_cancel.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), "true\nfalse\n");

    let exe = temp_artifact("cancel.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "native cancellation compile failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native cancellation binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native cancellation binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "true\nfalse\n");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "cancellation leaked: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_exe = temp_artifact("cancel-native-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--out", &threaded_exe, &file]);
    assert!(threaded_compile.status.success(), "native-thread cancellation compile failed: {}", stderr(&threaded_compile));
    let _ = fs::remove_file(&threaded_exe);
}

#[test]
fn running_tasks_honor_cancellation_at_cooperative_checkpoints() {
    let file = example_path("concurrency_cancel_safe.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = "started\n1\ntrue\ncontroller-finished\nfalse\n";
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let exe = temp_artifact("cancel-safe.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "safe cancellation compile failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native safe cancellation binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native safe cancellation failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "safe cancellation leaked: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_exe = temp_artifact("cancel-safe-native-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--leak-check", "--out", &threaded_exe, &file]);
    assert!(threaded_compile.status.success(), "native-thread safe cancellation compile failed: {}", stderr(&threaded_compile));
    let threaded_native = Command::new(&threaded_exe).output().expect("run native-thread safe cancellation binary");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded_native.status.success(), "native-thread safe cancellation failed: {}", String::from_utf8_lossy(&threaded_native.stderr));
    assert!(String::from_utf8_lossy(&threaded_native.stderr).contains("live_allocations=0"), "native-thread safe cancellation leaked: {}", String::from_utf8_lossy(&threaded_native.stderr));

    let threaded = run(&["--emit-c", "--native-threads", &file]);
    assert!(threaded.status.success(), "native-thread safe cancellation emission failed: {}", stderr(&threaded));
    let source = stdout(&threaded);
    assert!(source.contains("cancel_requested"));
    assert!(source.contains("ostrin_task_checkpoint"));
}

#[test]
fn cancelling_a_parent_task_cancels_its_spawn_scope_group() {
    let file = example_path("concurrency_scope_cancel.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), "parent-started\ntrue\n");

    let exe = temp_artifact("scope-cancel.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "scope cancellation compile failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run scope cancellation binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "scope cancellation binary failed: {}", String::from_utf8_lossy(&native.stderr));
    let native_stdout = String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n");
    assert!(!native_stdout.contains("parent-must-not-run"), "parent continued after cancellation: {native_stdout}");
    assert!(!native_stdout.contains("child-must-not-run"), "child escaped its cancelled scope: {native_stdout}");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "scope cancellation leaked: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_exe = temp_artifact("scope-cancel-native-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--leak-check", "--out", &threaded_exe, &file]);
    assert!(threaded_compile.status.success(), "native-thread scope cancellation compile failed: {}", stderr(&threaded_compile));
    let threaded_native = Command::new(&threaded_exe).output().expect("run native-thread scope cancellation binary");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded_native.status.success(), "native-thread scope cancellation failed: {}", String::from_utf8_lossy(&threaded_native.stderr));
    let threaded_stdout = String::from_utf8_lossy(&threaded_native.stdout);
    assert!(!threaded_stdout.contains("parent-must-not-run"), "native parent continued after cancellation: {threaded_stdout}");
    assert!(!threaded_stdout.contains("child-must-not-run"), "native child escaped its cancelled scope: {threaded_stdout}");
    assert!(String::from_utf8_lossy(&threaded_native.stderr).contains("live_allocations=0"), "native-thread scope cancellation leaked: {}", String::from_utf8_lossy(&threaded_native.stderr));
}

#[test]
fn cancelling_a_task_wakes_a_blocked_channel_receive() {
    let file = example_path("concurrency_cancel_blocked_receive.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), "blocked-started\ntrue\n");

    let exe = temp_artifact("cancel-blocked-receive.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "blocked receive compile failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run blocked receive binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "blocked receive binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "blocked-started\ntrue\n");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "blocked receive leaked: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_exe = temp_artifact("cancel-blocked-receive-native-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--leak-check", "--out", &threaded_exe, &file]);
    assert!(threaded_compile.status.success(), "native-thread blocked receive compile failed: {}", stderr(&threaded_compile));
    let threaded_native = Command::new(&threaded_exe).output().expect("run native-thread blocked receive binary");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded_native.status.success(), "native-thread blocked receive failed: {}", String::from_utf8_lossy(&threaded_native.stderr));
    assert_eq!(String::from_utf8_lossy(&threaded_native.stdout).replace("\r\n", "\n"), "blocked-started\ntrue\n");
    assert!(String::from_utf8_lossy(&threaded_native.stderr).contains("live_allocations=0"), "native-thread blocked receive leaked: {}", String::from_utf8_lossy(&threaded_native.stderr));

    let emitted = run(&["--emit-c", "--native-threads", &file]);
    assert!(emitted.status.success(), "native-thread blocked receive emission failed: {}", stderr(&emitted));
    assert!(stdout(&emitted).contains("ostrin_cond_wait_timeout"));
    assert!(stdout(&emitted).contains("ostrin_task_checkpoint"));
}

#[test]
fn cancelled_tasks_do_not_release_dropped_child_handles_twice() {
    let file = example_path("concurrency_cancel_after_drop.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), "parent-started\ntrue\n");

    let exe = temp_artifact("cancel-after-drop.exe");
    let compiled = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "cancel-after-drop compile failed: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run cancel-after-drop binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "cancel-after-drop binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), "parent-started\ntrue\n");
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "dropped child handle leaked or was released twice: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_exe = temp_artifact("cancel-after-drop-native-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--leak-check", "--out", &threaded_exe, &file]);
    assert!(threaded_compile.status.success(), "native-thread cancel-after-drop compile failed: {}", stderr(&threaded_compile));
    let threaded_native = Command::new(&threaded_exe).output().expect("run native-thread cancel-after-drop binary");
    let _ = fs::remove_file(&threaded_exe);
    assert!(threaded_native.status.success(), "native-thread cancel-after-drop failed: {}", String::from_utf8_lossy(&threaded_native.stderr));
    assert_eq!(String::from_utf8_lossy(&threaded_native.stdout).replace("\r\n", "\n"), "parent-started\ntrue\n");
    assert!(String::from_utf8_lossy(&threaded_native.stderr).contains("live_allocations=0"), "native-thread dropped child handle leaked or was released twice: {}", String::from_utf8_lossy(&threaded_native.stderr));

    let emitted = run(&["--emit-c", "--native-threads", &file]);
    assert!(emitted.status.success(), "cancel-after-drop emission failed: {}", stderr(&emitted));
    assert!(stdout(&emitted).contains("released"));
    assert!(stdout(&emitted).contains("ostrin_release_owned"));
}

#[test]
fn yield_advances_the_cooperative_scheduler_and_compiles_with_threads() {
    let file = example_path("concurrency_yield.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(expected, "main\ntask\nafter\n");

    let exe = temp_artifact("yield.exe");
    let compile = run(&["--compile", "--leak-check", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native yield compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run native yield binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native yield binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
    assert!(String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"), "yield leaked: {}", String::from_utf8_lossy(&native.stderr));

    let threaded_exe = temp_artifact("yield-native-threads.exe");
    let threaded_compile = run(&["--compile", "--native-threads", "--out", &threaded_exe, &file]);
    assert!(threaded_compile.status.success(), "native-thread yield compile failed: {}", stderr(&threaded_compile));
    let _ = fs::remove_file(&threaded_exe);
}

#[test]
fn concurrency_select_matches_between_interpreter_and_native_modes() {
    let file = example_path("concurrency_select.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(expected, "9\n7\nNone\n");

    for (label, extra) in [("cooperative", Vec::<&str>::new()), ("native-threads", vec!["--native-threads"])] {
        let exe = temp_artifact(&format!("concurrency-select-{label}.exe"));
        let mut args = vec!["--compile", "--leak-check", "--out", exe.as_str()];
        args.extend(extra);
        args.push(&file);
        let compile = run(&args);
        if skip_if_no_c_compiler(&compile) {
            return;
        }
        assert!(compile.status.success(), "{label} native compile failed: {}", stderr(&compile));
        let native = Command::new(&exe).output().expect("run native select binary");
        let _ = fs::remove_file(&exe);
        assert!(native.status.success(), "{label} native binary failed: {}", String::from_utf8_lossy(&native.stderr));
        assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected, "{label} output differs");
        assert!(
            String::from_utf8_lossy(&native.stderr).contains("live_allocations=0"),
            "{label} select runtime leaked allocations: {}",
            String::from_utf8_lossy(&native.stderr)
        );
    }
}

#[test]
fn select_requires_a_homogeneous_channel_list() {
    let out = run_stdin(
        &["--stdin", "--check", "--file", "C:/workspace/select_error.ostrin"],
        "fn main() -> Void {\n    print(select([1, 2]))\n}\n",
    );
    assert!(!out.status.success());
    let text = stderr(&out);
    assert!(text.contains("E1041"), "missing select type diagnostic: {text}");
    assert!(text.contains("List<Channel<T>>"), "missing select contract: {text}");
}

#[test]
fn spawn_capturing_mut_is_rejected() {
    let out = run(&[&example_path("concurrency_errors.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("E1100"));
}

#[test]
fn moved_channel_value_cannot_be_reused() {
    let out = run(&["--run", &example_path("moved_after_send.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("OSTRIN-E1101"));
}

#[test]
fn collections_map_set_and_combinators_work() {
    let out = run(&["--run", &example_path("collections.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            "Some(1.5)", "false", "Some(12)", "2", "3", "true", "false", "2",
            "[2, 4, 6, 8, 10, 12]", "[2, 4, 6]", "21", "Some(4)", "true", "true"
        ]
    );
}

#[test]
fn list_mutation_is_shared_through_aliases() {
    let out = run(&["--run", &example_path("list_mutation.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["2", "[1, 3, 4]", "[1, 3, 4]", "3"]);
}

#[test]
fn list_mutation_requires_mutable_binding() {
    let source = example_path("list_mutation_error.ostrin");
    let out = run(&[&source]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1053"), "missing E1053 in: {err}");
    assert!(err.contains("immutable binding 'numbers'"), "unexpected error: {err}");
}

#[test]
fn exhaustive_enum_match_checks_and_runs() {
    let out = run(&["--run", &example_path("match_exhaustive.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["red", "yellow", "green"]);
}

#[test]
fn non_exhaustive_enum_match_is_rejected() {
    let out = run(&[&example_path("match_non_exhaustive.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1060"), "missing E1060 in: {err}");
    assert!(err.contains("Green"), "missing variant in: {err}");
}

#[test]
fn generic_functions_infer_types_and_honor_trait_bounds() {
    let out = run(&["--run", &example_path("generics.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["true", "false", "7"]);
}

#[test]
fn generic_trait_bound_is_rejected_at_call_site() {
    let out = run(&[&example_path("generics_bound_error.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing E1042: {err}");
    assert!(err.contains("Plain"), "missing concrete type in: {err}");
    assert!(err.contains("Comparable"), "missing bound in: {err}");
}

#[test]
fn nested_enum_and_record_patterns_run() {
    let out = run(&["--run", &example_path("match_nested.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["42", "0", "5"]);
}

#[test]
fn malformed_match_patterns_are_rejected() {
    let out = run(&[&example_path("match_pattern_error.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1061"), "missing E1061: {err}");
    assert!(err.contains("other"), "missing unknown field diagnostic: {err}");
    assert!(err.contains("Pattern literal has type 'Bool'"), "missing nested type diagnostic: {err}");
    assert!(err.contains("carries data"), "missing bare constructor diagnostic: {err}");
}

#[test]
fn generic_enum_and_record_patterns_are_instantiated() {
    let out = run(&["--run", &example_path("generic_nested_patterns.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        stdout(&out).lines().collect::<Vec<_>>(),
        ["9", "7", "good", "failed", "empty", "4"]
    );
}

#[test]
fn nested_generic_match_reports_missing_inner_combination() {
    let out = run(&[&example_path("generic_nested_patterns_error.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1060"), "missing E1060: {err}");
    assert!(err.contains("Just"), "missing partially-covered variant: {err}");
}

#[test]
fn trait_supertraits_and_default_methods_dispatch() {
    let out = run(&["--run", &example_path("traits_defaults.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "7");
}

#[test]
fn trait_required_methods_supertraits_and_signatures_are_checked() {
    let out = run(&[&example_path("traits_semantics_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1050"), "missing supertrait diagnostic: {err}");
    assert!(err.contains("E1054"), "missing signature diagnostic: {err}");
    assert!(err.contains("E1055"), "missing required-method diagnostic: {err}");
}

#[test]
fn trait_supertraits_are_transitive_and_collisions_are_rejected() {
    let out = run(&[&example_path("traits_coherence_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1050"), "missing transitive-supertrait diagnostic: {err}");
    assert!(err.contains("Root"), "missing transitive supertrait name: {err}");
    assert!(err.contains("E1057"), "missing inherited-method collision diagnostic: {err}");
}

#[test]
fn trait_orphan_rule_rejects_foreign_trait_for_foreign_type() {
    let out = run(&[&example_path("proj_orphan/main.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1056"), "missing orphan-rule diagnostic: {err}");
    assert!(err.contains("ForeignTrait"), "missing foreign trait name: {err}");
    assert!(err.contains("ForeignType"), "missing foreign type name: {err}");
}

#[test]
fn trait_orphan_rule_allows_local_trait_for_foreign_type() {
    let out = run(&[&example_path("proj_coherence_allowed/main.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
}

#[test]
fn explicit_generic_arguments_select_function_types() {
    let out = run(&["--run", &example_path("generics_explicit.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["7", "a", "Just(a)", "5 m", "1"]);
}

#[test]
fn explicit_generic_arguments_check_arity_types_and_bounds() {
    let out = run(&[&example_path("generics_explicit_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing explicit-generic diagnostic: {err}");
    assert!(err.contains("explicit generic argument"), "missing arity diagnostic: {err}");
    assert!(err.contains("Plain"), "missing explicit-bound type: {err}");
}

#[test]
fn generic_impls_generic_methods_and_constructors_are_preserved() {
    let out = run(&["--run", &example_path("generic_impls_and_methods.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "ok");
}

#[test]
fn generic_trait_arguments_specialize_method_signatures() {
    let out = run(&[&example_path("generic_trait_args_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1054"), "missing trait-signature diagnostic: {err}");
    assert!(err.contains("Bad.convert"), "missing implementation name: {err}");
}

#[test]
fn transitive_trait_defaults_dispatch_through_supertraits() {
    let out = run(&["--run", &example_path("traits_defaults_transitive.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "9");
}

#[test]
fn applied_generic_impls_dispatch_by_record_arguments() {
    let out = run(&["--run", &example_path("generic_impl_dispatch.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["integer", "text"]);
}

#[test]
fn applied_generic_impls_do_not_fall_back_to_another_type() {
    let out = run(&["--run", &example_path("generic_impl_dispatch_error.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing static method diagnostic: {err}");
    assert!(
        err.contains("Type 'Box<String>' has no method 'label'"),
        "missing applied type/method in diagnostic: {err}"
    );
}

#[test]
fn concrete_method_arguments_are_checked_before_runtime() {
    let out = run(&[&example_path("concrete_method_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing static argument diagnostic: {err}");
    assert!(
        err.contains("Argument for 'add' expects 'Int', got 'String'"),
        "missing method argument types in diagnostic: {err}"
    );
}

#[test]
fn trait_default_bodies_are_type_checked() {
    let out = run(&[&example_path("trait_default_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1041"), "missing default-body type diagnostic: {err}");
    assert!(
        err.contains("Broken.value") && err.contains("Int") && err.contains("String"),
        "missing default method and types in diagnostic: {err}"
    );
}

#[test]
fn impl_generic_bounds_are_honored_for_applied_methods() {
    let out = run(&[&example_path("impl_bounds_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("E1042"), "missing impl-bound method diagnostic: {err}");
    assert!(
        err.contains("Box<Plain>") && err.contains("no method 'label'"),
        "missing rejected applied type/method in diagnostic: {err}"
    );
}

#[test]
fn applied_generic_enum_impls_dispatch_by_constructor_arguments() {
    let out = run(&["--run", &example_path("generic_enum_dispatch.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["integer enum", "text enum"]);
}

#[test]
fn quantity_impls_dispatch_by_dimension_argument() {
    let out = run(&["--run", &example_path("quantity_impl_dispatch.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).lines().collect::<Vec<_>>(), ["length quantity", "time quantity"]);
}

#[test]
fn multi_file_project_resolves_qualified_alias_and_named_imports() {
    let out = run(&["--run", &example_path("proj1/main.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let text = stdout(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines, ["298.15", "273.15", "5"]);
}

#[test]
fn private_symbol_is_rejected_across_modules() {
    let out = run(&[&example_path("proj_private/main.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("E1080"));
}

#[test]
fn circular_import_is_detected() {
    let out = run(&[&example_path("proj_cycle/main.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("E1081"));
}

#[test]
fn parser_recovers_and_reports_every_syntax_error() {
    let out = run(&[&example_path("parse_errors.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert_eq!(err.matches("parse error").count(), 3, "expected exactly 3 parse errors: {err}");
}

#[test]
fn path_dependency_resolves_and_runs() {
    let out = run(&["--run", &example_path("pkg_project/main_app/main.ostrin")]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "hola, Ostrin");
}

#[test]
fn transitive_path_dependencies_resolve_and_lock_reproducibly() {
    let root = std::env::temp_dir().join(format!("ostrin_transitive_package_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let project = root.join("project");
    let shared = root.join("shared");
    let nested = root.join("nested");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&shared).unwrap();
    fs::create_dir_all(&nested).unwrap();

    fs::write(
        nested.join("ostrin.toml"),
        "[package]\nname = \"nested\"\nversion = \"0.2.0\"\nentry = \"value.ostrin\"\n",
    )
    .unwrap();
    fs::write(nested.join("value.ostrin"), "pub fn text() -> String { \"transitive\" }\n").unwrap();
    fs::write(
        shared.join("ostrin.toml"),
        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nentry = \"helpers.ostrin\"\n\n[dependencies]\nnested_utils = { path = \"../nested\" }\n",
    )
    .unwrap();
    fs::write(
        shared.join("helpers.ostrin"),
        "import nested_utils.value\n\npub fn greet() -> String { value.text() }\n",
    )
    .unwrap();
    fs::write(
        project.join("ostrin.toml"),
        "[package]\nname = \"transitive_app\"\nversion = \"0.1.0\"\nentry = \"main.ostrin\"\n\n[dependencies]\nshared = { path = \"../shared\" }\n",
    )
    .unwrap();
    fs::write(
        project.join("main.ostrin"),
        "import shared.helpers\n\nfn main() -> Void {\n    print(helpers.greet())\n}\n",
    )
    .unwrap();

    let project_text = project.display().to_string();
    let first = run(&["--run", "--project", &project_text]);
    assert!(first.status.success(), "transitive package run failed: {}", stderr(&first));
    assert_eq!(stdout(&first).trim(), "transitive");

    let lockfile = fs::read_to_string(project.join("ostrin.lock")).unwrap();
    assert!(lockfile.contains("name = \"shared\""), "lockfile omitted direct package: {lockfile}");
    assert!(lockfile.contains("name = \"nested_utils\""), "lockfile omitted transitive package: {lockfile}");
    assert!(!lockfile.contains(&root.display().to_string()), "lockfile should keep transitive paths portable: {lockfile}");
    let locked = run(&["--locked", "--run", "--project", &project_text]);
    assert!(locked.status.success(), "locked transitive package run failed: {}", stderr(&locked));
    assert_eq!(stdout(&locked).trim(), "transitive");
    assert_eq!(fs::read_to_string(project.join("ostrin.lock")).unwrap(), lockfile);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn native_backend_compiles_project_manifest_and_path_dependency() {
    let project = example_path("pkg_project/main_app");
    let exe = temp_artifact("pkg_project.exe");
    let compile = run(&["--compile", "--project", &project, "--out", &exe]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "package compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("compiled package should run");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "compiled package exited unsuccessfully: {}", stderr(&native));
    assert_eq!(String::from_utf8_lossy(&native.stdout).trim(), "hola, Ostrin");
}

#[test]
fn project_manifest_selects_entry_and_writes_portable_lockfile() {
    let project = example_path("pkg_project/main_app");
    let out = run(&["--run", "--project", &project]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "hola, Ostrin");

    let lockfile = fs::read_to_string(format!("{project}/ostrin.lock")).expect("project build should write ostrin.lock");
    assert!(lockfile.contains("lockfile_version = 1"), "lockfile should declare its schema: {lockfile}");
    assert!(lockfile.contains("source = \"path\""), "lockfile should identify path dependencies: {lockfile}");
    assert!(lockfile.contains("package_version = \"0.0.0\""), "lockfile should record the dependency version: {lockfile}");
    assert!(
        lockfile.lines().any(|line| {
            line.strip_prefix("content_sha256 = \"")
                .and_then(|value| value.strip_suffix('"'))
                .is_some_and(|hash| hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit()))
        }),
        "lockfile should record a SHA-256 content hash: {lockfile}"
    );
    assert!(lockfile.contains("resolved_path = \"../shared_lib\""), "lockfile should use a project-relative path: {lockfile}");
    assert!(!lockfile.contains("Lenguaje nuevo"), "lockfile should not embed this checkout's absolute path: {lockfile}");
}

#[test]
fn locked_path_dependency_rejects_content_tampering() {
    let root = std::env::temp_dir().join(format!("ostrin_package_integrity_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dependency = root.join("shared");
    let project = root.join("project");
    fs::create_dir_all(&dependency).unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        dependency.join("ostrin.toml"),
        "[package]\nname = \"shared\"\nversion = \"0.1.0\"\nentry = \"helpers.ostrin\"\n",
    )
    .unwrap();
    fs::write(dependency.join("helpers.ostrin"), "pub fn greet() -> String { \"before\" }\n").unwrap();
    fs::write(
        project.join("ostrin.toml"),
        "[package]\nname = \"integrity_app\"\nversion = \"0.1.0\"\nentry = \"main.ostrin\"\n\n[dependencies]\nshared = { path = \"../shared\" }\n",
    )
    .unwrap();
    fs::write(
        project.join("main.ostrin"),
        "import shared.helpers\n\nfn main() -> Void {\n    print(helpers.greet())\n}\n",
    )
    .unwrap();

    let project_text = project.display().to_string();
    let first = run(&["--run", "--project", &project_text]);
    assert!(first.status.success(), "initial package run failed: {}", stderr(&first));
    let locked = run(&["--locked", "--run", "--project", &project_text]);
    assert!(locked.status.success(), "locked package run failed: {}", stderr(&locked));
    fs::write(dependency.join("helpers.ostrin"), "pub fn greet() -> String { \"after\" }\n").unwrap();

    let tampered = run(&["--locked", "--run", "--project", &project_text]);
    assert!(!tampered.status.success(), "locked package run should reject source tampering");
    let error = stderr(&tampered);
    assert!(error.contains("content hash changed"), "unexpected integrity error: {error}");
    assert!(error.contains("shared"), "integrity error should name the dependency: {error}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn git_dependency_fails_clearly_without_network_access() {
    let out = run(&[&example_path("pkg_git_test/main.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("does not fetch git dependencies automatically"), "unexpected message: {err}");
}

#[test]
fn git_dependency_can_be_fetched_only_with_explicit_flag() {
    if Command::new("git").arg("--version").output().is_err() {
        eprintln!("skipping explicit Git package test: git is not installed");
        return;
    }

    let root = std::env::temp_dir().join(format!("ostrin_git_package_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dependency = root.join("dependency");
    let project = root.join("project");
    fs::create_dir_all(&dependency).unwrap();
    fs::create_dir_all(&project).unwrap();
    fs::write(
        dependency.join("helpers.ostrin"),
        "pub fn greet() -> String { \"git package\" }\n",
    )
    .unwrap();
    let dependency_text = dependency.display().to_string().replace('\\', "/");
    let init = Command::new("git").args(["-C", &dependency_text, "init"]).output().unwrap();
    assert!(init.status.success(), "git init failed: {}", String::from_utf8_lossy(&init.stderr));
    let add = Command::new("git").args(["-C", &dependency_text, "add", "."]).output().unwrap();
    assert!(add.status.success(), "git add failed: {}", String::from_utf8_lossy(&add.stderr));
    let commit = Command::new("git")
        .args(["-C", &dependency_text, "-c", "user.name=Ostrin Tests", "-c", "user.email=ostrinc-tests@example.invalid", "commit", "-m", "initial"])
        .output()
        .unwrap();
    assert!(commit.status.success(), "git commit failed: {}", String::from_utf8_lossy(&commit.stderr));

    let git_url = format!("file:///{dependency_text}");
    fs::write(
        project.join("ostrin.toml"),
        format!(
            "[package]\nname = \"git_app\"\nversion = \"0.1.0\"\nentry = \"main.ostrin\"\n\n[dependencies]\nshared = {{ git = \"{git_url}\", rev = \"HEAD\" }}\n"
        ),
    )
    .unwrap();
    fs::write(
        project.join("main.ostrin"),
        "import shared.helpers\n\nfn main() -> Void {\n    print(helpers.greet())\n}\n",
    )
    .unwrap();

    let project_text = project.display().to_string();
    let out = run(&["--fetch", "--run", "--project", &project_text]);
    assert!(out.status.success(), "explicit Git fetch failed: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "git package");
    let lockfile = fs::read_to_string(project.join("ostrin.lock")).unwrap();
    assert!(lockfile.contains("source = \"git\""), "lockfile lost Git source: {lockfile}");
    assert!(lockfile.contains("requested = \"HEAD\""), "lockfile lost requested revision: {lockfile}");
    assert!(lockfile.lines().any(|line| line.starts_with("resolved_rev = \"") && line.len() >= 56), "lockfile did not record a resolved commit: {lockfile}");
    assert!(lockfile.contains("resolved_path = \".ostrin/packages/"), "lockfile did not use the project cache: {lockfile}");
    let resolved_path = lockfile
        .lines()
        .find_map(|line| line.strip_prefix("resolved_path = \"").and_then(|value| value.strip_suffix('"')))
        .expect("Git lockfile should contain a resolved path");
    let cache_path = project.join(resolved_path);
    let cache_path_text = cache_path.display().to_string();
    let remove_remote = Command::new("git")
        .args(["-C", &cache_path_text, "remote", "remove", "origin"])
        .output()
        .unwrap();
    assert!(remove_remote.status.success(), "could not make the cache offline: {}", String::from_utf8_lossy(&remove_remote.stderr));
    let locked = run(&["--locked", "--run", "--project", &project_text]);
    assert!(locked.status.success(), "locked build should use the cached commit: {}", stderr(&locked));
    assert_eq!(stdout(&locked).trim(), "git package");
    assert_eq!(fs::read_to_string(project.join("ostrin.lock")).unwrap(), lockfile, "--locked must not rewrite the lockfile");
    let normal = run(&["--run", "--project", &project_text]);
    assert!(normal.status.success(), "normal build should reuse a valid lockfile without network: {}", stderr(&normal));
    let _ = fs::remove_dir_all(&cache_path);
    let missing = run(&["--locked", "--run", "--project", &project_text]);
    assert!(!missing.status.success(), "--locked should reject a missing cached checkout");
    assert!(stderr(&missing).contains("checkout") && stderr(&missing).contains("missing"), "unexpected missing-cache error: {}", stderr(&missing));
    let restored = run(&["--fetch", "--run", "--project", &project_text]);
    assert!(restored.status.success(), "--fetch should restore a missing locked checkout: {}", stderr(&restored));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn native_backend_generic_records_and_enums_match_the_interpreter() {
    // Generic records/enums are monomorphized per concrete instantiation:
    // field access, generic + specialized `impl`s (including trait impls
    // for `Box<Int>` vs `Box<String>`), nested variant/record patterns,
    // explicit and inferred type arguments, and a bare `Nothing` completed
    // from the expected type.
    for file in [
        "native_generic_types.ostrin",
        "field_access.ostrin",
        "generic_impl_dispatch.ostrin",
        "generic_enum_dispatch.ostrin",
        "generic_nested_patterns.ostrin",
        "native_display.ostrin",
        "native_derive.ostrin",
        "native_named_args.ostrin",
        "native_concurrency.ostrin",
        "concurrency.ostrin",
        "concurrency_scheduler.ostrin",
        "fibonacci.ostrin",
        "native_builtins.ostrin",
        "native_generic_methods.ostrin",
        "generic_impls_and_methods.ostrin",
        "generics_explicit.ostrin",
        "quantity_impl_dispatch.ostrin",
        "option_result.ostrin",
        "option_result_types.ostrin",
        "try_result.ostrin",
        "native_trait_defaults.ostrin",
        "traits_defaults.ostrin",
        "traits_defaults_transitive.ostrin",
        "native_collections.ostrin",
        "collections.ostrin",
        "function_arguments.ostrin",
        "traits.ostrin",
    ] {
        let interpreted = run(&["--run", &example_path(file)]);
        assert!(interpreted.status.success(), "interpreter failed on {file}: {}", stderr(&interpreted));
        let expected = stdout(&interpreted).replace("\r\n", "\n");

        let exe = temp_artifact(&format!("{file}.exe"));
        let compile = run(&["--compile", "--out", &exe, &example_path(file)]);
        if skip_if_no_c_compiler(&compile) {
            return;
        }
        assert!(compile.status.success(), "compile failed for {file}: {}", stderr(&compile));
        let native = Command::new(&exe).output().unwrap();
        let _ = fs::remove_file(&exe);
        assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected, "output mismatch for {file}");
    }
}

#[test]
fn test_mode_runs_test_functions_and_reports_failures() {
    let ok = run(&["--test", &example_path("testing.ostrin")]);
    assert!(ok.status.success(), "all tests should pass: {}", stdout(&ok));
    let text = stdout(&ok);
    assert!(text.contains("test test_add ... ok") && text.contains("3 passed; 0 failed"), "unexpected output: {text}");

    let bad = run(&["--test", &example_path("testing_failure.ostrin")]);
    assert!(!bad.status.success(), "failing tests must give a non-zero exit code");
    let text = stdout(&bad);
    assert!(text.contains("test test_passes ... ok"), "unexpected output: {text}");
    assert!(text.contains("test test_fails ... FAILED (assertion failed: left = 4, right = 5)"), "unexpected output: {text}");
    assert!(text.contains("test test_condition_fails ... FAILED (assertion failed)"), "unexpected output: {text}");
    assert!(text.contains("1 passed; 2 failed"), "unexpected output: {text}");
}

#[test]
fn fixed_width_integer_overflow_is_an_error_in_both_backends() {
    // 250 + 5 fits a UInt8, 250 + 10 does not: both the interpreter and the
    // native backend must print the first result and then fail with an
    // overflow error rather than wrapping around.
    let file = example_path("sized_ints_overflow.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(!interpreted.status.success());
    assert!(stdout(&interpreted).replace("\r\n", "\n").starts_with("255\n"));
    assert!(stderr(&interpreted).contains("integer overflow"), "unexpected stderr: {}", stderr(&interpreted));

    let exe = temp_artifact("overflow.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().unwrap();
    let _ = fs::remove_file(&exe);
    assert!(!native.status.success());
    assert!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n").starts_with("255\n"));
    assert!(String::from_utf8_lossy(&native.stderr).contains("integer overflow"));
}

#[test]
fn array_shape_mismatch_is_a_runtime_error_in_both_backends() {
    // (2, 3) + (2,) can't be broadcast: both backends must print the shape,
    // then fail with the same kind of error instead of reading out of bounds.
    let file = example_path("arrays_shape_mismatch.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(!interpreted.status.success());
    assert!(stdout(&interpreted).replace("\r\n", "\n").starts_with("[2, 3]\n"));
    assert!(stderr(&interpreted).contains("shape mismatch"), "unexpected stderr: {}", stderr(&interpreted));

    let exe = temp_artifact("shape_mismatch.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().unwrap();
    let _ = fs::remove_file(&exe);
    assert!(!native.status.success());
    assert!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n").starts_with("[2, 3]\n"));
    assert!(String::from_utf8_lossy(&native.stderr).contains("shape mismatch"));
}

#[test]
fn empty_mask_is_a_runtime_error_in_both_backends() {
    // A mask that selects nothing would produce an empty array, which Ostrin
    // arrays never are: both backends print the first selection, then fail.
    let file = example_path("array_empty_mask.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(!interpreted.status.success());
    assert!(stdout(&interpreted).replace("\r\n", "\n").starts_with("[1, 2, 3]\n"));
    assert!(stderr(&interpreted).contains("selects no elements"), "unexpected stderr: {}", stderr(&interpreted));

    let exe = temp_artifact("empty_mask.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().unwrap();
    let _ = fs::remove_file(&exe);
    assert!(!native.status.success());
    assert!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n").starts_with("[1, 2, 3]\n"));
    assert!(String::from_utf8_lossy(&native.stderr).contains("selects no elements"));
}

#[test]
fn table_library_module_runs_identically_in_both_backends() {
    let entry = example_path("data_project/app/main.ostrin");
    let interpreted = run(&["--run", &entry]);
    assert!(interpreted.status.success(), "stderr: {}", stderr(&interpreted));
    let exe = temp_artifact("table_lib.exe");
    let compiled = run(&["--compile", "--out", &exe, &entry]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "stderr: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native binary");
    let _ = fs::remove_file(&exe);
    let native_text = String::from_utf8_lossy(&native.stdout).to_string();
    assert_eq!(stdout(&interpreted).lines().collect::<Vec<_>>(), native_text.lines().collect::<Vec<_>>());
    assert!(stdout(&interpreted).contains("Cusco: n=2 media=11"), "unexpected output: {}", stdout(&interpreted));
}

#[test]
fn svg_plot_package_runs_identically_in_both_backends() {
    let entry = example_path("plot_project/app/main.ostrin");
    let interpreted = run(&["--run", &entry]);
    assert!(interpreted.status.success(), "stderr: {}", stderr(&interpreted));
    let exe = temp_artifact("plot_lib.exe");
    let compiled = run(&["--compile", "--out", &exe, &entry]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "stderr: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native binary");
    let _ = fs::remove_file(&exe);
    let native_text = String::from_utf8_lossy(&native.stdout).to_string();
    assert_eq!(stdout(&interpreted).lines().collect::<Vec<_>>(), native_text.lines().collect::<Vec<_>>());
    assert!(stdout(&interpreted).contains("<polyline"), "expected an SVG polyline: {}", stdout(&interpreted));
}

#[test]
fn autodiff_package_runs_identically_in_both_backends() {
    let entry = example_path("autodiff_project/app/main.ostrin");
    let interpreted = run(&["--run", &entry]);
    assert!(interpreted.status.success(), "stderr: {}", stderr(&interpreted));
    let text = stdout(&interpreted);
    // f(x) = x^3 - 2x - 5: f(2) = -1, f'(2) = 10; Newton converges to 2.0945514815423265.
    assert_eq!(text.lines().collect::<Vec<_>>(), ["0.25", "-1", "10", "1.58448345995801", "-51", "50", "2.0945514815423265"]);
    let exe = temp_artifact("autodiff_lib.exe");
    let compiled = run(&["--compile", "--out", &exe, &entry]);
    if skip_if_no_c_compiler(&compiled) {
        return;
    }
    assert!(compiled.status.success(), "stderr: {}", stderr(&compiled));
    let native = Command::new(&exe).output().expect("run native binary");
    let _ = fs::remove_file(&exe);
    let native_text = String::from_utf8_lossy(&native.stdout).to_string();
    assert_eq!(text.lines().collect::<Vec<_>>(), native_text.lines().collect::<Vec<_>>());
}
