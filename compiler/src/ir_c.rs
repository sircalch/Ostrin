//! Conservative C emission for the first IR-backed native functions.
//!
//! The emitter consumes the explicit CFG rather than walking HIR a second time.
//! SSA values become named C temporaries, branches become labels/gotos, and
//! phi nodes select the incoming value using the predecessor edge. The first
//! managed families supported here are `String`, scalar-element `List<T>`
//! (including the `List<String>` values produced by `String.split()`/`lines()`),
//! and the scalar-key/value core of `Map<K,V>`/`Set<T>`, plus scalar-payload
//! `Result<T,E>` values such as `String.to_int()`/`to_float()`; wrappers can
//! now compose over scalar collections and over other `Option`/`Result` values
//! with recursive ownership markers while larger aggregates retain the
//! verified HIR/AST fallback.

use std::collections::{HashMap, HashSet};

use crate::ast::{BinOp, UnaryOp};
use crate::ir::{IrFunction, IrInstr, IrTerminator, ValueId};
use crate::types::Ty;

type Bail<T> = Result<T, ()>;
type Values = HashMap<ValueId, (String, Ty)>;
pub type RecordFields = HashMap<String, Vec<String>>;

fn c_type(ty: &Ty, records: &RecordFields) -> Bail<String> {
    Ok(match ty {
        Ty::Int => "int64_t".to_string(),
        Ty::Float => "double".to_string(),
        Ty::Float32 => "float".to_string(),
        Ty::Sized(kind) => kind.c_type().to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::String => "const char*".to_string(),
        Ty::List(element) if list_supported(element, records) => {
            format!("List_{}*", mangle_option_payload(element, records))
        }
        Ty::Map(key, value) if map_supported(key, value) => {
            format!("Map_{}_{}*", mangle_scalar(key), mangle_scalar(value))
        }
        Ty::Set(element) if set_supported(element) => format!("Set_{}*", mangle_scalar(element)),
        Ty::Named(name) if records.contains_key(name) => format!("{name}*"),
        Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_supported(&args[0], records) => {
            format!("Option_{}", mangle_option_payload(&args[0], records))
        }
        Ty::Applied(name, args)
            if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records) =>
        {
            format!(
                "Result_{}_{}",
                mangle_result_payload(&args[0], records),
                mangle_result_payload(&args[1], records)
            )
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

fn supported(ty: &Ty, records: &RecordFields) -> bool {
    scalar(ty)
        || matches!(ty, Ty::Named(name) if records.contains_key(name))
        || matches!(ty, Ty::List(element) if list_supported(element, records))
        || matches!(ty, Ty::Map(key, value) if map_supported(key, value))
        || matches!(ty, Ty::Set(element) if set_supported(element))
        || matches!(ty, Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_supported(&args[0], records))
        || matches!(ty, Ty::Applied(name, args) if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records))
}

fn list_element_supported(ty: &Ty) -> bool {
    scalar(ty) && *ty != Ty::Void
}

fn list_supported(ty: &Ty, records: &RecordFields) -> bool {
    list_element_supported(ty) || matches!(ty, Ty::Named(name) if records.contains_key(name))
}

fn map_supported(key: &Ty, value: &Ty) -> bool {
    list_element_supported(key) && list_element_supported(value)
}

fn set_supported(element: &Ty) -> bool {
    list_element_supported(element)
}

fn option_supported(element: &Ty, records: &RecordFields) -> bool {
    matches!(element, Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_) | Ty::Bool | Ty::String)
        || matches!(element, Ty::List(inner) if list_supported(inner, records))
        || matches!(element, Ty::Map(key, value) if map_supported(key, value))
        || matches!(element, Ty::Set(inner) if set_supported(inner))
        || matches!(element, Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_supported(&args[0], records))
        || matches!(element, Ty::Applied(name, args) if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records))
        || matches!(element, Ty::Named(name) if records.contains_key(name))
}

fn option_managed_payload(element: &Ty, records: &RecordFields) -> bool {
    managed_payload(element, records)
}

fn result_payload_supported(ty: &Ty, records: &RecordFields) -> bool {
    (scalar(ty) && *ty != Ty::Void)
        || matches!(ty, Ty::List(inner) if list_supported(inner, records))
        || matches!(ty, Ty::Map(key, value) if map_supported(key, value))
        || matches!(ty, Ty::Set(inner) if set_supported(inner))
        || matches!(ty, Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_supported(&args[0], records))
        || matches!(ty, Ty::Applied(name, args) if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records))
        || matches!(ty, Ty::Named(name) if records.contains_key(name))
}

fn result_supported(ok: &Ty, err: &Ty, records: &RecordFields) -> bool {
    result_payload_supported(ok, records) && result_payload_supported(err, records)
}

fn result_managed_payload(ty: &Ty, records: &RecordFields) -> bool {
    managed_payload(ty, records)
}

fn managed_payload(ty: &Ty, records: &RecordFields) -> bool {
    match ty {
        Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => true,
        Ty::Named(name) => records.contains_key(name),
        Ty::Applied(name, args) if name == "Option" && args.len() == 1 => {
            option_supported(&args[0], records) && managed_payload(&args[0], records)
        }
        Ty::Applied(name, args) if name == "Result" && args.len() == 2 => {
            result_supported(&args[0], &args[1], records)
                && (managed_payload(&args[0], records) || managed_payload(&args[1], records))
        }
        _ => false,
    }
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

fn mangle_option_payload(ty: &Ty, records: &RecordFields) -> String {
    match ty {
        Ty::Named(name) if records.contains_key(name) => name.clone(),
        Ty::List(element) if list_supported(element, records) => {
            format!("List_{}", mangle_option_payload(element, records))
        }
        Ty::Map(key, value) if map_supported(key, value) => {
            format!("Map_{}_{}", mangle_scalar(key), mangle_scalar(value))
        }
        Ty::Set(element) if set_supported(element) => {
            format!("Set_{}", mangle_scalar(element))
        }
        Ty::Applied(name, args)
            if name == "Option" && args.len() == 1 && option_supported(&args[0], records) =>
        {
            format!("Option_{}", mangle_option_payload(&args[0], records))
        }
        Ty::Applied(name, args)
            if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records) =>
        {
            format!(
                "Result_{}_{}",
                mangle_result_payload(&args[0], records),
                mangle_result_payload(&args[1], records)
            )
        }
        _ => mangle_scalar(ty),
    }
}

fn mangle_result_payload(ty: &Ty, records: &RecordFields) -> String {
    mangle_option_payload(ty, records)
}

fn retain_payload(access: &str, ty: &Ty, records: &RecordFields) -> Option<String> {
    match ty {
        Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => {
            Some(format!("ostrin_retain((void*){access})"))
        }
        Ty::Named(name) if records.contains_key(name) => {
            Some(format!("ostrin_retain((void*){access})"))
        }
        Ty::Applied(name, args)
            if name == "Option"
                && args.len() == 1
                && option_supported(&args[0], records)
                && option_managed_payload(&args[0], records) => retain_payload(&format!("({access}).value"), &args[0], records)
            .map(|body| format!("if (({access}).has) {{ {body}; }}")),
        Ty::Applied(name, args)
            if name == "Result"
                && args.len() == 2
                && result_supported(&args[0], &args[1], records)
                && result_managed_payload(ty, records) =>
        {
            let ok = retain_payload(&format!("({access}).value"), &args[0], records).unwrap_or_default();
            let err = retain_payload(&format!("({access}).error"), &args[1], records).unwrap_or_default();
            Some(format!("if (({access}).ok) {{ {ok}; }} else {{ {err}; }}"))
        }
        _ => None,
    }
}

fn release_payload(access: &str, ty: &Ty, records: &RecordFields) -> Option<String> {
    retain_payload(access, ty, records).map(|body| body.replace("ostrin_retain", "ostrin_release"))
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
        | IrInstr::TryErrorValue { dst, ty, .. }
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

fn collect_values(function: &IrFunction, records: &RecordFields) -> Bail<Values> {
    let mut values = HashMap::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Some((value, ty)) = defined_value(instruction) {
                if !supported(&ty, records) {
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
        BinOp::Rem => format!(
            "({{ {c} {a} = {left}; {c} {b} = {right}; if ({b} == 0) {{ fprintf(stderr, \"runtime error: division by zero\\n\"); exit(1); }} ({c})((__int128){a} % (__int128){b}); }})"
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
        && matches!(op, BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq)
        && *left_ty == *right_ty
    {
        let comparison = match op {
            BinOp::Eq => "== 0",
            BinOp::NotEq => "!= 0",
            BinOp::Lt => "< 0",
            BinOp::Gt => "> 0",
            BinOp::LtEq => "<= 0",
            _ => ">= 0",
        };
        return Ok(format!("(strcmp({left}, {right}) {comparison})"));
    }
    if *ty == Ty::Int && op == BinOp::Div {
        return Ok(format!("ostrin_idiv({left}, {right})"));
    }
    if op == BinOp::Rem {
        return Ok(if *left_ty == Ty::Int && *right_ty == Ty::Int {
            format!("ostrin_irem({left}, {right})")
        } else if *left_ty == Ty::Float32 {
            format!("fmodf({left}, {right})")
        } else {
            format!("fmod({left}, {right})")
        });
    }
    let symbol = match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => unreachable!("handled above"),
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

fn parse_int_result_code(receiver: &str) -> String {
    "({ Result_Int_String __ostrin_result; memset(&__ostrin_result, 0, sizeof __ostrin_result); const char* __ostrin_text = "
        .to_string()
        + receiver
        + "; if (*__ostrin_text == 0) { __ostrin_result.error = \"cannot parse integer from empty string\"; } else if (*__ostrin_text == ' ' || (*__ostrin_text >= 9 && *__ostrin_text <= 13)) { __ostrin_result.error = \"invalid digit found in string\"; } else { char* __ostrin_end; errno = 0; long long __ostrin_value = strtoll(__ostrin_text, &__ostrin_end, 10); if (errno == ERANGE) { __ostrin_result.error = __ostrin_value < 0 ? \"number too small to fit in target type\" : \"number too large to fit in target type\"; } else if (*__ostrin_end != 0 || __ostrin_end == __ostrin_text) { __ostrin_result.error = \"invalid digit found in string\"; } else { __ostrin_result.ok = true; __ostrin_result.value = (int64_t)__ostrin_value; } } __ostrin_result; })"
}

fn parse_float_result_code(receiver: &str) -> String {
    "({ Result_Float_String __ostrin_result; memset(&__ostrin_result, 0, sizeof __ostrin_result); const char* __ostrin_text = "
        .to_string()
        + receiver
        + "; int __ostrin_check = ostrin_s_float_check(__ostrin_text); if (__ostrin_check == 1) { __ostrin_result.error = \"cannot parse float from empty string\"; } else if (__ostrin_check == 2) { __ostrin_result.error = \"invalid float literal\"; } else { __ostrin_result.ok = true; __ostrin_result.value = strtod(__ostrin_text, NULL); } __ostrin_result; })"
}

fn emit_instruction(
    instruction: &IrInstr,
    values: &Values,
    known_functions: &HashSet<String>,
    records: &RecordFields,
    show: &mut dyn FnMut(&str, &Ty) -> Option<String>,
    out: &mut String,
) -> Bail<()> {
    match instruction {
        IrInstr::Param { dst, name, ty, .. } => {
            if !supported(ty, records) || *ty == Ty::Void {
                return Err(());
            }
            out.push_str(&format!("    {} = {name};\n", value_name(*dst)));
        }
        IrInstr::Global { dst, name, ty } => {
            let Ty::Applied(option_name, args) = ty else { return Err(()) };
            if name != "None" || option_name != "Option" || args.len() != 1 || !option_supported(&args[0], records) {
                return Err(());
            }
            let c_name = format!("Option_{}", mangle_option_payload(&args[0], records));
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
            if !supported(ty, records) || *ty == Ty::Void {
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
        IrInstr::TryCheck { dst, value } => {
            let Ty::Applied(name, args) = value_ty(values, *value)? else { return Err(()) };
            let source = value_code(values, *value)?;
            let check = match (name.as_str(), args.as_slice()) {
                ("Option", [inner]) if option_supported(inner, records) => format!("({source}).has"),
                ("Result", [ok, err]) if result_supported(ok, err, records) => format!("({source}).ok"),
                _ => return Err(()),
            };
            out.push_str(&format!("    {} = {check};\n", value_name(*dst)));
        }
        IrInstr::TryValue { dst, value, ty } => {
            let Ty::Applied(name, args) = value_ty(values, *value)? else { return Err(()) };
            let inner = match (name.as_str(), args.as_slice()) {
                ("Option", [inner]) if option_supported(inner, records) => inner,
                ("Result", [ok, err]) if result_supported(ok, err, records) => ok,
                _ => return Err(()),
            };
            if *ty != *inner || !supported(ty, records) {
                return Err(());
            }
            out.push_str(&format!(
                "    {} = ({}).value;\n",
                value_name(*dst),
                value_code(values, *value)?
            ));
        }
        IrInstr::TryError { dst, value, ty } => {
            let Ty::Applied(source_name, source_args) = value_ty(values, *value)? else { return Err(()) };
            let source = value_code(values, *value)?;
            match ty {
                Ty::Applied(name, args) if name == "Option" && args.len() == 1 => {
                    if source_name != "Option" || source_args.len() != 1 || !option_supported(&args[0], records) {
                        return Err(());
                    }
                    let option_name = format!("Option_{}", mangle_option_payload(&args[0], records));
                    out.push_str(&format!(
                        "    {} = (({option_name}){{ .has = false }});\n",
                        value_name(*dst)
                    ));
                }
                Ty::Applied(name, args) if name == "Result" && args.len() == 2 => {
                    if source_name != "Result"
                        || source_args.len() != 2
                        || source_args[1] != args[1]
                        || !result_supported(&args[0], &args[1], records)
                    {
                        return Err(());
                    }
                    let result_name = format!(
                        "Result_{}_{}",
                        mangle_result_payload(&args[0], records),
                        mangle_result_payload(&args[1], records)
                    );
                    out.push_str(&format!(
                        "    {} = (({result_name}){{ .ok = false, .error = ({}).error }});\n",
                        value_name(*dst),
                        source
                    ));
                    if let Some(retain) = retain_payload(&format!("({source}).error"), &args[1], records) {
                        out.push_str(&format!("    {retain};\n"));
                    }
                }
                _ => return Err(()),
            }
        }
        IrInstr::TryErrorValue { dst, value, ty } => {
            let Ty::Applied(name, args) = value_ty(values, *value)? else { return Err(()) };
            if name != "Result" || args.len() != 2 || *ty != args[1] || !result_supported(&args[0], &args[1], records) {
                return Err(());
            }
            out.push_str(&format!(
                "    {} = ({}).error;\n",
                value_name(*dst),
                value_code(values, *value)?
            ));
        }
        IrInstr::Aggregate {
            dst,
            kind,
            fields,
            field_names,
            ty,
        } => match ty {
            Ty::Named(name) => {
                let expected_kind = format!("record<{name}>");
                let Some(declared_fields) = records.get(name) else { return Err(()) };
                if kind != &expected_kind
                    || fields.len() != field_names.len()
                    || fields.len() != declared_fields.len()
                    || field_names.iter().any(|field| !declared_fields.contains(field))
                    || field_names.iter().collect::<HashSet<_>>().len() != field_names.len()
                {
                    return Err(());
                }
                for value in fields {
                    if !supported(&value_ty(values, *value)?, records) {
                        return Err(());
                    }
                }
                let code = value_name(*dst);
                out.push_str(&format!(
                    "    {code} = ({name}*)ostrin_calloc_with_drop(1, sizeof({name}), (void (*)(void*))(ostrin_drop_{name}));\n"
                ));
                out.push_str(&format!(
                    "    if (!{code}) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }}\n"
                ));
                for (field, value) in field_names.iter().zip(fields) {
                    out.push_str(&format!(
                        "    {code}->{field} = {};\n",
                        value_code(values, *value)?
                    ));
                }
            }
            Ty::List(element) => {
                if !list_supported(element, records) || (kind != "collection" && !kind.starts_with("empty_")) {
                    return Err(());
                }
                let element_c = c_type(element, records)?;
                let list_name = format!("List_{}", mangle_option_payload(element, records));
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
        IrInstr::Field { dst, object, field, ty } => {
            let Ty::Named(record) = value_ty(values, *object)? else { return Err(()) };
            if !records.contains_key(&record) || !supported(ty, records) {
                return Err(());
            }
            out.push_str(&format!(
                "    {} = ({})->{};\n",
                value_name(*dst),
                value_code(values, *object)?,
                field
            ));
        }
        IrInstr::Index { dst, object, index, ty } => {
            let Ty::List(element) = value_ty(values, *object)? else { return Err(()) };
            if !list_supported(&element, records) || value_ty(values, *index)? != Ty::Int || *ty != *element {
                return Err(());
            }
            let list_name = format!("List_{}", mangle_option_payload(&element, records));
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
                Ty::String => {
                    let string_args = args
                        .iter()
                        .map(|arg| (value_ty(values, *arg) == Ok(Ty::String)).then(|| value_code(values, *arg)))
                        .collect::<Option<Bail<Vec<_>>>>()
                        .ok_or(())??;
                    match (method.as_str(), string_args.as_slice(), ty) {
                        ("length", [], Ty::Int) => format!("ostrin_s_length({receiver})"),
                        ("is_empty", [], Ty::Bool) => format!("(*({receiver}) == 0)"),
                        ("trim", [], Ty::String) => format!("ostrin_s_trim({receiver})"),
                        ("to_upper", [], Ty::String) => format!("ostrin_s_upper({receiver})"),
                        ("to_lower", [], Ty::String) => format!("ostrin_s_lower({receiver})"),
                        ("contains", [pattern], Ty::Bool) => format!("(strstr({receiver}, {pattern}) != NULL)"),
                        ("starts_with", [pattern], Ty::Bool) => format!("ostrin_s_starts_with({receiver}, {pattern})"),
                        ("ends_with", [pattern], Ty::Bool) => format!("ostrin_s_ends_with({receiver}, {pattern})"),
                        ("replace", [from, to], Ty::String) => format!("ostrin_s_replace({receiver}, {from}, {to})"),
                        ("split", [separator], Ty::List(element)) if **element == Ty::String => format!(
                            "({{ int64_t __ostrin_split_count; const char** __ostrin_split_items = ostrin_s_split({receiver}, {separator}, &__ostrin_split_count); List_String* __ostrin_split_result = List_String_new_from_array(__ostrin_split_items, __ostrin_split_count); for (int64_t __ostrin_split_i = 0; __ostrin_split_i < __ostrin_split_count; __ostrin_split_i++) ostrin_release((void*)__ostrin_split_items[__ostrin_split_i]); ostrin_free((void*)__ostrin_split_items); __ostrin_split_result; }})"
                        ),
                        ("lines", [], Ty::List(element)) if **element == Ty::String => format!(
                            "({{ int64_t __ostrin_lines_count; const char** __ostrin_lines_items = ostrin_s_lines({receiver}, &__ostrin_lines_count); List_String* __ostrin_lines_result = List_String_new_from_array(__ostrin_lines_items, __ostrin_lines_count); for (int64_t __ostrin_lines_i = 0; __ostrin_lines_i < __ostrin_lines_count; __ostrin_lines_i++) ostrin_release((void*)__ostrin_lines_items[__ostrin_lines_i]); ostrin_free((void*)__ostrin_lines_items); __ostrin_lines_result; }})"
                        ),
                        ("to_int", [], Ty::Applied(name, args))
                            if name == "Result" && args == &vec![Ty::Int, Ty::String] => parse_int_result_code(&receiver),
                        ("to_float", [], Ty::Applied(name, args))
                            if name == "Result" && args == &vec![Ty::Float, Ty::String] => parse_float_result_code(&receiver),
                        _ => return Err(()),
                    }
                }
                Ty::List(element) if list_supported(&element, records) => {
                    let list_name = format!("List_{}", mangle_option_payload(&element, records));
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
                                && option_supported(value.as_ref(), records)
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
                    if name == "Option" && option_args.len() == 1 && option_supported(&option_args[0], records) =>
                {
                    let inner = option_args[0].clone();
                    let option_name = format!("Option_{}", mangle_option_payload(&inner, records));
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
                Ty::Applied(name, result_args)
                    if name == "Result"
                        && result_args.len() == 2
                        && result_supported(&result_args[0], &result_args[1], records) =>
                {
                    let ok = result_args[0].clone();
                    let err = result_args[1].clone();
                    let result_name = format!(
                        "Result_{}_{}",
                        mangle_result_payload(&ok, records),
                        mangle_result_payload(&err, records)
                    );
                    match method.as_str() {
                        "is_ok" if args.is_empty() && *ty == Ty::Bool => format!("({receiver}).ok"),
                        "is_err" if args.is_empty() && *ty == Ty::Bool => format!("!({receiver}).ok"),
                        "unwrap" if args.is_empty() && *ty == ok => format!(
                            "({{ {result_name} __ostrin_result = {receiver}; if (!__ostrin_result.ok) {{ fprintf(stderr, \"ostrin: unwrap on Err\\n\"); exit(1); }} __ostrin_result.value; }})"
                        ),
                        "unwrap_or"
                            if args.len() == 1 && *ty == ok && value_ty(values, args[0])? == ok => format!(
                                "({{ {result_name} __ostrin_result = {receiver}; __ostrin_result.ok ? __ostrin_result.value : {}; }})",
                                value_code(values, args[0])?
                            ),
                        "ok" if args.is_empty() && *ty == option_type(&ok) => {
                            let option_name = format!("Option_{}", mangle_option_payload(&ok, records));
                            if option_managed_payload(&ok, records) {
                                let retain = retain_payload("__ostrin_option.value", &ok, records).ok_or(())?;
                                format!(
                                    "({{ {option_name} __ostrin_option = (({option_name}){{ .has = ({receiver}).ok, .value = ({receiver}).value }}); if (__ostrin_option.has) {{ {retain}; }} __ostrin_option; }})"
                                )
                            } else {
                                format!("(({option_name}){{ .has = ({receiver}).ok, .value = ({receiver}).value }})")
                            }
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
            if !supported(ty, records) {
                return Err(());
            }
            let codes = args
                .iter()
                .map(|value| value_code(values, *value))
                .collect::<Bail<Vec<_>>>()?;
            let call = if callee == "Some" && args.len() == 1 {
                let inner = value_ty(values, args[0])?;
                if !option_supported(&inner, records) || *ty != option_type(&inner) {
                    return Err(());
                }
                let option_name = format!("Option_{}", mangle_option_payload(&inner, records));
                if !option_managed_payload(&inner, records) {
                    format!("(({option_name}){{ .has = true, .value = {} }})", codes[0])
                } else {
                    let retain = retain_payload("__ostrin_option.value", &inner, records).ok_or(())?;
                    format!(
                        "({{ {option_name} __ostrin_option = (({option_name}){{ .has = true, .value = {} }}); {retain}; __ostrin_option; }})",
                        codes[0]
                    )
                }
            } else if (callee == "Ok" || callee == "Err") && args.len() == 1 {
                let Ty::Applied(name, result_args) = ty else { return Err(()) };
                if name != "Result" || result_args.len() != 2 || !result_supported(&result_args[0], &result_args[1], records) {
                    return Err(());
                }
                let expected = if callee == "Ok" { &result_args[0] } else { &result_args[1] };
                if value_ty(values, args[0])? != *expected {
                    return Err(());
                }
                let result_name = format!(
                    "Result_{}_{}",
                    mangle_result_payload(&result_args[0], records),
                    mangle_result_payload(&result_args[1], records)
                );
                let field = if callee == "Ok" { "value" } else { "error" };
                let active = if callee == "Ok" { ".ok = true" } else { ".ok = false" };
                if result_managed_payload(expected, records) {
                    let retain = retain_payload(&format!("__ostrin_result.{field}"), expected, records).ok_or(())?;
                    format!(
                        "({{ {result_name} __ostrin_result = (({result_name}){{ {active}, .{field} = {} }}); {retain}; __ostrin_result; }})",
                        codes[0]
                    )
                } else {
                    format!("(({result_name}){{ {active}, .{field} = {} }})", codes[0])
                }
            } else if callee == "print" && args.len() == 1 {
                if *ty != Ty::Void {
                    return Err(());
                }
                let arg_ty = value_ty(values, args[0])?;
                if scalar(&arg_ty) {
                    print_code(&codes[0], &arg_ty)?
                } else if supported(&arg_ty, records) {
                    // Collections, options and records print through the generated
                    // `ostrin_show_*` helper; the rendered text is an owned string.
                    let shown = show(&codes[0], &arg_ty).ok_or(())?;
                    format!("({{ const char* __ostrin_shown = {shown}; printf(\"%s\\n\", __ostrin_shown); ostrin_release((void*)__ostrin_shown); }})")
                } else {
                    return Err(());
                }
            } else {
                if !known_functions.contains(callee) {
                    return Err(());
                }
                for value in args {
                    if !supported(&value_ty(values, *value)?, records) {
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
        IrInstr::PatternTest { dst, subject, pattern } => {
            let Ty::Applied(name, args) = value_ty(values, *subject)? else { return Err(()) };
            let subject = value_code(values, *subject)?;
            let test = if name == "Option" && args.len() == 1 && option_supported(&args[0], records) {
                if pattern == "Ident(\"None\")" {
                    format!("!({subject}).has")
                } else if pattern.starts_with("Variant(\"Some\",") {
                    format!("({subject}).has")
                } else {
                    return Err(());
                }
            } else if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records) {
                if pattern.starts_with("Variant(\"Ok\",") {
                    format!("({subject}).ok")
                } else if pattern.starts_with("Variant(\"Err\",") {
                    format!("!({subject}).ok")
                } else {
                    return Err(());
                }
            } else {
                return Err(());
            };
            out.push_str(&format!("    {} = {test};\n", value_name(*dst)));
        }
        IrInstr::PatternBind { dst, subject, path, ty, .. } => {
            let subject_ty = value_ty(values, *subject)?;
            let (code, bound_ty) = if path.is_empty() {
                (value_code(values, *subject)?, subject_ty)
            } else {
                let Ty::Applied(name, args) = subject_ty else { return Err(()) };
                if name == "Option" && args.len() == 1 && option_supported(&args[0], records) && path.len() == 1 {
                    (format!("({}).value", value_code(values, *subject)?), args[0].clone())
                } else if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records) && path.len() == 2 {
                    let field = match path[0].as_str() {
                        "Ok" => ("value", args[0].clone()),
                        "Err" => ("error", args[1].clone()),
                        _ => return Err(()),
                    };
                    (format!("({}).{}", value_code(values, *subject)?, field.0), field.1)
                } else {
                    return Err(());
                }
            };
            if bound_ty != *ty {
                return Err(());
            }
            out.push_str(&format!("    {} = {code};\n", value_name(*dst)));
        }
        IrInstr::Phi { dst, incoming, ty } => {
            if !supported(ty, records) || *ty == Ty::Void || incoming.is_empty() {
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
            let ty = value_ty(values, *value)?;
            match ty {
                Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => {
                    out.push_str(&format!("    ostrin_retain((void*){});\n", value_code(values, *value)?));
                }
                Ty::Named(name) if records.contains_key(&name) => {
                    out.push_str(&format!("    ostrin_retain((void*){});\n", value_code(values, *value)?));
                }
                Ty::Applied(name, args)
                    if name == "Option" && args.len() == 1 && option_managed_payload(&args[0], records) =>
                {
                    let code = value_code(values, *value)?;
                    let retain = retain_payload(&format!("({code}).value"), &args[0], records).ok_or(())?;
                    out.push_str(&format!("    if (({code}).has) {{ {retain}; }}\n"));
                }
                Ty::Applied(name, args)
                    if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records) =>
                {
                    let code = value_code(values, *value)?;
                    let ok = retain_payload(&format!("({code}).value"), &args[0], records);
                    let err = retain_payload(&format!("({code}).error"), &args[1], records);
                    if ok.is_some() || err.is_some() {
                        out.push_str(&format!(
                            "    if (({code}).ok) {{ {}; }} else {{ {}; }}\n",
                            ok.unwrap_or_default(),
                            err.unwrap_or_default()
                        ));
                    }
                }
                _ => return Err(()),
            }
        }
        IrInstr::Release { value } => {
            let ty = value_ty(values, *value)?;
            match ty {
                Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => {
                    out.push_str(&format!("    ostrin_release((void*){});\n", value_code(values, *value)?));
                }
                Ty::Named(name) if records.contains_key(&name) => {
                    out.push_str(&format!("    ostrin_release((void*){});\n", value_code(values, *value)?));
                }
                Ty::Applied(name, args)
                    if name == "Option" && args.len() == 1 && option_managed_payload(&args[0], records) =>
                {
                    let code = value_code(values, *value)?;
                    let release = release_payload(&format!("({code}).value"), &args[0], records).ok_or(())?;
                    out.push_str(&format!("    if (({code}).has) {{ {release}; }}\n"));
                }
                Ty::Applied(name, args)
                    if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records) =>
                {
                    let code = value_code(values, *value)?;
                    let ok = release_payload(&format!("({code}).value"), &args[0], records);
                    let err = release_payload(&format!("({code}).error"), &args[1], records);
                    if ok.is_some() || err.is_some() {
                        out.push_str(&format!(
                            "    if (({code}).ok) {{ {}; }} else {{ {}; }}\n",
                            ok.unwrap_or_default(),
                            err.unwrap_or_default()
                        ));
                    }
                }
                _ => return Err(()),
            }
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
pub fn generate(
    function: &IrFunction,
    known_functions: &HashSet<String>,
    records: &RecordFields,
    show: &mut dyn FnMut(&str, &Ty) -> Option<String>,
) -> Option<String> {
    if function.entry >= function.blocks.len()
        || function
            .params
            .iter()
        .any(|(_, ty)| !supported(ty, records) || *ty == Ty::Void)
        || !supported(&function.ret, records)
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

    let values = collect_values(function, records).ok()?;
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
            c_type(&ty, records).ok()?,
            value_name(value)
        ));
    }
    out.push_str(&format!("    goto {};\n", block_label(function.entry)));

    for block in &function.blocks {
        out.push_str(&format!("{}:\n", block_label(block.id)));
        for instruction in &block.instructions {
            emit_instruction(instruction, &values, known_functions, records, show, &mut out).ok()?;
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
