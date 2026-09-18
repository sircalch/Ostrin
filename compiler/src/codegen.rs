//! A real, honest first native backend: `ostrinc --emit-c`/`--compile`
//! transpile a *subset* of Ostrin to C and hand it to the system's C
//! compiler. This is not the whole language — enums, traits, generics,
//! dimensional `Quantity`, closures, collections and pattern matching all
//! still only run through the interpreter (`--run`). What is supported is
//! real: plain functions over `Int`/`Float`/`Bool`/`String`, plain records
//! (fields only, no `impl` methods — a method call fails with a clear error
//! since `gen_call` only accepts a bare function name as its callee),
//! recursion, `if`/`while`/`for <range>`, and the usual operators, compiled
//! all the way to a native executable — not reinterpreted, not simulated.
//!
//! The codegen does its own tiny, local type inference (see `CType`) rather
//! than reusing `typeck::Ty` directly: by the time this runs, the program
//! has already passed the real type checker, so this pass only needs to
//! know which concrete type each expression is (to pick a C type and a
//! `printf` conversion), not to validate anything.
//!
//! Nested `if`/blocks used *as expressions* (e.g. `x = if c { a } else { b }`)
//! are compiled using GNU statement expressions (`({ ... })`), which is why
//! `find_c_compiler` looks for gcc/clang rather than accepting any C89
//! compiler — this is a deliberate, documented trade-off to keep the
//! transpiler itself simple.
//!
//! Records are always heap-allocated and referred to through a pointer,
//! never copied by value, to match the interpreter's `Rc<RefCell<...>>`
//! identity semantics (two bindings that alias the same record must see
//! each other's field writes — see `Value::Record` in `interpreter/mod.rs`).
//! Nothing here ever frees that memory: for the short-lived programs this
//! backend targets that is an acceptable, documented trade-off, not an
//! oversight.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::symbols::type_to_string;

#[derive(Clone, PartialEq, Eq, Debug)]
enum CType {
    Int,
    Float,
    Bool,
    Str,
    Void,
    Record(String),
}

fn c_type_name(ty: &CType) -> String {
    match ty {
        CType::Int => "int64_t".to_string(),
        CType::Float => "double".to_string(),
        CType::Bool => "bool".to_string(),
        CType::Str => "const char*".to_string(),
        CType::Void => "void".to_string(),
        CType::Record(name) => format!("{name}*"),
    }
}

fn map_type(ty: &Type, record_names: &HashSet<String>) -> Result<CType, String> {
    match ty {
        Type::Named(name, args) if args.is_empty() => match name.as_str() {
            "Int" => Ok(CType::Int),
            "Float" => Ok(CType::Float),
            "Bool" => Ok(CType::Bool),
            "String" => Ok(CType::Str),
            "Void" => Ok(CType::Void),
            other if record_names.contains(other) => Ok(CType::Record(other.to_string())),
            _ => Err(format!("type '{}' is not supported by the native backend yet", type_to_string(ty))),
        },
        _ => Err(format!("type '{}' is not supported by the native backend yet", type_to_string(ty))),
    }
}

/// Like `map_type`, but resolves a method's `self`/`Self` to the record it
/// is implemented for. There is no dynamic dispatch anywhere in this
/// backend (no `dyn Trait`, no generics), so a call site always knows the
/// receiver's concrete record at compile time — `Self` is just a name for
/// it, nothing more.
fn map_method_type(ty: &Type, self_record: &str, record_names: &HashSet<String>) -> Result<CType, String> {
    if matches!(ty, Type::Named(name, args) if name == "Self" && args.is_empty()) {
        return Ok(CType::Record(self_record.to_string()));
    }
    map_type(ty, record_names)
}

fn c_function_name(name: &str) -> String {
    // The generated file supplies its own `main`, so the user's `main`
    // (which returns Void, not `int`, and takes no argv/argc) is renamed.
    if name == "main" { "ostrin_main".to_string() } else { name.to_string() }
}

const PRELUDE: &str = "#include <stdint.h>\n\
#include <stdbool.h>\n\
#include <stdio.h>\n\
#include <stdlib.h>\n\
#include <string.h>\n\
\n\
static char* ostrin_str_concat(const char* a, const char* b) {\n\
    size_t len = strlen(a) + strlen(b) + 1;\n\
    char* out = (char*)malloc(len);\n\
    if (!out) { fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }\n\
    snprintf(out, len, \"%s%s\", a, b);\n\
    return out;\n\
}\n\
\n";

/// A method resolved at codegen time — always to exactly one concrete
/// implementation, since this backend has no `dyn Trait`/generics for a call
/// site to actually be ambiguous about. `param_types` includes `self` as its
/// first entry, already resolved from `Self` to `Record(self_record)`.
struct MethodInfo<'a> {
    decl: &'a FunctionDecl,
    param_types: Vec<CType>,
    return_type: CType,
    c_name: String,
    self_record: String,
}

struct Codegen<'a> {
    signatures: HashMap<String, (Vec<CType>, CType)>,
    /// Record name -> its fields in declaration order. Every field type is
    /// itself already a resolved `CType` (including nested `Record(name)`
    /// references to other records — always valid as a pointer field even
    /// before that other record's own body has been emitted).
    records: HashMap<String, Vec<(String, CType)>>,
    /// Record name -> method name -> its resolved signature and body. Only
    /// ever holds inherent/trait methods on a record with no generics on
    /// either the `impl` block or the method itself; anything else (a
    /// generic method, an `impl` for a type this backend doesn't compile)
    /// is simply absent, so calling it fails with "no method" rather than
    /// miscompiling.
    methods: HashMap<String, HashMap<String, MethodInfo<'a>>>,
    record_names: HashSet<String>,
    scopes: Vec<HashMap<String, CType>>,
    temp_counter: usize,
}

impl<'a> Codegen<'a> {
    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: &str, ty: CType) {
        self.scopes.last_mut().expect("codegen scope stack must never be empty").insert(name.to_string(), ty);
    }

    fn lookup(&self, name: &str) -> Option<CType> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name).cloned())
    }

    fn next_temp(&mut self) -> String {
        let name = format!("__ostrin_tmp{}", self.temp_counter);
        self.temp_counter += 1;
        name
    }

    fn record_fields(&self, record_name: &str) -> &[(String, CType)] {
        self.records.get(record_name).map(Vec::as_slice).unwrap_or(&[])
    }

    fn field_type(&self, record_name: &str, field_name: &str) -> Option<CType> {
        self.record_fields(record_name).iter().find(|(name, _)| name == field_name).map(|(_, ty)| ty.clone())
    }

    fn gen_function_body(&mut self, f: &FunctionDecl, return_type: &CType, out: &mut String) -> Result<(), String> {
        self.gen_callable_body(&f.params, &f.body, return_type, None, out)
    }

    /// Shared by top-level functions and methods. `self_record` is `Some`
    /// only for a method body, so its `self`/`Self` params resolve to that
    /// record instead of going through the ordinary (`Self`-ignorant)
    /// `map_type`.
    fn gen_callable_body(
        &mut self,
        params: &[Param],
        body: &Block,
        return_type: &CType,
        self_record: Option<&str>,
        out: &mut String,
    ) -> Result<(), String> {
        self.push_scope();
        for param in params {
            let ty = match self_record {
                Some(record_name) => map_method_type(&param.ty, record_name, &self.record_names)?,
                None => map_type(&param.ty, &self.record_names)?,
            };
            self.define(&param.name, ty);
        }
        for stmt in &body.stmts {
            self.gen_stmt(&stmt.stmt, out)?;
        }
        match &body.tail {
            Some(e) => {
                let (code, _) = self.gen_expr(e)?;
                if *return_type == CType::Void {
                    out.push_str(&format!("    {code};\n    return;\n"));
                } else {
                    out.push_str(&format!("    return {code};\n"));
                }
            }
            None => out.push_str("    return;\n"),
        }
        self.pop_scope();
        Ok(())
    }

    /// Emits a block's statements followed by its tail expression (if any)
    /// as a plain, value-discarding expression statement. Used for
    /// `if`/`while`/`for` bodies, which never need that value — only a
    /// function's own top-level body (`gen_function_body`) turns a tail into
    /// a `return`, and only `gen_block_expr` keeps the tail's value around
    /// for a block used in expression position.
    fn gen_block_stmts(&mut self, block: &Block, out: &mut String) -> Result<(), String> {
        for stmt in &block.stmts {
            self.gen_stmt(&stmt.stmt, out)?;
        }
        if let Some(e) = &block.tail {
            let (code, _) = self.gen_expr(e)?;
            out.push_str(&format!("    {code};\n"));
        }
        Ok(())
    }

    fn gen_block_expr(&mut self, block: &Block) -> Result<(String, CType), String> {
        let mut body = String::new();
        self.push_scope();
        for stmt in &block.stmts {
            self.gen_stmt(&stmt.stmt, &mut body)?;
        }
        let (tail_code, tail_ty) = match &block.tail {
            Some(e) => self.gen_expr(e)?,
            None => ("(void)0".to_string(), CType::Void),
        };
        self.pop_scope();
        Ok((format!("({{ {body} {tail_code}; }})"), tail_ty))
    }

    fn gen_stmt(&mut self, stmt: &Stmt, out: &mut String) -> Result<(), String> {
        match stmt {
            Stmt::Binding { name, value, .. } => {
                let (code, ty) = self.gen_expr(value)?;
                out.push_str(&format!("    {} {} = {};\n", c_type_name(&ty), name, code));
                self.define(name, ty);
            }
            Stmt::Assign { name, value } => {
                let (code, ty) = self.gen_expr(value)?;
                // Ostrin has no `let` keyword: `name = value` without `mut`
                // parses as `Stmt::Assign` whether `name` already exists
                // (a plain reassignment) or not (an implicit new immutable
                // binding — see `Env::assign`'s fallback in the interpreter).
                // The parser can't tell the two apart without scope
                // tracking, so this backend re-derives it the same way the
                // interpreter does, from whether `name` is already in scope.
                if self.lookup(name).is_some() {
                    out.push_str(&format!("    {name} = {code};\n"));
                } else {
                    out.push_str(&format!("    {} {} = {};\n", c_type_name(&ty), name, code));
                    self.define(name, ty);
                }
            }
            Stmt::Return(value) => match value {
                Some(e) => {
                    let (code, _) = self.gen_expr(e)?;
                    out.push_str(&format!("    return {code};\n"));
                }
                None => out.push_str("    return;\n"),
            },
            Stmt::Break(value) => {
                if value.is_some() {
                    return Err("'break' with a value isn't supported by the native backend yet".to_string());
                }
                out.push_str("    break;\n");
            }
            Stmt::Continue => out.push_str("    continue;\n"),
            Stmt::While { cond, body } => {
                let (cond_code, _) = self.gen_expr(cond)?;
                out.push_str(&format!("    while ({cond_code}) {{\n"));
                self.push_scope();
                self.gen_block_stmts(body, out)?;
                self.pop_scope();
                out.push_str("    }\n");
            }
            Stmt::For { pattern, iter, body } => self.gen_for(pattern, iter, body, out)?,
            Stmt::FieldAssign { target, value } => {
                let Expr::FieldAccess(obj, field_name) = target.unlocated() else {
                    return Err("only 'record.field = value' assignments are supported by the native backend yet".to_string());
                };
                let (obj_code, obj_ty) = self.gen_expr(obj)?;
                let CType::Record(record_name) = &obj_ty else {
                    return Err("field assignment is only supported on records by the native backend yet".to_string());
                };
                if self.field_type(record_name, field_name).is_none() {
                    return Err(format!("record '{record_name}' has no field '{field_name}'"));
                }
                let (value_code, _) = self.gen_expr(value)?;
                out.push_str(&format!("    {obj_code}->{field_name} = {value_code};\n"));
            }
            Stmt::Expr(e) => {
                if let Expr::If(cond, then_b, else_b) = e.unlocated() {
                    let (cond_code, _) = self.gen_expr(cond)?;
                    out.push_str(&format!("    if ({cond_code}) {{\n"));
                    self.push_scope();
                    self.gen_block_stmts(then_b, out)?;
                    self.pop_scope();
                    out.push_str("    }\n");
                    if let Some(else_b) = else_b {
                        out.push_str("    else {\n");
                        self.push_scope();
                        self.gen_block_stmts(else_b, out)?;
                        self.pop_scope();
                        out.push_str("    }\n");
                    }
                } else {
                    let (code, _) = self.gen_expr(e)?;
                    out.push_str(&format!("    {code};\n"));
                }
            }
        }
        Ok(())
    }

    fn gen_for(&mut self, pattern: &str, iter: &Expr, body: &Block, out: &mut String) -> Result<(), String> {
        let Expr::Range(start, kind, end, step) = iter.unlocated() else {
            return Err("the native backend only supports 'for x in a to b' / 'a until b' ranges yet".to_string());
        };
        if step.is_some() {
            return Err("stepped ranges aren't supported by the native backend yet".to_string());
        }
        let (start_code, start_ty) = self.gen_expr(start)?;
        let (end_code, _) = self.gen_expr(end)?;
        if start_ty != CType::Int {
            return Err("the native backend only supports Int ranges in 'for' yet".to_string());
        }
        let cmp = match kind {
            RangeKind::To => "<=",
            RangeKind::Until => "<",
        };
        out.push_str(&format!("    for (int64_t {pattern} = {start_code}; {pattern} {cmp} {end_code}; {pattern}++) {{\n"));
        self.push_scope();
        self.define(pattern, CType::Int);
        self.gen_block_stmts(body, out)?;
        self.pop_scope();
        out.push_str("    }\n");
        Ok(())
    }

    fn gen_expr(&mut self, expr: &Expr) -> Result<(String, CType), String> {
        match expr.unlocated() {
            Expr::IntLiteral(v) => Ok((format!("INT64_C({v})"), CType::Int)),
            Expr::FloatLiteral(v) => Ok((format!("{v}"), CType::Float)),
            Expr::BoolLiteral(v) => Ok((if *v { "true".to_string() } else { "false".to_string() }, CType::Bool)),
            Expr::StringLiteral(s) => Ok((c_string_literal(s), CType::Str)),
            Expr::Ident(name) => {
                let ty = self
                    .lookup(name)
                    .ok_or_else(|| format!("internal error: no type recorded for '{name}' in the native backend"))?;
                Ok((name.clone(), ty))
            }
            Expr::Unary(op, inner) => {
                let (code, ty) = self.gen_expr(inner)?;
                match op {
                    UnaryOp::Neg => Ok((format!("(-{code})"), ty)),
                    UnaryOp::Not => Ok((format!("(!{code})"), CType::Bool)),
                }
            }
            Expr::Binary(op, l, r) => self.gen_binary(*op, l, r),
            Expr::Call(callee, args) => self.gen_call(callee, args),
            Expr::FieldAccess(obj, field_name) => {
                let (obj_code, obj_ty) = self.gen_expr(obj)?;
                let CType::Record(record_name) = &obj_ty else {
                    return Err("field access is only supported on records by the native backend yet (no method calls)".to_string());
                };
                let field_ty = self
                    .field_type(record_name, field_name)
                    .ok_or_else(|| format!("record '{record_name}' has no field '{field_name}'"))?;
                Ok((format!("{obj_code}->{field_name}"), field_ty))
            }
            Expr::RecordLiteral(name, fields) => self.gen_record_literal(name, fields),
            Expr::If(cond, then_b, else_b) => {
                let (cond_code, _) = self.gen_expr(cond)?;
                let (then_code, then_ty) = self.gen_block_expr(then_b)?;
                let (else_code, else_ty) = match else_b {
                    Some(b) => self.gen_block_expr(b)?,
                    None => ("({ (void)0; })".to_string(), CType::Void),
                };
                let result_ty = if then_ty == CType::Void || else_ty == CType::Void { CType::Void } else { then_ty };
                Ok((format!("({cond_code} ? {then_code} : {else_code})"), result_ty))
            }
            Expr::Block(b) => self.gen_block_expr(b),
            other => Err(format!("this expression isn't supported by the native backend yet: {other:?}")),
        }
    }

    fn gen_record_literal(&mut self, name: &str, fields: &[(String, Expr)]) -> Result<(String, CType), String> {
        if !self.records.contains_key(name) {
            return Err(format!("unknown record type '{name}'"));
        }
        let temp = self.next_temp();
        let mut body = format!(
            "{name}* {temp} = ({name}*)malloc(sizeof({name})); \
             if (!{temp}) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }} "
        );
        for (field_name, value_expr) in fields {
            if self.field_type(name, field_name).is_none() {
                return Err(format!("record '{name}' has no field '{field_name}'"));
            }
            let (value_code, _) = self.gen_expr(value_expr)?;
            body.push_str(&format!("{temp}->{field_name} = {value_code}; "));
        }
        Ok((format!("({{ {body} {temp}; }})"), CType::Record(name.to_string())))
    }

    fn gen_binary(&mut self, op: BinOp, l: &Expr, r: &Expr) -> Result<(String, CType), String> {
        let (lc, lt) = self.gen_expr(l)?;
        let (rc, rt) = self.gen_expr(r)?;
        if matches!(lt, CType::Record(_)) || matches!(rt, CType::Record(_)) {
            return Err("operators on records aren't supported by the native backend yet (no derive(Eq/Ord) dispatch)".to_string());
        }
        if lt == CType::Str || rt == CType::Str {
            return match op {
                BinOp::Add if lt == CType::Str && rt == CType::Str => Ok((format!("ostrin_str_concat({lc}, {rc})"), CType::Str)),
                BinOp::Eq if lt == CType::Str && rt == CType::Str => Ok((format!("(strcmp({lc}, {rc}) == 0)"), CType::Bool)),
                BinOp::NotEq if lt == CType::Str && rt == CType::Str => Ok((format!("(strcmp({lc}, {rc}) != 0)"), CType::Bool)),
                _ => Err("this operator isn't supported for String by the native backend yet".to_string()),
            };
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
        let result_ty = match op {
            BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq | BinOp::And | BinOp::Or => CType::Bool,
            // Arithmetic: the type checker already unified both operands, so
            // either side's type is the result.
            _ => lt,
        };
        Ok((format!("({lc} {c_op} {rc})"), result_ty))
    }

    fn gen_call(&mut self, callee: &Expr, args: &[Arg]) -> Result<(String, CType), String> {
        match callee.unlocated() {
            Expr::Ident(name) => self.gen_function_call(name, args),
            Expr::FieldAccess(obj, method_name) => self.gen_method_call(obj, method_name, args),
            _ => Err("only a direct function call or 'record.method(...)' is supported by the native backend yet".to_string()),
        }
    }

    fn gen_args(&mut self, args: &[Arg]) -> Result<(Vec<String>, Vec<CType>), String> {
        let mut codes = Vec::new();
        let mut types = Vec::new();
        for arg in args {
            let expr = match arg {
                Arg::Positional(e) => e,
                Arg::Named(_, _) => return Err("named arguments aren't supported by the native backend yet".to_string()),
            };
            let (code, ty) = self.gen_expr(expr)?;
            codes.push(code);
            types.push(ty);
        }
        Ok((codes, types))
    }

    fn gen_function_call(&mut self, name: &str, args: &[Arg]) -> Result<(String, CType), String> {
        let (arg_codes, arg_types) = self.gen_args(args)?;
        if name == "print" {
            return self.gen_print(&arg_codes, &arg_types);
        }
        let Some((param_types, return_type)) = self.signatures.get(name).cloned() else {
            return Err(format!("unknown function '{name}' (the native backend only sees other top-level 'fn' declarations)"));
        };
        if param_types.len() != arg_codes.len() {
            return Err(format!("function '{name}' expects {} argument(s), got {}", param_types.len(), arg_codes.len()));
        }
        Ok((format!("{}({})", c_function_name(name), arg_codes.join(", ")), return_type))
    }

    fn gen_method_call(&mut self, obj: &Expr, method_name: &str, args: &[Arg]) -> Result<(String, CType), String> {
        let (obj_code, obj_ty) = self.gen_expr(obj)?;
        let CType::Record(record_name) = &obj_ty else {
            return Err("method calls are only supported on records by the native backend yet".to_string());
        };
        let Some(method) = self.methods.get(record_name).and_then(|methods| methods.get(method_name)) else {
            return Err(format!(
                "record '{record_name}' has no method '{method_name}' the native backend can compile \
                 (generic methods and methods on unsupported types aren't supported yet)"
            ));
        };
        let (param_types, return_type, c_name) = (method.param_types.clone(), method.return_type.clone(), method.c_name.clone());
        let (arg_codes, arg_types) = self.gen_args(args)?;
        if param_types.len() != arg_types.len() + 1 {
            return Err(format!(
                "method '{record_name}.{method_name}' expects {} argument(s), got {}",
                param_types.len() - 1,
                arg_types.len()
            ));
        }
        let mut all_args = vec![obj_code];
        all_args.extend(arg_codes);
        Ok((format!("{c_name}({})", all_args.join(", ")), return_type))
    }

    fn gen_print(&self, arg_codes: &[String], arg_types: &[CType]) -> Result<(String, CType), String> {
        if arg_codes.len() != 1 {
            return Err("'print' expects exactly one argument".to_string());
        }
        let (spec, value) = match &arg_types[0] {
            CType::Int => ("%lld\\n", format!("(long long)({})", arg_codes[0])),
            CType::Float => ("%g\\n", arg_codes[0].clone()),
            CType::Bool => ("%s\\n", format!("(({}) ? \"true\" : \"false\")", arg_codes[0])),
            CType::Str => ("%s\\n", arg_codes[0].clone()),
            CType::Void => return Err("cannot 'print' a Void value".to_string()),
            CType::Record(name) => return Err(format!("cannot 'print' a record value ('{name}' has no derived Display)")),
        };
        Ok((format!("printf(\"{spec}\", {value})"), CType::Void))
    }
}

fn c_string_literal(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn item_name(item: &Item) -> String {
    match item {
        Item::Function(f) => format!("fn {}", f.name),
        Item::Record(r) => format!("record {}", r.name),
        Item::Enum(e) => format!("enum {}", e.name),
        Item::Impl(i) => format!("impl for {}", i.type_name),
        Item::Trait(t) => format!("trait {}", t.name),
        Item::Import(_) => "import".to_string(),
    }
}

/// Transpiles an already type-checked program to C. Top-level functions and
/// plain (non-generic) records are supported, including non-generic
/// inherent/trait methods on those records (no dynamic dispatch is needed:
/// with no `dyn Trait` and no generics anywhere in this backend, a record's
/// concrete method is always known at the call site). `enum`/`trait` make
/// this return a clear error naming the construct, rather than silently
/// ignoring it or emitting something incorrect. A generic method, or a
/// method on a type this backend doesn't otherwise compile, is simply left
/// out of the method table — calling it fails on its own in `gen_call`
/// ("no method"), rather than this function rejecting the whole program
/// up front for an `impl` block nothing may even use.
pub fn generate(items: &[Item]) -> Result<String, String> {
    let mut functions = Vec::new();
    let mut records = Vec::new();
    let mut impls = Vec::new();
    for item in items {
        match item {
            Item::Function(f) => functions.push(f),
            Item::Record(r) => records.push(r),
            Item::Impl(im) => impls.push(im),
            Item::Import(_) => {}
            other @ (Item::Enum(_) | Item::Trait(_)) => {
                return Err(format!(
                    "the native backend ('--emit-c'/'--compile') doesn't support '{}' yet — it needs the interpreter ('--run') for now",
                    item_name(other)
                ));
            }
        }
    }

    let record_names: HashSet<String> = records.iter().map(|r| r.name.clone()).collect();
    let mut codegen = Codegen {
        signatures: HashMap::new(),
        records: HashMap::new(),
        methods: HashMap::new(),
        record_names: record_names.clone(),
        scopes: vec![HashMap::new()],
        temp_counter: 0,
    };
    for r in &records {
        if !r.generics.is_empty() {
            return Err(format!("record '{}' is generic; the native backend doesn't support generics yet", r.name));
        }
        let fields = r
            .fields
            .iter()
            .map(|field| map_type(&field.ty, &record_names).map(|ty| (field.name.clone(), ty)))
            .collect::<Result<Vec<_>, _>>()?;
        codegen.records.insert(r.name.clone(), fields);
    }
    for f in &functions {
        if !f.generics.is_empty() {
            return Err(format!("function '{}' is generic; the native backend doesn't support generics yet", f.name));
        }
        let param_types = f.params.iter().map(|p| map_type(&p.ty, &record_names)).collect::<Result<Vec<_>, _>>()?;
        let return_type = map_type(&f.return_type, &record_names)?;
        codegen.signatures.insert(f.name.clone(), (param_types, return_type));
    }
    if !codegen.signatures.contains_key("main") {
        return Err("no 'main' function found".to_string());
    }

    // Only a non-generic `impl` block over a record this backend already
    // knows how to represent can have any of its methods compiled; anything
    // else is left out of the method table rather than rejected outright
    // (see the doc comment above).
    for im in &impls {
        if !im.generics.is_empty() || !record_names.contains(&im.type_name) {
            continue;
        }
        for method in &im.methods {
            if !method.generics.is_empty() {
                continue;
            }
            let param_types: Result<Vec<CType>, String> =
                method.params.iter().map(|p| map_method_type(&p.ty, &im.type_name, &record_names)).collect();
            let Ok(param_types) = param_types else { continue };
            let Ok(return_type) = map_method_type(&method.return_type, &im.type_name, &record_names) else { continue };
            let info = MethodInfo {
                decl: method,
                param_types,
                return_type,
                c_name: format!("{}__{}", im.type_name, method.name),
                self_record: im.type_name.clone(),
            };
            codegen.methods.entry(im.type_name.clone()).or_default().insert(method.name.clone(), info);
        }
    }

    let mut out = String::from(PRELUDE);
    // Forward-declare every record as an opaque typedef first: since a
    // record is always used as a pointer, its fields never need the other
    // record's full body to be visible yet, only the typedef name to exist —
    // which sidesteps ordering entirely, including two records that
    // reference each other.
    for r in &records {
        out.push_str(&format!("typedef struct {0} {0};\n", r.name));
    }
    if !records.is_empty() {
        out.push('\n');
    }
    for r in &records {
        out.push_str(&format!("struct {} {{\n", r.name));
        for (field_name, field_ty) in &codegen.records[&r.name] {
            out.push_str(&format!("    {} {};\n", c_type_name(field_ty), field_name));
        }
        out.push_str("};\n\n");
    }

    // Prototypes for everything before any body: Ostrin doesn't require
    // declaration-before-use (a function may call one written later in the
    // file, and two methods may call each other both ways), but plain C
    // does — this sidesteps ordering entirely instead of trying to
    // topologically sort call graphs.
    for f in &functions {
        let (param_types, return_type) = codegen.signatures.get(&f.name).cloned().unwrap();
        let params = render_params(&param_types, &f.params);
        out.push_str(&format!("{} {}({});\n", c_type_name(&return_type), c_function_name(&f.name), params));
    }
    for methods in codegen.methods.values() {
        for info in methods.values() {
            let params = render_params(&info.param_types, &info.decl.params);
            out.push_str(&format!("{} {}({});\n", c_type_name(&info.return_type), info.c_name, params));
        }
    }
    out.push('\n');

    for f in &functions {
        let (param_types, return_type) = codegen.signatures.get(&f.name).cloned().unwrap();
        let params = render_params(&param_types, &f.params);
        out.push_str(&format!("{} {}({}) {{\n", c_type_name(&return_type), c_function_name(&f.name), params));
        codegen.gen_function_body(f, &return_type, &mut out)?;
        out.push_str("}\n\n");
    }
    // Collected into a plain list first (one pass) so the mutable borrow
    // `gen_callable_body` needs doesn't fight the immutable one still
    // holding `info`/`decl` from `codegen.methods`.
    let method_bodies: Vec<(String, Vec<CType>, CType, String, &FunctionDecl)> = codegen
        .methods
        .values()
        .flat_map(|methods| methods.values())
        .map(|info| (info.self_record.clone(), info.param_types.clone(), info.return_type.clone(), info.c_name.clone(), info.decl))
        .collect();
    for (self_record, param_types, return_type, c_name, decl) in method_bodies {
        let params = render_params(&param_types, &decl.params);
        out.push_str(&format!("{} {}({}) {{\n", c_type_name(&return_type), c_name, params));
        codegen.gen_callable_body(&decl.params, &decl.body, &return_type, Some(&self_record), &mut out)?;
        out.push_str("}\n\n");
    }
    out.push_str("int main(void) {\n    ostrin_main();\n    return 0;\n}\n");
    Ok(out)
}

fn render_params(types: &[CType], params: &[Param]) -> String {
    if params.is_empty() {
        "void".to_string()
    } else {
        types.iter().zip(params).map(|(ty, p)| format!("{} {}", c_type_name(ty), p.name)).collect::<Vec<_>>().join(", ")
    }
}

/// Finds a GNU-C-compatible compiler to hand the generated source to.
/// `OSTRIN_CC` overrides the search; otherwise `cc`, `gcc` and `clang` are
/// tried in that order (the generated code leans on GNU statement
/// expressions, so a plain C89-only compiler — notably MSVC's `cl` — will
/// not work here).
pub fn find_c_compiler() -> Option<String> {
    if let Ok(cc) = std::env::var("OSTRIN_CC") {
        return Some(cc);
    }
    for candidate in ["cc", "gcc", "clang"] {
        let works = std::process::Command::new(candidate)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        if works {
            return Some(candidate.to_string());
        }
    }
    None
}
