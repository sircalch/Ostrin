//! Elementary math functions on `Float`, `Float32` and (elementwise) `Array`s:
//! `sin cos tan asin acos atan sinh cosh tanh exp ln log10 sqrt floor ceil
//! round abs pow atan2 pi`. The result type always equals the argument type;
//! `Int` is never converted implicitly (the checker asks for `x as Float`).

use super::{array, detmath, RuntimeError, Value};

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
    "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "ln", "log10",
    "sqrt", "floor", "ceil", "round", "erf",
];

fn f64_fn(name: &str, x: f64) -> f64 {
    match name {
        "sin" => detmath::sin(x),
        "cos" => detmath::cos(x),
        "tan" => detmath::tan(x),
        "asin" => detmath::asin(x),
        "acos" => detmath::acos(x),
        "atan" => detmath::atan(x),
        "sinh" => detmath::sinh(x),
        "cosh" => detmath::cosh(x),
        "tanh" => detmath::tanh(x),
        "exp" => detmath::exp(x),
        "ln" => detmath::ln(x),
        "log10" => detmath::log10(x),
        "erf" => detmath::erf(x),
        // Exactly rounded by IEEE 754, so the platform's own routine is safe.
        "sqrt" => x.sqrt(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "round" => x.round(),
        _ => unreachable!("checked by is_math"),
    }
}

/// `Float32` functions compute in double precision and round once.
fn f32_fn(name: &str, x: f32) -> f32 {
    match name {
        "sqrt" => x.sqrt(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "round" => x.round(),
        other => f64_fn(other, x as f64) as f32,
    }
}

fn unary(name: &str, value: &Value) -> Result<Value, RuntimeError> {
    match value {
        Value::Float(x) if name == "abs" => Ok(Value::Float(x.abs())),
        Value::F32(x) if name == "abs" => Ok(Value::F32(x.abs())),
        Value::Int(n) if name == "abs" => n.checked_abs().map(Value::Int).ok_or_else(|| {
            RuntimeError::Error("integer overflow: abs of the smallest Int".to_string())
        }),
        Value::Sized(n, kind) if name == "abs" => {
            let magnitude = n.abs();
            if kind.fits(magnitude) {
                Ok(Value::Sized(magnitude, *kind))
            } else {
                Err(RuntimeError::Error(format!(
                    "integer overflow: abs({n}) does not fit in {}",
                    kind.name()
                )))
            }
        }
        Value::Float(x) => Ok(Value::Float(f64_fn(name, *x))),
        Value::F32(x) => Ok(Value::F32(f32_fn(name, *x))),
        Value::Array(a) => {
            let a = a.borrow();
            let data = a
                .data
                .iter()
                .map(|x| unary(name, x))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(array::make(a.shape.clone(), data))
        }
        other => Err(RuntimeError::Error(format!(
            "'{name}' isn't defined for '{other}'"
        ))),
    }
}

/// Evaluates a math builtin whose arguments are already evaluated.
pub fn call(name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
    match (name, args) {
        ("pi", []) => Ok(Value::Float(std::f64::consts::PI)),
        ("pow", [Value::Float(a), Value::Float(b)]) => Ok(Value::Float(detmath::pow(*a, *b))),
        ("pow", [Value::F32(a), Value::F32(b)]) => {
            Ok(Value::F32(detmath::pow(*a as f64, *b as f64) as f32))
        }
        ("atan2", [Value::Float(y), Value::Float(x)]) => Ok(Value::Float(detmath::atan2(*y, *x))),
        ("atan2", [Value::F32(y), Value::F32(x)]) => {
            Ok(Value::F32(detmath::atan2(*y as f64, *x as f64) as f32))
        }
        (_, [value]) => unary(name, value),
        _ => Err(RuntimeError::Error(format!(
            "'{name}' was called with unsupported arguments"
        ))),
    }
}
