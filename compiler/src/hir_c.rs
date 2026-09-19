//! C generation straight from the HIR, for the functions it can already handle: this is the
//! first family of nodes (scalar code) moved off the AST in the plan of
//! `docs/design/20-hir-y-ir.md`. A function is *eligible* when every node in it is a scalar
//! (`Int`, `Float`, `Bool`, or `Void` for statements) and it only uses locals, scalar
//! literals, arithmetic/comparison/logic, `if`, `while`, `for` over an `Int` range, `return`,
//! `break`/`continue` and calls to other user functions with scalar arguments. Anything else
//! makes `generate` return `None` and the AST generator keeps that function.
//!
//! The emitted C mirrors what the AST generator writes for the same constructs (same
//! operators, `ostrin_idiv` for `Int / Int`, C blocks for scopes), so both paths behave
//! identically; the differential tests compare them through the interpreter.

use std::collections::{HashMap, HashSet};

use crate::ast::{BinOp, RangeKind, UnaryOp};
use crate::hir::{HirBlock, HirExpr, HirFunction, HirKind, HirStmt};
use crate::types::Ty;

type Bail<T> = Result<T, ()>;

fn c_scalar(ty: &Ty) -> Bail<&'static str> {
    match ty {
        Ty::Int => Ok("int64_t"),
        Ty::Float => Ok("double"),
        Ty::Bool => Ok("bool"),
        Ty::String => Ok("const char*"),
        _ => Err(()),
    }
}

struct Emitter<'a> {
    /// Names of the user functions a call may target, with their C names.
    functions: &'a HashSet<String>,
    c_name: fn(&str) -> String,
    scopes: Vec<HashSet<String>>,
    ret: Ty,
}

/// The body (without braces) of an eligible function, or `None`.
pub fn generate(f: &HirFunction, functions: &HashSet<String>, c_name: fn(&str) -> String) -> Option<String> {
    if !f.generics.is_empty() || f.params.iter().any(|(_, t)| c_scalar(t).is_err()) {
        return None;
    }
    if !matches!(f.ret, Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Void) {
        return None;
    }
    let mut e = Emitter { functions, c_name, scopes: vec![f.params.iter().map(|(n, _)| n.clone()).collect()], ret: f.ret.clone() };
    let mut out = String::new();
    e.body(&f.body, &mut out).ok()?;
    Some(out)
}

impl Emitter<'_> {
    fn declared(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }

    fn body(&mut self, block: &HirBlock, out: &mut String) -> Bail<()> {
        for stmt in &block.stmts {
            self.stmt(stmt, out)?;
        }
        match &block.tail {
            Some(tail) if self.ret == Ty::Void => {
                self.expr_stmt(tail, out)?;
                out.push_str("    return;\n");
            }
            Some(tail) => {
                let code = self.expr(tail)?;
                out.push_str(&format!("    return {code};\n"));
            }
            None => out.push_str("    return;\n"),
        }
        Ok(())
    }

    fn scoped_stmts(&mut self, block: &HirBlock, out: &mut String) -> Bail<()> {
        self.scopes.push(HashSet::new());
        for stmt in &block.stmts {
            self.stmt(stmt, out)?;
        }
        if let Some(tail) = &block.tail {
            self.expr_stmt(tail, out)?;
        }
        self.scopes.pop();
        Ok(())
    }

    fn stmt(&mut self, stmt: &HirStmt, out: &mut String) -> Bail<()> {
        match stmt {
            HirStmt::Let { name, declared, value, .. } => {
                if declared.is_some() {
                    return Err(());
                }
                let ty = c_scalar(&value.ty)?;
                let code = self.expr(value)?;
                out.push_str(&format!("    {ty} {name} = {code};\n"));
                self.scopes.last_mut().expect("scope").insert(name.clone());
            }
            HirStmt::Assign { name, value } => {
                let code = self.expr(value)?;
                if self.declared(name) {
                    out.push_str(&format!("    {name} = {code};\n"));
                } else {
                    let ty = c_scalar(&value.ty)?;
                    out.push_str(&format!("    {ty} {name} = {code};\n"));
                    self.scopes.last_mut().expect("scope").insert(name.clone());
                }
            }
            HirStmt::Return(Some(value)) => {
                let code = self.expr(value)?;
                out.push_str(&format!("    return {code};\n"));
            }
            HirStmt::Return(None) => out.push_str("    return;\n"),
            HirStmt::Break(None) => out.push_str("    break;\n"),
            HirStmt::Continue => out.push_str("    continue;\n"),
            HirStmt::While { cond, body } => {
                let c = self.expr(cond)?;
                out.push_str(&format!("    while ({c}) {{\n"));
                self.scoped_stmts(body, out)?;
                out.push_str("    }\n");
            }
            HirStmt::For { var, iter, body } => {
                let HirKind::Range(start, kind, end, None) = &iter.kind else { return Err(()) };
                if start.ty != Ty::Int || end.ty != Ty::Int {
                    return Err(());
                }
                let (s, e) = (self.expr(start)?, self.expr(end)?);
                let cmp = if *kind == RangeKind::To { "<=" } else { "<" };
                out.push_str(&format!("    for (int64_t {var} = {s}; {var} {cmp} {e}; {var}++) {{\n"));
                self.scopes.push(HashSet::from([var.clone()]));
                self.scoped_stmts(body, out)?;
                self.scopes.pop();
                out.push_str("    }\n");
            }
            HirStmt::Expr(e) => self.expr_stmt(e, out)?,
            _ => return Err(()),
        }
        Ok(())
    }

    /// An expression whose value is discarded (statement position).
    fn expr_stmt(&mut self, e: &HirExpr, out: &mut String) -> Bail<()> {
        match &e.kind {
            HirKind::If(cond, then_block, else_block) => {
                let c = self.expr(cond)?;
                out.push_str(&format!("    if ({c}) {{\n"));
                self.scoped_stmts(then_block, out)?;
                out.push_str("    }");
                if let Some(else_block) = else_block {
                    out.push_str(" else {\n");
                    self.scoped_stmts(else_block, out)?;
                    out.push_str("    }");
                }
                out.push('\n');
            }
            HirKind::Block(block) => {
                out.push_str("    {\n");
                self.scoped_stmts(block, out)?;
                out.push_str("    }\n");
            }
            _ => {
                let code = self.expr(e)?;
                out.push_str(&format!("    {code};\n"));
            }
        }
        Ok(())
    }

    fn block_value(&mut self, block: &HirBlock) -> Bail<String> {
        let mut body = String::new();
        self.scopes.push(HashSet::new());
        for stmt in &block.stmts {
            self.stmt(stmt, &mut body)?;
        }
        let tail = match &block.tail {
            Some(t) => self.expr(t)?,
            None => "(void)0".to_string(),
        };
        self.scopes.pop();
        Ok(format!("({{ {body} {tail}; }})"))
    }

    fn expr(&mut self, e: &HirExpr) -> Bail<String> {
        c_scalar(&e.ty).or_else(|_| if e.ty == Ty::Void { Ok("void") } else { Err(()) })?;
        match &e.kind {
            HirKind::Int(v) => Ok(format!("INT64_C({v})")),
            HirKind::Float(v) => Ok(format!("{v:?}")),
            HirKind::Bool(v) => Ok(if *v { "true" } else { "false" }.to_string()),
            HirKind::Str(s) => Ok(crate::codegen::c_string_literal(s)),
            HirKind::Local(name) => Ok(name.clone()),
            HirKind::Unary(op, inner) => {
                let code = self.expr(inner)?;
                match op {
                    UnaryOp::Neg if matches!(inner.ty, Ty::Int | Ty::Float) => Ok(format!("(-{code})")),
                    UnaryOp::Not if inner.ty == Ty::Bool => Ok(format!("(!{code})")),
                    _ => Err(()),
                }
            }
            HirKind::Binary(op, l, r) => {
                let (lc, rc) = (self.expr(l)?, self.expr(r)?);
                if l.ty == Ty::String || r.ty == Ty::String {
                    return match op {
                        BinOp::Add if l.ty == Ty::String && r.ty == Ty::String => Ok(format!("ostrin_str_concat({lc}, {rc})")),
                        BinOp::Eq if l.ty == r.ty => Ok(format!("(strcmp({lc}, {rc}) == 0)")),
                        BinOp::NotEq if l.ty == r.ty => Ok(format!("(strcmp({lc}, {rc}) != 0)")),
                        _ => Err(()),
                    };
                }
                if *op == BinOp::Div && l.ty == Ty::Int && r.ty == Ty::Int {
                    return Ok(format!("ostrin_idiv({lc}, {rc})"));
                }
                let c_op = match op {
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
                Ok(format!("({lc} {c_op} {rc})"))
            }
            HirKind::Call { callee, args, subst: None, type_args } if type_args.is_empty() => {
                let HirKind::Global(name) = &callee.kind else { return Err(()) };
                if name == "print" && !self.functions.contains(name) && args.len() == 1 && args[0].name.is_none() {
                    let arg = &args[0].value;
                    let code = self.expr(arg)?;
                    return match arg.ty {
                        Ty::Int => Ok(format!("printf(\"%lld\\n\", (long long)({code}))")),
                        Ty::Float => Ok(format!("ostrin_print_float({code})")),
                        Ty::Bool => Ok(format!("printf(\"%s\\n\", (({code}) ? \"true\" : \"false\"))")),
                        Ty::String => Ok(format!("printf(\"%s\\n\", {code})")),
                        _ => Err(()),
                    };
                }
                if !self.functions.contains(name) || args.iter().any(|a| a.name.is_some() || c_scalar(&a.value.ty).is_err()) {
                    return Err(());
                }
                let codes = args.iter().map(|a| self.expr(&a.value)).collect::<Bail<Vec<_>>>()?;
                Ok(format!("{}({})", (self.c_name)(name), codes.join(", ")))
            }
            HirKind::If(cond, then_block, Some(else_block)) if e.ty != Ty::Void => {
                let c = self.expr(cond)?;
                let (t, f) = (self.block_value(then_block)?, self.block_value(else_block)?);
                Ok(format!("(({c}) ? {t} : {f})"))
            }
            HirKind::Block(block) => self.block_value(block),
            _ => Err(()),
        }
    }
}

/// Every function the emitter can target, by name.
pub fn user_functions(arities: &HashMap<String, usize>, programs: &[HirFunction]) -> HashSet<String> {
    programs.iter().filter(|f| !f.name.contains('.') || f.name.contains("::")).map(|f| f.name.clone()).filter(|n| arities.contains_key(n)).collect()
}
