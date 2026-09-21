//! Conservative ownership facts over the explicit IR.
//!
//! This pass reports where retain/release insertion is safe to start. It does
//! not mutate the program yet: a value used across blocks or through an opaque
//! instruction must first go through the CFG/data-flow work that will make
//! automatic release correct at joins and loops.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

use crate::ast::{Item, Type};
use crate::ir::{IrInstr, IrProgram, IrTerminator, ValueId};
use crate::types::Ty;

#[derive(Debug, Default, Clone)]
pub struct OwnershipReport {
    pub functions: usize,
    pub managed_values: usize,
    pub last_use_candidates: usize,
    pub cross_block_values: usize,
    pub opaque_barriers: usize,
    pub unused_managed_values: usize,
    pub facts: Vec<OwnershipFact>,
}

#[derive(Debug, Clone)]
pub struct OwnershipFact {
    pub function: String,
    pub value: ValueId,
    pub ty: Ty,
    pub uses: usize,
    pub last_block: Option<usize>,
    pub last_instruction: Option<usize>,
    pub candidate: bool,
}

#[derive(Debug, Default, Clone)]
pub struct LoweringSummary {
    pub inserted_retains: usize,
    pub inserted_releases: usize,
    pub unresolved_values: usize,
    pub functions: usize,
}

#[derive(Debug, Clone)]
pub struct MoveViolation {
    pub function: String,
    pub value: ValueId,
    pub ty: Ty,
    pub send_block: usize,
    pub send_instruction: usize,
    pub use_block: usize,
    pub use_instruction: usize,
}

#[derive(Debug, Clone)]
struct Definition {
    ty: Ty,
    block: usize,
}

#[derive(Debug, Clone, Copy)]
struct UsePoint {
    block: usize,
    instruction: usize,
}

pub fn analyze(program: &IrProgram) -> OwnershipReport {
    let mut report = OwnershipReport { functions: program.functions.len(), ..OwnershipReport::default() };
    for function in &program.functions {
        analyze_function(function, &mut report);
    }
    report
}

/// Adds only releases whose last use is provably in one straight-line block.
/// The returned program is still an analysis artifact: the native backend does
/// not consume it yet, so this pass cannot accidentally change executable
/// behavior while the ownership contract is being completed.
pub fn lower_linear(program: &IrProgram) -> (IrProgram, LoweringSummary) {
    let mut lowered = program.clone();
    let mut summary = LoweringSummary { functions: lowered.functions.len(), ..LoweringSummary::default() };

    for function in &mut lowered.functions {
        let mut definitions: HashMap<ValueId, (Ty, usize, usize)> = HashMap::new();
        let mut uses: HashMap<ValueId, Vec<UsePoint>> = HashMap::new();
        let mut opaque_values = HashSet::new();
        let mut borrowed_values = HashSet::new();

        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                if let Some((value, ty)) = defined_value(instruction) {
                    definitions.insert(value, (ty, block.id, index));
                    if matches!(instruction, IrInstr::Param { .. }) {
                        borrowed_values.insert(value);
                    }
                }
                for value in used_values(instruction) {
                    uses.entry(value).or_default().push(UsePoint { block: block.id, instruction: index });
                }
                if matches!(instruction, IrInstr::Opaque { .. }) {
                    for value in used_values(instruction) {
                        opaque_values.insert(value);
                    }
                }
            }
            if let Some(terminator) = &block.terminator {
                for value in terminator_values(terminator) {
                    uses.entry(value).or_default().push(UsePoint {
                        block: block.id,
                        instruction: block.instructions.len(),
                    });
                }
            }
        }

        let mut retain_before: HashMap<(usize, usize), Vec<ValueId>> = HashMap::new();
        let mut retain_after: HashMap<(usize, usize), Vec<ValueId>> = HashMap::new();
        let mut release_after: HashMap<(usize, usize), Vec<ValueId>> = HashMap::new();
        let mut release_before_terminator: HashMap<usize, Vec<ValueId>> = HashMap::new();
        for (value, (ty, definition_block, definition_instruction)) in &definitions {
            if !requires_management(ty) {
                continue;
            }
            // Function parameters are borrowed C arguments. The caller owns
            // their reference, so a callee must not release a parameter just
            // because its last local use is visible in this function.
            if borrowed_values.contains(value) {
                continue;
            }
            let value_uses = uses.get(&value).cloned().unwrap_or_default();
            let blocks: HashSet<usize> = value_uses.iter().map(|point| point.block).collect();
            let candidate = !value_uses.is_empty() && blocks.len() == 1 && !opaque_values.contains(&value);
            if candidate {
                let last = value_uses
                    .iter()
                    .max_by_key(|point| point.instruction)
                    .copied()
                    .expect("candidate has a use");
                // A return transfers the value to the caller; releasing after
                // its terminator would be a use-after-release. All other
                // terminators are treated as unresolved for now.
                let block = &function.blocks[last.block];
                if last.instruction < block.instructions.len() && safe_release_site(&block.instructions[last.instruction]) {
                    release_after.entry((last.block, last.instruction + 1)).or_default().push(*value);
                } else {
                    summary.unresolved_values += 1;
                }
            } else if value_uses.is_empty() {
                release_after.entry((*definition_block, definition_instruction + 1)).or_default().push(*value);
            } else {
                summary.unresolved_values += 1;
            }
        }

        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                if let IrInstr::Aggregate { fields, ty, .. } = instruction {
                    // Native collection constructors and mutators retain
                    // reference elements themselves. Other aggregate paths
                    // still need the explicit IR retain until their backend
                    // contracts are migrated.
                    if !matches!(ty, Ty::List(_) | Ty::Map(_, _) | Ty::Set(_)) {
                        for value in fields {
                            if definitions.get(value).is_some_and(|(ty, _, _)| requires_management(ty)) {
                                retain_before.entry((block.id, index)).or_default().push(*value);
                            }
                        }
                    }
                }
                if let Some((value, _)) = alias_destination(instruction).filter(|(_, ty)| requires_management(ty)) {
                    let transfers_return = matches!(instruction, IrInstr::Phi { .. }) && is_single_return_use(function, value, &uses);
                    if !transfers_return {
                        retain_after.entry((block.id, index + 1)).or_default().push(value);
                    }
                    if let IrInstr::Phi { incoming, .. } = instruction {
                        if !transfers_return {
                            for (predecessor, incoming_value) in incoming {
                                if definitions
                                    .get(incoming_value)
                                    .is_some_and(|(incoming_ty, _, _)| requires_management(incoming_ty))
                                    && is_single_phi_use(*incoming_value, block.id, index, &uses)
                                {
                                    release_before_terminator.entry(*predecessor).or_default().push(*incoming_value);
                                }
                            }
                        }
                    }
                }
            }
        }

        for block in &mut function.blocks {
            let old = std::mem::take(&mut block.instructions);
            let mut instructions = Vec::with_capacity(old.len());
            for (index, instruction) in old.into_iter().enumerate() {
                if let Some(values) = retain_before.remove(&(block.id, index)) {
                    for value in values {
                        instructions.push(IrInstr::Retain { value });
                        summary.inserted_retains += 1;
                    }
                }
                instructions.push(instruction);
                if let Some(values) = retain_after.remove(&(block.id, index + 1)) {
                    for value in values {
                        instructions.push(IrInstr::Retain { value });
                        summary.inserted_retains += 1;
                    }
                }
                if let Some(values) = release_after.remove(&(block.id, index + 1)) {
                    for value in values {
                        instructions.push(IrInstr::Release { value });
                        summary.inserted_releases += 1;
                    }
                }
            }
            block.instructions = instructions;
            if let Some(values) = release_before_terminator.remove(&block.id) {
                for value in values {
                    block.instructions.push(IrInstr::Release { value });
                    summary.inserted_releases += 1;
                }
            }
        }
    }

    (lowered, summary)
}

/// Finds values that are sent through a channel and then used again in the
/// same function. This is deliberately conservative: only values with an
/// aggregate/reference-like type are considered movable, while scalar and
/// value-only `Option`/`Result` data remain copyable.
pub fn check_moves(program: &IrProgram) -> Vec<MoveViolation> {
    check_moves_impl(program, None)
}

pub fn check_moves_for_types(program: &IrProgram, movable_types: &HashSet<String>) -> Vec<MoveViolation> {
    check_moves_impl(program, Some(movable_types))
}

fn check_moves_impl(program: &IrProgram, movable_types: Option<&HashSet<String>>) -> Vec<MoveViolation> {
    let mut violations = Vec::new();
    for function in &program.functions {
        let mut definitions: HashMap<ValueId, Ty> = HashMap::new();
        let mut moved: HashMap<ValueId, (Ty, usize, usize)> = HashMap::new();
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                if let Some((value, ty)) = defined_value(instruction) {
                    definitions.insert(value, ty);
                }
                for value in used_values(instruction) {
                    if let Some((ty, send_block, send_instruction)) = moved.get(&value) {
                        violations.push(MoveViolation {
                            function: function.name.clone(),
                            value,
                            ty: ty.clone(),
                            send_block: *send_block,
                            send_instruction: *send_instruction,
                            use_block: block.id,
                            use_instruction: index,
                        });
                    }
                }
                if let IrInstr::ChannelSend { value, .. } = instruction {
                    if let Some(ty) = definitions
                        .get(value)
                        .cloned()
                        .filter(|ty| is_move_type(ty, movable_types))
                    {
                        moved.entry(*value).or_insert((ty, block.id, index));
                    }
                }
            }
        }
    }
    violations
}

fn is_move_type(ty: &Ty, movable_types: Option<&HashSet<String>>) -> bool {
    match ty {
        // Collections have reference identity even when their elements are
        // scalar, so sending `List<Int>`/`Map<String, Int>` is still a move
        // of the collection handle itself.
        Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => true,
        Ty::Dyn(_) | Ty::Fn(_, _) => movable_types.is_none(),
        Ty::Named(name) => movable_types.map_or(
            !matches!(name.as_str(), "Int" | "Float" | "Bool" | "Char" | "String" | "Void" | "Ordering"),
            |types| types.contains(name),
        ),
        Ty::Applied(name, args) => {
            if matches!(name.as_str(), "Option" | "Result") {
                return false;
            }
            movable_types.map_or(true, |types| types.contains(name) || args.iter().any(|arg| is_move_type(arg, Some(types))))
        }
        _ => false,
    }
}

pub fn movable_types(items: &[Item]) -> HashSet<String> {
    let records: HashMap<String, _> = items
        .iter()
        .filter_map(|item| match item {
            Item::Record(record) => Some((record.name.clone(), record)),
            _ => None,
        })
        .collect();
    let enums: HashMap<String, _> = items
        .iter()
        .filter_map(|item| match item {
            Item::Enum(decl) => Some((decl.name.clone(), decl)),
            _ => None,
        })
        .collect();
    let mut movable = records
        .values()
        .filter(|record| record.fields.iter().any(|field| field.is_mut))
        .map(|record| record.name.clone())
        .collect::<HashSet<_>>();

    loop {
        let mut changed = false;
        for record in records.values() {
            if movable.contains(&record.name) {
                continue;
            }
            if record.fields.iter().any(|field| type_contains_movable(&field.ty, &movable)) {
                changed |= movable.insert(record.name.clone());
            }
        }
        for decl in enums.values() {
            if movable.contains(&decl.name) {
                continue;
            }
            if decl
                .variants
                .iter()
                .flat_map(|variant| variant.fields.iter())
                .any(|field| type_contains_movable(&field.ty, &movable))
            {
                changed |= movable.insert(decl.name.clone());
            }
        }
        if !changed {
            break;
        }
    }
    movable
}

fn type_contains_movable(ty: &Type, movable: &HashSet<String>) -> bool {
    match ty {
        Type::Named(name, args) => {
            if matches!(name.as_str(), "List" | "Map" | "Set" | "Array" | "Channel" | "Task" | "Rng") {
                return true;
            }
            movable.contains(name) || args.iter().any(|arg| type_contains_movable(arg, movable))
        }
        Type::Mul(left, right) | Type::Div(left, right) => type_contains_movable(left, movable) || type_contains_movable(right, movable),
        Type::Pow(inner, _) => type_contains_movable(inner, movable),
        Type::Fn(params, ret) => params.iter().any(|param| type_contains_movable(param, movable)) || type_contains_movable(ret, movable),
        Type::Dyn(_) => false,
    }
}

fn safe_release_site(instruction: &IrInstr) -> bool {
    // These are the ownership transfers modeled by this first pass: binding a
    // value into a local with no later read, moving it into a channel, passing
    // it to a borrowing print/method call, or consuming it through an
    // aggregate/index operation whose native helper retains borrowed values.
    match instruction {
        IrInstr::StoreLocal { .. }
        | IrInstr::ChannelSend { .. }
        | IrInstr::Aggregate { .. }
        | IrInstr::Index { .. }
        | IrInstr::Field { .. } => true,
        // Ordinary function parameters borrow reference-like values for the
        // duration of the call; the caller can therefore release its last
        // local ownership after any direct call. `Some` is included here as
        // well because its constructor retains the payload before returning
        // the option value.
        IrInstr::Call { .. } => true,
        IrInstr::MethodCall { method, .. } => matches!(
            method.as_str(),
            "length"
                | "count"
                | "push"
                | "remove_at"
                | "is_empty"
                | "trim"
                | "to_upper"
                | "to_lower"
                | "contains"
                | "starts_with"
                | "ends_with"
                | "replace"
                | "contains_key"
                | "get"
                | "is_some"
                | "is_none"
                | "set"
                | "keys"
                | "values"
                | "add"
                | "remove"
        ),
        _ => false,
    }
}

fn is_single_phi_use(value: ValueId, phi_block: usize, phi_instruction: usize, uses: &HashMap<ValueId, Vec<UsePoint>>) -> bool {
    let Some(points) = uses.get(&value) else { return false };
    points.len() == 1 && points[0].block == phi_block && points[0].instruction == phi_instruction
}

fn is_single_return_use(function: &crate::ir::IrFunction, value: ValueId, uses: &HashMap<ValueId, Vec<UsePoint>>) -> bool {
    let Some(points) = uses.get(&value) else { return false };
    if points.len() != 1 {
        return false;
    }
    let point = points[0];
    point.instruction == function.blocks[point.block].instructions.len()
        && matches!(
            function.blocks[point.block].terminator,
            Some(IrTerminator::Return(Some(returned))) if returned == value
        )
}

fn alias_destination(instruction: &IrInstr) -> Option<(ValueId, Ty)> {
    match instruction {
        IrInstr::Field { dst, ty, .. }
        | IrInstr::Index { dst, ty, .. }
        | IrInstr::PatternBind { dst, ty, .. }
        | IrInstr::Phi { dst, ty, .. } => Some((*dst, ty.clone())),
        _ => None,
    }
}

fn analyze_function(function: &crate::ir::IrFunction, report: &mut OwnershipReport) {
    let mut definitions: HashMap<ValueId, Definition> = HashMap::new();
    let mut uses: HashMap<ValueId, Vec<UsePoint>> = HashMap::new();
    let mut opaque_values = HashSet::new();

    for block in &function.blocks {
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let Some((value, ty)) = defined_value(instruction) {
                definitions.insert(value, Definition { ty, block: block.id });
            }
            for value in used_values(instruction) {
                uses.entry(value).or_default().push(UsePoint { block: block.id, instruction: index });
            }
            if matches!(instruction, IrInstr::Opaque { .. }) {
                report.opaque_barriers += 1;
                for value in used_values(instruction) {
                    opaque_values.insert(value);
                }
            }
        }
        if let Some(terminator) = &block.terminator {
            for value in terminator_values(terminator) {
                uses.entry(value).or_default().push(UsePoint {
                    block: block.id,
                    instruction: block.instructions.len(),
                });
            }
        }
    }

    for (value, definition) in definitions {
        if !requires_management(&definition.ty) {
            continue;
        }
        report.managed_values += 1;
        let value_uses = uses.remove(&value).unwrap_or_default();
        let blocks: HashSet<usize> = value_uses.iter().map(|point| point.block).collect();
        let last = value_uses.iter().max_by_key(|point| (point.block, point.instruction)).copied();
        let candidate = !value_uses.is_empty() && blocks.len() == 1 && !opaque_values.contains(&value);
        if candidate {
            report.last_use_candidates += 1;
        } else if blocks.len() > 1 {
            report.cross_block_values += 1;
        } else if value_uses.is_empty() {
            report.unused_managed_values += 1;
        }
        report.facts.push(OwnershipFact {
            function: function.name.clone(),
            value,
            ty: definition.ty,
            uses: value_uses.len(),
            last_block: last.map(|point| point.block),
            last_instruction: last.map(|point| point.instruction),
            candidate,
        });
    }
}

fn requires_management(ty: &Ty) -> bool {
    match ty {
        Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) | Ty::Dyn(_) | Ty::Fn(_, _) => true,
        Ty::Applied(name, args) if name == "Option" && args.len() == 1 && option_value_payload(&args[0]) => false,
        Ty::Named(_) | Ty::Applied(_, _) => true,
        Ty::Quantity(_) | Ty::Int | Ty::Float | Ty::Bool | Ty::Char | Ty::Void | Ty::Sized(_) | Ty::Float32 | Ty::Generic(_) | Ty::Unknown => false,
    }
}

fn option_value_payload(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_) | Ty::Bool)
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
        IrInstr::PatternTest { dst, .. } | IrInstr::TryCheck { dst, .. } | IrInstr::IterHasNext { dst, .. } => Some((*dst, Ty::Bool)),
        IrInstr::Call { dst: Some(dst), ty, .. }
        | IrInstr::MethodCall { dst: Some(dst), ty, .. }
        | IrInstr::Opaque { dst: Some(dst), ty, .. } => Some((*dst, ty.clone())),
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

fn used_values(instruction: &IrInstr) -> Vec<ValueId> {
    match instruction {
        IrInstr::Move { source, .. } => vec![*source],
        IrInstr::StoreLocal { value, .. } => vec![*value],
        IrInstr::Unary { operand, .. } => vec![*operand],
        IrInstr::Binary { left, right, .. } => vec![*left, *right],
        IrInstr::Call { args, .. } => args.clone(),
        IrInstr::MethodCall { receiver, args, .. } => std::iter::once(*receiver).chain(args.iter().copied()).collect(),
        IrInstr::Field { object, .. } => vec![*object],
        IrInstr::Index { object, index, .. } => vec![*object, *index],
        IrInstr::Aggregate { fields, .. } => fields.clone(),
        IrInstr::IterInit { source, .. } => vec![*source],
        IrInstr::IterHasNext { iter, .. } | IrInstr::IterNext { iter, .. } => vec![*iter],
        IrInstr::PatternTest { subject, .. } | IrInstr::PatternBind { subject, .. } => vec![*subject],
        IrInstr::TryCheck { value, .. } | IrInstr::TryValue { value, .. } | IrInstr::TryError { value, .. } => vec![*value],
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
        IrTerminator::Goto(_) | IrTerminator::Return(None) | IrTerminator::RegionReturn(None) | IrTerminator::Unreachable => Vec::new(),
    }
}

pub fn dump(report: &OwnershipReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("ownership functions: {}\n", report.functions));
    out.push_str(&format!("ownership managed-values: {}\n", report.managed_values));
    out.push_str(&format!("ownership last-use-candidates: {}\n", report.last_use_candidates));
    out.push_str(&format!("ownership cross-block-values: {}\n", report.cross_block_values));
    out.push_str(&format!("ownership opaque-barriers: {}\n", report.opaque_barriers));
    out.push_str(&format!("ownership unused-managed-values: {}\n", report.unused_managed_values));
    for fact in &report.facts {
        out.push_str(&format!(
            "  {}: %{} {} uses={} last={}:{} candidate={}\n",
            fact.function,
            fact.value,
            fact.ty.describe(),
            fact.uses,
            fact.last_block.map_or_else(|| "-".to_string(), |value| value.to_string()),
            fact.last_instruction.map_or_else(|| "-".to_string(), |value| value.to_string()),
            fact.candidate
        ));
    }
    out
}

pub fn dump_moves(violations: &[MoveViolation]) -> String {
    let mut out = String::new();
    out.push_str(&format!("ownership move-violations: {}\n", violations.len()));
    for violation in violations {
        out.push_str(&format!(
            "  OSTRIN-E1101 {}: %{} {} sent at bb{}:{} then used at bb{}:{}\n",
            violation.function,
            violation.value,
            violation.ty.describe(),
            violation.send_block,
            violation.send_instruction,
            violation.use_block,
            violation.use_instruction
        ));
    }
    out
}

pub fn dump_lowering(summary: &LoweringSummary) -> String {
    format!(
        "ownership-ir functions: {}\nownership-ir inserted-retains: {}\nownership-ir inserted-releases: {}\nownership-ir unresolved-values: {}\n",
        summary.functions, summary.inserted_retains, summary.inserted_releases, summary.unresolved_values
    )
}
