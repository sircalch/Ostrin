//! A small, explicit control-flow IR sitting between HIR and the native backend.
//!
//! This is deliberately an additive stage at first: --ir exposes the lowering
//! and the C backend still consumes HIR/AST. Every expression gets a temporary
//! value, control flow gets basic blocks, and constructs not yet ready for a
//! semantic lowering are represented as named Opaque instructions instead of
//! disappearing. That makes the boundary measurable and gives the future
//! retain/release and last-use passes a stable place to work.
#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::ast::{BinOp, UnaryOp};
use crate::hir::{HirBlock, HirExpr, HirFunction, HirKind, HirProgram, HirStmt};
use crate::types::Ty;

pub type ValueId = usize;
pub type BlockId = usize;

#[derive(Debug, Clone)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
}

#[derive(Debug, Clone)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<(String, Ty)>,
    pub ret: Ty,
    pub entry: BlockId,
    pub blocks: Vec<IrBlock>,
}

#[derive(Debug, Clone)]
pub struct IrBlock {
    pub id: BlockId,
    pub instructions: Vec<IrInstr>,
    pub terminator: Option<IrTerminator>,
}

#[derive(Debug, Clone)]
pub enum IrInstr {
    Param {
        dst: ValueId,
        index: usize,
        name: String,
        ty: Ty,
    },
    Const {
        dst: ValueId,
        value: String,
        ty: Ty,
    },
    Global {
        dst: ValueId,
        name: String,
        ty: Ty,
    },
    Move {
        dst: ValueId,
        source: ValueId,
        ty: Ty,
    },
    StoreLocal {
        name: String,
        value: ValueId,
    },
    Unary {
        dst: ValueId,
        op: UnaryOp,
        operand: ValueId,
        ty: Ty,
    },
    Binary {
        dst: ValueId,
        op: BinOp,
        left: ValueId,
        right: ValueId,
        ty: Ty,
    },
    Call {
        dst: Option<ValueId>,
        callee: String,
        args: Vec<ValueId>,
        ty: Ty,
    },
    MethodCall {
        dst: Option<ValueId>,
        method: String,
        receiver: ValueId,
        args: Vec<ValueId>,
        ty: Ty,
    },
    Field {
        dst: ValueId,
        object: ValueId,
        field: String,
        ty: Ty,
    },
    Index {
        dst: ValueId,
        object: ValueId,
        index: ValueId,
        ty: Ty,
    },
    Aggregate {
        dst: ValueId,
        kind: String,
        fields: Vec<ValueId>,
        field_names: Vec<String>,
        ty: Ty,
    },
    IterInit {
        dst: ValueId,
        source: ValueId,
        ty: Ty,
    },
    IterHasNext {
        dst: ValueId,
        iter: ValueId,
    },
    IterNext {
        dst: ValueId,
        iter: ValueId,
        ty: Ty,
    },
    PatternTest {
        dst: ValueId,
        subject: ValueId,
        pattern: String,
    },
    PatternBind {
        dst: ValueId,
        subject: ValueId,
        name: String,
        path: Vec<String>,
        ty: Ty,
    },
    TryCheck {
        dst: ValueId,
        value: ValueId,
    },
    TryValue {
        dst: ValueId,
        value: ValueId,
        ty: Ty,
    },
    TryError {
        dst: ValueId,
        value: ValueId,
        ty: Ty,
    },
    Spawn {
        dst: ValueId,
        region: BlockId,
        scoped: bool,
        ty: Ty,
    },
    ChannelNew {
        dst: ValueId,
        capacity: Option<ValueId>,
        ty: Ty,
    },
    ChannelSend {
        channel: ValueId,
        value: ValueId,
    },
    ChannelReceive {
        dst: ValueId,
        channel: ValueId,
        ty: Ty,
    },
    ChannelClose {
        channel: ValueId,
    },
    TaskJoin {
        dst: ValueId,
        task: ValueId,
        ty: Ty,
    },
    Phi {
        dst: ValueId,
        incoming: Vec<(BlockId, ValueId)>,
        ty: Ty,
    },
    Opaque {
        dst: Option<ValueId>,
        op: String,
        inputs: Vec<ValueId>,
        ty: Ty,
    },
    Retain {
        value: ValueId,
    },
    Release {
        value: ValueId,
    },
}

#[derive(Debug, Clone)]
pub enum IrTerminator {
    Goto(BlockId),
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    },
    Return(Option<ValueId>),
    RegionReturn(Option<ValueId>),
    Unreachable,
}

#[derive(Debug, Default, Clone)]
pub struct VerifyReport {
    pub blocks: usize,
    pub instructions: usize,
    pub opaque: usize,
    pub unterminated: usize,
    pub violations: Vec<String>,
}

#[derive(Default)]
struct LoopEdges {
    breaks: Vec<(BlockId, HashMap<String, ValueId>)>,
    continues: Vec<(BlockId, HashMap<String, ValueId>)>,
}

struct Builder {
    function: IrFunction,
    current: BlockId,
    next_value: ValueId,
    locals: Vec<HashMap<String, ValueId>>,
    break_targets: Vec<(BlockId, BlockId)>,
    loop_edges: Vec<LoopEdges>,
    region_depth: usize,
}

impl Builder {
    fn new(function: &HirFunction) -> Self {
        let entry = IrBlock {
            id: 0,
            instructions: Vec::new(),
            terminator: None,
        };
        Self {
            function: IrFunction {
                name: function.name.clone(),
                params: function.params.clone(),
                ret: function.ret.clone(),
                entry: 0,
                blocks: vec![entry],
            },
            current: 0,
            next_value: 0,
            locals: vec![HashMap::new()],
            break_targets: Vec::new(),
            loop_edges: Vec::new(),
            region_depth: 0,
        }
    }

    fn block(&mut self, id: BlockId) -> &mut IrBlock {
        &mut self.function.blocks[id]
    }

    fn new_block(&mut self) -> BlockId {
        let id = self.function.blocks.len();
        self.function.blocks.push(IrBlock {
            id,
            instructions: Vec::new(),
            terminator: None,
        });
        id
    }

    fn fresh(&mut self) -> ValueId {
        let id = self.next_value;
        self.next_value += 1;
        id
    }

    fn terminated(&self) -> bool {
        self.function.blocks[self.current].terminator.is_some()
    }

    fn ensure_open(&mut self) {
        if self.terminated() {
            self.current = self.new_block();
        }
    }

    fn emit(&mut self, instruction: IrInstr) {
        self.ensure_open();
        self.block(self.current).instructions.push(instruction);
    }

    fn terminate(&mut self, terminator: IrTerminator) {
        if self.function.blocks[self.current].terminator.is_none() {
            self.function.blocks[self.current].terminator = Some(terminator);
        }
    }

    fn const_value(&mut self, value: impl Into<String>, ty: Ty) -> ValueId {
        let dst = self.fresh();
        self.emit(IrInstr::Const {
            dst,
            value: value.into(),
            ty,
        });
        dst
    }

    fn unit(&mut self) -> ValueId {
        self.const_value("unit", Ty::Void)
    }

    fn lookup(&self, name: &str) -> Option<ValueId> {
        self.locals
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    fn bind(&mut self, name: &str, value: ValueId) {
        if let Some(scope) = self
            .locals
            .iter_mut()
            .rev()
            .find(|scope| scope.contains_key(name))
        {
            scope.insert(name.to_string(), value);
        } else {
            self.locals
                .last_mut()
                .expect("an IR local scope always exists")
                .insert(name.to_string(), value);
        }
    }

    fn snapshot_visible(&self) -> HashMap<String, ValueId> {
        let mut visible = HashMap::new();
        for scope in &self.locals {
            visible.extend(scope.iter().map(|(name, value)| (name.clone(), *value)));
        }
        visible
    }

    fn known_value_type(&self, value: ValueId) -> Option<Ty> {
        self.function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .find_map(|instruction| {
                defined_value_type(instruction)
                    .filter(|(id, _)| *id == value)
                    .map(|(_, ty)| ty)
            })
    }

    fn patch_phi(&mut self, destination: ValueId, incoming: Vec<(BlockId, ValueId)>) {
        for block in &mut self.function.blocks {
            for instruction in &mut block.instructions {
                if let IrInstr::Phi {
                    dst,
                    incoming: values,
                    ..
                } = instruction
                {
                    if *dst == destination {
                        *values = incoming;
                        return;
                    }
                }
            }
        }
    }

    fn restore_visible(&mut self, visible: &HashMap<String, ValueId>) {
        for scope in &mut self.locals {
            for (name, value) in visible {
                if scope.contains_key(name) {
                    scope.insert(name.clone(), *value);
                }
            }
        }
    }

    fn lower_block(&mut self, block: &HirBlock) -> Option<ValueId> {
        self.locals.push(HashMap::new());
        let result = self.lower_block_contents(block);
        self.locals.pop();
        result
    }

    fn lower_block_contents(&mut self, block: &HirBlock) -> Option<ValueId> {
        for statement in &block.stmts {
            self.lower_stmt(statement);
        }
        block.tail.as_deref().map(|tail| self.lower_expr(tail))
    }

    fn lower_stmt(&mut self, statement: &HirStmt) {
        match statement {
            HirStmt::Let { name, value, .. } => {
                let value = self.lower_expr(value);
                self.emit(IrInstr::StoreLocal {
                    name: name.clone(),
                    value,
                });
                self.bind(name, value);
            }
            HirStmt::Assign { name, value } => {
                let value = self.lower_expr(value);
                self.emit(IrInstr::StoreLocal {
                    name: name.clone(),
                    value,
                });
                self.bind(name, value);
            }
            HirStmt::FieldAssign { target, value } => {
                let target = self.lower_expr(target);
                let value = self.lower_expr(value);
                self.emit(IrInstr::Opaque {
                    dst: None,
                    op: "field_assign".to_string(),
                    inputs: vec![target, value],
                    ty: Ty::Void,
                });
            }
            HirStmt::Return(value) => {
                let value = value.as_ref().map(|value| self.lower_expr(value));
                if self.region_depth > 0 {
                    self.terminate(IrTerminator::RegionReturn(value));
                } else {
                    self.terminate(IrTerminator::Return(value));
                }
            }
            HirStmt::Break(value) => {
                if let Some((break_block, _)) = self.break_targets.last().copied() {
                    if let Some(value) = value {
                        let value = self.lower_expr(value);
                        self.emit(IrInstr::Opaque {
                            dst: None,
                            op: "break_value".to_string(),
                            inputs: vec![value],
                            ty: Ty::Void,
                        });
                    }
                    self.record_loop_edge(true);
                    self.terminate(IrTerminator::Goto(break_block));
                } else {
                    self.terminate(IrTerminator::Unreachable);
                }
            }
            HirStmt::Continue => {
                if let Some((_, continue_block)) = self.break_targets.last().copied() {
                    self.record_loop_edge(false);
                    self.terminate(IrTerminator::Goto(continue_block));
                } else {
                    self.terminate(IrTerminator::Unreachable);
                }
            }
            HirStmt::While { cond, body } => self.lower_while(cond, body),
            HirStmt::For { var, iter, body } => self.lower_for(var, iter, body),
            HirStmt::Expr(expr) => {
                let _ = self.lower_expr(expr);
            }
        }
    }

    /// Records a `break`/`continue` edge with the visible bindings at that point,
    /// so the loop can give its `Phi`s one incoming value per edge.
    fn record_loop_edge(&mut self, is_break: bool) {
        let block = self.current;
        let visible = self.snapshot_visible();
        if let Some(edges) = self.loop_edges.last_mut() {
            if is_break {
                edges.breaks.push((block, visible));
            } else {
                edges.continues.push((block, visible));
            }
        }
    }

    /// Emits one `Phi` per assigned variable at loop entry. Returns
    /// `(name, phi, initial)` triples; incoming backedges are patched later.
    fn loop_entry_phis(
        &mut self,
        visible_before: &HashMap<String, ValueId>,
        body_text: &str,
        preheader: BlockId,
        backedge_placeholder: BlockId,
    ) -> Vec<(String, ValueId, ValueId)> {
        let mut loop_phis = Vec::new();
        let mut ordered: Vec<(&String, &ValueId)> = visible_before.iter().collect();
        ordered.sort();
        for (name, initial) in ordered {
            if !assigned_in(body_text, name) {
                continue;
            }
            let ty = self.known_value_type(*initial).unwrap_or(Ty::Unknown);
            let destination = self.fresh();
            self.emit(IrInstr::Phi {
                dst: destination,
                incoming: vec![(preheader, *initial), (backedge_placeholder, *initial)],
                ty,
            });
            self.bind(name, destination);
            loop_phis.push((name.clone(), destination, *initial));
        }
        loop_phis
    }

    /// Emits the loop-exit block contents: when the loop has `break`s, every
    /// assigned variable is merged between the normal exit and each break.
    fn loop_exit_bindings(
        &mut self,
        loop_phis: &[(String, ValueId, ValueId)],
        exit_block: BlockId,
        breaks: &[(BlockId, HashMap<String, ValueId>)],
    ) {
        for (name, destination, _) in loop_phis {
            if breaks.is_empty() {
                self.bind(name, *destination);
                continue;
            }
            let ty = self.known_value_type(*destination).unwrap_or(Ty::Unknown);
            let merged = self.fresh();
            let mut incoming = vec![(exit_block, *destination)];
            for (block, values) in breaks {
                incoming.push((*block, values.get(name).copied().unwrap_or(*destination)));
            }
            self.emit(IrInstr::Phi {
                dst: merged,
                incoming,
                ty,
            });
            self.bind(name, merged);
        }
    }

    fn lower_while(&mut self, condition: &HirExpr, body: &HirBlock) {
        self.ensure_open();
        let preheader = self.current;
        let visible_before = self.snapshot_visible();
        let condition_block = self.new_block();
        let body_block = self.new_block();
        let after_block = self.new_block();
        self.terminate(IrTerminator::Goto(condition_block));

        self.current = condition_block;
        let body_text = format!("{body:?}");
        let loop_phis = self.loop_entry_phis(&visible_before, &body_text, preheader, body_block);
        let condition_value = self.lower_expr(condition);
        self.terminate(IrTerminator::Branch {
            condition: condition_value,
            then_block: body_block,
            else_block: after_block,
        });

        self.current = body_block;
        self.break_targets.push((after_block, condition_block));
        self.loop_edges.push(LoopEdges::default());
        let _ = self.lower_block(body);
        self.break_targets.pop();
        let edges = self.loop_edges.pop().unwrap_or_default();
        let body_open = !self.terminated();
        let backedge_block = self.current;
        let body_values = self.snapshot_visible();
        if body_open {
            self.terminate(IrTerminator::Goto(condition_block));
        }
        let mut backedges = Vec::new();
        if body_open {
            backedges.push((backedge_block, body_values));
        }
        backedges.extend(edges.continues);
        for (name, destination, initial) in &loop_phis {
            let mut incoming = vec![(preheader, *initial)];
            for (block, values) in &backedges {
                incoming.push((*block, values.get(name).copied().unwrap_or(*initial)));
            }
            self.patch_phi(*destination, incoming);
        }
        self.current = after_block;
        self.loop_exit_bindings(&loop_phis, condition_block, &edges.breaks);
    }

    /// `for x in list` as an index-driven SSA loop (same phi scheme as `while`).
    /// `continue` and the end of the body meet in a `step` block that advances
    /// the index.
    fn lower_for_list(&mut self, var: &str, iter: &HirExpr, element: &Ty, body: &HirBlock) {
        let source = self.lower_expr(iter);
        let length = self.fresh();
        self.emit(IrInstr::MethodCall {
            dst: Some(length),
            method: "length".to_string(),
            receiver: source,
            args: Vec::new(),
            ty: Ty::Int,
        });
        let zero = self.const_value("0", Ty::Int);
        let preheader = self.current;
        let visible_before = self.snapshot_visible();
        let condition_block = self.new_block();
        let body_block = self.new_block();
        // A separate `step` block is only needed when `continue` must rejoin the
        // body's fall-through; otherwise the increment stays at the end of the
        // body so the loop's backedge block is the block holding the last uses.
        let body_text = format!("{body:?}");
        let needs_step = body_text.contains("Continue");
        let step_block = if needs_step { self.new_block() } else { body_block };
        let after_block = self.new_block();
        self.terminate(IrTerminator::Goto(condition_block));

        self.current = condition_block;
        let index = self.fresh();
        self.emit(IrInstr::Phi {
            dst: index,
            incoming: vec![(preheader, zero), (step_block, zero)],
            ty: Ty::Int,
        });
        let loop_phis = self.loop_entry_phis(&visible_before, &body_text, preheader, step_block);
        let has_next = self.fresh();
        self.emit(IrInstr::Binary {
            dst: has_next,
            op: BinOp::Lt,
            left: index,
            right: length,
            ty: Ty::Bool,
        });
        self.terminate(IrTerminator::Branch {
            condition: has_next,
            then_block: body_block,
            else_block: after_block,
        });

        self.current = body_block;
        self.break_targets.push((after_block, step_block));
        self.loop_edges.push(LoopEdges::default());
        self.locals.push(HashMap::new());
        let item = self.fresh();
        self.emit(IrInstr::Index {
            dst: item,
            object: source,
            index,
            ty: element.clone(),
        });
        self.locals
            .last_mut()
            .expect("loop scope")
            .insert(var.to_string(), item);
        let _ = self.lower_block_contents(body);
        self.locals.pop();
        self.break_targets.pop();
        let edges = self.loop_edges.pop().unwrap_or_default();
        let body_open = !self.terminated();
        let mut step_preds = Vec::new();
        if body_open {
            let values = self.snapshot_visible();
            step_preds.push((self.current, values));
            if needs_step {
                self.terminate(IrTerminator::Goto(step_block));
            }
        }
        step_preds.extend(edges.continues);

        // Step: merge the incoming bindings, then advance the index. Without
        // `continue` this runs inline at the end of the body block.
        let step_block = if needs_step { step_block } else { self.current };
        if needs_step {
            self.current = step_block;
        }
        let mut step_values: HashMap<String, ValueId> = HashMap::new();
        if step_preds.len() == 1 {
            step_values = step_preds[0].1.clone();
        } else {
            for (name, destination, initial) in &loop_phis {
                if step_preds.is_empty() {
                    break;
                }
                let ty = self.known_value_type(*destination).unwrap_or(Ty::Unknown);
                    let merged = self.fresh();
                let incoming = step_preds
                    .iter()
                    .map(|(block, values)| (*block, values.get(name).copied().unwrap_or(*initial)))
                    .collect();
                self.emit(IrInstr::Phi {
                    dst: merged,
                    incoming,
                    ty,
                });
                step_values.insert(name.clone(), merged);
            }
        }
        let reachable = !step_preds.is_empty();
        if reachable {
            let one = self.const_value("1", Ty::Int);
            let next_index = self.fresh();
            self.emit(IrInstr::Binary {
                dst: next_index,
                op: BinOp::Add,
                left: index,
                right: one,
                ty: Ty::Int,
            });
            self.terminate(IrTerminator::Goto(condition_block));
            self.patch_phi(index, vec![(preheader, zero), (step_block, next_index)]);
        } else {
            if !self.terminated() {
                self.terminate(IrTerminator::Unreachable);
            }
            self.patch_phi(index, vec![(preheader, zero)]);
        }
        for (name, destination, initial) in &loop_phis {
            let mut incoming = vec![(preheader, *initial)];
            if reachable {
                incoming.push((step_block, step_values.get(name).copied().unwrap_or(*initial)));
            }
            self.patch_phi(*destination, incoming);
        }
        self.current = after_block;
        self.loop_exit_bindings(&loop_phis, condition_block, &edges.breaks);
    }

    fn lower_for(&mut self, var: &str, iter: &HirExpr, body: &HirBlock) {
        if let Ty::List(element) = &iter.ty {
            let element = (**element).clone();
            self.lower_for_list(var, iter, &element, body);
            return;
        }
        let source = self.lower_expr(iter);
        let iterator = self.fresh();
        self.emit(IrInstr::IterInit {
            dst: iterator,
            source,
            ty: iter.ty.clone(),
        });

        let condition_block = self.new_block();
        let body_block = self.new_block();
        let after_block = self.new_block();
        self.terminate(IrTerminator::Goto(condition_block));

        self.current = condition_block;
        let has_next = self.fresh();
        self.emit(IrInstr::IterHasNext {
            dst: has_next,
            iter: iterator,
        });
        self.terminate(IrTerminator::Branch {
            condition: has_next,
            then_block: body_block,
            else_block: after_block,
        });

        self.current = body_block;
        self.break_targets.push((after_block, condition_block));
        self.loop_edges.push(LoopEdges::default());
        self.locals.push(HashMap::new());
        let item = self.fresh();
        self.emit(IrInstr::IterNext {
            dst: item,
            iter: iterator,
            ty: Ty::Unknown,
        });
        self.locals
            .last_mut()
            .expect("loop scope")
            .insert(var.to_string(), item);
        let _ = self.lower_block_contents(body);
        self.locals.pop();
        self.break_targets.pop();
        self.loop_edges.pop();
        if !self.terminated() {
            self.terminate(IrTerminator::Goto(condition_block));
        }
        self.current = after_block;
    }

    /// Names visible before a branch construct that some branch re-binds.
    fn assigned_names(before: &HashMap<String, ValueId>, texts: &[String]) -> Vec<String> {
        let mut names: Vec<String> = before
            .keys()
            .filter(|name| texts.iter().any(|text| assigned_in(text, name)))
            .cloned()
            .collect();
        names.sort();
        names
    }

    /// At a join block: give every re-bound variable a `Phi` over the branches
    /// that reach it (or reuse the single/common value).
    fn merge_branch_bindings(
        &mut self,
        names: &[String],
        before: &HashMap<String, ValueId>,
        edges: &[(BlockId, HashMap<String, ValueId>)],
    ) {
        for name in names {
            let Some(initial) = before.get(name).copied() else { continue };
            let incoming: Vec<(BlockId, ValueId)> = edges
                .iter()
                .map(|(block, values)| (*block, values.get(name).copied().unwrap_or(initial)))
                .collect();
            let Some(&(_, first)) = incoming.first() else { continue };
            if incoming.iter().all(|(_, value)| *value == first) {
                self.bind(name, first);
                continue;
            }
            let ty = self.known_value_type(initial).unwrap_or(Ty::Unknown);
            let merged = self.fresh();
            self.emit(IrInstr::Phi {
                dst: merged,
                incoming,
                ty,
            });
            self.bind(name, merged);
        }
    }

    fn lower_if(
        &mut self,
        condition: &HirExpr,
        then_block: &HirBlock,
        else_block: Option<&HirBlock>,
        ty: &Ty,
    ) -> ValueId {
        let condition_value = self.lower_expr(condition);
        let then_id = self.new_block();
        let else_id = self.new_block();
        let merge_id = self.new_block();
        self.terminate(IrTerminator::Branch {
            condition: condition_value,
            then_block: then_id,
            else_block: else_id,
        });

        let visible_before = self.snapshot_visible();
        let branch_texts = [format!("{then_block:?}"), format!("{else_block:?}")];
        let assigned = Self::assigned_names(&visible_before, &branch_texts);
        self.current = then_id;
        let then_result = self.lower_block(then_block);
        let then_open = !self.terminated();
        let then_predecessor = self.current;
        let then_values = self.snapshot_visible();
        let then_value =
            then_result.unwrap_or_else(|| if then_open { self.unit() } else { self.fresh() });
        if then_open {
            self.terminate(IrTerminator::Goto(merge_id));
        }
        self.restore_visible(&visible_before);

        self.current = else_id;
        let else_result = else_block.and_then(|block| self.lower_block(block));
        let else_open = !self.terminated();
        let else_predecessor = self.current;
        let else_values = self.snapshot_visible();
        let else_value =
            else_result.unwrap_or_else(|| if else_open { self.unit() } else { self.fresh() });
        if else_open {
            self.terminate(IrTerminator::Goto(merge_id));
        }

        self.current = merge_id;
        let mut incoming = Vec::new();
        let mut edges = Vec::new();
        if then_open {
            incoming.push((then_predecessor, then_value));
            edges.push((then_predecessor, then_values));
        }
        if else_open {
            incoming.push((else_predecessor, else_value));
            edges.push((else_predecessor, else_values));
        }
        self.merge_branch_bindings(&assigned, &visible_before, &edges);
        if *ty == Ty::Void {
            return self.unit();
        }
        let dst = self.fresh();
        self.emit(IrInstr::Phi {
            dst,
            incoming,
            ty: ty.clone(),
        });
        dst
    }

    fn lower_match(&mut self, subject: &HirExpr, arms: &[crate::hir::HirArm], ty: &Ty) -> ValueId {
        let subject_value = self.lower_expr(subject);
        let merge_block = self.new_block();
        let mut test_block = self.current;
        let mut incoming = Vec::new();
        let mut edges = Vec::new();
        let visible_before = self.snapshot_visible();
        let arm_texts: Vec<String> = arms.iter().map(|arm| format!("{:?}", arm.body)).collect();
        let assigned = Self::assigned_names(&visible_before, &arm_texts);

        for arm in arms {
            let arm_block = self.new_block();
            let next_test = self.new_block();
            self.current = test_block;
            let test = self.fresh();
            self.emit(IrInstr::PatternTest {
                dst: test,
                subject: subject_value,
                pattern: format!("{:?}", arm.pattern),
            });
            self.terminate(IrTerminator::Branch {
                condition: test,
                then_block: arm_block,
                else_block: next_test,
            });

            self.current = arm_block;
            self.locals.push(HashMap::new());
            self.bind_pattern(subject_value, &arm.pattern, Vec::new());
            let body_block = if arm.guard.is_some() {
                self.new_block()
            } else {
                arm_block
            };
            if let Some(guard) = &arm.guard {
                let guard_value = self.lower_expr(guard);
                self.terminate(IrTerminator::Branch {
                    condition: guard_value,
                    then_block: body_block,
                    else_block: next_test,
                });
                self.current = body_block;
            }
            let body_value = self.lower_block_contents(&arm.body).unwrap_or_else(|| {
                if !self.terminated() {
                    self.unit()
                } else {
                    self.fresh()
                }
            });
            let body_open = !self.terminated();
            if body_open {
                incoming.push((self.current, body_value));
                edges.push((self.current, self.snapshot_visible()));
                self.terminate(IrTerminator::Goto(merge_block));
            }
            self.restore_visible(&visible_before);
            self.locals.pop();
            test_block = next_test;
        }

        self.current = test_block;
        if !self.terminated() {
            self.terminate(IrTerminator::Unreachable);
        }
        self.current = merge_block;
        self.merge_branch_bindings(&assigned, &visible_before, &edges);
        if *ty == Ty::Void {
            return self.unit();
        }
        let dst = self.fresh();
        self.emit(IrInstr::Phi {
            dst,
            incoming,
            ty: ty.clone(),
        });
        dst
    }

    fn bind_pattern(&mut self, subject: ValueId, pattern: &crate::ast::Pattern, path: Vec<String>) {
        match pattern {
            crate::ast::Pattern::Ident(name) if name == "None" => {}
            crate::ast::Pattern::Ident(name) => {
                let subject_ty = self.known_value_type(subject).unwrap_or(Ty::Unknown);
                let ty = if path.is_empty() {
                    subject_ty
                } else {
                    match subject_ty {
                        Ty::Applied(name, args) if name == "Option" && args.len() == 1 && path.len() == 1 => args[0].clone(),
                        _ => Ty::Unknown,
                    }
                };
                let dst = self.fresh();
                self.emit(IrInstr::PatternBind {
                    dst,
                    subject,
                    name: name.clone(),
                    path,
                    ty,
                });
                self.locals
                    .last_mut()
                    .expect("match scope")
                    .insert(name.clone(), dst);
            }
            crate::ast::Pattern::Variant(_, fields) => {
                for (field, subpattern) in fields {
                    let mut nested = path.clone();
                    nested.push(field.clone());
                    self.bind_pattern(subject, subpattern, nested);
                }
            }
            crate::ast::Pattern::Wildcard
            | crate::ast::Pattern::Literal(_)
            | crate::ast::Pattern::Range(..) => {}
        }
    }

    fn lower_expr(&mut self, expression: &HirExpr) -> ValueId {
        match &expression.kind {
            HirKind::Int(value) => self.const_value(value.to_string(), expression.ty.clone()),
            HirKind::Sized(value, _) => self.const_value(value.to_string(), expression.ty.clone()),
            HirKind::Float(value) => self.const_value(format!("{value:?}"), expression.ty.clone()),
            HirKind::Float32(value) => {
                self.const_value(format!("{value:?}f32"), expression.ty.clone())
            }
            // Keep the source string unescaped in the IR. Each backend owns
            // the final literal encoding; the C emitter uses the same helper
            // as the HIR emitter instead of assuming Rust debug escaping is
            // valid C for every Unicode/control character.
            HirKind::Str(value) => self.const_value(value.clone(), expression.ty.clone()),
            HirKind::Char(value) => self.const_value(format!("{value:?}"), expression.ty.clone()),
            HirKind::Bool(value) => self.const_value(value.to_string(), expression.ty.clone()),
            HirKind::Unit(value, unit) => {
                let value = self.lower_expr(value);
                let dst = self.fresh();
                self.emit(IrInstr::Opaque {
                    dst: Some(dst),
                    op: format!("unit<{unit}>"),
                    inputs: vec![value],
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Local(name) => self.lookup(name).unwrap_or_else(|| {
                let dst = self.fresh();
                self.emit(IrInstr::Opaque {
                    dst: Some(dst),
                    op: format!("unbound_local<{name}>"),
                    inputs: Vec::new(),
                    ty: expression.ty.clone(),
                });
                dst
            }),
            HirKind::Global(name) => {
                let dst = self.fresh();
                self.emit(IrInstr::Global {
                    dst,
                    name: name.clone(),
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Unary(op, operand) => {
                let operand = self.lower_expr(operand);
                let dst = self.fresh();
                self.emit(IrInstr::Unary {
                    dst,
                    op: *op,
                    operand,
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Binary(op, left_expr, right_expr)
                if matches!(op, BinOp::And | BinOp::Or) && left_expr.ty == Ty::Bool && right_expr.ty == Ty::Bool =>
            {
                // Short-circuit: the right operand only runs when the left one
                // does not already decide the result.
                let left = self.lower_expr(left_expr);
                let left_block = self.current;
                let right_block = self.new_block();
                let merge_block = self.new_block();
                let (then_block, else_block) = if *op == BinOp::And {
                    (right_block, merge_block)
                } else {
                    (merge_block, right_block)
                };
                self.terminate(IrTerminator::Branch {
                    condition: left,
                    then_block,
                    else_block,
                });
                self.current = right_block;
                let right = self.lower_expr(right_expr);
                let right_end = self.current;
                self.terminate(IrTerminator::Goto(merge_block));
                self.current = merge_block;
                let dst = self.fresh();
                self.emit(IrInstr::Phi {
                    dst,
                    incoming: vec![(left_block, left), (right_end, right)],
                    ty: Ty::Bool,
                });
                dst
            }
            HirKind::Binary(op, left, right) => {
                let left = self.lower_expr(left);
                let right = self.lower_expr(right);
                let dst = self.fresh();
                self.emit(IrInstr::Binary {
                    dst,
                    op: *op,
                    left,
                    right,
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Range(start, kind, end, step) => {
                let mut inputs = vec![self.lower_expr(start), self.lower_expr(end)];
                if let Some(step) = step {
                    inputs.push(self.lower_expr(step));
                }
                let dst = self.fresh();
                self.emit(IrInstr::Opaque {
                    dst: Some(dst),
                    op: format!("range<{kind:?}>"),
                    inputs,
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Call { callee, args, .. } => {
                let callee_name = match &callee.kind {
                    HirKind::Global(name) => name.clone(),
                    _ => {
                        let _ = self.lower_expr(callee);
                        "<dynamic>".to_string()
                    }
                };
                let args: Vec<ValueId> =
                    args.iter().map(|arg| self.lower_expr(&arg.value)).collect();
                let dst = if expression.ty == Ty::Void {
                    None
                } else {
                    Some(self.fresh())
                };
                self.emit(IrInstr::Call {
                    dst,
                    callee: callee_name,
                    args,
                    ty: expression.ty.clone(),
                });
                dst.unwrap_or_else(|| self.unit())
            }
            HirKind::MethodCall {
                recv, method, args, ..
            } => {
                let receiver = self.lower_expr(recv);
                let args: Vec<ValueId> =
                    args.iter().map(|arg| self.lower_expr(&arg.value)).collect();
                match (method.as_str(), args.as_slice()) {
                    ("send", [value]) => {
                        self.emit(IrInstr::ChannelSend {
                            channel: receiver,
                            value: *value,
                        });
                        self.unit()
                    }
                    ("receive", []) => {
                        let dst = self.fresh();
                        self.emit(IrInstr::ChannelReceive {
                            dst,
                            channel: receiver,
                            ty: expression.ty.clone(),
                        });
                        dst
                    }
                    ("close", []) => {
                        self.emit(IrInstr::ChannelClose { channel: receiver });
                        self.unit()
                    }
                    ("join", []) => {
                        let dst = self.fresh();
                        self.emit(IrInstr::TaskJoin {
                            dst,
                            task: receiver,
                            ty: expression.ty.clone(),
                        });
                        dst
                    }
                    _ => {
                        let dst = if expression.ty == Ty::Void {
                            None
                        } else {
                            Some(self.fresh())
                        };
                        self.emit(IrInstr::MethodCall {
                            dst,
                            method: method.clone(),
                            receiver,
                            args,
                            ty: expression.ty.clone(),
                        });
                        dst.unwrap_or_else(|| self.unit())
                    }
                }
            }
            HirKind::Field(object, field) => {
                let object = self.lower_expr(object);
                let dst = self.fresh();
                self.emit(IrInstr::Field {
                    dst,
                    object,
                    field: field.clone(),
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Index(object, index) => {
                let object = self.lower_expr(object);
                let index = self.lower_expr(index);
                let dst = self.fresh();
                self.emit(IrInstr::Index {
                    dst,
                    object,
                    index,
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::If(condition, then_block, else_block) => {
                self.lower_if(condition, then_block, else_block.as_ref(), &expression.ty)
            }
            HirKind::Block(block) | HirKind::Loop(block) => {
                self.lower_block(block).unwrap_or_else(|| self.unit())
            }
            HirKind::Lambda(params, _) => {
                let dst = self.fresh();
                self.emit(IrInstr::Opaque {
                    dst: Some(dst),
                    op: format!("lambda<{}>", params.join(",")),
                    inputs: Vec::new(),
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::List(values) | HirKind::Set(values) => {
                let fields = values.iter().map(|value| self.lower_expr(value)).collect();
                let dst = self.fresh();
                self.emit(IrInstr::Aggregate {
                    dst,
                    kind: "collection".to_string(),
                    fields,
                    field_names: Vec::new(),
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Map(values) => {
                let fields = values
                    .iter()
                    .flat_map(|(key, value)| [self.lower_expr(key), self.lower_expr(value)])
                    .collect();
                let dst = self.fresh();
                self.emit(IrInstr::Aggregate {
                    dst,
                    kind: "map".to_string(),
                    fields,
                    field_names: Vec::new(),
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::EmptyCollection(name, _) => {
                let dst = self.fresh();
                self.emit(IrInstr::Aggregate {
                    dst,
                    kind: format!("empty_{name}"),
                    fields: Vec::new(),
                    field_names: Vec::new(),
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Try(value, handler) => {
                self.lower_try(value, handler.as_deref(), &expression.ty)
            }
            HirKind::Within(value, unit) => {
                let inputs = vec![self.lower_expr(value), self.lower_expr(unit)];
                let dst = self.fresh();
                self.emit(IrInstr::Opaque {
                    dst: Some(dst),
                    op: "within".to_string(),
                    inputs,
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Approximately(value, expected, tolerance) => {
                let inputs = vec![
                    self.lower_expr(value),
                    self.lower_expr(expected),
                    self.lower_expr(tolerance),
                ];
                let dst = self.fresh();
                self.emit(IrInstr::Opaque {
                    dst: Some(dst),
                    op: "approximately".to_string(),
                    inputs,
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::As(value, target) => {
                let value = self.lower_expr(value);
                let dst = self.fresh();
                self.emit(IrInstr::Opaque {
                    dst: Some(dst),
                    op: format!("as<{target}>"),
                    inputs: vec![value],
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Record { name, fields, .. } => {
                let field_names = fields.iter().map(|(name, _)| name.clone()).collect();
                let fields = fields.iter().map(|(_, value)| self.lower_expr(value)).collect();
                let dst = self.fresh();
                self.emit(IrInstr::Aggregate {
                    dst,
                    kind: format!("record<{name}>"),
                    fields,
                    field_names,
                    ty: expression.ty.clone(),
                });
                dst
            }
            HirKind::Match(subject, arms) => self.lower_match(subject, arms, &expression.ty),
            HirKind::Spawn(block) => self.lower_spawn(block, false, &expression.ty),
            HirKind::SpawnScope(block) => self.lower_spawn(block, true, &expression.ty),
            HirKind::Channel(_, capacity) => {
                let capacity = capacity.as_ref().map(|capacity| self.lower_expr(capacity));
                let dst = self.fresh();
                self.emit(IrInstr::ChannelNew {
                    dst,
                    capacity,
                    ty: expression.ty.clone(),
                });
                dst
            }
        }
    }

    fn lower_try(&mut self, value: &HirExpr, handler: Option<&HirExpr>, ty: &Ty) -> ValueId {
        let value = self.lower_expr(value);
        let normal_block = self.new_block();
        let catch_block = self.new_block();
        let merge_block = self.new_block();
        let check = self.fresh();
        self.emit(IrInstr::TryCheck { dst: check, value });
        self.terminate(IrTerminator::Branch {
            condition: check,
            then_block: normal_block,
            else_block: catch_block,
        });

        self.current = normal_block;
        let normal = self.fresh();
        self.emit(IrInstr::TryValue {
            dst: normal,
            value,
            ty: ty.clone(),
        });
        self.terminate(IrTerminator::Goto(merge_block));

        self.current = catch_block;
        let caught = if let Some(handler) = handler {
            self.lower_expr(handler)
        } else {
            let error = self.fresh();
            self.emit(IrInstr::TryError {
                dst: error,
                value,
                ty: ty.clone(),
            });
            error
        };
        let catch_predecessor = self.current;
        let catch_open = !self.terminated();
        if catch_open {
            self.terminate(IrTerminator::Goto(merge_block));
        }

        self.current = merge_block;
        let dst = self.fresh();
        let mut incoming = vec![(normal_block, normal)];
        if catch_open {
            incoming.push((catch_predecessor, caught));
        }
        self.emit(IrInstr::Phi {
            dst,
            incoming,
            ty: ty.clone(),
        });
        dst
    }

    fn lower_spawn(&mut self, block: &HirBlock, scoped: bool, ty: &Ty) -> ValueId {
        let caller = self.current;
        let caller_locals = self.locals.clone();
        let region = self.new_block();
        self.current = region;
        self.locals = caller_locals.clone();
        self.region_depth += 1;
        let result = self.lower_block(block);
        self.region_depth -= 1;
        if !self.terminated() {
            self.terminate(IrTerminator::RegionReturn(result));
        }
        self.locals = caller_locals;
        self.current = caller;
        let dst = self.fresh();
        self.emit(IrInstr::Spawn {
            dst,
            region,
            scoped,
            ty: ty.clone(),
        });
        dst
    }
}

fn defined_value_type(instruction: &IrInstr) -> Option<(ValueId, Ty)> {
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

/// Conservative check (via the HIR debug form) that `name` is re-bound by an
/// assignment somewhere in `text`; unassigned variables need no loop `Phi`.
fn assigned_in(text: &str, name: &str) -> bool {
    text.contains(&format!("Assign {{ name: {name:?}"))
}

pub fn lower(program: &HirProgram) -> IrProgram {
    IrProgram {
        functions: program.functions.iter().map(lower_function).collect(),
    }
}

fn lower_function(function: &HirFunction) -> IrFunction {
    let mut builder = Builder::new(function);
    for (index, (name, ty)) in function.params.iter().enumerate() {
        let value = builder.fresh();
        builder.emit(IrInstr::Param {
            dst: value,
            index,
            name: name.clone(),
            ty: ty.clone(),
        });
        builder.locals[0].insert(name.clone(), value);
    }
    let tail = builder.lower_block(&function.body);
    if !builder.terminated() {
        builder.terminate(IrTerminator::Return(tail));
    }
    for block in &mut builder.function.blocks {
        if block.terminator.is_none() {
            block.terminator = Some(IrTerminator::Unreachable);
        }
    }
    builder.function
}

pub fn verify(program: &IrProgram) -> VerifyReport {
    let mut report = VerifyReport::default();
    for function in &program.functions {
        report.blocks += function.blocks.len();
        let mut predecessors = vec![Vec::<BlockId>::new(); function.blocks.len()];
        for block in &function.blocks {
            report.instructions += block.instructions.len();
            if block.terminator.is_none() {
                report.unterminated += 1;
                report.violations.push(format!(
                    "{}: bb{} has no terminator",
                    function.name, block.id
                ));
            }
            for instruction in &block.instructions {
                if matches!(instruction, IrInstr::Opaque { .. }) {
                    report.opaque += 1;
                }
            }
            if let Some(terminator) = &block.terminator {
                let targets = match terminator {
                    IrTerminator::Goto(target) => vec![*target],
                    IrTerminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => vec![*then_block, *else_block],
                    IrTerminator::Return(_)
                    | IrTerminator::RegionReturn(_)
                    | IrTerminator::Unreachable => Vec::new(),
                };
                for target in targets {
                    if target >= function.blocks.len() {
                        report.violations.push(format!(
                            "{}: bb{} targets missing bb{}",
                            function.name, block.id, target
                        ));
                    } else {
                        predecessors[target].push(block.id);
                    }
                }
            }
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let IrInstr::Phi { incoming, .. } = instruction {
                    if incoming.is_empty() {
                        report.violations.push(format!(
                            "{}: bb{} phi has no incoming edge",
                            function.name, block.id
                        ));
                        continue;
                    }
                    let actual = predecessors.get(block.id).cloned().unwrap_or_default();
                    let mut seen = std::collections::HashSet::new();
                    for (predecessor, _) in incoming {
                        if *predecessor >= function.blocks.len() {
                            report.violations.push(format!(
                                "{}: bb{} phi references missing predecessor bb{}",
                                function.name, block.id, predecessor
                            ));
                        } else if !actual.contains(predecessor) {
                            report.violations.push(format!(
                                "{}: bb{} phi predecessor bb{} is not a CFG predecessor",
                                function.name, block.id, predecessor
                            ));
                        }
                        if !seen.insert(*predecessor) {
                            report.violations.push(format!(
                                "{}: bb{} phi repeats predecessor bb{}",
                                function.name, block.id, predecessor
                            ));
                        }
                    }
                    for predecessor in actual {
                        if !seen.contains(&predecessor) {
                            report.violations.push(format!(
                                "{}: bb{} phi is missing predecessor bb{}",
                                function.name, block.id, predecessor
                            ));
                        }
                    }
                }
            }
        }
    }
    report
}

pub fn dump(program: &IrProgram) -> String {
    let mut out = String::new();
    for function in &program.functions {
        let params = function
            .params
            .iter()
            .map(|(name, ty)| format!("{name}: {}", ty.describe()))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "ir fn {}({params}) -> {}",
            function.name,
            function.ret.describe()
        );
        for block in &function.blocks {
            let _ = writeln!(out, "bb{}:", block.id);
            for instruction in &block.instructions {
                let _ = writeln!(out, "  {}", display_instruction(instruction));
            }
            if let Some(terminator) = &block.terminator {
                let _ = writeln!(out, "  {}", display_terminator(terminator));
            }
        }
        out.push('\n');
    }
    out
}

fn display_instruction(instruction: &IrInstr) -> String {
    match instruction {
        IrInstr::Param {
            dst,
            index,
            name,
            ty,
        } => format!("%{dst} = param {index} {name}: {}", ty.describe()),
        IrInstr::Const { dst, value, ty } => format!("%{dst} = const {value}: {}", ty.describe()),
        IrInstr::Global { dst, name, ty } => format!("%{dst} = global @{name}: {}", ty.describe()),
        IrInstr::Move { dst, source, .. } => format!("%{dst} = move %{source}"),
        IrInstr::StoreLocal { name, value } => format!("store {name} <- %{value}"),
        IrInstr::Unary {
            dst, op, operand, ..
        } => format!("%{dst} = {op:?} %{operand}"),
        IrInstr::Binary {
            dst,
            op,
            left,
            right,
            ..
        } => format!("%{dst} = %{left} {op:?} %{right}"),
        IrInstr::Call {
            dst, callee, args, ..
        } => format!("{}call {callee}({})", result_prefix(*dst), value_list(args)),
        IrInstr::MethodCall {
            dst,
            method,
            receiver,
            args,
            ..
        } => format!(
            "{}method %{receiver}.{method}({})",
            result_prefix(*dst),
            value_list(args)
        ),
        IrInstr::Field {
            dst, object, field, ..
        } => format!("%{dst} = field %{object}.{field}"),
        IrInstr::Index {
            dst, object, index, ..
        } => format!("%{dst} = index %{object}[%{index}]"),
        IrInstr::Aggregate {
            dst, kind, fields, ..
        } => format!("%{dst} = {kind}({})", value_list(fields)),
        IrInstr::IterInit { dst, source, .. } => format!("%{dst} = iter_init %{source}"),
        IrInstr::IterHasNext { dst, iter } => format!("%{dst} = iter_has_next %{iter}"),
        IrInstr::IterNext { dst, iter, .. } => format!("%{dst} = iter_next %{iter}"),
        IrInstr::PatternTest {
            dst,
            subject,
            pattern,
        } => format!("%{dst} = pattern_test %{subject} {pattern}"),
        IrInstr::PatternBind {
            dst,
            subject,
            name,
            path,
            ..
        } => format!(
            "%{dst} = pattern_bind %{subject} {name} path={}",
            path.join(".")
        ),
        IrInstr::TryCheck { dst, value } => format!("%{dst} = try_check %{value}"),
        IrInstr::TryValue { dst, value, .. } => format!("%{dst} = try_value %{value}"),
        IrInstr::TryError { dst, value, .. } => format!("%{dst} = try_error %{value}"),
        IrInstr::Spawn {
            dst,
            region,
            scoped,
            ..
        } => format!(
            "%{dst} = spawn{} bb{region}",
            if *scoped { "_scope" } else { "" }
        ),
        IrInstr::ChannelNew { dst, capacity, .. } => format!(
            "%{dst} = channel({})",
            capacity.map_or_else(|| "unbounded".to_string(), |value| format!("%{value}"))
        ),
        IrInstr::ChannelSend { channel, value } => format!("channel_send %{channel}, %{value}"),
        IrInstr::ChannelReceive { dst, channel, .. } => {
            format!("%{dst} = channel_receive %{channel}")
        }
        IrInstr::ChannelClose { channel } => format!("channel_close %{channel}"),
        IrInstr::TaskJoin { dst, task, .. } => format!("%{dst} = task_join %{task}"),
        IrInstr::Phi { dst, incoming, .. } => format!(
            "%{dst} = phi {}",
            incoming
                .iter()
                .map(|(block, value)| format!("[bb{block}, %{value}]"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        IrInstr::Opaque {
            dst, op, inputs, ..
        } => format!("{}opaque {op}({})", result_prefix(*dst), value_list(inputs)),
        IrInstr::Retain { value } => format!("retain %{value}"),
        IrInstr::Release { value } => format!("release %{value}"),
    }
}

fn result_prefix(dst: Option<ValueId>) -> String {
    dst.map_or_else(String::new, |dst| format!("%{dst} = "))
}

fn value_list(values: &[ValueId]) -> String {
    values
        .iter()
        .map(|value| format!("%{value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_terminator(terminator: &IrTerminator) -> String {
    match terminator {
        IrTerminator::Goto(target) => format!("goto bb{target}"),
        IrTerminator::Branch {
            condition,
            then_block,
            else_block,
        } => format!("br %{condition} -> bb{then_block}, bb{else_block}"),
        IrTerminator::Return(value) => {
            value.map_or_else(|| "ret".to_string(), |value| format!("ret %{value}"))
        }
        IrTerminator::RegionReturn(value) => value.map_or_else(
            || "region_ret".to_string(),
            |value| format!("region_ret %{value}"),
        ),
        IrTerminator::Unreachable => "unreachable".to_string(),
    }
}
