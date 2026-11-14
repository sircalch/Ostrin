//! Native-only rewrite for `mut` bindings captured by closures.
//!
//! `docs/design/02-funciones-y-firmas.md` §4 gives a closure that captures a `mut` binding
//! inside the same task a reference to that binding: a read sees later outer assignments
//! and a write is seen by the enclosing function. The interpreter follows that rule. The
//! native backends copy captured values into the closure environment, so the rule breaks.
//!
//! This pass gives the native backends the shared binding without changing their code
//! generators. Each captured `mut` binding becomes a one-field record cell
//! (`OstrinCell<k> { mut value: T }`). Reads become `cell.value`, assignments become
//! `cell.value = …`, and a captured function-valued binding is called through a generated
//! helper (`ostrin_cell_call_<k>`), because the language cannot call a function stored in a
//! field directly. The record is a heap object shared by the closure and the enclosing
//! function, so both see the same storage.
//!
//! The analysis is conservative. A name is rewritten only when it has exactly one
//! declaration in its function and that declaration is a `mut` binding captured by a
//! closure. Anything it cannot prove is reported as an error, never silently compiled.
//! The interpreter does not run this pass.

use crate::ast::{
    Arg, Block, Expr, FieldDecl, FunctionDecl, Item, LocatedStmt, MatchArm, Param, Pattern,
    RecordDecl, Span, Stmt, Type,
};
use crate::typeck::TypedProgram;
use crate::types::Ty;
use std::collections::HashMap;

#[derive(Clone)]
struct Decl {
    /// A `let`-style binding (`name = value` or `mut name = value`); false for parameters,
    /// loop variables, lambda parameters and pattern variables.
    binding: bool,
    mutable: bool,
    block: usize,
    lambda_depth: usize,
    declared: Option<Type>,
    value_ty: Option<Ty>,
}

#[derive(Clone)]
struct Use {
    name: String,
    lambda_depth: usize,
    stack: Vec<usize>,
}

#[derive(Default)]
struct Analysis {
    decls: HashMap<String, Vec<Decl>>,
    uses: Vec<Use>,
    assigns: Vec<Use>,
}

#[derive(Clone)]
struct Cell {
    record: String,
    decl_block: usize,
    helper: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Analyze,
    Rewrite,
}

struct Walker<'a> {
    mode: Mode,
    typed: &'a TypedProgram,
    stack: Vec<usize>,
    next_block: usize,
    lambda_depth: usize,
    analysis: Analysis,
    cells: HashMap<String, Cell>,
}

impl<'a> Walker<'a> {
    fn new(mode: Mode, typed: &'a TypedProgram, cells: HashMap<String, Cell>) -> Self {
        Walker {
            mode,
            typed,
            stack: Vec::new(),
            next_block: 0,
            lambda_depth: 0,
            analysis: Analysis::default(),
            cells,
        }
    }

    fn enter(&mut self, names: &[String]) {
        let id = self.next_block;
        self.next_block += 1;
        self.stack.push(id);
        if self.mode == Mode::Analyze {
            for name in names {
                self.analysis
                    .decls
                    .entry(name.clone())
                    .or_default()
                    .push(Decl {
                        binding: false,
                        mutable: false,
                        block: id,
                        lambda_depth: self.lambda_depth,
                        declared: None,
                        value_ty: None,
                    });
            }
        }
    }

    fn leave(&mut self) {
        self.stack.pop();
    }

    fn current_block(&self) -> usize {
        *self.stack.last().expect("a block is open")
    }

    fn block(&mut self, b: &mut Block, names: &[String]) {
        self.enter(names);
        self.block_body(b);
        self.leave();
    }

    fn block_body(&mut self, b: &mut Block) {
        for s in &mut b.stmts {
            self.stmt(&mut s.stmt);
        }
        if let Some(tail) = &mut b.tail {
            self.expr(tail);
        }
    }

    fn record_use(&mut self, name: &str) {
        if self.mode == Mode::Analyze {
            self.analysis.uses.push(Use {
                name: name.to_string(),
                lambda_depth: self.lambda_depth,
                stack: self.stack.clone(),
            });
        }
    }

    /// The cell of `name` when this use sits inside the scope of its declaration.
    fn live_cell(&self, name: &str) -> Option<Cell> {
        if self.mode != Mode::Rewrite {
            return None;
        }
        self.cells
            .get(name)
            .filter(|cell| self.stack.contains(&cell.decl_block))
            .cloned()
    }

    fn stmt(&mut self, s: &mut Stmt) {
        let replacement: Option<Stmt> = match s {
            Stmt::Binding {
                mut_,
                name,
                ty,
                value,
            } => {
                self.expr(value);
                if self.mode == Mode::Analyze {
                    let key = &*value as *const Expr as usize;
                    let value_ty = self.typed.node_types.get(&key).cloned();
                    let block = self.current_block();
                    self.analysis
                        .decls
                        .entry(name.clone())
                        .or_default()
                        .push(Decl {
                            binding: true,
                            mutable: *mut_,
                            block,
                            lambda_depth: self.lambda_depth,
                            declared: ty.clone(),
                            value_ty,
                        });
                } else if let Some(cell) = self.cells.get(name.as_str()).cloned() {
                    let inner = std::mem::replace(value, Expr::BoolLiteral(false));
                    *value = Expr::RecordLiteral(
                        cell.record.clone(),
                        vec![("value".to_string(), inner)],
                    );
                    *ty = Some(Type::Named(cell.record, Vec::new()));
                }
                None
            }
            Stmt::Assign { name, value } => {
                self.expr(value);
                if self.mode == Mode::Analyze {
                    let stack = self.stack.clone();
                    self.analysis.assigns.push(Use {
                        name: name.clone(),
                        lambda_depth: self.lambda_depth,
                        stack,
                    });
                    None
                } else if self.live_cell(name).is_some() {
                    let value = std::mem::replace(value, Expr::BoolLiteral(false));
                    Some(Stmt::FieldAssign {
                        target: field_value(Expr::Ident(name.clone())),
                        value,
                    })
                } else {
                    None
                }
            }
            Stmt::Return(value) | Stmt::Break(value) => {
                if let Some(e) = value {
                    self.expr(e);
                }
                None
            }
            Stmt::For {
                pattern,
                iter,
                body,
            } => {
                self.expr(iter);
                self.block(body, std::slice::from_ref(pattern));
                None
            }
            Stmt::While { cond, body } => {
                self.expr(cond);
                self.block(body, &[]);
                None
            }
            Stmt::FieldAssign { target, value } => {
                self.expr(target);
                self.expr(value);
                None
            }
            Stmt::Expr(e) => {
                self.expr(e);
                None
            }
            Stmt::Continue => None,
        };
        if let Some(stmt) = replacement {
            *s = stmt;
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        if let Some(replacement) = self.expr_inner(e) {
            *e = replacement;
        }
    }

    /// Visits `e`; returns a replacement when the node itself must change.
    fn expr_inner(&mut self, e: &mut Expr) -> Option<Expr> {
        match e {
            Expr::Located(inner, _) => {
                self.expr(inner);
                None
            }
            Expr::Ident(name) => {
                let name = name.clone();
                self.record_use(&name);
                self.live_cell(&name)
                    .map(|_| field_value(Expr::Ident(name)))
            }
            Expr::Call(callee, args) => {
                if let Some(name) = ident_of(callee).map(str::to_string) {
                    self.record_use(&name);
                    for arg in args.iter_mut() {
                        self.arg(arg);
                    }
                    if let Some(cell) = self.live_cell(&name) {
                        let helper = cell.helper.expect(
                            "a captured binding called as a function has a helper (checked by the analysis)",
                        );
                        let mut new_args = vec![Arg::Positional(Expr::Ident(name))];
                        new_args.append(args);
                        return Some(Expr::Call(Box::new(Expr::Ident(helper)), new_args));
                    }
                    return None;
                }
                self.expr(callee);
                for arg in args.iter_mut() {
                    self.arg(arg);
                }
                None
            }
            Expr::GenericCall(callee, _, args) => {
                self.expr(callee);
                for arg in args.iter_mut() {
                    self.arg(arg);
                }
                None
            }
            Expr::Unary(_, a) | Expr::FieldAccess(a, _) | Expr::Try(a, None) => {
                self.expr(a);
                None
            }
            Expr::Try(a, Some(b)) | Expr::Binary(_, a, b) | Expr::Index(a, b) | Expr::Within(a, b) | Expr::As(a, b) => {
                self.expr(a);
                self.expr(b);
                None
            }
            Expr::Range(a, _, b, rest) => {
                self.expr(a);
                self.expr(b);
                if let Some(c) = rest {
                    self.expr(c);
                }
                None
            }
            Expr::Approximately(a, b, c) => {
                self.expr(a);
                self.expr(b);
                self.expr(c);
                None
            }
            Expr::If(cond, then, otherwise) => {
                self.expr(cond);
                self.block(then, &[]);
                if let Some(otherwise) = otherwise {
                    self.block(otherwise, &[]);
                }
                None
            }
            Expr::Block(b) | Expr::Loop(b) | Expr::Spawn(b) | Expr::SpawnScope(b) => {
                self.block(b, &[]);
                None
            }
            Expr::Lambda(names, body) => {
                self.lambda_depth += 1;
                let names = names.clone();
                self.block(body, &names);
                self.lambda_depth -= 1;
                None
            }
            Expr::ListLiteral(items) | Expr::SetLiteral(items) => {
                for item in items.iter_mut() {
                    self.expr(item);
                }
                None
            }
            Expr::MapLiteral(pairs) => {
                for (k, v) in pairs.iter_mut() {
                    self.expr(k);
                    self.expr(v);
                }
                None
            }
            Expr::RecordLiteral(_, fields) | Expr::GenericRecordLiteral(_, _, fields) => {
                for (_, v) in fields.iter_mut() {
                    self.expr(v);
                }
                None
            }
            Expr::Match(scrutinee, arms) => {
                self.expr(scrutinee);
                for arm in arms.iter_mut() {
                    self.arm(arm);
                }
                None
            }
            Expr::Channel(_, capacity) => {
                if let Some(c) = capacity {
                    self.expr(c);
                }
                None
            }
            Expr::UnitLiteral(inner, _) => {
                self.expr(inner);
                None
            }
            Expr::IntLiteral(_)
            | Expr::SizedIntLiteral(..)
            | Expr::FloatLiteral(_)
            | Expr::Float32Literal(_)
            | Expr::StringLiteral(_)
            | Expr::CharLiteral(_)
            | Expr::BoolLiteral(_)
            | Expr::EmptyCollection(..) => None,
        }
    }

    fn arg(&mut self, arg: &mut Arg) {
        match arg {
            Arg::Positional(e) | Arg::Named(_, e) => self.expr(e),
        }
    }

    fn arm(&mut self, arm: &mut MatchArm) {
        let mut names = Vec::new();
        pattern_names(&arm.pattern, &mut names);
        self.enter(&names);
        if let Some(guard) = &mut arm.guard {
            self.expr(guard);
        }
        self.block_body(&mut arm.body);
        self.leave();
    }
}

fn field_value(target: Expr) -> Expr {
    Expr::FieldAccess(Box::new(target), "value".to_string())
}

fn ident_of(e: &Expr) -> Option<&str> {
    match e.unlocated() {
        Expr::Ident(name) => Some(name.as_str()),
        _ => None,
    }
}

fn pattern_names(p: &Pattern, out: &mut Vec<String>) {
    match p {
        Pattern::Ident(name) => out.push(name.clone()),
        Pattern::Variant(_, fields) => {
            for (_, sub) in fields {
                pattern_names(sub, out);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::Range(..) => {}
    }
}

fn named(name: &str) -> Type {
    Type::Named(name.to_string(), Vec::new())
}

/// The surface type of a checker type, when the cell can name it. `None` means the
/// pass cannot represent the binding, and the caller reports that instead of guessing.
fn ty_to_type(ty: &Ty) -> Option<Type> {
    Some(match ty {
        Ty::Int => named("Int"),
        Ty::Float => named("Float"),
        Ty::Bool => named("Bool"),
        Ty::Char => named("Char"),
        Ty::String => named("String"),
        Ty::Void => named("Void"),
        Ty::Named(name) => named(name),
        Ty::List(inner) => Type::Named("List".to_string(), vec![ty_to_type(inner)?]),
        Ty::Fn(params, ret) => Type::Fn(
            params.iter().map(ty_to_type).collect::<Option<Vec<_>>>()?,
            Box::new(ty_to_type(ret)?),
        ),
        _ => return None,
    })
}

fn located(stmt: Stmt) -> LocatedStmt {
    LocatedStmt {
        stmt,
        span: Span::default(),
    }
}

/// `ostrin_cell_call_<k>(cell, args…)`: calls the function held by a cell.
fn push_helper(
    k: usize,
    record: &str,
    params: &[Type],
    ret: &Type,
    extra: &mut Vec<Item>,
) -> String {
    let helper = format!("ostrin_cell_call_{k}");
    let mut fn_params = vec![Param {
        name: "cell".to_string(),
        ty: Type::Named(record.to_string(), Vec::new()),
        default: None,
        is_mut: false,
    }];
    let mut args = Vec::new();
    for (i, ty) in params.iter().enumerate() {
        let name = format!("arg{i}");
        fn_params.push(Param {
            name: name.clone(),
            ty: ty.clone(),
            default: None,
            is_mut: false,
        });
        args.push(Arg::Positional(Expr::Ident(name)));
    }
    let call = Expr::Call(Box::new(Expr::Ident("callee".to_string())), args);
    let is_void = matches!(ret, Type::Named(n, a) if n == "Void" && a.is_empty());
    let mut stmts = vec![located(Stmt::Binding {
        mut_: false,
        name: "callee".to_string(),
        ty: None,
        value: field_value(Expr::Ident("cell".to_string())),
    })];
    if is_void {
        stmts.push(located(Stmt::Expr(call)));
    } else {
        stmts.push(located(Stmt::Return(Some(call))));
    }
    extra.push(Item::Function(FunctionDecl {
        name: helper.clone(),
        is_pub: false,
        generics: Vec::new(),
        params: fn_params,
        return_type: ret.clone(),
        body: Block { stmts, tail: None },
        span: Span::default(),
        source_file: None,
    }));
    helper
}

fn compute_cells(
    analysis: &Analysis,
    counter: &mut usize,
    extra: &mut Vec<Item>,
) -> Result<HashMap<String, Cell>, String> {
    let mut names: Vec<&String> = analysis.decls.keys().collect();
    names.sort();
    let mut cells = HashMap::new();
    for name in names {
        let decls = &analysis.decls[name];
        let captured = analysis.uses.iter().any(|u| {
            &u.name == name
                && decls.iter().any(|d| {
                    d.binding && d.mutable && u.lambda_depth > d.lambda_depth && u.stack.contains(&d.block)
                })
        });
        if !captured {
            continue;
        }
        if decls.len() != 1 {
            return Err(format!(
                "native backend: the mutable binding '{name}' is captured by a closure, but the name is declared more than once in the same function; give the bindings different names"
            ));
        }
        let decl = &decls[0];
        if analysis
            .assigns
            .iter()
            .any(|a| &a.name == name && !a.stack.contains(&decl.block))
        {
            return Err(format!(
                "native backend: the mutable binding '{name}' is captured by a closure and also assigned where it is not in scope; give the bindings different names"
            ));
        }
        let ty = match &decl.declared {
            Some(declared) => declared.clone(),
            None => {
                let checked = decl.value_ty.as_ref().ok_or_else(|| {
                    format!("native backend: the type of the mutable binding '{name}' captured by a closure is unknown")
                })?;
                ty_to_type(checked).ok_or_else(|| {
                    format!(
                        "native backend: a closure captures the mutable binding '{name}' of type {checked:?}, which is not supported yet"
                    )
                })?
            }
        };
        let k = *counter;
        *counter += 1;
        let record = format!("OstrinCell{k}");
        extra.push(Item::Record(RecordDecl {
            name: record.clone(),
            module_path: Vec::new(),
            is_pub: false,
            generics: Vec::new(),
            derives: Vec::new(),
            fields: vec![FieldDecl {
                name: "value".to_string(),
                is_pub: false,
                is_mut: true,
                ty: ty.clone(),
                default: None,
            }],
            span: Span::default(),
            source_file: None,
        }));
        let helper = match &ty {
            Type::Fn(params, ret) => Some(push_helper(k, &record, params, ret, extra)),
            _ => None,
        };
        cells.insert(
            name.clone(),
            Cell {
                record,
                decl_block: decl.block,
                helper,
            },
        );
    }
    Ok(cells)
}

fn rewrite_function(
    params: &[Param],
    body: &mut Block,
    typed: &TypedProgram,
    extra: &mut Vec<Item>,
    counter: &mut usize,
    changed: &mut bool,
) -> Result<(), String> {
    let names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    let mut analyze = Walker::new(Mode::Analyze, typed, HashMap::new());
    analyze.block(body, &names);
    let cells = compute_cells(&analyze.analysis, counter, extra)?;
    if cells.is_empty() {
        return Ok(());
    }
    *changed = true;
    let mut rewrite = Walker::new(Mode::Rewrite, typed, cells);
    rewrite.block(body, &names);
    Ok(())
}

/// Rewrites captured `mut` bindings in every function and method. Returns the program and
/// whether anything changed. `typed` must come from checking `items` as passed in.
pub fn rewrite_native(
    mut items: Vec<Item>,
    typed: &TypedProgram,
) -> Result<(Vec<Item>, bool), String> {
    let mut extra = Vec::new();
    let mut counter = 0;
    let mut changed = false;
    for item in items.iter_mut() {
        match item {
            Item::Function(f) => rewrite_function(
                &f.params,
                &mut f.body,
                typed,
                &mut extra,
                &mut counter,
                &mut changed,
            )?,
            Item::Impl(imp) => {
                for method in imp.methods.iter_mut() {
                    rewrite_function(
                        &method.params,
                        &mut method.body,
                        typed,
                        &mut extra,
                        &mut counter,
                        &mut changed,
                    )?;
                }
            }
            _ => {}
        }
    }
    if changed {
        items.extend(extra);
    }
    Ok((items, changed))
}
