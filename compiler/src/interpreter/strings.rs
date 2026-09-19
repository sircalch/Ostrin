//! Methods of `String`: `length`, `trim`, `split`, `lines`, `to_float`, …
//!
//! Everything here is ASCII-based on purpose (case mapping and whitespace), so that the native
//! backend (`STRINGS_RUNTIME` in `codegen.rs`, `strings_runtime.c`) can mirror it exactly
//! without a Unicode table. `length` counts characters (code points), not bytes.

use std::cell::RefCell;
use std::rc::Rc;

use super::{err_value, ok_value, RuntimeError, Value};

fn fail<T>(message: impl Into<String>) -> Result<T, RuntimeError> {
    Err(RuntimeError::Error(message.into()))
}

fn text_arg<'a>(args: &'a [Value], method: &str) -> Result<&'a str, RuntimeError> {
    match args.first() {
        Some(Value::String(s)) => Ok(s),
        _ => fail(format!("'{method}' expects a String argument")),
    }
}

fn list_of(items: Vec<String>) -> Value {
    Value::List(Rc::new(RefCell::new(items.into_iter().map(Value::String).collect())))
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0C' | '\r')
}

/// Returns `None` when `method` isn't a `String` method.
pub fn call_method(text: &str, method: &str, args: &[Value]) -> Option<Result<Value, RuntimeError>> {
    Some((|| -> Result<Value, RuntimeError> {
        match (method, args.len()) {
            ("length", 0) => Ok(Value::Int(text.chars().count() as i64)),
            ("is_empty", 0) => Ok(Value::Bool(text.is_empty())),
            ("trim", 0) => Ok(Value::String(text.trim_matches(is_space).to_string())),
            ("to_upper", 0) => Ok(Value::String(text.to_ascii_uppercase())),
            ("to_lower", 0) => Ok(Value::String(text.to_ascii_lowercase())),
            ("contains", 1) => Ok(Value::Bool(text.contains(text_arg(args, method)?))),
            ("starts_with", 1) => Ok(Value::Bool(text.starts_with(text_arg(args, method)?))),
            ("ends_with", 1) => Ok(Value::Bool(text.ends_with(text_arg(args, method)?))),
            ("replace", 2) => {
                let (Value::String(from), Value::String(to)) = (&args[0], &args[1]) else {
                    return fail("'replace' expects two String arguments");
                };
                if from.is_empty() {
                    return fail("'replace' needs a non-empty pattern");
                }
                Ok(Value::String(text.replace(from.as_str(), to)))
            }
            ("split", 1) => {
                let sep = text_arg(args, method)?;
                if sep.is_empty() {
                    return fail("'split' needs a non-empty separator");
                }
                Ok(list_of(text.split(sep).map(str::to_string).collect()))
            }
            ("lines", 0) => Ok(list_of(text.lines().map(str::to_string).collect())),
            ("to_int", 0) => Ok(match text.parse::<i64>() {
                Ok(v) => ok_value(Value::Int(v)),
                Err(e) => err_value(Value::String(e.to_string())),
            }),
            ("to_float", 0) => Ok(match text.parse::<f64>() {
                Ok(v) => ok_value(Value::Float(v)),
                Err(e) => err_value(Value::String(e.to_string())),
            }),
            _ => fail(format!("String has no method '{method}' with {} argument(s)", args.len())),
        }
    })())
    .filter(|_| STRING_METHODS.contains(&method))
}

pub const STRING_METHODS: &[&str] = &[
    "length", "is_empty", "trim", "to_upper", "to_lower", "contains", "starts_with", "ends_with", "replace", "split", "lines", "to_int", "to_float",
];

/// `list.join(separator)` for a list of strings.
pub fn join(items: &[Value], sep: &str) -> Result<Value, RuntimeError> {
    let mut parts = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::String(s) => parts.push(s.as_str()),
            _ => return fail("'join' needs a List<String>"),
        }
    }
    Ok(Value::String(parts.join(sep)))
}
