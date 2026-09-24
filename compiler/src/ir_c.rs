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
//! with recursive ownership markers. Straight-line tasks additionally use a
//! generated C environment for immutable captures while larger aggregates,
//! branching task bodies and scopes retain the verified HIR/AST fallback.

use std::collections::{HashMap, HashSet};

use crate::ast::{BinOp, UnaryOp};
use crate::ir::{BlockId, IrFunction, IrInstr, IrTerminator, ValueId};
use crate::types::Ty;

type Bail<T> = Result<T, ()>;
type Values = HashMap<ValueId, (String, Ty)>;
pub type RecordFields = HashMap<String, Vec<String>>;
pub type MethodNames = HashMap<(String, String), String>;
pub type FunctionNames = HashMap<String, String>;

fn record_name(ty: &Ty, records: &RecordFields) -> Option<String> {
    match ty {
        Ty::Named(name) if records.contains_key(name) => Some(name.clone()),
        Ty::Applied(name, args) => {
            let instance = format!(
                "{name}__{}",
                args.iter()
                    .map(|arg| mangle_record_type(arg, records))
                    .collect::<Vec<_>>()
                    .join("_")
            );
            records.contains_key(&instance).then_some(instance)
        }
        _ => None,
    }
}

fn mangle_record_type(ty: &Ty, records: &RecordFields) -> String {
    match ty {
        Ty::Int => "Int".to_string(),
        Ty::Float => "Float".to_string(),
        Ty::Float32 => "Float32".to_string(),
        Ty::Sized(kind) => kind.name().to_string(),
        Ty::Bool => "Bool".to_string(),
        Ty::Char => "Char".to_string(),
        Ty::String => "String".to_string(),
        Ty::Void => "Void".to_string(),
        Ty::Named(name) => name.clone(),
        Ty::Applied(name, args)
            if matches!(
                name.as_str(),
                "Option" | "Result" | "List" | "Map" | "Set" | "Channel" | "Task"
            ) =>
        {
            format!(
                "{name}_{}",
                args.iter()
                    .map(|arg| mangle_record_type(arg, records))
                    .collect::<Vec<_>>()
                    .join("_")
            )
        }
        Ty::Applied(name, args) => format!(
            "{name}__{}",
            args.iter()
                .map(|arg| mangle_record_type(arg, records))
                .collect::<Vec<_>>()
                .join("_")
        ),
        Ty::List(element) => format!("List_{}", mangle_record_type(element, records)),
        Ty::Map(key, value) => format!(
            "Map_{}_{}",
            mangle_record_type(key, records),
            mangle_record_type(value, records)
        ),
        Ty::Set(element) => format!("Set_{}", mangle_record_type(element, records)),
        Ty::Fn(_, _) => "Fn".to_string(),
        Ty::Quantity(_) => "Quantity".to_string(),
        Ty::Dyn(name) => name.clone(),
        Ty::Generic(name) => name.clone(),
        Ty::Unknown => "Unknown".to_string(),
    }
}

#[derive(Clone, Copy)]
pub enum HelperRequest {
    Show,
    Equality,
}

type HelperGenerator<'a> = dyn FnMut(HelperRequest, &str, &str, &Ty) -> Option<String> + 'a;

#[derive(Clone)]
struct SpawnHelper {
    callback: String,
    env_type: Option<String>,
    drop_env: Option<String>,
    captures: Vec<(ValueId, Ty)>,
}

type SpawnHelpers = HashMap<BlockId, SpawnHelper>;

#[derive(Clone)]
struct ClosureHelper {
    adapter: String,
    env_type: Option<String>,
    drop_name: Option<String>,
    captures: Vec<(String, Ty)>,
}

type ClosureHelpers = HashMap<usize, ClosureHelper>;

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
        Ty::Fn(_, _) => "OstrinClosure".to_string(),
        Ty::Named(name) if records.contains_key(name) => format!("{name}*"),
        Ty::Applied(_, _) if record_name(ty, records).is_some() => {
            format!("{}*", record_name(ty, records).ok_or(())?)
        }
        Ty::Applied(name, args)
            if name == "Channel" && args.len() == 1 && channel_supported(&args[0], records) =>
        {
            format!("Channel_{}*", mangle_option_payload(&args[0], records))
        }
        Ty::Applied(name, args)
            if name == "Task" && args.len() == 1 && task_supported(&args[0], records) =>
        {
            format!("Task_{}*", mangle_task_payload(&args[0], records))
        }
        Ty::Applied(name, args)
            if name == "Option" && args.len() == 1 && option_supported(&args[0], records) =>
        {
            format!("Option_{}", mangle_option_payload(&args[0], records))
        }
        Ty::Applied(name, args)
            if name == "Result"
                && args.len() == 2
                && result_supported(&args[0], &args[1], records) =>
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
        || record_name(ty, records).is_some()
        || matches!(ty, Ty::List(element) if list_supported(element, records))
        || matches!(ty, Ty::Map(key, value) if map_supported(key, value))
        || matches!(ty, Ty::Set(element) if set_supported(element))
        || matches!(ty, Ty::Fn(_, _))
        || matches!(ty, Ty::Applied(name, args) if name == "Channel" && args.len() == 1 && channel_supported(&args[0], records))
        || matches!(ty, Ty::Applied(name, args) if name == "Task" && args.len() == 1 && task_supported(&args[0], records))
        || matches!(ty, Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_supported(&args[0], records))
        || matches!(ty, Ty::Applied(name, args) if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records))
}

fn list_element_supported(ty: &Ty) -> bool {
    scalar(ty) && *ty != Ty::Void
}

fn list_supported(ty: &Ty, records: &RecordFields) -> bool {
    list_element_supported(ty)
        || record_name(ty, records).is_some()
        || matches!(ty, Ty::Applied(name, args) if name == "Channel" && args.len() == 1 && channel_supported(&args[0], records))
}

fn map_supported(key: &Ty, value: &Ty) -> bool {
    list_element_supported(key) && list_element_supported(value)
}

fn set_supported(element: &Ty) -> bool {
    list_element_supported(element)
}

fn channel_supported(element: &Ty, records: &RecordFields) -> bool {
    option_supported(element, records)
}

fn task_supported(result: &Ty, records: &RecordFields) -> bool {
    supported(result, records)
}

fn option_supported(element: &Ty, records: &RecordFields) -> bool {
    matches!(
        element,
        Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_) | Ty::Bool | Ty::String
    ) || matches!(element, Ty::List(inner) if list_supported(inner, records))
        || matches!(element, Ty::Map(key, value) if map_supported(key, value))
        || matches!(element, Ty::Set(inner) if set_supported(inner))
        || matches!(element, Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_supported(&args[0], records))
        || matches!(element, Ty::Applied(name, args) if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records))
        || record_name(element, records).is_some()
}

fn option_managed_payload(element: &Ty, records: &RecordFields) -> bool {
    managed_payload(element, records)
}

fn result_payload_supported(ty: &Ty, records: &RecordFields) -> bool {
    scalar(ty)
        || matches!(ty, Ty::List(inner) if list_supported(inner, records))
        || matches!(ty, Ty::Map(key, value) if map_supported(key, value))
        || matches!(ty, Ty::Set(inner) if set_supported(inner))
        || matches!(ty, Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_supported(&args[0], records))
        || matches!(ty, Ty::Applied(name, args) if name == "Result" && args.len() == 2 && result_supported(&args[0], &args[1], records))
        || record_name(ty, records).is_some()
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
        Ty::Applied(_, _) if record_name(ty, records).is_some() => true,
        Ty::Applied(name, args) if name == "Channel" && args.len() == 1 => {
            channel_supported(&args[0], records)
        }
        Ty::Applied(name, args) if name == "Task" && args.len() == 1 => {
            task_supported(&args[0], records)
        }
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
        Ty::Applied(_, _) if record_name(ty, records).is_some() => {
            record_name(ty, records).unwrap_or_default()
        }
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
            if name == "Result"
                && args.len() == 2
                && result_supported(&args[0], &args[1], records) =>
        {
            format!(
                "Result_{}_{}",
                mangle_result_payload(&args[0], records),
                mangle_result_payload(&args[1], records)
            )
        }
        Ty::Applied(name, args)
            if name == "Channel" && args.len() == 1 && channel_supported(&args[0], records) =>
        {
            format!("Channel_{}", mangle_option_payload(&args[0], records))
        }
        _ => mangle_scalar(ty),
    }
}

fn mangle_result_payload(ty: &Ty, records: &RecordFields) -> String {
    if *ty == Ty::Void {
        "Void".to_string()
    } else {
        mangle_option_payload(ty, records)
    }
}

fn mangle_task_payload(ty: &Ty, records: &RecordFields) -> String {
    if *ty == Ty::Void {
        "Void".to_string()
    } else {
        mangle_option_payload(ty, records)
    }
}

fn retain_payload(access: &str, ty: &Ty, records: &RecordFields) -> Option<String> {
    match ty {
        Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => {
            Some(format!("ostrin_retain((void*){access})"))
        }
        Ty::Named(name) if records.contains_key(name) => {
            Some(format!("ostrin_retain((void*){access})"))
        }
        Ty::Applied(_, _) if record_name(ty, records).is_some() => {
            Some(format!("ostrin_retain((void*){access})"))
        }
        Ty::Applied(name, args)
            if name == "Option"
                && args.len() == 1
                && option_supported(&args[0], records)
                && option_managed_payload(&args[0], records) =>
        {
            retain_payload(&format!("({access}).value"), &args[0], records)
                .map(|body| format!("if (({access}).has) {{ {body}; }}"))
        }
        Ty::Applied(name, args)
            if name == "Result"
                && args.len() == 2
                && result_supported(&args[0], &args[1], records)
                && result_managed_payload(ty, records) =>
        {
            let ok =
                retain_payload(&format!("({access}).value"), &args[0], records).unwrap_or_default();
            let err =
                retain_payload(&format!("({access}).error"), &args[1], records).unwrap_or_default();
            Some(format!("if (({access}).ok) {{ {ok}; }} else {{ {err}; }}"))
        }
        Ty::Applied(name, args)
            if name == "Task" && args.len() == 1 && task_supported(&args[0], records) =>
        {
            Some(format!("ostrin_retain((void*){access})"))
        }
        Ty::Applied(name, args)
            if name == "Channel" && args.len() == 1 && channel_supported(&args[0], records) =>
        {
            Some(format!("ostrin_retain((void*){access})"))
        }
        Ty::Fn(_, _) => Some(format!("ostrin_retain((void*)({access}).env)")),
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

fn closure_adapter_name(owner: &str, target: &str) -> String {
    format!(
        "ostrin_ir_closure_{}_{}",
        crate::codegen::c_function_name(owner),
        crate::codegen::c_function_name(target)
    )
}

fn closure_fn_type(params: &[Ty], ret: &Ty, records: &RecordFields) -> Bail<String> {
    let params = params
        .iter()
        .map(|ty| c_type(ty, records))
        .collect::<Bail<Vec<_>>>()?;
    let ret = c_type(ret, records)?;
    let params = std::iter::once("void*".to_string())
        .chain(params)
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!("{ret} (*)({params})"))
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
        | IrInstr::ClosureMake { dst, ty, .. }
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
        | IrInstr::ClosureCall {
            dst: Some(dst), ty, ..
        }
        | IrInstr::MethodCall {
            dst: Some(dst), ty, ..
        }
        | IrInstr::Opaque {
            dst: Some(dst), ty, ..
        } => Some((*dst, ty.clone())),
        IrInstr::StoreLocal { .. }
        | IrInstr::FieldStore { .. }
        | IrInstr::Call { dst: None, .. }
        | IrInstr::ClosureCall { dst: None, .. }
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
    equality: &mut HelperGenerator<'_>,
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
    if matches!(op, BinOp::Eq | BinOp::NotEq) && left_ty == right_ty && !scalar(left_ty) {
        let expression = equality(HelperRequest::Equality, left, right, left_ty).ok_or(())?;
        return Ok(if op == BinOp::NotEq {
            format!("(!({expression}))")
        } else {
            expression
        });
    }
    if (*left_ty == Ty::String || *right_ty == Ty::String)
        && matches!(
            op,
            BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq
        )
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

fn read_file_result_code(path: &str) -> String {
    format!(
        "({{ OstrinFileOutcome __ostrin_outcome = ostrin_file_read_cancelable({path}); Result_String_String __ostrin_result; memset(&__ostrin_result, 0, sizeof __ostrin_result); if (__ostrin_outcome.ok) {{ __ostrin_result.ok = true; __ostrin_result.value = ostrin_s_dup(__ostrin_outcome.value, strlen(__ostrin_outcome.value)); }} else {{ __ostrin_result.error = ostrin_s_dup(__ostrin_outcome.error, strlen(__ostrin_outcome.error)); }} ostrin_file_outcome_dispose(&__ostrin_outcome); __ostrin_result; }})",
        path = path
    )
}

fn write_file_result_code(path: &str, text: &str) -> String {
    format!(
        "({{ OstrinFileOutcome __ostrin_outcome = ostrin_file_write_cancelable({path}, {text}); Result_Void_String __ostrin_result; memset(&__ostrin_result, 0, sizeof __ostrin_result); if (__ostrin_outcome.ok) {{ __ostrin_result.ok = true; }} else {{ __ostrin_result.error = ostrin_s_dup(__ostrin_outcome.error, strlen(__ostrin_outcome.error)); }} ostrin_file_outcome_dispose(&__ostrin_outcome); __ostrin_result; }})",
        path = path,
        text = text
    )
}

fn format_float_result_code(value: &str, digits: &str) -> String {
    format!(
        "({{ Result_String_String __ostrin_result; memset(&__ostrin_result, 0, sizeof __ostrin_result); if (({digits}) < 0 || ({digits}) > 18) {{ __ostrin_result.error = \"float precision must be between 0 and 18\"; }} else {{ __ostrin_result.ok = true; __ostrin_result.value = ostrin_float_format((double)({value}), (int64_t)({digits})); }} __ostrin_result; }})"
    )
}

fn emit_instruction(
    instruction: &IrInstr,
    values: &Values,
    owner: &str,
    known_functions: &FunctionNames,
    methods: &MethodNames,
    records: &RecordFields,
    spawn_helpers: &SpawnHelpers,
    closure_helpers: &ClosureHelpers,
    helper: &mut HelperGenerator<'_>,
    out: &mut String,
) -> Bail<()> {
    match instruction {
        IrInstr::Param { dst, name, ty, .. } => {
            if !supported(ty, records) || *ty == Ty::Void {
                return Err(());
            }
            out.push_str(&format!("    {} = {name};\n", value_name(*dst)));
        }
        IrInstr::Global { dst, name, ty } => match ty {
            Ty::Applied(option_name, args)
                if name == "None"
                    && option_name == "Option"
                    && args.len() == 1
                    && option_supported(&args[0], records) =>
            {
                let c_name = format!("Option_{}", mangle_option_payload(&args[0], records));
                out.push_str(&format!(
                    "    {} = (({c_name}){{ .has = false }});\n",
                    value_name(*dst)
                ));
            }
            Ty::Fn(_, _) if known_functions.contains_key(name) => {
                let adapter = closure_adapter_name(owner, name);
                out.push_str(&format!(
                    "    {} = ((OstrinClosure){{ (void*){}, NULL }});\n",
                    value_name(*dst),
                    adapter
                ));
            }
            _ => return Err(()),
        },
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
            let structural_equality = matches!(op, BinOp::Eq | BinOp::NotEq)
                && left_ty == right_ty
                && supported(&left_ty, records);
            if (!scalar(&left_ty) || !scalar(&right_ty)) && !structural_equality {
                return Err(());
            }
            let code = binary_code(
                *op,
                &value_code(values, *left)?,
                &left_ty,
                &value_code(values, *right)?,
                &right_ty,
                ty,
                helper,
            )?;
            out.push_str(&format!("    {} = {code};\n", value_name(*dst)));
        }
        IrInstr::TryCheck { dst, value } => {
            let Ty::Applied(name, args) = value_ty(values, *value)? else {
                return Err(());
            };
            let source = value_code(values, *value)?;
            let check = match (name.as_str(), args.as_slice()) {
                ("Option", [inner]) if option_supported(inner, records) => {
                    format!("({source}).has")
                }
                ("Result", [ok, err]) if result_supported(ok, err, records) => {
                    format!("({source}).ok")
                }
                _ => return Err(()),
            };
            out.push_str(&format!("    {} = {check};\n", value_name(*dst)));
        }
        IrInstr::TryValue { dst, value, ty } => {
            let Ty::Applied(name, args) = value_ty(values, *value)? else {
                return Err(());
            };
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
            let Ty::Applied(source_name, source_args) = value_ty(values, *value)? else {
                return Err(());
            };
            let source = value_code(values, *value)?;
            match ty {
                Ty::Applied(name, args) if name == "Option" && args.len() == 1 => {
                    if source_name != "Option"
                        || source_args.len() != 1
                        || !option_supported(&args[0], records)
                    {
                        return Err(());
                    }
                    let option_name =
                        format!("Option_{}", mangle_option_payload(&args[0], records));
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
                    if let Some(retain) =
                        retain_payload(&format!("({source}).error"), &args[1], records)
                    {
                        out.push_str(&format!("    {retain};\n"));
                    }
                }
                _ => return Err(()),
            }
        }
        IrInstr::TryErrorValue { dst, value, ty } => {
            let Ty::Applied(name, args) = value_ty(values, *value)? else {
                return Err(());
            };
            if name != "Result"
                || args.len() != 2
                || *ty != args[1]
                || !result_supported(&args[0], &args[1], records)
            {
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
            Ty::Named(_) | Ty::Applied(_, _) => {
                let Some(name) = record_name(ty, records) else {
                    return Err(());
                };
                let base_name = name
                    .split_once("__")
                    .map_or(name.as_str(), |(base, _)| base);
                let expected_kind = format!("record<{base_name}>");
                let Some(declared_fields) = records.get(&name) else {
                    return Err(());
                };
                if kind != &expected_kind
                    || fields.len() != field_names.len()
                    || fields.len() != declared_fields.len()
                    || field_names
                        .iter()
                        .any(|field| !declared_fields.contains(field))
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
                if !list_supported(element, records)
                    || (kind != "collection" && !kind.starts_with("empty_"))
                {
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
                if !set_supported(element) || (kind != "collection" && !kind.starts_with("empty_"))
                {
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
                if !map_supported(key, value)
                    || (kind != "map" && !kind.starts_with("empty_"))
                    || fields.len() % 2 != 0
                {
                    return Err(());
                }
                let map_name = format!("Map_{}_{}", mangle_scalar(key), mangle_scalar(value));
                out.push_str(&format!("    {} = {map_name}_new();\n", value_name(*dst)));
                for pair in fields.chunks_exact(2) {
                    if value_ty(values, pair[0])? != **key || value_ty(values, pair[1])? != **value
                    {
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
        IrInstr::Field {
            dst,
            object,
            field,
            ty,
        } => {
            let object_ty = value_ty(values, *object)?;
            let Some(record) = record_name(&object_ty, records) else {
                return Err(());
            };
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
        IrInstr::FieldStore {
            object,
            field,
            value,
        } => {
            let object_ty = value_ty(values, *object)?;
            let Some(record) = record_name(&object_ty, records) else {
                return Err(());
            };
            let value_ty = value_ty(values, *value)?;
            if !records
                .get(&record)
                .is_some_and(|fields| fields.iter().any(|name| name == field))
                || !scalar(&value_ty)
            {
                return Err(());
            }
            out.push_str(&format!(
                "    ({})->{} = {};\n",
                value_code(values, *object)?,
                field,
                value_code(values, *value)?
            ));
        }
        IrInstr::Index {
            dst,
            object,
            index,
            ty,
        } => {
            let Ty::List(element) = value_ty(values, *object)? else {
                return Err(());
            };
            if !list_supported(&element, records)
                || value_ty(values, *index)? != Ty::Int
                || *ty != *element
            {
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
                        .map(|arg| {
                            (value_ty(values, *arg) == Ok(Ty::String))
                                .then(|| value_code(values, *arg))
                        })
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
                        "push"
                            if args.len() == 1
                                && *ty == Ty::Void
                                && value_ty(values, args[0])? == *element =>
                        {
                            format!(
                                "{list_name}_push({receiver}, {})",
                                value_code(values, args[0])?
                            )
                        }
                        "remove_at"
                            if args.len() == 1
                                && *ty == *element
                                && value_ty(values, args[0])? == Ty::Int =>
                        {
                            format!(
                                "{list_name}_remove_at({receiver}, {})",
                                value_code(values, args[0])?
                            )
                        }
                        _ => return Err(()),
                    }
                }
                Ty::Map(key, value) if map_supported(&key, &value) => {
                    let map_name = format!("Map_{}_{}", mangle_scalar(&key), mangle_scalar(&value));
                    match method.as_str() {
                        "contains_key"
                            if args.len() == 1
                                && *ty == Ty::Bool
                                && value_ty(values, args[0])? == *key =>
                        {
                            format!(
                                "{map_name}_contains_key({receiver}, {})",
                                value_code(values, args[0])?
                            )
                        }
                        "get" | "remove"
                            if args.len() == 1
                                && option_supported(value.as_ref(), records)
                                && *ty == option_type(value.as_ref())
                                && value_ty(values, args[0])? == *key =>
                        {
                            format!(
                                "{map_name}_{method}({receiver}, {})",
                                value_code(values, args[0])?
                            )
                        }
                        "count" if args.is_empty() && *ty == Ty::Int => {
                            format!("{map_name}_count({receiver})")
                        }
                        "set"
                            if args.len() == 2
                                && *ty == Ty::Void
                                && value_ty(values, args[0])? == *key
                                && value_ty(values, args[1])? == *value =>
                        {
                            format!(
                                "{map_name}_set({receiver}, {}, {})",
                                value_code(values, args[0])?,
                                value_code(values, args[1])?
                            )
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
                    if name == "Option"
                        && option_args.len() == 1
                        && option_supported(&option_args[0], records) =>
                {
                    let inner = option_args[0].clone();
                    let option_name = format!("Option_{}", mangle_option_payload(&inner, records));
                    match method.as_str() {
                        "is_some" if args.is_empty() && *ty == Ty::Bool => {
                            format!("({receiver}).has")
                        }
                        "is_none" if args.is_empty() && *ty == Ty::Bool => {
                            format!("!({receiver}).has")
                        }
                        "unwrap" if args.is_empty() && *ty == inner => {
                            let inner_c = c_type(&inner, records)?;
                            let retain = retain_payload("__ostrin_unwrapped", &inner, records)
                                .unwrap_or_default();
                            format!(
                                "({{ {option_name} __ostrin_option = {receiver}; if (!__ostrin_option.has) {{ fprintf(stderr, \"ostrin: unwrap on None\\n\"); exit(1); }} {inner_c} __ostrin_unwrapped = __ostrin_option.value; {retain}; __ostrin_unwrapped; }})"
                            )
                        }
                        "unwrap_or"
                            if args.len() == 1
                                && *ty == inner
                                && value_ty(values, args[0])? == inner =>
                        {
                            let fallback = value_code(values, args[0])?;
                            if option_managed_payload(&inner, records) {
                                let inner_c = c_type(&inner, records)?;
                                let retain_value =
                                    retain_payload("__ostrin_unwrapped", &inner, records)
                                        .unwrap_or_default();
                                let retain_fallback =
                                    retain_payload(&fallback, &inner, records).unwrap_or_default();
                                format!(
                                    "({{ {option_name} __ostrin_option = {receiver}; {inner_c} __ostrin_unwrapped; if (__ostrin_option.has) {{ __ostrin_unwrapped = __ostrin_option.value; {retain_value}; }} else {{ __ostrin_unwrapped = {fallback}; {retain_fallback}; }} __ostrin_unwrapped; }})"
                                )
                            } else {
                                format!(
                                    "({{ {option_name} __ostrin_option = {receiver}; __ostrin_option.has ? __ostrin_option.value : {fallback}; }})"
                                )
                            }
                        }
                        "ok_or" if args.len() == 1 => {
                            let error = value_ty(values, args[0])?;
                            let Ty::Applied(result_name_ty, result_args) = ty else {
                                return Err(());
                            };
                            if result_name_ty != "Result"
                                || result_args.len() != 2
                                || result_args[0] != inner
                                || result_args[1] != error
                                || !result_supported(&result_args[0], &result_args[1], records)
                            {
                                return Err(());
                            }
                            let result_name = format!(
                                "Result_{}_{}",
                                mangle_result_payload(&result_args[0], records),
                                mangle_result_payload(&result_args[1], records)
                            );
                            let error_code = value_code(values, args[0])?;
                            let retain_value =
                                retain_payload("__ostrin_result.value", &inner, records)
                                    .unwrap_or_default();
                            let retain_error =
                                retain_payload("__ostrin_result.error", &error, records)
                                    .unwrap_or_default();
                            format!(
                                "({{ {option_name} __ostrin_option = {receiver}; {result_name} __ostrin_result; memset(&__ostrin_result, 0, sizeof __ostrin_result); if (__ostrin_option.has) {{ __ostrin_result.ok = true; __ostrin_result.value = __ostrin_option.value; {retain_value}; }} else {{ __ostrin_result.ok = false; __ostrin_result.error = {error_code}; {retain_error}; }} __ostrin_result; }})"
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
                        "is_err" if args.is_empty() && *ty == Ty::Bool => {
                            format!("!({receiver}).ok")
                        }
                        "unwrap" if args.is_empty() && *ty == ok => {
                            let ok_c = c_type(&ok, records)?;
                            let retain = retain_payload("__ostrin_unwrapped", &ok, records)
                                .unwrap_or_default();
                            format!(
                                "({{ {result_name} __ostrin_result = {receiver}; if (!__ostrin_result.ok) {{ fprintf(stderr, \"ostrin: unwrap on Err\\n\"); exit(1); }} {ok_c} __ostrin_unwrapped = __ostrin_result.value; {retain}; __ostrin_unwrapped; }})"
                            )
                        }
                        "unwrap_or"
                            if args.len() == 1 && *ty == ok && value_ty(values, args[0])? == ok =>
                        {
                            let fallback = value_code(values, args[0])?;
                            if result_managed_payload(&ok, records) {
                                let ok_c = c_type(&ok, records)?;
                                let retain_value =
                                    retain_payload("__ostrin_unwrapped", &ok, records)
                                        .unwrap_or_default();
                                let retain_fallback =
                                    retain_payload(&fallback, &ok, records).unwrap_or_default();
                                format!(
                                    "({{ {result_name} __ostrin_result = {receiver}; {ok_c} __ostrin_unwrapped; if (__ostrin_result.ok) {{ __ostrin_unwrapped = __ostrin_result.value; {retain_value}; }} else {{ __ostrin_unwrapped = {fallback}; {retain_fallback}; }} __ostrin_unwrapped; }})"
                                )
                            } else {
                                format!(
                                    "({{ {result_name} __ostrin_result = {receiver}; __ostrin_result.ok ? __ostrin_result.value : {fallback}; }})"
                                )
                            }
                        }
                        "ok" if args.is_empty() && *ty == option_type(&ok) => {
                            let option_name =
                                format!("Option_{}", mangle_option_payload(&ok, records));
                            if option_managed_payload(&ok, records) {
                                let retain = retain_payload("__ostrin_option.value", &ok, records)
                                    .ok_or(())?;
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
                        "contains"
                            if args.len() == 1
                                && *ty == Ty::Bool
                                && value_ty(values, args[0])? == *element =>
                        {
                            format!(
                                "{set_name}_contains({receiver}, {})",
                                value_code(values, args[0])?
                            )
                        }
                        "add" | "remove"
                            if args.len() == 1
                                && *ty == Ty::Void
                                && value_ty(values, args[0])? == *element =>
                        {
                            format!(
                                "{set_name}_{method}({receiver}, {})",
                                value_code(values, args[0])?
                            )
                        }
                        "count" if args.is_empty() && *ty == Ty::Int => {
                            format!("{set_name}_count({receiver})")
                        }
                        _ => return Err(()),
                    }
                }
                Ty::Applied(name, task_args)
                    if name == "Task"
                        && task_args.len() == 1
                        && task_supported(&task_args[0], records) =>
                {
                    match method.as_str() {
                        "cancel" if args.is_empty() && *ty == Ty::Bool => {
                            let task_name =
                                format!("Task_{}", mangle_task_payload(&task_args[0], records));
                            format!("{task_name}_cancel({receiver})")
                        }
                        _ => return Err(()),
                    }
                }
                receiver_ty if record_name(&receiver_ty, records).is_some() => {
                    let record = record_name(&receiver_ty, records).ok_or(())?;
                    let c_name = methods.get(&(record, method.clone())).ok_or(())?;
                    if !supported(ty, records) {
                        return Err(());
                    }
                    let mut call_args = Vec::with_capacity(args.len() + 1);
                    call_args.push(receiver.clone());
                    for arg in args {
                        if !supported(&value_ty(values, *arg)?, records) {
                            return Err(());
                        }
                        call_args.push(value_code(values, *arg)?);
                    }
                    format!("{c_name}({})", call_args.join(", "))
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
            let call = if callee == "clone" && args.len() == 1 && *ty == value_ty(values, args[0])?
            {
                let arg_ty = value_ty(values, args[0])?;
                if !supported(&arg_ty, records) || arg_ty == Ty::Void {
                    return Err(());
                }
                if let Some(retain) = retain_payload("__ostrin_clone", &arg_ty, records) {
                    let ctype = c_type(&arg_ty, records)?;
                    format!(
                        "({{ {ctype} __ostrin_clone = {}; {retain}; __ostrin_clone; }})",
                        codes[0]
                    )
                } else {
                    codes[0].clone()
                }
            } else if callee == "drop" && args.len() == 1 && *ty == Ty::Void {
                let arg_ty = value_ty(values, args[0])?;
                if !supported(&arg_ty, records) {
                    return Err(());
                }
                if let Some(release) = release_payload(&codes[0], &arg_ty, records) {
                    format!("({{ {release}; (void)0; }})")
                } else {
                    "(void)0".to_string()
                }
            } else if callee == "args" && args.is_empty() && *ty == Ty::List(Box::new(Ty::String)) {
                "({ List_String* __ostrin_args = List_String_new_from_array((const char**)ostrin_argv, (int64_t)ostrin_argc); __ostrin_args; })".to_string()
            } else if callee == "env" && args.len() == 1 && value_ty(values, args[0])? == Ty::String
            {
                let Ty::Applied(name, option_args) = ty else {
                    return Err(());
                };
                if name != "Option" || option_args.as_slice() != [Ty::String] {
                    return Err(());
                }
                format!(
                    "({{ const char* __ostrin_env = getenv({}); Option_String __ostrin_option; memset(&__ostrin_option, 0, sizeof __ostrin_option); if (__ostrin_env) {{ __ostrin_option.has = true; __ostrin_option.value = ostrin_s_dup(__ostrin_env, strlen(__ostrin_env)); }} __ostrin_option; }})",
                    codes[0]
                )
            } else if callee == "cwd" && args.is_empty() && *ty == Ty::String {
                "ostrin_cwd()".to_string()
            } else if callee == "path_join"
                && args.len() == 2
                && args
                    .iter()
                    .all(|value| value_ty(values, *value) == Ok(Ty::String))
                && *ty == Ty::String
            {
                format!("ostrin_s_path_join({}, {})", codes[0], codes[1])
            } else if callee == "file_exists"
                && args.len() == 1
                && value_ty(values, args[0])? == Ty::String
                && *ty == Ty::Bool
            {
                format!("ostrin_file_exists({})", codes[0])
            } else if callee == "read_file"
                && args.len() == 1
                && value_ty(values, args[0])? == Ty::String
                && *ty == Ty::Applied("Result".to_string(), vec![Ty::String, Ty::String])
            {
                read_file_result_code(&codes[0])
            } else if callee == "write_file"
                && args.len() == 2
                && args
                    .iter()
                    .all(|value| value_ty(values, *value) == Ok(Ty::String))
                && *ty == Ty::Applied("Result".to_string(), vec![Ty::Void, Ty::String])
            {
                write_file_result_code(&codes[0], &codes[1])
            } else if callee == "format_float_value"
                && args.len() == 2
                && value_ty(values, args[0])? == Ty::Float
                && value_ty(values, args[1])? == Ty::Int
                && *ty == Ty::Applied("Result".to_string(), vec![Ty::String, Ty::String])
            {
                format_float_result_code(&codes[0], &codes[1])
            } else if callee == "yield" && args.is_empty() && *ty == Ty::Void {
                "({\n#if defined(OSTRIN_NATIVE_THREADS)\n    ostrin_select_wait();\n#else\n    (void)ostrin_poll_one();\n#endif\n    ostrin_task_checkpoint();\n    (void)0;\n})"
                    .to_string()
            } else if callee == "select" && args.len() == 1 {
                let Ty::List(element) = value_ty(values, args[0])? else {
                    return Err(());
                };
                let Ty::Applied(channel_name, channel_args) = element.as_ref() else {
                    return Err(());
                };
                if channel_name != "Channel"
                    || channel_args.len() != 1
                    || !channel_supported(&channel_args[0], records)
                    || *ty != option_type(&channel_args[0])
                {
                    return Err(());
                }
                let list_name = format!("List_{}", mangle_option_payload(&element, records));
                let channel_name = format!(
                    "Channel_{}",
                    mangle_option_payload(&channel_args[0], records)
                );
                let result_name = format!(
                    "Option_{}",
                    mangle_option_payload(&channel_args[0], records)
                );
                let element_c = c_type(&channel_args[0], records)?;
                let input = &codes[0];
                format!(
                    "({{ {list_name}* __ostrin_select_list = {input}; {result_name} __ostrin_select_result; memset(&__ostrin_select_result, 0, sizeof __ostrin_select_result); bool __ostrin_select_ready = false; if (!__ostrin_select_list || __ostrin_select_list->length == 0) OSTRIN_FAIL(\"select expects at least one channel\"); while (!__ostrin_select_ready) {{ if (ostrin_cancellation_requested()) {{ __ostrin_select_ready = true; }} else {{ for (int64_t __ostrin_select_index = 0; __ostrin_select_index < __ostrin_select_list->length; __ostrin_select_index++) {{ {element_c} __ostrin_select_value; int __ostrin_select_status = {channel_name}_try_receive(__ostrin_select_list->items[__ostrin_select_index], &__ostrin_select_value); if (__ostrin_select_status > 0) {{ __ostrin_select_result.has = true; __ostrin_select_result.value = __ostrin_select_value; __ostrin_select_ready = true; break; }} if (__ostrin_select_status < 0) {{ __ostrin_select_ready = true; break; }} }} if (!__ostrin_select_ready) {{\n#if defined(OSTRIN_NATIVE_THREADS)\n    ostrin_select_wait();\n#else\n    if (!ostrin_poll_all()) OSTRIN_FAIL(\"select would block: no runnable task remains\");\n#endif\n    }} }} }} ostrin_task_checkpoint(); __ostrin_select_result; }})"
                )
            } else if callee == "Some" && args.len() == 1 {
                let inner = value_ty(values, args[0])?;
                if !option_supported(&inner, records) || *ty != option_type(&inner) {
                    return Err(());
                }
                let option_name = format!("Option_{}", mangle_option_payload(&inner, records));
                if !option_managed_payload(&inner, records) {
                    format!("(({option_name}){{ .has = true, .value = {} }})", codes[0])
                } else {
                    let retain =
                        retain_payload("__ostrin_option.value", &inner, records).ok_or(())?;
                    format!(
                        "({{ {option_name} __ostrin_option = (({option_name}){{ .has = true, .value = {} }}); {retain}; __ostrin_option; }})",
                        codes[0]
                    )
                }
            } else if callee == "None" && args.is_empty() {
                let Ty::Applied(name, option_args) = ty else {
                    return Err(());
                };
                if name != "Option"
                    || option_args.len() != 1
                    || !option_supported(&option_args[0], records)
                {
                    return Err(());
                }
                let option_name =
                    format!("Option_{}", mangle_option_payload(&option_args[0], records));
                format!("(({option_name}){{ .has = false }})")
            } else if (callee == "Ok" || callee == "Err") && args.len() == 1 {
                let Ty::Applied(name, result_args) = ty else {
                    return Err(());
                };
                if name != "Result"
                    || result_args.len() != 2
                    || !result_supported(&result_args[0], &result_args[1], records)
                {
                    return Err(());
                }
                let expected = if callee == "Ok" {
                    &result_args[0]
                } else {
                    &result_args[1]
                };
                if value_ty(values, args[0])? != *expected {
                    return Err(());
                }
                let result_name = format!(
                    "Result_{}_{}",
                    mangle_result_payload(&result_args[0], records),
                    mangle_result_payload(&result_args[1], records)
                );
                let field = if callee == "Ok" { "value" } else { "error" };
                let active = if callee == "Ok" {
                    ".ok = true"
                } else {
                    ".ok = false"
                };
                if result_managed_payload(expected, records) {
                    let retain =
                        retain_payload(&format!("__ostrin_result.{field}"), expected, records)
                            .ok_or(())?;
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
                    let shown = helper(HelperRequest::Show, &codes[0], "", &arg_ty).ok_or(())?;
                    format!("({{ const char* __ostrin_shown = {shown}; printf(\"%s\\n\", __ostrin_shown); ostrin_release((void*)__ostrin_shown); }})")
                } else {
                    return Err(());
                }
            } else {
                let Some(c_function) = known_functions.get(callee) else {
                    return Err(());
                };
                for value in args {
                    if !supported(&value_ty(values, *value)?, records) {
                        return Err(());
                    }
                }
                format!("{}({})", c_function, codes.join(", "))
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
        IrInstr::ClosureCall {
            dst,
            callee,
            args,
            ty,
        } => {
            let Ty::Fn(params, ret) = value_ty(values, *callee)? else {
                return Err(());
            };
            if *ret != *ty || params.len() != args.len() || !supported(ty, records) {
                return Err(());
            }
            for (param, arg) in params.iter().zip(args) {
                if value_ty(values, *arg)? != *param || !supported(param, records) {
                    return Err(());
                }
            }
            let closure = value_code(values, *callee)?;
            let fn_type = closure_fn_type(&params, &ret, records)?;
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(format!("{closure}.env"));
            for arg in args {
                call_args.push(value_code(values, *arg)?);
            }
            let call = format!(
                "({{ OstrinClosure __ostrin_closure = {closure}; (({fn_type})__ostrin_closure.fn)({}); }})",
                call_args.join(", ")
            );
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
        IrInstr::ClosureMake {
            dst,
            closure,
            captures,
            ty,
        } => {
            let Ty::Fn(_, _) = ty else { return Err(()) };
            let helper = closure_helpers.get(closure).ok_or(())?;
            if helper.captures.len() != captures.len() {
                return Err(());
            }
            for ((_, capture_ty), capture) in helper.captures.iter().zip(captures) {
                if value_ty(values, *capture)? != *capture_ty || !supported(capture_ty, records) {
                    return Err(());
                }
            }
            let env = if let (Some(env_type), Some(drop_name)) =
                (&helper.env_type, &helper.drop_name)
            {
                let mut init = String::new();
                for ((name, capture_ty), capture) in helper.captures.iter().zip(captures) {
                    let field = format!("__ce->{name}");
                    init.push_str(&format!("{field} = {}; ", value_code(values, *capture)?));
                    if let Some(retain) = retain_payload(&field, capture_ty, records) {
                        init.push_str(&retain);
                        init.push_str("; ");
                    }
                }
                format!(
                    "({{ {env_type}* __ce = ({env_type}*)ostrin_calloc_with_drop(1, sizeof *__ce, (void (*)(void*)){drop_name}); {init}(void*)__ce; }})"
                )
            } else if helper.captures.is_empty() {
                "NULL".to_string()
            } else {
                return Err(());
            };
            out.push_str(&format!(
                "    {} = ((OstrinClosure){{ (void*){}, {} }});\n",
                value_name(*dst),
                helper.adapter,
                env
            ));
        }
        IrInstr::PatternTest {
            dst,
            subject,
            pattern,
        } => {
            let Ty::Applied(name, args) = value_ty(values, *subject)? else {
                return Err(());
            };
            let subject = value_code(values, *subject)?;
            let test = if name == "Option" && args.len() == 1 && option_supported(&args[0], records)
            {
                if pattern == "Ident(\"None\")" {
                    format!("!({subject}).has")
                } else if pattern.starts_with("Variant(\"Some\",") {
                    format!("({subject}).has")
                } else {
                    return Err(());
                }
            } else if name == "Result"
                && args.len() == 2
                && result_supported(&args[0], &args[1], records)
            {
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
        IrInstr::PatternBind {
            dst,
            subject,
            path,
            ty,
            ..
        } => {
            let subject_ty = value_ty(values, *subject)?;
            let (code, bound_ty) = if path.is_empty() {
                (value_code(values, *subject)?, subject_ty)
            } else {
                let Ty::Applied(name, args) = subject_ty else {
                    return Err(());
                };
                if name == "Option"
                    && args.len() == 1
                    && option_supported(&args[0], records)
                    && path.len() == 1
                {
                    (
                        format!("({}).value", value_code(values, *subject)?),
                        args[0].clone(),
                    )
                } else if name == "Result"
                    && args.len() == 2
                    && result_supported(&args[0], &args[1], records)
                    && path.len() == 2
                {
                    let field = match path[0].as_str() {
                        "Ok" => ("value", args[0].clone()),
                        "Err" => ("error", args[1].clone()),
                        _ => return Err(()),
                    };
                    (
                        format!("({}).{}", value_code(values, *subject)?, field.0),
                        field.1,
                    )
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
        IrInstr::ChannelNew { dst, capacity, ty } => {
            let Ty::Applied(name, args) = ty else {
                return Err(());
            };
            if name != "Channel" || args.len() != 1 || !channel_supported(&args[0], records) {
                return Err(());
            }
            if let Some(capacity) = capacity {
                if value_ty(values, *capacity)? != Ty::Int {
                    return Err(());
                }
            }
            let channel_name = format!("Channel_{}", mangle_option_payload(&args[0], records));
            out.push_str(&format!(
                "    {} = {channel_name}_new();\n",
                value_name(*dst)
            ));
        }
        IrInstr::Spawn {
            dst,
            region,
            scoped,
            ty,
        } => {
            let Ty::Applied(name, args) = ty else {
                return Err(());
            };
            if name != "Task" || args.len() != 1 || *scoped || !task_supported(&args[0], records) {
                return Err(());
            }
            let helper = spawn_helpers.get(region).ok_or(())?;
            let task_name = format!("Task_{}", mangle_task_payload(&args[0], records));
            let task = value_name(*dst);
            let env = if helper.captures.is_empty() {
                "NULL".to_string()
            } else {
                let env_type = helper.env_type.as_ref().ok_or(())?;
                let mut init = String::new();
                for (value, capture_ty) in &helper.captures {
                    init.push_str(&format!(
                        "__ce->__ir_v{value} = {}; ",
                        value_code(values, *value)?
                    ));
                    if let Some(retain) =
                        retain_payload(&format!("__ce->__ir_v{value}"), capture_ty, records)
                    {
                        init.push_str(&retain);
                        init.push_str("; ");
                    }
                }
                format!(
                    "({{ {env_type}* __ce = ({env_type}*)ostrin_alloc(sizeof *__ce); {init}(void*)__ce; }})"
                )
            };
            out.push_str(&format!(
                "    {task} = ({task_name}*)ostrin_calloc_with_drop(1, sizeof *{task}, (void (*)(void*)){task_name}_drop);\n"
            ));
            out.push_str(&format!(
                "    {task}->run = {}; {task}->env = {env}; {task}->drop_env = {}; {task}->group = ostrin_current_group();\n",
                helper.callback,
                helper.drop_env.as_deref().unwrap_or("NULL")
            ));
            out.push_str(&format!(
                "    ostrin_register_task({task}, {task_name}_poll, {task_name}_cancel_adapter); ostrin_track_task_handle({task});\n"
            ));
            out.push_str(&format!(
                "#if defined(OSTRIN_NATIVE_THREADS)\n    {task_name}_start({task});\n#endif\n"
            ));
        }
        IrInstr::ChannelSend { channel, value } => {
            let Ty::Applied(name, args) = value_ty(values, *channel)? else {
                return Err(());
            };
            if name != "Channel"
                || args.len() != 1
                || !channel_supported(&args[0], records)
                || value_ty(values, *value)? != args[0]
            {
                return Err(());
            }
            let channel_name = format!("Channel_{}", mangle_option_payload(&args[0], records));
            out.push_str(&format!(
                "    {channel_name}_send({}, {});\n",
                value_code(values, *channel)?,
                value_code(values, *value)?
            ));
        }
        IrInstr::ChannelReceive { dst, channel, ty } => {
            let Ty::Applied(channel_name, channel_args) = value_ty(values, *channel)? else {
                return Err(());
            };
            let Ty::Applied(option_name, option_args) = ty else {
                return Err(());
            };
            if channel_name != "Channel"
                || channel_args.len() != 1
                || option_name != "Option"
                || option_args.len() != 1
                || channel_args[0] != option_args[0]
                || !channel_supported(&channel_args[0], records)
            {
                return Err(());
            }
            let channel_c_name = format!(
                "Channel_{}",
                mangle_option_payload(&channel_args[0], records)
            );
            out.push_str(&format!(
                "    {} = {channel_c_name}_receive({});\n",
                value_name(*dst),
                value_code(values, *channel)?
            ));
        }
        IrInstr::ChannelClose { channel } => {
            let Ty::Applied(name, args) = value_ty(values, *channel)? else {
                return Err(());
            };
            if name != "Channel" || args.len() != 1 || !channel_supported(&args[0], records) {
                return Err(());
            }
            let channel_name = format!("Channel_{}", mangle_option_payload(&args[0], records));
            out.push_str(&format!(
                "    {channel_name}_close({});\n",
                value_code(values, *channel)?
            ));
        }
        IrInstr::TaskJoin { dst, task, ty } => {
            let Ty::Applied(name, args) = value_ty(values, *task)? else {
                return Err(());
            };
            if name != "Task"
                || args.len() != 1
                || args[0] != *ty
                || !task_supported(&args[0], records)
            {
                return Err(());
            }
            let task_name = format!("Task_{}", mangle_task_payload(&args[0], records));
            let call = format!("{task_name}_join({})", value_code(values, *task)?);
            if *ty == Ty::Void {
                out.push_str(&format!("    {call};\n"));
            } else {
                out.push_str(&format!("    {} = {call};\n", value_name(*dst)));
            }
        }
        IrInstr::Retain { value } => {
            let ty = value_ty(values, *value)?;
            match ty {
                Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => {
                    out.push_str(&format!(
                        "    ostrin_retain((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Named(name) if records.contains_key(&name) => {
                    out.push_str(&format!(
                        "    ostrin_retain((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Applied(_, _) if record_name(&ty, records).is_some() => {
                    out.push_str(&format!(
                        "    ostrin_retain((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Applied(name, args)
                    if name == "Channel"
                        && args.len() == 1
                        && channel_supported(&args[0], records) =>
                {
                    out.push_str(&format!(
                        "    ostrin_retain((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Applied(name, args)
                    if name == "Task" && args.len() == 1 && task_supported(&args[0], records) =>
                {
                    out.push_str(&format!(
                        "    ostrin_retain((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Applied(name, args)
                    if name == "Option"
                        && args.len() == 1
                        && option_managed_payload(&args[0], records) =>
                {
                    let code = value_code(values, *value)?;
                    let retain =
                        retain_payload(&format!("({code}).value"), &args[0], records).ok_or(())?;
                    out.push_str(&format!("    if (({code}).has) {{ {retain}; }}\n"));
                }
                Ty::Applied(name, args)
                    if name == "Result"
                        && args.len() == 2
                        && result_supported(&args[0], &args[1], records) =>
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
                Ty::Fn(_, _) => {
                    out.push_str(&format!(
                        "    ostrin_retain((void*)({}).env);\n",
                        value_code(values, *value)?
                    ));
                }
                _ => return Err(()),
            }
        }
        IrInstr::Release { value } => {
            let ty = value_ty(values, *value)?;
            match ty {
                Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => {
                    out.push_str(&format!(
                        "    ostrin_release((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Named(name) if records.contains_key(&name) => {
                    out.push_str(&format!(
                        "    ostrin_release((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Applied(_, _) if record_name(&ty, records).is_some() => {
                    out.push_str(&format!(
                        "    ostrin_release((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Applied(name, args)
                    if name == "Channel"
                        && args.len() == 1
                        && channel_supported(&args[0], records) =>
                {
                    out.push_str(&format!(
                        "    ostrin_release((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Applied(name, args)
                    if name == "Task" && args.len() == 1 && task_supported(&args[0], records) =>
                {
                    out.push_str(&format!(
                        "    ostrin_release((void*){});\n",
                        value_code(values, *value)?
                    ));
                }
                Ty::Applied(name, args)
                    if name == "Option"
                        && args.len() == 1
                        && option_managed_payload(&args[0], records) =>
                {
                    let code = value_code(values, *value)?;
                    let release =
                        release_payload(&format!("({code}).value"), &args[0], records).ok_or(())?;
                    out.push_str(&format!("    if (({code}).has) {{ {release}; }}\n"));
                }
                Ty::Applied(name, args)
                    if name == "Result"
                        && args.len() == 2
                        && result_supported(&args[0], &args[1], records) =>
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
                Ty::Fn(_, _) => {
                    out.push_str(&format!(
                        "    ostrin_release((void*)({}).env);\n",
                        value_code(values, *value)?
                    ));
                }
                _ => return Err(()),
            }
        }
        IrInstr::Opaque {
            dst: None,
            op,
            inputs,
            ty,
        } if inputs.is_empty() && *ty == Ty::Void => {
            if let Some(scope) = op
                .strip_prefix("scope_begin<")
                .and_then(|value| value.strip_suffix('>'))
            {
                let scope = scope.parse::<usize>().map_err(|_| ())?;
                out.push_str(&format!(
                    "    __ostrin_ir_scope_{scope} = ostrin_scope_begin();\n"
                ));
            } else if let Some(scope) = op
                .strip_prefix("scope_end<")
                .and_then(|value| value.strip_suffix('>'))
            {
                let scope = scope.parse::<usize>().map_err(|_| ())?;
                out.push_str(&format!(
                    "    ostrin_scope_end(__ostrin_ir_scope_{scope});\n"
                ));
            } else {
                return Err(());
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

fn emit_region_terminator(
    block: BlockId,
    terminator: &IrTerminator,
    region_blocks: &HashSet<BlockId>,
    values: &Values,
    result_ty: &Ty,
    captures: &[(ValueId, Ty)],
    records: &RecordFields,
    out: &mut String,
) -> Bail<()> {
    let valid_target = |target: BlockId| region_blocks.contains(&target);
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
            if !valid_target(*then_block)
                || !valid_target(*else_block)
                || value_ty(values, *condition)? != Ty::Bool
            {
                return Err(());
            }
            let condition = value_code(values, *condition)?;
            out.push_str(&format!(
                "    if ({condition}) {{ __ostrin_ir_pred = {block}; goto {}; }} else {{ __ostrin_ir_pred = {block}; goto {}; }}\n",
                block_label(*then_block),
                block_label(*else_block)
            ));
        }
        IrTerminator::RegionReturn(value) => match value {
            Some(value) => {
                if value_ty(values, *value)? != *result_ty {
                    return Err(());
                }
                if *result_ty == Ty::Void {
                    out.push_str("    return;\n");
                } else {
                    let return_code = value_code(values, *value)?;
                    if captures.iter().any(|(captured, _)| *captured == *value)
                        && retain_payload(&return_code, result_ty, records).is_some()
                    {
                        out.push_str(&format!(
                            "    {};\n",
                            retain_payload(&return_code, result_ty, records).ok_or(())?
                        ));
                    }
                    out.push_str(&format!("    return {return_code};\n"));
                }
            }
            None if *result_ty == Ty::Void => out.push_str("    return;\n"),
            None => return Err(()),
        },
        IrTerminator::Unreachable => out.push_str("    abort();\n"),
        IrTerminator::Return(_) => return Err(()),
    }
    Ok(())
}

fn used_values(instruction: &IrInstr) -> Vec<ValueId> {
    match instruction {
        IrInstr::Move { source, .. } => vec![*source],
        IrInstr::StoreLocal { value, .. } => vec![*value],
        IrInstr::Unary { operand, .. } => vec![*operand],
        IrInstr::Binary { left, right, .. } => vec![*left, *right],
        IrInstr::Call { args, .. } => args.clone(),
        IrInstr::ClosureCall { callee, args, .. } => std::iter::once(*callee)
            .chain(args.iter().copied())
            .collect(),
        IrInstr::ClosureMake { captures, .. } => captures.clone(),
        IrInstr::MethodCall { receiver, args, .. } => std::iter::once(*receiver)
            .chain(args.iter().copied())
            .collect(),
        IrInstr::Field { object, .. } => vec![*object],
        IrInstr::FieldStore { object, value, .. } => vec![*object, *value],
        IrInstr::Index { object, index, .. } => vec![*object, *index],
        IrInstr::Aggregate { fields, .. } => fields.clone(),
        IrInstr::IterInit { source, .. } => vec![*source],
        IrInstr::IterHasNext { iter, .. } | IrInstr::IterNext { iter, .. } => vec![*iter],
        IrInstr::PatternTest { subject, .. } | IrInstr::PatternBind { subject, .. } => {
            vec![*subject]
        }
        IrInstr::TryCheck { value, .. }
        | IrInstr::TryValue { value, .. }
        | IrInstr::TryError { value, .. }
        | IrInstr::TryErrorValue { value, .. } => vec![*value],
        IrInstr::Spawn { .. } => Vec::new(),
        IrInstr::ChannelNew { capacity, .. } => capacity.iter().copied().collect(),
        IrInstr::ChannelSend { channel, value } => vec![*channel, *value],
        IrInstr::ChannelReceive { channel, .. } => vec![*channel],
        IrInstr::ChannelClose { channel } => vec![*channel],
        IrInstr::TaskJoin { task, .. } => vec![*task],
        IrInstr::Phi { incoming, .. } => incoming.iter().map(|(_, value)| *value).collect(),
        IrInstr::Opaque { inputs, .. } => inputs.clone(),
        IrInstr::Retain { value } | IrInstr::Release { value } => vec![*value],
        IrInstr::Param { .. } | IrInstr::Const { .. } | IrInstr::Global { .. } => Vec::new(),
    }
}

fn terminator_values(terminator: &IrTerminator) -> Vec<ValueId> {
    match terminator {
        IrTerminator::Branch { condition, .. } => vec![*condition],
        IrTerminator::Return(Some(value)) | IrTerminator::RegionReturn(Some(value)) => vec![*value],
        IrTerminator::Goto(_)
        | IrTerminator::Return(None)
        | IrTerminator::RegionReturn(None)
        | IrTerminator::Unreachable => Vec::new(),
    }
}

fn reachable_region_blocks(function: &IrFunction, root: BlockId) -> Bail<Vec<BlockId>> {
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    while let Some(block_id) = pending.pop() {
        if !seen.insert(block_id) {
            continue;
        }
        let block = function.blocks.get(block_id).ok_or(())?;
        let terminator = block.terminator.as_ref().ok_or(())?;
        match terminator {
            IrTerminator::Goto(target) => pending.push(*target),
            IrTerminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                pending.push(*then_block);
                pending.push(*else_block);
            }
            IrTerminator::RegionReturn(_) | IrTerminator::Unreachable => {}
            IrTerminator::Return(_) => return Err(()),
        }
    }
    let mut blocks: Vec<_> = seen.into_iter().collect();
    blocks.sort_unstable();
    Ok(blocks)
}

fn build_closure_adapters(
    function: &IrFunction,
    known_functions: &FunctionNames,
    records: &RecordFields,
) -> Bail<Vec<(String, String)>> {
    let mut targets = HashSet::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let IrInstr::Global {
                name,
                ty: Ty::Fn(_, _),
                ..
            } = instruction
            {
                targets.insert(name.clone());
            }
        }
    }

    let mut adapters = Vec::new();
    let mut targets: Vec<_> = targets.into_iter().collect();
    targets.sort();
    for target in targets {
        let c_target = known_functions.get(&target).ok_or(())?;
        let adapter = closure_adapter_name(&function.name, &target);
        let Ty::Fn(params, ret) = function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .find_map(|instruction| match instruction {
                IrInstr::Global { name, ty, .. } if *name == target => Some(ty.clone()),
                _ => None,
            })
            .ok_or(())?
        else {
            return Err(());
        };
        let c_params = params
            .iter()
            .enumerate()
            .map(|(index, ty)| Ok(format!("{} arg{index}", c_type(ty, records)?)))
            .collect::<Bail<Vec<_>>>()?;
        let signature = format!(
            "static {} {adapter}(void* __env{} )",
            c_type(&ret, records)?,
            if c_params.is_empty() {
                String::new()
            } else {
                format!(", {}", c_params.join(", "))
            }
        );
        let mut body = String::from("    (void)__env;\n");
        let call_args = (0..params.len())
            .map(|index| format!("arg{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        if ret.as_ref() == &Ty::Void {
            body.push_str(&format!("    {c_target}({call_args});\n"));
        } else {
            body.push_str(&format!("    return {c_target}({call_args});\n"));
        }
        adapters.push((signature, body));
    }
    Ok(adapters)
}

pub struct Generated {
    pub body: String,
    pub declarations: Vec<String>,
    pub helpers: Vec<(String, String)>,
}

fn build_spawn_helpers(
    function: &IrFunction,
    values: &Values,
    known_functions: &FunctionNames,
    methods: &MethodNames,
    records: &RecordFields,
    spawn_helpers: &mut SpawnHelpers,
    closure_helpers: &ClosureHelpers,
    helper: &mut HelperGenerator<'_>,
) -> Bail<(Vec<String>, Vec<(String, String)>)> {
    let mut specs = Vec::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            let IrInstr::Spawn {
                region, scoped, ty, ..
            } = instruction
            else {
                continue;
            };
            let Ty::Applied(name, args) = ty else {
                return Err(());
            };
            if name != "Task" || args.len() != 1 || *scoped || !task_supported(&args[0], records) {
                return Err(());
            }
            if spawn_helpers.contains_key(region) {
                return Err(());
            }
            let callback = format!(
                "ostrin_ir_task_{}_{}",
                crate::codegen::c_function_name(&function.name),
                region
            );
            spawn_helpers.insert(
                *region,
                SpawnHelper {
                    callback,
                    env_type: None,
                    drop_env: None,
                    captures: Vec::new(),
                },
            );
            specs.push((*region, args[0].clone()));
        }
    }

    specs.sort_by_key(|(region, _)| std::cmp::Reverse(*region));
    let mut declarations = Vec::new();
    let mut helpers = Vec::with_capacity(specs.len());
    for (region, result_ty) in specs {
        let region_blocks = reachable_region_blocks(function, region)?;
        let region_set: HashSet<_> = region_blocks.iter().copied().collect();
        let mut definitions = HashSet::new();
        let mut has_return = false;
        for block_id in &region_blocks {
            let block = function.blocks.get(*block_id).ok_or(())?;
            for instruction in &block.instructions {
                if matches!(instruction, IrInstr::Param { .. }) {
                    return Err(());
                }
                if matches!(instruction, IrInstr::Opaque { op, .. } if op.starts_with("scope_begin<") || op.starts_with("scope_end<"))
                {
                    return Err(());
                }
                if let Some((value, ty)) = defined_value(instruction) {
                    if !supported(&ty, records) || !definitions.insert(value) {
                        return Err(());
                    }
                }
            }
            if matches!(block.terminator, Some(IrTerminator::RegionReturn(_))) {
                has_return = true;
            }
        }
        if !has_return {
            return Err(());
        }
        let mut captured_values = HashSet::new();
        for block_id in &region_blocks {
            let block = function.blocks.get(*block_id).ok_or(())?;
            for instruction in &block.instructions {
                for value in used_values(instruction) {
                    if !definitions.contains(&value) {
                        captured_values.insert(value);
                    }
                }
                if let IrInstr::Spawn { region: nested, .. } = instruction {
                    let nested_helper = spawn_helpers.get(nested).ok_or(())?;
                    for (value, _) in &nested_helper.captures {
                        if !definitions.contains(value) {
                            captured_values.insert(*value);
                        }
                    }
                }
            }
            let terminator = block.terminator.as_ref().ok_or(())?;
            for value in terminator_values(terminator) {
                if !definitions.contains(&value) {
                    captured_values.insert(value);
                }
            }
        }
        captured_values.retain(|value| !definitions.contains(value));
        let mut captures: Vec<(ValueId, Ty)> = captured_values
            .into_iter()
            .map(|value| {
                values
                    .get(&value)
                    .map(|(_, ty)| (value, ty.clone()))
                    .ok_or(())
            })
            .collect::<Bail<Vec<_>>>()?;
        captures.sort_by_key(|(value, _)| *value);
        if captures
            .iter()
            .any(|(_, ty)| *ty == Ty::Void || !supported(ty, records))
        {
            return Err(());
        }
        for block_id in &region_blocks {
            let terminator = function.blocks[*block_id].terminator.as_ref().ok_or(())?;
            match terminator {
                IrTerminator::RegionReturn(Some(value))
                    if value_ty(values, *value)? != result_ty =>
                {
                    return Err(())
                }
                IrTerminator::RegionReturn(None) if result_ty != Ty::Void => return Err(()),
                _ => {}
            }
        }

        let helper_name = format!(
            "ostrin_ir_task_{}_{}",
            crate::codegen::c_function_name(&function.name),
            region
        );
        let env_type = (!captures.is_empty()).then(|| {
            format!(
                "OstrinIrTaskEnv_{}_{}",
                crate::codegen::c_function_name(&function.name),
                region
            )
        });
        let drop_name = env_type.as_ref().map(|_| {
            format!(
                "ostrin_ir_task_env_drop_{}_{}",
                crate::codegen::c_function_name(&function.name),
                region
            )
        });
        if let Some(env_type) = &env_type {
            let fields = captures
                .iter()
                .map(|(value, ty)| {
                    c_type(ty, records).map(|ctype| format!("{ctype} __ir_v{value};"))
                })
                .collect::<Bail<Vec<_>>>()?;
            declarations.push(format!(
                "typedef struct {{ {} }} {env_type};",
                fields.join(" ")
            ));
            declarations.push(format!(
                "static void {}(void* __env);",
                drop_name.as_deref().ok_or(())?
            ));
            let mut drop_body = format!("    {env_type}* __e = __env;\n");
            for (value, ty) in &captures {
                if let Some(release) = release_payload(&format!("__e->__ir_v{value}"), ty, records)
                {
                    drop_body.push_str("    ");
                    drop_body.push_str(&release);
                    drop_body.push_str(";\n");
                }
            }
            drop_body.push_str("    ostrin_release((void*)__env);\n");
            helpers.push((
                format!(
                    "static void {}(void* __env)",
                    drop_name.as_deref().ok_or(())?
                ),
                drop_body,
            ));
        }
        spawn_helpers.insert(
            region,
            SpawnHelper {
                callback: helper_name.clone(),
                env_type: env_type.clone(),
                drop_env: drop_name.clone(),
                captures: captures.clone(),
            },
        );
        let signature = format!(
            "static {} {helper_name}(void* __env)",
            c_type(&result_ty, records)?
        );
        let mut helper_values = values.clone();
        for (value, ty) in &captures {
            helper_values.insert(*value, (format!("__e->__ir_v{value}"), ty.clone()));
        }
        let mut body = if let Some(env_type) = &env_type {
            format!("    {env_type}* __e = __env;\n")
        } else {
            String::from("    (void)__env;\n")
        };
        let mut declarations: Vec<(ValueId, Ty)> = definitions
            .iter()
            .filter_map(|value| values.get(value).map(|(_, ty)| (*value, ty.clone())))
            .filter(|(_, ty)| *ty != Ty::Void)
            .collect();
        declarations.sort_by_key(|(value, _)| *value);
        for (value, ty) in declarations {
            body.push_str(&format!(
                "    {} {};\n",
                c_type(&ty, records)?,
                value_name(value)
            ));
        }
        body.push_str("    int __ostrin_ir_pred = -1;\n");
        body.push_str(&format!("    goto {};\n", block_label(region)));
        for block_id in &region_blocks {
            let block = function.blocks.get(*block_id).ok_or(())?;
            body.push_str(&format!("{}:\n", block_label(block.id)));
            for instruction in &block.instructions {
                emit_instruction(
                    instruction,
                    &helper_values,
                    &function.name,
                    known_functions,
                    methods,
                    records,
                    spawn_helpers,
                    closure_helpers,
                    helper,
                    &mut body,
                )?;
            }
            emit_region_terminator(
                block.id,
                block.terminator.as_ref().ok_or(())?,
                &region_set,
                &helper_values,
                &result_ty,
                &captures,
                records,
                &mut body,
            )?;
        }
        body.push_str("    abort();\n");
        helpers.push((signature, body));
    }
    Ok((declarations, helpers))
}

fn build_closure_helpers(
    function: &IrFunction,
    known_functions: &FunctionNames,
    methods: &MethodNames,
    records: &RecordFields,
    helper: &mut HelperGenerator<'_>,
) -> Bail<(ClosureHelpers, Vec<String>, Vec<(String, String)>)> {
    let mut closure_helpers = ClosureHelpers::new();
    let mut declarations = Vec::new();
    let mut helpers = Vec::new();

    for (index, closure) in function.closures.iter().enumerate() {
        if closure.captures.len() > closure.body.params.len()
            || closure
                .captures
                .iter()
                .zip(&closure.body.params)
                .any(|((name, ty), (body_name, body_ty))| name != body_name || ty != body_ty)
        {
            return Err(());
        }
        let (lowered_body_program, _) = crate::ownership::lower_linear(&crate::ir::IrProgram {
            functions: vec![closure.body.clone()],
        });
        let lowered_body = lowered_body_program
            .functions
            .into_iter()
            .next()
            .ok_or(())?;
        let generated =
            generate_with_helpers(&lowered_body, known_functions, methods, records, helper)
                .ok_or(())?;
        declarations.extend(generated.declarations);
        helpers.extend(generated.helpers);

        let base = crate::codegen::c_function_name(&closure.name);
        let body_name = format!("ostrin_ir_closure_body_{base}");
        let adapter = format!("ostrin_ir_closure_{base}");
        let env_type = (!closure.captures.is_empty()).then(|| format!("OstrinIrClosureEnv_{base}"));
        let drop_name = env_type
            .as_ref()
            .map(|_| format!("ostrin_ir_closure_drop_{base}"));

        let body_params = closure
            .body
            .params
            .iter()
            .map(|(name, ty)| Ok(format!("{} {name}", c_type(ty, records)?)))
            .collect::<Bail<Vec<_>>>()?;
        let body_param_list = if body_params.is_empty() {
            "void".to_string()
        } else {
            body_params.join(", ")
        };
        let body_signature = format!(
            "static {} {body_name}({body_param_list})",
            c_type(&closure.ret, records)?
        );
        declarations.push(format!("{body_signature};"));
        helpers.push((body_signature, generated.body));

        if let Some(env_type) = &env_type {
            let fields = closure
                .captures
                .iter()
                .map(|(name, ty)| Ok(format!("{} {name};", c_type(ty, records)?)))
                .collect::<Bail<Vec<_>>>()?;
            declarations.push(format!(
                "typedef struct {{ {} }} {env_type};",
                fields.join(" ")
            ));
            let drop_name = drop_name.as_deref().ok_or(())?;
            declarations.push(format!("static void {drop_name}(void* __env);"));
            let mut drop_body = format!("    {env_type}* __e = __env;\n");
            for (name, ty) in &closure.captures {
                if let Some(release) = release_payload(&format!("__e->{name}"), ty, records) {
                    drop_body.push_str("    ");
                    drop_body.push_str(&release);
                    drop_body.push_str(";\n");
                }
            }
            helpers.push((format!("static void {drop_name}(void* __env)"), drop_body));
        }

        let lambda_params = closure.body.params.iter().skip(closure.captures.len());
        let adapter_params = lambda_params
            .map(|(name, ty)| Ok(format!(", {} {name}", c_type(ty, records)?)))
            .collect::<Bail<Vec<_>>>()?
            .join("");
        let adapter_signature = format!(
            "static {} {adapter}(void* __env{adapter_params})",
            c_type(&closure.ret, records)?
        );
        declarations.push(format!("{adapter_signature};"));
        let mut adapter_body = String::new();
        if let Some(env_type) = &env_type {
            adapter_body.push_str(&format!("    {env_type}* __e = __env;\n"));
        } else {
            adapter_body.push_str("    (void)__env;\n");
        }
        let call_args = closure
            .captures
            .iter()
            .map(|(name, _)| {
                env_type
                    .as_ref()
                    .map(|_| format!("__e->{name}"))
                    .unwrap_or_else(|| name.clone())
            })
            .chain(
                closure
                    .body
                    .params
                    .iter()
                    .skip(closure.captures.len())
                    .map(|(name, _)| name.clone()),
            )
            .collect::<Vec<_>>()
            .join(", ");
        let call = format!("{body_name}({call_args})");
        if closure.ret == Ty::Void {
            adapter_body.push_str(&format!("    {call};\n"));
        } else {
            adapter_body.push_str(&format!("    return {call};\n"));
        }
        helpers.push((adapter_signature, adapter_body));

        closure_helpers.insert(
            index,
            ClosureHelper {
                adapter,
                env_type,
                drop_name,
                captures: closure.captures.clone(),
            },
        );
    }
    Ok((closure_helpers, declarations, helpers))
}

/// Emits an IR function when all of its values use a supported scalar or
/// collection representation and its CFG can be represented with ordinary C
/// labels and gotos.
#[allow(dead_code)]
pub fn generate(
    function: &IrFunction,
    known_functions: &FunctionNames,
    methods: &MethodNames,
    records: &RecordFields,
    show: &mut dyn FnMut(&str, &Ty) -> Option<String>,
) -> Option<String> {
    let mut helper = |request: HelperRequest, left: &str, _right: &str, ty: &Ty| match request {
        HelperRequest::Show => show(left, ty),
        HelperRequest::Equality => None,
    };
    generate_with_helpers(function, known_functions, methods, records, &mut helper)
        .map(|generated| generated.body)
}

pub fn generate_with_helpers(
    function: &IrFunction,
    known_functions: &FunctionNames,
    methods: &MethodNames,
    records: &RecordFields,
    helper: &mut HelperGenerator<'_>,
) -> Option<Generated> {
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
    let mut spawn_helpers = SpawnHelpers::new();
    let mut helper_declarations = Vec::new();
    let (closure_helpers, closure_declarations, mut helpers) =
        build_closure_helpers(function, known_functions, methods, records, helper).ok()?;
    helper_declarations.extend(closure_declarations);
    helpers.extend(build_closure_adapters(function, known_functions, records).ok()?);
    let (spawn_declarations, spawn_helper_bodies) = build_spawn_helpers(
        function,
        &values,
        known_functions,
        methods,
        records,
        &mut spawn_helpers,
        &closure_helpers,
        helper,
    )
    .ok()?;
    helper_declarations.extend(spawn_declarations);
    helpers.extend(spawn_helper_bodies);
    let mut region_blocks = HashSet::new();
    for root in spawn_helpers.keys().copied() {
        region_blocks.extend(reachable_region_blocks(function, root).ok()?);
    }
    if function.blocks.iter().any(|block| {
        matches!(block.terminator, Some(IrTerminator::RegionReturn(_)))
            && !region_blocks.contains(&block.id)
    }) {
        return None;
    }
    let mut out = String::new();
    out.push_str("    int __ostrin_ir_pred = -1;\n");
    let mut scopes = HashSet::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let IrInstr::Opaque {
                dst: None,
                op,
                inputs,
                ty,
            } = instruction
            {
                if inputs.is_empty() && *ty == Ty::Void {
                    if let Some(scope) = op
                        .strip_prefix("scope_begin<")
                        .and_then(|value| value.strip_suffix('>'))
                    {
                        scopes.insert(scope.parse::<usize>().ok()?);
                    }
                }
            }
        }
    }
    let mut scopes: Vec<_> = scopes.into_iter().collect();
    scopes.sort_unstable();
    for scope in scopes {
        out.push_str(&format!(
            "    OstrinTaskGroup* __ostrin_ir_scope_{scope};\n"
        ));
    }
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
        if region_blocks.contains(&block.id) {
            continue;
        }
        out.push_str(&format!("{}:\n", block_label(block.id)));
        for instruction in &block.instructions {
            emit_instruction(
                instruction,
                &values,
                &function.name,
                known_functions,
                methods,
                records,
                &spawn_helpers,
                &closure_helpers,
                helper,
                &mut out,
            )
            .ok()?;
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
    Some(Generated {
        body: out,
        declarations: helper_declarations,
        helpers,
    })
}
