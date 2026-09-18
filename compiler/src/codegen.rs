//! A real, honest native backend: `ostrinc --emit-c`/`--compile` transpile a
//! *subset* of Ostrin to C and hand it to the system's C compiler. This is
//! not the whole language — dimensional `Quantity`, closures, and
//! collections (`List<T>` and friends) still only run through the
//! interpreter (`--run`), and `dyn Trait` support stops at a standalone
//! value: `List<dyn Trait>` isn't reachable without `List` itself. What is
//! supported, all the way to a native executable — not reinterpreted, not
//! simulated: plain functions (including generic ones, monomorphized per
//! concrete instantiation — see `PendingInstance`), plain records with
//! their non-generic `impl` methods (resolved statically — a call site
//! always knows the receiver's concrete record type), plain enums with
//! `match`, and standalone `dyn Trait` values (the one place in this whole
//! backend where a call is actually resolved through a real vtable at
//! *runtime* — see `CType::DynTrait` and `PendingVTable`) — over
//! `Int`/`Float`/`Bool`/`String`, recursion, `if`/`while`/`for <range>`,
//! and the usual operators.
//!
//! The codegen does its own tiny, local type inference (see `CType`) rather
//! than reusing `typeck::Ty` directly: by the time this runs, the program
//! has already passed the real type checker, so this pass only needs to
//! know which concrete type each expression is (to pick a C type and a
//! `printf` conversion), not to validate anything — including for a generic
//! function's own type parameters, inferred fresh at each call site purely
//! from argument types (see `infer_generic_substitutions`), never from
//! `typeck`'s own (more capable) inference.
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
//! oversight. Enums are the opposite: a plain-by-value tagged union, since
//! `Value::EnumInstance` is itself deep-cloned on assignment in the
//! interpreter, i.e. already a value type there.

use std::collections::{HashMap, HashSet, VecDeque};

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
    /// A plain-C tagged union, always passed *by value* (unlike `Record`):
    /// the interpreter's `Value::EnumInstance` carries its own owned data
    /// and is deep-cloned on assignment, so it behaves like a value type,
    /// not a shared reference — see the module doc comment.
    Enum(String),
    /// A `dyn Trait` value: a fat pointer (`{ void* self; const
    /// TraitName_VTable* vtable; }`) — the one place in this whole backend
    /// where a call is actually resolved at *runtime*, through a function
    /// pointer, rather than known outright at compile time. Everything else
    /// (records' methods, generic instantiations) gets away with static
    /// resolution; a `dyn` value's whole reason to exist is that its
    /// concrete type is erased, so there is no way around a vtable here.
    DynTrait(String),
}

fn c_type_name(ty: &CType) -> String {
    match ty {
        CType::Int => "int64_t".to_string(),
        CType::Float => "double".to_string(),
        CType::Bool => "bool".to_string(),
        CType::Str => "const char*".to_string(),
        CType::Void => "void".to_string(),
        CType::Record(name) => format!("{name}*"),
        CType::Enum(name) => name.clone(),
        CType::DynTrait(name) => format!("{name}_Dyn"),
    }
}

/// Names of every plain record/enum/trait this backend has agreed to
/// represent — just enough to resolve a bare type name to the right `CType`
/// variant. Field/method type-checking already happened in `typeck`; this
/// only picks which concrete C shape a name maps to.
struct NamedTypes<'a> {
    records: &'a HashSet<String>,
    enums: &'a HashSet<String>,
    traits: &'a HashSet<String>,
}

fn map_type(ty: &Type, types: &NamedTypes) -> Result<CType, String> {
    match ty {
        Type::Named(name, args) if args.is_empty() => match name.as_str() {
            "Int" => Ok(CType::Int),
            "Float" => Ok(CType::Float),
            "Bool" => Ok(CType::Bool),
            "String" => Ok(CType::Str),
            "Void" => Ok(CType::Void),
            other if types.records.contains(other) => Ok(CType::Record(other.to_string())),
            other if types.enums.contains(other) => Ok(CType::Enum(other.to_string())),
            _ => Err(format!("type '{}' is not supported by the native backend yet", type_to_string(ty))),
        },
        Type::Dyn(traits) => {
            if traits.len() != 1 {
                return Err("'dyn A + B' (more than one trait) isn't supported by the native backend yet".to_string());
            }
            if types.traits.contains(&traits[0]) {
                Ok(CType::DynTrait(traits[0].clone()))
            } else {
                Err(format!(
                    "'dyn {}' isn't supported by the native backend yet (the trait itself, or one of its methods, uses something this backend can't represent)",
                    traits[0]
                ))
            }
        }
        _ => Err(format!("type '{}' is not supported by the native backend yet", type_to_string(ty))),
    }
}

/// Like `map_type`, but resolves a name found in `subst` before anything
/// else. Used for two distinct things that turn out to be the same
/// mechanism: a method's `self`/`Self` (`subst` holds just `"Self" ->
/// Record(...)`), and a monomorphized generic function's own type
/// parameters (`subst` holds one entry per `<T, U, ...>`, inferred at the
/// call site — see `infer_generic_substitutions`). Either way, there is no
/// dynamic dispatch or runtime type information anywhere in this backend:
/// every name in `subst` is already a concrete `CType` by the time this
/// runs, known once and for all at compile time.
fn map_type_with_subst(ty: &Type, types: &NamedTypes, subst: &HashMap<String, CType>) -> Result<CType, String> {
    if let Type::Named(name, args) = ty {
        if args.is_empty() {
            if let Some(concrete) = subst.get(name) {
                return Ok(concrete.clone());
            }
        }
    }
    map_type(ty, types)
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

/// A variant resolved at codegen time: which enum it belongs to, its `switch`
/// tag, and its fields (already `CType`-resolved) in declaration order. Named
/// fields keep their name; positional fields (`name: None` in the AST) are
/// given the synthetic names `f0`, `f1`, ... in order, both here and in the
/// generated union member.
#[derive(Clone)]
struct VariantInfo {
    enum_name: String,
    name: String,
    tag: usize,
    fields: Vec<(String, CType)>,
}

/// One concrete instantiation of a generic function, queued the first time
/// `gen_function_call` sees it called with a given set of argument types,
/// and drained (its body generated) after every non-generic function's and
/// method's body — by then, any *further* instantiations a generic body
/// itself triggers have also had a chance to be queued, so draining loops
/// until the queue is empty rather than assuming one pass suffices.
struct PendingInstance<'a> {
    c_name: String,
    decl: &'a FunctionDecl,
    subst: HashMap<String, CType>,
    param_types: Vec<CType>,
    return_type: CType,
}

struct Codegen<'a> {
    signatures: HashMap<String, (Vec<CType>, CType)>,
    /// Every generic top-level function, kept out of `signatures` (a generic
    /// function has no single concrete signature) and never emitted itself
    /// — only its instantiations, discovered on demand, are.
    generic_functions: HashMap<String, &'a FunctionDecl>,
    /// Mangled name (`identity__Int`) -> resolved signature, for every
    /// instantiation discovered so far; doubles as the dedup key so calling
    /// the same generic function with the same types twice reuses one C
    /// function instead of emitting it again.
    instantiations: HashMap<String, (Vec<CType>, CType)>,
    pending: VecDeque<PendingInstance<'a>>,
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
    /// Variant name -> its info. A flat namespace, matching the interpreter's
    /// own `variant_to_enum` map: bare variant names are unique across the
    /// whole program, never qualified by their enum.
    variants: HashMap<String, VariantInfo>,
    /// Trait name -> method name -> its *abstract* signature (param types
    /// excluding the receiver, and return type — `Self` never resolved to
    /// anything concrete here, since a trait's own declaration doesn't know
    /// which record will eventually implement it). A method is present only
    /// if `Self` never appears anywhere but as the exact `self` receiver:
    /// `map_type` fails on a bare `Self` name (it isn't in `subst`), so a
    /// signature like `fn combine(self, other: Self) -> Self` is silently
    /// left out — the same "object safety" rule Rust enforces for `dyn
    /// Trait`, arrived at for free rather than checked explicitly.
    trait_methods: HashMap<String, HashMap<String, (Vec<CType>, CType)>>,
    trait_names: HashSet<String>,
    /// (trait, record) pairs whose vtable has already been queued or
    /// emitted, so boxing the same record into the same `dyn Trait` twice
    /// reuses one vtable instead of duplicating it.
    vtables_emitted: HashSet<(String, String)>,
    pending_vtables: VecDeque<PendingVTable>,
    record_names: HashSet<String>,
    enum_names: HashSet<String>,
    scopes: Vec<HashMap<String, CType>>,
    temp_counter: usize,
}

/// A `dyn Trait` boxing site the first `coerce()` call for this exact
/// (trait, record) pair discovered — queued the same way a generic
/// instantiation is (see `PendingInstance`), and for the same reason: it's
/// only known to be needed once a record is actually seen being boxed into
/// that trait, not upfront.
struct PendingVTable {
    trait_name: String,
    record_name: String,
}

/// Every `CType` this backend knows, spelled as a valid piece of a C
/// identifier — used only to build a monomorphized function's mangled name
/// (`identity__Int`, `pair__Int_String`), never emitted as an actual type.
fn mangle_ctype(ty: &CType) -> String {
    match ty {
        CType::Int => "Int".to_string(),
        CType::Float => "Float".to_string(),
        CType::Bool => "Bool".to_string(),
        CType::Str => "String".to_string(),
        CType::Void => "Void".to_string(),
        CType::Record(name) | CType::Enum(name) | CType::DynTrait(name) => name.clone(),
    }
}

impl<'a> Codegen<'a> {
    fn named_types(&self) -> NamedTypes<'_> {
        NamedTypes { records: &self.record_names, enums: &self.enum_names, traits: &self.trait_names }
    }

    /// Converts a value from `from` to `to` where they differ — today the
    /// only conversion this backend ever needs is boxing a concrete record
    /// into a `dyn Trait` it implements, at a function call argument, a
    /// `return`/tail value, or an explicitly-typed binding. Queues that
    /// pair's vtable (see `PendingVTable`) the first time it's needed.
    fn coerce(&mut self, code: &str, from: &CType, to: &CType) -> Result<String, String> {
        if from == to {
            return Ok(code.to_string());
        }
        let (CType::Record(record_name), CType::DynTrait(trait_name)) = (from, to) else {
            return Err(format!("cannot use a value of type '{}' where '{}' was expected", c_type_name(from), c_type_name(to)));
        };
        let Some(trait_method_names) = self.trait_methods.get(trait_name).map(|methods| methods.keys().cloned().collect::<Vec<_>>()) else {
            return Err(format!("unknown trait '{trait_name}'"));
        };
        let implements = trait_method_names
            .iter()
            .all(|method_name| self.methods.get(record_name).is_some_and(|methods| methods.contains_key(method_name)));
        if !implements {
            return Err(format!(
                "record '{record_name}' doesn't implement all of trait '{trait_name}''s methods \
                 (or the native backend couldn't compile one of them)"
            ));
        }
        let key = (trait_name.clone(), record_name.clone());
        if self.vtables_emitted.insert(key) {
            self.pending_vtables.push_back(PendingVTable { trait_name: trait_name.clone(), record_name: record_name.clone() });
        }
        Ok(format!("(({trait_name}_Dyn){{ .self = (void*)({code}), .vtable = &{trait_name}__{record_name}__vtable }})"))
    }
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
        self.gen_callable_body(&f.params, &f.body, return_type, &HashMap::new(), out)
    }

    /// Shared by top-level functions, methods and generic instantiations.
    /// `subst` resolves `self`/`Self` for a method, or a generic function's
    /// own `<T, ...>` for an instantiation; it's empty for a plain function.
    fn gen_callable_body(
        &mut self,
        params: &[Param],
        body: &Block,
        return_type: &CType,
        subst: &HashMap<String, CType>,
        out: &mut String,
    ) -> Result<(), String> {
        self.push_scope();
        for param in params {
            let ty = map_type_with_subst(&param.ty, &self.named_types(), subst)?;
            self.define(&param.name, ty);
        }
        for stmt in &body.stmts {
            self.gen_stmt(&stmt.stmt, out)?;
        }
        match &body.tail {
            Some(e) => {
                let (code, ty) = self.gen_expr(e)?;
                if *return_type == CType::Void {
                    out.push_str(&format!("    {code};\n    return;\n"));
                } else {
                    let code = self.coerce(&code, &ty, return_type)?;
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
            Stmt::Binding { name, ty: declared, value, .. } => {
                let (code, actual_ty) = self.gen_expr(value)?;
                // An explicit `name: dyn Trait = ConcreteRecord { ... }`
                // needs boxing right here — with no annotation, `ty` is
                // just whatever the value already produced.
                let (final_ty, code) = match declared {
                    Some(declared_ty) => {
                        let declared_ctype = map_type(declared_ty, &self.named_types())?;
                        let coerced = self.coerce(&code, &actual_ty, &declared_ctype)?;
                        (declared_ctype, coerced)
                    }
                    None => (actual_ty, code),
                };
                out.push_str(&format!("    {} {} = {};\n", c_type_name(&final_ty), name, code));
                self.define(name, final_ty);
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
                if let Some(ty) = self.lookup(name) {
                    return Ok((name.clone(), ty));
                }
                // A unit variant (`None`, or any fieldless variant of a
                // user enum) reads as a bare identifier, never a call — see
                // `Expr::Ident` in `eval_expr`, `interpreter/mod.rs`.
                if let Some(variant) = self.variants.get(name).cloned() {
                    if !variant.fields.is_empty() {
                        return Err(format!("variant '{name}' has fields; construct it as '{name}(...)'"));
                    }
                    return Ok((
                        format!("(({}){{ .tag = {} }})", variant.enum_name, variant.tag),
                        CType::Enum(variant.enum_name),
                    ));
                }
                Err(format!("internal error: no type recorded for '{name}' in the native backend"))
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
            Expr::Match(scrutinee, arms) => self.gen_match(scrutinee, arms),
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

    /// Compiles `match` to a `({ ... })` statement expression: a hidden
    /// `matched` flag and result variable, then one `if (!matched && ...)`
    /// per arm, in source order, each setting the result and the flag on
    /// success. Falling through *without* setting the flag (a guard that
    /// evaluated false) is exactly what lets a later arm — even one with
    /// the same variant tag — still be tried, matching Ostrin's top-to-
    /// bottom arm semantics. `typeck` has already proven the match
    /// exhaustive; the final `if (!matched) abort()` only guards against a
    /// bug in this codegen itself, not a real program's possible outcomes.
    fn gen_match(&mut self, scrutinee: &Expr, arms: &[MatchArm]) -> Result<(String, CType), String> {
        if arms.is_empty() {
            return Err("'match' needs at least one arm".to_string());
        }
        let (scrutinee_code, scrutinee_ty) = self.gen_expr(scrutinee)?;
        let scrutinee_var = self.next_temp();
        let matched_var = self.next_temp();
        let result_var = self.next_temp();

        let mut arm_blocks = String::new();
        let mut result_ty: Option<CType> = None;
        for arm in arms {
            self.push_scope();
            let mut condition = format!("!{matched_var}");
            let mut bindings = String::new();
            let bind_result = self.gen_pattern(&arm.pattern, &scrutinee_var, &scrutinee_ty, &mut condition, &mut bindings);
            if let Err(error) = bind_result {
                self.pop_scope();
                return Err(error);
            }
            let guard_code = match &arm.guard {
                Some(guard_expr) => match self.gen_expr(guard_expr) {
                    Ok((code, _)) => Some(code),
                    Err(error) => {
                        self.pop_scope();
                        return Err(error);
                    }
                },
                None => None,
            };
            let body = match self.gen_block_expr(&arm.body) {
                Ok(body) => body,
                Err(error) => {
                    self.pop_scope();
                    return Err(error);
                }
            };
            self.pop_scope();
            let (body_code, body_ty) = body;
            if result_ty.is_none() {
                result_ty = Some(body_ty);
            }
            // `condition` only ever covers the structural check (tag,
            // literal, range): the guard is evaluated separately, in a
            // *nested* `if` emitted after `bindings`, because a guard can
            // reference names the pattern just bound (`n if n > 0 => ...`)
            // — folding it into `condition` would reference those C
            // variables before their own declaration even exists.
            let commit = format!("{result_var} = {body_code}; {matched_var} = 1;");
            let guarded_commit = match guard_code {
                Some(guard_code) => format!("if ({guard_code}) {{ {commit} }}"),
                None => commit,
            };
            arm_blocks.push_str(&format!("if ({condition}) {{ {bindings} {guarded_commit} }} "));
        }
        let result_ty = result_ty.expect("checked arms.is_empty() above");

        let body = format!(
            "{scrut_ty} {scrutinee_var} = {scrutinee_code}; \
             int {matched_var} = 0; \
             {result_ty_c} {result_var}; \
             {arm_blocks} \
             if (!{matched_var}) {{ fprintf(stderr, \"ostrin: non-exhaustive match at runtime\\n\"); abort(); }}",
            scrut_ty = c_type_name(&scrutinee_ty),
            result_ty_c = c_type_name(&result_ty),
        );
        Ok((format!("({{ {body} {result_var}; }})"), result_ty))
    }

    /// Appends this pattern's match condition to `condition` and any field
    /// bindings it introduces to `bindings`, defining each bound name in the
    /// current (already pushed, by the caller) scope. `scrutinee_var` is
    /// always the whole match's scrutinee, by name — the same C variable
    /// regardless of which arm is being compiled.
    fn gen_pattern(
        &mut self,
        pattern: &Pattern,
        scrutinee_var: &str,
        scrutinee_ty: &CType,
        condition: &mut String,
        bindings: &mut String,
    ) -> Result<(), String> {
        match pattern {
            Pattern::Wildcard => Ok(()),
            Pattern::Ident(name) => {
                bindings.push_str(&format!("{} {} = {}; ", c_type_name(scrutinee_ty), name, scrutinee_var));
                self.define(name, scrutinee_ty.clone());
                Ok(())
            }
            Pattern::Literal(literal) => {
                let (literal_code, literal_ty) = self.gen_expr(literal)?;
                let comparison = if literal_ty == CType::Str {
                    format!("strcmp({scrutinee_var}, {literal_code}) == 0")
                } else {
                    format!("{scrutinee_var} == {literal_code}")
                };
                condition.push_str(&format!(" && ({comparison})"));
                Ok(())
            }
            Pattern::Range(start, kind, end) => {
                let (start_code, _) = self.gen_expr(start)?;
                let (end_code, _) = self.gen_expr(end)?;
                let upper = match kind {
                    RangeKind::To => "<=",
                    RangeKind::Until => "<",
                };
                condition.push_str(&format!(" && ({scrutinee_var} >= {start_code} && {scrutinee_var} {upper} {end_code})"));
                Ok(())
            }
            Pattern::Variant(name, fields) => {
                let Some(variant) = self.variants.get(name).cloned() else {
                    return Err(format!("unknown variant '{name}' in a match pattern"));
                };
                condition.push_str(&format!(" && ({scrutinee_var}.tag == {})", variant.tag));
                // The parser resolves the short form (`Circle(radius)`) by
                // matching the pattern's field name against the declared
                // one; for a variant with unnamed (positional) fields it
                // instead falls back to position — see `pattern_field_value`
                // in `interpreter/mod.rs`, which this mirrors exactly so a
                // positional variant's fields are addressable the same way
                // from either backend.
                for (position, (field_name, sub_pattern)) in fields.iter().enumerate() {
                    let resolved = variant
                        .fields
                        .iter()
                        .find(|(declared_name, _)| declared_name == field_name)
                        .or_else(|| variant.fields.get(position))
                        .cloned();
                    let Some((actual_field_name, field_ty)) = resolved else {
                        return Err(format!("variant '{name}' has no field matching '{field_name}'"));
                    };
                    match sub_pattern {
                        Pattern::Wildcard => {}
                        Pattern::Ident(binding_name) => {
                            bindings.push_str(&format!(
                                "{} {} = {}.data.{}.{}; ",
                                c_type_name(&field_ty),
                                binding_name,
                                scrutinee_var,
                                variant.name,
                                actual_field_name
                            ));
                            self.define(binding_name, field_ty);
                        }
                        _ => {
                            return Err(
                                "nested patterns inside a variant's fields aren't supported by the native backend yet".to_string()
                            );
                        }
                    }
                }
                Ok(())
            }
        }
    }

    fn gen_binary(&mut self, op: BinOp, l: &Expr, r: &Expr) -> Result<(String, CType), String> {
        let (lc, lt) = self.gen_expr(l)?;
        let (rc, rt) = self.gen_expr(r)?;
        if matches!(lt, CType::Record(_) | CType::Enum(_) | CType::DynTrait(_)) || matches!(rt, CType::Record(_) | CType::Enum(_) | CType::DynTrait(_)) {
            // C has no `==`/`<`/etc. on struct values at all (a compile
            // error, not just the wrong answer) — but even where a raw `==`
            // on two records *would* compile (comparing their pointers), it
            // would silently mean identity, not the structural
            // `derive(Eq)`/`impl Eq` comparison Ostrin actually defines.
            return Err(
                "operators on records/enums/'dyn Trait' values aren't supported by the native backend yet (no derive(Eq/Ord) dispatch)"
                    .to_string(),
            );
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
        if let Some(variant) = self.variants.get(name).cloned() {
            let arg_codes = self.gen_variant_args(&variant, args)?;
            return self.gen_variant_construct(&variant, &arg_codes);
        }
        let (arg_codes, arg_types) = self.gen_args(args)?;
        if name == "print" {
            return self.gen_print(&arg_codes, &arg_types);
        }
        if let Some(decl) = self.generic_functions.get(name).copied() {
            return self.gen_generic_call(decl, &arg_codes, &arg_types);
        }
        let Some((param_types, return_type)) = self.signatures.get(name).cloned() else {
            return Err(format!("unknown function '{name}' (the native backend only sees other top-level 'fn' declarations)"));
        };
        if param_types.len() != arg_codes.len() {
            return Err(format!("function '{name}' expects {} argument(s), got {}", param_types.len(), arg_codes.len()));
        }
        let coerced_codes = self.coerce_args(&arg_codes, &arg_types, &param_types)?;
        Ok((format!("{}({})", c_function_name(name), coerced_codes.join(", ")), return_type))
    }

    /// Boxes each argument whose declared parameter type differs from what
    /// it actually evaluated to (in practice, only ever a record being
    /// boxed into a `dyn Trait` parameter — see `coerce`).
    fn coerce_args(&mut self, arg_codes: &[String], arg_types: &[CType], param_types: &[CType]) -> Result<Vec<String>, String> {
        arg_codes.iter().zip(arg_types.iter().zip(param_types.iter())).map(|(code, (from, to))| self.coerce(code, from, to)).collect()
    }

    /// Infers `<T, U, ...>` from the concrete types of the arguments at this
    /// call site — never from the return type, which this backend has no
    /// way to know ahead of time (no bidirectional type inference, unlike
    /// `typeck`, which already proved this call sound). Monomorphizes on
    /// first use of a given (function, concrete types) pair and reuses the
    /// same C function for later calls with the same types.
    fn gen_generic_call(&mut self, decl: &'a FunctionDecl, arg_codes: &[String], arg_types: &[CType]) -> Result<(String, CType), String> {
        if decl.params.len() != arg_codes.len() {
            return Err(format!("function '{}' expects {} argument(s), got {}", decl.name, decl.params.len(), arg_codes.len()));
        }
        let subst = infer_generic_substitutions(decl, arg_types)?;
        let mangled_suffix: Vec<String> =
            decl.generics.iter().map(|g| mangle_ctype(subst.get(&g.name).expect("checked by infer_generic_substitutions"))).collect();
        let c_name = format!("{}__{}", decl.name, mangled_suffix.join("_"));

        // `decl.params.len() == arg_codes.len()` was already checked above,
        // and every instantiation's param count always equals that, so a
        // cache hit needs no further arity check.
        if let Some((_, return_type)) = self.instantiations.get(&c_name).cloned() {
            return Ok((format!("{c_name}({})", arg_codes.join(", ")), return_type));
        }

        let types = self.named_types();
        let param_types = decl.params.iter().map(|p| map_type_with_subst(&p.ty, &types, &subst)).collect::<Result<Vec<_>, _>>()?;
        let return_type = map_type_with_subst(&decl.return_type, &types, &subst)?;
        self.instantiations.insert(c_name.clone(), (param_types.clone(), return_type.clone()));
        self.pending.push_back(PendingInstance { c_name: c_name.clone(), decl, subst, param_types, return_type: return_type.clone() });
        Ok((format!("{c_name}({})", arg_codes.join(", ")), return_type))
    }

    /// Resolves a variant constructor's arguments to its declared fields —
    /// unlike ordinary function/method calls, named arguments are common and
    /// idiomatic here (`Circle(radius: 3)`), so they're supported for
    /// construction specifically, matched by field name; positional
    /// arguments still fill in declaration order.
    fn gen_variant_args(&mut self, variant: &VariantInfo, args: &[Arg]) -> Result<Vec<String>, String> {
        let mut codes: Vec<Option<String>> = vec![None; variant.fields.len()];
        let mut next_positional = 0usize;
        for arg in args {
            match arg {
                Arg::Positional(expr) => {
                    if next_positional >= variant.fields.len() {
                        return Err(format!(
                            "variant '{}' expects {} argument(s), got more",
                            variant.name,
                            variant.fields.len()
                        ));
                    }
                    let (code, _) = self.gen_expr(expr)?;
                    codes[next_positional] = Some(code);
                    next_positional += 1;
                }
                Arg::Named(field_name, expr) => {
                    let Some(index) = variant.fields.iter().position(|(name, _)| name == field_name) else {
                        return Err(format!("variant '{}' has no field '{field_name}'", variant.name));
                    };
                    let (code, _) = self.gen_expr(expr)?;
                    codes[index] = Some(code);
                }
            }
        }
        codes
            .into_iter()
            .enumerate()
            .map(|(index, code)| {
                code.ok_or_else(|| format!("variant '{}' is missing argument for field '{}'", variant.name, variant.fields[index].0))
            })
            .collect()
    }

    /// Builds a variant instance with a C99 designated initializer
    /// (`.tag = ..., .data.VariantName = { .field = ... }`) — args are
    /// positional (named arguments are rejected earlier, in `gen_args`),
    /// matched to the variant's fields in declaration order.
    fn gen_variant_construct(&self, variant: &VariantInfo, arg_codes: &[String]) -> Result<(String, CType), String> {
        if variant.fields.len() != arg_codes.len() {
            return Err(format!(
                "variant '{}' expects {} argument(s), got {}",
                variant.name,
                variant.fields.len(),
                arg_codes.len()
            ));
        }
        if variant.fields.is_empty() {
            return Ok((
                format!("(({}){{ .tag = {} }})", variant.enum_name, variant.tag),
                CType::Enum(variant.enum_name.clone()),
            ));
        }
        let inits: Vec<String> =
            variant.fields.iter().zip(arg_codes).map(|((field_name, _), code)| format!(".{field_name} = {code}")).collect();
        Ok((
            format!("(({}){{ .tag = {}, .data.{} = {{ {} }} }})", variant.enum_name, variant.tag, variant.name, inits.join(", ")),
            CType::Enum(variant.enum_name.clone()),
        ))
    }

    fn gen_method_call(&mut self, obj: &Expr, method_name: &str, args: &[Arg]) -> Result<(String, CType), String> {
        let (obj_code, obj_ty) = self.gen_expr(obj)?;
        match &obj_ty {
            CType::Record(record_name) => {
                let Some(method) = self.methods.get(record_name).and_then(|methods| methods.get(method_name)) else {
                    return Err(format!(
                        "record '{record_name}' has no method '{method_name}' the native backend can compile \
                         (generic methods and methods on unsupported types aren't supported yet)"
                    ));
                };
                let (param_types, return_type, c_name) =
                    (method.param_types.clone(), method.return_type.clone(), method.c_name.clone());
                let (arg_codes, arg_types) = self.gen_args(args)?;
                if param_types.len() != arg_types.len() + 1 {
                    return Err(format!(
                        "method '{record_name}.{method_name}' expects {} argument(s), got {}",
                        param_types.len() - 1,
                        arg_types.len()
                    ));
                }
                let coerced = self.coerce_args(&arg_codes, &arg_types, &param_types[1..])?;
                let mut all_args = vec![obj_code];
                all_args.extend(coerced);
                Ok((format!("{c_name}({})", all_args.join(", ")), return_type))
            }
            CType::DynTrait(trait_name) => {
                let Some((param_types, return_type)) = self.trait_methods.get(trait_name).and_then(|m| m.get(method_name)).cloned()
                else {
                    return Err(format!("trait '{trait_name}' has no method '{method_name}' the native backend can dispatch through 'dyn'"));
                };
                let (arg_codes, arg_types) = self.gen_args(args)?;
                if param_types.len() != arg_types.len() {
                    return Err(format!(
                        "method '{trait_name}.{method_name}' expects {} argument(s), got {}",
                        param_types.len(),
                        arg_types.len()
                    ));
                }
                let coerced = self.coerce_args(&arg_codes, &arg_types, &param_types)?;
                // The receiver expression is only evaluated once, into a
                // temporary: it may not be a bare variable (e.g. a freshly
                // boxed record literal), and both `.self` and `.vtable` are
                // needed from it.
                let temp = self.next_temp();
                let call_args = if coerced.is_empty() {
                    format!("{temp}.self")
                } else {
                    format!("{temp}.self, {}", coerced.join(", "))
                };
                let call = format!("({{ {trait_name}_Dyn {temp} = {obj_code}; {temp}.vtable->{method_name}({call_args}); }})");
                Ok((call, return_type))
            }
            _ => Err("method calls are only supported on records or 'dyn Trait' values by the native backend yet".to_string()),
        }
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
            CType::Enum(name) => return Err(format!("cannot 'print' an enum value yet ('{name}' has no generated Display)")),
            CType::DynTrait(name) => return Err(format!("cannot 'print' a 'dyn {name}' value")),
        };
        Ok((format!("printf(\"{spec}\", {value})"), CType::Void))
    }
}

/// Infers a generic function's `<T, U, ...>` bindings purely from its
/// parameters' concrete argument types at one call site — the return type
/// is never consulted, since nothing upstream of this call is expecting a
/// particular result type to unify against (that already happened, in
/// `typeck`, before this backend ever runs). A generic parameter used only
/// in the return type (or nested inside another generic type, e.g. a
/// hypothetical `List<T>` parameter — collections aren't supported at all
/// here) can't be inferred this way and is reported clearly rather than
/// silently guessed.
fn infer_generic_substitutions(decl: &FunctionDecl, arg_types: &[CType]) -> Result<HashMap<String, CType>, String> {
    let generic_names: HashSet<&str> = decl.generics.iter().map(|g| g.name.as_str()).collect();
    let mut subst: HashMap<String, CType> = HashMap::new();
    for (param, arg_ty) in decl.params.iter().zip(arg_types) {
        if let Type::Named(name, args) = &param.ty {
            if args.is_empty() && generic_names.contains(name.as_str()) {
                if let Some(existing) = subst.get(name) {
                    if existing != arg_ty {
                        return Err(format!(
                            "generic parameter '{name}' of function '{}' would need to be both '{}' and '{}' for these arguments",
                            decl.name,
                            mangle_ctype(existing),
                            mangle_ctype(arg_ty)
                        ));
                    }
                } else {
                    subst.insert(name.clone(), arg_ty.clone());
                }
            }
        }
    }
    for generic in &decl.generics {
        if !subst.contains_key(&generic.name) {
            return Err(format!(
                "cannot infer generic parameter '{}' of function '{}' from its arguments (only used in the return type, \
                 or nested inside another type — neither is supported by the native backend yet)",
                generic.name, decl.name
            ));
        }
    }
    Ok(subst)
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

/// Transpiles an already type-checked program to C. Top-level functions
/// (generic ones monomorphized per call site — see `PendingInstance`),
/// plain (non-generic) records with their non-generic `impl` methods
/// (resolved statically — a record method call always knows its concrete
/// target), plain (non-generic) enums with `match`, and standalone `dyn
/// Trait` values (dispatched through a real vtable — see `PendingVTable`)
/// are all supported. A non-generic trait whose methods are all "object
/// safe" (`Self` never appears anywhere but as the exact `self` receiver —
/// checked implicitly, not as a separate pass: see the `trait_methods`
/// field doc comment) gets a vtable/fat-pointer type declared eagerly;
/// nothing about a trait forces the whole program to be rejected on its
/// own. A generic *method*, a generic *record* or *enum*, a trait method
/// that isn't object-safe, or an `impl` for a type this backend doesn't
/// otherwise compile, is left out of its respective table rather than
/// rejecting the whole program up front — only an actual, unsupported use
/// (a call, a match arm, a boxing site) fails on its own.
pub fn generate(items: &[Item]) -> Result<String, String> {
    let mut functions = Vec::new();
    let mut records = Vec::new();
    let mut enums = Vec::new();
    let mut impls = Vec::new();
    let mut traits = Vec::new();
    for item in items {
        match item {
            Item::Function(f) => functions.push(f),
            Item::Record(r) => records.push(r),
            Item::Enum(e) => enums.push(e),
            Item::Impl(im) => impls.push(im),
            Item::Trait(t) => traits.push(t),
            Item::Import(_) => {}
        }
    }

    let record_names: HashSet<String> = records.iter().map(|r| r.name.clone()).collect();
    let enum_names: HashSet<String> = enums.iter().map(|e| e.name.clone()).collect();
    let trait_names: HashSet<String> = traits.iter().map(|t| t.name.clone()).collect();
    let mut codegen = Codegen {
        signatures: HashMap::new(),
        generic_functions: HashMap::new(),
        instantiations: HashMap::new(),
        pending: VecDeque::new(),
        records: HashMap::new(),
        methods: HashMap::new(),
        variants: HashMap::new(),
        trait_methods: HashMap::new(),
        trait_names,
        vtables_emitted: HashSet::new(),
        pending_vtables: VecDeque::new(),
        record_names: record_names.clone(),
        enum_names: enum_names.clone(),
        scopes: vec![HashMap::new()],
        temp_counter: 0,
    };
    // A trait method is only ever object-safe here if `Self` never appears
    // anywhere but as the exact `self` receiver — see the field doc comment
    // on `trait_methods`. Generic traits/methods are skipped entirely (a
    // `dyn` value has no type parameters of its own to carry).
    for t in &traits {
        if !t.generics.is_empty() {
            continue;
        }
        let mut methods = HashMap::new();
        for method in &t.methods {
            if !method.generics.is_empty() {
                continue;
            }
            let Some((receiver, rest)) = method.params.split_first() else { continue };
            if receiver.name != "self" {
                continue;
            }
            let Ok(param_types) = rest.iter().map(|p| map_type(&p.ty, &codegen.named_types())).collect::<Result<Vec<_>, _>>() else {
                continue;
            };
            let Ok(return_type) = map_type(&method.return_type, &codegen.named_types()) else { continue };
            methods.insert(method.name.clone(), (param_types, return_type));
        }
        codegen.trait_methods.insert(t.name.clone(), methods);
    }
    for r in &records {
        if !r.generics.is_empty() {
            return Err(format!("record '{}' is generic; the native backend doesn't support generics yet", r.name));
        }
        let fields = r
            .fields
            .iter()
            .map(|field| map_type(&field.ty, &codegen.named_types()).map(|ty| (field.name.clone(), ty)))
            .collect::<Result<Vec<_>, _>>()?;
        codegen.records.insert(r.name.clone(), fields);
    }
    for e in &enums {
        if !e.generics.is_empty() {
            return Err(format!("enum '{}' is generic; the native backend doesn't support generics yet", e.name));
        }
        for (tag, variant) in e.variants.iter().enumerate() {
            let fields = variant
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let field_name = field.name.clone().unwrap_or_else(|| format!("f{index}"));
                    map_type(&field.ty, &codegen.named_types()).map(|ty| (field_name, ty))
                })
                .collect::<Result<Vec<_>, _>>()?;
            codegen.variants.insert(variant.name.clone(), VariantInfo { enum_name: e.name.clone(), name: variant.name.clone(), tag, fields });
        }
    }
    // A generic function has no single concrete signature to register up
    // front — it's kept aside and only monomorphized, on demand, the first
    // time `gen_generic_call` sees it invoked with a particular set of
    // concrete argument types (see `PendingInstance`).
    for f in &functions {
        if !f.generics.is_empty() {
            codegen.generic_functions.insert(f.name.clone(), f);
            continue;
        }
        let param_types = f.params.iter().map(|p| map_type(&p.ty, &codegen.named_types())).collect::<Result<Vec<_>, _>>()?;
        let return_type = map_type(&f.return_type, &codegen.named_types())?;
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
            let self_subst: HashMap<String, CType> = HashMap::from([("Self".to_string(), CType::Record(im.type_name.clone()))]);
            let param_types: Result<Vec<CType>, String> =
                method.params.iter().map(|p| map_type_with_subst(&p.ty, &codegen.named_types(), &self_subst)).collect();
            let Ok(param_types) = param_types else { continue };
            let Ok(return_type) = map_type_with_subst(&method.return_type, &codegen.named_types(), &self_subst) else { continue };
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

    // Enums are tagged unions passed by value (never by pointer, see the
    // module doc comment), so — unlike records — an enum containing another
    // enum as a direct field genuinely needs that other enum's *complete*
    // body already emitted; declared in source order is good enough for
    // every case except that one, which is left as a known, undocumented-
    // in-code gap (it would surface as an opaque C compiler error, not a
    // silently wrong program).
    for e in &enums {
        out.push_str(&format!("typedef struct {0} {0};\n", e.name));
    }
    if !enums.is_empty() {
        out.push('\n');
    }
    for e in &enums {
        out.push_str(&format!("struct {} {{\n    int tag;\n", e.name));
        let has_fields = e.variants.iter().any(|variant| !variant.fields.is_empty());
        if has_fields {
            out.push_str("    union {\n");
            for variant in &e.variants {
                let info = &codegen.variants[&variant.name];
                if info.fields.is_empty() {
                    continue;
                }
                out.push_str(&format!("        struct {{\n"));
                for (field_name, field_ty) in &info.fields {
                    out.push_str(&format!("            {} {};\n", c_type_name(field_ty), field_name));
                }
                out.push_str(&format!("        }} {};\n", variant.name));
            }
            out.push_str("    } data;\n");
        }
        out.push_str("};\n\n");
    }

    // Every trait's vtable and fat-pointer types are declared eagerly (they
    // are cheap, and needed to even *state* a `dyn Trait` parameter's type);
    // only the actual vtable *instances* for a given (trait, record) pair
    // are lazy — see `PendingVTable` and the drain loop below.
    for (trait_name, methods) in &codegen.trait_methods {
        out.push_str(&format!("typedef struct {{\n"));
        for (method_name, (param_types, return_type)) in methods {
            let params = std::iter::once("void*".to_string()).chain(param_types.iter().map(c_type_name)).collect::<Vec<_>>().join(", ");
            out.push_str(&format!("    {} (*{})({});\n", c_type_name(return_type), method_name, params));
        }
        out.push_str(&format!("}} {trait_name}_VTable;\n\n"));
        out.push_str(&format!("typedef struct {{ void* self; const {trait_name}_VTable* vtable; }} {trait_name}_Dyn;\n\n"));
    }

    // Bodies are generated *before* any prototype is written out, because a
    // generic function's instantiations aren't known until something is
    // actually seen calling them — which only happens while generating a
    // body. Draining `codegen.pending` in a loop (an instantiation's own
    // body can call another generic function for the first time, queuing
    // yet another instantiation) means every prototype below is emitted
    // with the complete picture, so an earlier-in-file function calling a
    // later-discovered instantiation still compiles: C requires the
    // prototype before use, not the body.
    let mut bodies: Vec<(String, String)> = Vec::new(); // (signature, body)
    for f in &functions {
        if !f.generics.is_empty() {
            continue;
        }
        let (param_types, return_type) = codegen.signatures.get(&f.name).cloned().unwrap();
        let params = render_params(&param_types, &f.params);
        let signature = format!("{} {}({})", c_type_name(&return_type), c_function_name(&f.name), params);
        let mut body = String::new();
        codegen.gen_function_body(f, &return_type, &mut body)?;
        bodies.push((signature, body));
    }
    // Collected into a plain list first (one pass) so the mutable borrow
    // `gen_callable_body` needs doesn't fight the immutable one still
    // holding `info`/`decl` from `codegen.methods`.
    let method_infos: Vec<(String, Vec<CType>, CType, String, &FunctionDecl)> = codegen
        .methods
        .values()
        .flat_map(|methods| methods.values())
        .map(|info| (info.self_record.clone(), info.param_types.clone(), info.return_type.clone(), info.c_name.clone(), info.decl))
        .collect();
    for (self_record, param_types, return_type, c_name, decl) in method_infos {
        let params = render_params(&param_types, &decl.params);
        let signature = format!("{} {}({})", c_type_name(&return_type), c_name, params);
        let self_subst: HashMap<String, CType> = HashMap::from([("Self".to_string(), CType::Record(self_record))]);
        let mut body = String::new();
        codegen.gen_callable_body(&decl.params, &decl.body, &return_type, &self_subst, &mut body)?;
        bodies.push((signature, body));
    }
    // A generic instantiation's body can call another generic function (or
    // box a record into a `dyn Trait`) for the first time, and a `dyn`
    // boxing needs no further discovery of its own (a thunk's body is just
    // a one-line forwarding call) — but draining both queues in a loop,
    // rather than assuming one pass each suffices, costs nothing and keeps
    // that ordering assumption from ever mattering.
    let mut thunk_prototypes: Vec<String> = Vec::new();
    let mut vtable_defs: Vec<String> = Vec::new();
    loop {
        let mut progressed = false;
        while let Some(job) = codegen.pending.pop_front() {
            progressed = true;
            let params = render_params(&job.param_types, &job.decl.params);
            let signature = format!("{} {}({})", c_type_name(&job.return_type), job.c_name, params);
            let mut body = String::new();
            codegen.gen_callable_body(&job.decl.params, &job.decl.body, &job.return_type, &job.subst, &mut body)?;
            bodies.push((signature, body));
        }
        while let Some(PendingVTable { trait_name, record_name }) = codegen.pending_vtables.pop_front() {
            progressed = true;
            let trait_method_sigs = codegen.trait_methods[&trait_name].clone();
            let mut entries = Vec::new();
            for (method_name, (param_types, return_type)) in &trait_method_sigs {
                let record_method_c_name = codegen.methods[&record_name][method_name].c_name.clone();
                let thunk_name = format!("{trait_name}__{record_name}__{method_name}");
                let param_list = std::iter::once("void* self".to_string())
                    .chain(param_types.iter().enumerate().map(|(index, ty)| format!("{} arg{index}", c_type_name(ty))))
                    .collect::<Vec<_>>()
                    .join(", ");
                let signature = format!("static {} {thunk_name}({param_list})", c_type_name(return_type));
                let call_args = std::iter::once(format!("({record_name}*)self"))
                    .chain((0..param_types.len()).map(|index| format!("arg{index}")))
                    .collect::<Vec<_>>()
                    .join(", ");
                thunk_prototypes.push(format!("{signature};"));
                bodies.push((signature, format!("    return {record_method_c_name}({call_args});\n")));
                entries.push(format!(".{method_name} = {thunk_name}"));
            }
            vtable_defs.push(format!(
                "static const {trait_name}_VTable {trait_name}__{record_name}__vtable = {{ {} }};",
                entries.join(", ")
            ));
        }
        if !progressed {
            break;
        }
    }

    for f in &functions {
        if f.generics.is_empty() {
            let (param_types, return_type) = codegen.signatures.get(&f.name).cloned().unwrap();
            let params = render_params(&param_types, &f.params);
            out.push_str(&format!("{} {}({});\n", c_type_name(&return_type), c_function_name(&f.name), params));
        }
    }
    for methods in codegen.methods.values() {
        for info in methods.values() {
            let params = render_params(&info.param_types, &info.decl.params);
            out.push_str(&format!("{} {}({});\n", c_type_name(&info.return_type), info.c_name, params));
        }
    }
    for (c_name, (param_types, return_type)) in &codegen.instantiations {
        out.push_str(&format!("{} {}({});\n", c_type_name(return_type), c_name, render_params_by_type(param_types)));
    }
    for prototype in &thunk_prototypes {
        out.push_str(prototype);
        out.push('\n');
    }
    out.push('\n');
    // Vtable instances must be fully defined — not just declared — before
    // any function body that references one by name (`ostrin_main` itself
    // is often the very first place a record gets boxed), so they're
    // written out here, right after every thunk's prototype exists, rather
    // than alongside the function bodies that come next.
    for vtable_def in &vtable_defs {
        out.push_str(vtable_def);
        out.push_str("\n\n");
    }

    for (signature, body) in bodies {
        out.push_str(&format!("{signature} {{\n{body}}}\n\n"));
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

/// Like `render_params`, but for a standalone prototype with no `Param`
/// list at hand (used for a generic instantiation's forward declaration,
/// built straight from `codegen.instantiations`) — C doesn't require
/// parameter names in a prototype, only their types.
fn render_params_by_type(types: &[CType]) -> String {
    if types.is_empty() {
        "void".to_string()
    } else {
        types.iter().map(c_type_name).collect::<Vec<_>>().join(", ")
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
