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
    assert!(source.contains("int64_t fib(int64_t n)"));
    assert!(source.contains("int main(void)"), "the generated file must supply its own C main: {source}");
}

#[test]
fn native_backend_rejects_constructs_it_does_not_support_yet() {
    // `shapes.ostrin` uses records/traits, which only the interpreter runs;
    // the native backend must fail with a clear message pointing back at
    // `--run`, not silently emit something wrong.
    let out = run(&["--emit-c", &example_path("shapes.ostrin")]);
    assert!(!out.status.success(), "the native backend should refuse a program it can't fully compile");
    let error = stderr(&out);
    assert!(error.contains("--run"), "the error should point users back at the interpreter: {error}");
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
fn spawn_capturing_mut_is_rejected() {
    let out = run(&[&example_path("concurrency_errors.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("E1100"));
}

#[test]
fn moved_channel_value_cannot_be_reused() {
    let out = run(&["--run", &example_path("moved_after_send.ostrin")]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("moved"));
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
fn git_dependency_fails_clearly_without_network_access() {
    let out = run(&[&example_path("pkg_git_test/main.ostrin")]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("does not fetch git dependencies automatically"), "unexpected message: {err}");
}
