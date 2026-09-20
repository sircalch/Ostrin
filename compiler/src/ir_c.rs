//! Conservative C emission for the first IR-backed native functions.
//!
//! The emitter consumes the explicit CFG rather than walking HIR a second time.
//! SSA values become named C temporaries, branches become labels/gotos, and
//! phi nodes select the incoming value using the predecessor edge. The first
//! managed families supported here are `String`, scalar-element `List<T>`, and
//! the scalar-key/value core of `Map<K,V>`/`Set<T>`; their ownership markers
//! and native helpers are emitted directly into C while option-returning
//! lookups and larger aggregates retain the verified HIR/AST fallback.

use std::collections::{HashMap, HashSet};

use crate::ast::{BinOp, UnaryOp};
use crate::ir::{IrFunction, IrInstr, IrTerminator, ValueId};
use crate::types::Ty;

type Bail<T> = Result<T, ()>;
type Values = HashMap<ValueId, (String, Ty)>;

fn c_type(ty: &Ty) -> Bail<String> {
    Ok(match ty {
        Ty::Int => "int64_t".to_string(),
        Ty::Float => "double".to_string(),
        Ty::Float32 => "float".to_string(),
        Ty::Sized(kind) => kind.c_type().to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::String => "const char*".to_string(),
        Ty::List(element) if list_element_supported(element) => format!("List_{}*", mangle_scalar(element)),
        Ty::Map(key, value) if map_supported(key, value) => {
            format!("Map_{}_{}*", mangle_scalar(key), mangle_scalar(value))
        }
        Ty::Set(element) if set_supported(element) => format!("Set_{}*", mangle_scalar(element)),
        Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_supported(&args[0]) => {
            format!("Option_{}", mangle_scalar(&args[0]))
        }
        Ty::Void => "void".to_string(),
        _ => return Err(()),
    })
}

fn scalar(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_) | Ty::Bool | Ty::String | Ty::Void
    )
}

fn supported(ty: &Ty) -> bool {
    scalar(ty)
        || matches!(ty, Ty::List(element) if list_element_supported(element))
        || matches!(ty, Ty::Map(key, value) if map_supported(key, value))
        || matches!(ty, Ty::Set(element) if set_supported(element))
        || matches!(ty, Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_supported(&args[0]))
}

fn list_element_supported(ty: &Ty) -> bool {
    scalar(ty) && *ty != Ty::Void
}

fn map_supported(key: &Ty, value: &Ty) -> bool {
    list_element_supported(key) && list_element_supported(value)
}

fn set_supported(element: &Ty) -> bool {
    list_element_supported(element)
}

fn option_supported(element: &Ty) -> bool {
    matches!(element, Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_) | Ty::Bool)
}

fn option_type(element: &Ty) -> Ty {
    Ty::Applied("Option".to_string(), vec![element.clone()])
}

fn mangle_scalar(ty: &Ty) -> String {
    match ty {
        Ty::Int => "Int".to_string(),
        Ty::Float => "Float".to_string(),
        Ty::Float32 => "Float32".to_string(),
        Ty::Sized(kind) => kind.name().to_string(),
        Ty::Bool => "Bool".to_string(),
        Ty::String => "String".to_string(),
        _ => unreachable!("mangle_scalar only accepts scalar IR types"),
    }
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
                if !supported(&ty) {
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
        Ty::String => crate::codegen::c_string_literal(value),
        Ty::Void if value == "unit" => "(void)0".to_string(),
        _ => return Err(()),
    })
}

fn c_int_literal(value: i128) -> String {
    if value > i64::MAX as i128 {
        format!("{value}ULL")
    } else if value == i64::MIN as i128 {
        "(-9223372036854775807LL - 1)".to_string()
    } else if value < 0 {
        format!("(-{}LL)", -value)
    } else {
        format!("{value}LL")
    }
}

const OVERFLOW_ABORT: &str = "fprintf(stderr, \"runtime error: integer overflow\\n\"); exit(1);";

fn unary_code(op: UnaryOp, operand: &str, ty: &Ty) -> Bail<String> {
    match (op, ty) {
        (UnaryOp::Neg, Ty::Int | Ty::Float | Ty::Float32) => Ok(format!("(-{operand})")),
        (UnaryOp::Neg, Ty::Sized(kind)) if kind.is_signed() => Ok(format!(
            "({{ {c} __ostrin_ir_neg = {operand}; if (__ostrin_ir_neg == ({c}){min}) {{ {OVERFLOW_ABORT} }} ({c})(-(__int128)__ostrin_ir_neg); }})",
            c = kind.c_type(),
            min = c_int_literal(kind.min()),
        )),
        (UnaryOp::Not, Ty::Bool) => Ok(format!("(!{operand})")),
        _ => Err(()),
    }
}

fn sized_binary_code(
    op: BinOp,
    left: &str,
    right: &str,
    kind: crate::ast::IntKind,
) -> Bail<String> {
    let c = kind.c_type();
    let a = "__ostrin_ir_a";
    let b = "__ostrin_ir_b";
    let checked = |builtin: &str| {
        format!(
            "({{ {c} {a} = {left}; {c} {b} = {right}; {c} __ostrin_ir_result; if ({builtin}({a}, {b}, &__ostrin_ir_result)) {{ {OVERFLOW_ABORT} }} __ostrin_ir_result; }})"
        )
    };
    Ok(match op {
        BinOp::Add => checked("__builtin_add_overflow"),
        BinOp::Sub => checked("__builtin_sub_overflow"),
        BinOp::Mul => checked("__builtin_mul_overflow"),
        BinOp::Div => format!(
            "({{ {c} {a} = {left}; {c} {b} = {right}; if ({b} == 0) {{ fprintf(stderr, \"runtime error: division by zero\\n\"); exit(1); }} __int128 __ostrin_ir_q = (__int128){a} / (__int128){b}; if (__ostrin_ir_q < (__int128){min} || __ostrin_ir_q > (__int128){max}) {{ {OVERFLOW_ABORT} }} ({c})__ostrin_ir_q; }})",
            min = c_int_literal(kind.min()),
            max = c_int_literal(kind.max()),
        ),
        BinOp::Eq => format!("(({left}) == ({right}))"),
        BinOp::NotEq => format!("(({left}) != ({right}))"),
        BinOp::Lt => format!("(({left}) < ({right}))"),
        BinOp::Gt => format!("(({left}) > ({right}))"),
        BinOp::LtEq => format!("(({left}) <= ({right}))"),
        BinOp::GtEq => format!("(({left}) >= ({right}))"),
        BinOp::And | BinOp::Or => return Err(()),
    })
}

fn binary_code(
    op: BinOp,
    left: &str,
    left_ty: &Ty,
    right: &str,
    right_ty: &Ty,
    ty: &Ty,
) -> Bail<String> {
    if let (Ty::Sized(left_kind), Ty::Sized(right_kind)) = (left_ty, right_ty) {
        if left_kind != right_kind {
            return Err(());
        }
        return sized_binary_code(op, left, right, *left_kind);
    }
    if *ty == Ty::String && op == BinOp::Add && *left_ty == Ty::String && *right_ty == Ty::String {
        return Ok(format!("ostrin_str_concat({left}, {right})"));
    }
    if (*left_ty == Ty::String || *right_ty == Ty::String)
        && matches!(op, BinOp::Eq | BinOp::NotEq)
        && *left_ty == *right_ty
    {
        let comparison = if op == BinOp::Eq { "== 0" } else { "!= 0" };
        return Ok(format!("(strcmp({left}, {right}) {comparison})"));
    }
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
        Ty::String => format!("printf(\"%s\\n\", {value})"),
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
            if !supported(ty) || *ty == Ty::Void {
                return Err(());
            }
            out.push_str(&format!("    {} = {name};\n", value_name(*dst)));
        }
        IrInstr::Global { dst, name, ty } => {
            let Ty::Applied(option_name, args) = ty else { return Err(()) };
            if name != "None" || option_name != "Option" || args.len() != 1 || !option_supported(&args[0]) {
                return Err(());
            }
            let c_name = format!("Option_{}", mangle_scalar(&args[0]));
            out.push_str(&format!(
                "    {} = (({c_name}){{ .has = false }});\n",
                value_name(*dst)
            ));
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
            if !supported(ty) || *ty == Ty::Void {
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
                &left_ty,
                &value_code(values, *right)?,
                &right_ty,
                ty,
            )?;
            out.push_str(&format!("    {} = {code};\n", value_name(*dst)));
        }
        IrInstr::Aggregate { dst, kind, fields, ty } => match ty {
            Ty::List(element) => {
                if !list_element_supported(element) || (kind != "collection" && !kind.starts_with("empty_")) {
                    return Err(());
                }
                let element_c = c_type(element)?;
                let list_name = format!("List_{}", mangle_scalar(element));
                let values = fields
                    .iter()
                    .map(|value| {
                        if value_ty(values, *value)? != **element {
                            return Err(());
                        }
                        value_code(values, *value)
                    })
                    .collect::<Bail<Vec<_>>>()?;
                let source = if values.is_empty() {
                    "NULL".to_string()
                } else {
                    format!("({element_c}[]){{ {} }}", values.join(", "))
                };
                out.push_str(&format!(
                    "    {} = {list_name}_new_from_array({source}, {});\n",
                    value_name(*dst),
                    values.len()
                ));
            }
            Ty::Set(element) => {
                if !set_supported(element) || (kind != "collection" && !kind.starts_with("empty_")) {
                    return Err(());
                }
                let set_name = format!("Set_{}", mangle_scalar(element));
                out.push_str(&format!("    {} = {set_name}_new();\n", value_name(*dst)));
                for value in fields {
                    if value_ty(values, *value)? != **element {
                        return Err(());
                    }
                    out.push_str(&format!(
                        "    {set_name}_add({}, {});\n",
                        value_name(*dst),
                        value_code(values, *value)?
                    ));
                }
            }
            Ty::Map(key, value) => {
                if !map_supported(key, value) || (kind != "map" && !kind.starts_with("empty_")) || fields.len() % 2 != 0 {
                    return Err(());
                }
                let map_name = format!("Map_{}_{}", mangle_scalar(key), mangle_scalar(value));
                out.push_str(&format!("    {} = {map_name}_new();\n", value_name(*dst)));
                for pair in fields.chunks_exact(2) {
                    if value_ty(values, pair[0])? != **key || value_ty(values, pair[1])? != **value {
                        return Err(());
                    }
                    out.push_str(&format!(
                        "    {map_name}_set({}, {}, {});\n",
                        value_name(*dst),
                        value_code(values, pair[0])?,
                        value_code(values, pair[1])?
                    ));
                }
            }
            _ => return Err(()),
        },
        IrInstr::Index { dst, object, index, ty } => {
            let Ty::List(element) = value_ty(values, *object)? else { return Err(()) };
            if !list_element_supported(&element) || value_ty(values, *index)? != Ty::Int || *ty != *element {
                return Err(());
            }
            let list_name = format!("List_{}", mangle_scalar(&element));
            out.push_str(&format!(
                "    {} = {list_name}_get({}, {});\n",
                value_name(*dst),
                value_code(values, *object)?,
                value_code(values, *index)?
            ));
        }
        IrInstr::MethodCall {
            dst,
            method,
            receiver,
            args,
            ty,
        } => {
            let receiver_ty = value_ty(values, *receiver)?;
            let receiver = value_code(values, *receiver)?;
            let call = match receiver_ty {
                Ty::List(element) if list_element_supported(&element) => {
                    let list_name = format!("List_{}", mangle_scalar(&element));
                    match method.as_str() {
                        "length" | "count" if args.is_empty() && *ty == Ty::Int => {
                            format!("{list_name}_length({receiver})")
                        }
                        "push" if args.len() == 1 && *ty == Ty::Void && value_ty(values, args[0])? == *element => {
                            format!("{list_name}_push({receiver}, {})", value_code(values, args[0])?)
                        }
                        "remove_at" if args.len() == 1 && *ty == *element && value_ty(values, args[0])? == Ty::Int => {
                            format!("{list_name}_remove_at({receiver}, {})", value_code(values, args[0])?)
                        }
                        _ => return Err(()),
                    }
                }
                Ty::Map(key, value) if map_supported(&key, &value) => {
                    let map_name = format!("Map_{}_{}", mangle_scalar(&key), mangle_scalar(&value));
                    match method.as_str() {
                        "contains_key" if args.len() == 1 && *ty == Ty::Bool && value_ty(values, args[0])? == *key => {
                            format!("{map_name}_contains_key({receiver}, {})", value_code(values, args[0])?)
                        }
                        "get" | "remove"
                            if args.len() == 1
                                && option_supported(value.as_ref())
                                && *ty == option_type(value.as_ref())
                                && value_ty(values, args[0])? == *key =>
                        {
                            format!("{map_name}_{method}({receiver}, {})", value_code(values, args[0])?)
                        }
                        "count" if args.is_empty() && *ty == Ty::Int => format!("{map_name}_count({receiver})"),
                        "set" if args.len() == 2 && *ty == Ty::Void && value_ty(values, args[0])? == *key && value_ty(values, args[1])? == *value => {
                            format!("{map_name}_set({receiver}, {}, {})", value_code(values, args[0])?, value_code(values, args[1])?)
                        }
                        "keys" if args.is_empty() && *ty == Ty::List(key.clone()) => {
                            format!("{map_name}_keys({receiver})")
                        }
                        "values" if args.is_empty() && *ty == Ty::List(value.clone()) => {
                            format!("{map_name}_values({receiver})")
                        }
                        _ => return Err(()),
                    }
                }
                Ty::Applied(name, option_args)
                    if name == "Option" && option_args.len() == 1 && option_supported(&option_args[0]) =>
                {
                    let inner = option_args[0].clone();
                    let option_name = format!("Option_{}", mangle_scalar(&inner));
                    match method.as_str() {
                        "is_some" if args.is_empty() && *ty == Ty::Bool => format!("({receiver}).has"),
                        "is_none" if args.is_empty() && *ty == Ty::Bool => format!("!({receiver}).has"),
                        "unwrap" if args.is_empty() && *ty == inner => format!(
                            "({{ {option_name} __ostrin_option = {receiver}; if (!__ostrin_option.has) {{ fprintf(stderr, \"ostrin: unwrap on None\\n\"); exit(1); }} __ostrin_option.value; }})"
                        ),
                        "unwrap_or"
                            if args.len() == 1
                                && *ty == inner
                                && value_ty(values, args[0])? == inner =>
                        {
                            format!(
                                "({{ {option_name} __ostrin_option = {receiver}; __ostrin_option.has ? __ostrin_option.value : {}; }})",
                                value_code(values, args[0])?
                            )
                        }
                        _ => return Err(()),
                    }
                }
                Ty::Set(element) if set_supported(&element) => {
                    let set_name = format!("Set_{}", mangle_scalar(&element));
                    match method.as_str() {
                        "contains" if args.len() == 1 && *ty == Ty::Bool && value_ty(values, args[0])? == *element => {
                            format!("{set_name}_contains({receiver}, {})", value_code(values, args[0])?)
                        }
                        "add" | "remove" if args.len() == 1 && *ty == Ty::Void && value_ty(values, args[0])? == *element => {
                            format!("{set_name}_{method}({receiver}, {})", value_code(values, args[0])?)
                        }
                        "count" if args.is_empty() && *ty == Ty::Int => format!("{set_name}_count({receiver})"),
                        _ => return Err(()),
                    }
                }
                _ => return Err(()),
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
        IrInstr::Call {
            dst,
            callee,
            args,
            ty,
        } => {
            if !supported(ty) {
                return Err(());
            }
            let codes = args
                .iter()
                .map(|value| value_code(values, *value))
                .collect::<Bail<Vec<_>>>()?;
            let call = if callee == "Some" && args.len() == 1 {
                let inner = value_ty(values, args[0])?;
                if !option_supported(&inner) || *ty != option_type(&inner) {
                    return Err(());
                }
                let option_name = format!("Option_{}", mangle_scalar(&inner));
                format!("(({option_name}){{ .has = true, .value = {} }})", codes[0])
            } else if callee == "print" && args.len() == 1 {
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
                    if !supported(&value_ty(values, *value)?) {
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
            if !supported(ty) || *ty == Ty::Void || incoming.is_empty() {
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
        IrInstr::Retain { value } => {
            if !matches!(value_ty(values, *value)?, Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_)) {
                return Err(());
            }
            out.push_str(&format!("    ostrin_retain((void*){});\n", value_code(values, *value)?));
        }
        IrInstr::Release { value } => {
            if !matches!(value_ty(values, *value)?, Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_)) {
                return Err(());
            }
            out.push_str(&format!("    ostrin_release((void*){});\n", value_code(values, *value)?));
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

/// Emits an IR function when all of its values use a supported scalar or
/// collection representation and its CFG can be represented with ordinary C
/// labels and gotos.
pub fn generate(function: &IrFunction, known_functions: &HashSet<String>) -> Option<String> {
    if function.entry >= function.blocks.len()
        || function
            .params
            .iter()
        .any(|(_, ty)| !supported(ty) || *ty == Ty::Void)
        || !supported(&function.ret)
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
