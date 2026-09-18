use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::process::ExitCode;

use serde_json::{json, Value};

use crate::ast::Span;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::symbols::{self, MemberSymbol, Symbol};
use crate::typeck::{Checker, EditorBinding, EditorExpression};

#[derive(Clone)]
struct DiagnosticRecord {
    message: String,
    code: Option<String>,
    line: Option<usize>,
    column: Option<usize>,
}

struct Analysis {
    diagnostics: Vec<DiagnosticRecord>,
    symbols: Vec<Symbol>,
    members: Vec<MemberSymbol>,
    bindings: Vec<EditorBinding>,
    expressions: Vec<EditorExpression>,
}

struct Document {
    uri: String,
    text: String,
    analysis: Analysis,
}

pub fn run() -> ExitCode {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let mut documents = HashMap::<String, Document>::new();
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
        if let Err(error) = handle_message(&message, &mut documents, &mut writer, &mut shutdown) {
            eprintln!("ostrinc LSP: {error}");
        }
        if shutdown && message.get("method").and_then(Value::as_str) == Some("exit") {
            break;
        }
    }

    ExitCode::SUCCESS
}

fn read_message<R: BufRead + Read>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            return Ok(None);
        }
        let trimmed = header.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                content_length = Some(value.trim().parse::<usize>().map_err(|error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid Content-Length: {error}"),
                    )
                })?);
            }
        }
    }
    let length = content_length.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP message has no Content-Length header",
        )
    })?;
    let mut body = vec![0_u8; length];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

fn write_message<W: Write>(writer: &mut W, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
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

fn handle_message<W: Write>(
    message: &Value,
    documents: &mut HashMap<String, Document>,
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
            if let Some(id) = request_id {
                respond(
                    writer,
                    id,
                    json!({
                        "serverInfo": { "name": "ostrinc", "version": "0.1.0" },
                        "capabilities": {
                            "textDocumentSync": { "openClose": true, "change": 1, "save": { "includeText": true } },
                            "hoverProvider": true,
                            "completionProvider": { "triggerCharacters": [".", ":"] },
                            "definitionProvider": true
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
                let analysis = analyze(&text, &uri);
                let document = Document {
                    uri: uri.clone(),
                    text,
                    analysis,
                };
                publish_diagnostics(writer, &document)?;
                documents.insert(uri, document);
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
                let analysis = analyze(&text, &uri);
                let document = Document {
                    uri: uri.clone(),
                    text,
                    analysis,
                };
                publish_diagnostics(writer, &document)?;
                documents.insert(uri, document);
            }
        }
        "textDocument/didSave" => {
            let uri = params
                .get("textDocument")
                .map(|value| string_field(value, "uri"))
                .unwrap_or_default();
            if let Some(text) = params.get("text").and_then(Value::as_str) {
                let analysis = analyze(text, &uri);
                let document = Document {
                    uri: uri.clone(),
                    text: text.to_string(),
                    analysis,
                };
                publish_diagnostics(writer, &document)?;
                documents.insert(uri, document);
            }
        }
        "textDocument/didClose" => {
            if let Some(uri) = params
                .get("textDocument")
                .map(|value| string_field(value, "uri"))
            {
                documents.remove(&uri);
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
                    .and_then(|uri| documents.get(&uri).map(|document| (uri, document)))
                    .and_then(|(uri, document)| hover(document, params.get("position"), &uri));
                respond(writer, id, result.unwrap_or(Value::Null))?;
            }
        }
        "textDocument/completion" => {
            if let Some(id) = request_id {
                let result = params
                    .get("textDocument")
                    .map(|value| string_field(value, "uri"))
                    .and_then(|uri| documents.get(&uri))
                    .map(completion)
                    .unwrap_or_else(|| json!([]));
                respond(writer, id, result)?;
            }
        }
        "textDocument/definition" => {
            if let Some(id) = request_id {
                let result = params
                    .get("textDocument")
                    .map(|value| string_field(value, "uri"))
                    .and_then(|uri| documents.get(&uri).map(|document| (uri, document)))
                    .map(|(uri, document)| definition(document, params.get("position"), &uri))
                    .unwrap_or_else(|| json!([]));
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

fn analyze(text: &str, source_file: &str) -> Analysis {
    let tokens = match Lexer::new(text).tokenize() {
        Ok(tokens) => tokens,
        Err(error) => {
            return Analysis {
                diagnostics: vec![DiagnosticRecord {
                    message: format!("lex error: {}", error.message),
                    code: None,
                    line: Some(error.line),
                    column: Some(error.col),
                }],
                symbols: Vec::new(),
                members: Vec::new(),
                bindings: Vec::new(),
                expressions: Vec::new(),
            };
        }
    };
    let (items, parse_errors) = Parser::new(tokens).parse_program();
    let mut diagnostics = parse_errors
        .into_iter()
        .map(|error| DiagnosticRecord {
            message: format!("parse error: {}", error.message),
            code: None,
            line: Some(error.line),
            column: Some(error.col),
        })
        .collect::<Vec<_>>();
    let (type_errors, bindings, expressions) =
        Checker::new().check_program_with_editor_data(&items);
    diagnostics.extend(type_errors.into_iter().map(|error| DiagnosticRecord {
        message: error.message,
        code: Some(format!("OSTRIN-{}", error.code)),
        line: error.span.map(|span| span.line),
        column: error.span.map(|span| span.col),
    }));
    let mut symbols = symbols::collect(&items);
    let mut members = symbols::collect_members(&items);
    for symbol in &mut symbols {
        if symbol.source_file.is_none() {
            symbol.source_file = Some(source_file.to_string());
        }
    }
    for member in &mut members {
        if member.source_file.is_none() {
            member.source_file = Some(source_file.to_string());
        }
    }
    Analysis {
        diagnostics,
        symbols,
        members,
        bindings,
        expressions,
    }
}

fn publish_diagnostics<W: Write>(writer: &mut W, document: &Document) -> io::Result<()> {
    let diagnostics = document
        .analysis
        .diagnostics
        .iter()
        .map(diagnostic_json)
        .collect::<Vec<_>>();
    write_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": { "uri": document.uri, "diagnostics": diagnostics }
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

fn hover(document: &Document, position: Option<&Value>, uri: &str) -> Option<Value> {
    let (line, character) = position_pair(position?);
    let (word, start, end) = word_at(&document.text, line, character)?;
    let member = document
        .analysis
        .members
        .iter()
        .find(|member| member.name == word);
    let binding = document
        .analysis
        .bindings
        .iter()
        .find(|binding| binding.name == word);
    let symbol = document
        .analysis
        .symbols
        .iter()
        .find(|symbol| short_name(&symbol.name) == word);
    let line_one = line + 1;
    let column_one = character + 1;
    let expression = document
        .analysis
        .expressions
        .iter()
        .filter(|expression| expression_contains(expression, line_one, column_one))
        .min_by_key(|expression| expression_size(expression));
    let (markdown, range) = if let Some(member) = member {
        (
            format!("**Ostrin {}**\n\n`{}`", member.kind, member.detail),
            token_range(line, start, end),
        )
    } else if let Some(binding) = binding {
        (
            format!(
                "**Ostrin local binding**\n\n`{}: {}`",
                binding.name, binding.type_name
            ),
            token_range(line, start, end),
        )
    } else if let Some(symbol) = symbol {
        (
            format!("**Ostrin {}**\n\n`{}`", symbol.kind, symbol.detail),
            token_range(line, start, end),
        )
    } else if let Some(expression) = expression {
        (
            format!(
                "**Ostrin inferred expression**\n\n`{}`",
                expression.type_name
            ),
            span_range(
                expression.span.line,
                expression.span.col,
                expression.end.line,
                expression.end.col,
            ),
        )
    } else {
        return None;
    };
    Some(json!({
        "contents": { "kind": "markdown", "value": markdown },
        "range": range,
        "_ostrinUri": uri
    }))
}

fn completion(document: &Document) -> Value {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for symbol in &document.analysis.symbols {
        let name = short_name(&symbol.name);
        if seen.insert(name.to_string()) {
            items.push(json!({ "label": name, "kind": completion_kind(symbol.kind), "detail": symbol.detail }));
        }
    }
    for member in &document.analysis.members {
        if seen.insert(member.name.clone()) {
            items.push(json!({ "label": member.name, "kind": 2, "detail": member.detail }));
        }
    }
    for binding in &document.analysis.bindings {
        if seen.insert(binding.name.clone()) {
            items.push(json!({ "label": binding.name, "kind": 6, "detail": binding.type_name }));
        }
    }
    Value::Array(items)
}

fn definition(document: &Document, position: Option<&Value>, uri: &str) -> Value {
    let Some((line, character)) = position.map(position_pair) else {
        return json!([]);
    };
    let Some((word, _, _)) = word_at(&document.text, line, character) else {
        return json!([]);
    };
    if let Some(binding) = document
        .analysis
        .bindings
        .iter()
        .find(|binding| binding.name == word)
    {
        return json!([location_json(uri, binding.span)]);
    }
    if let Some(symbol) = document
        .analysis
        .symbols
        .iter()
        .find(|symbol| short_name(&symbol.name) == word)
    {
        return json!([location_json(uri, symbol.span)]);
    }
    if let Some(member) = document
        .analysis
        .members
        .iter()
        .find(|member| member.name == word)
    {
        if let Some(span) = member.span {
            return json!([location_json(uri, span)]);
        }
    }
    json!([])
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
