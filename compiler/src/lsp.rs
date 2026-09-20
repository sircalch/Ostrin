use std::collections::HashMap;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{json, Value};

use crate::ast::Span;
use crate::modules;
use crate::package;
use crate::protocol::{read_message, write_message};
use crate::symbols::{self, MemberSymbol, Symbol};
use crate::typeck::{Checker, EditorBinding, EditorExpression};

#[derive(Clone)]
struct DiagnosticRecord {
    message: String,
    code: Option<String>,
    line: Option<usize>,
    column: Option<usize>,
}

/// Everything the server knows about a single source file, refreshed every
/// time some open document's workspace analysis touches it. Kept indexed by
/// the same absolute-path string `modules.rs` stamps onto every AST node's
/// `source_file`, so cross-file lookups never depend on which document was
/// actually edited.
#[derive(Default, Clone)]
struct FileCache {
    symbols: Vec<Symbol>,
    members: Vec<MemberSymbol>,
    bindings: Vec<EditorBinding>,
    expressions: Vec<EditorExpression>,
}

struct WorkspaceResult {
    diagnostics_by_file: HashMap<String, Vec<DiagnosticRecord>>,
    symbols: Vec<Symbol>,
    members: Vec<MemberSymbol>,
    bindings: Vec<EditorBinding>,
    expressions: Vec<EditorExpression>,
}

/// Server-wide state. `documents` holds the live (possibly unsaved) text for
/// every open buffer; `index` accumulates per-file symbol/member/binding data
/// discovered while resolving any open document's imports, so a file that is
/// only ever *imported* (never itself opened) still gets indexed the moment
/// something that imports it is analyzed.
#[derive(Default)]
struct Server {
    documents: HashMap<String, String>,
    index: HashMap<String, FileCache>,
    root: Option<PathBuf>,
}

pub fn run() -> ExitCode {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let mut server = Server::default();
    let mut shutdown = false;

    loop {
        let Some(payload) = read_message(&mut reader).unwrap_or(None) else {
            break;
        };
        let message: Value = match serde_json::from_slice(&payload) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("ostrinc LSP: invalid JSON message: {error}");
                continue;
            }
        };
        if let Err(error) = handle_message(&message, &mut server, &mut writer, &mut shutdown) {
            eprintln!("ostrinc LSP: {error}");
        }
        if shutdown && message.get("method").and_then(Value::as_str) == Some("exit") {
            break;
        }
    }

    ExitCode::SUCCESS
}

fn respond<W: Write>(writer: &mut W, id: Value, result: Value) -> io::Result<()> {
    write_message(
        writer,
        &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    )
}

fn respond_error<W: Write>(writer: &mut W, id: Value, code: i64, message: &str) -> io::Result<()> {
    write_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        }),
    )
}

const TOKEN_TYPES: &[&str] = &[
    "function", "type", "enum", "enumMember", "interface", "property", "method", "variable",
];

fn handle_message<W: Write>(
    message: &Value,
    server: &mut Server,
    writer: &mut W,
    shutdown: &mut bool,
) -> io::Result<()> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = message.get("params").unwrap_or(&Value::Null);
    let request_id = message.get("id").cloned();

    match method {
        "initialize" => {
            server.root = params
                .get("rootUri")
                .and_then(Value::as_str)
                .and_then(uri_to_path)
                .or_else(|| {
                    params
                        .get("rootPath")
                        .and_then(Value::as_str)
                        .map(PathBuf::from)
                });
            if let Some(id) = request_id {
                respond(
                    writer,
                    id,
                    json!({
                        "serverInfo": { "name": "ostrinc", "version": "0.2.0" },
                        "capabilities": {
                            "textDocumentSync": { "openClose": true, "change": 1, "save": { "includeText": true } },
                            "hoverProvider": true,
                            "completionProvider": { "triggerCharacters": [".", ":"] },
                            "definitionProvider": true,
                            "signatureHelpProvider": { "triggerCharacters": ["(", ","] },
                            "referencesProvider": true,
                            "renameProvider": { "prepareProvider": false },
                            "semanticTokensProvider": {
                                "legend": { "tokenTypes": TOKEN_TYPES, "tokenModifiers": [] },
                                "full": true
                            }
                        }
                    }),
                )?;
            }
        }
        "initialized" => {}
        "shutdown" => {
            *shutdown = true;
            if let Some(id) = request_id {
                respond(writer, id, Value::Null)?;
            }
        }
        "exit" => {}
        "textDocument/didOpen" => {
            if let Some(text_document) = params.get("textDocument") {
                let uri = string_field(text_document, "uri");
                let text = string_field(text_document, "text");
                server.documents.insert(uri.clone(), text);
                analyze_and_publish(server, &uri, writer)?;
            }
        }
        "textDocument/didChange" => {
            let uri = params
                .get("textDocument")
                .map(|value| string_field(value, "uri"))
                .unwrap_or_default();
            if let Some(change) = params
                .get("contentChanges")
                .and_then(Value::as_array)
                .and_then(|changes| changes.first())
            {
                let text = string_field(change, "text");
                server.documents.insert(uri.clone(), text);
                analyze_and_publish(server, &uri, writer)?;
            }
        }
        "textDocument/didSave" => {
            let uri = params
                .get("textDocument")
                .map(|value| string_field(value, "uri"))
                .unwrap_or_default();
            if let Some(text) = params.get("text").and_then(Value::as_str) {
                server.documents.insert(uri.clone(), text.to_string());
                analyze_and_publish(server, &uri, writer)?;
            } else if server.documents.contains_key(&uri) {
                analyze_and_publish(server, &uri, writer)?;
            }
        }
        "textDocument/didClose" => {
            if let Some(uri) = params
                .get("textDocument")
                .map(|value| string_field(value, "uri"))
            {
                server.documents.remove(&uri);
                write_message(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": { "uri": uri, "diagnostics": [] }
                    }),
                )?;
            }
        }
        "textDocument/hover" => {
            if let Some(id) = request_id {
                let result = params
                    .get("textDocument")
                    .map(|value| string_field(value, "uri"))
                    .and_then(|uri| server.documents.get(&uri).map(|text| (uri, text.clone())))
                    .and_then(|(uri, text)| hover(server, &uri, &text, params.get("position")));
                respond(writer, id, result.unwrap_or(Value::Null))?;
            }
        }
        "textDocument/completion" => {
            if let Some(id) = request_id {
                let result = params
                    .get("textDocument")
                    .map(|value| string_field(value, "uri"))
                    .map(|uri| completion(server, &uri))
                    .unwrap_or_else(|| json!([]));
                respond(writer, id, result)?;
            }
        }
        "textDocument/definition" => {
            if let Some(id) = request_id {
                let result = params
                    .get("textDocument")
                    .map(|value| string_field(value, "uri"))
                    .and_then(|uri| server.documents.get(&uri).map(|text| (uri, text.clone())))
                    .map(|(uri, text)| definition(server, &uri, &text, params.get("position")))
                    .unwrap_or_else(|| json!([]));
                respond(writer, id, result)?;
            }
        }
        "textDocument/signatureHelp" => {
            if let Some(id) = request_id {
                let result = params
                    .get("textDocument")
                    .map(|value| string_field(value, "uri"))
                    .and_then(|uri| server.documents.get(&uri).map(|text| text.clone()))
                    .and_then(|text| signature_help(server, &text, params.get("position")));
                respond(writer, id, result.unwrap_or(Value::Null))?;
            }
        }
        "textDocument/references" => {
            if let Some(id) = request_id {
                let result = params
                    .get("textDocument")
                    .map(|value| string_field(value, "uri"))
                    .and_then(|uri| server.documents.get(&uri).map(|text| (uri, text.clone())))
                    .map(|(uri, text)| references(server, &uri, &text, params.get("position")))
                    .unwrap_or_else(|| json!([]));
                respond(writer, id, result)?;
            }
        }
        "textDocument/rename" => {
            if let Some(id) = request_id {
                let new_name = params
                    .get("newName")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let result = params
                    .get("textDocument")
                    .map(|value| string_field(value, "uri"))
                    .and_then(|uri| server.documents.get(&uri).map(|text| (uri, text.clone())))
                    .and_then(|(uri, text)| {
                        rename(server, &uri, &text, params.get("position"), &new_name)
                    });
                respond(writer, id, result.unwrap_or(Value::Null))?;
            }
        }
        "textDocument/semanticTokens/full" => {
            if let Some(id) = request_id {
                let result = params
                    .get("textDocument")
                    .map(|value| string_field(value, "uri"))
                    .and_then(|uri| server.documents.get(&uri).map(|text| (uri, text.clone())))
                    .map(|(uri, text)| semantic_tokens(server, &uri, &text))
                    .unwrap_or_else(|| json!({ "data": [] }));
                respond(writer, id, result)?;
            }
        }
        _ if request_id.is_some() => {
            respond_error(
                writer,
                request_id.unwrap_or(Value::Null),
                -32601,
                "Method not supported",
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Resolves `uri`'s whole project (its imports, and the dependencies declared
/// in a neighboring `ostrin.toml` if any), publishes diagnostics for every
/// file touched by that resolution, and folds the fresh symbol/member/binding
/// data into `server.index` so other documents benefit from it immediately.
fn analyze_and_publish<W: Write>(server: &mut Server, uri: &str, writer: &mut W) -> io::Result<()> {
    let Some(entry_path) = uri_to_path(uri) else {
        return Ok(());
    };
    let overrides = build_overrides(server);
    let result = analyze_workspace(&entry_path, &overrides);

    for (file, diagnostics) in &result.diagnostics_by_file {
        publish_diagnostics(writer, file, diagnostics)?;
    }

    let entry_key = entry_path.display().to_string();
    // Files that type-checked cleanly this round don't appear in
    // `diagnostics_by_file` at all; make sure their tab still clears any
    // stale diagnostics from a previous, broken revision.
    let mut touched: std::collections::HashSet<String> = std::collections::HashSet::new();
    touched.insert(entry_key.clone());
    for symbol in &result.symbols {
        if let Some(file) = &symbol.source_file {
            touched.insert(file.clone());
        }
    }
    for file in &touched {
        if !result.diagnostics_by_file.contains_key(file) {
            publish_diagnostics(writer, file, &[])?;
        }
    }

    regroup_into_index(&entry_key, result, &mut server.index);
    Ok(())
}

/// Every `.ostrin` file the server can see for a project-wide, text-based
/// search: every open buffer (its live, possibly-unsaved text) plus every
/// `.ostrin` file on disk under the workspace root that isn't currently open
/// (read fresh from disk, since nothing overrides it). Used by
/// references/rename so they aren't limited to documents the editor happens
/// to have open.
fn workspace_files(server: &Server) -> Vec<(String, String)> {
    let mut files: HashMap<String, String> = HashMap::new();
    let mut open_paths: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for (uri, text) in &server.documents {
        files.insert(uri.clone(), text.clone());
        if let Some(path) = uri_to_path(uri) {
            open_paths.insert(std::fs::canonicalize(&path).unwrap_or(path));
        }
    }
    if let Some(root) = &server.root {
        for path in walk_ostrin_files(root) {
            let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if open_paths.contains(&canonical) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                files.insert(path_to_uri(&path), text);
            }
        }
    }
    files.into_iter().collect()
}

/// Depth-limited recursive walk collecting `.ostrin` files, skipping the
/// directories a Rust/Ostrin project never wants scanned (VCS metadata,
/// dependency/build output). Best-effort: unreadable directories are
/// silently skipped rather than failing the whole request.
fn walk_ostrin_files(root: &Path) -> Vec<PathBuf> {
    const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules", ".vscode"];
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                if !SKIP_DIRS.contains(&name) {
                    stack.push(path);
                }
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("ostrin") {
                files.push(path);
            }
        }
    }
    files
}

fn build_overrides(server: &Server) -> HashMap<PathBuf, String> {
    let mut overrides = HashMap::new();
    for (uri, text) in &server.documents {
        if let Some(path) = uri_to_path(uri) {
            let canonical = std::fs::canonicalize(&path).unwrap_or(path);
            overrides.insert(canonical, text.clone());
        }
    }
    overrides
}

fn analyze_workspace(entry_path: &Path, overrides: &HashMap<PathBuf, String>) -> WorkspaceResult {
    let entry_key = entry_path.display().to_string();
    let manifest_path = entry_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("ostrin.toml");
    let deps = if manifest_path.is_file() {
        package::load_manifest(&manifest_path)
            .and_then(|manifest| package::resolve_dependency_roots(&manifest, manifest_path.parent().unwrap_or_else(|| Path::new(".")), false, false))
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let mut diagnostics_by_file: HashMap<String, Vec<DiagnosticRecord>> = HashMap::new();

    match modules::load_project(entry_path, &deps, overrides) {
        Err(diagnostics) => {
            for diagnostic in diagnostics {
                let key = diagnostic.file.clone().unwrap_or_else(|| entry_key.clone());
                diagnostics_by_file
                    .entry(key)
                    .or_default()
                    .push(DiagnosticRecord {
                        message: diagnostic.message,
                        code: None,
                        line: diagnostic.line,
                        column: diagnostic.col,
                    });
            }
            WorkspaceResult {
                diagnostics_by_file,
                symbols: Vec::new(),
                members: Vec::new(),
                bindings: Vec::new(),
                expressions: Vec::new(),
            }
        }
        Ok(items) => {
            let (type_errors, bindings, expressions) =
                Checker::new().check_program_with_editor_data(&items);
            for error in type_errors {
                let key = error.source_file.clone().unwrap_or_else(|| entry_key.clone());
                diagnostics_by_file
                    .entry(key)
                    .or_default()
                    .push(DiagnosticRecord {
                        message: error.message,
                        code: Some(format!("OSTRIN-{}", error.code)),
                        line: error.span.map(|span| span.line),
                        column: error.span.map(|span| span.col),
                    });
            }
            let mut symbols = symbols::collect(&items);
            let mut members = symbols::collect_members(&items);
            for symbol in &mut symbols {
                if symbol.source_file.is_none() {
                    symbol.source_file = Some(entry_key.clone());
                }
            }
            for member in &mut members {
                if member.source_file.is_none() {
                    member.source_file = Some(entry_key.clone());
                }
            }
            WorkspaceResult {
                diagnostics_by_file,
                symbols,
                members,
                bindings,
                expressions,
            }
        }
    }
}

fn regroup_into_index(entry_key: &str, result: WorkspaceResult, index: &mut HashMap<String, FileCache>) {
    let mut per_file: HashMap<String, FileCache> = HashMap::new();
    for symbol in result.symbols {
        let key = symbol.source_file.clone().unwrap_or_else(|| entry_key.to_string());
        per_file.entry(key).or_default().symbols.push(symbol);
    }
    for member in result.members {
        let key = member.source_file.clone().unwrap_or_else(|| entry_key.to_string());
        per_file.entry(key).or_default().members.push(member);
    }
    for binding in result.bindings {
        let key = binding.source_file.clone().unwrap_or_else(|| entry_key.to_string());
        per_file.entry(key).or_default().bindings.push(binding);
    }
    for expression in result.expressions {
        let key = expression.source_file.clone().unwrap_or_else(|| entry_key.to_string());
        per_file.entry(key).or_default().expressions.push(expression);
    }
    for (file, cache) in per_file {
        index.insert(file, cache);
    }
}

fn publish_diagnostics<W: Write>(writer: &mut W, file: &str, diagnostics: &[DiagnosticRecord]) -> io::Result<()> {
    let uri = path_to_uri(Path::new(file));
    let diagnostics = diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>();
    write_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": { "uri": uri, "diagnostics": diagnostics }
        }),
    )
}

fn diagnostic_json(diagnostic: &DiagnosticRecord) -> Value {
    let range = span_range(
        diagnostic.line.unwrap_or(1),
        diagnostic.column.unwrap_or(1),
        diagnostic.line.unwrap_or(1),
        diagnostic.column.unwrap_or(1).saturating_add(1),
    );
    json!({
        "range": range,
        "severity": 1,
        "code": diagnostic.code,
        "source": "ostrinc",
        "message": diagnostic.message
    })
}

fn own_file_key(uri: &str) -> Option<String> {
    uri_to_path(uri).map(|path| path.display().to_string())
}

fn hover(server: &Server, uri: &str, text: &str, position: Option<&Value>) -> Option<Value> {
    let (line, character) = position_pair(position?);
    let (word, start, end) = word_at(text, line, character)?;
    let own_key = own_file_key(uri);
    let own_cache = own_key.as_deref().and_then(|key| server.index.get(key));

    let binding = own_cache.and_then(|cache| cache.bindings.iter().find(|binding| binding.name == word));
    let member = all_files(server).find_map(|cache| {
        cache.members.iter().find(|member| member.name == word)
    });
    let symbol = all_files(server).find_map(|cache| {
        cache.symbols.iter().find(|symbol| short_name(&symbol.name) == word)
    });
    let line_one = line + 1;
    let column_one = character + 1;
    let expression = own_cache.and_then(|cache| {
        cache
            .expressions
            .iter()
            .filter(|expression| expression_contains(expression, line_one, column_one))
            .min_by_key(|expression| expression_size(expression))
    });

    let (markdown, range) = if let Some(binding) = binding {
        (
            format!("**Ostrin local binding**\n\n`{}: {}`", binding.name, binding.type_name),
            token_range(line, start, end),
        )
    } else if let Some(member) = member {
        (
            format!("**Ostrin {}**\n\n`{}`", member.kind, member.detail),
            token_range(line, start, end),
        )
    } else if let Some(symbol) = symbol {
        (
            format!("**Ostrin {}**\n\n`{}`", symbol.kind, symbol.detail),
            token_range(line, start, end),
        )
    } else if let Some(expression) = expression {
        (
            format!("**Ostrin inferred expression**\n\n`{}`", expression.type_name),
            span_range(expression.span.line, expression.span.col, expression.end.line, expression.end.col),
        )
    } else {
        return None;
    };
    Some(json!({
        "contents": { "kind": "markdown", "value": markdown },
        "range": range
    }))
}

fn all_files(server: &Server) -> impl Iterator<Item = &FileCache> {
    server.index.values()
}

fn completion(server: &Server, uri: &str) -> Value {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let own_key = own_file_key(uri);
    if let Some(cache) = own_key.as_deref().and_then(|key| server.index.get(key)) {
        for binding in &cache.bindings {
            if seen.insert(binding.name.clone()) {
                items.push(json!({ "label": binding.name, "kind": 6, "detail": binding.type_name }));
            }
        }
    }
    for cache in all_files(server) {
        for symbol in &cache.symbols {
            let name = short_name(&symbol.name);
            if seen.insert(name.to_string()) {
                items.push(json!({ "label": name, "kind": completion_kind(symbol.kind), "detail": symbol.detail }));
            }
        }
        for member in &cache.members {
            if seen.insert(member.name.clone()) {
                items.push(json!({ "label": member.name, "kind": 2, "detail": member.detail }));
            }
        }
    }
    Value::Array(items)
}

fn definition(server: &Server, uri: &str, text: &str, position: Option<&Value>) -> Value {
    let Some((line, character)) = position.map(position_pair) else {
        return json!([]);
    };
    let Some((word, _, _)) = word_at(text, line, character) else {
        return json!([]);
    };
    let own_key = own_file_key(uri);
    if let Some(binding) = own_key
        .as_deref()
        .and_then(|key| server.index.get(key))
        .and_then(|cache| cache.bindings.iter().find(|binding| binding.name == word))
    {
        let target_uri = binding
            .source_file
            .as_deref()
            .map(|file| path_to_uri(Path::new(file)))
            .unwrap_or_else(|| uri.to_string());
        return json!([location_json(&target_uri, binding.span)]);
    }
    for cache in all_files(server) {
        if let Some(symbol) = cache.symbols.iter().find(|symbol| short_name(&symbol.name) == word) {
            let target_uri = symbol
                .source_file
                .as_deref()
                .map(|file| path_to_uri(Path::new(file)))
                .unwrap_or_else(|| uri.to_string());
            return json!([location_json(&target_uri, symbol.span)]);
        }
        if let Some(member) = cache.members.iter().find(|member| member.name == word) {
            if let Some(span) = member.span {
                let target_uri = member
                    .source_file
                    .as_deref()
                    .map(|file| path_to_uri(Path::new(file)))
                    .unwrap_or_else(|| uri.to_string());
                return json!([location_json(&target_uri, span)]);
            }
        }
    }
    json!([])
}

fn signature_help(server: &Server, text: &str, position: Option<&Value>) -> Option<Value> {
    let (line, character) = position_pair(position?);
    let (name, active_parameter) = call_context(text, line, character)?;
    let mut candidates: Vec<(&str, &str)> = Vec::new();
    for cache in all_files(server) {
        for symbol in &cache.symbols {
            if (symbol.kind == "function" || symbol.kind == "method") && short_name(&symbol.name) == name {
                candidates.push((symbol.kind, symbol.detail.as_str()));
            }
        }
        for member in &cache.members {
            if member.kind == "method" && member.name == name {
                candidates.push((member.kind, member.detail.as_str()));
            }
        }
    }
    if candidates.is_empty() {
        return None;
    }
    let signatures: Vec<Value> = candidates
        .iter()
        .map(|(kind, detail)| {
            let parameters = parse_params(detail);
            json!({
                "label": detail,
                "documentation": format!("Ostrin {kind}"),
                "parameters": parameters.iter().map(|p| json!({ "label": p })).collect::<Vec<_>>()
            })
        })
        .collect();
    let parameter_count = parse_params(candidates[0].1).len();
    let active_parameter = if parameter_count == 0 {
        0
    } else {
        active_parameter.min(parameter_count - 1)
    };
    Some(json!({
        "signatures": signatures,
        "activeSignature": 0,
        "activeParameter": active_parameter
    }))
}

/// Scans backward from `line`/`character` for the call this cursor sits
/// inside of: the identifier right before the nearest unmatched `(`, plus how
/// many top-level commas separate it from the cursor (the active parameter).
fn call_context(text: &str, line: usize, character: usize) -> Option<(String, usize)> {
    let offset = offset_for(text, line, character)?;
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0i32;
    let mut active_parameter = 0usize;
    let mut i = offset;
    while i > 0 {
        i -= 1;
        match chars[i] {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    let mut j = i;
                    while j > 0 && chars[j - 1].is_whitespace() {
                        j -= 1;
                    }
                    let end = j;
                    while j > 0 && (chars[j - 1].is_alphanumeric() || chars[j - 1] == '_') {
                        j -= 1;
                    }
                    if j == end {
                        return None;
                    }
                    let name: String = chars[j..end].iter().collect();
                    return Some((name, active_parameter));
                }
                depth -= 1;
            }
            ',' if depth == 0 => active_parameter += 1,
            _ => {}
        }
    }
    None
}

fn offset_for(text: &str, line: usize, character: usize) -> Option<usize> {
    let mut offset = 0usize;
    for (index, current) in text.split('\n').enumerate() {
        if index == line {
            let chars: Vec<char> = current.chars().collect();
            return Some(offset + character.min(chars.len()));
        }
        offset += current.chars().count() + 1;
    }
    None
}

fn parse_params(detail: &str) -> Vec<String> {
    let Some(start) = detail.find('(') else {
        return Vec::new();
    };
    let chars: Vec<char> = detail.chars().collect();
    let mut depth = 0i32;
    let mut end = None;
    for (i, ch) in chars.iter().enumerate().skip(start) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        return Vec::new();
    };
    let inner: String = chars[start + 1..end].iter().collect();
    if inner.trim().is_empty() {
        return Vec::new();
    }
    let mut params = Vec::new();
    let mut current = String::new();
    let mut nesting = 0i32;
    for ch in inner.chars() {
        match ch {
            '<' | '(' => {
                nesting += 1;
                current.push(ch);
            }
            '>' | ')' => {
                nesting -= 1;
                current.push(ch);
            }
            ',' if nesting == 0 => {
                params.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        params.push(current.trim().to_string());
    }
    params
}

/// A word counts as a "local binding" when the current file's own index has
/// a binding by that name; renaming/finding references for it never needs to
/// leave this file. Anything else (functions, records, fields, methods…) is
/// treated as project-wide and searched across every open document's text.
fn is_local_binding(server: &Server, file_key: &str, word: &str) -> bool {
    server
        .index
        .get(file_key)
        .map(|cache| cache.bindings.iter().any(|binding| binding.name == word))
        .unwrap_or(false)
}

fn references(server: &Server, uri: &str, text: &str, position: Option<&Value>) -> Value {
    let Some((line, character)) = position.map(position_pair) else {
        return json!([]);
    };
    let Some((word, _, _)) = word_at(text, line, character) else {
        return json!([]);
    };
    let own_key = own_file_key(uri);
    let local = own_key
        .as_deref()
        .map(|key| is_local_binding(server, key, &word))
        .unwrap_or(false);

    let mut locations = Vec::new();
    if local {
        for (found_line, start, end) in find_word_occurrences(text, &word) {
            locations.push(location_range_json(uri, found_line, start, end));
        }
    } else {
        for (doc_uri, doc_text) in workspace_files(server) {
            for (found_line, start, end) in find_word_occurrences(&doc_text, &word) {
                locations.push(location_range_json(&doc_uri, found_line, start, end));
            }
        }
    }
    Value::Array(locations)
}

fn rename(server: &Server, uri: &str, text: &str, position: Option<&Value>, new_name: &str) -> Option<Value> {
    if new_name.is_empty() {
        return None;
    }
    let (line, character) = position_pair(position?);
    let (word, _, _) = word_at(text, line, character)?;
    let own_key = own_file_key(uri);
    let local = own_key
        .as_deref()
        .map(|key| is_local_binding(server, key, &word))
        .unwrap_or(false);

    let mut changes: HashMap<String, Vec<Value>> = HashMap::new();
    let mut record = |target_uri: &str, found_line: usize, start: usize, end: usize| {
        changes
            .entry(target_uri.to_string())
            .or_default()
            .push(json!({ "range": token_range(found_line, start, end), "newText": new_name }));
    };
    if local {
        for (found_line, start, end) in find_word_occurrences(text, &word) {
            record(uri, found_line, start, end);
        }
    } else {
        for (doc_uri, doc_text) in workspace_files(server) {
            for (found_line, start, end) in find_word_occurrences(&doc_text, &word) {
                record(&doc_uri, found_line, start, end);
            }
        }
    }
    if changes.is_empty() {
        return None;
    }
    let changes_json: serde_json::Map<String, Value> = changes
        .into_iter()
        .map(|(uri, edits)| (uri, Value::Array(edits)))
        .collect();
    Some(json!({ "changes": Value::Object(changes_json) }))
}

fn find_word_occurrences(text: &str, word: &str) -> Vec<(usize, usize, usize)> {
    let word_chars: Vec<char> = word.chars().collect();
    let word_len = word_chars.len();
    if word_len == 0 {
        return Vec::new();
    }
    let mut occurrences = Vec::new();
    for (line_index, line) in text.split('\n').enumerate() {
        let chars: Vec<char> = line.chars().collect();
        if chars.len() < word_len {
            continue;
        }
        let mut i = 0;
        while i + word_len <= chars.len() {
            if chars[i..i + word_len] == word_chars[..] {
                let is_word = |value: char| value.is_alphanumeric() || value == '_';
                let before_ok = i == 0 || !is_word(chars[i - 1]);
                let after_ok = i + word_len == chars.len() || !is_word(chars[i + word_len]);
                if before_ok && after_ok {
                    occurrences.push((line_index, i, i + word_len));
                    i += word_len;
                    continue;
                }
            }
            i += 1;
        }
    }
    occurrences
}

fn semantic_tokens(server: &Server, uri: &str, text: &str) -> Value {
    let Some(key) = own_file_key(uri) else {
        return json!({ "data": [] });
    };
    let Some(cache) = server.index.get(&key) else {
        return json!({ "data": [] });
    };

    let mut tagged: Vec<(usize, usize, usize, u32)> = Vec::new();
    let mut tag = |name: &str, type_index: u32| {
        for (line, start, end) in find_word_occurrences(text, name) {
            tagged.push((line, start, end - start, type_index));
        }
    };
    for symbol in &cache.symbols {
        let Some(type_index) = symbol_token_type(symbol.kind) else { continue };
        tag(short_name(&symbol.name), type_index);
    }
    for member in &cache.members {
        let Some(type_index) = member_token_type(member.kind) else { continue };
        tag(&member.name, type_index);
    }
    for binding in &cache.bindings {
        tag(&binding.name, 7);
    }

    tagged.sort_by_key(|(line, start, _, _)| (*line, *start));
    tagged.dedup_by_key(|(line, start, _, _)| (*line, *start));

    let mut data = Vec::with_capacity(tagged.len() * 5);
    let mut previous_line = 0usize;
    let mut previous_start = 0usize;
    for (line, start, length, type_index) in tagged {
        let delta_line = line.saturating_sub(previous_line);
        let delta_start = if delta_line == 0 { start.saturating_sub(previous_start) } else { start };
        data.push(delta_line as u64);
        data.push(delta_start as u64);
        data.push(length as u64);
        data.push(type_index as u64);
        data.push(0u64);
        previous_line = line;
        previous_start = start;
    }
    json!({ "data": data })
}

fn symbol_token_type(kind: &str) -> Option<u32> {
    match kind {
        "function" | "method" => Some(0),
        "record" => Some(1),
        "enum" => Some(2),
        "enumMember" => Some(3),
        "trait" => Some(4),
        "field" => Some(5),
        _ => None,
    }
}

fn member_token_type(kind: &str) -> Option<u32> {
    match kind {
        "method" => Some(6),
        "field" => Some(5),
        "enumMember" => Some(3),
        _ => None,
    }
}

fn completion_kind(kind: &str) -> u8 {
    match kind {
        "function" => 3,
        "record" => 22,
        "enum" => 13,
        "trait" => 8,
        "field" => 5,
        "enumMember" => 20,
        "method" => 2,
        _ => 6,
    }
}

fn location_json(uri: &str, span: Span) -> Value {
    json!({ "uri": uri, "range": span_range(span.line, span.col, span.line, span.col.saturating_add(1)) })
}

fn location_range_json(uri: &str, line: usize, start: usize, end: usize) -> Value {
    json!({ "uri": uri, "range": token_range(line, start, end) })
}

fn expression_contains(expression: &EditorExpression, line: usize, column: usize) -> bool {
    let start_line = expression.span.line;
    let start_column = expression.span.col;
    let end_line = expression.end.line.max(start_line);
    let end_column = expression.end.col.max(start_column + 1);
    if line < start_line || line > end_line {
        return false;
    }
    if line == start_line && column < start_column {
        return false;
    }
    if line == end_line && column >= end_column {
        return false;
    }
    true
}

fn expression_size(expression: &EditorExpression) -> usize {
    (expression.end.line.saturating_sub(expression.span.line)) * 100_000
        + expression
            .end
            .col
            .saturating_sub(expression.span.col)
            .max(1)
}

fn short_name(name: &str) -> &str {
    name.rsplit_once("::")
        .map(|(_, name)| name)
        .or_else(|| name.rsplit_once('.').map(|(_, name)| name))
        .unwrap_or(name)
}

fn position_pair(value: &Value) -> (usize, usize) {
    (
        value.get("line").and_then(Value::as_u64).unwrap_or(0) as usize,
        value.get("character").and_then(Value::as_u64).unwrap_or(0) as usize,
    )
}

fn word_at(text: &str, line: usize, character: usize) -> Option<(String, usize, usize)> {
    let line_text = text.lines().nth(line)?;
    let characters = line_text.chars().collect::<Vec<_>>();
    if characters.is_empty() {
        return None;
    }
    let cursor = character.min(characters.len());
    let is_word = |value: char| value.is_ascii_alphanumeric() || value == '_';
    let mut start = cursor;
    while start > 0 && is_word(characters[start - 1]) {
        start -= 1;
    }
    let mut end = cursor;
    while end < characters.len() && is_word(characters[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some((characters[start..end].iter().collect(), start, end))
}

fn token_range(line: usize, start: usize, end: usize) -> Value {
    json!({
        "start": { "line": line, "character": start },
        "end": { "line": line, "character": end }
    })
}

fn span_range(start_line: usize, start_column: usize, end_line: usize, end_column: usize) -> Value {
    json!({
        "start": { "line": start_line.saturating_sub(1), "character": start_column.saturating_sub(1) },
        "end": { "line": end_line.saturating_sub(1), "character": end_column.saturating_sub(1) }
    })
}

fn string_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Decodes the percent-escapes a client puts in `file://` URIs (spaces as
/// `%20`, etc.) and strips the extra leading slash Windows drive-letter URIs
/// carry (`file:///C:/...` -> `C:/...`).
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_decode(rest);
    let decoded = if decoded.len() > 2 && decoded.starts_with('/') && decoded.as_bytes()[2] == b':' {
        decoded[1..].to_string()
    } else {
        decoded
    };
    Some(PathBuf::from(decoded))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn path_to_uri(path: &Path) -> String {
    let normalized = path.display().to_string().replace('\\', "/");
    let mut out = String::from("file://");
    if !normalized.starts_with('/') {
        out.push('/');
    }
    for ch in normalized.chars() {
        match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' | ':' => out.push(ch),
            other => {
                let mut buffer = [0_u8; 4];
                for byte in other.encode_utf8(&mut buffer).bytes() {
                    out.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    out
}
