//! A real, honest native backend: `ostrinc --emit-c`/`--compile` transpile a
//! *subset* of Ostrin to C and hand it to the system's C compiler. This is
//! not the whole language — dimensional `Quantity` and closures still only
//! run through the interpreter (`--run`), and without closures, `List`'s
//! own combinators (`map`/`filter`/`fold`/`find`/`any`/`all`, all of which
//! take a function) stay out of reach too — only `length`/`push`/
//! `remove_at`, indexing and `for x in list` are supported. What is
//! supported, all the way to a native executable — not reinterpreted, not
//! simulated: plain functions (including generic ones, monomorphized per
//! concrete instantiation — see `PendingInstance`), plain records with
//! their non-generic `impl` methods (resolved statically — a call site
//! always knows the receiver's concrete record type), plain enums with
//! `match`, `dyn Trait` values (the one place in this whole backend where a
//! call is actually resolved through a real vtable at *runtime* — see
//! `CType::DynTrait` and `PendingVTable`), and `List<T>` (heap-allocated,
//! by reference, monomorphized per element type exactly like a generic
//! function — see `Codegen::ensure_list`) — over
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
use crate::types::{dim_div, dim_is_dimensionless, dim_mul, dim_pow, dim_single, dim_to_string, resolve_unit_expr, Dimension};

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
    /// A `List<T>`, heap-allocated and always by reference — like `Record`,
    /// not like `Enum` — to match `Value::List`'s own `Rc<RefCell<...>>`
    /// reference identity in the interpreter (two bindings sharing a list
    /// must see each other's `push`). Monomorphized per element type the
    /// same way a generic function is: see `Codegen::ensure_list`.
    List(Box<CType>),
    /// `Option<T>`: a by-value `{ bool has; T value; }`, monomorphized per
    /// `T` (see `Codegen::ensure_option`). Option isn't a declared enum in
    /// the AST (the interpreter registers it as built-in), so it can't go
    /// through the user-enum path.
    Option(Box<CType>),
    /// The type of a bare `None`, which by itself carries no `T`: it only
    /// becomes a concrete `Option<T>` when `coerce` meets an expected type.
    NoneLit,
    /// A physical quantity: its *dimension* is part of the static type (as in
    /// `typeck`), its *unit* is a runtime string carried in the value
    /// (`Qty { double v; const char* u; }`), exactly like `Value::Quantity`
    /// in the interpreter — so `5 nm + 2 m` and function arguments in mixed
    /// units behave identically in both backends without monomorphizing on
    /// units.
    Quantity(Dimension),
    /// `Result<T, E>`: by-value `{ bool ok; T value; E error; }`, monomorphized per (T, E).
    Result(Box<CType>, Box<CType>),
    /// A bare `Ok(x)` / `Err(e)` knows only one side of its `Result`; like
    /// `NoneLit`, `coerce` completes it once an expected type is known.
    OkLit(Box<CType>),
    ErrLit(Box<CType>),
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
        CType::List(elem) => format!("{}*", list_struct_name(elem)),
        CType::Option(inner) => format!("Option_{}", mangle_ctype(inner)),
        CType::NoneLit | CType::OkLit(_) | CType::ErrLit(_) => "int".to_string(),
        CType::Quantity(_) => "Qty".to_string(),
        CType::Result(t, e) => format!("Result_{}_{}", mangle_ctype(t), mangle_ctype(e)),
    }
}

/// The mangled struct name for a `List` of this element type
/// (`List_Int`, `List_Circle`, ...) — pure name construction, used both by
/// `c_type_name` (which never needs `&mut self`) and `ensure_list` (which
/// does, to queue the instantiation).
fn list_struct_name(elem: &CType) -> String {
    format!("List_{}", mangle_ctype(elem))
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

/// Resolves a dimension expression (`Length`, `Length / Time`, `D`, ...) the
/// same way `typeck` does, with generic `D`s taken from `subst`.
fn resolve_dimension(ty: &Type, subst: &HashMap<String, Dimension>) -> Dimension {
    match ty {
        Type::Named(name, _) => subst.get(name).cloned().unwrap_or_else(|| dim_single(name)),
        Type::Mul(a, b) => dim_mul(&resolve_dimension(a, subst), &resolve_dimension(b, subst)),
        Type::Div(a, b) => dim_div(&resolve_dimension(a, subst), &resolve_dimension(b, subst)),
        Type::Pow(a, n) => dim_pow(&resolve_dimension(a, subst), *n as i32),
        _ => HashMap::new(),
    }
}

fn map_type(ty: &Type, types: &NamedTypes) -> Result<CType, String> {
    match ty {
        Type::Named(name, args) if name == "Quantity" && args.len() == 1 => Ok(CType::Quantity(resolve_dimension(&args[0], &HashMap::new()))),
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
        Type::Named(name, args) if name == "Result" && args.len() == 2 => {
            Ok(CType::Result(Box::new(map_type(&args[0], types)?), Box::new(map_type(&args[1], types)?)))
        }
        Type::Named(name, args) if name == "Option" && args.len() == 1 => Ok(CType::Option(Box::new(map_type(&args[0], types)?))),
        Type::Named(name, args) if name == "List" && args.len() == 1 => Ok(CType::List(Box::new(map_type(&args[0], types)?))),
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
        if name == "Quantity" && args.len() == 1 {
            let dims: HashMap<String, Dimension> = subst
                .iter()
                .filter_map(|(k, v)| if let CType::Quantity(d) = v { Some((k.clone(), d.clone())) } else { None })
                .collect();
            return Ok(CType::Quantity(resolve_dimension(&args[0], &dims)));
        }
    }
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

/// Quantity runtime (unit table, conversion, arithmetic helpers), spliced in
/// right after `PRELUDE` only when a program actually uses `Qty`.
const QTY_RUNTIME: &str = include_str!("qty_runtime.c");

const PRELUDE: &str = "#include <stdint.h>\n\
#include <stdbool.h>\n\
#include <stdio.h>\n\
#include <stdlib.h>\n\
#include <string.h>\n\
\n\
static int64_t ostrin_idiv(int64_t a, int64_t b) {\n\
    if (b == 0) { fprintf(stderr, \"runtime error: division by zero\\n\"); exit(1); }\n\
    return a / b;\n\
}\n\
\n\
static void ostrin_fmt_double(double v, char* buf, size_t n) {\n\
    int prec;\n\
    for (prec = 1; prec <= 17; prec++) {\n\
        snprintf(buf, n, \"%.*g\", prec, v);\n\
        if (strtod(buf, NULL) == v) break;\n\
    }\n\
    if (strchr(buf, 'e')) {\n\
        char t[64];\n\
        snprintf(t, sizeof t, \"%.*e\", prec - 1, v);\n\
        int decimals = prec - 1 - atoi(strchr(t, 'e') + 1);\n\
        snprintf(buf, n, \"%.*f\", decimals < 0 ? 0 : decimals, v);\n\
    }\n\
}\n\
\n\
static const char* ostrin_int_to_string(int64_t v) {\n\
    char* out = (char*)malloc(32);\n\
    snprintf(out, 32, \"%lld\", (long long)v);\n\
    return out;\n\
}\n\
\n\
static const char* ostrin_float_to_string(double v) {\n\
    char* out = (char*)malloc(64);\n\
    ostrin_fmt_double(v, out, 64);\n\
    return out;\n\
}\n\
\n\
static void ostrin_print_float(double v) {\n\
    char buf[64];\n\
    ostrin_fmt_double(v, buf, sizeof buf);\n\
    printf(\"%s\\n\", buf);\n\
}\n\
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
/// first entry, already resolved from `Self` to `Record(..)`/`Enum(..)` (methods work on both).
struct MethodInfo<'a> {
    decl: &'a FunctionDecl,
    param_types: Vec<CType>,
    return_type: CType,
    c_name: String,
    self_ty: CType,
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
    /// Every `List` element type discovered so far, keyed by its mangled
    /// struct name (`List_Int`) — the dedup key, same idea as
    /// `instantiations`: a second `List<Int>` reuses the first one's struct
    /// and helper functions instead of generating them again.
    list_instantiations: HashMap<String, CType>,
    pending_lists: VecDeque<CType>,
    option_instantiations: HashSet<String>,
    pending_options: VecDeque<CType>,
    result_instantiations: HashSet<String>,
    pending_results: VecDeque<(CType, CType)>,
    current_return: Vec<CType>,
    lambda_depth: usize,
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
        CType::List(elem) => format!("List_{}", mangle_ctype(elem)),
        CType::Option(inner) => format!("Option_{}", mangle_ctype(inner)),
        CType::NoneLit => "None".to_string(),
        CType::Quantity(d) => format!("Q_{}", dim_to_string(d).chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect::<String>()),
        CType::OkLit(t) => format!("Ok_{}", mangle_ctype(t)),
        CType::ErrLit(t) => format!("Err_{}", mangle_ctype(t)),
        CType::Result(t, e) => format!("Result_{}_{}", mangle_ctype(t), mangle_ctype(e)),
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
        if let (CType::NoneLit, CType::Option(inner)) = (from, to) {
            self.register_list_types(to);
            return Ok(format!("(({}){{ .has = false }})", c_type_name(&CType::Option(inner.clone()))));
        }
        if let (CType::OkLit(_), CType::Result(..)) = (from, to) {
            self.register_list_types(to);
            return Ok(format!("(({}){{ .ok = true, .value = {code} }})", c_type_name(to)));
        }
        if let (CType::ErrLit(_), CType::Result(..)) = (from, to) {
            self.register_list_types(to);
            return Ok(format!("(({}){{ .ok = false, .error = {code} }})", c_type_name(to)));
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

    /// Registers (if new) the struct + helper functions a `List` of this
    /// element type needs, and returns its mangled struct name. Called
    /// wherever a list is actually constructed, indexed, iterated or has a
    /// method called on it — never from `map_type`, which only builds the
    /// `CType` shape and has no `&mut self` to queue anything with.
    fn ensure_list(&mut self, elem: &CType) -> String {
        let name = list_struct_name(elem);
        if !self.list_instantiations.contains_key(&name) {
            self.list_instantiations.insert(name.clone(), elem.clone());
            self.pending_lists.push_back(elem.clone());
        }
        name
    }

    /// Walks a resolved `CType` and calls `ensure_list` on every `List`
    /// found in it (including nested ones, `List<List<Int>>`). Signatures
    /// (function/method params and return types) are resolved with plain
    /// `map_type`, which never touches `&mut self` — this is the one place
    /// a signature-only `List<T>` (never itself constructed, just forwarded
    /// from a parameter to a return value, say) still gets its struct and
    /// helpers queued, so its type actually exists in the generated C.
    fn ensure_option(&mut self, inner: &CType) -> String {
        let name = format!("Option_{}", mangle_ctype(inner));
        if self.option_instantiations.insert(name.clone()) {
            self.pending_options.push_back(inner.clone());
        }
        name
    }

    fn ensure_result(&mut self, ok: &CType, err: &CType) {
        let name = format!("Result_{}_{}", mangle_ctype(ok), mangle_ctype(err));
        if self.result_instantiations.insert(name) {
            self.pending_results.push_back((ok.clone(), err.clone()));
        }
    }

    fn register_list_types(&mut self, ty: &CType) {
        if let CType::Result(ok, err) = ty {
            self.register_list_types(ok);
            self.register_list_types(err);
            self.ensure_result(ok, err);
        }
        if let CType::Option(inner) = ty {
            self.register_list_types(inner);
            self.ensure_option(inner);
        }
        if let CType::List(elem) = ty {
            self.register_list_types(elem);
            self.ensure_list(elem);
        }
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
        self.current_return.push(return_type.clone());
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
        self.current_return.pop();
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
                // A list literal bound to an explicit `List<dyn Trait>` needs
                // its elements boxed one by one, so it must know the element
                // type it is expected to produce before generating them.
                let expected_elem = match (declared, value.unlocated()) {
                    (Some(declared_ty), Expr::ListLiteral(_)) => match map_type(declared_ty, &self.named_types()) {
                        Ok(CType::List(elem)) => Some(*elem),
                        _ => None,
                    },
                    _ => None,
                };
                let (code, actual_ty) = match (&expected_elem, value.unlocated()) {
                    (Some(elem), Expr::ListLiteral(items)) => self.gen_list_literal(items, Some(elem))?,
                    _ => self.gen_expr(value)?,
                };
                // An explicit `name: dyn Trait = ConcreteRecord { ... }`
                // needs boxing right here — with no annotation, `ty` is
                // just whatever the value already produced.
                let (final_ty, code) = match declared {
                    Some(declared_ty) => {
                        let declared_ctype = map_type(declared_ty, &self.named_types())?;
                        self.register_list_types(&declared_ctype);
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
                    let (code, ty) = self.gen_expr(e)?;
                    let code = match self.current_return.last().cloned() {
                        Some(expected) => self.coerce(&code, &ty, &expected)?,
                        None => code,
                    };
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
        if let Expr::Range(start, kind, end, step) = iter.unlocated() {
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
            return Ok(());
        }

        let (iter_code, iter_ty) = self.gen_expr(iter)?;
        let CType::List(elem_ty) = iter_ty else {
            return Err("the native backend only supports 'for x in a to b' ranges or a List yet".to_string());
        };
        let elem_ty = *elem_ty;
        let list_type_name = c_type_name(&CType::List(Box::new(elem_ty.clone())));
        let list_temp = self.next_temp();
        let index_temp = self.next_temp();
        out.push_str(&format!("    {{\n        {list_type_name} {list_temp} = {iter_code};\n"));
        out.push_str(&format!(
            "        for (int64_t {index_temp} = 0; {index_temp} < {list_temp}->length; {index_temp}++) {{\n"
        ));
        out.push_str(&format!("            {} {pattern} = {list_temp}->items[{index_temp}];\n", c_type_name(&elem_ty)));
        self.push_scope();
        self.define(pattern, elem_ty);
        self.gen_block_stmts(body, out)?;
        self.pop_scope();
        out.push_str("        }\n    }\n");
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
                if name == "None" {
                    return Ok(("0".to_string(), CType::NoneLit));
                }
                Err(format!("internal error: no type recorded for '{name}' in the native backend"))
            }
            Expr::Unary(op, inner) => {
                let (code, ty) = self.gen_expr(inner)?;
                match op {
                    UnaryOp::Neg if matches!(ty, CType::Quantity(_)) => {
                        let temp = self.next_temp();
                        Ok((format!("({{ Qty {temp} = {code}; {temp}.v = -{temp}.v; {temp}; }})"), ty))
                    }
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
                if then_ty == CType::Void || else_ty == CType::Void {
                    return Ok((format!("({cond_code} ? {then_code} : {else_code})"), CType::Void));
                }
                let result_ty = unify_types(&then_ty, &else_ty);
                let then_code = self.coerce(&then_code, &then_ty, &result_ty)?;
                let else_code = self.coerce(&else_code, &else_ty, &result_ty)?;
                Ok((format!("({cond_code} ? {then_code} : {else_code})"), result_ty))
            }
            Expr::Block(b) => self.gen_block_expr(b),
            Expr::Match(scrutinee, arms) => self.gen_match(scrutinee, arms),
            Expr::ListLiteral(items) => self.gen_list_literal(items, None),
            Expr::Try(inner, handler) => self.gen_try(inner, handler.as_deref()),
            Expr::UnitLiteral(num, unit) => {
                let (code, ty) = self.gen_expr(num)?;
                let dim = resolve_unit_expr(unit).map_err(|u| format!("unknown unit '{u}'"))?;
                let v = self.as_f64_code(&code, &ty)?;
                Ok((format!("((Qty){{ {v}, {} }})", c_string_literal(unit)), CType::Quantity(dim)))
            }
            Expr::As(inner, unit_expr) => {
                let (code, ty) = self.gen_expr(inner)?;
                let Expr::Ident(sym) = unit_expr.unlocated() else {
                    return Err("'as' expects a unit identifier".to_string());
                };
                let dim = resolve_unit_expr(sym).map_err(|u| format!("unknown unit '{u}'"))?;
                let v = self.as_f64_code(&code, &ty)?;
                Ok((format!("((Qty){{ {v}, {} }})", c_string_literal(sym)), CType::Quantity(dim)))
            }
            Expr::Within(value, range) => {
                let Expr::Range(start, kind, end, _) = range.unlocated() else {
                    return Err("'within' expects a range on the right-hand side".to_string());
                };
                let (vc, vt) = self.gen_expr(value)?;
                let (sc, st) = self.gen_expr(start)?;
                let (ec, et) = self.gen_expr(end)?;
                let (v, s, e) = (self.as_f64_code(&vc, &vt)?, self.as_f64_code(&sc, &st)?, self.as_f64_code(&ec, &et)?);
                let temp = self.next_temp();
                let upper = if *kind == RangeKind::To { "<=" } else { "<" };
                Ok((format!("({{ double {temp} = {v}; {temp} >= {s} && {temp} {upper} {e}; }})"), CType::Bool))
            }
            Expr::Approximately(a, b, tol) => {
                let (ac, at) = self.gen_expr(a)?;
                let (bc, bt) = self.gen_expr(b)?;
                let (tc, tt) = self.gen_expr(tol)?;
                let (a, b, t) = (self.as_f64_code(&ac, &at)?, self.as_f64_code(&bc, &bt)?, self.as_f64_code(&tc, &tt)?);
                Ok((format!("(({a} - {b}) < 0 ? -(({a}) - ({b})) : (({a}) - ({b}))) <= {t}"), CType::Bool))
            }
            Expr::Index(obj, idx) => {
                let (obj_code, obj_ty) = self.gen_expr(obj)?;
                let CType::List(elem_ty) = obj_ty else {
                    return Err("indexing is only supported on List values by the native backend yet".to_string());
                };
                let elem_ty = *elem_ty;
                let struct_name = self.ensure_list(&elem_ty);
                let (idx_code, _) = self.gen_expr(idx)?;
                Ok((format!("{struct_name}_get({obj_code}, {idx_code})"), elem_ty))
            }
            other => Err(format!("this expression isn't supported by the native backend yet: {other:?}")),
        }
    }

    /// `expr?` / `try expr`: unwrap `Some`/`Ok`, or return early from the
    /// *enclosing function* (a C `return` inside a GNU statement expression
    /// is legal). A `None`/`Err` propagates as the function's own
    /// `Option`/`Result`; an optional handler lambda maps the error first.
    fn gen_try(&mut self, inner: &Expr, handler: Option<&Expr>) -> Result<(String, CType), String> {
        if self.lambda_depth > 0 {
            return Err("'?' inside a lambda isn't supported by the native backend yet".to_string());
        }
        let (code, ty) = self.gen_expr(inner)?;
        let Some(fn_ret) = self.current_return.last().cloned() else {
            return Err("'?' outside a function".to_string());
        };
        let temp = self.next_temp();
        let tc = c_type_name(&ty);
        match (&ty, &fn_ret) {
            (CType::Option(t), CType::Option(_)) => Ok((
                format!("({{ {tc} {temp} = {code}; if (!{temp}.has) {{ return (({}){{ .has = false }}); }} {temp}.value; }})", c_type_name(&fn_ret)),
                (**t).clone(),
            )),
            (CType::Result(t, e), CType::Result(_, fe)) => {
                let err_code = match handler {
                    Some(h) => {
                        let Expr::Lambda(names, body) = h.unlocated() else {
                            return Err("the '?' error handler must be an inline lambda in the native backend".to_string());
                        };
                        let (hc, ht) = self.inline_lambda(names, &[(**e).clone()], body)?;
                        // The handler's parameter is bound by name in scope; bind it in C too.
                        let bound = format!("({{ {} {} = {temp}.error; {hc}; }})", c_type_name(e), names[0]);
                        self.coerce(&bound, &ht, fe)?
                    }
                    None => {
                        if **e != **fe {
                            return Err("'?' needs the same error type as the function's Result (or an error handler)".to_string());
                        }
                        format!("{temp}.error")
                    }
                };
                Ok((
                    format!("({{ {tc} {temp} = {code}; if (!{temp}.ok) {{ return (({}){{ .ok = false, .error = {err_code} }}); }} {temp}.value; }})", c_type_name(&fn_ret)),
                    (**t).clone(),
                ))
            }
            _ => Err("'?' needs an Option in a function returning Option, or a Result in a function returning Result".to_string()),
        }
    }

    /// A list literal's element type comes from its first element; every
    /// other element must match it exactly (no implicit widening, same as
    /// everywhere else in this backend). An empty literal (`[]`) has no
    /// element to infer from and isn't supported.
    fn gen_list_literal(&mut self, items: &[Expr], expected: Option<&CType>) -> Result<(String, CType), String> {
        if items.is_empty() {
            return Err("empty list literals aren't supported by the native backend yet (the element type can't be inferred)".to_string());
        }
        let mut codes = Vec::with_capacity(items.len());
        let mut elem_ty: Option<CType> = expected.cloned();
        for item in items {
            let (mut code, mut ty) = self.gen_expr(item)?;
            if let Some(expected) = expected {
                code = self.coerce(&code, &ty, expected)?;
                ty = expected.clone();
            }
            match &elem_ty {
                Some(expected) if *expected != ty => {
                    return Err(format!(
                        "list literal elements must all have the same type ('{}' vs '{}')",
                        c_type_name(expected),
                        c_type_name(&ty)
                    ));
                }
                Some(_) => {}
                None => elem_ty = Some(ty),
            }
            codes.push(code);
        }
        let elem_ty = elem_ty.expect("checked items.is_empty() above");
        let struct_name = self.ensure_list(&elem_ty);
        let array_literal = format!("({}[]){{ {} }}", c_type_name(&elem_ty), codes.join(", "));
        Ok((format!("{struct_name}_new_from_array({array_literal}, {})", items.len()), CType::List(Box::new(elem_ty))))
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
        let mut arm_bodies: Vec<(String, CType)> = Vec::new();
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
            // A bare `None` arm has no `T` of its own: remember every arm's
            // body and coerce them all once the real result type is known
            // (the first arm that isn't a bare `None`).
            let placeholder = format!("@@ARM{}@@", arm_bodies.len());
            arm_bodies.push((body_code, body_ty.clone()));
            let body_code = placeholder;
            result_ty = Some(match &result_ty {
                None => body_ty.clone(),
                Some(prev) => unify_types(prev, &body_ty),
            });
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
        for (index, (code, ty)) in arm_bodies.into_iter().enumerate() {
            let coerced = self.coerce(&code, &ty, &result_ty)?;
            arm_blocks = arm_blocks.replace(&format!("@@ARM{index}@@"), &coerced);
        }

        // A match used for effect (every arm Void) has no value to store.
        let is_void = result_ty == CType::Void;
        let (result_decl, arm_blocks, yielded) = if is_void {
            (String::new(), arm_blocks.replace(&format!("{result_var} = "), ""), "(void)0".to_string())
        } else {
            (format!("{} {result_var};", c_type_name(&result_ty)), arm_blocks, result_var.clone())
        };
        let body = format!(
            "{scrut_ty} {scrutinee_var} = {scrutinee_code}; \
             int {matched_var} = 0; \
             {result_decl} \
             {arm_blocks} \
             if (!{matched_var}) {{ fprintf(stderr, \"ostrin: non-exhaustive match at runtime\\n\"); abort(); }}",
            scrut_ty = c_type_name(&scrutinee_ty),
        );
        Ok((format!("({{ {body} {yielded}; }})"), result_ty))
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
            Pattern::Variant(name, fields) if (name == "Ok" || name == "Err") && matches!(scrutinee_ty, CType::Result(..)) => {
                let CType::Result(ok, err) = scrutinee_ty else { unreachable!() };
                let (flag, field, ty) = if name == "Ok" { (format!("{scrutinee_var}.ok"), "value", ok) } else { (format!("!{scrutinee_var}.ok"), "error", err) };
                condition.push_str(&format!(" && ({flag})"));
                match fields.as_slice() {
                    [(_, Pattern::Ident(b))] => {
                        bindings.push_str(&format!("{} {} = {}.{field}; ", c_type_name(ty), b, scrutinee_var));
                        self.define(b, (**ty).clone());
                        Ok(())
                    }
                    [(_, Pattern::Wildcard)] => Ok(()),
                    _ => Err("only 'Ok(name)' / 'Err(name)' patterns are supported by the native backend yet".to_string()),
                }
            }
            Pattern::Ident(name) if name == "None" && matches!(scrutinee_ty, CType::Option(_)) => {
                condition.push_str(&format!(" && (!{scrutinee_var}.has)"));
                Ok(())
            }
            Pattern::Variant(name, fields) if name == "Some" && matches!(scrutinee_ty, CType::Option(_)) => {
                let CType::Option(inner) = scrutinee_ty else { unreachable!() };
                condition.push_str(&format!(" && ({scrutinee_var}.has)"));
                match fields.as_slice() {
                    [(_, Pattern::Ident(b))] => {
                        bindings.push_str(&format!("{} {} = {}.value; ", c_type_name(inner), b, scrutinee_var));
                        self.define(b, (**inner).clone());
                        Ok(())
                    }
                    [(_, Pattern::Wildcard)] => Ok(()),
                    _ => Err("only 'Some(name)' / 'Some(_)' patterns are supported by the native backend yet".to_string()),
                }
            }
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
        if matches!(lt, CType::Quantity(_)) || matches!(rt, CType::Quantity(_)) {
            return self.gen_quantity_binary(op, &lc, &lt, &rc, &rt);
        }
        if matches!(lt, CType::Record(_) | CType::Enum(_) | CType::DynTrait(_) | CType::List(_) | CType::Option(_) | CType::NoneLit | CType::Result(..) | CType::OkLit(_) | CType::ErrLit(_))
            || matches!(rt, CType::Record(_) | CType::Enum(_) | CType::DynTrait(_) | CType::List(_) | CType::Option(_) | CType::NoneLit | CType::Result(..) | CType::OkLit(_) | CType::ErrLit(_))
        {
            // C has no `==`/`<`/etc. on struct values at all (a compile
            // error, not just the wrong answer) — but even where a raw `==`
            // on two records *would* compile (comparing their pointers), it
            // would silently mean identity, not the structural
            // `derive(Eq)`/`impl Eq` comparison Ostrin actually defines.
            return Err(
                "operators on records/enums/'dyn Trait'/List values aren't supported by the native backend yet (no derive(Eq/Ord) dispatch)"
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
            _ => lt.clone(),
        };
        if op == BinOp::Div && lt == CType::Int && rt == CType::Int {
            return Ok((format!("ostrin_idiv({lc}, {rc})"), CType::Int));
        }
        Ok((format!("({lc} {c_op} {rc})"), result_ty))
    }

    /// Arithmetic/comparison where at least one side is a `Quantity`,
    /// following `eval_binary_builtin`/`compare` in the interpreter rule by
    /// rule. Which helper runs (and the result's dimension) is decided here,
    /// statically; only the unit strings are resolved at runtime.
    fn gen_quantity_binary(&mut self, op: BinOp, lc: &str, lt: &CType, rc: &str, rt: &CType) -> Result<(String, CType), String> {
        let scalar = |code: &str, ty: &CType| -> Option<String> {
            matches!(ty, CType::Int | CType::Float).then(|| format!("(double)({code})"))
        };
        match (lt, rt) {
            (CType::Quantity(d1), CType::Quantity(d2)) => match op {
                BinOp::Add => Ok((format!("ostrin_qty_add({lc}, {rc})"), lt.clone())),
                BinOp::Sub => Ok((format!("ostrin_qty_sub({lc}, {rc})"), lt.clone())),
                BinOp::Mul => Ok((format!("ostrin_qty_mul({lc}, {rc})"), CType::Quantity(dim_mul(d1, d2)))),
                BinOp::Div => {
                    let combined = dim_div(d1, d2);
                    if dim_is_dimensionless(&combined) {
                        Ok((format!("ostrin_qty_ratio({lc}, {rc})"), CType::Float))
                    } else {
                        Ok((format!("ostrin_qty_div({lc}, {rc})"), CType::Quantity(combined)))
                    }
                }
                BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
                    let c_op = match op {
                        BinOp::Eq => "==",
                        BinOp::NotEq => "!=",
                        BinOp::Lt => "<",
                        BinOp::Gt => ">",
                        BinOp::LtEq => "<=",
                        _ => ">=",
                    };
                    Ok((format!("(ostrin_qty_cmp({lc}, {rc}) {c_op} 0)"), CType::Bool))
                }
                _ => Err("this operator isn't supported on Quantity values".to_string()),
            },
            (CType::Quantity(d), other) => {
                let Some(s) = scalar(rc, other) else {
                    return Err("cannot combine a Quantity with this operand in the native backend".to_string());
                };
                match op {
                    BinOp::Mul => Ok((format!("ostrin_qty_scale_mul({lc}, {s})"), lt.clone())),
                    BinOp::Div => Ok((format!("ostrin_qty_scale_div({lc}, {s})"), CType::Quantity(d.clone()))),
                    _ => Err("cannot combine a Quantity with a plain scalar without an explicit unit ('as <unit>')".to_string()),
                }
            }
            (other, CType::Quantity(d)) => {
                let Some(s) = scalar(lc, other) else {
                    return Err("cannot combine a Quantity with this operand in the native backend".to_string());
                };
                match op {
                    BinOp::Mul => Ok((format!("ostrin_qty_scale_mul({rc}, {s})"), rt.clone())),
                    BinOp::Div => Ok((format!("ostrin_scalar_div_qty({s}, {rc})"), CType::Quantity(dim_pow(d, -1)))),
                    _ => Err("cannot combine a Quantity with a plain scalar without an explicit unit ('as <unit>')".to_string()),
                }
            }
            _ => unreachable!("gen_quantity_binary is only called with a Quantity operand"),
        }
    }

    /// A numeric operand as a bare `double` (a Quantity contributes its raw
    /// value, ignoring its unit — as the interpreter's `as_f64` does for
    /// `as`, `within` and `approximately`).
    fn as_f64_code(&self, code: &str, ty: &CType) -> Result<String, String> {
        match ty {
            CType::Quantity(_) => Ok(format!("({code}).v")),
            CType::Int | CType::Float => Ok(format!("(double)({code})")),
            _ => Err("expected a number".to_string()),
        }
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
        if (name == "Ok" || name == "Err") && args.len() == 1 {
            let (codes, types) = self.gen_args(args)?;
            let ty = if name == "Ok" { CType::OkLit(Box::new(types[0].clone())) } else { CType::ErrLit(Box::new(types[0].clone())) };
            return Ok((codes[0].clone(), ty));
        }
        if name == "Some" && args.len() == 1 {
            let (codes, types) = self.gen_args(args)?;
            let ty = CType::Option(Box::new(types[0].clone()));
            self.register_list_types(&ty);
            return Ok((format!("(({}){{ .has = true, .value = {} }})", c_type_name(&ty), codes[0]), ty));
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
        for ty in param_types.iter().chain(std::iter::once(&return_type)) {
            self.register_list_types(ty);
        }
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
        // `to_string()` exists on every scalar in the interpreter
        // (`receiver.to_string()` in `eval_call`); records/enums aren't
        // covered (their printed form needs a generated Display).
        if method_name == "to_string" && args.is_empty() {
            let text = match &obj_ty {
                CType::Int => Some(format!("ostrin_int_to_string({obj_code})")),
                CType::Float => Some(format!("ostrin_float_to_string({obj_code})")),
                CType::Bool => Some(format!("(({obj_code}) ? \"true\" : \"false\")")),
                CType::Str => Some(obj_code.clone()),
                CType::Quantity(_) => Some(format!("ostrin_qty_to_string({obj_code})")),
                _ => None,
            };
            if let Some(text) = text {
                return Ok((text, CType::Str));
            }
        }
        match &obj_ty {
            CType::Record(record_name) | CType::Enum(record_name) => {
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
            CType::Result(ok, err) => {
                let (ok, err) = ((**ok).clone(), (**err).clone());
                let (arg_codes, arg_types) = self.gen_args(args)?;
                let temp = self.next_temp();
                let rc = c_type_name(&obj_ty);
                match method_name {
                    "is_ok" => Ok((format!("({{ {rc} {temp} = {obj_code}; {temp}.ok; }})"), CType::Bool)),
                    "is_err" => Ok((format!("({{ {rc} {temp} = {obj_code}; !{temp}.ok; }})"), CType::Bool)),
                    "unwrap" => Ok((
                        format!("({{ {rc} {temp} = {obj_code}; if (!{temp}.ok) {{ fprintf(stderr, \"ostrin: unwrap on Err\\n\"); exit(1); }} {temp}.value; }})"),
                        ok,
                    )),
                    "unwrap_or" => {
                        if arg_codes.len() != 1 {
                            return Err("'unwrap_or' expects one argument".to_string());
                        }
                        let d = self.coerce(&arg_codes[0], &arg_types[0], &ok)?;
                        let _ = err;
                        Ok((format!("({{ {rc} {temp} = {obj_code}; {temp}.ok ? {temp}.value : ({d}); }})"), ok))
                    }
                    other => Err(format!("Result has no method '{other}' the native backend supports yet")),
                }
            }
            CType::Option(inner) => {
                let inner = (**inner).clone();
                let (arg_codes, arg_types) = self.gen_args(args)?;
                let temp = self.next_temp();
                let oc = c_type_name(&obj_ty);
                match method_name {
                    "is_some" => Ok((format!("({{ {oc} {temp} = {obj_code}; {temp}.has; }})"), CType::Bool)),
                    "is_none" => Ok((format!("({{ {oc} {temp} = {obj_code}; !{temp}.has; }})"), CType::Bool)),
                    "unwrap" => Ok((
                        format!("({{ {oc} {temp} = {obj_code}; if (!{temp}.has) {{ fprintf(stderr, \"ostrin: unwrap on None\\n\"); exit(1); }} {temp}.value; }})"),
                        inner,
                    )),
                    "unwrap_or" => {
                        if arg_codes.len() != 1 {
                            return Err("'unwrap_or' expects one argument".to_string());
                        }
                        let d = self.coerce(&arg_codes[0], &arg_types[0], &inner)?;
                        Ok((format!("({{ {oc} {temp} = {obj_code}; {temp}.has ? {temp}.value : ({d}); }})"), inner))
                    }
                    other => Err(format!("Option has no method '{other}' the native backend supports yet")),
                }
            }
            CType::List(elem_ty) => {
                let elem_ty = (**elem_ty).clone();
                let struct_name = self.ensure_list(&elem_ty);
                if matches!(method_name, "map" | "filter" | "fold" | "any" | "all" | "find") {
                    return self.gen_list_combinator(&obj_code, &elem_ty, method_name, args);
                }
                let (arg_codes, arg_types) = self.gen_args(args)?;
                match method_name {
                    "length" | "count" => {
                        if !arg_codes.is_empty() {
                            return Err("'length' takes no arguments".to_string());
                        }
                        Ok((format!("{struct_name}_length({obj_code})"), CType::Int))
                    }
                    "push" => {
                        if arg_codes.len() != 1 {
                            return Err("'push' expects exactly one argument".to_string());
                        }
                        let coerced = self.coerce(&arg_codes[0], &arg_types[0], &elem_ty)?;
                        Ok((format!("{struct_name}_push({obj_code}, {coerced})"), CType::Void))
                    }
                    "remove_at" => {
                        if arg_codes.len() != 1 {
                            return Err("'remove_at' expects exactly one argument".to_string());
                        }
                        Ok((format!("{struct_name}_remove_at({obj_code}, {})", arg_codes[0]), elem_ty))
                    }
                    other => Err(format!(
                        "List has no method '{other}' the native backend supports yet \
                         ('map'/'filter'/'fold'/'find'/'any'/'all' need closures, which aren't supported)"
                    )),
                }
            }
            _ => Err("method calls are only supported on records, 'dyn Trait' values or List by the native backend yet".to_string()),
        }
    }

    /// Splits a combinator argument into a lambda's parameter names and body.
    fn lambda_of<'e>(arg: &'e Arg, method: &str) -> Result<(&'e [String], &'e Block), String> {
        match arg {
            Arg::Positional(e) => match e.unlocated() {
                Expr::Lambda(params, body) => Ok((params.as_slice(), body)),
                _ => Err(format!("'{method}' only supports an inline lambda argument in the native backend (function values aren't supported)")),
            },
            Arg::Named(..) => Err("named arguments aren't supported by the native backend yet".to_string()),
        }
    }

    /// Types and generates a lambda body with its parameters bound to the
    /// given concrete types. There is no closure object here at all: the
    /// lambda is only ever accepted as a direct argument of a list
    /// combinator, which is expanded *inline* as a loop, so the body simply
    /// sees the enclosing C scope — captured variables need no environment
    /// struct, function pointer or escape analysis.
    fn inline_lambda(&mut self, names: &[String], types: &[CType], body: &Block) -> Result<(String, CType), String> {
        if names.len() != types.len() {
            return Err(format!("this lambda takes {} parameter(s) but the combinator supplies {}", names.len(), types.len()));
        }
        self.push_scope();
        for (name, ty) in names.iter().zip(types) {
            self.define(name, ty.clone());
        }
        self.lambda_depth += 1;
        let result = self.gen_block_expr(body);
        self.lambda_depth -= 1;
        self.pop_scope();
        result
    }

    /// `map`/`filter`/`fold`/`any`/`all` on a list, expanded to an inline
    /// loop inside a GNU statement expression (see `inline_lambda`).
    fn gen_list_combinator(&mut self, list_code: &str, elem_ty: &CType, method: &str, args: &[Arg]) -> Result<(String, CType), String> {
        let elem_c = c_type_name(elem_ty);
        let src = self.next_temp();
        let idx = self.next_temp();
        let list_c = c_type_name(&CType::List(Box::new(elem_ty.clone())));
        let head = format!("{list_c} {src} = {list_code};");
        let loop_head = format!("for (int64_t {idx} = 0; {idx} < {src}->length; {idx}++)");
        match method {
            "fold" => {
                if args.len() != 2 {
                    return Err("'fold' expects an initial value and a lambda".to_string());
                }
                let Arg::Positional(init_expr) = &args[0] else {
                    return Err("named arguments aren't supported by the native backend yet".to_string());
                };
                let (init_code, acc_ty) = self.gen_expr(init_expr)?;
                let (names, body) = Self::lambda_of(&args[1], method)?;
                if names.len() != 2 {
                    return Err("'fold' needs a lambda with two parameters (accumulator, element)".to_string());
                }
                let (body_code, body_ty) = self.inline_lambda(names, &[acc_ty.clone(), elem_ty.clone()], body)?;
                let body_code = self.coerce(&body_code, &body_ty, &acc_ty)?;
                let acc_c = c_type_name(&acc_ty);
                Ok((
                    format!(
                        "({{ {head} {acc_c} {acc} = {init_code}; {loop_head} {{ {elem_c} {el} = {src}->items[{idx}]; {acc} = {body_code}; }} {acc}; }})",
                        acc = names[0],
                        el = names[1]
                    ),
                    acc_ty,
                ))
            }
            _ => {
                if args.len() != 1 {
                    return Err(format!("'{method}' expects exactly one lambda"));
                }
                let (names, body) = Self::lambda_of(&args[0], method)?;
                if names.len() != 1 {
                    return Err(format!("'{method}' needs a lambda with one parameter"));
                }
                let (body_code, body_ty) = self.inline_lambda(names, &[elem_ty.clone()], body)?;
                let el = &names[0];
                let bind = format!("{elem_c} {el} = {src}->items[{idx}];");
                match method {
                    "map" => {
                        if body_ty == CType::Void {
                            return Err("'map' lambda must produce a value".to_string());
                        }
                        let out_struct = self.ensure_list(&body_ty);
                        let dst = self.next_temp();
                        let out_c = c_type_name(&CType::List(Box::new(body_ty.clone())));
                        Ok((
                            format!(
                                "({{ {head} {out_c} {dst} = {out_struct}_new_from_array(NULL, 0); {loop_head} {{ {bind} {out_struct}_push({dst}, {body_code}); }} {dst}; }})"
                            ),
                            CType::List(Box::new(body_ty)),
                        ))
                    }
                    "filter" => {
                        let dst = self.next_temp();
                        let st = list_struct_name(elem_ty);
                        Ok((
                            format!(
                                "({{ {head} {list_c} {dst} = {st}_new_from_array(NULL, 0); {loop_head} {{ {bind} if ({body_code}) {{ {st}_push({dst}, {el}); }} }} {dst}; }})"
                            ),
                            CType::List(Box::new(elem_ty.clone())),
                        ))
                    }
                    "find" => {
                        let r = self.next_temp();
                        let oty = CType::Option(Box::new(elem_ty.clone()));
                        self.register_list_types(&oty);
                        let oc = c_type_name(&oty);
                        Ok((
                            format!(
                                "({{ {head} {oc} {r} = ({oc}){{ .has = false }}; {loop_head} {{ {bind} if ({body_code}) {{ {r}.has = true; {r}.value = {el}; break; }} }} {r}; }})"
                            ),
                            oty,
                        ))
                    }
                    "any" | "all" => {
                        let r = self.next_temp();
                        let (init, test, set) = if method == "any" {
                            ("false", format!("({body_code})"), "true")
                        } else {
                            ("true", format!("!({body_code})"), "false")
                        };
                        Ok((
                            format!(
                                "({{ {head} bool {r} = {init}; {loop_head} {{ {bind} if ({test}) {{ {r} = {set}; break; }} }} {r}; }})"
                            ),
                            CType::Bool,
                        ))
                    }
                    _ => unreachable!("only combinator names reach here"),
                }
            }
        }
    }

    fn gen_print(&self, arg_codes: &[String], arg_types: &[CType]) -> Result<(String, CType), String> {
        if arg_codes.len() != 1 {
            return Err("'print' expects exactly one argument".to_string());
        }
        let (spec, value) = match &arg_types[0] {
            CType::Int => ("%lld\\n", format!("(long long)({})", arg_codes[0])),
            CType::Float => return Ok((format!("ostrin_print_float({})", arg_codes[0]), CType::Void)),
            CType::Bool => ("%s\\n", format!("(({}) ? \"true\" : \"false\")", arg_codes[0])),
            CType::Str => ("%s\\n", arg_codes[0].clone()),
            CType::Void => return Err("cannot 'print' a Void value".to_string()),
            CType::Record(name) => return Err(format!("cannot 'print' a record value ('{name}' has no derived Display)")),
            CType::Enum(name) => return Err(format!("cannot 'print' an enum value yet ('{name}' has no generated Display)")),
            CType::DynTrait(name) => return Err(format!("cannot 'print' a 'dyn {name}' value")),
            CType::List(elem) => return Err(format!("cannot 'print' a List<{}> value yet", c_type_name(elem))),
            CType::Quantity(_) => return Ok((format!("ostrin_print_qty({})", arg_codes[0]), CType::Void)),
            CType::Option(_) | CType::NoneLit | CType::Result(..) | CType::OkLit(_) | CType::ErrLit(_) => {
                return Err("cannot 'print' an Option/Result value yet".to_string())
            }
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
        // `Quantity<D>`: `D` stands for the dimension of the argument's
        // Quantity (recorded as a `CType::Quantity` so it can be threaded
        // through `subst` like any other type parameter).
        if let (Type::Named(q, qargs), CType::Quantity(_)) = (&param.ty, arg_ty) {
            if q == "Quantity" && qargs.len() == 1 {
                if let Type::Named(d, dargs) = &qargs[0] {
                    if dargs.is_empty() && generic_names.contains(d.as_str()) {
                        subst.entry(d.clone()).or_insert_with(|| arg_ty.clone());
                        continue;
                    }
                }
            }
        }
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

/// The common type of two branch results, completing partial literals
/// (`None`, `Ok(x)`, `Err(e)`) from whichever side is more informative.
fn unify_types(a: &CType, b: &CType) -> CType {
    match (a, b) {
        (x, y) if x == y => x.clone(),
        (CType::NoneLit, other) | (other, CType::NoneLit) => other.clone(),
        (CType::OkLit(t), CType::ErrLit(e)) | (CType::ErrLit(e), CType::OkLit(t)) => CType::Result(t.clone(), e.clone()),
        (CType::OkLit(_) | CType::ErrLit(_), r @ CType::Result(..)) | (r @ CType::Result(..), CType::OkLit(_) | CType::ErrLit(_)) => r.clone(),
        _ => a.clone(),
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

/// Transpiles an already type-checked program to C. Top-level functions
/// (generic ones monomorphized per call site — see `PendingInstance`),
/// plain (non-generic) records with their non-generic `impl` methods
/// (resolved statically — a record method call always knows its concrete
/// target), plain (non-generic) enums with `match`, `dyn Trait` values
/// (dispatched through a real vtable — see `PendingVTable`), and `List<T>`
/// (monomorphized per element type — see `Codegen::ensure_list`) are all
/// supported. A non-generic trait whose methods are all "object
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
        list_instantiations: HashMap::new(),
        pending_lists: VecDeque::new(),
        option_instantiations: HashSet::new(),
        pending_options: VecDeque::new(),
        result_instantiations: HashSet::new(),
        pending_results: VecDeque::new(),
        current_return: Vec::new(),
        lambda_depth: 0,
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
        for (_, ty) in &fields {
            codegen.register_list_types(ty);
        }
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
            for (_, ty) in &fields {
                codegen.register_list_types(ty);
            }
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
        for ty in param_types.iter().chain(std::iter::once(&return_type)) {
            codegen.register_list_types(ty);
        }
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
        if !im.generics.is_empty() {
            continue;
        }
        let self_ty = if record_names.contains(&im.type_name) {
            CType::Record(im.type_name.clone())
        } else if enum_names.contains(&im.type_name) {
            CType::Enum(im.type_name.clone())
        } else {
            continue;
        };
        for method in &im.methods {
            if !method.generics.is_empty() {
                continue;
            }
            let self_subst: HashMap<String, CType> = HashMap::from([("Self".to_string(), self_ty.clone())]);
            let param_types: Result<Vec<CType>, String> =
                method.params.iter().map(|p| map_type_with_subst(&p.ty, &codegen.named_types(), &self_subst)).collect();
            let Ok(param_types) = param_types else { continue };
            let Ok(return_type) = map_type_with_subst(&method.return_type, &codegen.named_types(), &self_subst) else { continue };
            for ty in param_types.iter().chain(std::iter::once(&return_type)) {
                codegen.register_list_types(ty);
            }
            let info = MethodInfo {
                decl: method,
                param_types,
                return_type,
                c_name: format!("{}__{}", im.type_name, method.name),
                self_ty: self_ty.clone(),
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
    let method_infos: Vec<(CType, Vec<CType>, CType, String, &FunctionDecl)> = codegen
        .methods
        .values()
        .flat_map(|methods| methods.values())
        .map(|info| (info.self_ty.clone(), info.param_types.clone(), info.return_type.clone(), info.c_name.clone(), info.decl))
        .collect();
    for (self_ty, param_types, return_type, c_name, decl) in method_infos {
        let params = render_params(&param_types, &decl.params);
        let signature = format!("{} {}({})", c_type_name(&return_type), c_name, params);
        let self_subst: HashMap<String, CType> = HashMap::from([("Self".to_string(), self_ty)]);
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
    let mut list_type_decls = String::new();
    let mut option_inners: Vec<CType> = Vec::new();
    let mut result_pairs: Vec<(CType, CType)> = Vec::new();
    let mut list_helper_prototypes: Vec<String> = Vec::new();
    loop {
        let mut progressed = false;
        while let Some(pair) = codegen.pending_results.pop_front() {
            progressed = true;
            result_pairs.push(pair);
        }
        while let Some(inner) = codegen.pending_options.pop_front() {
            progressed = true;
            option_inners.push(inner);
        }
        while let Some(elem_ty) = codegen.pending_lists.pop_front() {
            progressed = true;
            let struct_name = list_struct_name(&elem_ty);
            let elem_c = c_type_name(&elem_ty);
            list_type_decls.push_str(&format!(
                "typedef struct {struct_name} {struct_name};\n\
                 struct {struct_name} {{\n    {elem_c}* items;\n    int64_t length;\n    int64_t capacity;\n}};\n\n"
            ));

            let new_sig = format!("static {struct_name}* {struct_name}_new_from_array({elem_c}* src_items, int64_t count)");
            let new_body = format!(
                "    {struct_name}* list = ({struct_name}*)malloc(sizeof({struct_name}));\n\
                 \x20   if (!list) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }}\n\
                 \x20   list->capacity = count > 0 ? count : 1;\n\
                 \x20   list->length = count;\n\
                 \x20   list->items = ({elem_c}*)malloc(sizeof({elem_c}) * (size_t)list->capacity);\n\
                 \x20   if (!list->items) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }}\n\
                 \x20   for (int64_t i = 0; i < count; i++) {{ list->items[i] = src_items[i]; }}\n\
                 \x20   return list;\n"
            );

            let push_sig = format!("static void {struct_name}_push({struct_name}* list, {elem_c} value)");
            let push_body = format!(
                "    if (list->length >= list->capacity) {{\n\
                 \x20       list->capacity = list->capacity == 0 ? 4 : list->capacity * 2;\n\
                 \x20       list->items = ({elem_c}*)realloc(list->items, sizeof({elem_c}) * (size_t)list->capacity);\n\
                 \x20       if (!list->items) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }}\n\
                 \x20   }}\n\
                 \x20   list->items[list->length] = value;\n\
                 \x20   list->length = list->length + 1;\n"
            );

            let length_sig = format!("static int64_t {struct_name}_length({struct_name}* list)");
            let length_body = "    return list->length;\n".to_string();

            let bounds_check = format!(
                "    if (index < 0 || index >= list->length) {{ \
                 fprintf(stderr, \"ostrin: index out of bounds: %lld\\n\", (long long)index); exit(1); }}\n"
            );
            let get_sig = format!("static {elem_c} {struct_name}_get({struct_name}* list, int64_t index)");
            let get_body = format!("{bounds_check}    return list->items[index];\n");

            let remove_sig = format!("static {elem_c} {struct_name}_remove_at({struct_name}* list, int64_t index)");
            let remove_body = format!(
                "{bounds_check}\
                 \x20   {elem_c} removed = list->items[index];\n\
                 \x20   for (int64_t i = index; i < list->length - 1; i++) {{ list->items[i] = list->items[i + 1]; }}\n\
                 \x20   list->length = list->length - 1;\n\
                 \x20   return removed;\n"
            );

            for (signature, body) in [
                (new_sig, new_body),
                (push_sig, push_body),
                (length_sig, length_body),
                (get_sig, get_body),
                (remove_sig, remove_body),
            ] {
                list_helper_prototypes.push(format!("{signature};"));
                bodies.push((signature, body));
            }
        }
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

    // Every `List` struct is declared here, right before the prototype
    // section that follows — unlike records/enums/trait vtable types
    // (known upfront, from item declarations, so they're already in `out`
    // by this point), a `List`'s element type is only known once the drain
    // loop above has finished discovering it from actual usage.
    out.push_str(&list_type_decls);
    for (ok, err) in std::mem::take(&mut result_pairs) {
        let name = format!("Result_{}_{}", mangle_ctype(&ok), mangle_ctype(&err));
        out.push_str(&format!("typedef struct {{ bool ok; {} value; {} error; }} {name};

", c_type_name(&ok), c_type_name(&err)));
    }
    for inner in std::mem::take(&mut option_inners) {
        let name = format!("Option_{}", mangle_ctype(&inner));
        out.push_str(&format!("typedef struct {{ bool has; {} value; }} {name};

", c_type_name(&inner)));
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
    for prototype in list_helper_prototypes.iter().chain(&thunk_prototypes) {
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
    if out.contains("Qty") {
        out = out.replacen(PRELUDE, &format!("{PRELUDE}{QTY_RUNTIME}"), 1);
    }
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
