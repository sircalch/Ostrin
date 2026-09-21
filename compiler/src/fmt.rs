//! Official source formatter (`ostrinc --fmt`).
//!
//! The formatter is deliberately conservative: it normalizes layout only
//! (indentation by bracket depth, trailing whitespace, blank-line runs, line
//! endings, final newline) and never rewrites tokens. `format_source` verifies
//! that the lexer sees exactly the same token stream before and after, so a
//! formatting bug can never change the meaning of a program. Text inside block
//! comments and strings that span lines is preserved verbatim.

use crate::lexer::Lexer;

const INDENT: &str = "    ";

fn signature(source: &str) -> Result<Vec<String>, String> {
    let tokens = Lexer::new(source).tokenize().map_err(|error| format!("{error:?}"))?;
    Ok(tokens.iter().map(|t| format!("{:?}\u{0}{}", t.kind, t.lexeme)).collect())
}

/// Formats `source`. Fails (leaving the caller's text untouched) if the input
/// does not lex or if the layout pass would change the token stream.
pub fn format_source(source: &str) -> Result<String, String> {
    let before = signature(source)?;
    let formatted = layout(source);
    let after = signature(&formatted)?;
    if before != after {
        return Err("internal formatter error: token stream changed; refusing to format".to_string());
    }
    Ok(formatted)
}

fn layout(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let mut out: Vec<String> = Vec::new();
    let mut depth: i64 = 0;
    let mut in_block = false;
    let mut in_string = false;
    let mut pending_blank = false;

    for line in normalized.split('\n') {
        if in_block || in_string {
            // Continuation of a multi-line comment/string: keep verbatim.
            flush_blank(&mut out, &mut pending_blank, false);
            out.push(line.to_string());
            scan(line, &mut depth, &mut in_block, &mut in_string);
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            pending_blank = !out.is_empty();
            continue;
        }
        let leading_closers = trimmed.chars().take_while(|c| matches!(c, '}' | ')' | ']')).count() as i64;
        let closes_block = trimmed.starts_with('}');
        flush_blank(&mut out, &mut pending_blank, closes_block);
        let indent = (depth - leading_closers).max(0) as usize;
        out.push(format!("{}{}", INDENT.repeat(indent), trimmed));
        scan(trimmed, &mut depth, &mut in_block, &mut in_string);
        depth = depth.max(0);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// Emits at most one blank line, and none right after `{` or before `}`.
fn flush_blank(out: &mut Vec<String>, pending: &mut bool, before_close: bool) {
    if *pending && !before_close && !out.last().is_some_and(|l| l.trim_end().ends_with('{')) {
        out.push(String::new());
    }
    *pending = false;
}

/// Updates bracket depth and comment/string state across one line.
fn scan(line: &str, depth: &mut i64, in_block: &mut bool, in_string: &mut bool) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if *in_block {
            if c == '*' && chars.get(i + 1) == Some(&'/') {
                *in_block = false;
                i += 1;
            }
        } else if *in_string {
            if c == '\\' {
                i += 1;
            } else if c == '"' {
                *in_string = false;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            return;
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            *in_block = true;
            i += 1;
        } else if c == '"' {
            *in_string = true;
        } else if matches!(c, '{' | '(' | '[') {
            *depth += 1;
        } else if matches!(c, '}' | ')' | ']') {
            *depth -= 1;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indents_by_depth_and_is_idempotent() {
        let src = "fn main() -> Void {\r\nprint(\"a{\")   \n\n\n    if true {\nprint(1)\n}\n}\n\n\n";
        let once = format_source(src).unwrap();
        assert_eq!(
            once,
            "fn main() -> Void {\n    print(\"a{\")\n\n    if true {\n        print(1)\n    }\n}\n"
        );
        assert_eq!(format_source(&once).unwrap(), once);
    }

    #[test]
    fn keeps_block_comments_verbatim() {
        let src = "/* a\n      b */\nfn f() -> Void {\n// c {\n}\n";
        let out = format_source(src).unwrap();
        assert!(out.starts_with("/* a\n      b */\n"));
        assert_eq!(format_source(&out).unwrap(), out);
    }
}
