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
    Param { dst: ValueId, index: usize, name: String, ty: Ty },
    Const { dst: ValueId, value: String, ty: Ty },
    Global { dst: ValueId, name: String, ty: Ty },
    Move { dst: ValueId, source: ValueId, ty: Ty },
    StoreLocal { name: String, value: ValueId },
    Unary { dst: ValueId, op: UnaryOp, operand: ValueId, ty: Ty },
    Binary { dst: ValueId, op: BinOp, left: ValueId, right: ValueId, ty: Ty },
    Call { dst: Option<ValueId>, callee: String, args: Vec<ValueId>, ty: Ty },
    MethodCall { dst: Option<ValueId>, method: String, receiver: ValueId, args: Vec<ValueId>, ty: Ty },
    Field { dst: ValueId, object: ValueId, field: String, ty: Ty },
    Index { dst: ValueId, object: ValueId, index: ValueId, ty: Ty },
    Aggregate { dst: ValueId, kind: String, fields: Vec<ValueId>, ty: Ty },
    IterInit { dst: ValueId, source: ValueId, ty: Ty },
    IterHasNext { dst: ValueId, iter: ValueId },
    IterNext { dst: ValueId, iter: ValueId, ty: Ty },
    PatternTest { dst: ValueId, subject: ValueId, pattern: String },
    PatternBind { dst: ValueId, subject: ValueId, name: String, path: Vec<String>, ty: Ty },
    TryCheck { dst: ValueId, value: ValueId },
    TryValue { dst: ValueId, value: ValueId, ty: Ty },
    TryError { dst: ValueId, value: ValueId, ty: Ty },
    Phi { dst: ValueId, incoming: Vec<(BlockId, ValueId)>, ty: Ty },
    Opaque { dst: Option<ValueId>, op: String, inputs: Vec<ValueId>, ty: Ty },
    Retain { value: ValueId },
    Release { value: ValueId },
}

#[derive(Debug, Clone)]
pub enum IrTerminator {
    Goto(BlockId),
    Branch { condition: ValueId, then_block: BlockId, else_block: BlockId },
    Return(Option<ValueId>),
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

struct Builder {
    function: IrFunction,
    current: BlockId,
    next_value: ValueId,
    locals: Vec<HashMap<String, ValueId>>,
    break_targets: Vec<(BlockId, BlockId)>,
}

impl Builder {
    fn new(function: &HirFunction) -> Self {
        let entry = IrBlock { id: 0, instructions: Vec::new(), terminator: None };
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
        }
    }

    fn block(&mut self, id: BlockId) -> &mut IrBlock {
        &mut self.function.blocks[id]
    }

    fn new_block(&mut self) -> BlockId {
        let id = self.function.blocks.len();
        self.function.blocks.push(IrBlock { id, instructions: Vec::new(), terminator: None });
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
        self.emit(IrInstr::Const { dst, value: value.into(), ty });
        dst
    }

    fn unit(&mut self) -> ValueId {
        self.const_value("unit", Ty::Void)
    }

    fn lookup(&self, name: &str) -> Option<ValueId> {
        self.locals.iter().rev().find_map(|scope| scope.get(name).copied())
    }

    fn bind(&mut self, name: &str, value: ValueId) {
        if let Some(scope) = self.locals.iter_mut().rev().find(|scope| scope.contains_key(name)) {
            scope.insert(name.to_string(), value);
        } else {
            self.locals.last_mut().expect("an IR local scope always exists").insert(name.to_string(), value);
        }
    }

    fn snapshot_visible(&self) -> HashMap<String, ValueId> {
        let mut visible = HashMap::new();
        for scope in &self.locals {
            visible.extend(scope.iter().map(|(name, value)| (name.clone(), *value)));
        }
        visible
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
                self.emit(IrInstr::StoreLocal { name: name.clone(), value });
                self.bind(name, value);
            }
            HirStmt::Assign { name, value } => {
                let value = self.lower_expr(value);
                self.emit(IrInstr::StoreLocal { name: name.clone(), value });
                self.bind(name, value);
            }
            HirStmt::FieldAssign { target, value } => {
                let target = self.lower_expr(target);
                let value = self.lower_expr(value);
                self.emit(IrInstr::Opaque { dst: None, op: "field_assign".to_string(), inputs: vec![target, value], ty: Ty::Void });
            }
            HirStmt::Return(value) => {
                let value = value.as_ref().map(|value| self.lower_expr(value));
                self.terminate(IrTerminator::Return(value));
            }
            HirStmt::Break(value) => {
                if let Some((break_block, _)) = self.break_targets.last().copied() {
                    if let Some(value) = value {
                        let value = self.lower_expr(value);
                        self.emit(IrInstr::Opaque { dst: None, op: "break_value".to_string(), inputs: vec![value], ty: Ty::Void });
                    }
                    self.terminate(IrTerminator::Goto(break_block));
                } else {
                    self.terminate(IrTerminator::Unreachable);
                }
            }
            HirStmt::Continue => {
                if let Some((_, continue_block)) = self.break_targets.last().copied() {
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

    fn lower_while(&mut self, condition: &HirExpr, body: &HirBlock) {
        self.ensure_open();
        let condition_block = self.new_block();
        let body_block = self.new_block();
        let after_block = self.new_block();
        self.terminate(IrTerminator::Goto(condition_block));

        self.current = condition_block;
        let condition_value = self.lower_expr(condition);
        self.terminate(IrTerminator::Branch { condition: condition_value, then_block: body_block, else_block: after_block });

        self.current = body_block;
        self.break_targets.push((after_block, condition_block));
        let _ = self.lower_block(body);
        self.break_targets.pop();
        if !self.terminated() {
            self.terminate(IrTerminator::Goto(condition_block));
        }
        self.current = after_block;
    }

    fn lower_for(&mut self, var: &str, iter: &HirExpr, body: &HirBlock) {
        let source = self.lower_expr(iter);
        let iterator = self.fresh();
        self.emit(IrInstr::IterInit { dst: iterator, source, ty: iter.ty.clone() });

        let condition_block = self.new_block();
        let body_block = self.new_block();
        let after_block = self.new_block();
        self.terminate(IrTerminator::Goto(condition_block));

        self.current = condition_block;
        let has_next = self.fresh();
        self.emit(IrInstr::IterHasNext { dst: has_next, iter: iterator });
        self.terminate(IrTerminator::Branch { condition: has_next, then_block: body_block, else_block: after_block });

        self.current = body_block;
        self.break_targets.push((after_block, condition_block));
        self.locals.push(HashMap::new());
        let item = self.fresh();
        self.emit(IrInstr::IterNext { dst: item, iter: iterator, ty: Ty::Unknown });
        self.locals.last_mut().expect("loop scope").insert(var.to_string(), item);
        let _ = self.lower_block_contents(body);
        self.locals.pop();
        self.break_targets.pop();
        if !self.terminated() {
            self.terminate(IrTerminator::Goto(condition_block));
        }
        self.current = after_block;
    }

    fn lower_if(&mut self, condition: &HirExpr, then_block: &HirBlock, else_block: Option<&HirBlock>, ty: &Ty) -> ValueId {
        let condition_value = self.lower_expr(condition);
        let then_id = self.new_block();
        let else_id = self.new_block();
        let merge_id = self.new_block();
        self.terminate(IrTerminator::Branch { condition: condition_value, then_block: then_id, else_block: else_id });

        let visible_before = self.snapshot_visible();
        self.current = then_id;
        let then_result = self.lower_block(then_block);
        let then_open = !self.terminated();
        let then_value = then_result.unwrap_or_else(|| if then_open { self.unit() } else { self.fresh() });
        if then_open {
            self.terminate(IrTerminator::Goto(merge_id));
        }
        self.restore_visible(&visible_before);

        self.current = else_id;
        let else_result = else_block.and_then(|block| self.lower_block(block));
        let else_open = !self.terminated();
        let else_value = else_result.unwrap_or_else(|| if else_open { self.unit() } else { self.fresh() });
        if else_open {
            self.terminate(IrTerminator::Goto(merge_id));
        }

        self.current = merge_id;
        let mut incoming = Vec::new();
        if then_open {
            incoming.push((then_id, then_value));
        }
        if else_open {
            incoming.push((else_id, else_value));
        }
        let dst = self.fresh();
        self.emit(IrInstr::Phi { dst, incoming, ty: ty.clone() });
        dst
    }

    fn lower_match(&mut self, subject: &HirExpr, arms: &[crate::hir::HirArm], ty: &Ty) -> ValueId {
        let subject_value = self.lower_expr(subject);
        let merge_block = self.new_block();
        let mut test_block = self.current;
        let mut incoming = Vec::new();

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
            self.terminate(IrTerminator::Branch { condition: test, then_block: arm_block, else_block: next_test });

            self.current = arm_block;
            self.locals.push(HashMap::new());
            self.bind_pattern(subject_value, &arm.pattern, Vec::new());
            let body_block = if arm.guard.is_some() { self.new_block() } else { arm_block };
            if let Some(guard) = &arm.guard {
                let guard_value = self.lower_expr(guard);
                self.terminate(IrTerminator::Branch { condition: guard_value, then_block: body_block, else_block: next_test });
                self.current = body_block;
            }
            let body_value = self.lower_block_contents(&arm.body).unwrap_or_else(|| if !self.terminated() { self.unit() } else { self.fresh() });
            let body_open = !self.terminated();
            if body_open {
                incoming.push((self.current, body_value));
                self.terminate(IrTerminator::Goto(merge_block));
            }
            self.locals.pop();
            test_block = next_test;
        }

        self.current = test_block;
        if !self.terminated() {
            self.terminate(IrTerminator::Unreachable);
        }
        self.current = merge_block;
        let dst = self.fresh();
        self.emit(IrInstr::Phi { dst, incoming, ty: ty.clone() });
        dst
    }

    fn bind_pattern(&mut self, subject: ValueId, pattern: &crate::ast::Pattern, path: Vec<String>) {
        match pattern {
            crate::ast::Pattern::Ident(name) => {
                let dst = self.fresh();
                self.emit(IrInstr::PatternBind {
                    dst,
                    subject,
                    name: name.clone(),
                    path,
                    ty: Ty::Unknown,
                });
                self.locals.last_mut().expect("match scope").insert(name.clone(), dst);
            }
            crate::ast::Pattern::Variant(_, fields) => {
                for (field, subpattern) in fields {
                    let mut nested = path.clone();
                    nested.push(field.clone());
                    self.bind_pattern(subject, subpattern, nested);
                }
            }
            crate::ast::Pattern::Wildcard | crate::ast::Pattern::Literal(_) | crate::ast::Pattern::Range(..) => {}
        }
    }

    fn lower_expr(&mut self, expression: &HirExpr) -> ValueId {
        match &expression.kind {
            HirKind::Int(value) => self.const_value(value.to_string(), expression.ty.clone()),
            HirKind::Sized(value, kind) => self.const_value(format!("{value}{}", kind.name()), expression.ty.clone()),
            HirKind::Float(value) => self.const_value(format!("{value:?}"), expression.ty.clone()),
            HirKind::Float32(value) => self.const_value(format!("{value:?}f32"), expression.ty.clone()),
            HirKind::Str(value) => self.const_value(format!("{value:?}"), expression.ty.clone()),
            HirKind::Char(value) => self.const_value(format!("{value:?}"), expression.ty.clone()),
            HirKind::Bool(value) => self.const_value(value.to_string(), expression.ty.clone()),
            HirKind::Unit(value, unit) => {
                let value = self.lower_expr(value);
                let dst = self.fresh();
                self.emit(IrInstr::Opaque { dst: Some(dst), op: format!("unit<{unit}>"), inputs: vec![value], ty: expression.ty.clone() });
                dst
            }
            HirKind::Local(name) => self.lookup(name).unwrap_or_else(|| {
                let dst = self.fresh();
                self.emit(IrInstr::Opaque { dst: Some(dst), op: format!("unbound_local<{name}>"), inputs: Vec::new(), ty: expression.ty.clone() });
                dst
            }),
            HirKind::Global(name) => {
                let dst = self.fresh();
                self.emit(IrInstr::Global { dst, name: name.clone(), ty: expression.ty.clone() });
                dst
            }
            HirKind::Unary(op, operand) => {
                let operand = self.lower_expr(operand);
                let dst = self.fresh();
                self.emit(IrInstr::Unary { dst, op: *op, operand, ty: expression.ty.clone() });
                dst
            }
            HirKind::Binary(op, left, right) => {
                let left = self.lower_expr(left);
                let right = self.lower_expr(right);
                let dst = self.fresh();
                self.emit(IrInstr::Binary { dst, op: *op, left, right, ty: expression.ty.clone() });
                dst
            }
            HirKind::Range(start, kind, end, step) => {
                let mut inputs = vec![self.lower_expr(start), self.lower_expr(end)];
                if let Some(step) = step {
                    inputs.push(self.lower_expr(step));
                }
                let dst = self.fresh();
                self.emit(IrInstr::Opaque { dst: Some(dst), op: format!("range<{kind:?}>"), inputs, ty: expression.ty.clone() });
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
                let args = args.iter().map(|arg| self.lower_expr(&arg.value)).collect();
                let dst = if expression.ty == Ty::Void { None } else { Some(self.fresh()) };
                self.emit(IrInstr::Call { dst, callee: callee_name, args, ty: expression.ty.clone() });
                dst.unwrap_or_else(|| self.unit())
            }
            HirKind::MethodCall { recv, method, args, .. } => {
                let receiver = self.lower_expr(recv);
                let args = args.iter().map(|arg| self.lower_expr(&arg.value)).collect();
                let dst = if expression.ty == Ty::Void { None } else { Some(self.fresh()) };
                self.emit(IrInstr::MethodCall { dst, method: method.clone(), receiver, args, ty: expression.ty.clone() });
                dst.unwrap_or_else(|| self.unit())
            }
            HirKind::Field(object, field) => {
                let object = self.lower_expr(object);
                let dst = self.fresh();
                self.emit(IrInstr::Field { dst, object, field: field.clone(), ty: expression.ty.clone() });
                dst
            }
            HirKind::Index(object, index) => {
                let object = self.lower_expr(object);
                let index = self.lower_expr(index);
                let dst = self.fresh();
                self.emit(IrInstr::Index { dst, object, index, ty: expression.ty.clone() });
                dst
            }
            HirKind::If(condition, then_block, else_block) => self.lower_if(condition, then_block, else_block.as_ref(), &expression.ty),
            HirKind::Block(block) | HirKind::Loop(block) => self.lower_block(block).unwrap_or_else(|| self.unit()),
            HirKind::Lambda(params, _) => {
                let dst = self.fresh();
                self.emit(IrInstr::Opaque { dst: Some(dst), op: format!("lambda<{}>", params.join(",")), inputs: Vec::new(), ty: expression.ty.clone() });
                dst
            }
            HirKind::List(values) | HirKind::Set(values) => {
                let fields = values.iter().map(|value| self.lower_expr(value)).collect();
                let dst = self.fresh();
                self.emit(IrInstr::Aggregate { dst, kind: "collection".to_string(), fields, ty: expression.ty.clone() });
                dst
            }
            HirKind::Map(values) => {
                let fields = values.iter().flat_map(|(key, value)| [self.lower_expr(key), self.lower_expr(value)]).collect();
                let dst = self.fresh();
                self.emit(IrInstr::Aggregate { dst, kind: "map".to_string(), fields, ty: expression.ty.clone() });
                dst
            }
            HirKind::EmptyCollection(name, _) => {
                let dst = self.fresh();
                self.emit(IrInstr::Aggregate { dst, kind: format!("empty_{name}"), fields: Vec::new(), ty: expression.ty.clone() });
                dst
            }
            HirKind::Try(value, handler) => self.lower_try(value, handler.as_deref(), &expression.ty),
            HirKind::Within(value, unit) => {
                let inputs = vec![self.lower_expr(value), self.lower_expr(unit)];
                let dst = self.fresh();
                self.emit(IrInstr::Opaque { dst: Some(dst), op: "within".to_string(), inputs, ty: expression.ty.clone() });
                dst
            }
            HirKind::Approximately(value, expected, tolerance) => {
                let inputs = vec![self.lower_expr(value), self.lower_expr(expected), self.lower_expr(tolerance)];
                let dst = self.fresh();
                self.emit(IrInstr::Opaque { dst: Some(dst), op: "approximately".to_string(), inputs, ty: expression.ty.clone() });
                dst
            }
            HirKind::As(value, target) => {
                let value = self.lower_expr(value);
                let dst = self.fresh();
                self.emit(IrInstr::Opaque { dst: Some(dst), op: format!("as<{target}>"), inputs: vec![value], ty: expression.ty.clone() });
                dst
            }
            HirKind::Record { name, fields, .. } => {
                let fields = fields.iter().map(|(_, value)| self.lower_expr(value)).collect();
                let dst = self.fresh();
                self.emit(IrInstr::Aggregate { dst, kind: format!("record<{name}>"), fields, ty: expression.ty.clone() });
                dst
            }
            HirKind::Match(subject, arms) => self.lower_match(subject, arms, &expression.ty),
            HirKind::Spawn(_) | HirKind::SpawnScope(_) | HirKind::Channel(_, _) => {
                let dst = self.fresh();
                self.emit(IrInstr::Opaque { dst: Some(dst), op: "concurrency".to_string(), inputs: Vec::new(), ty: expression.ty.clone() });
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
        self.terminate(IrTerminator::Branch { condition: check, then_block: normal_block, else_block: catch_block });

        self.current = normal_block;
        let normal = self.fresh();
        self.emit(IrInstr::TryValue { dst: normal, value, ty: ty.clone() });
        self.terminate(IrTerminator::Goto(merge_block));

        self.current = catch_block;
        let caught = if let Some(handler) = handler {
            self.lower_expr(handler)
        } else {
            let error = self.fresh();
            self.emit(IrInstr::TryError { dst: error, value, ty: ty.clone() });
            error
        };
        if !self.terminated() {
            self.terminate(IrTerminator::Goto(merge_block));
        }

        self.current = merge_block;
        let dst = self.fresh();
        self.emit(IrInstr::Phi { dst, incoming: vec![(normal_block, normal), (catch_block, caught)], ty: ty.clone() });
        dst
    }
}

pub fn lower(program: &HirProgram) -> IrProgram {
    IrProgram { functions: program.functions.iter().map(lower_function).collect() }
}

fn lower_function(function: &HirFunction) -> IrFunction {
    let mut builder = Builder::new(function);
    for (index, (name, ty)) in function.params.iter().enumerate() {
        let value = builder.fresh();
        builder.emit(IrInstr::Param { dst: value, index, name: name.clone(), ty: ty.clone() });
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
        for block in &function.blocks {
            report.instructions += block.instructions.len();
            if block.terminator.is_none() {
                report.unterminated += 1;
                report.violations.push(format!("{}: bb{} has no terminator", function.name, block.id));
            }
            for instruction in &block.instructions {
                if matches!(instruction, IrInstr::Opaque { .. }) {
                    report.opaque += 1;
                }
            }
            if let Some(terminator) = &block.terminator {
                let targets = match terminator {
                    IrTerminator::Goto(target) => vec![*target],
                    IrTerminator::Branch { then_block, else_block, .. } => vec![*then_block, *else_block],
                    IrTerminator::Return(_) | IrTerminator::Unreachable => Vec::new(),
                };
                for target in targets {
                    if target >= function.blocks.len() {
                        report.violations.push(format!("{}: bb{} targets missing bb{}", function.name, block.id, target));
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
        let params = function.params.iter().map(|(name, ty)| format!("{name}: {}", ty.describe())).collect::<Vec<_>>().join(", ");
        let _ = writeln!(out, "ir fn {}({params}) -> {}", function.name, function.ret.describe());
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
        IrInstr::Param { dst, index, name, ty } => format!("%{dst} = param {index} {name}: {}", ty.describe()),
        IrInstr::Const { dst, value, ty } => format!("%{dst} = const {value}: {}", ty.describe()),
        IrInstr::Global { dst, name, ty } => format!("%{dst} = global @{name}: {}", ty.describe()),
        IrInstr::Move { dst, source, .. } => format!("%{dst} = move %{source}"),
        IrInstr::StoreLocal { name, value } => format!("store {name} <- %{value}"),
        IrInstr::Unary { dst, op, operand, .. } => format!("%{dst} = {op:?} %{operand}"),
        IrInstr::Binary { dst, op, left, right, .. } => format!("%{dst} = %{left} {op:?} %{right}"),
        IrInstr::Call { dst, callee, args, .. } => format!("{}call {callee}({})", result_prefix(*dst), value_list(args)),
        IrInstr::MethodCall { dst, method, receiver, args, .. } => format!("{}method %{receiver}.{method}({})", result_prefix(*dst), value_list(args)),
        IrInstr::Field { dst, object, field, .. } => format!("%{dst} = field %{object}.{field}"),
        IrInstr::Index { dst, object, index, .. } => format!("%{dst} = index %{object}[%{index}]"),
        IrInstr::Aggregate { dst, kind, fields, .. } => format!("%{dst} = {kind}({})", value_list(fields)),
        IrInstr::IterInit { dst, source, .. } => format!("%{dst} = iter_init %{source}"),
        IrInstr::IterHasNext { dst, iter } => format!("%{dst} = iter_has_next %{iter}"),
        IrInstr::IterNext { dst, iter, .. } => format!("%{dst} = iter_next %{iter}"),
        IrInstr::PatternTest { dst, subject, pattern } => format!("%{dst} = pattern_test %{subject} {pattern}"),
        IrInstr::PatternBind { dst, subject, name, path, .. } => format!("%{dst} = pattern_bind %{subject} {name} path={}", path.join(".")),
        IrInstr::TryCheck { dst, value } => format!("%{dst} = try_check %{value}"),
        IrInstr::TryValue { dst, value, .. } => format!("%{dst} = try_value %{value}"),
        IrInstr::TryError { dst, value, .. } => format!("%{dst} = try_error %{value}"),
        IrInstr::Phi { dst, incoming, .. } => format!("%{dst} = phi {}", incoming.iter().map(|(block, value)| format!("[bb{block}, %{value}]")).collect::<Vec<_>>().join(" ")),
        IrInstr::Opaque { dst, op, inputs, .. } => format!("{}opaque {op}({})", result_prefix(*dst), value_list(inputs)),
        IrInstr::Retain { value } => format!("retain %{value}"),
        IrInstr::Release { value } => format!("release %{value}"),
    }
}

fn result_prefix(dst: Option<ValueId>) -> String {
    dst.map_or_else(String::new, |dst| format!("%{dst} = "))
}

fn value_list(values: &[ValueId]) -> String {
    values.iter().map(|value| format!("%{value}")).collect::<Vec<_>>().join(", ")
}

fn display_terminator(terminator: &IrTerminator) -> String {
    match terminator {
        IrTerminator::Goto(target) => format!("goto bb{target}"),
        IrTerminator::Branch { condition, then_block, else_block } => format!("br %{condition} -> bb{then_block}, bb{else_block}"),
        IrTerminator::Return(value) => value.map_or_else(|| "ret".to_string(), |value| format!("ret %{value}")),
        IrTerminator::Unreachable => "unreachable".to_string(),
    }
}
