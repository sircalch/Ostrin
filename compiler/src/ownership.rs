//! Conservative ownership facts over the explicit IR.
//!
//! This pass inserts only ownership operations whose transfer points are
//! explicit in the IR. Straight-line last uses are handled locally; simple
//! `Phi` joins additionally transfer an incoming owned value into the join,
//! and loop-carried `Phi` values are released on a proven backedge after their
//! final body use. More general CFG liveness remains deliberately unresolved.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};

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
    /// Functions with a managed value the pass could not place a release for;
    /// the native backend keeps those on the verified HIR path.
    pub unresolved_functions: HashSet<String>,
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
    let mut report = OwnershipReport {
        functions: program.functions.len(),
        ..OwnershipReport::default()
    };
    for function in &program.functions {
        analyze_function(function, &mut report);
    }
    report
}

fn reachable_region_blocks(function: &crate::ir::IrFunction, root: usize) -> Option<Vec<usize>> {
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    while let Some(block_id) = pending.pop() {
        if !seen.insert(block_id) {
            continue;
        }
        let block = function.blocks.get(block_id)?;
        let terminator = block.terminator.as_ref()?;
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
            IrTerminator::Return(_) => return None,
        }
    }
    let mut blocks: Vec<_> = seen.into_iter().collect();
    blocks.sort_unstable();
    Some(blocks)
}

/// Adds ownership markers for proven straight-line uses and simple CFG joins.
///
/// A managed `Phi` can transfer ownership when every managed incoming value is
/// used only by that `Phi` (apart from the no-op local stores produced while
/// building the SSA graph). In that case the join must not retain the result
/// or release the incoming value on the predecessor edge: the incoming owned
/// reference becomes the `Phi` value. For a loop backedge, the current `Phi`
/// value is released after its final safe use in the loop body; the exit edge
/// can still return it to the caller.
pub fn lower_linear(program: &IrProgram) -> (IrProgram, LoweringSummary) {
    let mut lowered = program.clone();
    let mut summary = LoweringSummary {
        functions: lowered.functions.len(),
        ..LoweringSummary::default()
    };

    for function in &mut lowered.functions {
        let mut definitions: HashMap<ValueId, (Ty, usize, usize)> = HashMap::new();
        let mut uses: HashMap<ValueId, Vec<UsePoint>> = HashMap::new();
        let mut opaque_values = HashSet::new();
        let mut borrowed_values = HashSet::new();
        let mut phi_edges: HashMap<ValueId, Vec<(usize, usize)>> = HashMap::new();

        // A spawned region is represented as an otherwise disconnected IR
        // block. Values read there are captured by the task environment, so
        // ownership must model the Spawn instruction as their transfer point
        // instead of treating the RegionReturn as an ordinary CFG use.
        let mut region_captures: HashMap<usize, HashSet<ValueId>> = HashMap::new();
        let mut capture_sites: Vec<(usize, usize, ValueId)> = Vec::new();
        let mut spawn_sites = Vec::new();
        for parent in function.blocks.iter() {
            for (index, instruction) in parent.instructions.iter().enumerate() {
                if let IrInstr::Spawn { region, .. } = instruction {
                    spawn_sites.push((parent.id, index, *region));
                }
            }
        }

        let mut region_blocks_by_root: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut region_definitions_by_root: HashMap<usize, HashSet<ValueId>> = HashMap::new();
        let mut direct_captures_by_root: HashMap<usize, HashSet<ValueId>> = HashMap::new();
        for (_, _, region) in &spawn_sites {
            if region_blocks_by_root.contains_key(region) {
                continue;
            }
            let Some(region_blocks) = reachable_region_blocks(function, *region) else {
                continue;
            };
            let local_definitions: HashSet<ValueId> = region_blocks
                .iter()
                .filter_map(|block| function.blocks.get(*block))
                .flat_map(|block| block.instructions.iter().filter_map(defined_value))
                .map(|(value, _)| value)
                .collect();
            let mut captured = HashSet::new();
            for region_id in &region_blocks {
                let Some(region_block) = function.blocks.get(*region_id) else {
                    continue;
                };
                for nested in &region_block.instructions {
                    for value in used_values(nested) {
                        if !local_definitions.contains(&value) {
                            captured.insert(value);
                        }
                    }
                }
                if let Some(terminator) = &region_block.terminator {
                    for value in terminator_values(terminator) {
                        if !local_definitions.contains(&value) {
                            captured.insert(value);
                        }
                    }
                }
            }
            region_blocks_by_root.insert(*region, region_blocks);
            region_definitions_by_root.insert(*region, local_definitions);
            direct_captures_by_root.insert(*region, captured);
        }

        let mut roots: Vec<_> = region_blocks_by_root.keys().copied().collect();
        roots.sort_by_key(|region| std::cmp::Reverse(*region));
        let mut resolved_captures: HashMap<usize, HashSet<ValueId>> = HashMap::new();
        for region in roots {
            let region_blocks = region_blocks_by_root
                .get(&region)
                .cloned()
                .unwrap_or_default();
            let local_definitions = region_definitions_by_root
                .get(&region)
                .cloned()
                .unwrap_or_default();
            let mut captured = direct_captures_by_root.remove(&region).unwrap_or_default();
            for region_id in &region_blocks {
                let Some(region_block) = function.blocks.get(*region_id) else {
                    continue;
                };
                for instruction in &region_block.instructions {
                    if let IrInstr::Spawn { region: nested, .. } = instruction {
                        if let Some(nested_captures) = resolved_captures.get(nested) {
                            captured.extend(nested_captures.iter().copied());
                        }
                    }
                }
            }
            captured.retain(|value| !local_definitions.contains(value));
            for region_id in region_blocks {
                region_captures.insert(region_id, captured.clone());
            }
            resolved_captures.insert(region, captured);
        }
        let mut region_owner_by_block = HashMap::new();
        for (region, blocks) in &region_blocks_by_root {
            for block in blocks {
                region_owner_by_block.insert(*block, *region);
            }
        }
        for (parent, index, region) in &spawn_sites {
            let Some(captured) = resolved_captures.get(region) else {
                continue;
            };
            let inherited = region_owner_by_block
                .get(parent)
                .and_then(|owner| resolved_captures.get(owner));
            for value in captured {
                if inherited.is_some_and(|values| values.contains(value)) {
                    continue;
                }
                capture_sites.push((*parent, *index, *value));
            }
        }

        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                if let Some((value, ty)) = defined_value(instruction) {
                    definitions.insert(value, (ty, block.id, index));
                    if matches!(instruction, IrInstr::Param { .. }) {
                        borrowed_values.insert(value);
                    }
                }
                if let IrInstr::Phi { incoming, .. } = instruction {
                    for (predecessor, value) in incoming {
                        phi_edges
                            .entry(*value)
                            .or_default()
                            .push((*predecessor, block.id));
                    }
                }
                for value in used_values(instruction) {
                    if region_captures
                        .get(&block.id)
                        .is_some_and(|captured| captured.contains(&value))
                    {
                        continue;
                    }
                    uses.entry(value).or_default().push(UsePoint {
                        block: block.id,
                        instruction: index,
                    });
                }
                if matches!(instruction, IrInstr::Opaque { .. }) {
                    for value in used_values(instruction) {
                        opaque_values.insert(value);
                    }
                }
            }
            if let Some(terminator) = &block.terminator {
                for value in terminator_values(terminator) {
                    if region_captures
                        .get(&block.id)
                        .is_some_and(|captured| captured.contains(&value))
                    {
                        continue;
                    }
                    uses.entry(value).or_default().push(UsePoint {
                        block: block.id,
                        instruction: block.instructions.len(),
                    });
                }
            }
        }
        for (block, instruction, value) in capture_sites {
            uses.entry(value)
                .or_default()
                .push(UsePoint { block, instruction });
        }

        let mut retain_before: HashMap<(usize, usize), Vec<ValueId>> = HashMap::new();
        let mut retain_after: HashMap<(usize, usize), Vec<ValueId>> = HashMap::new();
        let mut release_after: HashMap<(usize, usize), Vec<ValueId>> = HashMap::new();
        let mut edge_ops: HashMap<(usize, usize), Vec<EdgeOp>> = HashMap::new();
        let mut return_retains: HashMap<usize, Vec<ValueId>> = HashMap::new();
        for (value, (ty, definition_block, definition_instruction)) in &definitions {
            if !requires_management(ty) {
                continue;
            }
            let edges = phi_edges.get(value).cloned().unwrap_or_default();
            // Function parameters are borrowed C arguments: the callee never releases
            // them, but a `Phi` input or a returned parameter needs its own reference.
            if borrowed_values.contains(value) {
                for edge in &edges {
                    edge_ops
                        .entry(*edge)
                        .or_default()
                        .push(EdgeOp::Retain(*value));
                }
                for block in &function.blocks {
                    if matches!(block.terminator, Some(IrTerminator::Return(Some(returned))) if returned == *value)
                    {
                        return_retains.entry(block.id).or_default().push(*value);
                    }
                }
                continue;
            }
            let value_uses: Vec<UsePoint> = uses
                .get(value)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|point| {
                    !matches!(
                        function.blocks[point.block]
                            .instructions
                            .get(point.instruction),
                        Some(IrInstr::Phi { .. })
                    )
                })
                .collect();
            let blocks: HashSet<usize> = value_uses.iter().map(|point| point.block).collect();
            let single_block = edges.is_empty()
                && !value_uses.is_empty()
                && blocks.len() == 1
                && !opaque_values.contains(value);
            if single_block {
                let last = value_uses
                    .iter()
                    .max_by_key(|point| point.instruction)
                    .copied()
                    .expect("a used value has a last use");
                // A return transfers the value to the caller; releasing after
                // its terminator would be a use-after-release.
                let block = &function.blocks[last.block];
                if last.instruction < block.instructions.len() {
                    if safe_release_site(&block.instructions[last.instruction]) {
                        release_after
                            .entry((last.block, last.instruction + 1))
                            .or_default()
                            .push(*value);
                    } else {
                        summary.unresolved_values += 1;
                        note_unresolved(
                            &mut summary,
                            &function.name,
                            ty,
                            matches!(
                                function.blocks[*definition_block]
                                    .instructions
                                    .get(*definition_instruction),
                                Some(IrInstr::Const { .. })
                            ),
                        );
                    }
                } else if !matches!(
                    block.terminator,
                    Some(IrTerminator::Return(Some(returned)) | IrTerminator::RegionReturn(Some(returned))) if returned == *value
                ) {
                    summary.unresolved_values += 1;
                    note_unresolved(
                        &mut summary,
                        &function.name,
                        ty,
                        matches!(
                            function.blocks[*definition_block]
                                .instructions
                                .get(*definition_instruction),
                            Some(IrInstr::Const { .. })
                        ),
                    );
                }
            } else if value_uses.is_empty() && edges.is_empty() {
                release_after
                    .entry((*definition_block, definition_instruction + 1))
                    .or_default()
                    .push(*value);
            } else if let Some(plan) = plan_cross_block_releases(
                function,
                *definition_block,
                *value,
                &value_uses,
                &edges,
                &opaque_values,
            ) {
                for (block, index) in plan.after {
                    release_after
                        .entry((block, index + 1))
                        .or_default()
                        .push(*value);
                }
                for (edge, op) in plan.edge_ops {
                    edge_ops.entry(edge).or_default().push(op);
                }
            } else {
                summary.unresolved_values += 1;
                note_unresolved(
                    &mut summary,
                    &function.name,
                    ty,
                    matches!(
                        function.blocks[*definition_block]
                            .instructions
                            .get(*definition_instruction),
                        Some(IrInstr::Const { .. })
                    ),
                );
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
                            if definitions
                                .get(value)
                                .is_some_and(|(ty, _, _)| requires_management(ty))
                            {
                                retain_before
                                    .entry((block.id, index))
                                    .or_default()
                                    .push(*value);
                            }
                        }
                    }
                }
                if let Some((value, _)) =
                    alias_destination(instruction).filter(|(_, ty)| requires_management(ty))
                {
                    retain_after
                        .entry((block.id, index + 1))
                        .or_default()
                        .push(value);
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
            if let Some(values) = return_retains.remove(&block.id) {
                for value in values {
                    block.instructions.push(IrInstr::Retain { value });
                    summary.inserted_retains += 1;
                }
            }
        }
        apply_edge_ops(function, edge_ops, &mut summary);
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

pub fn check_moves_for_types(
    program: &IrProgram,
    movable_types: &HashSet<String>,
) -> Vec<MoveViolation> {
    check_moves_impl(program, Some(movable_types))
}

fn check_moves_impl(
    program: &IrProgram,
    movable_types: Option<&HashSet<String>>,
) -> Vec<MoveViolation> {
    let mut violations = Vec::new();
    for function in &program.functions {
        let mut definitions: HashMap<ValueId, Ty> = HashMap::new();
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Some((value, ty)) = defined_value(instruction) {
                    definitions.insert(value, ty);
                }
            }
        }

        // A block's position in `function.blocks` is not execution order:
        // branches can be mutually exclusive and loop backedges can revisit
        // an earlier block. Propagate moved SSA values over reachable CFG
        // edges to a fixed point, starting independent spawned regions with
        // their own empty state.
        let mut successors = vec![Vec::new(); function.blocks.len()];
        let mut roots = vec![function.entry];
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let IrInstr::Spawn { region, .. } = instruction {
                    roots.push(*region);
                }
            }
            let next = match block.terminator.as_ref() {
                Some(IrTerminator::Goto(target)) => vec![*target],
                Some(IrTerminator::Branch {
                    then_block,
                    else_block,
                    ..
                }) => vec![*then_block, *else_block],
                _ => Vec::new(),
            };
            for target in next {
                if target < successors.len() && !successors[block.id].contains(&target) {
                    successors[block.id].push(target);
                }
            }
        }
        roots.retain(|root| *root < function.blocks.len());
        roots.sort_unstable();
        roots.dedup();

        // Each moved value retains the earliest deterministic source site
        // reaching a block. The map merge is a finite, monotone dataflow
        // lattice, so loops converge without depending on block numbering.
        let mut incoming = vec![HashMap::<ValueId, (usize, usize)>::new(); function.blocks.len()];
        let mut outgoing_states =
            vec![HashMap::<ValueId, (usize, usize)>::new(); function.blocks.len()];
        let mut reachable = vec![false; function.blocks.len()];
        let mut pending = VecDeque::new();
        for root in roots {
            if !reachable[root] {
                reachable[root] = true;
                pending.push_back(root);
            }
        }
        while let Some(block_id) = pending.pop_front() {
            let block = &function.blocks[block_id];
            let mut outgoing = incoming[block_id].clone();
            for (index, instruction) in block.instructions.iter().enumerate() {
                if let IrInstr::ChannelSend { value, .. } = instruction {
                    if definitions
                        .get(value)
                        .is_some_and(|ty| is_move_type(ty, movable_types))
                    {
                        let site = (block.id, index);
                        outgoing
                            .entry(*value)
                            .and_modify(|known| *known = (*known).min(site))
                            .or_insert(site);
                    }
                }
            }
            outgoing_states[block_id] = outgoing.clone();
            for target in &successors[block_id] {
                let mut changed = !reachable[*target];
                reachable[*target] = true;
                let mut edge_state = outgoing.clone();
                for instruction in &function.blocks[*target].instructions {
                    let IrInstr::Phi {
                        dst,
                        incoming: phi_inputs,
                        ..
                    } = instruction
                    else {
                        continue;
                    };
                    for (predecessor, value) in phi_inputs {
                        if *predecessor == block_id {
                            if let Some(site) = outgoing.get(value).copied() {
                                edge_state
                                    .entry(*dst)
                                    .and_modify(|known| *known = (*known).min(site))
                                    .or_insert(site);
                            }
                        }
                    }
                }
                for (value, site) in &edge_state {
                    match incoming[*target].get_mut(value) {
                        Some(known) if *site < *known => {
                            *known = *site;
                            changed = true;
                        }
                        Some(_) => {}
                        None => {
                            incoming[*target].insert(*value, *site);
                            changed = true;
                        }
                    }
                }
                if changed {
                    pending.push_back(*target);
                }
            }
        }

        let mut reported = HashSet::new();
        for block in &function.blocks {
            if block.id >= reachable.len() || !reachable[block.id] {
                continue;
            }
            let mut moved = incoming[block.id].clone();
            for (index, instruction) in block.instructions.iter().enumerate() {
                if matches!(instruction, IrInstr::Phi { .. }) {
                    // Phi operands are edge uses, not simultaneous uses in the
                    // merge block. They are checked against their selecting
                    // predecessor below.
                    continue;
                }
                let mut instruction_uses = HashSet::new();
                for value in used_values(instruction) {
                    if !instruction_uses.insert(value) {
                        continue;
                    }
                    record_move_violation(
                        &function.name,
                        &definitions,
                        &moved,
                        value,
                        block.id,
                        index,
                        &mut reported,
                        &mut violations,
                    );
                }
                if let IrInstr::ChannelSend { value, .. } = instruction {
                    if definitions
                        .get(value)
                        .is_some_and(|ty| is_move_type(ty, movable_types))
                    {
                        moved.entry(*value).or_insert((block.id, index));
                    }
                }
            }
            if let Some(terminator) = &block.terminator {
                for value in terminator_values(terminator) {
                    record_move_violation(
                        &function.name,
                        &definitions,
                        &moved,
                        value,
                        block.id,
                        block.instructions.len(),
                        &mut reported,
                        &mut violations,
                    );
                }
            }
            for target in &successors[block.id] {
                for (index, instruction) in function.blocks[*target].instructions.iter().enumerate()
                {
                    let IrInstr::Phi {
                        incoming: phi_inputs,
                        ..
                    } = instruction
                    else {
                        continue;
                    };
                    for (predecessor, value) in phi_inputs {
                        if *predecessor == block.id {
                            record_move_violation(
                                &function.name,
                                &definitions,
                                &outgoing_states[block.id],
                                *value,
                                *target,
                                index,
                                &mut reported,
                                &mut violations,
                            );
                        }
                    }
                }
            }
        }
    }
    violations.sort_by(|left, right| {
        (
            &left.function,
            left.send_block,
            left.send_instruction,
            left.use_block,
            left.use_instruction,
            left.value,
        )
            .cmp(&(
                &right.function,
                right.send_block,
                right.send_instruction,
                right.use_block,
                right.use_instruction,
                right.value,
            ))
    });
    violations
}

fn record_move_violation(
    function: &str,
    definitions: &HashMap<ValueId, Ty>,
    moved: &HashMap<ValueId, (usize, usize)>,
    value: ValueId,
    use_block: usize,
    use_instruction: usize,
    reported: &mut HashSet<(ValueId, usize, usize, usize, usize)>,
    violations: &mut Vec<MoveViolation>,
) {
    let (Some(ty), Some((send_block, send_instruction))) =
        (definitions.get(&value), moved.get(&value))
    else {
        return;
    };
    let key = (
        value,
        *send_block,
        *send_instruction,
        use_block,
        use_instruction,
    );
    if reported.insert(key) {
        violations.push(MoveViolation {
            function: function.to_string(),
            value,
            ty: ty.clone(),
            send_block: *send_block,
            send_instruction: *send_instruction,
            use_block,
            use_instruction,
        });
    }
}

fn is_move_type(ty: &Ty, movable_types: Option<&HashSet<String>>) -> bool {
    match ty {
        // Collections have reference identity even when their elements are
        // scalar, so sending `List<Int>`/`Map<String, Int>` is still a move
        // of the collection handle itself.
        Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => true,
        Ty::Dyn(_) | Ty::Fn(_, _) => movable_types.is_none(),
        Ty::Named(name) => movable_types.map_or(
            !matches!(
                name.as_str(),
                "Int" | "Float" | "Bool" | "Char" | "String" | "Void" | "Ordering"
            ),
            |types| types.contains(name),
        ),
        Ty::Applied(name, args) => {
            if matches!(name.as_str(), "Option" | "Result") {
                return false;
            }
            movable_types.map_or(true, |types| {
                types.contains(name) || args.iter().any(|arg| is_move_type(arg, Some(types)))
            })
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
            if record
                .fields
                .iter()
                .any(|field| type_contains_movable(&field.ty, &movable))
            {
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
            if matches!(
                name.as_str(),
                "List" | "Map" | "Set" | "Array" | "Channel" | "Task" | "Rng"
            ) {
                return true;
            }
            movable.contains(name) || args.iter().any(|arg| type_contains_movable(arg, movable))
        }
        Type::Mul(left, right) | Type::Div(left, right) => {
            type_contains_movable(left, movable) || type_contains_movable(right, movable)
        }
        Type::Pow(inner, _) => type_contains_movable(inner, movable),
        Type::Fn(params, ret) => {
            params
                .iter()
                .any(|param| type_contains_movable(param, movable))
                || type_contains_movable(ret, movable)
        }
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
        | IrInstr::Spawn { .. }
        | IrInstr::ChannelSend { .. }
        | IrInstr::ChannelReceive { .. }
        | IrInstr::ChannelClose { .. }
        | IrInstr::TaskJoin { .. }
        | IrInstr::Aggregate { .. }
        | IrInstr::ClosureMake { .. }
        | IrInstr::Index { .. }
        | IrInstr::Field { .. }
        | IrInstr::Binary { .. }
        | IrInstr::PatternTest { .. }
        | IrInstr::PatternBind { .. }
        | IrInstr::TryCheck { .. }
        | IrInstr::TryValue { .. }
        | IrInstr::TryError { .. }
        | IrInstr::TryErrorValue { .. } => true,
        // Ordinary function parameters borrow reference-like values for the
        // duration of the call; the caller can therefore release its last
        // local ownership after any direct call. `Some` is included here as
        // well because its constructor retains the payload before returning
        // the option value.
        IrInstr::Call { .. } | IrInstr::ClosureCall { .. } => true,
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
                | "split"
                | "lines"
                | "to_int"
                | "to_float"
                | "contains_key"
                | "get"
                | "shape"
                | "rank"
                | "size"
                | "sum"
                | "min"
                | "max"
                | "mean"
                | "to_list"
                | "cumsum"
                | "sort"
                | "any"
                | "all"
                | "count_true"
                | "row"
                | "col"
                | "reshape"
                | "transpose"
                | "sum_axis"
                | "dot"
                | "matmul"
                | "var"
                | "std"
                | "sample_var"
                | "sample_std"
                | "median"
                | "percentile"
                | "is_ok"
                | "is_err"
                | "is_some"
                | "is_none"
                | "unwrap"
                | "unwrap_or"
                | "ok"
                | "ok_or"
                | "cancel"
                | "set"
                | "keys"
                | "values"
                | "add"
                | "remove"
        ),
        _ => false,
    }
}

/// `Option`/`Result` wrappers (`get(..).unwrap_or(..)`, `remove(..).unwrap()`) are consumed by
/// native methods that retain an extracted managed payload before this pass releases the
/// receiver and arguments.
fn note_unresolved(summary: &mut LoweringSummary, function: &str, ty: &Ty, literal: bool) {
    // String literals are static: an unreleased literal cannot leak.
    if !literal && !matches!(ty, Ty::Applied(_, _)) {
        summary.unresolved_functions.insert(function.to_string());
    }
}

#[derive(Debug, Clone, Copy)]
enum EdgeOp {
    Retain(ValueId),
    Release(ValueId),
}

/// Where the reference of a managed value that spans several blocks (or feeds a
/// `Phi`) is released or duplicated, from a liveness analysis of the CFG: after
/// its last use in every block where it dies, and on each CFG edge either a
/// `release` (the target no longer needs it) or one `retain` per `Phi` that
/// consumes it while it is still live. When the value dies on a `Phi` edge its
/// reference is transferred to the `Phi`.
struct CrossBlockPlan {
    after: Vec<(usize, usize)>,
    edge_ops: Vec<((usize, usize), EdgeOp)>,
}

fn plan_cross_block_releases(
    function: &crate::ir::IrFunction,
    definition_block: usize,
    value: ValueId,
    value_uses: &[UsePoint],
    phi_edges: &[(usize, usize)],
    opaque_values: &HashSet<ValueId>,
) -> Option<CrossBlockPlan> {
    if opaque_values.contains(&value) {
        return None;
    }
    let count = function.blocks.len();
    let mut used_in = vec![false; count];
    let mut last_use: Vec<Option<usize>> = vec![None; count];
    for point in value_uses {
        let block = function.blocks.get(point.block)?;
        match block.instructions.get(point.instruction) {
            Some(IrInstr::Opaque { .. }) => return None,
            Some(_) => {}
            None => {
                // A terminator use: branch conditions are harmless, and returning
                // the value itself transfers it to the caller.
                let transfers = matches!(
                    block.terminator,
                    Some(IrTerminator::Return(Some(returned)) | IrTerminator::RegionReturn(Some(returned))) if returned == value
                );
                if !transfers && !matches!(block.terminator, Some(IrTerminator::Branch { .. })) {
                    return None;
                }
            }
        }
        used_in[point.block] = true;
        last_use[point.block] = Some(
            last_use[point.block].map_or(point.instruction, |last| last.max(point.instruction)),
        );
    }
    // A `Phi` input is consumed when control leaves the predecessor.
    let mut end_use = vec![false; count];
    for (pred, _) in phi_edges {
        used_in[*pred] = true;
        end_use[*pred] = true;
    }
    let successors = |block: usize| -> Vec<usize> {
        let mut next = match function
            .blocks
            .get(block)
            .and_then(|b| b.terminator.as_ref())
        {
            Some(IrTerminator::Goto(target)) => vec![*target],
            Some(IrTerminator::Branch {
                then_block,
                else_block,
                ..
            }) => vec![*then_block, *else_block],
            _ => Vec::new(),
        };
        next.dedup();
        next
    };

    // Backward liveness of a single SSA value: it is born in `definition_block`.
    let mut live_in = vec![false; count];
    let mut live_out = vec![false; count];
    loop {
        let mut changed = false;
        for block in (0..count).rev() {
            let out = successors(block).into_iter().any(|next| live_in[next]);
            let inn = block != definition_block && (used_in[block] || out);
            if out != live_out[block] || inn != live_in[block] {
                live_out[block] = out;
                live_in[block] = inn;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut plan = CrossBlockPlan {
        after: Vec::new(),
        edge_ops: Vec::new(),
    };
    for block in 0..count {
        if block != definition_block && !live_in[block] {
            continue;
        }
        if live_out[block] || end_use[block] {
            for next in successors(block) {
                let consumers = phi_edges
                    .iter()
                    .filter(|edge| **edge == (block, next))
                    .count();
                let needed_after = live_in[next];
                let (retains, release) = match (needed_after, consumers) {
                    (true, m) => (m, false),
                    (false, 0) => (0, true),
                    (false, m) => (m - 1, false),
                };
                for _ in 0..retains {
                    plan.edge_ops.push(((block, next), EdgeOp::Retain(value)));
                }
                if release {
                    plan.edge_ops.push(((block, next), EdgeOp::Release(value)));
                }
            }
        } else if used_in[block] {
            let index = last_use[block].expect("a used block has a last use");
            let instructions = &function.blocks[block].instructions;
            if index < instructions.len() {
                if !safe_release_site(&instructions[index]) {
                    return None;
                }
                plan.after.push((block, index));
            }
            // A use by the terminator is a return (transfer) or a branch condition.
        } else {
            return None;
        }
    }
    Some(plan)
}

/// Applies edge retains/releases: at the end of the predecessor when it has a
/// single successor, otherwise on a fresh block that splits the edge (also
/// retargeting the successor's `Phi` inputs).
fn apply_edge_ops(
    function: &mut crate::ir::IrFunction,
    edge_ops: HashMap<(usize, usize), Vec<EdgeOp>>,
    summary: &mut LoweringSummary,
) {
    let mut edges: Vec<_> = edge_ops.into_iter().collect();
    edges.sort_by_key(|(edge, _)| *edge);
    for ((pred, succ), ops) in edges {
        let materialize = |ops: Vec<EdgeOp>, summary: &mut LoweringSummary| -> Vec<IrInstr> {
            ops.into_iter()
                .map(|op| match op {
                    EdgeOp::Retain(value) => {
                        summary.inserted_retains += 1;
                        IrInstr::Retain { value }
                    }
                    EdgeOp::Release(value) => {
                        summary.inserted_releases += 1;
                        IrInstr::Release { value }
                    }
                })
                .collect()
        };
        if matches!(function.blocks[pred].terminator, Some(IrTerminator::Goto(target)) if target == succ)
        {
            let instructions = materialize(ops, summary);
            function.blocks[pred].instructions.extend(instructions);
            continue;
        }
        let new_id = function.blocks.len();
        let instructions = materialize(ops, summary);
        function.blocks.push(crate::ir::IrBlock {
            id: new_id,
            instructions,
            terminator: Some(IrTerminator::Goto(succ)),
        });
        if let Some(IrTerminator::Branch {
            then_block,
            else_block,
            ..
        }) = function.blocks[pred].terminator.as_mut()
        {
            if *then_block == succ {
                *then_block = new_id;
            }
            if *else_block == succ {
                *else_block = new_id;
            }
        }
        for instruction in &mut function.blocks[succ].instructions {
            if let IrInstr::Phi { incoming, .. } = instruction {
                for (block, _) in incoming.iter_mut() {
                    if *block == pred {
                        *block = new_id;
                    }
                }
            }
        }
    }
}

fn alias_destination(instruction: &IrInstr) -> Option<(ValueId, Ty)> {
    match instruction {
        IrInstr::Field { dst, ty, .. }
        | IrInstr::Index { dst, ty, .. }
        | IrInstr::PatternBind { dst, ty, .. }
        | IrInstr::TryValue { dst, ty, .. }
        | IrInstr::TryErrorValue { dst, ty, .. } => Some((*dst, ty.clone())),
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
                definitions.insert(
                    value,
                    Definition {
                        ty,
                        block: block.id,
                    },
                );
            }
            for value in used_values(instruction) {
                uses.entry(value).or_default().push(UsePoint {
                    block: block.id,
                    instruction: index,
                });
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
        let last = value_uses
            .iter()
            .max_by_key(|point| (point.block, point.instruction))
            .copied();
        let candidate =
            !value_uses.is_empty() && blocks.len() == 1 && !opaque_values.contains(&value);
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

pub(crate) fn requires_management(ty: &Ty) -> bool {
    match ty {
        Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) | Ty::Dyn(_) | Ty::Fn(_, _) => true,
        Ty::Applied(name, args)
            if name == "Option" && args.len() == 1 && option_value_payload(&args[0]) =>
        {
            false
        }
        Ty::Named(_) | Ty::Applied(_, _) => true,
        Ty::Quantity(_)
        | Ty::Int
        | Ty::Float
        | Ty::Bool
        | Ty::Char
        | Ty::Void
        | Ty::Sized(_)
        | Ty::Float32
        | Ty::Generic(_)
        | Ty::Unknown => false,
    }
}

fn option_value_payload(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_) | Ty::Bool
    )
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

pub fn dump(report: &OwnershipReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("ownership functions: {}\n", report.functions));
    out.push_str(&format!(
        "ownership managed-values: {}\n",
        report.managed_values
    ));
    out.push_str(&format!(
        "ownership last-use-candidates: {}\n",
        report.last_use_candidates
    ));
    out.push_str(&format!(
        "ownership cross-block-values: {}\n",
        report.cross_block_values
    ));
    out.push_str(&format!(
        "ownership opaque-barriers: {}\n",
        report.opaque_barriers
    ));
    out.push_str(&format!(
        "ownership unused-managed-values: {}\n",
        report.unused_managed_values
    ));
    for fact in &report.facts {
        out.push_str(&format!(
            "  {}: %{} {} uses={} last={}:{} candidate={}\n",
            fact.function,
            fact.value,
            fact.ty.describe(),
            fact.uses,
            fact.last_block
                .map_or_else(|| "-".to_string(), |value| value.to_string()),
            fact.last_instruction
                .map_or_else(|| "-".to_string(), |value| value.to_string()),
            fact.candidate
        ));
    }
    out
}

pub fn dump_moves(violations: &[MoveViolation]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "ownership move-violations: {}\n",
        violations.len()
    ));
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
    let mut unresolved: Vec<&str> = summary
        .unresolved_functions
        .iter()
        .map(|name| name.as_str())
        .collect();
    unresolved.sort();
    format!(
        "ownership-ir functions: {}\nownership-ir inserted-retains: {}\nownership-ir inserted-releases: {}\nownership-ir unresolved-values: {}\nownership-ir unresolved-functions: {}\n",
        summary.functions,
        summary.inserted_retains,
        summary.inserted_releases,
        summary.unresolved_values,
        unresolved.join(", ")
    )
}
