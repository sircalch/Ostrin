//! HIR: a typed tree, one per function, built from the AST plus what the type
//! checker learned (`TypedProgram`). Every node carries its type. It is the
//! first stage of the migration described in `docs/design/20-hir-y-ir.md`:
//! backends will consume this instead of re-inferring types from the AST.
//!
//! This module currently *builds*, *verifies* and *prints* the HIR
//! (`ostrinc --hir`); the native backend is being moved onto it in steps.
//! Fields no consumer reads yet are part of the contract, hence `dead_code` is allowed.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use crate::ast::*;
use crate::typeck::{CallSubst, ExprKey, TypedProgram};
use crate::types::{dim_mul, dim_pow, Dimension, Ty};

#[derive(Debug, Clone)]
pub struct HirProgram {
    pub functions: Vec<HirFunction>,
    /// Parameter count of every user function and every enum variant constructor: after
    /// lowering, a call to one of these carries exactly this many positional arguments.
    pub arities: std::collections::HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct HirFunction {
    /// `name`, or `Type.name` for a method (`Trait.name` for a trait's default body).
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<(String, Ty)>,
    pub ret: Ty,
    pub body: HirBlock,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HirBlock {
    pub stmts: Vec<HirStmt>,
    pub tail: Option<Box<HirExpr>>,
}

#[derive(Debug, Clone)]
pub enum HirStmt {
    Let { name: String, mutable: bool, declared: Option<Type>, value: HirExpr },
    Assign { name: String, value: HirExpr },
    FieldAssign { target: HirExpr, value: HirExpr },
    Return(Option<HirExpr>),
    Break(Option<HirExpr>),
    Continue,
    While { cond: HirExpr, body: HirBlock },
    For { var: String, iter: HirExpr, body: HirBlock },
    Expr(HirExpr),
}

#[derive(Debug, Clone)]
pub struct HirExpr {
    pub ty: Ty,
    pub kind: HirKind,
}

#[derive(Debug, Clone)]
pub struct HirArg {
    pub name: Option<String>,
    pub value: HirExpr,
}

#[derive(Debug, Clone)]
pub struct HirArm {
    pub pattern: Pattern,
    pub guard: Option<HirExpr>,
    pub body: HirBlock,
}

#[derive(Debug, Clone)]
pub enum HirKind {
    Int(i64),
    Sized(i128, IntKind),
    Float(f64),
    Float32(f32),
    Str(String),
    Char(char),
    Bool(bool),
    Unit(Box<HirExpr>, String),
    Local(String),
    Global(String),
    Unary(UnaryOp, Box<HirExpr>),
    Binary(BinOp, Box<HirExpr>, Box<HirExpr>),
    Range(Box<HirExpr>, RangeKind, Box<HirExpr>, Option<Box<HirExpr>>),
    Call { callee: Box<HirExpr>, args: Vec<HirArg>, type_args: Vec<Type>, subst: Option<CallSubst> },
    /// `recv.method(args)`: resolved to a concrete impl/vtable slot by later stages.
    MethodCall { recv: Box<HirExpr>, method: String, args: Vec<HirArg>, type_args: Vec<Type>, subst: Option<CallSubst> },
    Field(Box<HirExpr>, String),
    Index(Box<HirExpr>, Box<HirExpr>),
    If(Box<HirExpr>, HirBlock, Option<HirBlock>),
    Block(HirBlock),
    Lambda(Vec<String>, HirBlock),
    List(Vec<HirExpr>),
    Set(Vec<HirExpr>),
    Map(Vec<(HirExpr, HirExpr)>),
    EmptyCollection(String, Vec<Type>),
    Try(Box<HirExpr>, Option<Box<HirExpr>>),
    Within(Box<HirExpr>, Box<HirExpr>),
    Approximately(Box<HirExpr>, Box<HirExpr>, Box<HirExpr>),
    As(Box<HirExpr>, String),
    Loop(HirBlock),
    Record { name: String, type_args: Vec<Type>, fields: Vec<(String, HirExpr)> },
    Match(Box<HirExpr>, Vec<HirArm>),
    Spawn(HirBlock),
    SpawnScope(HirBlock),
    Channel(Type, Option<Box<HirExpr>>),
}

/// The declarations calls are resolved against (to normalize named/default arguments).
struct Signatures<'a> {
    functions: std::collections::HashMap<&'a str, &'a FunctionDecl>,
    methods: std::collections::HashMap<(&'a str, &'a str), &'a FunctionDecl>,
    /// Variant name -> its field names, in declaration order.
    variants: std::collections::HashMap<&'a str, Vec<Option<String>>>,
}

struct Lowerer<'a> {
    typed: &'a TypedProgram,
    signatures: &'a Signatures<'a>,
    file: Option<String>,
    /// Innermost-last stack of local names, to tell `Local` from `Global`.
    scopes: Vec<HashSet<String>>,
    /// Known types of locals (parameters, bindings), used when the checker recorded none for a use.
    local_types: std::collections::HashMap<String, Ty>,
}

impl<'a> Lowerer<'a> {
    fn ty_of(&self, expr: &Expr) -> Ty {
        self.typed.node_types.get(&(expr as *const Expr as usize)).cloned().unwrap_or(Ty::Unknown)
    }

    fn declare(&mut self, name: &str) {
        self.scopes.last_mut().expect("a scope is always open").insert(name.to_string());
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }

    fn block(&mut self, block: &Block) -> HirBlock {
        self.scopes.push(HashSet::new());
        let stmts = block.stmts.iter().map(|s| self.stmt(&s.stmt)).collect();
        let tail = block.tail.as_ref().map(|e| Box::new(self.expr(e)));
        self.scopes.pop();
        HirBlock { stmts, tail }
    }

    fn stmt(&mut self, stmt: &Stmt) -> HirStmt {
        match stmt {
            Stmt::Binding { mut_, name, ty, value } => {
                let value = self.expr(value);
                self.declare(name);
                let bound = match ty {
                    Some(declared) => crate::typeck::resolve_type(declared),
                    None => value.ty.clone(),
                };
                self.local_types.insert(name.clone(), bound);
                HirStmt::Let { name: name.clone(), mutable: *mut_, declared: ty.clone(), value }
            }
            Stmt::Assign { name, value } => {
                let value = self.expr(value);
                // `x = v` without a prior binding introduces one (there is no `let`).
                if !self.is_local(name) {
                    self.declare(name);
                    self.local_types.insert(name.clone(), value.ty.clone());
                }
                HirStmt::Assign { name: name.clone(), value }
            }
            Stmt::FieldAssign { target, value } => HirStmt::FieldAssign { target: self.expr(target), value: self.expr(value) },
            Stmt::Return(value) => HirStmt::Return(value.as_ref().map(|e| self.expr(e))),
            Stmt::Break(value) => HirStmt::Break(value.as_ref().map(|e| self.expr(e))),
            Stmt::Continue => HirStmt::Continue,
            Stmt::While { cond, body } => HirStmt::While { cond: self.expr(cond), body: self.block(body) },
            Stmt::For { pattern, iter, body } => {
                let iter = self.expr(iter);
                self.scopes.push(HashSet::new());
                self.declare(pattern);
                let body = self.block(body);
                self.scopes.pop();
                HirStmt::For { var: pattern.clone(), iter, body }
            }
            Stmt::Expr(e) => HirStmt::Expr(self.expr(e)),
        }
    }

    fn args(&mut self, args: &[Arg]) -> Vec<HirArg> {
        args.iter()
            .map(|a| match a {
                Arg::Positional(e) => HirArg { name: None, value: self.expr(e) },
                Arg::Named(n, e) => HirArg { name: Some(n.clone()), value: self.expr(e) },
            })
            .collect()
    }

    fn bind_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Ident(name) => self.declare(name),
            Pattern::Variant(_, fields) => {
                for (_, sub) in fields {
                    self.bind_pattern(sub);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) | Pattern::Range(..) => {}
        }
    }

    fn expr(&mut self, expr: &Expr) -> HirExpr {
        self.expr_keyed(expr, None)
    }

    /// `key` is the source key of the nearest enclosing `Located`, used to find
    /// a call's resolved generic arguments.
    fn expr_keyed(&mut self, expr: &Expr, key: Option<ExprKey>) -> HirExpr {
        let ty = self.ty_of(expr);
        let kind = match expr {
            Expr::Located(inner, range) => {
                let key = ExprKey { file: self.file.clone(), start: range.start, end: range.end };
                let mut lowered = self.expr_keyed(inner, Some(key));
                // The wrapper's own recorded type wins when the inner node has none.
                if lowered.ty == Ty::Unknown {
                    lowered.ty = ty;
                }
                return lowered;
            }
            Expr::IntLiteral(n) => HirKind::Int(*n),
            Expr::SizedIntLiteral(n, k) => HirKind::Sized(*n, *k),
            Expr::FloatLiteral(f) => HirKind::Float(*f),
            Expr::Float32Literal(f) => HirKind::Float32(*f),
            Expr::StringLiteral(s) => HirKind::Str(s.clone()),
            Expr::CharLiteral(c) => HirKind::Char(*c),
            Expr::BoolLiteral(b) => HirKind::Bool(*b),
            Expr::UnitLiteral(n, u) => HirKind::Unit(Box::new(self.expr(n)), u.clone()),
            Expr::Ident(name) => {
                if self.is_local(name) {
                    // The checker sometimes records nothing for a use (`self`); fall back to the binding's type.
                    if ty == Ty::Unknown {
                        if let Some(known) = self.local_types.get(name) {
                            return HirExpr { ty: known.clone(), kind: HirKind::Local(name.clone()) };
                        }
                    }
                    HirKind::Local(name.clone())
                } else {
                    HirKind::Global(name.clone())
                }
            }
            Expr::Unary(op, e) => HirKind::Unary(*op, Box::new(self.expr(e))),
            Expr::Binary(op, l, r) => HirKind::Binary(*op, Box::new(self.expr(l)), Box::new(self.expr(r))),
            Expr::Range(a, kind, b, step) => {
                HirKind::Range(Box::new(self.expr(a)), *kind, Box::new(self.expr(b)), step.as_ref().map(|s| Box::new(self.expr(s))))
            }
            Expr::Call(callee, args) => self.call(callee, &[], args, key, &ty, expr as *const Expr as usize),
            Expr::GenericCall(callee, type_args, args) => self.call(callee, type_args, args, key, &ty, expr as *const Expr as usize),
            Expr::FieldAccess(obj, field) => HirKind::Field(Box::new(self.expr(obj)), field.clone()),
            Expr::Index(obj, idx) => HirKind::Index(Box::new(self.expr(obj)), Box::new(self.expr(idx))),
            Expr::If(cond, then_b, else_b) => {
                HirKind::If(Box::new(self.expr(cond)), self.block(then_b), else_b.as_ref().map(|b| self.block(b)))
            }
            Expr::Block(b) => HirKind::Block(self.block(b)),
            Expr::Lambda(params, body) => {
                self.scopes.push(params.iter().cloned().collect());
                let body = self.block(body);
                self.scopes.pop();
                HirKind::Lambda(params.clone(), body)
            }
            Expr::ListLiteral(items) => HirKind::List(items.iter().map(|e| self.expr(e)).collect()),
            Expr::SetLiteral(items) => HirKind::Set(items.iter().map(|e| self.expr(e)).collect()),
            Expr::MapLiteral(pairs) => HirKind::Map(pairs.iter().map(|(k, v)| (self.expr(k), self.expr(v))).collect()),
            Expr::EmptyCollection(name, types) => HirKind::EmptyCollection(name.clone(), types.clone()),
            Expr::Try(e, handler) => HirKind::Try(Box::new(self.expr(e)), handler.as_ref().map(|h| Box::new(self.expr(h)))),
            Expr::Within(a, b) => HirKind::Within(Box::new(self.expr(a)), Box::new(self.expr(b))),
            Expr::Approximately(a, b, t) => HirKind::Approximately(Box::new(self.expr(a)), Box::new(self.expr(b)), Box::new(self.expr(t))),
            Expr::As(e, target) => {
                let name = match target.unlocated() {
                    Expr::Ident(n) => n.clone(),
                    _ => "?".to_string(),
                };
                HirKind::As(Box::new(self.expr(e)), name)
            }
            Expr::Loop(b) => HirKind::Loop(self.block(b)),
            Expr::RecordLiteral(name, fields) => HirKind::Record {
                name: name.clone(),
                type_args: Vec::new(),
                fields: fields.iter().map(|(n, e)| (n.clone(), self.expr(e))).collect(),
            },
            Expr::GenericRecordLiteral(name, type_args, fields) => HirKind::Record {
                name: name.clone(),
                type_args: type_args.clone(),
                fields: fields.iter().map(|(n, e)| (n.clone(), self.expr(e))).collect(),
            },
            Expr::Match(scrutinee, arms) => {
                let scrutinee = Box::new(self.expr(scrutinee));
                let arms = arms
                    .iter()
                    .map(|arm| {
                        self.scopes.push(HashSet::new());
                        self.bind_pattern(&arm.pattern);
                        let guard = arm.guard.as_ref().map(|g| self.expr(g));
                        let body = self.block(&arm.body);
                        self.scopes.pop();
                        HirArm { pattern: arm.pattern.clone(), guard, body }
                    })
                    .collect();
                HirKind::Match(scrutinee, arms)
            }
            Expr::Spawn(b) => HirKind::Spawn(self.block(b)),
            Expr::SpawnScope(b) => HirKind::SpawnScope(self.block(b)),
            Expr::Channel(ty, cap) => HirKind::Channel(ty.clone(), cap.as_ref().map(|c| Box::new(self.expr(c)))),
        };
        HirExpr { ty, kind }
    }

    /// Reorders `args` into `params` order, filling omitted ones from their defaults (lowered here,
    /// at the call site, like the interpreter evaluates them). Leaves the call untouched when an
    /// argument can't be matched, so the verifier reports it.
    fn normalize(&mut self, params: &[Param], args: Vec<HirArg>) -> Vec<HirArg> {
        let original = args.clone();
        let named: Vec<(Option<String>, HirExpr)> = args.into_iter().map(|a| (a.name, a.value)).collect();
        match arrange_arguments(params, named, |d| self.expr(d)) {
            Ok(list) => list.into_iter().map(|value| HirArg { name: None, value }).collect(),
            Err(_) => original,
        }
    }

    fn call(&mut self, callee: &Expr, type_args: &[Type], args: &[Arg], key: Option<ExprKey>, result: &Ty, node: usize) -> HirKind {
        let subst = self.typed.call_substs_by_node.get(&node).cloned().or_else(|| key.and_then(|k| self.typed.call_substs.get(&k).cloned()));
        let type_args = type_args.to_vec();
        // `recv.method(args)` is a method call, not a call of a field value.
        if let Expr::FieldAccess(recv, method) = callee.unlocated() {
            let recv = self.expr(recv);
            let mut args = self.args(args);
            let owner = match &recv.ty {
                Ty::Named(n) | Ty::Applied(n, _) => Some(n.clone()),
                _ => None,
            };
            if let Some(decl) = owner.and_then(|n| self.signatures.methods.get(&(n.as_str(), method.as_str())).copied()) {
                let without_self = if decl.params.first().is_some_and(|p| p.name == "self") { &decl.params[1..] } else { &decl.params[..] };
                args = self.normalize(without_self, args);
            }
            return HirKind::MethodCall { recv: Box::new(recv), method: method.clone(), args, type_args, subst };
        }
        let mut args = self.args(args);
        // Named and defaulted arguments become plain positional ones, in parameter order.
        if let Expr::Ident(name) = callee.unlocated() {
            if let Some(decl) = self.signatures.functions.get(name.as_str()).copied() {
                args = self.normalize(&decl.params, args);
            } else if let Some(fields) = self.signatures.variants.get(name.as_str()) {
                args = normalize_variant(fields, args);
            }
        }
        let mut callee = self.expr(callee);
        // The callee itself is rarely typed by the checker (builtins, constructors);
        // its type follows from the arguments and the call's result.
        if callee.ty == Ty::Unknown {
            callee.ty = Ty::Fn(args.iter().map(|a| a.value.ty.clone()).collect(), Box::new(result.clone()));
        }
        HirKind::Call { callee: Box::new(callee), args, type_args, subst }
    }
}

/// Orders a variant constructor's arguments by field name.
fn normalize_variant(fields: &[Option<String>], args: Vec<HirArg>) -> Vec<HirArg> {
    if args.iter().all(|a| a.name.is_none()) {
        return args;
    }
    let original = args.clone();
    let mut slots: Vec<Option<HirExpr>> = (0..fields.len()).map(|_| None).collect();
    let mut next = 0usize;
    for arg in args {
        match arg.name {
            None => {
                if next >= slots.len() {
                    return original;
                }
                slots[next] = Some(arg.value);
                next += 1;
            }
            Some(name) => match fields.iter().position(|f| f.as_deref() == Some(name.as_str())) {
                Some(index) => slots[index] = Some(arg.value),
                None => return original,
            },
        }
    }
    if slots.iter().any(Option::is_none) {
        return original;
    }
    slots.into_iter().map(|s| HirArg { name: None, value: s.expect("checked above") }).collect()
}

/// Builds the HIR of every function, method and trait default body.
pub fn lower<'a>(items: &'a [Item], typed: &'a TypedProgram) -> HirProgram {
    let mut signatures = Signatures { functions: Default::default(), methods: Default::default(), variants: Default::default() };
    let mut arities = std::collections::HashMap::new();
    for item in items {
        match item {
            Item::Function(f) => {
                signatures.functions.insert(f.name.as_str(), f);
                arities.insert(f.name.clone(), f.params.len());
            }
            Item::Impl(im) => {
                for m in &im.methods {
                    signatures.methods.insert((im.type_name.as_str(), m.name.as_str()), m);
                    let skip = usize::from(m.params.first().is_some_and(|p| p.name == "self"));
                    arities.insert(format!("{}.{}", im.type_name, m.name), m.params.len() - skip);
                }
            }
            Item::Enum(e) => {
                for v in &e.variants {
                    signatures.variants.insert(v.name.as_str(), v.fields.iter().map(|f| f.name.clone()).collect());
                    arities.insert(v.name.clone(), v.fields.len());
                }
            }
            _ => {}
        }
    }
    let signatures = &signatures;
    let mut functions = Vec::new();
    let lower_fn = |name: String, f: &FunctionDecl, extra_generics: &[GenericParam], self_ty: Ty| {
        let mut lowerer = Lowerer { typed, signatures, file: f.source_file.clone(), scopes: vec![HashSet::new()], local_types: Default::default() };
        for p in &f.params {
            lowerer.declare(&p.name);
            let ty = if p.name == "self" { self_ty.clone() } else { crate::typeck::resolve_type(&p.ty) };
            lowerer.local_types.insert(p.name.clone(), ty);
        }
        let body = lowerer.block(&f.body);
        HirFunction {
            name,
            generics: extra_generics.iter().chain(&f.generics).map(|g| g.name.clone()).collect(),
            params: f.params.iter().map(|p| (p.name.clone(), if p.name == "self" { self_ty.clone() } else { crate::typeck::resolve_type(&p.ty) })).collect(),
            ret: crate::typeck::resolve_type(&f.return_type),
            body,
            source_file: f.source_file.clone(),
        }
    };
    for item in items {
        match item {
            Item::Function(f) => functions.push(lower_fn(f.name.clone(), f, &[], Ty::Unknown)),
            Item::Impl(im) => {
                let self_ty = if im.type_args.is_empty() {
                    Ty::Named(im.type_name.clone())
                } else {
                    Ty::Applied(im.type_name.clone(), im.type_args.iter().map(crate::typeck::resolve_type).collect())
                };
                for m in &im.methods {
                    functions.push(lower_fn(format!("{}.{}", im.type_name, m.name), m, &im.generics, self_ty.clone()));
                }
            }
            _ => {}
        }
    }
    HirProgram { functions, arities }
}

/// Creates the concrete HIR body of one monomorphized generic function.
///
/// Generic functions are lowered once, with `Ty::Generic` nodes, because the
/// checker validates their body independently of every call site. The native
/// backend, however, emits one C function per concrete instantiation. This
/// pass specializes the already-validated HIR just before that C body is
/// emitted; it deliberately does not re-check the AST or infer anything.
pub fn specialize_function(function: &HirFunction, subst: &HashMap<String, Ty>) -> HirFunction {
    HirFunction {
        name: function.name.clone(),
        generics: Vec::new(),
        params: function
            .params
            .iter()
            .map(|(name, ty)| (name.clone(), specialize_ty(ty, subst)))
            .collect(),
        ret: specialize_ty(&function.ret, subst),
        body: specialize_block(&function.body, subst),
        source_file: function.source_file.clone(),
    }
}

fn specialize_dimension(dimension: &Dimension, subst: &HashMap<String, Ty>) -> Dimension {
    let mut result = Dimension::new();
    for (name, exponent) in dimension {
        if let Some(Ty::Quantity(bound)) = subst.get(name) {
            result = dim_mul(&result, &dim_pow(bound, *exponent));
        } else {
            result = dim_mul(&result, &HashMap::from([(name.clone(), *exponent)]));
        }
    }
    result
}

fn specialize_ty(ty: &Ty, subst: &HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::Generic(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        // Function and method signatures are lowered from their source
        // `Type`, so a generic parameter there arrives as `Ty::Named("T")`
        // while expression nodes normally carry `Ty::Generic("T")`.
        Ty::Named(name) if subst.contains_key(name) => subst[name].clone(),
        Ty::Quantity(dimension) => Ty::Quantity(specialize_dimension(dimension, subst)),
        Ty::List(inner) => Ty::List(Box::new(specialize_ty(inner, subst))),
        Ty::Map(key, value) => Ty::Map(Box::new(specialize_ty(key, subst)), Box::new(specialize_ty(value, subst))),
        Ty::Set(inner) => Ty::Set(Box::new(specialize_ty(inner, subst))),
        Ty::Applied(name, args) => Ty::Applied(name.clone(), args.iter().map(|arg| specialize_ty(arg, subst)).collect()),
        Ty::Fn(params, ret) => Ty::Fn(
            params.iter().map(|param| specialize_ty(param, subst)).collect(),
            Box::new(specialize_ty(ret, subst)),
        ),
        _ => ty.clone(),
    }
}

fn specialize_subst(call: &Option<CallSubst>, subst: &HashMap<String, Ty>) -> Option<CallSubst> {
    call.as_ref().map(|call| CallSubst {
        types: call.types.iter().map(|(name, ty)| (name.clone(), specialize_ty(ty, subst))).collect(),
        dims: call.dims.iter().map(|(name, dimension)| (name.clone(), specialize_dimension(dimension, subst))).collect(),
    })
}

fn specialize_block(block: &HirBlock, subst: &HashMap<String, Ty>) -> HirBlock {
    HirBlock {
        stmts: block.stmts.iter().map(|stmt| specialize_stmt(stmt, subst)).collect(),
        tail: block.tail.as_ref().map(|expr| Box::new(specialize_expr(expr, subst))),
    }
}

fn specialize_stmt(stmt: &HirStmt, subst: &HashMap<String, Ty>) -> HirStmt {
    match stmt {
        HirStmt::Let { name, mutable, value, .. } => HirStmt::Let {
            name: name.clone(),
            mutable: *mutable,
            // The HIR expression type is authoritative after specialization;
            // retaining the source Type<T> annotation would make the emitter
            // compare it against an already-concrete Ty and reject valid code.
            declared: None,
            value: specialize_expr(value, subst),
        },
        HirStmt::Assign { name, value } => HirStmt::Assign { name: name.clone(), value: specialize_expr(value, subst) },
        HirStmt::FieldAssign { target, value } => HirStmt::FieldAssign {
            target: specialize_expr(target, subst),
            value: specialize_expr(value, subst),
        },
        HirStmt::Return(value) => HirStmt::Return(value.as_ref().map(|value| specialize_expr(value, subst))),
        HirStmt::Break(value) => HirStmt::Break(value.as_ref().map(|value| specialize_expr(value, subst))),
        HirStmt::Continue => HirStmt::Continue,
        HirStmt::While { cond, body } => HirStmt::While {
            cond: specialize_expr(cond, subst),
            body: specialize_block(body, subst),
        },
        HirStmt::For { var, iter, body } => HirStmt::For {
            var: var.clone(),
            iter: specialize_expr(iter, subst),
            body: specialize_block(body, subst),
        },
        HirStmt::Expr(expr) => HirStmt::Expr(specialize_expr(expr, subst)),
    }
}

fn specialize_arg(arg: &HirArg, subst: &HashMap<String, Ty>) -> HirArg {
    HirArg { name: arg.name.clone(), value: specialize_expr(&arg.value, subst) }
}

fn specialize_arm(arm: &HirArm, subst: &HashMap<String, Ty>) -> HirArm {
    HirArm {
        pattern: arm.pattern.clone(),
        guard: arm.guard.as_ref().map(|guard| specialize_expr(guard, subst)),
        body: specialize_block(&arm.body, subst),
    }
}

fn specialize_expr(expr: &HirExpr, subst: &HashMap<String, Ty>) -> HirExpr {
    let kind = match &expr.kind {
        HirKind::Int(value) => HirKind::Int(*value),
        HirKind::Sized(value, kind) => HirKind::Sized(*value, *kind),
        HirKind::Float(value) => HirKind::Float(*value),
        HirKind::Float32(value) => HirKind::Float32(*value),
        HirKind::Str(value) => HirKind::Str(value.clone()),
        HirKind::Char(value) => HirKind::Char(*value),
        HirKind::Bool(value) => HirKind::Bool(*value),
        HirKind::Unit(value, unit) => HirKind::Unit(Box::new(specialize_expr(value, subst)), unit.clone()),
        HirKind::Local(name) => HirKind::Local(name.clone()),
        HirKind::Global(name) => HirKind::Global(name.clone()),
        HirKind::Unary(op, value) => HirKind::Unary(*op, Box::new(specialize_expr(value, subst))),
        HirKind::Binary(op, left, right) => HirKind::Binary(
            *op,
            Box::new(specialize_expr(left, subst)),
            Box::new(specialize_expr(right, subst)),
        ),
        HirKind::Range(start, kind, end, step) => HirKind::Range(
            Box::new(specialize_expr(start, subst)),
            *kind,
            Box::new(specialize_expr(end, subst)),
            step.as_ref().map(|step| Box::new(specialize_expr(step, subst))),
        ),
        HirKind::Call { callee, args, type_args, subst: call } => HirKind::Call {
            callee: Box::new(specialize_expr(callee, subst)),
            args: args.iter().map(|arg| specialize_arg(arg, subst)).collect(),
            type_args: type_args.clone(),
            subst: specialize_subst(call, subst),
        },
        HirKind::MethodCall { recv, method, args, type_args, subst: call } => HirKind::MethodCall {
            recv: Box::new(specialize_expr(recv, subst)),
            method: method.clone(),
            args: args.iter().map(|arg| specialize_arg(arg, subst)).collect(),
            type_args: type_args.clone(),
            subst: specialize_subst(call, subst),
        },
        HirKind::Field(value, field) => HirKind::Field(Box::new(specialize_expr(value, subst)), field.clone()),
        HirKind::Index(value, index) => HirKind::Index(
            Box::new(specialize_expr(value, subst)),
            Box::new(specialize_expr(index, subst)),
        ),
        HirKind::If(cond, then_block, else_block) => HirKind::If(
            Box::new(specialize_expr(cond, subst)),
            specialize_block(then_block, subst),
            else_block.as_ref().map(|block| specialize_block(block, subst)),
        ),
        HirKind::Block(block) => HirKind::Block(specialize_block(block, subst)),
        HirKind::Lambda(params, block) => HirKind::Lambda(params.clone(), specialize_block(block, subst)),
        HirKind::List(values) => HirKind::List(values.iter().map(|value| specialize_expr(value, subst)).collect()),
        HirKind::Set(values) => HirKind::Set(values.iter().map(|value| specialize_expr(value, subst)).collect()),
        HirKind::Map(values) => HirKind::Map(
            values
                .iter()
                .map(|(key, value)| (specialize_expr(key, subst), specialize_expr(value, subst)))
                .collect(),
        ),
        HirKind::EmptyCollection(name, type_args) => HirKind::EmptyCollection(name.clone(), type_args.clone()),
        HirKind::Try(value, handler) => HirKind::Try(
            Box::new(specialize_expr(value, subst)),
            handler.as_ref().map(|handler| Box::new(specialize_expr(handler, subst))),
        ),
        HirKind::Within(left, right) => HirKind::Within(
            Box::new(specialize_expr(left, subst)),
            Box::new(specialize_expr(right, subst)),
        ),
        HirKind::Approximately(value, target, tolerance) => HirKind::Approximately(
            Box::new(specialize_expr(value, subst)),
            Box::new(specialize_expr(target, subst)),
            Box::new(specialize_expr(tolerance, subst)),
        ),
        HirKind::As(value, target) => HirKind::As(Box::new(specialize_expr(value, subst)), target.clone()),
        HirKind::Loop(block) => HirKind::Loop(specialize_block(block, subst)),
        HirKind::Record { name, type_args, fields } => HirKind::Record {
            name: name.clone(),
            type_args: type_args.clone(),
            fields: fields.iter().map(|(name, value)| (name.clone(), specialize_expr(value, subst))).collect(),
        },
        HirKind::Match(scrutinee, arms) => HirKind::Match(
            Box::new(specialize_expr(scrutinee, subst)),
            arms.iter().map(|arm| specialize_arm(arm, subst)).collect(),
        ),
        HirKind::Spawn(block) => HirKind::Spawn(specialize_block(block, subst)),
        HirKind::SpawnScope(block) => HirKind::SpawnScope(specialize_block(block, subst)),
        HirKind::Channel(ty, capacity) => HirKind::Channel(
            ty.clone(),
            capacity.as_ref().map(|capacity| Box::new(specialize_expr(capacity, subst))),
        ),
    };
    HirExpr { ty: specialize_ty(&expr.ty, subst), kind }
}

/// A HIR invariant that does not hold.
#[derive(Debug)]
pub struct Violation {
    pub function: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct VerifyReport {
    pub nodes: usize,
    pub unknown: usize,
    pub violations: Vec<Violation>,
}

/// Checks the invariants: every node has a known type, and every call of a
/// generic function carries its resolved type arguments.
pub fn verify(program: &HirProgram, generic_functions: &HashSet<String>) -> VerifyReport {
    let mut report = VerifyReport::default();
    for f in &program.functions {
        let mut ctx = (f.name.clone(), &mut report, &program.arities);
        verify_block(&f.body, generic_functions, &mut ctx);
    }
    report
}

type Ctx<'r> = (String, &'r mut VerifyReport, &'r std::collections::HashMap<String, usize>);

fn verify_block(block: &HirBlock, generics: &HashSet<String>, ctx: &mut Ctx) {
    for s in &block.stmts {
        match s {
            HirStmt::Let { value, .. } | HirStmt::Assign { value, .. } => verify_expr(value, generics, ctx),
            HirStmt::FieldAssign { target, value } => {
                verify_expr(target, generics, ctx);
                verify_expr(value, generics, ctx);
            }
            HirStmt::Return(v) | HirStmt::Break(v) => {
                if let Some(v) = v {
                    verify_expr(v, generics, ctx);
                }
            }
            HirStmt::Continue => {}
            HirStmt::While { cond, body } => {
                verify_expr(cond, generics, ctx);
                verify_block(body, generics, ctx);
            }
            HirStmt::For { iter, body, .. } => {
                verify_expr(iter, generics, ctx);
                verify_block(body, generics, ctx);
            }
            HirStmt::Expr(e) => verify_expr(e, generics, ctx),
        }
    }
    if let Some(t) = &block.tail {
        verify_expr(t, generics, ctx);
    }
}

fn verify_expr(e: &HirExpr, generics: &HashSet<String>, ctx: &mut Ctx) {
    ctx.1.nodes += 1;
    if crate::types::ty_contains_unknown(&e.ty) {
        ctx.1.unknown += 1;
    }
    let go = |x: &HirExpr, ctx: &mut Ctx| verify_expr(x, generics, ctx);
    match &e.kind {
        HirKind::Unit(a, _) | HirKind::Unary(_, a) | HirKind::Field(a, _) | HirKind::As(a, _) => go(a, ctx),
        HirKind::Binary(_, a, b) | HirKind::Index(a, b) | HirKind::Within(a, b) => {
            go(a, ctx);
            go(b, ctx);
        }
        HirKind::Approximately(a, b, c) => {
            go(a, ctx);
            go(b, ctx);
            go(c, ctx);
        }
        HirKind::Range(a, _, b, step) => {
            go(a, ctx);
            go(b, ctx);
            if let Some(s) = step {
                go(s, ctx);
            }
        }
        HirKind::MethodCall { recv, method, args, .. } => {
            if args.iter().any(|a| a.name.is_some()) {
                let function = ctx.0.clone();
                ctx.1.violations.push(Violation { function, message: format!("method call '.{method}' still has named arguments") });
            }
            go(recv, ctx);
            for a in args {
                go(&a.value, ctx);
            }
        }
        HirKind::Call { callee, args, subst, .. } => {
            if let HirKind::Global(name) = &callee.kind {
                if generics.contains(name) && subst.is_none() {
                    let function = ctx.0.clone();
                    ctx.1.violations.push(Violation { function, message: format!("call to generic function '{name}' has no resolved type arguments") });
                }
            }
            if let HirKind::Global(name) = &callee.kind {
                if let Some(&arity) = ctx.2.get(name) {
                    if args.len() != arity || args.iter().any(|a| a.name.is_some()) {
                        let function = ctx.0.clone();
                        ctx.1.violations.push(Violation { function, message: format!("call to '{name}' has unresolved named/default arguments ({} given, {arity} expected)", args.len()) });
                    }
                }
            }
            go(callee, ctx);
            for a in args {
                go(&a.value, ctx);
            }
        }
        HirKind::If(c, t, f) => {
            go(c, ctx);
            verify_block(t, generics, ctx);
            if let Some(f) = f {
                verify_block(f, generics, ctx);
            }
        }
        HirKind::Block(b) | HirKind::Loop(b) | HirKind::Spawn(b) | HirKind::SpawnScope(b) | HirKind::Lambda(_, b) => verify_block(b, generics, ctx),
        HirKind::List(v) | HirKind::Set(v) => {
            for x in v {
                go(x, ctx);
            }
        }
        HirKind::Map(v) => {
            for (k, x) in v {
                go(k, ctx);
                go(x, ctx);
            }
        }
        HirKind::Try(a, h) => {
            go(a, ctx);
            if let Some(h) = h {
                go(h, ctx);
            }
        }
        HirKind::Record { fields, .. } => {
            for (_, x) in fields {
                go(x, ctx);
            }
        }
        HirKind::Match(s, arms) => {
            go(s, ctx);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    go(g, ctx);
                }
                verify_block(&arm.body, generics, ctx);
            }
        }
        HirKind::Channel(_, cap) => {
            if let Some(c) = cap {
                go(c, ctx);
            }
        }
        HirKind::Int(_) | HirKind::Sized(..) | HirKind::Float(_) | HirKind::Float32(_) | HirKind::Str(_) | HirKind::Char(_) | HirKind::Bool(_) | HirKind::Local(_) | HirKind::Global(_) | HirKind::EmptyCollection(..) => {}
    }
}

/// A compact, indented rendering of the HIR (`ostrinc --hir`).
pub fn dump(program: &HirProgram) -> String {
    let mut out = String::new();
    for f in &program.functions {
        let generics = if f.generics.is_empty() { String::new() } else { format!("<{}>", f.generics.join(", ")) };
        let params: Vec<String> = f.params.iter().map(|(n, t)| format!("{n}: {}", t.describe())).collect();
        let _ = writeln!(out, "fn {}{}({}) -> {}", f.name, generics, params.join(", "), f.ret.describe());
        dump_block(&f.body, 1, &mut out);
        out.push('\n');
    }
    out
}

fn pad(depth: usize) -> String {
    "  ".repeat(depth)
}

fn dump_block(b: &HirBlock, depth: usize, out: &mut String) {
    for s in &b.stmts {
        match s {
            HirStmt::Let { name, mutable, value, .. } => {
                let _ = writeln!(out, "{}{} {name} = {}", pad(depth), if *mutable { "let mut" } else { "let" }, one_line(value));
            }
            HirStmt::Assign { name, value } => {
                let _ = writeln!(out, "{}{name} := {}", pad(depth), one_line(value));
            }
            HirStmt::FieldAssign { target, value } => {
                let _ = writeln!(out, "{}{} := {}", pad(depth), one_line(target), one_line(value));
            }
            HirStmt::Return(v) => {
                let _ = writeln!(out, "{}return {}", pad(depth), v.as_ref().map(one_line).unwrap_or_default());
            }
            HirStmt::Break(_) => {
                let _ = writeln!(out, "{}break", pad(depth));
            }
            HirStmt::Continue => {
                let _ = writeln!(out, "{}continue", pad(depth));
            }
            HirStmt::While { cond, body } => {
                let _ = writeln!(out, "{}while {}", pad(depth), one_line(cond));
                dump_block(body, depth + 1, out);
            }
            HirStmt::For { var, iter, body } => {
                let _ = writeln!(out, "{}for {var} in {}", pad(depth), one_line(iter));
                dump_block(body, depth + 1, out);
            }
            HirStmt::Expr(e) => {
                let _ = writeln!(out, "{}{}", pad(depth), one_line(e));
            }
        }
    }
    if let Some(t) = &b.tail {
        let _ = writeln!(out, "{}=> {}", pad(depth), one_line(t));
    }
}

/// One-line form of an expression; nested blocks are elided as `{…}`.
fn one_line(e: &HirExpr) -> String {
    let inner = match &e.kind {
        HirKind::Int(n) => n.to_string(),
        HirKind::Sized(n, k) => format!("{n}{}", k.name()),
        HirKind::Float(f) => format!("{f:?}"),
        HirKind::Float32(f) => format!("{f:?}f32"),
        HirKind::Str(s) => format!("{s:?}"),
        HirKind::Char(c) => format!("{c:?}"),
        HirKind::Bool(b) => b.to_string(),
        HirKind::Unit(n, u) => format!("{} {u}", one_line(n)),
        HirKind::Local(n) => n.clone(),
        HirKind::Global(n) => format!("@{n}"),
        HirKind::Unary(op, a) => format!("({op:?} {})", one_line(a)),
        HirKind::Binary(op, a, b) => format!("({} {op:?} {})", one_line(a), one_line(b)),
        HirKind::Range(a, k, b, _) => format!("({} {k:?} {})", one_line(a), one_line(b)),
        HirKind::Call { callee, args, subst, .. } => {
            let args: Vec<String> = args.iter().map(|a| one_line(&a.value)).collect();
            let subst = match subst {
                Some(s) if !s.types.is_empty() || !s.dims.is_empty() => {
                    let mut parts: Vec<String> = s.types.iter().map(|(k, v)| format!("{k}={}", v.describe())).collect();
                    parts.sort();
                    format!("<{}>", parts.join(", "))
                }
                _ => String::new(),
            };
            format!("{}{subst}({})", one_line(callee), args.join(", "))
        }
        HirKind::MethodCall { recv, method, args, .. } => {
            format!("{}.{method}({})", one_line(recv), args.iter().map(|a| one_line(&a.value)).collect::<Vec<_>>().join(", "))
        }
        HirKind::Field(a, f) => format!("{}.{f}", one_line(a)),
        HirKind::Index(a, i) => format!("{}[{}]", one_line(a), one_line(i)),
        HirKind::If(c, ..) => format!("if {} {{…}}", one_line(c)),
        HirKind::Block(_) => "{…}".to_string(),
        HirKind::Lambda(p, _) => format!("fn({}) {{…}}", p.join(", ")),
        HirKind::List(v) => format!("[{}]", v.iter().map(one_line).collect::<Vec<_>>().join(", ")),
        HirKind::Set(v) => format!("{{{}}}", v.iter().map(one_line).collect::<Vec<_>>().join(", ")),
        HirKind::Map(v) => format!("[{}]", v.iter().map(|(k, x)| format!("{}: {}", one_line(k), one_line(x))).collect::<Vec<_>>().join(", ")),
        HirKind::EmptyCollection(n, _) => format!("{n}()"),
        HirKind::Try(a, _) => format!("try {}", one_line(a)),
        HirKind::Within(a, b) => format!("({} within {})", one_line(a), one_line(b)),
        HirKind::Approximately(a, b, t) => format!("({} ≈ {} ± {})", one_line(a), one_line(b), one_line(t)),
        HirKind::As(a, t) => format!("({} as {t})", one_line(a)),
        HirKind::Loop(_) => "loop {…}".to_string(),
        HirKind::Record { name, fields, .. } => {
            format!("{name} {{ {} }}", fields.iter().map(|(n, x)| format!("{n}: {}", one_line(x))).collect::<Vec<_>>().join(", "))
        }
        HirKind::Match(s, arms) => format!("match {} {{ {} arm(s) }}", one_line(s), arms.len()),
        HirKind::Spawn(_) => "spawn {…}".to_string(),
        HirKind::SpawnScope(_) => "spawn_scope {…}".to_string(),
        HirKind::Channel(t, _) => format!("channel<{}>()", crate::symbols::type_to_string(t)),
    };
    format!("{inner}:{}", e.ty.describe())
}

/// Reorders call arguments into parameter order and fills omitted ones from their defaults
/// (lowered by `lower_default`). The single implementation shared by the HIR and the native
/// backend, so both agree on what a call with named/default arguments means.
pub fn arrange_arguments<T>(
    params: &[Param],
    args: Vec<(Option<String>, T)>,
    mut lower_default: impl FnMut(&Expr) -> T,
) -> Result<Vec<T>, String> {
    let mut slots: Vec<Option<T>> = (0..params.len()).map(|_| None).collect();
    let mut next = 0usize;
    for (name, value) in args {
        match name {
            None => {
                if next >= slots.len() {
                    return Err("too many arguments in call".to_string());
                }
                slots[next] = Some(value);
                next += 1;
            }
            Some(name) => match params.iter().position(|p| p.name == name) {
                Some(index) => slots[index] = Some(value),
                None => return Err(format!("no parameter named '{name}'")),
            },
        }
    }
    slots
        .into_iter()
        .zip(params)
        .map(|(slot, param)| match slot {
            Some(value) => Ok(value),
            None => match &param.default {
                Some(default) => Ok(lower_default(default)),
                None => Err(format!("missing argument for parameter '{}'", param.name)),
            },
        })
        .collect()
}
