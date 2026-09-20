//! Conservative C emission for the first IR-backed native functions.
//!
//! This emitter intentionally accepts only straight-line scalar functions. It
//! is a real IR consumer (not a second HIR walker): SSA values become named C
//! temporaries, calls are emitted from IR call nodes, and unsupported control
//! flow or managed values fall back to the existing HIR/AST path.

use std::collections::{HashMap, HashSet};

use crate::ast::{BinOp, UnaryOp};
use crate::ir::{IrFunction, IrInstr, IrTerminator, ValueId};
use crate::types::Ty;

type Bail<T> = Result<T, ()>;

fn c_type(ty: &Ty) -> Bail<&'static str> {
    Ok(match ty {
        Ty::Int => "int64_t",
        Ty::Float => "double",
        Ty::Float32 => "float",
        Ty::Sized(kind) => kind.c_type(),
        Ty::Bool => "bool",
        Ty::Void => "void",
        _ => return Err(()),
    })
}

fn scalar(ty: &Ty) -> bool {
    // Fixed-width integers keep checked overflow and division semantics in
    // the HIR/AST emitters until the IR has checked arithmetic intrinsics.
    matches!(ty, Ty::Int | Ty::Float | Ty::Float32 | Ty::Bool | Ty::Void)
}

fn value_name(value: ValueId) -> String {
    format!("__ir_v{value}")
}

fn value_code(values: &HashMap<ValueId, (String, Ty)>, value: ValueId) -> Bail<String> {
    values.get(&value).map(|(code, _)| code.clone()).ok_or(())
}

fn value_ty(values: &HashMap<ValueId, (String, Ty)>, value: ValueId) -> Bail<Ty> {
    values.get(&value).map(|(_, ty)| ty.clone()).ok_or(())
}

fn const_code(value: &str, ty: &Ty) -> Bail<String> {
    Ok(match ty {
        Ty::Int => format!("INT64_C({value})"),
        Ty::Float => value.to_string(),
        Ty::Float32 => {
            let value = value.strip_suffix("f32").unwrap_or(value);
            format!("((float)({value}))")
        }
        Ty::Sized(kind) => {
            let value = value.strip_suffix(kind.name()).unwrap_or(value);
            format!("(({}){value})", kind.c_type())
        }
        Ty::Bool => value.to_string(),
        Ty::Void if value == "unit" => "(void)0".to_string(),
        _ => return Err(()),
    })
}

fn unary_code(op: UnaryOp, operand: &str, ty: &Ty) -> Bail<String> {
    match (op, ty) {
        (UnaryOp::Neg, Ty::Int | Ty::Float | Ty::Float32) => Ok(format!("(-{operand})")),
        (UnaryOp::Neg, Ty::Sized(kind)) if kind.is_signed() => Ok(format!("(-{operand})")),
        (UnaryOp::Not, Ty::Bool) => Ok(format!("(!{operand})")),
        _ => Err(()),
    }
}

fn binary_code(op: BinOp, left: &str, right: &str, ty: &Ty) -> Bail<String> {
    if *ty == Ty::Int && op == BinOp::Div {
        return Ok(format!("ostrin_idiv({left}, {right})"));
    }
    let symbol = match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::LtEq => "<=",
        BinOp::GtEq => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    };
    Ok(format!("(({left}) {symbol} ({right}))"))
}

fn print_code(value: &str, ty: &Ty) -> Bail<String> {
    Ok(match ty {
        Ty::Int => format!("printf(\"%lld\\n\", (long long)({value}))"),
        Ty::Float => format!("ostrin_print_float({value})"),
        Ty::Float32 => format!("ostrin_print_single({value})"),
        Ty::Sized(kind) if kind.is_signed() => format!("printf(\"%lld\\n\", (long long)({value}))"),
        Ty::Sized(_) => format!("printf(\"%llu\\n\", (unsigned long long)({value}))"),
        Ty::Bool => format!("printf(\"%s\\n\", (({value}) ? \"true\" : \"false\"))"),
        _ => return Err(()),
    })
}

/// Emits an IR function when it is a single scalar basic block.
pub fn generate(function: &IrFunction, known_functions: &HashSet<String>) -> Option<String> {
    if function.blocks.len() != 1
        || function.params.iter().any(|(_, ty)| !scalar(ty))
        || !scalar(&function.ret)
    {
        return None;
    }
    let block = function.blocks.first()?;
    let mut values: HashMap<ValueId, (String, Ty)> = HashMap::new();
    let mut out = String::new();

    for instruction in &block.instructions {
        match instruction {
            IrInstr::Param { dst, name, ty, .. } => {
                if !scalar(ty) {
                    return None;
                }
                values.insert(*dst, (name.clone(), ty.clone()));
            }
            IrInstr::Const { dst, value, ty } => {
                if !scalar(ty) {
                    return None;
                }
                let code = const_code(value, ty).ok()?;
                if *ty == Ty::Void {
                    values.insert(*dst, (code, ty.clone()));
                } else {
                    let cty = c_type(ty).ok()?;
                    let name = value_name(*dst);
                    out.push_str(&format!("    {cty} {name} = {code};\n"));
                    values.insert(*dst, (name, ty.clone()));
                }
            }
            IrInstr::Move { dst, source, ty } => {
                if !scalar(ty) {
                    return None;
                }
                let source_code = value_code(&values, *source).ok()?;
                let cty = c_type(ty).ok()?;
                let name = value_name(*dst);
                out.push_str(&format!("    {cty} {name} = {source_code};\n"));
                values.insert(*dst, (name, ty.clone()));
            }
            IrInstr::StoreLocal { .. } => {}
            IrInstr::Unary {
                dst,
                op,
                operand,
                ty,
            } => {
                if !scalar(ty) {
                    return None;
                }
                let operand_code = value_code(&values, *operand).ok()?;
                let operand_ty = value_ty(&values, *operand).ok()?;
                if !scalar(&operand_ty) {
                    return None;
                }
                let code = unary_code(*op, &operand_code, &operand_ty).ok()?;
                let cty = c_type(ty).ok()?;
                let name = value_name(*dst);
                out.push_str(&format!("    {cty} {name} = {code};\n"));
                values.insert(*dst, (name, ty.clone()));
            }
            IrInstr::Binary {
                dst,
                op,
                left,
                right,
                ty,
            } => {
                if !scalar(ty) {
                    return None;
                }
                let left_code = value_code(&values, *left).ok()?;
                let right_code = value_code(&values, *right).ok()?;
                if !scalar(&value_ty(&values, *left).ok()?)
                    || !scalar(&value_ty(&values, *right).ok()?)
                {
                    return None;
                }
                let code = binary_code(*op, &left_code, &right_code, ty).ok()?;
                let cty = c_type(ty).ok()?;
                let name = value_name(*dst);
                out.push_str(&format!("    {cty} {name} = {code};\n"));
                values.insert(*dst, (name, ty.clone()));
            }
            IrInstr::Call {
                dst,
                callee,
                args,
                ty,
            } => {
                if !scalar(ty) {
                    return None;
                }
                let codes = args
                    .iter()
                    .map(|value| value_code(&values, *value))
                    .collect::<Bail<Vec<_>>>()
                    .ok()?;
                let call = if callee == "print" && args.len() == 1 {
                    let arg_ty = value_ty(&values, args[0]).ok()?;
                    if !scalar(&arg_ty) {
                        return None;
                    }
                    print_code(&codes[0], &arg_ty).ok()?
                } else {
                    if !known_functions.contains(callee) {
                        return None;
                    }
                    for value in args {
                        if !scalar(&value_ty(&values, *value).ok()?) {
                            return None;
                        }
                    }
                    if args.iter().any(|value| !values.contains_key(value)) {
                        return None;
                    }
                    format!(
                        "{}({})",
                        crate::codegen::c_function_name(callee),
                        codes.join(", ")
                    )
                };
                if let Some(dst) = dst {
                    if *ty == Ty::Void {
                        return None;
                    }
                    let cty = c_type(ty).ok()?;
                    let name = value_name(*dst);
                    out.push_str(&format!("    {cty} {name} = {call};\n"));
                    values.insert(*dst, (name, ty.clone()));
                } else {
                    out.push_str(&format!("    {call};\n"));
                }
            }
            _ => return None,
        }
    }

    match block.terminator.as_ref()? {
        IrTerminator::Return(value) => {
            if let Some(value) = value {
                let ty = value_ty(&values, *value).ok()?;
                if ty == Ty::Void {
                    out.push_str("    return;\n");
                } else {
                    out.push_str(&format!(
                        "    return {};\n",
                        value_code(&values, *value).ok()?
                    ));
                }
            } else {
                out.push_str("    return;\n");
            }
        }
        _ => return None,
    }
    Some(out)
}
