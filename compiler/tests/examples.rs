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
fn structural_equality_matches_between_interpreter_and_native() {
    let file = example_path("structural_equality.ostrin");
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    let expected = stdout(&interpreted).replace("\r\n", "\n");
    assert_eq!(expected, "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n");

    let exe = temp_artifact("structural-equality.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "native compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("run structural equality binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "native binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
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
    assert_eq!(
        String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"),
        "main\ntask\n42\nscope-body\nscope-task\n7\n"
    );
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
    assert!(source.contains("ownership-ir inserted-releases: 1"), "unexpected lowering summary: {source}");
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
    // `try`. Lambda-based combinators and `catch` remain AST responsibilities
    // until the closure family is migrated.
    for (file, minimum_hir_functions) in [
        ("native_option.ostrin", 2usize),
        ("native_hir_option_locals.ostrin", 1usize),
        ("native_result.ostrin", 5usize),
        ("native_result_catch.ostrin", 2usize),
        ("try_result.ostrin", 5usize),
    ] {
        let report = run(&["--native-type-report", &example_path(file)]);
        if skip_if_no_c_compiler(&report) {
            return;
        }
        assert!(report.status.success(), "native type report failed for {file}: {}", stderr(&report));
        let hir_functions = stdout(&report)
            .lines()
            .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
            .unwrap_or(0);
        assert!(hir_functions >= minimum_hir_functions, "{file} generated only {hir_functions} HIR function(s), expected at least {minimum_hir_functions}");
    }
}

#[test]
fn native_hir_handles_collections_core() {
    // Collection literals, indexing, iteration and the non-closure methods
    // are now emitted directly from HIR. Closure combinators remain an AST
    // fallback until the closure family is migrated.
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
    let hir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(hir_functions >= 1, "collections example did not use the HIR backend");

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
fn native_hir_handles_closures_core() {
    // HIR now emits a captured closure, a named function used as a value and
    // the list combinators that invoke closures. The AST path remains the
    // fallback for nested/unsupported closure shapes.
    let file = example_path("native_hir_closures.ostrin");
    let expected = "15\n5\n";
    let interpreted = run(&["--run", &file]);
    assert!(interpreted.status.success(), "interpreter failed: {}", stderr(&interpreted));
    assert_eq!(stdout(&interpreted).replace("\r\n", "\n"), expected);

    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let hir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(hir_functions >= 2, "closure example generated only {hir_functions} HIR functions");

    let exe = temp_artifact("native_hir_closures.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run closure binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "closure binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
}

#[test]
fn native_hir_handles_concrete_generic_instances() {
    // The generic declaration is lowered once, then specialized into HIR for
    // each concrete call: scalar identity, List indexing, and Option methods
    // all share the same native representation as their non-generic forms.
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
    let hir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(hir_functions >= 4, "generic HIR example generated only {hir_functions} functions");

    let exe = temp_artifact("native_hir_generics.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run generic HIR binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "generic HIR binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), expected);
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
    assert!(hir_functions >= 10, "generic record/enum example generated only {hir_functions} HIR functions");

    let c = run(&["--emit-c", &file]);
    assert!(c.status.success(), "C emission failed: {}", stderr(&c));
    assert!(stdout(&c).contains("Pair__String_Int* __hir_rec"), "generic record literal did not come from HIR");

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
fn native_hir_handles_generic_methods() {
    // Generic method instances use the same pending queue as the AST backend,
    // but HIR must preserve the declaring impl when multiple methods share a
    // source name and must resolve nested calls such as `container.map<U>`.
    let file = example_path("native_generic_methods.ostrin");
    let report = run(&["--native-type-report", &file]);
    if skip_if_no_c_compiler(&report) {
        return;
    }
    assert!(report.status.success(), "native type report failed: {}", stderr(&report));
    let hir_functions = stdout(&report)
        .lines()
        .find_map(|line| line.strip_prefix("hir-generated: ").and_then(|n| n.trim().parse::<usize>().ok()))
        .unwrap_or(0);
    assert!(hir_functions >= 6, "generic method example generated only {hir_functions} HIR functions");

    let c = run(&["--emit-c", &file]);
    assert!(c.status.success(), "C emission failed: {}", stderr(&c));
    assert!(stdout(&c).contains("Box__String* __hir_rec"), "generic method record return did not come from HIR");

    let expected = run(&["--run", &file]);
    assert!(expected.status.success(), "interpreter failed: {}", stderr(&expected));
    let exe = temp_artifact("native_generic_methods_hir.exe");
    let compile = run(&["--compile", "--out", &exe, &file]);
    if skip_if_no_c_compiler(&compile) {
        return;
    }
    assert!(compile.status.success(), "compile failed: {}", stderr(&compile));
    let native = Command::new(&exe).output().expect("failed to run generic method binary");
    let _ = fs::remove_file(&exe);
    assert!(native.status.success(), "generic method binary failed: {}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(String::from_utf8_lossy(&native.stdout).replace("\r\n", "\n"), stdout(&expected).replace("\r\n", "\n"));
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
    assert!(lockfile.contains("resolved_path = \"../shared_lib\""), "lockfile should use a project-relative path: {lockfile}");
    assert!(!lockfile.contains("Lenguaje nuevo"), "lockfile should not embed this checkout's absolute path: {lockfile}");
}

#[test]
fn git_dependency_fails_clearly_without_network_access() {
    let out = run(&[&example_path("pkg_git_test/main.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("does not fetch git dependencies automatically"), "unexpected message: {err}");
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
