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

use crate::ast::{BinOp, Expr, RangeKind, UnaryOp};
use crate::hir::{HirBlock, HirExpr, HirFunction, HirKind, HirStmt};
use crate::types::Ty;

type Bail<T> = Result<T, ()>;

/// What the emitter needs to know about the program, taken from the native backend's registry
/// (C type names are compared as strings, so this module never sees the backend's own types).
pub struct World {
    /// User functions: name -> (C parameter types, C return type).
    pub functions: HashMap<String, (Vec<String>, String)>,
    /// Non-generic records: name -> [(field, C type)] in declaration order. Empty when reads of
    /// records must be tracked (E1101), which only the AST path knows how to do.
    pub records: HashMap<String, Vec<(String, String)>>,
    /// Methods of records: (record, method) -> (C name, C parameter types with `self` first, C return type).
    pub methods: HashMap<(String, String), (String, Vec<String>, String)>,
    pub c_name: fn(&str) -> String,
    /// Non-generic enums (tagged unions passed by value) and their variants by name.
    pub enums: HashSet<String>,
    pub variants: HashMap<String, VariantView>,
}

/// One enum variant: its enum, tag and fields with C types (in declaration order).
#[derive(Clone)]
pub struct VariantView {
    pub enum_name: String,
    pub tag: usize,
    pub fields: Vec<(String, String)>,
}

fn is_scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::String)
}

/// Scalars compatible in C without a conversion helper (`Int` where a `Float` goes, …).
fn c_compatible(arg: &str, param: &str) -> bool {
    arg == param || (matches!(arg, "int64_t" | "double" | "bool") && matches!(param, "int64_t" | "double" | "bool"))
}

struct Emitter<'a> {
    world: &'a World,
    scopes: Vec<HashSet<String>>,
    ret: Ty,
    temp: usize,
}

impl Emitter<'_> {
    /// The C type of a value the emitter handles: scalars and non-generic records.
    fn c_type(&self, ty: &Ty) -> Bail<String> {
        match ty {
            Ty::Int => Ok("int64_t".to_string()),
            Ty::Float => Ok("double".to_string()),
            Ty::Bool => Ok("bool".to_string()),
            Ty::String => Ok("const char*".to_string()),
            Ty::Named(n) if self.world.records.contains_key(n) => Ok(format!("{n}*")),
            Ty::Named(n) if self.world.enums.contains(n) => Ok(n.clone()),
            _ => Err(()),
        }
    }

    fn field(&self, record: &Ty, field: &str) -> Bail<String> {
        let Ty::Named(n) = record else { return Err(()) };
        self.world.records.get(n).and_then(|fs| fs.iter().find(|(f, _)| f == field)).map(|(_, t)| t.clone()).ok_or(())
    }
}

/// The body (without braces) of an eligible function, or `None`.
pub fn generate(f: &HirFunction, world: &World) -> Option<String> {
    let mut e = Emitter { world, scopes: vec![f.params.iter().map(|(n, _)| n.clone()).collect()], ret: f.ret.clone(), temp: 0 };
    if !f.generics.is_empty() || f.params.iter().any(|(_, t)| e.c_type(t).is_err()) {
        return None;
    }
    if f.ret != Ty::Void && e.c_type(&f.ret).is_err() {
        return None;
    }
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
                let ty = self.c_type(&value.ty)?;
                let code = self.expr(value)?;
                out.push_str(&format!("    {ty} {name} = {code};\n"));
                self.scopes.last_mut().expect("scope").insert(name.clone());
            }
            HirStmt::Assign { name, value } => {
                let code = self.expr(value)?;
                if self.declared(name) {
                    out.push_str(&format!("    {name} = {code};\n"));
                } else {
                    let ty = self.c_type(&value.ty)?;
                    out.push_str(&format!("    {ty} {name} = {code};\n"));
                    self.scopes.last_mut().expect("scope").insert(name.clone());
                }
            }
            HirStmt::FieldAssign { target, value } => {
                let HirKind::Field(obj, field) = &target.kind else { return Err(()) };
                let field_ty = self.field(&obj.ty, field)?;
                if !c_compatible(&self.c_type(&value.ty)?, &field_ty) && self.c_type(&value.ty)? != field_ty {
                    return Err(());
                }
                let (o, v) = (self.expr(obj)?, self.expr(value)?);
                out.push_str(&format!("    {o}->{field} = {v};\n"));
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

    /// `match`, compiled like the AST path: a scrutinee variable, a `matched` flag, a result
    /// variable and one `if (!matched && <pattern>)` per arm, in source order.
    fn matching(&mut self, e: &HirExpr, scrutinee: &HirExpr, arms: &[crate::hir::HirArm]) -> Bail<String> {
        if arms.is_empty() {
            return Err(());
        }
        let scrutinee_c = self.c_type(&scrutinee.ty)?;
        let void = e.ty == Ty::Void;
        let result_c = if void { String::new() } else { self.c_type(&e.ty)? };
        let scrutinee_code = self.expr(scrutinee)?;
        self.temp += 1;
        let (svar, mvar, rvar) = (format!("__hir_s{}", self.temp), format!("__hir_m{}", self.temp), format!("__hir_r{}", self.temp));
        let mut blocks = String::new();
        for arm in arms {
            let mut condition = format!("!{mvar}");
            let mut bindings = String::new();
            let mut bound = HashSet::new();
            self.pattern(&arm.pattern, &svar, &scrutinee.ty, &mut condition, &mut bindings, &mut bound)?;
            self.scopes.push(bound);
            let guard = match &arm.guard {
                Some(g) => Some(self.expr(g)?),
                None => None,
            };
            let body = self.block_value(&arm.body);
            self.scopes.pop();
            let body = body?;
            let commit = if void { format!("{body}; {mvar} = 1;") } else { format!("{rvar} = {body}; {mvar} = 1;") };
            let commit = match guard {
                Some(g) => format!("if ({g}) {{ {commit} }}"),
                None => commit,
            };
            blocks.push_str(&format!("if ({condition}) {{ {bindings} {commit} }} "));
        }
        let decl = if void { String::new() } else { format!("{result_c} {rvar};") };
        let yielded = if void { "(void)0".to_string() } else { rvar };
        Ok(format!(
            "({{ {scrutinee_c} {svar} = {scrutinee_code}; int {mvar} = 0; {decl} {blocks} if (!{mvar}) {{ fprintf(stderr, \"ostrin: non-exhaustive match at runtime\\n\"); abort(); }} {yielded}; }})"
        ))
    }

    /// One pattern: its condition (appended to `condition`) and the bindings it introduces.
    fn pattern(&mut self, pattern: &crate::ast::Pattern, var: &str, ty: &Ty, condition: &mut String, bindings: &mut String, bound: &mut HashSet<String>) -> Bail<()> {
        use crate::ast::Pattern;
        let c = self.c_type(ty)?;
        match pattern {
            Pattern::Wildcard => Ok(()),
            Pattern::Ident(name) => {
                let unit = self.world.variants.get(name).filter(|v| v.fields.is_empty() && *ty == Ty::Named(v.enum_name.clone()));
                match unit {
                    Some(v) => {
                        condition.push_str(&format!(" && ({var}.tag == {})", v.tag));
                    }
                    None => {
                        bindings.push_str(&format!("{c} {name} = {var}; "));
                        bound.insert(name.clone());
                    }
                }
                Ok(())
            }
            Pattern::Variant(name, fields) => {
                let v = self.world.variants.get(name).filter(|v| *ty == Ty::Named(v.enum_name.clone())).ok_or(())?.clone();
                condition.push_str(&format!(" && ({var}.tag == {})", v.tag));
                for (position, (field_name, sub)) in fields.iter().enumerate() {
                    let (actual, field_c) = v.fields.iter().find(|(n, _)| n == field_name).or_else(|| v.fields.get(position)).ok_or(())?.clone();
                    let field_ty = self.ty_of_c(&field_c)?;
                    self.pattern(sub, &format!("{var}.data.{name}.{actual}"), &field_ty, condition, bindings, bound)?;
                }
                Ok(())
            }
            Pattern::Literal(lit) => {
                let code = match lit.unlocated() {
                    Expr::IntLiteral(v) if *ty == Ty::Int => format!("INT64_C({v})"),
                    Expr::BoolLiteral(v) if *ty == Ty::Bool => v.to_string(),
                    Expr::StringLiteral(s) if *ty == Ty::String => crate::codegen::c_string_literal(s),
                    _ => return Err(()),
                };
                condition.push_str(&if *ty == Ty::String { format!(" && (strcmp({var}, {code}) == 0)") } else { format!(" && ({var} == {code})") });
                Ok(())
            }
            Pattern::Range(lo, kind, hi) => {
                let (Expr::IntLiteral(lo), Expr::IntLiteral(hi)) = (lo.unlocated(), hi.unlocated()) else { return Err(()) };
                if *ty != Ty::Int {
                    return Err(());
                }
                let upper = if *kind == RangeKind::To { "<=" } else { "<" };
                condition.push_str(&format!(" && ({var} >= INT64_C({lo}) && {var} {upper} INT64_C({hi}))"));
                Ok(())
            }
        }
    }

    /// The `Ty` a variant field's C type stands for (only the types this emitter handles).
    fn ty_of_c(&self, c: &str) -> Bail<Ty> {
        Ok(match c {
            "int64_t" => Ty::Int,
            "double" => Ty::Float,
            "bool" => Ty::Bool,
            "const char*" => Ty::String,
            other => {
                if let Some(name) = other.strip_suffix('*') {
                    if self.world.records.contains_key(name) {
                        return Ok(Ty::Named(name.to_string()));
                    }
                }
                if self.world.enums.contains(other) {
                    return Ok(Ty::Named(other.to_string()));
                }
                return Err(());
            }
        })
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
        if e.ty != Ty::Void {
            self.c_type(&e.ty)?;
        }
        match &e.kind {
            HirKind::Int(v) => Ok(format!("INT64_C({v})")),
            HirKind::Float(v) => Ok(format!("{v:?}")),
            HirKind::Bool(v) => Ok(if *v { "true" } else { "false" }.to_string()),
            HirKind::Str(s) => Ok(crate::codegen::c_string_literal(s)),
            HirKind::Local(name) => Ok(name.clone()),
            HirKind::Global(name) if self.world.variants.get(name).is_some_and(|v| v.fields.is_empty()) => {
                let v = &self.world.variants[name];
                if self.c_type(&e.ty)? != v.enum_name {
                    return Err(());
                }
                Ok(format!("(({}){{ .tag = {} }})", v.enum_name, v.tag))
            }
            HirKind::Match(scrutinee, arms) => self.matching(e, scrutinee, arms),
            HirKind::Field(obj, field) => {
                let field_ty = self.field(&obj.ty, field)?;
                if field_ty != self.c_type(&e.ty)? {
                    return Err(());
                }
                Ok(format!("{}->{field}", self.expr(obj)?))
            }
            HirKind::Record { name, type_args, fields } if type_args.is_empty() => {
                let declared = self.world.records.get(name).ok_or(())?.clone();
                if fields.len() != declared.len() {
                    return Err(());
                }
                // Same shape as the AST path: allocate, then assign each field in source order.
                let temp = format!("__hir_rec{}", self.scopes.len());
                let mut body = format!("{name}* {temp} = ({name}*)malloc(sizeof({name})); if (!{temp}) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }} ");
                for (field, value) in fields {
                    let want = declared.iter().find(|(f, _)| f == field).map(|(_, t)| t.clone()).ok_or(())?;
                    let have = self.c_type(&value.ty)?;
                    if !c_compatible(&have, &want) {
                        return Err(());
                    }
                    let code = self.expr(value)?;
                    body.push_str(&format!("{temp}->{field} = {code}; "));
                }
                Ok(format!("({{ {body} {temp}; }})"))
            }
            HirKind::MethodCall { recv, method, args, subst: None, type_args } if type_args.is_empty() => {
                let Ty::Named(record) = &recv.ty else { return Err(()) };
                let (c_name, params, ret) = self.world.methods.get(&(record.clone(), method.clone())).ok_or(())?.clone();
                if params.len() != args.len() + 1 || params[0] != self.c_type(&recv.ty)? || args.iter().any(|a| a.name.is_some()) {
                    return Err(());
                }
                if e.ty == Ty::Void { if ret != "void" { return Err(()); } } else if ret != self.c_type(&e.ty)? {
                    return Err(());
                }
                let mut codes = vec![self.expr(recv)?];
                for (arg, want) in args.iter().zip(&params[1..]) {
                    if !c_compatible(&self.c_type(&arg.value.ty)?, want) {
                        return Err(());
                    }
                    codes.push(self.expr(&arg.value)?);
                }
                Ok(format!("{c_name}({})", codes.join(", ")))
            }
            HirKind::Unary(op, inner) => {
                if !is_scalar(&inner.ty) {
                    return Err(());
                }
                let code = self.expr(inner)?;
                match op {
                    UnaryOp::Neg if matches!(inner.ty, Ty::Int | Ty::Float) => Ok(format!("(-{code})")),
                    UnaryOp::Not if inner.ty == Ty::Bool => Ok(format!("(!{code})")),
                    _ => Err(()),
                }
            }
            HirKind::Binary(op, l, r) => {
                if !is_scalar(&l.ty) || !is_scalar(&r.ty) {
                    return Err(());
                }
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
                if name == "print" && !self.world.functions.contains_key(name) && args.len() == 1 && args[0].name.is_none() {
                    let arg = &args[0].value;
                    if !is_scalar(&arg.ty) {
                        return Err(());
                    }
                    let code = self.expr(arg)?;
                    return match arg.ty {
                        Ty::Int => Ok(format!("printf(\"%lld\\n\", (long long)({code}))")),
                        Ty::Float => Ok(format!("ostrin_print_float({code})")),
                        Ty::Bool => Ok(format!("printf(\"%s\\n\", (({code}) ? \"true\" : \"false\"))")),
                        Ty::String => Ok(format!("printf(\"%s\\n\", {code})")),
                        _ => Err(()),
                    };
                }
                if let (Some(v), false) = (self.world.variants.get(name), self.world.functions.contains_key(name)) {
                    let v = v.clone();
                    if v.fields.len() != args.len() || args.iter().any(|a| a.name.is_some()) || self.c_type(&e.ty)? != v.enum_name {
                        return Err(());
                    }
                    let mut inits = Vec::new();
                    for (arg, (field, want)) in args.iter().zip(&v.fields) {
                        if !c_compatible(&self.c_type(&arg.value.ty)?, want) {
                            return Err(());
                        }
                        inits.push(format!(".{field} = {}", self.expr(&arg.value)?));
                    }
                    return Ok(format!("(({}){{ .tag = {}, .data.{name} = {{ {} }} }})", v.enum_name, v.tag, inits.join(", ")));
                }
                let (params, ret) = self.world.functions.get(name).ok_or(())?.clone();
                if params.len() != args.len() || args.iter().any(|a| a.name.is_some()) {
                    return Err(());
                }
                for (arg, want) in args.iter().zip(&params) {
                    if !c_compatible(&self.c_type(&arg.value.ty)?, want) {
                        return Err(());
                    }
                }
                if e.ty == Ty::Void { if ret != "void" { return Err(()); } } else if !c_compatible(&self.c_type(&e.ty)?, &ret) {
                    return Err(());
                }
                let codes = args.iter().map(|a| self.expr(&a.value)).collect::<Bail<Vec<_>>>()?;
                Ok(format!("{}({})", (self.world.c_name)(name), codes.join(", ")))
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

