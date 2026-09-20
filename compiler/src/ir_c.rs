//! Conservative C emission for the first IR-backed native functions.
//!
//! The emitter is intentionally limited to scalar values, but it consumes the
//! explicit CFG rather than walking HIR a second time. SSA values become named
//! C temporaries, branches become labels/gotos, and phi nodes select the
//! incoming value using the predecessor edge. Unsupported instructions,
//! managed values and checked arithmetic families return `None`, preserving
//! the verified HIR/AST fallback.

use std::collections::{HashMap, HashSet};

use crate::ast::{BinOp, UnaryOp};
use crate::ir::{IrFunction, IrInstr, IrTerminator, ValueId};
use crate::types::Ty;

type Bail<T> = Result<T, ()>;
type Values = HashMap<ValueId, (String, Ty)>;

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

fn value_code(values: &Values, value: ValueId) -> Bail<String> {
    values.get(&value).map(|(code, _)| code.clone()).ok_or(())
}

fn value_ty(values: &Values, value: ValueId) -> Bail<Ty> {
    values.get(&value).map(|(_, ty)| ty.clone()).ok_or(())
}

fn defined_value(instruction: &IrInstr) -> Option<(ValueId, Ty)> {
    match instruction {
        IrInstr::Param { dst, ty, .. }
        | IrInstr::Const { dst, ty, .. }
        | IrInstr::Global { dst, ty, .. }
        | IrInstr::Move { dst, ty, .. }
        | IrInstr::Unary { dst, ty, .. }
        | IrInstr::Binary { dst, ty, .. }
        | IrInstr::Field { dst, ty, .. }
        | IrInstr::Index { dst, ty, .. }
        | IrInstr::Aggregate { dst, ty, .. }
        | IrInstr::IterInit { dst, ty, .. }
        | IrInstr::IterNext { dst, ty, .. }
        | IrInstr::PatternBind { dst, ty, .. }
        | IrInstr::TryValue { dst, ty, .. }
        | IrInstr::TryError { dst, ty, .. }
        | IrInstr::Spawn { dst, ty, .. }
        | IrInstr::ChannelNew { dst, ty, .. }
        | IrInstr::ChannelReceive { dst, ty, .. }
        | IrInstr::TaskJoin { dst, ty, .. }
        | IrInstr::Phi { dst, ty, .. } => Some((*dst, ty.clone())),
        IrInstr::PatternTest { dst, .. }
        | IrInstr::TryCheck { dst, .. }
        | IrInstr::IterHasNext { dst, .. } => Some((*dst, Ty::Bool)),
        IrInstr::Call {
            dst: Some(dst), ty, ..
        }
        | IrInstr::MethodCall {
            dst: Some(dst), ty, ..
        }
        | IrInstr::Opaque {
            dst: Some(dst), ty, ..
        } => Some((*dst, ty.clone())),
        IrInstr::StoreLocal { .. }
        | IrInstr::Call { dst: None, .. }
        | IrInstr::MethodCall { dst: None, .. }
        | IrInstr::Opaque { dst: None, .. }
        | IrInstr::ChannelSend { .. }
        | IrInstr::ChannelClose { .. }
        | IrInstr::Retain { .. }
        | IrInstr::Release { .. } => None,
    }
}

fn collect_values(function: &IrFunction) -> Bail<Values> {
    let mut values = HashMap::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Some((value, ty)) = defined_value(instruction) {
                if !scalar(&ty) {
                    return Err(());
                }
                if values.insert(value, (value_name(value), ty)).is_some() {
                    return Err(());
                }
            }
        }
    }
    Ok(values)
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
        Ty::Sized(kind) if kind.is_signed() => {
            format!("printf(\"%lld\\n\", (long long)({value}))")
        }
        Ty::Sized(_) => format!("printf(\"%llu\\n\", (unsigned long long)({value}))"),
        Ty::Bool => format!("printf(\"%s\\n\", (({value}) ? \"true\" : \"false\"))"),
        _ => return Err(()),
    })
}

fn emit_instruction(
    instruction: &IrInstr,
    values: &Values,
    known_functions: &HashSet<String>,
    out: &mut String,
) -> Bail<()> {
    match instruction {
        IrInstr::Param { dst, name, ty, .. } => {
            if !scalar(ty) || *ty == Ty::Void {
                return Err(());
            }
            out.push_str(&format!("    {} = {name};\n", value_name(*dst)));
        }
        IrInstr::Const { dst, value, ty } => {
            if !scalar(ty) {
                return Err(());
            }
            if *ty != Ty::Void {
                out.push_str(&format!(
                    "    {} = {};\n",
                    value_name(*dst),
                    const_code(value, ty)?
                ));
            }
        }
        IrInstr::Move { dst, source, ty } => {
            if !scalar(ty) || *ty == Ty::Void {
                return Err(());
            }
            out.push_str(&format!(
                "    {} = {};\n",
                value_name(*dst),
                value_code(values, *source)?
            ));
        }
        IrInstr::StoreLocal { .. } => {}
        IrInstr::Unary {
            dst,
            op,
            operand,
            ty,
        } => {
            if !scalar(ty) || *ty == Ty::Void {
                return Err(());
            }
            let operand_ty = value_ty(values, *operand)?;
            if !scalar(&operand_ty) {
                return Err(());
            }
            let code = unary_code(*op, &value_code(values, *operand)?, &operand_ty)?;
            out.push_str(&format!("    {} = {code};\n", value_name(*dst)));
        }
        IrInstr::Binary {
            dst,
            op,
            left,
            right,
            ty,
        } => {
            if !scalar(ty) || *ty == Ty::Void {
                return Err(());
            }
            let left_ty = value_ty(values, *left)?;
            let right_ty = value_ty(values, *right)?;
            if !scalar(&left_ty) || !scalar(&right_ty) {
                return Err(());
            }
            let code = binary_code(
                *op,
                &value_code(values, *left)?,
                &value_code(values, *right)?,
                ty,
            )?;
            out.push_str(&format!("    {} = {code};\n", value_name(*dst)));
        }
        IrInstr::Call {
            dst,
            callee,
            args,
            ty,
        } => {
            if !scalar(ty) {
                return Err(());
            }
            let codes = args
                .iter()
                .map(|value| value_code(values, *value))
                .collect::<Bail<Vec<_>>>()?;
            let call = if callee == "print" && args.len() == 1 {
                if *ty != Ty::Void {
                    return Err(());
                }
                let arg_ty = value_ty(values, args[0])?;
                if !scalar(&arg_ty) {
                    return Err(());
                }
                print_code(&codes[0], &arg_ty)?
            } else {
                if !known_functions.contains(callee) {
                    return Err(());
                }
                for value in args {
                    if !scalar(&value_ty(values, *value)?) {
                        return Err(());
                    }
                }
                format!(
                    "{}({})",
                    crate::codegen::c_function_name(callee),
                    codes.join(", ")
                )
            };
            if let Some(dst) = dst {
                if *ty == Ty::Void {
                    return Err(());
                }
                out.push_str(&format!("    {} = {call};\n", value_name(*dst)));
            } else {
                if *ty != Ty::Void {
                    return Err(());
                }
                out.push_str(&format!("    {call};\n"));
            }
        }
        IrInstr::Phi { dst, incoming, ty } => {
            if !scalar(ty) || *ty == Ty::Void || incoming.is_empty() {
                return Err(());
            }
            for (index, (predecessor, value)) in incoming.iter().enumerate() {
                let incoming_ty = value_ty(values, *value)?;
                if incoming_ty != *ty {
                    return Err(());
                }
                let keyword = if index == 0 { "if" } else { "else if" };
                out.push_str(&format!(
                    "    {keyword} (__ostrin_ir_pred == {predecessor}) {{ {} = {}; }}\n",
                    value_name(*dst),
                    value_code(values, *value)?
                ));
            }
            out.push_str("    else { abort(); }\n");
        }
        _ => return Err(()),
    }
    Ok(())
}

fn block_label(block: usize) -> String {
    format!("__ostrin_ir_bb{block}")
}

fn emit_terminator(
    function: &IrFunction,
    block: usize,
    terminator: &IrTerminator,
    values: &Values,
    out: &mut String,
) -> Bail<()> {
    let valid_target = |target: usize| target < function.blocks.len();
    match terminator {
        IrTerminator::Goto(target) => {
            if !valid_target(*target) {
                return Err(());
            }
            out.push_str(&format!(
                "    __ostrin_ir_pred = {block}; goto {};\n",
                block_label(*target)
            ));
        }
        IrTerminator::Branch {
            condition,
            then_block,
            else_block,
        } => {
            if !valid_target(*then_block) || !valid_target(*else_block) {
                return Err(());
            }
            if value_ty(values, *condition)? != Ty::Bool {
                return Err(());
            }
            let condition = value_code(values, *condition)?;
            out.push_str(&format!(
                "    if ({condition}) {{ __ostrin_ir_pred = {block}; goto {}; }} else {{ __ostrin_ir_pred = {block}; goto {}; }}\n",
                block_label(*then_block),
                block_label(*else_block)
            ));
        }
        IrTerminator::Return(value) => match value {
            Some(value) => {
                let ty = value_ty(values, *value)?;
                if ty != function.ret {
                    return Err(());
                }
                if ty == Ty::Void {
                    out.push_str("    return;\n");
                } else {
                    out.push_str(&format!("    return {};\n", value_code(values, *value)?));
                }
            }
            None if function.ret == Ty::Void => out.push_str("    return;\n"),
            None => return Err(()),
        },
        IrTerminator::Unreachable => out.push_str("    abort();\n"),
        IrTerminator::RegionReturn(_) => return Err(()),
    }
    Ok(())
}

/// Emits an IR function when all of its values are scalar and its CFG can be
/// represented with ordinary C labels and gotos.
pub fn generate(function: &IrFunction, known_functions: &HashSet<String>) -> Option<String> {
    if function.entry >= function.blocks.len()
        || function
            .params
            .iter()
            .any(|(_, ty)| !scalar(ty) || *ty == Ty::Void)
        || !scalar(&function.ret)
    {
        return None;
    }
    if function
        .blocks
        .iter()
        .enumerate()
        .any(|(index, block)| block.id != index || block.terminator.is_none())
    {
        return None;
    }

    let values = collect_values(function).ok()?;
    let mut out = String::new();
    out.push_str("    int __ostrin_ir_pred = -1;\n");
    let mut declarations: Vec<(ValueId, Ty)> = values
        .iter()
        .filter_map(|(value, (_, ty))| (*ty != Ty::Void).then_some((*value, ty.clone())))
        .collect();
    declarations.sort_by_key(|(value, _)| *value);
    for (value, ty) in declarations {
        out.push_str(&format!(
            "    {} {};\n",
            c_type(&ty).ok()?,
            value_name(value)
        ));
    }
    out.push_str(&format!("    goto {};\n", block_label(function.entry)));

    for block in &function.blocks {
        out.push_str(&format!("{}:\n", block_label(block.id)));
        for instruction in &block.instructions {
            emit_instruction(instruction, &values, known_functions, &mut out).ok()?;
        }
        emit_terminator(
            function,
            block.id,
            block.terminator.as_ref()?,
            &values,
            &mut out,
        )
        .ok()?;
    }
    // Every legal edge above terminates, but keep the C function well-formed
    // even if a future IR terminator gains a fall-through representation.
    out.push_str("    abort();\n");
    Some(out)
}
