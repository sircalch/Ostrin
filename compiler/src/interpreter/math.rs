//! Elementary math functions on `Float`, `Float32` and (elementwise) `Array`s:
//! `sin cos tan asin acos atan sinh cosh tanh exp ln log10 sqrt floor ceil
//! round abs pow atan2 pi`. The result type always equals the argument type;
//! `Int` is never converted implicitly (the checker asks for `x as Float`).

use super::{array, RuntimeError, Value};

/// True when `name` with `arity` arguments is one of the math builtins.
pub fn is_math(name: &str, arity: usize) -> bool {
    match arity {
        0 => name == "pi",
        1 => UNARY.contains(&name) || name == "abs",
        2 => matches!(name, "pow" | "atan2"),
        _ => false,
    }
}

pub const UNARY: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "ln", "log10", "sqrt", "floor", "ceil", "round",
];

fn f64_fn(name: &str, x: f64) -> f64 {
    match name {
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        "sinh" => x.sinh(),
        "cosh" => x.cosh(),
        "tanh" => x.tanh(),
        "exp" => x.exp(),
        "ln" => x.ln(),
        "log10" => x.log10(),
        "sqrt" => x.sqrt(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "round" => x.round(),
        _ => unreachable!("checked by is_math"),
    }
}

fn f32_fn(name: &str, x: f32) -> f32 {
    match name {
        "sin" => x.sin(),
        "cos" => x.cos(),
        "tan" => x.tan(),
        "asin" => x.asin(),
        "acos" => x.acos(),
        "atan" => x.atan(),
        "sinh" => x.sinh(),
        "cosh" => x.cosh(),
        "tanh" => x.tanh(),
        "exp" => x.exp(),
        "ln" => x.ln(),
        "log10" => x.log10(),
        "sqrt" => x.sqrt(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "round" => x.round(),
        _ => unreachable!("checked by is_math"),
    }
}

fn unary(name: &str, value: &Value) -> Result<Value, RuntimeError> {
    match value {
        Value::Float(x) if name == "abs" => Ok(Value::Float(x.abs())),
        Value::F32(x) if name == "abs" => Ok(Value::F32(x.abs())),
        Value::Int(n) if name == "abs" => {
            n.checked_abs().map(Value::Int).ok_or_else(|| RuntimeError::Error("integer overflow: abs of the smallest Int".to_string()))
        }
        Value::Sized(n, kind) if name == "abs" => {
            let magnitude = n.abs();
            if kind.fits(magnitude) {
                Ok(Value::Sized(magnitude, *kind))
            } else {
                Err(RuntimeError::Error(format!("integer overflow: abs({n}) does not fit in {}", kind.name())))
            }
        }
        Value::Float(x) => Ok(Value::Float(f64_fn(name, *x))),
        Value::F32(x) => Ok(Value::F32(f32_fn(name, *x))),
        Value::Array(a) => {
            let a = a.borrow();
            let data = a.data.iter().map(|x| unary(name, x)).collect::<Result<Vec<_>, _>>()?;
            Ok(array::make(a.shape.clone(), data))
        }
        other => Err(RuntimeError::Error(format!("'{name}' isn't defined for '{other}'"))),
    }
}

/// Evaluates a math builtin whose arguments are already evaluated.
pub fn call(name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
    match (name, args) {
        ("pi", []) => Ok(Value::Float(std::f64::consts::PI)),
        ("pow", [Value::Float(a), Value::Float(b)]) => Ok(Value::Float(a.powf(*b))),
        ("pow", [Value::F32(a), Value::F32(b)]) => Ok(Value::F32(a.powf(*b))),
        ("atan2", [Value::Float(y), Value::Float(x)]) => Ok(Value::Float(y.atan2(*x))),
        ("atan2", [Value::F32(y), Value::F32(x)]) => Ok(Value::F32(y.atan2(*x))),
        (_, [value]) => unary(name, value),
        _ => Err(RuntimeError::Error(format!("'{name}' was called with unsupported arguments"))),
    }
}
