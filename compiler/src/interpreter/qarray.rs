//! `Array<Quantity<D>>`: a numeric array plus the unit every element shares
//! (`ArrayData::unit`). Operations work on the numbers and carry, convert or
//! combine the unit exactly as the scalar `Quantity` rules do, so each element
//! gets the value the scalar operation would give. The native runtime
//! (`codegen.rs`, `ostrin_qa_*`) mirrors this operation for operation.
use std::cell::RefCell;
use std::rc::Rc;

use super::array::{self, ArrayData};
use super::{convert, unit_error, RuntimeError, Value};
use crate::ast::BinOp;
use crate::types::{dim_div, dim_is_dimensionless, dim_mul, resolve_unit_expr, resolve_unit_factor, unit_combine};

type Res<T> = Result<T, RuntimeError>;

fn fail<T>(message: impl Into<String>) -> Res<T> {
    Err(RuntimeError::Error(message.into()))
}

pub fn unit_of(value: &Value) -> Option<String> {
    match value {
        Value::Array(a) => a.borrow().unit.clone(),
        _ => None,
    }
}

/// A new array with the same shape and numbers and the given unit.
pub fn with_unit(value: &Value, unit: Option<String>) -> Value {
    match value {
        Value::Array(a) => {
            let a = a.borrow();
            Value::Array(Rc::new(RefCell::new(ArrayData { shape: a.shape.clone(), data: a.data.clone(), unit })))
        }
        other => other.clone(),
    }
}

pub fn quantity(value: f64, unit: &str) -> Res<Value> {
    Ok(Value::Quantity(value, resolve_unit_expr(unit).map_err(unit_error)?, unit.to_string()))
}

fn number(value: &Value) -> Res<f64> {
    match value {
        Value::Float(f) => Ok(*f),
        Value::Int(n) => Ok(*n as f64),
        other => fail(format!("expected a number, got '{other}'")),
    }
}

/// The numbers of an array converted from one unit into another (elementwise,
/// with the scalar `convert` formula).
fn converted(value: &Value, from: &str, to: &str) -> Res<Value> {
    if from == to {
        return Ok(value.clone());
    }
    match value {
        Value::Array(a) => {
            let a = a.borrow();
            let data = a.data.iter().map(|v| Ok(Value::Float(convert(number(v)?, from, to)?))).collect::<Res<Vec<_>>>()?;
            Ok(array::make(a.shape.clone(), data))
        }
        other => Ok(Value::Float(convert(number(other)?, from, to)?)),
    }
}

/// `x as unit` on an array: converts a quantity array, or gives a plain
/// Float array a unit.
pub fn as_unit(value: &Value, target: &str) -> Res<Value> {
    let numbers = match unit_of(value) {
        Some(from) => converted(&with_unit(value, None), &from, target)?,
        None => value.clone(),
    };
    Ok(with_unit(&numbers, Some(target.to_string())))
}

/// `array([1 m, 250 cm])`: the numbers in the first element's unit.
pub fn from_quantities(value: &Value) -> Res<Option<Value>> {
    let Value::Array(a) = value else { return Ok(None) };
    let first_unit = match a.borrow().data.first() {
        Some(Value::Quantity(_, _, unit)) => unit.clone(),
        _ => return Ok(None),
    };
    let a = a.borrow();
    let data = a
        .data
        .iter()
        .map(|v| match v {
            Value::Quantity(n, _, unit) => Ok(Value::Float(convert(*n, unit, &first_unit)?)),
            other => fail(format!("array(...) mixes quantities with '{other}'")),
        })
        .collect::<Res<Vec<_>>>()?;
    Ok(Some(Value::Array(Rc::new(RefCell::new(ArrayData { shape: a.shape.clone(), data, unit: Some(first_unit) })))))
}

/// Splits an operand into its numbers and its unit (`None` for a plain number or array).
fn side(value: &Value) -> (Value, Option<String>) {
    match value {
        Value::Array(_) => (with_unit(value, None), unit_of(value)),
        Value::Quantity(n, _, unit) => (Value::Float(*n), Some(unit.clone())),
        other => (other.clone(), None),
    }
}

/// Binary operators with a quantity array on either side, or a plain array
/// with a scalar quantity. `None` when neither applies.
pub fn binary(op: BinOp, lv: &Value, rv: &Value) -> Option<Res<Value>> {
    let involved = unit_of(lv).is_some()
        || unit_of(rv).is_some()
        || (matches!(lv, Value::Array(_)) && matches!(rv, Value::Quantity(..)))
        || (matches!(lv, Value::Quantity(..)) && matches!(rv, Value::Array(_)));
    if !involved {
        return None;
    }
    Some(binary_units(op, lv, rv))
}

fn binary_units(op: BinOp, lv: &Value, rv: &Value) -> Res<Value> {
    use BinOp::*;
    let (a, ua) = side(lv);
    let (b, ub) = side(rv);
    match op {
        Add | Sub | Eq | NotEq | Lt | Gt | LtEq | GtEq => {
            let (Some(ua), Some(ub)) = (ua, ub) else {
                return fail("cannot combine a quantity array with a plain number without a unit ('as <unit>')");
            };
            // The right side is converted into the left side's unit.
            let b = converted(&b, &ub, &ua)?;
            let result = array::binary(op, a, b)?;
            Ok(if matches!(op, Add | Sub) { with_unit(&result, Some(ua)) } else { result })
        }
        Mul | Div => {
            let divide = op == Div;
            match (ua, ub) {
                (Some(ua), Some(ub)) => {
                    let (da, db) = (resolve_unit_expr(&ua).map_err(unit_error)?, resolve_unit_expr(&ub).map_err(unit_error)?);
                    let dim = if divide { dim_div(&da, &db) } else { dim_mul(&da, &db) };
                    if divide && dim_is_dimensionless(&dim) {
                        let b = converted(&b, &ub, &ua)?;
                        return array::binary(Div, a, b);
                    }
                    let (scale, unit) = unit_combine(&ua, &ub, divide).map_err(unit_error)?;
                    let mut result = array::binary(op, a, b)?;
                    if scale != 1.0 {
                        result = array::binary(Mul, result, Value::Float(scale))?;
                    }
                    if dim_is_dimensionless(&dim) {
                        if !unit.is_empty() {
                            result = array::binary(Mul, result, Value::Float(resolve_unit_factor(&unit).map_err(unit_error)?))?;
                        }
                        return Ok(result);
                    }
                    Ok(with_unit(&result, Some(unit)))
                }
                (Some(ua), None) => Ok(with_unit(&array::binary(op, a, b)?, Some(ua))),
                (None, Some(ub)) => {
                    let result = array::binary(op, a, b)?;
                    let unit = if divide { unit_combine("", &ub, true).map_err(unit_error)?.1 } else { ub };
                    Ok(with_unit(&result, Some(unit)))
                }
                (None, None) => unreachable!("binary() only calls this with a unit"),
            }
        }
        Rem | And | Or => fail("this operator isn't defined on arrays of quantities"),
    }
}

pub fn negate(value: &Value) -> Res<Value> {
    let unit = unit_of(value);
    Ok(with_unit(&array::negate(&with_unit(value, None))?, unit))
}

pub fn index(element: Value, unit: &str) -> Res<Value> {
    quantity(number(&element)?, unit)
}

/// Methods of a quantity array (`receiver` has a unit).
pub fn call_method(receiver: &Rc<RefCell<ArrayData>>, unit: &str, method: &str, args: Vec<Value>) -> Res<Value> {
    let plain = with_unit(&Value::Array(receiver.clone()), None);
    let Value::Array(numbers) = &plain else { unreachable!() };
    match method {
        "unit" => Ok(Value::String(unit.to_string())),
        "values" => Ok(plain.clone()),
        "sum" | "min" | "max" | "mean" | "median" | "std" | "sample_std" | "percentile" | "get" => {
            quantity(number(&array::call_method(numbers, method, args)?)?, unit)
        }
        "var" | "sample_var" => {
            let squared = unit_combine(unit, unit, false).map_err(unit_error)?.1;
            quantity(number(&array::call_method(numbers, method, args)?)?, &squared)
        }
        "to_list" => {
            let a = numbers.borrow();
            let items = a.data.iter().map(|v| quantity(number(v)?, unit)).collect::<Res<Vec<_>>>()?;
            Ok(Value::List(Rc::new(RefCell::new(items))))
        }
        "sort" | "cumsum" | "transpose" | "reshape" | "row" | "col" => {
            Ok(with_unit(&array::call_method(numbers, method, args)?, Some(unit.to_string())))
        }
        "shape" | "rank" | "size" | "length" | "count" => array::call_method(numbers, method, args),
        other => fail(format!("'{other}' isn't supported on arrays of quantities yet")),
    }
}
