//! Conservative ownership facts over the explicit IR.
//!
//! This pass reports where retain/release insertion is safe to start. It does
//! not mutate the program yet: a value used across blocks or through an opaque
//! instruction must first go through the CFG/data-flow work that will make
//! automatic release correct at joins and loops.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

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
        Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) | Ty::Dyn(_) | Ty::Fn(_, _) => true,
        Ty::Named(_) | Ty::Applied(_, _) => true,
        Ty::Quantity(_) | Ty::Int | Ty::Float | Ty::Bool | Ty::Char | Ty::String | Ty::Void | Ty::Sized(_) | Ty::Float32 | Ty::Generic(_) | Ty::Unknown => false,
    }
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
        | IrInstr::Phi { dst, ty, .. } => Some((*dst, ty.clone())),
        IrInstr::Call { dst: Some(dst), ty, .. }
        | IrInstr::MethodCall { dst: Some(dst), ty, .. }
        | IrInstr::Opaque { dst: Some(dst), ty, .. } => Some((*dst, ty.clone())),
        IrInstr::StoreLocal { .. }
        | IrInstr::Call { dst: None, .. }
        | IrInstr::MethodCall { dst: None, .. }
        | IrInstr::IterHasNext { .. }
        | IrInstr::Opaque { dst: None, .. }
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
        IrInstr::Phi { incoming, .. } => incoming.iter().map(|(_, value)| *value).collect(),
        IrInstr::Opaque { inputs, .. } => inputs.clone(),
        IrInstr::Retain { value } | IrInstr::Release { value } => vec![*value],
        IrInstr::Param { .. } | IrInstr::Const { .. } | IrInstr::Global { .. } => Vec::new(),
    }
}

fn terminator_values(terminator: &IrTerminator) -> Vec<ValueId> {
    match terminator {
        IrTerminator::Branch { condition, .. } => vec![*condition],
        IrTerminator::Return(Some(value)) => vec![*value],
        IrTerminator::Goto(_) | IrTerminator::Return(None) | IrTerminator::Unreachable => Vec::new(),
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
