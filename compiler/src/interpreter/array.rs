//! `Array<T>`: dense, row-major, N-dimensional numeric arrays with NumPy-style
//! broadcasting. Elements are ordinary numeric `Value`s, and every element
//! operation goes through `eval_binary_builtin`, so an array of `UInt8` checks
//! overflow and an array of `Float32` rounds to single precision exactly like
//! the scalars do. Arrays are never empty (every dimension is at least 1),
//! which keeps the element type observable at run time.

use std::cell::RefCell;
use std::rc::Rc;

use super::{eval_binary_builtin, RuntimeError, Value};
use crate::ast::BinOp;

pub struct ArrayData {
    pub shape: Vec<usize>,
    pub data: Vec<Value>,
}

type Res<T> = Result<T, RuntimeError>;

fn fail<T>(message: impl Into<String>) -> Res<T> {
    Err(RuntimeError::Error(message.into()))
}

pub fn make(shape: Vec<usize>, data: Vec<Value>) -> Value {
    Value::Array(Rc::new(RefCell::new(ArrayData { shape, data })))
}

fn list_of(values: Vec<Value>) -> Value {
    Value::List(Rc::new(RefCell::new(values)))
}

fn shape_arg(value: &Value) -> Res<Vec<usize>> {
    let Value::List(items) = value else { return fail("an array shape must be a List<Int>") };
    let mut shape = Vec::new();
    for item in items.borrow().iter() {
        match item {
            Value::Int(n) if *n >= 1 => shape.push(*n as usize),
            Value::Int(n) => return fail(format!("array dimensions must be at least 1, got {n}")),
            other => return fail(format!("an array shape must hold Int values, got '{other}'")),
        }
    }
    if shape.is_empty() {
        return fail("an array needs at least one dimension");
    }
    Ok(shape)
}

fn total(shape: &[usize]) -> usize {
    shape.iter().product()
}

/// `array([[1, 2], [3, 4]])`: builds an array from (nested) lists.
pub fn from_list(value: &Value) -> Res<Value> {
    fn dims(v: &Value, shape: &mut Vec<usize>) {
        if let Value::List(items) = v {
            let items = items.borrow();
            shape.push(items.len());
            if let Some(first) = items.first() {
                dims(first, shape);
            }
        }
    }
    fn collect(v: &Value, depth: usize, shape: &[usize], out: &mut Vec<Value>) -> Res<()> {
        if depth == shape.len() {
            return match v {
                Value::List(_) => fail("array(...) needs lists nested to the same depth"),
                other => {
                    out.push(other.clone());
                    Ok(())
                }
            };
        }
        let Value::List(items) = v else { return fail("array(...) needs lists nested to the same depth") };
        let items = items.borrow();
        if items.len() != shape[depth] {
            return fail("array(...) needs rectangular nested lists (all rows the same length)");
        }
        for item in items.iter() {
            collect(item, depth + 1, shape, out)?;
        }
        Ok(())
    }
    let mut shape = Vec::new();
    dims(value, &mut shape);
    if shape.is_empty() || shape.contains(&0) {
        return fail("an array can't be empty");
    }
    let mut data = Vec::with_capacity(total(&shape));
    collect(value, 0, &shape, &mut data)?;
    Ok(make(shape, data))
}

pub fn full(shape: &Value, value: Value) -> Res<Value> {
    let shape = shape_arg(shape)?;
    let n = total(&shape);
    Ok(make(shape, vec![value; n]))
}

pub fn arange(start: i64, stop: i64) -> Res<Value> {
    if stop <= start {
        return fail("arange needs start < stop");
    }
    Ok(make(vec![(stop - start) as usize], (start..stop).map(Value::Int).collect()))
}

pub fn linspace(a: f64, b: f64, n: i64) -> Res<Value> {
    if n < 1 {
        return fail("linspace needs at least one point");
    }
    let n = n as usize;
    let data = (0..n)
        .map(|i| {
            if i + 1 == n && n > 1 {
                Value::Float(b)
            } else if n == 1 {
                Value::Float(a)
            } else {
                Value::Float(a + (b - a) * (i as f64) / ((n - 1) as f64))
            }
        })
        .collect();
    Ok(make(vec![n], data))
}

fn broadcast_shape(a: &[usize], b: &[usize]) -> Res<Vec<usize>> {
    let rank = a.len().max(b.len());
    let mut out = vec![0; rank];
    for i in 0..rank {
        let da = if i < rank - a.len() { 1 } else { a[i - (rank - a.len())] };
        let db = if i < rank - b.len() { 1 } else { b[i - (rank - b.len())] };
        out[i] = if da == db {
            da
        } else if da == 1 {
            db
        } else if db == 1 {
            da
        } else {
            return fail(format!("shape mismatch: {a:?} and {b:?} can't be broadcast together"));
        };
    }
    Ok(out)
}

/// Linear index into an array of shape `shape` for output coordinates `coords`
/// of a (right-aligned, broadcast) result.
fn broadcast_index(shape: &[usize], coords: &[usize]) -> usize {
    let offset = coords.len() - shape.len();
    let mut index = 0;
    for (d, &dim) in shape.iter().enumerate() {
        let c = if dim == 1 { 0 } else { coords[d + offset] };
        index = index * dim + c;
    }
    index
}

fn coords_of(mut linear: usize, shape: &[usize]) -> Vec<usize> {
    let mut coords = vec![0; shape.len()];
    for d in (0..shape.len()).rev() {
        coords[d] = linear % shape[d];
        linear /= shape[d];
    }
    coords
}

/// `+ - * /` between two arrays (broadcast) or an array and a scalar.
pub fn binary(op: BinOp, lv: Value, rv: Value) -> Res<Value> {
    if !matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div) {
        return fail("only + - * / are defined on arrays");
    }
    match (&lv, &rv) {
        (Value::Array(a), Value::Array(b)) => {
            let (a, b) = (a.borrow(), b.borrow());
            let shape = broadcast_shape(&a.shape, &b.shape)?;
            let n = total(&shape);
            let mut data = Vec::with_capacity(n);
            for linear in 0..n {
                let coords = coords_of(linear, &shape);
                let x = a.data[broadcast_index(&a.shape, &coords)].clone();
                let y = b.data[broadcast_index(&b.shape, &coords)].clone();
                data.push(eval_binary_builtin(op, x, y)?);
            }
            Ok(make(shape, data))
        }
        (Value::Array(a), scalar) => {
            let a = a.borrow();
            let data = a.data.iter().map(|x| eval_binary_builtin(op, x.clone(), scalar.clone())).collect::<Res<Vec<_>>>()?;
            Ok(make(a.shape.clone(), data))
        }
        (scalar, Value::Array(b)) => {
            let b = b.borrow();
            let data = b.data.iter().map(|y| eval_binary_builtin(op, scalar.clone(), y.clone())).collect::<Res<Vec<_>>>()?;
            Ok(make(b.shape.clone(), data))
        }
        _ => unreachable!("binary() is only called with at least one array"),
    }
}

pub fn negate(value: &Value) -> Res<Value> {
    let Value::Array(a) = value else { return fail("cannot negate this value") };
    let a = a.borrow();
    let data = a
        .data
        .iter()
        .map(|x| match x {
            Value::Int(n) => Ok(Value::Int(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            Value::F32(f) => Ok(Value::F32(-f)),
            Value::Sized(n, k) if k.is_signed() && k.fits(-n) => Ok(Value::Sized(-n, *k)),
            other => fail(format!("cannot negate '{other}'")),
        })
        .collect::<Res<Vec<_>>>()?;
    Ok(make(a.shape.clone(), data))
}

fn offset(shape: &[usize], indices: &[Value]) -> Res<usize> {
    if indices.len() != shape.len() {
        return fail(format!("this array has {} dimension(s), but {} index(es) were given", shape.len(), indices.len()));
    }
    let mut linear = 0;
    for (dim, index) in shape.iter().zip(indices) {
        let Value::Int(i) = index else { return fail("array indices must be Int") };
        if *i < 0 || (*i as usize) >= *dim {
            return fail(format!("index out of bounds: {i}"));
        }
        linear = linear * dim + *i as usize;
    }
    Ok(linear)
}

pub fn index1(array: &Rc<RefCell<ArrayData>>, index: i64) -> Res<Value> {
    let a = array.borrow();
    if a.shape.len() != 1 {
        return fail("a[i] needs a one-dimensional array; use a.get(i, j, ...) for more dimensions");
    }
    if index < 0 || index as usize >= a.shape[0] {
        return fail(format!("index out of bounds: {index}"));
    }
    Ok(a.data[index as usize].clone())
}

fn accumulate(op: BinOp, items: impl Iterator<Item = Value>) -> Res<Value> {
    let mut iter = items;
    let mut acc = iter.next().expect("arrays are never empty");
    for item in iter {
        acc = eval_binary_builtin(op, acc, item)?;
    }
    Ok(acc)
}

fn ordered(pick_less: bool, items: &[Value]) -> Res<Value> {
    let mut best = items[0].clone();
    for item in &items[1..] {
        let better = eval_binary_builtin(if pick_less { BinOp::Lt } else { BinOp::Gt }, item.clone(), best.clone())?;
        if matches!(better, Value::Bool(true)) {
            best = item.clone();
        }
    }
    Ok(best)
}

/// Methods on an array receiver; `args` are already evaluated.
pub fn call_method(receiver: &Rc<RefCell<ArrayData>>, method: &str, args: Vec<Value>) -> Res<Value> {
    let a = receiver.borrow();
    match method {
        "shape" => Ok(list_of(a.shape.iter().map(|d| Value::Int(*d as i64)).collect())),
        "rank" => Ok(Value::Int(a.shape.len() as i64)),
        "size" | "length" | "count" => Ok(Value::Int(a.data.len() as i64)),
        "sum" => accumulate(BinOp::Add, a.data.iter().cloned()),
        "mean" => {
            let sum = accumulate(BinOp::Add, a.data.iter().cloned())?;
            let n = a.data.len();
            match sum {
                Value::Int(s) => Ok(Value::Float(s as f64 / n as f64)),
                Value::Float(s) => Ok(Value::Float(s / n as f64)),
                Value::F32(s) => Ok(Value::F32(s / n as f32)),
                other => fail(format!("mean isn't defined for '{other}' elements")),
            }
        }
        "min" => ordered(true, &a.data),
        "max" => ordered(false, &a.data),
        "to_list" => Ok(list_of(a.data.clone())),
        "get" => Ok(a.data[offset(&a.shape, &args)?].clone()),
        "set" => {
            drop(a);
            let (value, indices) = args.split_last().ok_or_else(|| RuntimeError::Error("set needs indices and a value".to_string()))?;
            let mut a = receiver.borrow_mut();
            let at = offset(&a.shape, indices)?;
            a.data[at] = value.clone();
            Ok(Value::Void)
        }
        "reshape" => {
            let shape = shape_arg(&args[0])?;
            if total(&shape) != a.data.len() {
                return fail(format!("cannot reshape an array of {} element(s) to {shape:?}", a.data.len()));
            }
            Ok(make(shape, a.data.clone()))
        }
        "transpose" => {
            if a.shape.len() != 2 {
                return fail("transpose needs a two-dimensional array");
            }
            let (r, c) = (a.shape[0], a.shape[1]);
            let mut data = Vec::with_capacity(r * c);
            for j in 0..c {
                for i in 0..r {
                    data.push(a.data[i * c + j].clone());
                }
            }
            Ok(make(vec![c, r], data))
        }
        "sum_axis" => {
            let Value::Int(axis) = &args[0] else { return fail("sum_axis expects an Int axis") };
            if a.shape.len() < 2 {
                return fail("sum_axis needs at least two dimensions");
            }
            if *axis < 0 || *axis as usize >= a.shape.len() {
                return fail(format!("axis {axis} is out of range for {} dimension(s)", a.shape.len()));
            }
            let axis = *axis as usize;
            let mut out_shape = a.shape.clone();
            out_shape.remove(axis);
            let mut data = Vec::with_capacity(total(&out_shape));
            for linear in 0..total(&out_shape) {
                let mut coords = coords_of(linear, &out_shape);
                coords.insert(axis, 0);
                let mut terms = Vec::new();
                for k in 0..a.shape[axis] {
                    coords[axis] = k;
                    let mut at = 0;
                    for (d, dim) in a.shape.iter().enumerate() {
                        at = at * dim + coords[d];
                    }
                    terms.push(a.data[at].clone());
                }
                data.push(accumulate(BinOp::Add, terms.into_iter())?);
            }
            Ok(make(out_shape, data))
        }
        "dot" => {
            let Value::Array(other) = &args[0] else { return fail("dot expects an array") };
            let b = other.borrow();
            if a.shape.len() != 1 || b.shape.len() != 1 || a.shape[0] != b.shape[0] {
                return fail("dot needs two one-dimensional arrays of the same length");
            }
            let products = a.data.iter().zip(b.data.iter()).map(|(x, y)| eval_binary_builtin(BinOp::Mul, x.clone(), y.clone())).collect::<Res<Vec<_>>>()?;
            accumulate(BinOp::Add, products.into_iter())
        }
        "matmul" => {
            let Value::Array(other) = &args[0] else { return fail("matmul expects an array") };
            let b = other.borrow();
            if a.shape.len() != 2 || b.shape.len() != 2 || a.shape[1] != b.shape[0] {
                return fail(format!("matmul needs (m, k) x (k, n) matrices, got {:?} and {:?}", a.shape, b.shape));
            }
            let (m, k, n) = (a.shape[0], a.shape[1], b.shape[1]);
            let mut data = Vec::with_capacity(m * n);
            for i in 0..m {
                for j in 0..n {
                    let mut terms = Vec::with_capacity(k);
                    for t in 0..k {
                        terms.push(eval_binary_builtin(BinOp::Mul, a.data[i * k + t].clone(), b.data[t * n + j].clone())?);
                    }
                    data.push(accumulate(BinOp::Add, terms.into_iter())?);
                }
            }
            Ok(make(vec![m, n], data))
        }
        other => fail(format!("Array has no method '{other}'")),
    }
}

pub fn display(array: &ArrayData) -> String {
    fn rec(a: &ArrayData, dim: usize, offset: usize, out: &mut String) {
        if dim == a.shape.len() {
            out.push_str(&a.data[offset].to_string());
            return;
        }
        let stride: usize = a.shape[dim + 1..].iter().product();
        out.push('[');
        for i in 0..a.shape[dim] {
            if i > 0 {
                out.push_str(", ");
            }
            rec(a, dim + 1, offset + i * stride, out);
        }
        out.push(']');
    }
    let mut out = String::new();
    rec(array, 0, 0, &mut out);
    out
}
