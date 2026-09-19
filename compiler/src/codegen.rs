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

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::ast::*;
use crate::symbols::type_to_string;
use crate::typeck::ExprKey;
use crate::types::Ty;
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
    /// A bare variant of a generic enum (`Nothing`) with no payload to infer
    /// `T` from: (enum base name, variant name). `coerce` completes it once
    /// the expected instance is known.
    GenLit(String, String),
    /// `Map<K, V>` / `Set<T>`: heap-allocated, by reference, insertion-ordered
    /// arrays searched linearly (like the interpreter's `Vec` state).
    Map(Box<CType>, Box<CType>),
    Set(Box<CType>),
    /// `channel<T>()`: a FIFO queue in the heap, by reference. `spawn` runs
    /// synchronously, exactly like the interpreter, so no locking is needed.
    Channel(Box<CType>),
    /// A finished `spawn` block's result (by value); `join()` reads it.
    Task(Box<CType>),
    /// A fixed-width integer other than `Int` (`UInt8`, `Int32`, …): a C `stdint` type.
    Sized(IntKind),
    /// Single-precision float (C `float`).
    Float32,
    /// `Array<T>`: a heap-allocated dense N-dimensional array (by reference); `T` is `Int`, `Float` or `Float32`.
    Array(Box<CType>),
}

/// A struct-field spelling of a type: `Void` (a `Result<Void, E>`'s value) becomes a placeholder `char`.
fn field_c_type(ty: &CType) -> String {
    if *ty == CType::Void { "char".to_string() } else { c_type_name(ty) }
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
        CType::Map(..) | CType::Set(_) | CType::Channel(_) | CType::Array(_) => format!("{}*", mangle_ctype(ty)),
        CType::Task(_) => mangle_ctype(ty),
        CType::Sized(kind) => kind.c_type().to_string(),
        CType::Float32 => "float".to_string(),
        CType::Option(inner) => format!("Option_{}", mangle_ctype(inner)),
        CType::NoneLit | CType::OkLit(_) | CType::ErrLit(_) | CType::GenLit(..) => "int".to_string(),
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
    /// Generic record/enum name -> (is_enum, number of type parameters).
    generics: &'a HashMap<String, (bool, usize)>,
    /// Every concrete instantiation (`Score<Int>`) `map_type` resolves is
    /// logged here — it has no `&mut self` — and registered by
    /// `Codegen::flush_instances`.
    seen: &'a RefCell<Vec<(String, Vec<CType>)>>,
}

/// The mangled name of one instantiation of a generic record/enum.
fn instance_name(base: &str, args: &[CType]) -> String {
    format!("{base}__{}", args.iter().map(mangle_ctype).collect::<Vec<_>>().join("_"))
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
    map_type_with_subst(ty, types, &HashMap::new())
}

/// Resolves a type, taking any name found in `subst` first. Used for two
/// distinct things that turn out to be the same mechanism: a method's
/// `self`/`Self` and a generic's own type parameters (a monomorphized
/// function, or a generic record/enum's instantiation). Either way, every
/// name in `subst` is already a concrete `CType` by the time this runs.
fn map_type_with_subst(ty: &Type, types: &NamedTypes, subst: &HashMap<String, CType>) -> Result<CType, String> {
    match ty {
        Type::Named(name, args) if name == "Quantity" && args.len() == 1 => {
            let dims: HashMap<String, Dimension> = subst
                .iter()
                .filter_map(|(k, v)| if let CType::Quantity(d) = v { Some((k.clone(), d.clone())) } else { None })
                .collect();
            Ok(CType::Quantity(resolve_dimension(&args[0], &dims)))
        }
        Type::Named(name, args) if args.is_empty() && subst.contains_key(name) => Ok(subst[name].clone()),
        Type::Named(name, args) if args.is_empty() => match name.as_str() {
            "Int" | "Int64" => Ok(CType::Int),
            other if IntKind::from_name(other).is_some() => Ok(CType::Sized(IntKind::from_name(other).unwrap())),
            "Float" | "Float64" => Ok(CType::Float),
            "Float32" => Ok(CType::Float32),
            "Bool" => Ok(CType::Bool),
            "String" => Ok(CType::Str),
            "Void" => Ok(CType::Void),
            other if types.records.contains(other) => Ok(CType::Record(other.to_string())),
            other if types.enums.contains(other) => Ok(CType::Enum(other.to_string())),
            _ => Err(format!("type '{}' is not supported by the native backend yet", type_to_string(ty))),
        },
        Type::Named(name, args) if name == "Result" && args.len() == 2 => Ok(CType::Result(
            Box::new(map_type_with_subst(&args[0], types, subst)?),
            Box::new(map_type_with_subst(&args[1], types, subst)?),
        )),
        Type::Named(name, args) if name == "Option" && args.len() == 1 => {
            Ok(CType::Option(Box::new(map_type_with_subst(&args[0], types, subst)?)))
        }
        Type::Named(name, args) if name == "Map" && args.len() == 2 => Ok(CType::Map(
            Box::new(map_type_with_subst(&args[0], types, subst)?),
            Box::new(map_type_with_subst(&args[1], types, subst)?),
        )),
        Type::Named(name, args) if name == "Channel" && args.len() == 1 => Ok(CType::Channel(Box::new(map_type_with_subst(&args[0], types, subst)?))),
        Type::Named(name, args) if name == "Task" && args.len() == 1 => Ok(CType::Task(Box::new(map_type_with_subst(&args[0], types, subst)?))),
        Type::Named(name, args) if name == "Array" && args.len() == 1 => {
            let elem = map_type_with_subst(&args[0], types, subst)?;
            if matches!(elem, CType::Int | CType::Float | CType::Float32) {
                Ok(CType::Array(Box::new(elem)))
            } else {
                Err("Array<T> is only supported by the native backend for Int, Float and Float32 elements yet".to_string())
            }
        }
        Type::Named(name, args) if name == "Set" && args.len() == 1 => Ok(CType::Set(Box::new(map_type_with_subst(&args[0], types, subst)?))),
        Type::Named(name, args) if name == "List" && args.len() == 1 => {
            Ok(CType::List(Box::new(map_type_with_subst(&args[0], types, subst)?)))
        }
        Type::Named(name, args) if types.generics.get(name).is_some_and(|(_, arity)| *arity == args.len()) => {
            let (is_enum, _) = types.generics[name];
            let concrete = args.iter().map(|a| map_type_with_subst(a, types, subst)).collect::<Result<Vec<_>, _>>()?;
            let mangled = instance_name(name, &concrete);
            types.seen.borrow_mut().push((name.clone(), concrete));
            Ok(if is_enum { CType::Enum(mangled) } else { CType::Record(mangled) })
        }
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

fn c_function_name(name: &str) -> String {
    // The generated file supplies its own `main`, so the user's `main`
    // (which returns Void, not `int`, and takes no argv/argc) is renamed.
    // Every user function is prefixed so its name can never collide with a C
    // keyword or type (`double`, `int`, `default`) or a libc function (`abs`, `exit`).
    if name == "main" { "ostrin_main".to_string() } else { format!("ostrin_fn_{name}") }
}

/// Quantity runtime (unit table, conversion, arithmetic helpers), spliced in
/// right after `PRELUDE` only when a program actually uses `Qty`.
const QTY_RUNTIME: &str = include_str!("qty_runtime.c");

/// `Array<T>` runtime template, instantiated per element type (see the header of the file).
const ARRAY_RUNTIME: &str = include_str!("array_runtime.c");

const PRELUDE: &str = "#include <stdint.h>\n\
#include <stdbool.h>\n\
#include <stdio.h>\n\
#include <stdlib.h>\n\
#include <string.h>\n\
#include <errno.h>\n\
#include <math.h>\n
#define OSTRIN_FAIL(msg) do { fprintf(stderr, \"runtime error: %s\\n\", msg); exit(1); } while (0)\n\
#define OSTRIN_OOM() do { fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); } while (0)\n\
\n\
static int64_t ostrin_abs_i64(int64_t x) {\n\
    if (x == INT64_MIN) OSTRIN_FAIL(\"integer overflow: abs of the smallest Int\");\n\
    return x < 0 ? -x : x;\n\
}\n\
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
static void ostrin_fmt_single(float v, char* buf, size_t n) {\n\
    int prec;\n\
    for (prec = 1; prec <= 9; prec++) {\n\
        snprintf(buf, n, \"%.*g\", prec, (double)v);\n\
        if (strtof(buf, NULL) == v) break;\n\
    }\n\
    if (strchr(buf, 'e')) {\n\
        char t[64];\n\
        snprintf(t, sizeof t, \"%.*e\", prec - 1, (double)v);\n\
        int decimals = prec - 1 - atoi(strchr(t, 'e') + 1);\n\
        snprintf(buf, n, \"%.*f\", decimals < 0 ? 0 : decimals, (double)v);\n\
    }\n\
}\n\
\n\
static const char* ostrin_single_to_string(float v) {\n\
    char* out = (char*)malloc(64);\n\
    ostrin_fmt_single(v, out, 64);\n\
    return out;\n\
}\n\
\n\
static void ostrin_print_single(float v) {\n\
    char buf[64];\n\
    ostrin_fmt_single(v, buf, sizeof buf);\n\
    printf(\"%s\\n\", buf);\n\
}\n\
\n\
static const char* ostrin_int_to_string(int64_t v) {\n\
    char* out = (char*)malloc(32);\n\
    snprintf(out, 32, \"%lld\", (long long)v);\n\
    return out;\n\
}\n\
\n\
static const char* ostrin_uint_to_string(uint64_t v) {\n\
    char* out = (char*)malloc(32);\n\
    snprintf(out, 32, \"%llu\", (unsigned long long)v);\n\
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

/// How the backend's own type inference compares with the checker's
/// (the typed-expression table), expression by expression.
#[derive(Debug, Default, Clone)]
pub struct NativeTypeReport {
    /// Expressions where both agree.
    pub agreed: usize,
    /// Expressions with no checker type, an unknown one, or inside a generic
    /// instantiation (whose checker types are still abstract).
    pub unchecked: usize,
    /// Expressions the backend types only partially (`None`, `Ok(x)`, …) but
    /// the checker knows completely: places where reinference can be retired.
    pub partial: usize,
    /// Of those, how many were completed from the checker's type.
    pub completed: usize,
    /// Generic function calls instantiated from the checker's resolved arguments / by the backend's own inference.
    pub calls_from_checker: usize,
    pub calls_inferred: usize,
    /// Real disagreements: `file:line:col: checker says …, native says …`.
    pub divergences: Vec<String>,
}

/// A method with its own type parameters (`fn map<U>(self, ..)`), kept
/// aside until a call site fixes them.
#[derive(Clone)]
struct GenericMethod<'a> {
    decl: &'a FunctionDecl,
    /// The enclosing `impl`'s own substitution (its type parameters, `Self`).
    binds: HashMap<String, CType>,
    key: String,
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
    /// Generic records/enums (`Score<T>`, `Maybe<T>`): only their concrete
    /// instantiations, discovered on demand, are ever emitted.
    generic_records: HashMap<String, &'a RecordDecl>,
    generic_enums: HashMap<String, &'a EnumDecl>,
    generic_arity: HashMap<String, (bool, usize)>,
    generic_impls: Vec<&'a ImplDecl>,
    /// Variant name -> the generic enum declaring it.
    generic_variant_owner: HashMap<String, String>,
    seen_instances: RefCell<Vec<(String, Vec<CType>)>>,
    instances_done: HashSet<String>,
    /// Mangled instance name -> (generic base name, concrete type arguments).
    instance_info: HashMap<String, (String, Vec<CType>)>,
    /// Instances in discovery order (arguments always precede the instance
    /// that contains them): (is_enum, mangled name).
    instance_order: Vec<(bool, String)>,
    instance_variants: HashMap<String, Vec<VariantInfo>>,
    /// The type the enclosing context expects the next expression to have;
    /// consumed by `gen_expr` and used to complete generic literals.
    expected: Option<CType>,
    /// Records/enums whose generated `ostrin_show_*` (used by `print`) is queued.
    show_queue: VecDeque<CType>,
    /// The checker's type for every expression, when the caller supplied it
    /// (see `generate_with_report`): used only to *compare*, never to generate.
    checker_types: Option<&'a HashMap<ExprKey, Ty>>,
    /// Integer literals the checker typed as fixed-width.
    literal_kinds: Option<&'a HashMap<ExprKey, LitKind>>,
    /// The checker's resolved generic arguments per call site.
    call_substs: Option<&'a HashMap<ExprKey, crate::typeck::CallSubst>>,
    /// Set by `gen_expr` for a call to a plain identifier, consumed by `gen_function_call`.
    current_call_key: Option<ExprKey>,
    current_file: Option<String>,
    compare_enabled: bool,
    type_report: NativeTypeReport,
    /// Type key (record/enum/instance/quantity name) -> generic methods, instantiated per call.
    generic_methods: HashMap<String, HashMap<String, GenericMethod<'a>>>,
    quantity_impls: Vec<&'a ImplDecl>,
    quantity_done: HashSet<String>,
    /// The type-parameter substitution of the function body being generated.
    subst_stack: Vec<HashMap<String, CType>>,
    /// Trait name -> its default-bodied methods, as function declarations.
    trait_defaults: HashMap<String, Vec<&'a FunctionDecl>>,
    pending_colls: VecDeque<CType>,
    coll_done: HashSet<String>,
    /// Every top-level function by name, for named/default argument resolution.
    function_decls: HashMap<String, &'a FunctionDecl>,
    /// `derive(Eq)`/`derive(Ord)` helpers queued: (is_compare, type).
    op_queue: VecDeque<(bool, CType)>,
    op_done: HashSet<(bool, String)>,
    derives: HashMap<String, Vec<String>>,
    show_done: HashSet<String>,
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

/// Aborts the program with the interpreter's error text for an overflow.
const OVERFLOW_ABORT: &str = "fprintf(stderr, \"runtime error: integer overflow\\n\"); exit(1);";

/// A C integer constant for any value in a fixed-width integer's range.
fn c_int_literal(value: i128) -> String {
    if value > i64::MAX as i128 {
        format!("{value}ULL")
    } else if value == i64::MIN as i128 {
        "(-9223372036854775807LL - 1)".to_string()
    } else if value < 0 {
        format!("(-{}LL)", -value)
    } else {
        format!("{value}LL")
    }
}

/// A C single-precision literal (`{:e}` is Rust's shortest round-trip form).
fn c_f32_literal(value: f32) -> String {
    if value.is_nan() {
        "((float)NAN)".to_string()
    } else if value.is_infinite() {
        format!("((float){}INFINITY)", if value < 0.0 { "-" } else { "" })
    } else {
        format!("((float){value:e}f)")
    }
}

fn c_sized_literal(value: i128, kind: IntKind) -> String {
    format!("(({}){})", kind.c_type(), c_int_literal(value))
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
        CType::Map(k, v) => format!("Map_{}_{}", mangle_ctype(k), mangle_ctype(v)),
        CType::Set(t) => format!("Set_{}", mangle_ctype(t)),
        CType::Sized(kind) => kind.name().to_string(),
        CType::Float32 => "Float32".to_string(),
        CType::Array(t) => format!("Array_{}", mangle_ctype(t)),
        CType::Channel(t) => format!("Channel_{}", mangle_ctype(t)),
        CType::Task(t) => format!("Task_{}", mangle_ctype(t)),
        CType::Option(inner) => format!("Option_{}", mangle_ctype(inner)),
        CType::NoneLit => "None".to_string(),
        CType::GenLit(base, variant) => format!("Lit_{base}_{variant}"),
        CType::Quantity(d) => format!("Q_{}", dim_to_string(d).chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect::<String>()),
        CType::OkLit(t) => format!("Ok_{}", mangle_ctype(t)),
        CType::ErrLit(t) => format!("Err_{}", mangle_ctype(t)),
        CType::Result(t, e) => format!("Result_{}_{}", mangle_ctype(t), mangle_ctype(e)),
    }
}

impl<'a> Codegen<'a> {
    fn named_types(&self) -> NamedTypes<'_> {
        NamedTypes {
            records: &self.record_names,
            enums: &self.enum_names,
            traits: &self.trait_names,
            generics: &self.generic_arity,
            seen: &self.seen_instances,
        }
    }

    /// Registers every generic instantiation `map_type` has logged since the
    /// last call: its fields/variants, and the methods of every `impl` that
    /// applies to it (queued like any other monomorphized function).
    fn flush_instances(&mut self) -> Result<(), String> {
        loop {
            let batch: Vec<(String, Vec<CType>)> = self.seen_instances.borrow_mut().drain(..).collect();
            if batch.is_empty() {
                return Ok(());
            }
            for (base, args) in batch {
                self.register_instance(&base, &args)?;
            }
        }
    }

    /// An `impl`'s own methods plus the default methods of its trait that it
    /// doesn't override.
    fn impl_method_list(&self, im: &'a ImplDecl) -> Vec<&'a FunctionDecl> {
        let mut methods: Vec<&'a FunctionDecl> = im.methods.iter().collect();
        if let Some(defaults) = im.trait_name.as_ref().and_then(|t| self.trait_defaults.get(t)) {
            for d in defaults {
                if !im.methods.iter().any(|m| m.name == d.name) {
                    methods.push(d);
                }
            }
        }
        methods
    }

    /// Resolves a written type in the context of the function body being
    /// generated (so `U` means whatever this instantiation bound it to).
    fn resolve_type(&self, ty: &Type) -> Result<CType, String> {
        match self.subst_stack.last() {
            Some(subst) => map_type_with_subst(ty, &self.named_types(), subst),
            None => map_type(ty, &self.named_types()),
        }
    }

    /// Registers the methods of one `impl` for one concrete type: plain
    /// methods go into `methods` (and, when `queue`, are queued for
    /// generation); methods with their own type parameters wait for a call.
    fn register_impl_methods(&mut self, im: &'a ImplDecl, key: &str, self_ty: &CType, binds: &HashMap<String, CType>, queue: bool) {
        for method in self.impl_method_list(im) {
            if !method.generics.is_empty() {
                self.generic_methods
                    .entry(key.to_string())
                    .or_default()
                    .insert(method.name.clone(), GenericMethod { decl: method, binds: binds.clone(), key: key.to_string() });
                continue;
            }
            let param_types: Result<Vec<CType>, String> =
                method.params.iter().map(|p| map_type_with_subst(&p.ty, &self.named_types(), binds)).collect();
            let Ok(param_types) = param_types else { continue };
            let Ok(return_type) = map_type_with_subst(&method.return_type, &self.named_types(), binds) else { continue };
            for ty in param_types.iter().chain(std::iter::once(&return_type)) {
                self.register_list_types(ty);
            }
            let c_name = format!("{key}__{}", method.name);
            self.methods.entry(key.to_string()).or_default().insert(
                method.name.clone(),
                MethodInfo { decl: method, param_types: param_types.clone(), return_type: return_type.clone(), c_name: c_name.clone(), self_ty: self_ty.clone() },
            );
            if queue {
                self.pending.push_back(PendingInstance { c_name, decl: method, subst: binds.clone(), param_types, return_type });
            }
        }
    }

    /// `impl Trait for Quantity<Length>` / `impl<D: Dimension> ... for
    /// Quantity<D>`: registered lazily per dimension, the first time a
    /// method is called on a quantity of that dimension.
    fn ensure_quantity_methods(&mut self, dim: &Dimension) {
        let self_ty = CType::Quantity(dim.clone());
        let key = mangle_ctype(&self_ty);
        if !self.quantity_done.insert(key.clone()) {
            return;
        }
        for im in self.quantity_impls.clone() {
            let Some(arg) = im.type_args.first() else { continue };
            let mut binds: HashMap<String, CType> = HashMap::new();
            let matches = match arg {
                Type::Named(n, a) if a.is_empty() && im.generics.iter().any(|g| &g.name == n) => {
                    binds.insert(n.clone(), self_ty.clone());
                    true
                }
                other => &resolve_dimension(other, &HashMap::new()) == dim,
            };
            if !matches {
                continue;
            }
            binds.insert("Self".to_string(), self_ty.clone());
            self.register_impl_methods(im, &key, &self_ty, &binds, true);
        }
    }

    /// A call to a method with its own type parameters: inferred from the
    /// argument types (and any explicit `<...>`), then monomorphized.
    fn gen_generic_method_call(&mut self, gm: GenericMethod<'a>, obj_code: &str, type_args: Option<&[Type]>, args: &[Arg]) -> Result<(String, CType), String> {
        let decl = gm.decl;
        let generics: Vec<String> = decl.generics.iter().map(|g| g.name.clone()).collect();
        let (arg_codes, arg_types) = self.gen_args(args)?;
        if arg_codes.len() + 1 != decl.params.len() {
            return Err(format!("method '{}' expects {} argument(s), got {}", decl.name, decl.params.len() - 1, arg_codes.len()));
        }
        let mut subst: HashMap<String, CType> = HashMap::new();
        if let Some(types) = type_args {
            for (g, t) in generics.iter().zip(types) {
                subst.insert(g.clone(), self.resolve_type(t)?);
            }
        }
        for (param, arg_ty) in decl.params[1..].iter().zip(&arg_types) {
            self.bind_type(&param.ty, arg_ty, &generics, &mut subst, &decl.name)?;
        }
        if let Some(missing) = generics.iter().find(|g| !subst.contains_key(*g)) {
            return Err(format!("cannot infer type parameter '{missing}' of method '{}'; write it explicitly", decl.name));
        }
        let mut full = gm.binds.clone();
        full.extend(subst.clone());
        let suffix: Vec<String> = generics.iter().map(|g| mangle_ctype(&subst[g])).collect();
        let c_name = format!("{}__{}__{}", gm.key, decl.name, suffix.join("_"));
        let (param_types, return_type) = match self.instantiations.get(&c_name).cloned() {
            Some(sig) => sig,
            None => {
                let types = self.named_types();
                let param_types = decl.params.iter().map(|p| map_type_with_subst(&p.ty, &types, &full)).collect::<Result<Vec<_>, _>>()?;
                let return_type = map_type_with_subst(&decl.return_type, &types, &full)?;
                for ty in param_types.iter().chain(std::iter::once(&return_type)) {
                    self.register_list_types(ty);
                }
                self.flush_instances()?;
                self.instantiations.insert(c_name.clone(), (param_types.clone(), return_type.clone()));
                self.pending.push_back(PendingInstance { c_name: c_name.clone(), decl, subst: full, param_types: param_types.clone(), return_type: return_type.clone() });
                (param_types, return_type)
            }
        };
        let coerced = self.coerce_args(&arg_codes, &arg_types, &param_types[1..])?;
        let mut all = vec![obj_code.to_string()];
        all.extend(coerced);
        Ok((format!("{c_name}({})", all.join(", ")), return_type))
    }

    fn register_instance(&mut self, base: &str, args: &[CType]) -> Result<(), String> {
        let mangled = instance_name(base, args);
        if !self.instances_done.insert(mangled.clone()) {
            return Ok(());
        }
        self.instance_info.insert(mangled.clone(), (base.to_string(), args.to_vec()));
        let (is_enum, _) = self.generic_arity[base];
        let generics: Vec<String> = if is_enum {
            self.generic_enums[base].generics.iter().map(|g| g.name.clone()).collect()
        } else {
            self.generic_records[base].generics.iter().map(|g| g.name.clone()).collect()
        };
        let subst: HashMap<String, CType> = generics.iter().cloned().zip(args.iter().cloned()).collect();
        let self_ty;
        if is_enum {
            let decl = self.generic_enums[base];
            let mut infos = Vec::new();
            for (tag, variant) in decl.variants.iter().enumerate() {
                let mut fields = Vec::new();
                for (index, field) in variant.fields.iter().enumerate() {
                    let field_name = field.name.clone().unwrap_or_else(|| format!("f{index}"));
                    let ty = map_type_with_subst(&field.ty, &self.named_types(), &subst)?;
                    self.register_list_types(&ty);
                    fields.push((field_name, ty));
                }
                infos.push(VariantInfo { enum_name: mangled.clone(), name: variant.name.clone(), tag, fields });
            }
            self.instance_variants.insert(mangled.clone(), infos);
            self.enum_names.insert(mangled.clone());
            self_ty = CType::Enum(mangled.clone());
        } else {
            let decl = self.generic_records[base];
            let mut fields = Vec::new();
            for field in &decl.fields {
                let ty = map_type_with_subst(&field.ty, &self.named_types(), &subst)?;
                self.register_list_types(&ty);
                fields.push((field.name.clone(), ty));
            }
            self.records.insert(mangled.clone(), fields);
            self.record_names.insert(mangled.clone());
            self_ty = CType::Record(mangled.clone());
        }
        self.instance_order.push((is_enum, mangled.clone()));

        let impls: Vec<&'a ImplDecl> = self.generic_impls.iter().copied().filter(|im| im.type_name == base).collect();
        for im in impls {
            if im.type_args.len() != args.len() {
                continue;
            }
            let impl_generics: Vec<&str> = im.generics.iter().map(|g| g.name.as_str()).collect();
            let mut binds: HashMap<String, CType> = HashMap::new();
            let mut matches = true;
            for (ty, arg) in im.type_args.iter().zip(args) {
                match ty {
                    Type::Named(n, a) if a.is_empty() && impl_generics.contains(&n.as_str()) => {
                        if binds.get(n).is_some_and(|prev| prev != arg) {
                            matches = false;
                        }
                        binds.insert(n.clone(), arg.clone());
                    }
                    other => match map_type(other, &self.named_types()) {
                        Ok(concrete) if &concrete == arg => {}
                        _ => matches = false,
                    },
                }
            }
            if !matches {
                continue;
            }
            binds.insert("Self".to_string(), self_ty.clone());
            self.register_impl_methods(im, &mangled, &self_ty, &binds, true);
        }
        Ok(())
    }


    /// Infers `<T, U, ...>` from an explicit `<...>` list and/or the concrete
    /// types of the arguments at this call site — never from the return
    /// type, which this backend can't know ahead of time.
    fn infer_generic_substitutions(&self, decl: &FunctionDecl, explicit: Option<&[Type]>, arg_types: &[CType]) -> Result<HashMap<String, CType>, String> {
        let generic_names: Vec<String> = decl.generics.iter().map(|g| g.name.clone()).collect();
        let mut subst: HashMap<String, CType> = HashMap::new();
        if let Some(types) = explicit {
            for (generic, ty) in decl.generics.iter().zip(types) {
                let concrete = match map_type(ty, &self.named_types()) {
                    Ok(concrete) => concrete,
                    Err(error) if generic.bounds.iter().any(|b| b == "Dimension") => {
                        let _ = error;
                        CType::Quantity(resolve_dimension(ty, &HashMap::new()))
                    }
                    Err(error) => return Err(error),
                };
                subst.insert(generic.name.clone(), concrete);
            }
        }
        for (param, arg_ty) in decl.params.iter().zip(arg_types) {
            self.bind_type(&param.ty, arg_ty, &generic_names, &mut subst, &decl.name)?;
        }
        for generic in &decl.generics {
            if !subst.contains_key(&generic.name) {
                return Err(format!(
                    "cannot infer generic parameter '{}' of function '{}' from its arguments (only used in the return type, \
                     or bound only by a literal like 'None'; write it explicitly, e.g. '{}<Int>(...)')",
                    generic.name, decl.name, decl.name
                ));
            }
        }
        Ok(subst)
    }

    /// The instantiation `base<args...>` as a `CType`, registered right away.
    fn instance_type(&mut self, base: &str, args: Vec<CType>) -> Result<CType, String> {
        let (is_enum, _) = self.generic_arity[base];
        let mangled = instance_name(base, &args);
        self.seen_instances.borrow_mut().push((base.to_string(), args));
        self.flush_instances()?;
        Ok(if is_enum { CType::Enum(mangled) } else { CType::Record(mangled) })
    }

    /// Binds the type parameters in `generics` by walking a declared type
    /// against the concrete type an expression actually produced.
    fn bind_type(&self, ty: &Type, actual: &CType, generics: &[String], subst: &mut HashMap<String, CType>, owner: &str) -> Result<(), String> {
        if matches!(actual, CType::NoneLit | CType::OkLit(_) | CType::ErrLit(_) | CType::GenLit(..)) {
            return Ok(());
        }
        match (ty, actual) {
            (Type::Named(q, qargs), CType::Quantity(_)) if q == "Quantity" && qargs.len() == 1 => {
                if let Type::Named(d, dargs) = &qargs[0] {
                    if dargs.is_empty() && generics.contains(d) {
                        subst.entry(d.clone()).or_insert_with(|| actual.clone());
                    }
                }
                Ok(())
            }
            (Type::Named(name, args), _) if args.is_empty() && generics.contains(name) => match subst.get(name) {
                Some(existing) if existing != actual => Err(format!(
                    "generic parameter '{name}' of '{owner}' would need to be both '{}' and '{}' for these arguments",
                    mangle_ctype(existing),
                    mangle_ctype(actual)
                )),
                Some(_) => Ok(()),
                None => {
                    subst.insert(name.clone(), actual.clone());
                    Ok(())
                }
            },
            (Type::Named(n, args), CType::List(elem)) if n == "List" && args.len() == 1 => self.bind_type(&args[0], elem, generics, subst, owner),
            (Type::Named(n, args), CType::Map(k, v)) if n == "Map" && args.len() == 2 => {
                self.bind_type(&args[0], k, generics, subst, owner)?;
                self.bind_type(&args[1], v, generics, subst, owner)
            }
            (Type::Named(n, args), CType::Array(t)) if n == "Array" && args.len() == 1 => self.bind_type(&args[0], t, generics, subst, owner),
            (Type::Named(n, args), CType::Set(t)) if n == "Set" && args.len() == 1 => self.bind_type(&args[0], t, generics, subst, owner),
            (Type::Named(n, args), CType::Option(inner)) if n == "Option" && args.len() == 1 => self.bind_type(&args[0], inner, generics, subst, owner),
            (Type::Named(n, args), CType::Result(ok, err)) if n == "Result" && args.len() == 2 => {
                self.bind_type(&args[0], ok, generics, subst, owner)?;
                self.bind_type(&args[1], err, generics, subst, owner)
            }
            (Type::Named(n, args), CType::Record(inst) | CType::Enum(inst)) => {
                if let Some((base, inst_args)) = self.instance_info.get(inst) {
                    if base == n && inst_args.len() == args.len() {
                        for (a, c) in args.iter().zip(inst_args) {
                            self.bind_type(a, c, generics, subst, owner)?;
                        }
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The type arguments already known for a generic record/enum: an
    /// explicit `<...>` list, else the expected type's own arguments.
    fn seed_instance_subst(&self, base: &str, generics: &[String], explicit: Option<&[Type]>, hint: Option<&CType>) -> Result<HashMap<String, CType>, String> {
        let mut subst = HashMap::new();
        if let Some(types) = explicit {
            for (g, t) in generics.iter().zip(types) {
                subst.insert(g.clone(), self.resolve_type(t)?);
            }
        } else if let Some(CType::Enum(inst) | CType::Record(inst)) = hint {
            if let Some((b, inst_args)) = self.instance_info.get(inst) {
                if b == base {
                    for (g, a) in generics.iter().zip(inst_args) {
                        subst.insert(g.clone(), a.clone());
                    }
                }
            }
        }
        Ok(subst)
    }

    /// `Just(9)`, `Item<Int>(1)`, bare `Nothing`: a generic enum's variant.
    fn gen_generic_variant(&mut self, name: &str, explicit: Option<&[Type]>, args: &[Arg], hint: Option<CType>) -> Result<(String, CType), String> {
        self.flush_instances()?;
        let base = self.generic_variant_owner[name].clone();
        let decl: &'a EnumDecl = self.generic_enums[&base];
        let generics: Vec<String> = decl.generics.iter().map(|g| g.name.clone()).collect();
        let variant_decl = decl.variants.iter().find(|v| v.name == name).expect("variant owner");
        let field_names: Vec<String> =
            variant_decl.fields.iter().enumerate().map(|(i, f)| f.name.clone().unwrap_or_else(|| format!("f{i}"))).collect();
        let exprs = arrange_args(&field_names, name, args)?;
        let mut subst = self.seed_instance_subst(&base, &generics, explicit, hint.as_ref())?;
        let mut values: Vec<(String, CType)> = Vec::new();
        for (field, expr) in variant_decl.fields.iter().zip(&exprs) {
            let field_hint = map_type_with_subst(&field.ty, &self.named_types(), &subst).ok();
            let (code, ty) = self.gen_expr_hint(expr, field_hint)?;
            self.bind_type(&field.ty, &ty, &generics, &mut subst, &base)?;
            values.push((code, ty));
        }
        if generics.iter().any(|g| !subst.contains_key(g)) {
            if args.is_empty() {
                return Ok(("0".to_string(), CType::GenLit(base, name.to_string())));
            }
            return Err(format!("cannot infer the type parameters of '{name}' here; annotate it (e.g. '{name}<Int>(...)')"));
        }
        let inst_args: Vec<CType> = generics.iter().map(|g| subst[g].clone()).collect();
        let CType::Enum(inst) = self.instance_type(&base, inst_args)? else { unreachable!() };
        let info = self.instance_variants[&inst].iter().find(|v| v.name == name).cloned().expect("variant registered");
        let mut codes = Vec::new();
        for ((code, ty), (_, field_ty)) in values.iter().zip(&info.fields) {
            codes.push(self.coerce(code, ty, field_ty)?);
        }
        self.gen_variant_construct(&info, &codes)
    }

    /// `Pair { first: 4, second: 8 }` / `Score<Int> { ... }`.
    fn gen_generic_record_literal(&mut self, base: &str, explicit: Option<&[Type]>, fields: &[(String, Expr)], hint: Option<CType>) -> Result<(String, CType), String> {
        self.flush_instances()?;
        let decl: &'a RecordDecl = self.generic_records[base];
        let generics: Vec<String> = decl.generics.iter().map(|g| g.name.clone()).collect();
        let mut subst = self.seed_instance_subst(base, &generics, explicit, hint.as_ref())?;
        let mut values: Vec<(String, String, CType)> = Vec::new();
        for (field_name, expr) in fields {
            let Some(field) = decl.fields.iter().find(|f| &f.name == field_name) else {
                return Err(format!("record '{base}' has no field '{field_name}'"));
            };
            let field_hint = map_type_with_subst(&field.ty, &self.named_types(), &subst).ok();
            let (code, ty) = self.gen_expr_hint(expr, field_hint)?;
            self.bind_type(&field.ty, &ty, &generics, &mut subst, base)?;
            values.push((field_name.clone(), code, ty));
        }
        if let Some(missing) = generics.iter().find(|g| !subst.contains_key(*g)) {
            return Err(format!("cannot infer type parameter '{missing}' of record '{base}' here; write '{base}<...> {{ ... }}'"));
        }
        let inst_args: Vec<CType> = generics.iter().map(|g| subst[g].clone()).collect();
        let CType::Record(inst) = self.instance_type(base, inst_args)? else { unreachable!() };
        let temp = self.next_temp();
        let mut body = format!(
            "{inst}* {temp} = ({inst}*)malloc(sizeof({inst})); \
             if (!{temp}) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }} "
        );
        for (field_name, code, ty) in values {
            let field_ty = self.field_type(&inst, &field_name).expect("field checked above");
            let code = self.coerce(&code, &ty, &field_ty)?;
            body.push_str(&format!("{temp}->{field_name} = {code}; "));
        }
        Ok((format!("({{ {body} {temp}; }})"), CType::Record(inst)))
    }

    fn gen_expr_hint(&mut self, expr: &Expr, hint: Option<CType>) -> Result<(String, CType), String> {
        self.expected = hint;
        let result = self.gen_expr(expr);
        self.expected = None;
        result
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
        if let (CType::GenLit(base, variant), CType::Enum(inst)) = (from, to) {
            self.flush_instances()?;
            if self.instance_info.get(inst).is_some_and(|(b, _)| b == base) {
                if let Some(info) = self.instance_variants.get(inst).and_then(|vs| vs.iter().find(|v| &v.name == variant)) {
                    return Ok(format!("(({inst}){{ .tag = {} }})", info.tag));
                }
            }
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
        if let CType::Map(k, v) = ty {
            self.register_list_types(k);
            self.register_list_types(v);
            self.ensure_option(v);
            self.ensure_list(k);
            self.ensure_list(v);
            if self.coll_done.insert(mangle_ctype(ty)) {
                self.pending_colls.push_back(ty.clone());
            }
        }
        if let CType::Channel(t) | CType::Task(t) = ty {
            self.register_list_types(t);
            if matches!(ty, CType::Channel(_)) {
                self.ensure_option(t);
            }
            if self.coll_done.insert(mangle_ctype(ty)) {
                self.pending_colls.push_back(ty.clone());
            }
        }
        if let CType::Array(t) = ty {
            self.register_list_types(t);
            // The runtime returns/consumes these list types.
            let rows = CType::List(t.clone());
            self.ensure_list(&CType::Int);
            self.ensure_list(t);
            self.ensure_list(&rows);
            self.ensure_list(&CType::List(Box::new(rows.clone())));
            if self.coll_done.insert(mangle_ctype(ty)) {
                self.pending_colls.push_back(ty.clone());
            }
        }
        if let CType::Set(t) = ty {
            self.register_list_types(t);
            if self.coll_done.insert(mangle_ctype(ty)) {
                self.pending_colls.push_back(ty.clone());
            }
        }
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
        self.compare_enabled = true;
        self.push_scope();
        self.subst_stack.push(subst.clone());
        self.current_return.push(return_type.clone());
        for param in params {
            let ty = map_type_with_subst(&param.ty, &self.named_types(), subst)?;
            self.define(&param.name, ty);
        }
        self.flush_instances()?;
        for stmt in &body.stmts {
            self.gen_stmt(&stmt.stmt, out)?;
        }
        match &body.tail {
            Some(e) => {
                let (code, ty) = self.gen_expr_hint(e, Some(return_type.clone()))?;
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
        self.subst_stack.pop();
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

    /// A bare `Ok(x)` / `Err(e)` / `None` bound to a name with no annotation
    /// has only half a type. The unknown half can never be observed by a
    /// program that type-checked without ever supplying it, so it is
    /// filled with a harmless `Int` placeholder.
    fn settle_literal(&mut self, ty: CType, code: String) -> Result<(String, CType), String> {
        let target = match &ty {
            CType::OkLit(t) => CType::Result(t.clone(), Box::new(CType::Int)),
            CType::ErrLit(e) => CType::Result(Box::new(CType::Int), e.clone()),
            CType::NoneLit => CType::Option(Box::new(CType::Int)),
            _ => return Ok((code, ty)),
        };
        self.register_list_types(&target);
        let code = self.coerce(&code, &ty, &target)?;
        Ok((code, target))
    }

    fn gen_stmt(&mut self, stmt: &Stmt, out: &mut String) -> Result<(), String> {
        match stmt {
            Stmt::Binding { name, ty: declared, value, .. } => {
                // A list literal bound to an explicit `List<dyn Trait>` needs
                // its elements boxed one by one, so it must know the element
                // type it is expected to produce before generating them.
                let expected_elem = match (declared, value.unlocated()) {
                    (Some(declared_ty), Expr::ListLiteral(_)) => match self.resolve_type(declared_ty) {
                        Ok(CType::List(elem)) => Some(*elem),
                        _ => None,
                    },
                    _ => None,
                };
                let (code, actual_ty) = match (&expected_elem, value.unlocated()) {
                    (Some(elem), Expr::ListLiteral(items)) => self.gen_list_literal(items, Some(elem))?,
                    _ => {
                        let hint = declared.as_ref().and_then(|t| self.resolve_type(t).ok());
                        self.gen_expr_hint(value, hint)?
                    }
                };
                // An explicit `name: dyn Trait = ConcreteRecord { ... }`
                // needs boxing right here — with no annotation, `ty` is
                // just whatever the value already produced.
                let (final_ty, code) = match declared {
                    Some(declared_ty) => {
                        let declared_ctype = self.resolve_type(declared_ty)?;
                        self.register_list_types(&declared_ctype);
                        let coerced = self.coerce(&code, &actual_ty, &declared_ctype)?;
                        (declared_ctype, coerced)
                    }
                    None => {
                        let (code, ty) = self.settle_literal(actual_ty, code)?;
                        (ty, code)
                    }
                };
                out.push_str(&format!("    {} {} = {};\n", c_type_name(&final_ty), name, code));
                self.define(name, final_ty);
            }
            Stmt::Assign { name, value } => {
                let existing = self.lookup(name);
                let (code, ty) = self.gen_expr_hint(value, existing.clone())?;
                let (code, ty) = match existing {
                    Some(existing_ty) => (self.coerce(&code, &ty, &existing_ty)?, existing_ty),
                    None => self.settle_literal(ty, code)?,
                };
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
                    let hint = self.current_return.last().cloned();
                    let (code, ty) = self.gen_expr_hint(e, hint)?;
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
                let field_ty = self.field_type(record_name, field_name).expect("checked above");
                let (value_code, value_ty) = self.gen_expr_hint(value, Some(field_ty.clone()))?;
                let value_code = self.coerce(&value_code, &value_ty, &field_ty)?;
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
        if let CType::Channel(elem_ty) = &iter_ty {
            let ch = self.next_temp();
            out.push_str(&format!("    {{\n        {} {ch} = {iter_code};\n", c_type_name(&iter_ty)));
            out.push_str(&format!("        while ({ch}->head < {ch}->length) {{\n            {} {pattern} = {ch}->items[{ch}->head++];\n", c_type_name(elem_ty)));
            self.push_scope();
            self.define(pattern, (**elem_ty).clone());
            self.gen_block_stmts(body, out)?;
            self.pop_scope();
            out.push_str("        }\n    }\n");
            return Ok(());
        }
        // Iterator protocol: a record with a `next(mut self) -> Option<T>`
        // method is polled until it returns `None`.
        if let CType::Record(record_name) = &iter_ty {
            let next = self.methods.get(record_name).and_then(|m| m.get("next")).map(|m| (m.c_name.clone(), m.return_type.clone()));
            if let Some((c_name, CType::Option(elem_ty))) = next {
                let iter_temp = self.next_temp();
                let item_temp = self.next_temp();
                let option_c = c_type_name(&CType::Option(elem_ty.clone()));
                out.push_str(&format!("    {{
        {} {iter_temp} = {iter_code};
        for (;;) {{
", c_type_name(&iter_ty)));
                out.push_str(&format!("            {option_c} {item_temp} = {c_name}({iter_temp});
            if (!{item_temp}.has) break;
"));
                out.push_str(&format!("            {} {pattern} = {item_temp}.value;
", c_type_name(&elem_ty)));
                self.push_scope();
                self.define(pattern, *elem_ty);
                self.gen_block_stmts(body, out)?;
                self.pop_scope();
                out.push_str("        }
    }
");
                return Ok(());
            }
        }
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

    /// Records whether the backend's inferred type for `expr` agrees with the
    /// checker's. Observation only: never changes what is generated.
    fn compare_with_checker(&mut self, expr: &Expr, ty: &CType) {
        let Some(types) = self.checker_types else { return };
        let Expr::Located(_, range) = expr else { return };
        if !self.compare_enabled {
            self.type_report.unchecked += 1;
            return;
        }
        let key = ExprKey { file: self.current_file.clone(), start: range.start, end: range.end };
        let Some(checker_ty) = types.get(&key) else {
            // Literals in `match` patterns are never inferred by the checker;
            // that is not a gap in its expression typing.
            let mut node = expr;
            while let Expr::Located(inner, _) = node {
                node = inner;
            }
            if !matches!(node, Expr::IntLiteral(_)) {
                self.type_report.unchecked += 1;
            }
            return;
        };
        if crate::types::ty_contains_unknown(checker_ty) || matches!(checker_ty, Ty::Fn(..)) {
            self.type_report.unchecked += 1;
        } else if matches!(ty, CType::NoneLit | CType::OkLit(_) | CType::ErrLit(_) | CType::GenLit(..)) {
            self.type_report.partial += 1;
        } else if self.ctype_agrees(checker_ty, ty) {
            self.type_report.agreed += 1;
        } else {
            let file = self.current_file.clone().unwrap_or_default();
            self.type_report.divergences.push(format!(
                "{file}:{}:{}: checker says '{}', native says '{}'",
                range.start.line,
                range.start.col,
                checker_ty.describe(),
                mangle_ctype(ty)
            ));
        }
    }

    /// For a constructor expression (a call, a bare variant name, a record
    /// literal), the checker's full type for it: the authoritative source for
    /// the type arguments of a generic record/enum, so they need not be
    /// re-derived from the arguments.
    fn checker_hint(&mut self, expr: &Expr) -> Option<CType> {
        let types = self.checker_types?;
        if !self.compare_enabled {
            return None;
        }
        let Expr::Located(inner, range) = expr else { return None };
        if !matches!(inner.as_ref(), Expr::Call(..) | Expr::GenericCall(..) | Expr::Ident(_) | Expr::RecordLiteral(..) | Expr::GenericRecordLiteral(..)) {
            return None;
        }
        let key = ExprKey { file: self.current_file.clone(), start: range.start, end: range.end };
        let checker_ty = types.get(&key)?;
        if crate::types::ty_contains_unknown(checker_ty) || !matches!(checker_ty, Ty::Applied(..)) {
            return None;
        }
        let ctype = self.ty_to_ctype(checker_ty)?;
        matches!(ctype, CType::Record(_) | CType::Enum(_)).then_some(ctype)
    }

    /// A literal the backend can only type partially (`None`, `Ok(x)`, a bare
    /// `Nothing`) is completed with the checker's full type for that very
    /// expression, instead of waiting for a parent to supply a hint.
    fn complete_from_checker(&mut self, expr: &Expr, result: (String, CType)) -> Result<(String, CType), String> {
        let (code, ty) = result;
        if !matches!(ty, CType::NoneLit | CType::OkLit(_) | CType::ErrLit(_) | CType::GenLit(..)) || !self.compare_enabled {
            return Ok((code, ty));
        }
        let (Some(types), Expr::Located(_, range)) = (self.checker_types, expr) else { return Ok((code, ty)) };
        let key = ExprKey { file: self.current_file.clone(), start: range.start, end: range.end };
        let Some(checker_ty) = types.get(&key) else { return Ok((code, ty)) };
        if crate::types::ty_contains_unknown(checker_ty) {
            return Ok((code, ty));
        }
        let Some(full) = self.ty_to_ctype(checker_ty) else { return Ok((code, ty)) };
        self.register_list_types(&full);
        self.flush_instances()?;
        let completed = self.coerce(&code, &ty, &full)?;
        self.type_report.completed += 1;
        Ok((completed, full))
    }

    /// The backend's type for a fully known checker type (the inverse of
    /// `ctype_agrees`); `None` when the backend has no representation.
    fn ty_to_ctype(&mut self, ty: &Ty) -> Option<CType> {
        Some(match ty {
            Ty::Int => CType::Int,
            Ty::Float => CType::Float,
            Ty::Bool => CType::Bool,
            Ty::String => CType::Str,
            Ty::Void => CType::Void,
            Ty::Quantity(d) => CType::Quantity(self.substitute_dimension(d)),
            Ty::Sized(kind) => CType::Sized(*kind),
            Ty::Float32 => CType::Float32,
            Ty::Applied(n, args) if n == "Array" && args.len() == 1 => {
                let elem = self.ty_to_ctype(&args[0])?;
                matches!(elem, CType::Int | CType::Float | CType::Float32).then(|| CType::Array(Box::new(elem)))?
            }
            Ty::List(t) => CType::List(Box::new(self.ty_to_ctype(t)?)),
            Ty::Set(t) => CType::Set(Box::new(self.ty_to_ctype(t)?)),
            Ty::Map(k, v) => CType::Map(Box::new(self.ty_to_ctype(k)?), Box::new(self.ty_to_ctype(v)?)),
            Ty::Applied(n, args) if n == "Option" && args.len() == 1 => CType::Option(Box::new(self.ty_to_ctype(&args[0])?)),
            Ty::Applied(n, args) if n == "Result" && args.len() == 2 => {
                CType::Result(Box::new(self.ty_to_ctype(&args[0])?), Box::new(self.ty_to_ctype(&args[1])?))
            }
            Ty::Applied(n, args) if self.generic_arity.contains_key(n) => {
                let concrete = args.iter().map(|a| self.ty_to_ctype(a)).collect::<Option<Vec<_>>>()?;
                self.instance_type(n, concrete).ok()?
            }
            Ty::Named(n) if self.record_names.contains(n) => CType::Record(n.clone()),
            Ty::Named(n) if self.enum_names.contains(n) => CType::Enum(n.clone()),
            Ty::Dyn(t) => CType::DynTrait(t.clone()),
            Ty::Generic(name) => self.subst_stack.last()?.get(name)?.clone(),
            _ => return None,
        })
    }

    /// A checker dimension may mention this body's dimension parameters
    /// (`Quantity<D>`); replace each with the dimension it was bound to.
    fn substitute_dimension(&self, dim: &Dimension) -> Dimension {
        let Some(subst) = self.subst_stack.last() else { return dim.clone() };
        let mut out = Dimension::new();
        for (name, exponent) in dim {
            match subst.get(name) {
                Some(CType::Quantity(bound)) => out = dim_mul(&out, &dim_pow(bound, *exponent)),
                _ => out = dim_mul(&out, &HashMap::from([(name.clone(), *exponent)])),
            }
        }
        out
    }

    /// The checker's resolved type arguments for a call, as backend types
    /// (dimension parameters as `Quantity`), or `None` when the checker
    /// recorded none or one has no native representation.
    fn checker_call_subst(&mut self, decl: &FunctionDecl, key: &ExprKey) -> Option<HashMap<String, CType>> {
        let recorded = self.call_substs?.get(key)?.clone();
        let mut out = HashMap::new();
        for generic in &decl.generics {
            if let Some(dim) = recorded.dims.get(&generic.name) {
                out.insert(generic.name.clone(), CType::Quantity(self.substitute_dimension(dim)));
            } else {
                let ty = recorded.types.get(&generic.name)?;
                if crate::types::ty_contains_unknown(ty) {
                    return None;
                }
                out.insert(generic.name.clone(), self.ty_to_ctype(ty)?);
            }
        }
        Some(out)
    }

    fn ctype_agrees(&self, ty: &Ty, c: &CType) -> bool {
        match (ty, c) {
            (Ty::Int, CType::Int) | (Ty::Float, CType::Float) | (Ty::Bool, CType::Bool) | (Ty::String, CType::Str) | (Ty::Void, CType::Void) => true,
            (Ty::Char, _) => true,
            // Inside a monomorphized body a checker type parameter stands for
            // whatever this instantiation bound it to.
            (Ty::Generic(name), c) => self.subst_stack.last().and_then(|s| s.get(name)).is_none_or(|bound| bound == c),
            (Ty::Quantity(a), CType::Quantity(b)) => &self.substitute_dimension(a) == b,
            (Ty::Sized(a), CType::Sized(b)) => a == b,
            (Ty::Float32, CType::Float32) => true,
            (Ty::Applied(n, args), CType::Array(inner)) if n == "Array" && args.len() == 1 => self.ctype_agrees(&args[0], inner),
            (Ty::List(a), CType::List(b)) | (Ty::Set(a), CType::Set(b)) => self.ctype_agrees(a, b),
            (Ty::Map(k, v), CType::Map(ck, cv)) => self.ctype_agrees(k, ck) && self.ctype_agrees(v, cv),
            (Ty::Applied(n, args), CType::Option(inner)) if n == "Option" && args.len() == 1 => self.ctype_agrees(&args[0], inner),
            (Ty::Applied(n, args), CType::Result(ok, err)) if n == "Result" && args.len() == 2 => {
                self.ctype_agrees(&args[0], ok) && self.ctype_agrees(&args[1], err)
            }
            (Ty::Applied(n, args), CType::Channel(inner)) | (Ty::Applied(n, args), CType::Task(inner))
                if (n == "Channel" || n == "Task") && args.len() == 1 =>
            {
                self.ctype_agrees(&args[0], inner)
            }
            (Ty::Named(n), CType::Record(m) | CType::Enum(m)) => n == m,
            (Ty::Applied(n, args), CType::Record(m) | CType::Enum(m)) => {
                self.instance_info.get(m).is_some_and(|(base, cargs)| {
                    base == n && cargs.len() == args.len() && args.iter().zip(cargs).all(|(a, c)| self.ctype_agrees(a, c))
                })
            }
            (Ty::Dyn(t), CType::DynTrait(u)) => t == u,
            _ => false,
        }
    }

    /// Every expression is generated through here so that any generic
    /// instantiation its type mentions is registered before a parent looks
    /// up its fields, variants or methods.
    fn gen_expr(&mut self, expr: &Expr) -> Result<(String, CType), String> {
        let mut hint = self.expected.take();
        if let Some(literal) = self.typed_int_literal(expr) {
            return Ok(literal);
        }
        self.current_call_key = match expr {
            Expr::Located(inner, range) if matches!(inner.as_ref(), Expr::Call(callee, _) | Expr::GenericCall(callee, _, _) if matches!(callee.unlocated(), Expr::Ident(_))) => {
                Some(ExprKey { file: self.current_file.clone(), start: range.start, end: range.end })
            }
            _ => None,
        };
        if hint.is_none() {
            hint = self.checker_hint(expr);
        }
        let result = self.gen_expr_inner(expr, hint)?;
        self.compare_with_checker(expr, &result.1);
        let result = self.complete_from_checker(expr, result)?;
        self.flush_instances()?;
        Ok(result)
    }

    fn gen_expr_inner(&mut self, expr: &Expr, hint: Option<CType>) -> Result<(String, CType), String> {
        match expr.unlocated() {
            Expr::SizedIntLiteral(value, kind) => Ok((c_sized_literal(*value, *kind), CType::Sized(*kind))),
            Expr::IntLiteral(v) => Ok((format!("INT64_C({v})"), CType::Int)),
            // `{:?}` keeps `3.0` a C double literal (`{}` would print `3`, an int).
            Expr::FloatLiteral(v) => Ok((format!("{v:?}"), CType::Float)),
            Expr::Float32Literal(v) => Ok((c_f32_literal(*v), CType::Float32)),
            Expr::BoolLiteral(v) => Ok((if *v { "true".to_string() } else { "false".to_string() }, CType::Bool)),
            Expr::StringLiteral(s) => Ok((c_string_literal(s), CType::Str)),
            Expr::Ident(name) => {
                if let Some(ty) = self.lookup(name) {
                    return Ok((name.clone(), ty));
                }
                // A unit variant (`None`, or any fieldless variant of a
                // user enum) reads as a bare identifier, never a call — see
                // `Expr::Ident` in `eval_expr`, `interpreter/mod.rs`.
                if self.generic_variant_owner.contains_key(name) {
                    return self.gen_generic_variant(name, None, &[], hint);
                }
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
            Expr::Unary(UnaryOp::Neg, inner)
                if matches!(inner.unlocated(), Expr::SizedIntLiteral(v, k) if k.is_signed() && *v == -k.min()) =>
            {
                let Expr::SizedIntLiteral(_, kind) = inner.unlocated() else { unreachable!() };
                Ok((c_sized_literal(kind.min(), *kind), CType::Sized(*kind)))
            }
            Expr::Unary(op, inner) => {
                let (code, ty) = self.gen_expr(inner)?;
                match op {
                    UnaryOp::Neg if matches!(ty, CType::Quantity(_)) => {
                        let temp = self.next_temp();
                        Ok((format!("({{ Qty {temp} = {code}; {temp}.v = -{temp}.v; {temp}; }})"), ty))
                    }
                    UnaryOp::Neg if matches!(ty, CType::Array(_)) => Ok((format!("{}_neg({code})", mangle_ctype(&ty)), ty)),
                    UnaryOp::Neg if matches!(ty, CType::Sized(_)) => {
                        let CType::Sized(kind) = ty else { unreachable!() };
                        let temp = self.next_temp();
                        Ok((
                            format!("({{ {} {temp} = {code}; if ({temp} == {}) {{ {OVERFLOW_ABORT} }} ({}){}-{temp}; }})", kind.c_type(), c_int_literal(kind.min()), kind.c_type(), ""),
                            ty,
                        ))
                    }
                    UnaryOp::Neg => Ok((format!("(-{code})"), ty)),
                    UnaryOp::Not => Ok((format!("(!{code})"), CType::Bool)),
                }
            }
            Expr::Binary(op, l, r) => self.gen_binary(*op, l, r),
            Expr::Call(callee, args) => self.gen_call(callee, None, args, hint),
            Expr::GenericCall(callee, type_args, args) => self.gen_call(callee, Some(type_args), args, hint),
            Expr::GenericRecordLiteral(name, type_args, fields) => {
                if self.generic_records.contains_key(name) {
                    self.gen_generic_record_literal(name, Some(type_args), fields, hint)
                } else {
                    Err(format!("'{name}' isn't a generic record the native backend can compile"))
                }
            }
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
            Expr::RecordLiteral(name, fields) if self.generic_records.contains_key(name) => {
                self.gen_generic_record_literal(name, None, fields, hint)
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
            Expr::Spawn(block) => {
                let (code, ty) = self.gen_block_expr(block)?;
                let task_ty = CType::Task(Box::new(ty.clone()));
                self.register_list_types(&task_ty);
                let name = c_type_name(&task_ty);
                let temp = self.next_temp();
                Ok(if ty == CType::Void {
                    (format!("({{ {code}; ({name}){{ 0 }}; }})"), task_ty)
                } else {
                    (format!("({{ {} {temp} = {code}; ({name}){{ {temp} }}; }})", c_type_name(&ty)), task_ty)
                })
            }
            Expr::SpawnScope(block) => self.gen_block_expr(block),
            Expr::Channel(elem, _) => {
                let ty = CType::Channel(Box::new(self.resolve_type(elem)?));
                self.register_list_types(&ty);
                Ok((format!("{}_new()", mangle_ctype(&ty)), ty))
            }
            Expr::EmptyCollection(name, types) => {
                let ty = match (name.as_str(), types.as_slice()) {
                    ("Map", [k, v]) => CType::Map(Box::new(self.resolve_type(k)?), Box::new(self.resolve_type(v)?)),
                    ("Set", [t]) => CType::Set(Box::new(self.resolve_type(t)?)),
                    _ => return Err(format!("'{name}' has the wrong number of type arguments")),
                };
                self.register_list_types(&ty);
                Ok((format!("{}_new()", mangle_ctype(&ty)), ty))
            }
            Expr::SetLiteral(items) => {
                let mut parts = Vec::new();
                let mut elem_ty: Option<CType> = None;
                for item in items {
                    let (code, ty) = self.gen_expr(item)?;
                    elem_ty = Some(match &elem_ty {
                        None => ty.clone(),
                        Some(prev) => unify_types(prev, &ty),
                    });
                    parts.push((code, ty));
                }
                let elem_ty = elem_ty.ok_or("an empty set literal needs a type; use Set<T>()")?;
                let set_ty = CType::Set(Box::new(elem_ty.clone()));
                self.register_list_types(&set_ty);
                let name = mangle_ctype(&set_ty);
                let temp = self.next_temp();
                let mut body = format!("{name}* {temp} = {name}_new(); ");
                for (code, ty) in parts {
                    let code = self.coerce(&code, &ty, &elem_ty)?;
                    body.push_str(&format!("{name}_add({temp}, {code}); "));
                }
                Ok((format!("({{ {body} {temp}; }})"), set_ty))
            }
            Expr::MapLiteral(pairs) => {
                let mut parts = Vec::new();
                let (mut key_ty, mut val_ty): (Option<CType>, Option<CType>) = (None, None);
                for (k, v) in pairs {
                    let (kc, kt) = self.gen_expr(k)?;
                    let (vc, vt) = self.gen_expr(v)?;
                    key_ty = Some(match &key_ty {
                        None => kt.clone(),
                        Some(prev) => unify_types(prev, &kt),
                    });
                    val_ty = Some(match &val_ty {
                        None => vt.clone(),
                        Some(prev) => unify_types(prev, &vt),
                    });
                    parts.push((kc, kt, vc, vt));
                }
                let key_ty = key_ty.ok_or("an empty map literal needs a type; use Map<K, V>()")?;
                let val_ty = val_ty.expect("non-empty");
                let map_ty = CType::Map(Box::new(key_ty.clone()), Box::new(val_ty.clone()));
                self.register_list_types(&map_ty);
                let name = mangle_ctype(&map_ty);
                let temp = self.next_temp();
                let mut body = format!("{name}* {temp} = {name}_new(); ");
                for (kc, kt, vc, vt) in parts {
                    let kc = self.coerce(&kc, &kt, &key_ty)?;
                    let vc = self.coerce(&vc, &vt, &val_ty)?;
                    body.push_str(&format!("{name}_set({temp}, {kc}, {vc}); "));
                }
                Ok((format!("({{ {body} {temp}; }})"), map_ty))
            }
            Expr::Try(inner, handler) => self.gen_try(inner, handler.as_deref()),
            Expr::UnitLiteral(num, unit) => {
                let (code, ty) = self.gen_expr(num)?;
                let dim = resolve_unit_expr(unit).map_err(|u| format!("unknown unit '{u}'"))?;
                let v = self.as_f64_code(&code, &ty)?;
                Ok((format!("((Qty){{ {v}, {} }})", c_string_literal(unit)), CType::Quantity(dim)))
            }
            Expr::As(inner, unit_expr) if matches!(unit_expr.unlocated(), Expr::Ident(sym) if matches!(sym.as_str(), "Int" | "Int64" | "Float" | "Float64" | "Float32") || IntKind::from_name(sym).is_some()) => {
                let (code, ty) = self.gen_expr(inner)?;
                let Expr::Ident(target) = unit_expr.unlocated() else { unreachable!() };
                self.gen_numeric_conversion(&code, &ty, target)
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
                if let CType::Array(elem) = &obj_ty {
                    let (idx_code, _) = self.gen_expr(idx)?;
                    return Ok((format!("{}_index1({obj_code}, {idx_code})", mangle_ctype(&obj_ty)), (**elem).clone()));
                }
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
            let field_ty = self.field_type(name, field_name).expect("checked above");
            let (value_code, value_ty) = self.gen_expr_hint(value_expr, Some(field_ty.clone()))?;
            let value_code = self.coerce(&value_code, &value_ty, &field_ty)?;
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
            Pattern::Ident(name) if self.variant_for_pattern(name, scrutinee_ty).is_some_and(|v| v.fields.is_empty()) => {
                let variant = self.variant_for_pattern(name, scrutinee_ty).expect("checked by the guard");
                condition.push_str(&format!(" && ({scrutinee_var}.tag == {})", variant.tag));
                Ok(())
            }
            Pattern::Variant(name, fields) if self.record_pattern_target(name, scrutinee_ty).is_some() => {
                let record_name = self.record_pattern_target(name, scrutinee_ty).expect("checked by the guard");
                let declared = self.record_fields(&record_name).to_vec();
                for (position, (field_name, sub_pattern)) in fields.iter().enumerate() {
                    let resolved = declared.iter().find(|(n, _)| n == field_name).or_else(|| declared.get(position)).cloned();
                    let Some((actual_name, field_ty)) = resolved else {
                        return Err(format!("record '{name}' has no field matching '{field_name}'"));
                    };
                    self.gen_pattern(sub_pattern, &format!("{scrutinee_var}->{actual_name}"), &field_ty, condition, bindings)?;
                }
                Ok(())
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
                let Some(variant) = self.variant_for_pattern(name, scrutinee_ty) else {
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
                    let field_path = format!("{scrutinee_var}.data.{}.{actual_field_name}", variant.name);
                    self.gen_pattern(sub_pattern, &field_path, &field_ty, condition, bindings)?;
                }
                Ok(())
            }
        }
    }

    /// The variant a pattern's name refers to: for a generic enum's
    /// instance, that instance's own variants; otherwise the flat table.
    fn variant_for_pattern(&self, name: &str, scrutinee_ty: &CType) -> Option<VariantInfo> {
        if let CType::Enum(enum_name) = scrutinee_ty {
            if let Some(variants) = self.instance_variants.get(enum_name) {
                return variants.iter().find(|v| v.name == name).cloned();
            }
            return self.variants.get(name).filter(|v| &v.enum_name == enum_name).cloned();
        }
        None
    }

    /// `Pair(first: x, second: _)` destructures a record: returns the record
    /// type name if `name` is the scrutinee record's own (or its generic base's) name.
    fn record_pattern_target(&self, name: &str, scrutinee_ty: &CType) -> Option<String> {
        let CType::Record(record_name) = scrutinee_ty else { return None };
        let base = self.instance_info.get(record_name).map(|(b, _)| b.as_str()).unwrap_or(record_name.as_str());
        (base == name).then(|| record_name.clone())
    }

    fn has_derive(&self, type_name: &str, trait_name: &str) -> bool {
        let base = self.instance_info.get(type_name).map(|(b, _)| b.as_str()).unwrap_or(type_name);
        self.derives.get(base).is_some_and(|d| d.iter().any(|t| t == trait_name))
    }

    /// `==`/`<`/`+`... on a record or enum, dispatched like the interpreter's
    /// `eval_binary`: the user's `impl Add`/`impl Eq` method first, then
    /// `derive(Eq)`/`derive(Ord)`. (A hand-written `impl Ord` returns the
    /// built-in `Ordering`, which this backend doesn't represent yet.)
    fn gen_user_operator(&mut self, op: BinOp, lc: &str, lt: &CType, rc: &str, rt: &CType) -> Result<(String, CType), String> {
        let (CType::Record(name) | CType::Enum(name)) = lt else { unreachable!() };
        let method_name = match op {
            BinOp::Add => Some("add"),
            BinOp::Sub => Some("sub"),
            BinOp::Mul => Some("mul"),
            BinOp::Div => Some("div"),
            BinOp::Eq | BinOp::NotEq => Some("equals"),
            _ => None,
        };
        if let Some(method_name) = method_name {
            if let Some(info) = self.methods.get(name).and_then(|m| m.get(method_name)) {
                let (c_name, param_types, return_type) = (info.c_name.clone(), info.param_types.clone(), info.return_type.clone());
                let arg = self.coerce(rc, rt, param_types.get(1).unwrap_or(rt))?;
                let call = format!("{c_name}({lc}, {arg})");
                return Ok(if op == BinOp::NotEq { (format!("(!{call})"), CType::Bool) } else { (call, return_type) });
            }
        }
        if matches!(op, BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq) {
            if let Some(info) = self.methods.get(name).and_then(|m| m.get("compare")) {
                let (c_name, param_types) = (info.c_name.clone(), info.param_types.clone());
                let arg = self.coerce(rc, rt, param_types.get(1).unwrap_or(rt))?;
                let test = match op {
                    BinOp::Lt => "== 0",
                    BinOp::Gt => "== 2",
                    BinOp::LtEq => "!= 2",
                    _ => "!= 0",
                };
                return Ok((format!("({c_name}({lc}, {arg}).tag {test})"), CType::Bool));
            }
        }
        match op {
            BinOp::Eq | BinOp::NotEq if self.has_derive(name, "Eq") => {
                let eq = self.eq_expr(lc, rc, lt)?;
                Ok((if op == BinOp::Eq { eq } else { format!("(!{eq})") }, CType::Bool))
            }
            BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq if self.has_derive(name, "Ord") && matches!(lt, CType::Record(_)) => {
                let cmp = self.cmp_expr(lc, rc, lt)?;
                let c_op = match op {
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::LtEq => "<=",
                    _ => ">=",
                };
                Ok((format!("({cmp} {c_op} 0)"), CType::Bool))
            }
            _ => Err(format!("'{name}' doesn't implement the trait needed for this operator (in a form the native backend supports)")),
        }
    }

    /// A C boolean expression for `a == b` under the interpreter's rules.
    fn eq_expr(&mut self, a: &str, b: &str, ty: &CType) -> Result<String, String> {
        match ty {
            CType::Int | CType::Float | CType::Float32 | CType::Bool | CType::Sized(_) => Ok(format!("(({a}) == ({b}))")),
            CType::Str => Ok(format!("(strcmp({a}, {b}) == 0)")),
            CType::Quantity(_) => Ok(format!("(ostrin_qty_cmp({a}, {b}) == 0)")),
            CType::Record(n) | CType::Enum(n) => {
                if let Some(info) = self.methods.get(n).and_then(|m| m.get("equals")) {
                    return Ok(format!("{}({a}, {b})", info.c_name));
                }
                if !self.has_derive(n, "Eq") {
                    return Err(format!("'{n}' has no 'equals' method or derive(Eq) for '=='"));
                }
                if self.op_done.insert((false, n.clone())) {
                    self.op_queue.push_back((false, ty.clone()));
                }
                Ok(format!("ostrin_eq_{n}({a}, {b})"))
            }
            other => Err(format!("cannot compare values of type '{}' with '==' yet", c_type_name(other))),
        }
    }

    /// A C `int` expression: negative, zero or positive, like `compare`.
    fn cmp_expr(&mut self, a: &str, b: &str, ty: &CType) -> Result<String, String> {
        match ty {
            CType::Int | CType::Float | CType::Float32 | CType::Bool | CType::Sized(_) => Ok(format!("((({a}) < ({b})) ? -1 : ((({a}) > ({b})) ? 1 : 0))")),
            CType::Str => Ok(format!("strcmp({a}, {b})")),
            CType::Quantity(_) => Ok(format!("ostrin_qty_cmp({a}, {b})")),
            CType::Record(n) if self.has_derive(n, "Ord") => {
                if self.op_done.insert((true, n.clone())) {
                    self.op_queue.push_back((true, ty.clone()));
                }
                Ok(format!("ostrin_cmp_{n}({a}, {b})"))
            }
            other => Err(format!("cannot order values of type '{}' (needs a record with derive(Ord))", c_type_name(other))),
        }
    }

    /// Body of a generated `ostrin_eq_<T>` / `ostrin_cmp_<T>` helper.
    fn gen_op_body(&mut self, is_compare: bool, ty: &CType) -> Result<String, String> {
        let mut out = String::new();
        match (is_compare, ty) {
            (false, CType::Record(n)) => {
                let mut terms = Vec::new();
                for (f, fty) in self.record_fields(n).to_vec() {
                    terms.push(self.eq_expr(&format!("a->{f}"), &format!("b->{f}"), &fty)?);
                }
                out.push_str(&format!("    return {};\n", if terms.is_empty() { "true".to_string() } else { terms.join(" && ") }));
            }
            (false, CType::Enum(n)) => {
                let variants: Vec<VariantInfo> = match self.instance_variants.get(n) {
                    Some(vs) => vs.clone(),
                    None => self.variants.values().filter(|v| &v.enum_name == n).cloned().collect(),
                };
                out.push_str("    if (a.tag != b.tag) return false;\n");
                for v in variants.iter().filter(|v| !v.fields.is_empty()) {
                    let mut terms = Vec::new();
                    for (f, fty) in &v.fields {
                        terms.push(self.eq_expr(&format!("a.data.{}.{f}", v.name), &format!("b.data.{}.{f}", v.name), fty)?);
                    }
                    out.push_str(&format!("    if (a.tag == {}) return {};\n", v.tag, terms.join(" && ")));
                }
                out.push_str("    return true;\n");
            }
            (true, CType::Record(n)) => {
                out.push_str("    int c;\n");
                for (f, fty) in self.record_fields(n).to_vec() {
                    let cmp = self.cmp_expr(&format!("a->{f}"), &format!("b->{f}"), &fty)?;
                    out.push_str(&format!("    c = {cmp};\n    if (c != 0) return c;\n"));
                }
                out.push_str("    return 0;\n");
            }
            _ => unreachable!("only records and enums are queued"),
        }
        Ok(out)
    }

    /// An integer literal the checker typed as fixed-width (`a * 1` with `a:
    /// UInt8`): its real C type, taken from the checker's decision.
    fn typed_int_literal(&self, expr: &Expr) -> Option<(String, CType)> {
        let kinds = self.literal_kinds?;
        if kinds.is_empty() {
            return None;
        }
        let mut node = expr;
        let mut range = None;
        while let Expr::Located(inner, r) = node {
            range = Some(*r);
            node = inner;
        }
        let range = range?;
        let key = ExprKey { file: self.current_file.clone(), start: range.start, end: range.end };
        match (node, *kinds.get(&key)?) {
            (Expr::IntLiteral(n), LitKind::Int(kind)) => Some((c_sized_literal(*n as i128, kind), CType::Sized(kind))),
            (Expr::FloatLiteral(f), LitKind::F32) => Some((c_f32_literal(*f as f32), CType::Float32)),
            _ => None,
        }
    }

    /// `+ - * /` (overflow-checked) and comparisons on fixed-width integers.
    fn gen_sized_binary(&mut self, op: BinOp, lc: &str, lt: &CType, rc: &str, rt: &CType) -> Result<(String, CType), String> {
        let (CType::Sized(kind), CType::Sized(other)) = (lt, rt) else {
            return Err("fixed-width integers can't be mixed with other types here; the checker should have rejected this".to_string());
        };
        if kind != other {
            return Err("mismatched fixed-width integer types".to_string());
        }
        let c = kind.c_type();
        let (a, b) = (self.next_temp(), self.next_temp());
        let decl = format!("{c} {a} = {lc}; {c} {b} = {rc};");
        let checked = |builtin: &str| {
            let r = "__r";
            format!("({{ {decl} {c} {r}; if ({builtin}({a}, {b}, &{r})) {{ {OVERFLOW_ABORT} }} {r}; }})")
        };
        Ok(match op {
            BinOp::Add => (checked("__builtin_add_overflow"), lt.clone()),
            BinOp::Sub => (checked("__builtin_sub_overflow"), lt.clone()),
            BinOp::Mul => (checked("__builtin_mul_overflow"), lt.clone()),
            BinOp::Div => (
                format!(
                    "({{ {decl} if ({b} == 0) {{ fprintf(stderr, \"runtime error: division by zero\\n\"); exit(1); }} \
                     __int128 __q = (__int128){a} / (__int128){b}; \
                     if (__q < (__int128){} || __q > (__int128){}) {{ {OVERFLOW_ABORT} }} ({c})__q; }})",
                    c_int_literal(kind.min()),
                    c_int_literal(kind.max())
                ),
                lt.clone(),
            ),
            BinOp::Eq => (format!("(({lc}) == ({rc}))"), CType::Bool),
            BinOp::NotEq => (format!("(({lc}) != ({rc}))"), CType::Bool),
            BinOp::Lt => (format!("(({lc}) < ({rc}))"), CType::Bool),
            BinOp::Gt => (format!("(({lc}) > ({rc}))"), CType::Bool),
            BinOp::LtEq => (format!("(({lc}) <= ({rc}))"), CType::Bool),
            BinOp::GtEq => (format!("(({lc}) >= ({rc}))"), CType::Bool),
            BinOp::And | BinOp::Or => return Err("logical operators need Bool operands".to_string()),
        })
    }

    /// `x as UInt8` / `as Int` / `as Float`: explicit and range-checked, like the interpreter's.
    fn gen_numeric_conversion(&mut self, code: &str, from: &CType, target: &str) -> Result<(String, CType), String> {
        if !matches!(from, CType::Int | CType::Float | CType::Float32 | CType::Sized(_)) {
            return Err(format!("cannot convert '{}' with 'as'", mangle_ctype(from)));
        }
        if target == "Float32" {
            return Ok((format!("((float)({code}))"), CType::Float32));
        }
        // A `Float32` converts exactly like the `Float` holding the same value.
        let widened;
        let (code, from) = if *from == CType::Float32 {
            widened = format!("((double)({code}))");
            (widened.as_str(), &CType::Float)
        } else {
            (code, from)
        };
        if target == "Float" || target == "Float64" {
            return Ok((format!("((double)({code}))"), CType::Float));
        }
        let (to_ty, min, max, c_name) = match IntKind::from_name(target) {
            Some(kind) => (CType::Sized(kind), kind.min(), kind.max(), kind.c_type()),
            None => (CType::Int, i64::MIN as i128, i64::MAX as i128, "int64_t"),
        };
        let temp = self.next_temp();
        let (lo, hi) = (c_int_literal(min), c_int_literal(max));
        let fail = "fprintf(stderr, \"runtime error: value does not fit in the target integer type\\n\"); exit(1);";
        let converted = if *from == CType::Float {
            // Truncate toward zero, then range-check: `d <= min - 1` (below range) or `d >= max + 1`.
            let low_check = if min == 0 { "-1.0".to_string() } else if min == i64::MIN as i128 { "-9223372036854775809.0".to_string() } else { format!("(double)({})", c_int_literal(min - 1)) };
            let low_cmp = if min == i64::MIN as i128 { "<" } else { "<=" };
            let high = format!("{}.0", max + 1);
            format!("({{ double {temp} = {code}; if ({temp} != {temp} || {temp} {low_cmp} {low_check} || {temp} >= {high}) {{ {fail} }} ({c_name}){temp}; }})")
        } else {
            format!("({{ __int128 {temp} = (__int128)({code}); if ({temp} < (__int128){lo} || {temp} > (__int128){hi}) {{ {fail} }} ({c_name}){temp}; }})")
        };
        Ok((converted, to_ty))
    }

    fn gen_binary(&mut self, op: BinOp, l: &Expr, r: &Expr) -> Result<(String, CType), String> {
        let (lc, lt) = self.gen_expr(l)?;
        let (rc, rt) = self.gen_expr(r)?;
        if matches!(lt, CType::Quantity(_)) || matches!(rt, CType::Quantity(_)) {
            return self.gen_quantity_binary(op, &lc, &lt, &rc, &rt);
        }
        if matches!(lt, CType::Array(_)) || matches!(rt, CType::Array(_)) {
            let code = match op {
                BinOp::Add => 0,
                BinOp::Sub => 1,
                BinOp::Mul => 2,
                BinOp::Div => 3,
                _ => return Err("only + - * / are defined on arrays".to_string()),
            };
            return match (&lt, &rt) {
                (CType::Array(a), CType::Array(b)) if a == b => Ok((format!("{}_binop({lc}, {rc}, {code})", mangle_ctype(&lt)), lt.clone())),
                (CType::Array(a), scalar) if **a == *scalar => Ok((format!("{}_scalar({lc}, {rc}, {code}, 0)", mangle_ctype(&lt)), lt.clone())),
                (scalar, CType::Array(b)) if **b == *scalar => Ok((format!("{}_scalar({rc}, {lc}, {code}, 1)", mangle_ctype(&rt)), rt.clone())),
                _ => Err("array operands must have the same element type".to_string()),
            };
        }
        if matches!(lt, CType::Sized(_)) || matches!(rt, CType::Sized(_)) {
            return self.gen_sized_binary(op, &lc, &lt, &rc, &rt);
        }
        if matches!(lt, CType::Float32) || matches!(rt, CType::Float32) {
            if lt != rt {
                return Err("Float32 can't be mixed with other types here; the checker should have rejected this".to_string());
            }
            let (c_op, result) = match op {
                BinOp::Add => ("+", CType::Float32),
                BinOp::Sub => ("-", CType::Float32),
                BinOp::Mul => ("*", CType::Float32),
                BinOp::Div => ("/", CType::Float32),
                BinOp::Eq => ("==", CType::Bool),
                BinOp::NotEq => ("!=", CType::Bool),
                BinOp::Lt => ("<", CType::Bool),
                BinOp::Gt => (">", CType::Bool),
                BinOp::LtEq => ("<=", CType::Bool),
                BinOp::GtEq => (">=", CType::Bool),
                BinOp::And | BinOp::Or => return Err("logical operators need Bool operands".to_string()),
            };
            let code = if result == CType::Float32 { format!("((float)(({lc}) {c_op} ({rc})))") } else { format!("(({lc}) {c_op} ({rc}))") };
            return Ok((code, result));
        }
        if matches!(lt, CType::Record(_) | CType::Enum(_)) && !matches!(op, BinOp::And | BinOp::Or) {
            return self.gen_user_operator(op, &lc, &lt, &rc, &rt);
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

    fn gen_call(&mut self, callee: &Expr, type_args: Option<&[Type]>, args: &[Arg], hint: Option<CType>) -> Result<(String, CType), String> {
        match callee.unlocated() {
            Expr::Ident(name) => self.gen_function_call(name, type_args, args, hint),
            Expr::FieldAccess(obj, method_name) => self.gen_method_call(obj, method_name, type_args, args),
            _ => Err("only a direct function call or 'record.method(...)' is supported by the native backend yet".to_string()),
        }
    }

    fn gen_args(&mut self, args: &[Arg]) -> Result<(Vec<String>, Vec<CType>), String> {
        self.gen_args_hinted(args, &[])
    }

    /// Like `gen_args`, but each argument is generated knowing the type its
    /// parameter expects (see `Codegen::expected`).
    fn gen_args_hinted(&mut self, args: &[Arg], hints: &[CType]) -> Result<(Vec<String>, Vec<CType>), String> {
        let mut codes = Vec::new();
        let mut types = Vec::new();
        for (index, arg) in args.iter().enumerate() {
            let expr = match arg {
                Arg::Positional(e) => e,
                Arg::Named(_, _) => return Err("named arguments aren't supported by the native backend yet".to_string()),
            };
            let (code, ty) = self.gen_expr_hint(expr, hints.get(index).cloned())?;
            codes.push(code);
            types.push(ty);
        }
        Ok((codes, types))
    }

    fn gen_function_call(&mut self, name: &str, type_args: Option<&[Type]>, args: &[Arg], hint: Option<CType>) -> Result<(String, CType), String> {
        let call_key = self.current_call_key.take();
        if self.generic_variant_owner.contains_key(name) {
            return self.gen_generic_variant(name, type_args, args, hint);
        }
        if let Some(variant) = self.variants.get(name).cloned() {
            let arg_codes = self.gen_variant_args(&variant, args)?;
            return self.gen_variant_construct(&variant, &arg_codes);
        }
        if (name == "Map" || name == "Set") && args.is_empty() {
            if let Some(types) = type_args {
                let ty = match (name, types) {
                    ("Map", [k, v]) => CType::Map(Box::new(self.resolve_type(k)?), Box::new(self.resolve_type(v)?)),
                    ("Set", [t]) => CType::Set(Box::new(self.resolve_type(t)?)),
                    _ => return Err(format!("'{name}' has the wrong number of type arguments")),
                };
                self.register_list_types(&ty);
                return Ok((format!("{}_new()", mangle_ctype(&ty)), ty));
            }
        }
        if name == "None" && args.is_empty() {
            if let Some([t]) = type_args {
                let ty = CType::Option(Box::new(self.resolve_type(t)?));
                self.register_list_types(&ty);
                let code = self.coerce("0", &CType::NoneLit, &ty)?;
                return Ok((code, ty));
            }
        }
        if (name == "Ok" || name == "Err") && args.len() == 1 {
            if let Some([t, e]) = type_args {
                let ty = CType::Result(Box::new(self.resolve_type(t)?), Box::new(self.resolve_type(e)?));
                self.register_list_types(&ty);
                let expected_arg = if name == "Ok" { match &ty { CType::Result(o, _) => (**o).clone(), _ => unreachable!() } } else { match &ty { CType::Result(_, e) => (**e).clone(), _ => unreachable!() } };
                let (code, arg_ty) = self.gen_expr_hint(match &args[0] { Arg::Positional(e) => e, Arg::Named(_, e) => e }, Some(expected_arg.clone()))?;
                let value = self.coerce(&code, &arg_ty, &expected_arg)?;
                let lit = if name == "Ok" { CType::OkLit(Box::new(expected_arg)) } else { CType::ErrLit(Box::new(expected_arg)) };
                let code = self.coerce(&value, &lit, &ty)?;
                return Ok((code, ty));
            }
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
        let normalized;
        let mut args = args;
        if let Some(decl) = self.function_decls.get(name).copied() {
            if let Some(list) = normalize_call_args(&decl.params, args, 0)? {
                normalized = list;
                args = &normalized;
            }
        }
        let hints = self.signatures.get(name).map(|(params, _)| params.clone()).unwrap_or_default();
        let (arg_codes, arg_types) = self.gen_args_hinted(args, &hints)?;
        if name == "print" {
            return self.gen_print(&arg_codes, &arg_types);
        }
        if !self.function_decls.contains_key(name) {
            if let Some(result) = self.gen_builtin(name, &arg_codes, &arg_types)? {
                return Ok(result);
            }
        }
        if let Some(decl) = self.generic_functions.get(name).copied() {
            return self.gen_generic_call(decl, type_args, call_key, &arg_codes, &arg_types);
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
    fn gen_generic_call(&mut self, decl: &'a FunctionDecl, type_args: Option<&[Type]>, call_key: Option<ExprKey>, arg_codes: &[String], arg_types: &[CType]) -> Result<(String, CType), String> {
        if decl.params.len() != arg_codes.len() {
            return Err(format!("function '{}' expects {} argument(s), got {}", decl.name, decl.params.len(), arg_codes.len()));
        }
        // The checker already resolved this call's type arguments: use them.
        // The backend's own inference stays as a fallback (and as a cross-check).
        let from_checker = call_key.as_ref().and_then(|key| self.checker_call_subst(decl, key));
        let subst = match from_checker {
            Some(subst) => {
                self.type_report.calls_from_checker += 1;
                if let Ok(own) = self.infer_generic_substitutions(decl, type_args, arg_types) {
                    if own != subst {
                        let file = self.current_file.clone().unwrap_or_default();
                        self.type_report.divergences.push(format!("{file}: generic call to '{}': checker and backend resolved different type arguments", decl.name));
                    }
                }
                subst
            }
            None => {
                self.type_report.calls_inferred += 1;
                self.infer_generic_substitutions(decl, type_args, arg_types)?
            }
        };
        let mangled_suffix: Vec<String> =
            decl.generics.iter().map(|g| mangle_ctype(subst.get(&g.name).expect("checked by infer_generic_substitutions"))).collect();
        let c_name = format!("{}__{}", decl.name, mangled_suffix.join("_"));

        // `decl.params.len() == arg_codes.len()` was already checked above,
        // and every instantiation's param count always equals that, so a
        // cache hit needs no further arity check.
        if let Some((param_types, return_type)) = self.instantiations.get(&c_name).cloned() {
            let coerced = self.coerce_args(arg_codes, arg_types, &param_types)?;
            return Ok((format!("{c_name}({})", coerced.join(", ")), return_type));
        }

        let types = self.named_types();
        let param_types = decl.params.iter().map(|p| map_type_with_subst(&p.ty, &types, &subst)).collect::<Result<Vec<_>, _>>()?;
        let return_type = map_type_with_subst(&decl.return_type, &types, &subst)?;
        for ty in param_types.iter().chain(std::iter::once(&return_type)) {
            self.register_list_types(ty);
        }
        self.flush_instances()?;
        self.instantiations.insert(c_name.clone(), (param_types.clone(), return_type.clone()));
        let coerced = self.coerce_args(arg_codes, arg_types, &param_types)?;
        self.pending.push_back(PendingInstance { c_name: c_name.clone(), decl, subst, param_types, return_type: return_type.clone() });
        Ok((format!("{c_name}({})", coerced.join(", ")), return_type))
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

    fn gen_method_call(&mut self, obj: &Expr, method_name: &str, type_args: Option<&[Type]>, args: &[Arg]) -> Result<(String, CType), String> {
        let (obj_code, obj_ty) = self.gen_expr(obj)?;
        // `to_string()` exists on every scalar in the interpreter
        // (`receiver.to_string()` in `eval_call`); records/enums aren't
        // covered (their printed form needs a generated Display).
        if method_name == "to_string" && args.is_empty() {
            let text = match &obj_ty {
                CType::Int => Some(format!("ostrin_int_to_string({obj_code})")),
                CType::Sized(kind) if kind.is_signed() => Some(format!("ostrin_int_to_string({obj_code})")),
                CType::Sized(_) => Some(format!("ostrin_uint_to_string({obj_code})")),
                CType::Float32 => Some(format!("ostrin_single_to_string({obj_code})")),
                CType::Float => Some(format!("ostrin_float_to_string({obj_code})")),
                CType::Bool => Some(format!("(({obj_code}) ? \"true\" : \"false\")")),
                CType::Str => Some(obj_code.clone()),
                CType::Quantity(_) => Some(format!("ostrin_qty_to_string({obj_code})")),
                // Everything else prints through its generated `ostrin_show_*`.
                CType::Array(_) | CType::List(_) | CType::Map(..) | CType::Set(_) | CType::Option(_) | CType::Result(..) | CType::Record(_) | CType::Enum(_) => self.show_expr(&obj_code, &obj_ty).ok(),
                _ => None,
            };
            if let Some(text) = text {
                return Ok((text, CType::Str));
            }
        }
        if matches!(obj_ty, CType::Option(_) | CType::Result(..)) && matches!(method_name, "map" | "then" | "map_err") {
            return self.gen_wrapper_combinator(&obj_code, &obj_ty, method_name, args);
        }
        let type_key = match &obj_ty {
            CType::Record(n) | CType::Enum(n) => Some(n.clone()),
            CType::Quantity(dim) => {
                self.ensure_quantity_methods(dim);
                Some(mangle_ctype(&obj_ty))
            }
            _ => None,
        };
        match &obj_ty {
            CType::Record(_) | CType::Enum(_) | CType::Quantity(_) => {
                let record_name = &type_key.clone().expect("computed above");
                let Some(method) = self.methods.get(record_name).and_then(|methods| methods.get(method_name)) else {
                    if let Some(gm) = self.generic_methods.get(record_name).and_then(|m| m.get(method_name)).cloned() {
                        return self.gen_generic_method_call(gm, &obj_code, type_args, args);
                    }
                    return Err(format!(
                        "record '{record_name}' has no method '{method_name}' the native backend can compile \
                         (generic methods and methods on unsupported types aren't supported yet)"
                    ));
                };
                let (param_types, return_type, c_name) =
                    (method.param_types.clone(), method.return_type.clone(), method.c_name.clone());
                let decl = method.decl;
                let normalized = normalize_call_args(&decl.params, args, 1)?;
                let args = normalized.as_deref().unwrap_or(args);
                let (arg_codes, arg_types) = self.gen_args_hinted(args, param_types.get(1..).unwrap_or(&[]))?;
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
                    "ok" => {
                        let opt = CType::Option(Box::new(ok.clone()));
                        self.register_list_types(&opt);
                        let oc = c_type_name(&opt);
                        Ok((format!("({{ {rc} {temp} = {obj_code}; {oc} __r; memset(&__r, 0, sizeof __r); if ({temp}.ok) {{ __r.has = true; __r.value = {temp}.value; }} __r; }})"), opt))
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
                    "ok_or" => {
                        if arg_codes.len() != 1 {
                            return Err("'ok_or' expects one argument".to_string());
                        }
                        let result_ty = CType::Result(Box::new(inner.clone()), Box::new(arg_types[0].clone()));
                        self.register_list_types(&result_ty);
                        let rc = c_type_name(&result_ty);
                        Ok((
                            format!("({{ {oc} {temp} = {obj_code}; {rc} __r; memset(&__r, 0, sizeof __r); if ({temp}.has) {{ __r.ok = true; __r.value = {temp}.value; }} else {{ __r.ok = false; __r.error = {}; }} __r; }})", arg_codes[0]),
                            result_ty,
                        ))
                    }
                    other => Err(format!("Option has no method '{other}' the native backend supports yet")),
                }
            }
            CType::Map(k, v) => {
                let (k, v) = ((**k).clone(), (**v).clone());
                let name = mangle_ctype(&obj_ty);
                let hints: Vec<CType> = match method_name {
                    "set" => vec![k.clone(), v.clone()],
                    _ => vec![k.clone()],
                };
                let (codes, types) = self.gen_args_hinted(args, &hints)?;
                let expect = |n: usize| if codes.len() == n { Ok(()) } else { Err(format!("Map.{method_name} expects {n} argument(s)")) };
                match method_name {
                    "get" | "remove" => {
                        expect(1)?;
                        let key = self.coerce(&codes[0], &types[0], &k)?;
                        Ok((format!("{name}_{method_name}({obj_code}, {key})"), CType::Option(Box::new(v))))
                    }
                    "contains_key" => {
                        expect(1)?;
                        let key = self.coerce(&codes[0], &types[0], &k)?;
                        Ok((format!("{name}_contains_key({obj_code}, {key})"), CType::Bool))
                    }
                    "count" => Ok((format!("{name}_count({obj_code})"), CType::Int)),
                    "set" => {
                        expect(2)?;
                        let key = self.coerce(&codes[0], &types[0], &k)?;
                        let value = self.coerce(&codes[1], &types[1], &v)?;
                        Ok((format!("{name}_set({obj_code}, {key}, {value})"), CType::Void))
                    }
                    "keys" => Ok((format!("{name}_keys({obj_code})"), CType::List(Box::new(k)))),
                    "values" => Ok((format!("{name}_values({obj_code})"), CType::List(Box::new(v)))),
                    other => Err(format!("Map has no method '{other}' the native backend supports yet")),
                }
            }
            CType::Array(t) => {
                let t = (**t).clone();
                let n = mangle_ctype(&obj_ty);
                let (codes, types) = self.gen_args(args)?;
                let one = |this: &Self, want: usize| -> Result<(), String> {
                    let _ = this;
                    if codes.len() == want { Ok(()) } else { Err(format!("Array.{method_name} expects {want} argument(s)")) }
                };
                match method_name {
                    "shape" => Ok((format!("{n}_shape({obj_code})"), CType::List(Box::new(CType::Int)))),
                    "rank" => Ok((format!("{n}_rank({obj_code})"), CType::Int)),
                    "size" | "length" | "count" => Ok((format!("{n}_size({obj_code})"), CType::Int)),
                    "sum" | "min" | "max" => Ok((format!("{n}_{method_name}({obj_code})"), t)),
                    "mean" => Ok((format!("{n}_mean({obj_code})"), if t == CType::Int { CType::Float } else { t })),
                    "to_list" => Ok((format!("{n}_to_list({obj_code})"), CType::List(Box::new(t)))),
                    "transpose" => Ok((format!("{n}_transpose({obj_code})"), obj_ty.clone())),
                    "reshape" => {
                        one(self, 1)?;
                        Ok((format!("{n}_reshape({obj_code}, {})", codes[0]), obj_ty.clone()))
                    }
                    "sum_axis" => {
                        one(self, 1)?;
                        Ok((format!("{n}_sum_axis({obj_code}, {})", codes[0]), obj_ty.clone()))
                    }
                    "dot" => {
                        one(self, 1)?;
                        Ok((format!("{n}_dot({obj_code}, {})", codes[0]), t))
                    }
                    "matmul" => {
                        one(self, 1)?;
                        Ok((format!("{n}_matmul({obj_code}, {})", codes[0]), obj_ty.clone()))
                    }
                    "get" if !codes.is_empty() => Ok((format!("{n}_get({obj_code}, (int64_t[]){{ {} }}, {})", codes.join(", "), codes.len()), t)),
                    "set" if codes.len() >= 2 => {
                        let (value, indices) = codes.split_last().expect("checked above");
                        let value = self.coerce(value, &types[types.len() - 1], &t)?;
                        Ok((format!("{n}_set({obj_code}, (int64_t[]){{ {} }}, {}, {value})", indices.join(", "), indices.len()), CType::Void))
                    }
                    other => Err(format!("Array has no method '{other}' the native backend supports")),
                }
            }
            CType::Task(t) => {
                if method_name != "join" || !args.is_empty() {
                    return Err(format!("Task has no method '{method_name}' the native backend supports"));
                }
                if **t == CType::Void {
                    Ok((format!("({{ (void)({obj_code}); (void)0; }})"), CType::Void))
                } else {
                    Ok((format!("({obj_code}).value"), (**t).clone()))
                }
            }
            CType::Channel(t) => {
                let t = (**t).clone();
                let name = mangle_ctype(&obj_ty);
                let (codes, types) = self.gen_args_hinted(args, &[t.clone()])?;
                match method_name {
                    "send" if codes.len() == 1 => {
                        if matches!(t, CType::Record(_)) {
                            return Err("sending a record through a channel isn't supported by the native backend: the interpreter's 'moved after send' check (E1101) has no native equivalent yet".to_string());
                        }
                        let item = self.coerce(&codes[0], &types[0], &t)?;
                        Ok((format!("{name}_send({obj_code}, {item})"), CType::Void))
                    }
                    "receive" if codes.is_empty() => Ok((format!("{name}_receive({obj_code})"), CType::Option(Box::new(t)))),
                    "close" if codes.is_empty() => Ok((format!("({obj_code})->closed = true"), CType::Void)),
                    other => Err(format!("Channel has no method '{other}' the native backend supports yet")),
                }
            }
            CType::Set(t) => {
                let t = (**t).clone();
                let name = mangle_ctype(&obj_ty);
                let (codes, types) = self.gen_args_hinted(args, &[t.clone()])?;
                match method_name {
                    "contains" | "add" | "remove" if codes.len() == 1 => {
                        let item = self.coerce(&codes[0], &types[0], &t)?;
                        let ret = if method_name == "contains" { CType::Bool } else { CType::Void };
                        Ok((format!("{name}_{method_name}({obj_code}, {item})"), ret))
                    }
                    "count" => Ok((format!("{name}_count({obj_code})"), CType::Int)),
                    other => Err(format!("Set has no method '{other}' the native backend supports yet")),
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

    /// `map`/`then` on an `Option`, `map`/`map_err`/`then` on a `Result`,
    /// expanded inline (see `inline_lambda`).
    fn gen_wrapper_combinator(&mut self, obj_code: &str, obj_ty: &CType, method: &str, args: &[Arg]) -> Result<(String, CType), String> {
        let [arg] = args else { return Err(format!("'{method}' expects one lambda argument")) };
        let (names, body) = Self::lambda_of(arg, method)?;
        let temp = self.next_temp();
        let oc = c_type_name(obj_ty);
        match (obj_ty, method) {
            (CType::Option(inner), "map") => {
                let (code, ty) = self.inline_lambda(names, &[(**inner).clone()], body)?;
                let out_ty = CType::Option(Box::new(ty.clone()));
                self.register_list_types(&out_ty);
                let name = &names[0];
                Ok((
                    format!("({{ {oc} {temp} = {obj_code}; {} __r; memset(&__r, 0, sizeof __r); if ({temp}.has) {{ {} {name} = {temp}.value; __r.has = true; __r.value = {code}; }} __r; }})", c_type_name(&out_ty), c_type_name(inner)),
                    out_ty,
                ))
            }
            (CType::Option(inner), "then") => {
                let (code, ty) = self.inline_lambda(names, &[(**inner).clone()], body)?;
                let CType::Option(_) = ty else { return Err("'then' needs a lambda that returns an Option".to_string()) };
                let name = &names[0];
                Ok((
                    format!("({{ {oc} {temp} = {obj_code}; {} __r; memset(&__r, 0, sizeof __r); if ({temp}.has) {{ {} {name} = {temp}.value; __r = {code}; }} __r; }})", c_type_name(&ty), c_type_name(inner)),
                    ty,
                ))
            }
            (CType::Result(ok, err), "map") => {
                let (code, ty) = self.inline_lambda(names, &[(**ok).clone()], body)?;
                let out_ty = CType::Result(Box::new(ty), err.clone());
                self.register_list_types(&out_ty);
                let name = &names[0];
                Ok((
                    format!("({{ {oc} {temp} = {obj_code}; {} __r; memset(&__r, 0, sizeof __r); __r.ok = {temp}.ok; if ({temp}.ok) {{ {} {name} = {temp}.value; __r.value = {code}; }} else {{ __r.error = {temp}.error; }} __r; }})", c_type_name(&out_ty), c_type_name(ok)),
                    out_ty,
                ))
            }
            (CType::Result(ok, err), "map_err") => {
                let (code, ty) = self.inline_lambda(names, &[(**err).clone()], body)?;
                let out_ty = CType::Result(ok.clone(), Box::new(ty));
                self.register_list_types(&out_ty);
                let name = &names[0];
                Ok((
                    format!("({{ {oc} {temp} = {obj_code}; {} __r; memset(&__r, 0, sizeof __r); __r.ok = {temp}.ok; if ({temp}.ok) {{ __r.value = {temp}.value; }} else {{ {} {name} = {temp}.error; __r.error = {code}; }} __r; }})", c_type_name(&out_ty), c_type_name(err)),
                    out_ty,
                ))
            }
            (CType::Result(ok, err), "then") => {
                let (code, ty) = self.inline_lambda(names, &[(**ok).clone()], body)?;
                let CType::Result(..) = ty else { return Err("'then' needs a lambda that returns a Result".to_string()) };
                let name = &names[0];
                let _ = err;
                Ok((
                    format!("({{ {oc} {temp} = {obj_code}; {} __r; memset(&__r, 0, sizeof __r); if ({temp}.ok) {{ {} {name} = {temp}.value; __r = {code}; }} else {{ __r.ok = false; __r.error = {temp}.error; }} __r; }})", c_type_name(&ty), c_type_name(ok)),
                    ty,
                ))
            }
            _ => Err(format!("'{method}' isn't supported on this type by the native backend yet")),
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

    /// A C expression (`const char*`) rendering `code` the way `print` does.
    /// Records and enums go through a generated `ostrin_show_<Name>`.
    fn show_expr(&mut self, code: &str, ty: &CType) -> Result<String, String> {
        match ty {
            CType::Int => Ok(format!("ostrin_int_to_string({code})")),
            CType::Sized(kind) if kind.is_signed() => Ok(format!("ostrin_int_to_string({code})")),
            CType::Sized(_) => Ok(format!("ostrin_uint_to_string({code})")),
            CType::Float32 => Ok(format!("ostrin_single_to_string({code})")),
            CType::Float => Ok(format!("ostrin_float_to_string({code})")),
            CType::Bool => Ok(format!("(({code}) ? \"true\" : \"false\")")),
            CType::Str => Ok(code.to_string()),
            CType::Quantity(_) => Ok(format!("ostrin_qty_to_string({code})")),
            CType::Record(_) | CType::Enum(_) | CType::List(_) | CType::Option(_) | CType::Result(..) | CType::Map(..) | CType::Set(_) | CType::Array(_) => {
                let name = mangle_ctype(ty);
                if self.show_done.insert(name.clone()) {
                    self.show_queue.push_back(ty.clone());
                }
                Ok(format!("ostrin_show_{name}({code})"))
            }
            other => Err(format!("cannot 'print' a value of type '{}' yet", c_type_name(other))),
        }
    }

    /// Body of `ostrin_show_<name>`: `Circle(radius: 1.5)`, `Rect(2, 3)`,
    /// `P { x: 1, name: a }` — the interpreter's own display format.
    fn gen_show_body(&mut self, ty: &CType) -> Result<String, String> {
        let mut out = String::new();
        match ty {
            CType::Array(_) => {
                out.push_str(&format!("    return {}_show_rec(v, 0, 0);\n", mangle_ctype(ty)));
            }
            CType::List(elem) => {
                let shown = self.show_expr("v->items[i]", elem)?;
                out.push_str("    const char* s = \"[\";\n");
                out.push_str(&format!(
                    "    for (int64_t i = 0; i < v->length; i++) {{\n        if (i > 0) s = ostrin_str_concat(s, \", \");\n        s = ostrin_str_concat(s, {shown});\n    }}\n"
                ));
                out.push_str("    return ostrin_str_concat(s, \"]\");\n");
            }
            CType::Set(elem) => {
                let shown = self.show_expr("v->items[i]", elem)?;
                out.push_str("    const char* s = \"{\";\n");
                out.push_str(&format!(
                    "    for (int64_t i = 0; i < v->length; i++) {{\n        if (i > 0) s = ostrin_str_concat(s, \", \");\n        s = ostrin_str_concat(s, {shown});\n    }}\n"
                ));
                out.push_str("    return ostrin_str_concat(s, \"}\");\n");
            }
            CType::Map(k, val) => {
                let ks = self.show_expr("v->keys[i]", k)?;
                let vs = self.show_expr("v->vals[i]", val)?;
                out.push_str("    const char* s = \"[\";\n");
                out.push_str(&format!(
                    "    for (int64_t i = 0; i < v->length; i++) {{\n        if (i > 0) s = ostrin_str_concat(s, \", \");\n        s = ostrin_str_concat(s, {ks});\n        s = ostrin_str_concat(s, \": \");\n        s = ostrin_str_concat(s, {vs});\n    }}\n"
                ));
                out.push_str("    return ostrin_str_concat(s, \"]\");\n");
            }
            CType::Result(ok, err) => {
                let ok_shown = self.show_expr("v.value", ok)?;
                let err_shown = self.show_expr("v.error", err)?;
                out.push_str(&format!(
                    "    if (v.ok) return ostrin_str_concat(ostrin_str_concat(\"Ok(\", {ok_shown}), \")\");\n    return ostrin_str_concat(ostrin_str_concat(\"Err(\", {err_shown}), \")\");\n"
                ));
            }
            CType::Option(inner) => {
                let shown = self.show_expr("v.value", inner)?;
                out.push_str(&format!(
                    "    if (!v.has) return \"None\";\n    return ostrin_str_concat(ostrin_str_concat(\"Some(\", {shown}), \")\");\n"
                ));
            }
            CType::Enum(name) => {
                let variants: Vec<VariantInfo> = match self.instance_variants.get(name) {
                    Some(vs) => vs.clone(),
                    None => self.variants.values().filter(|v| &v.enum_name == name).cloned().collect(),
                };
                for v in variants {
                    out.push_str(&format!("    if (v.tag == {}) {{\n        const char* s = {};\n", v.tag, c_string_literal(&if v.fields.is_empty() { v.name.clone() } else { format!("{}(", v.name) })));
                    for (i, (field_name, field_ty)) in v.fields.iter().enumerate() {
                        if i > 0 {
                            out.push_str("        s = ostrin_str_concat(s, \", \");\n");
                        }
                        if *field_name != format!("f{i}") {
                            out.push_str(&format!("        s = ostrin_str_concat(s, {});\n", c_string_literal(&format!("{field_name}: "))));
                        }
                        let shown = self.show_expr(&format!("v.data.{}.{field_name}", v.name), field_ty)?;
                        out.push_str(&format!("        s = ostrin_str_concat(s, {shown});\n"));
                    }
                    if !v.fields.is_empty() {
                        out.push_str("        s = ostrin_str_concat(s, \")\");\n");
                    }
                    out.push_str("        return s;\n    }\n");
                }
                out.push_str("    return \"?\";\n");
            }
            CType::Record(name) => {
                let base = self.instance_info.get(name).map(|(b, _)| b.clone()).unwrap_or_else(|| name.clone());
                out.push_str(&format!("    const char* s = {};\n", c_string_literal(&format!("{base} {{ "))));
                for (i, (field_name, field_ty)) in self.record_fields(name).to_vec().iter().enumerate() {
                    if i > 0 {
                        out.push_str("    s = ostrin_str_concat(s, \", \");\n");
                    }
                    out.push_str(&format!("    s = ostrin_str_concat(s, {});\n", c_string_literal(&format!("{field_name}: "))));
                    let shown = self.show_expr(&format!("v->{field_name}"), field_ty)?;
                    out.push_str(&format!("    s = ostrin_str_concat(s, {shown});\n"));
                }
                out.push_str("    return ostrin_str_concat(s, \" }\");\n");
            }
            _ => unreachable!("only records and enums are queued"),
        }
        Ok(out)
    }

    /// The interpreter's built-in functions beyond `print`: `read_file`,
    /// `write_file`, `parse_int`, `sum` and `panic`.
    fn gen_builtin(&mut self, name: &str, codes: &[String], types: &[CType]) -> Result<Option<(String, CType)>, String> {
        let arity = match name {
            "read_file" | "parse_int" | "sum" | "panic" | "assert" | "array" | "zeros" | "ones" | "abs" => 1,
            n if ["sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "ln", "log10", "sqrt", "floor", "ceil", "round"].contains(&n) => 1,
            "pi" => 0,
            "pow" | "atan2" => 2,
            "write_file" | "assert_eq" | "full" | "arange" => 2,
            "linspace" => 3,
            _ => return Ok(None),
        };
        if codes.len() != arity {
            return Err(format!("'{name}' expects {arity} argument(s), got {}", codes.len()));
        }
        let (r, a, b, c) = (self.next_temp(), self.next_temp(), self.next_temp(), self.next_temp());
        match name {
            "read_file" => {
                let ty = CType::Result(Box::new(CType::Str), Box::new(CType::Str));
                self.register_list_types(&ty);
                Ok(Some((
                    format!(
                        "({{ Result_String_String {r}; memset(&{r}, 0, sizeof {r}); FILE* {a} = fopen({p}, \"rb\"); \
                         if (!{a}) {{ {r}.error = strerror(errno); }} else {{ \
                         fseek({a}, 0, SEEK_END); long {b} = ftell({a}); fseek({a}, 0, SEEK_SET); \
                         char* {c} = (char*)malloc((size_t){b} + 1); \
                         if (!{c}) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }} \
                         size_t {r}_n = fread({c}, 1, (size_t){b}, {a}); {c}[{r}_n] = 0; fclose({a}); \
                         {r}.ok = true; {r}.value = {c}; }} {r}; }})",
                        p = codes[0]
                    ),
                    ty,
                )))
            }
            "write_file" => {
                let ty = CType::Result(Box::new(CType::Void), Box::new(CType::Str));
                self.register_list_types(&ty);
                Ok(Some((
                    format!(
                        "({{ Result_Void_String {r}; memset(&{r}, 0, sizeof {r}); FILE* {a} = fopen({p}, \"wb\"); \
                         if (!{a}) {{ {r}.error = strerror(errno); }} else {{ fputs({t}, {a}); fclose({a}); {r}.ok = true; }} {r}; }})",
                        p = codes[0],
                        t = codes[1]
                    ),
                    ty,
                )))
            }
            "parse_int" => {
                let ty = CType::Result(Box::new(CType::Int), Box::new(CType::Str));
                self.register_list_types(&ty);
                Ok(Some((
                    format!(
                        "({{ Result_Int_String {r}; memset(&{r}, 0, sizeof {r}); const char* {a} = {t}; \
                         if (*{a} == 0) {{ {r}.error = \"cannot parse integer from empty string\"; }} \
                         else if (*{a} == ' ' || (*{a} >= 9 && *{a} <= 13)) {{ {r}.error = \"invalid digit found in string\"; }} \
                         else {{ char* {b}; errno = 0; long long {c} = strtoll({a}, &{b}, 10); \
                         if (errno == ERANGE) {{ {r}.error = {c} < 0 ? \"number too small to fit in target type\" : \"number too large to fit in target type\"; }} \
                         else if (*{b} != 0 || {b} == {a}) {{ {r}.error = \"invalid digit found in string\"; }} \
                         else {{ {r}.ok = true; {r}.value = (int64_t){c}; }} }} {r}; }})",
                        t = codes[0]
                    ),
                    ty,
                )))
            }
            "pi" => Ok(Some(("3.141592653589793".to_string(), CType::Float))),
            "pow" | "atan2" => match (&types[0], &types[1]) {
                (CType::Float, CType::Float) => Ok(Some((format!("{name}({}, {})", codes[0], codes[1]), CType::Float))),
                (CType::Float32, CType::Float32) => Ok(Some((format!("{name}f({}, {})", codes[0], codes[1]), CType::Float32))),
                _ => Err(format!("'{name}' needs two Float or two Float32 arguments")),
            },
            "abs" => {
                let (elem, array) = match &types[0] {
                    CType::Array(t) => ((**t).clone(), true),
                    other => (other.clone(), false),
                };
                let function = match &elem {
                    CType::Float => "fabs".to_string(),
                    CType::Float32 => "fabsf".to_string(),
                    CType::Int => "ostrin_abs_i64".to_string(),
                    CType::Sized(_) if !array => String::new(),
                    other => return Err(format!("'abs' isn't supported on '{}' by the native backend", c_type_name(other))),
                };
                if array {
                    let array_ty = types[0].clone();
                    return Ok(Some((format!("{}_map({}, {function})", mangle_ctype(&array_ty), codes[0]), array_ty)));
                }
                if let CType::Sized(kind) = &elem {
                    let temp = self.next_temp();
                    let c = kind.c_type();
                    return Ok(Some((
                        format!("({{ {c} {temp} = {}; if ({temp} == {}) {{ {OVERFLOW_ABORT} }} ({c})({temp} < 0 ? -{temp} : {temp}); }})", codes[0], c_int_literal(kind.min())),
                        elem,
                    )));
                }
                Ok(Some((format!("{function}({})", codes[0]), elem)))
            }
            m if ["sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "ln", "log10", "sqrt", "floor", "ceil", "round"].contains(&m) => {
                let c_name = if m == "ln" { "log" } else { m };
                let (elem, array) = match &types[0] {
                    CType::Array(t) => ((**t).clone(), true),
                    other => (other.clone(), false),
                };
                let function = match elem {
                    CType::Float => c_name.to_string(),
                    CType::Float32 => format!("{c_name}f"),
                    other => return Err(format!("'{m}' isn't supported on '{}' by the native backend", c_type_name(&other))),
                };
                if array {
                    let array_ty = types[0].clone();
                    Ok(Some((format!("{}_map({}, {function})", mangle_ctype(&array_ty), codes[0]), array_ty)))
                } else {
                    Ok(Some((format!("{function}({})", codes[0]), types[0].clone())))
                }
            }
            "array" => {
                let mut ty = &types[0];
                let mut depth = 0;
                while let CType::List(inner) = ty {
                    ty = inner;
                    depth += 1;
                }
                if !(1..=3).contains(&depth) || !matches!(ty, CType::Int | CType::Float | CType::Float32) {
                    return Err("'array' supports nested lists (up to 3 deep) of Int, Float or Float32 in the native backend".to_string());
                }
                let array_ty = CType::Array(Box::new(ty.clone()));
                self.register_list_types(&array_ty);
                Ok(Some((format!("{}_from{depth}({})", mangle_ctype(&array_ty), codes[0]), array_ty)))
            }
            "zeros" | "ones" => {
                let array_ty = CType::Array(Box::new(CType::Float));
                self.register_list_types(&array_ty);
                Ok(Some((format!("Array_Float_full({}, {})", codes[0], if name == "ones" { "1.0" } else { "0.0" }), array_ty)))
            }
            "full" => {
                if !matches!(types[1], CType::Int | CType::Float | CType::Float32) {
                    return Err("'full' supports Int, Float or Float32 fill values in the native backend".to_string());
                }
                let array_ty = CType::Array(Box::new(types[1].clone()));
                self.register_list_types(&array_ty);
                Ok(Some((format!("{}_full({}, {})", mangle_ctype(&array_ty), codes[0], codes[1]), array_ty)))
            }
            "arange" => {
                let array_ty = CType::Array(Box::new(CType::Int));
                self.register_list_types(&array_ty);
                Ok(Some((format!("Array_Int_arange({}, {})", codes[0], codes[1]), array_ty)))
            }
            "linspace" => {
                let array_ty = CType::Array(Box::new(CType::Float));
                self.register_list_types(&array_ty);
                Ok(Some((format!("Array_Float_linspace({}, {}, {})", codes[0], codes[1], codes[2]), array_ty)))
            }
            "assert" => Ok(Some((
                format!("({{ if (!({})) {{ fprintf(stderr, \"runtime error: assertion failed\\n\"); exit(1); }} }})", codes[0]),
                CType::Void,
            ))),
            "assert_eq" => {
                let equal = self.eq_expr(&codes[0], &codes[1], &types[0])?;
                Ok(Some((
                    format!("({{ if (!{equal}) {{ fprintf(stderr, \"runtime error: assertion failed: left != right\\n\"); exit(1); }} }})"),
                    CType::Void,
                )))
            }
            "panic" => Ok(Some((
                format!("({{ fprintf(stderr, \"runtime error: panic: %s\\n\", {}); exit(1); }})", codes[0]),
                CType::Void,
            ))),
            "sum" => {
                let CType::List(elem) = &types[0] else { return Err("'sum' expects a List".to_string()) };
                let struct_name = self.ensure_list(elem);
                let list = format!("{struct_name}* {r} = {};", codes[0]);
                match &**elem {
                    CType::Int => Ok(Some((
                        format!("({{ {list} int64_t {a} = 0; for (int64_t {b} = 0; {b} < {r}->length; {b}++) {{ {a} += {r}->items[{b}]; }} {a}; }})"),
                        CType::Int,
                    ))),
                    CType::Float => Ok(Some((
                        format!("({{ {list} double {a} = 0.0; for (int64_t {b} = 0; {b} < {r}->length; {b}++) {{ {a} += {r}->items[{b}]; }} {a}; }})"),
                        CType::Float,
                    ))),
                    CType::Quantity(d) => Ok(Some((
                        format!(
                            "({{ {list} if ({r}->length == 0) {{ fprintf(stderr, \"runtime error: sum of an empty list of quantities\\n\"); exit(1); }} \
                             Qty {a} = {r}->items[0]; for (int64_t {b} = 1; {b} < {r}->length; {b}++) {{ {a} = ostrin_qty_add({a}, {r}->items[{b}]); }} {a}; }})"
                        ),
                        CType::Quantity(d.clone()),
                    ))),
                    other => Err(format!("'sum' isn't supported on List<{}> by the native backend", c_type_name(other))),
                }
            }
            _ => unreachable!(),
        }
    }

    fn gen_print(&mut self, arg_codes: &[String], arg_types: &[CType]) -> Result<(String, CType), String> {
        if arg_codes.len() != 1 {
            return Err("'print' expects exactly one argument".to_string());
        }
        let (spec, value) = match &arg_types[0] {
            CType::Int => ("%lld\\n", format!("(long long)({})", arg_codes[0])),
            CType::Float32 => return Ok((format!("ostrin_print_single({})", arg_codes[0]), CType::Void)),
            CType::Sized(kind) if kind.is_signed() => ("%lld\\n", format!("(long long)({})", arg_codes[0])),
            CType::Sized(_) => ("%llu\\n", format!("(unsigned long long)({})", arg_codes[0])),
            CType::Float => return Ok((format!("ostrin_print_float({})", arg_codes[0]), CType::Void)),
            CType::Bool => ("%s\\n", format!("(({}) ? \"true\" : \"false\")", arg_codes[0])),
            CType::Str => ("%s\\n", arg_codes[0].clone()),
            CType::Void => return Err("cannot 'print' a Void value".to_string()),
            CType::Record(_) | CType::Enum(_) => {
                let shown = self.show_expr(&arg_codes[0], &arg_types[0].clone())?;
                ("%s\\n", shown)
            }
            CType::DynTrait(name) => return Err(format!("cannot 'print' a 'dyn {name}' value")),
            CType::List(_) | CType::Map(..) | CType::Set(_) | CType::Option(_) | CType::Result(..) | CType::Array(_) => {
                let shown = self.show_expr(&arg_codes[0], &arg_types[0].clone())?;
                ("%s\\n", shown)
            }
            CType::Quantity(_) => return Ok((format!("ostrin_print_qty({})", arg_codes[0]), CType::Void)),
            CType::GenLit(..) => return Err("cannot infer the enum instance to print here".to_string()),
            CType::Channel(_) | CType::Task(_) => return Err("cannot 'print' a Task or Channel value".to_string()),
            CType::NoneLit | CType::OkLit(_) | CType::ErrLit(_) => {
                return Err("cannot 'print' a bare None/Ok/Err literal; its type can't be inferred here".to_string())
            }
        };
        Ok((format!("printf(\"{spec}\", {value})"), CType::Void))
    }
}

/// Resolves named and defaulted arguments against `params` (skipping the
/// first `skip` — a method's `self`), returning the arguments in positional
/// order, or `None` when the call is already plain positional and complete.
/// A default is evaluated at the call site, like the interpreter does.
fn normalize_call_args(params: &[Param], args: &[Arg], skip: usize) -> Result<Option<Vec<Arg>>, String> {
    let params = &params[skip.min(params.len())..];
    if args.len() == params.len() && args.iter().all(|a| matches!(a, Arg::Positional(_))) {
        return Ok(None);
    }
    let mut slots: Vec<Option<Expr>> = vec![None; params.len()];
    let mut next = 0usize;
    for arg in args {
        match arg {
            Arg::Positional(e) => {
                if next >= params.len() {
                    return Err("too many arguments in call".to_string());
                }
                slots[next] = Some(e.clone());
                next += 1;
            }
            Arg::Named(name, e) => {
                let Some(index) = params.iter().position(|p| &p.name == name) else {
                    return Err(format!("no parameter named '{name}'"));
                };
                slots[index] = Some(e.clone());
            }
        }
    }
    slots
        .into_iter()
        .zip(params)
        .map(|(slot, param)| {
            slot.or_else(|| param.default.clone())
                .map(Arg::Positional)
                .ok_or_else(|| format!("missing argument for parameter '{}'", param.name))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

/// Matches a variant constructor's arguments to its declared fields:
/// positional ones fill in declaration order, named ones go by field name.
fn arrange_args<'e>(field_names: &[String], variant: &str, args: &'e [Arg]) -> Result<Vec<&'e Expr>, String> {
    let mut slots: Vec<Option<&'e Expr>> = vec![None; field_names.len()];
    let mut next_positional = 0usize;
    for arg in args {
        match arg {
            Arg::Positional(expr) => {
                if next_positional >= field_names.len() {
                    return Err(format!("variant '{variant}' expects {} argument(s), got more", field_names.len()));
                }
                slots[next_positional] = Some(expr);
                next_positional += 1;
            }
            Arg::Named(field_name, expr) => {
                let Some(index) = field_names.iter().position(|n| n == field_name) else {
                    return Err(format!("variant '{variant}' has no field '{field_name}'"));
                };
                slots[index] = Some(expr);
            }
        }
    }
    slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| slot.ok_or_else(|| format!("variant '{variant}' is missing argument for field '{}'", field_names[index])))
        .collect()
}

/// The common type of two branch results, completing partial literals
/// (`None`, `Ok(x)`, `Err(e)`) from whichever side is more informative.
fn unify_types(a: &CType, b: &CType) -> CType {
    match (a, b) {
        (x, y) if x == y => x.clone(),
        (CType::NoneLit, other) | (other, CType::NoneLit) => other.clone(),
        (CType::GenLit(..), other) | (other, CType::GenLit(..)) => other.clone(),
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
/// Transpiles a type-checked program to C. `checker_types` is the checker's
/// typed-expression table: it completes the types the backend can only infer
/// partially (`None`, `Ok(x)`, `Nothing`, …) and lets `NativeTypeReport`
/// compare the backend's own inference with the checker's.
pub fn generate_with_report(items: &[Item], typed: &crate::typeck::TypedProgram) -> Result<(String, NativeTypeReport), String> {
    generate_impl(items, Some(typed))
}

fn generate_impl(items: &[Item], typed: Option<&crate::typeck::TypedProgram>) -> Result<(String, NativeTypeReport), String> {
    let checker_types = typed.map(|t| &t.expr_types);
    let call_substs = typed.map(|t| &t.call_substs);
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

    // Generic records/enums never get a C type of their own: only their
    // concrete instantiations do (see `Codegen::register_instance`).
    let generic_records: HashMap<String, &RecordDecl> = records.iter().filter(|r| !r.generics.is_empty()).map(|r| (r.name.clone(), *r)).collect();
    let generic_enums: HashMap<String, &EnumDecl> = enums.iter().filter(|e| !e.generics.is_empty()).map(|e| (e.name.clone(), *e)).collect();
    let mut generic_arity: HashMap<String, (bool, usize)> = HashMap::new();
    for (name, r) in &generic_records {
        generic_arity.insert(name.clone(), (false, r.generics.len()));
    }
    let mut generic_variant_owner: HashMap<String, String> = HashMap::new();
    for (name, e) in &generic_enums {
        generic_arity.insert(name.clone(), (true, e.generics.len()));
        for variant in &e.variants {
            generic_variant_owner.insert(variant.name.clone(), name.clone());
        }
    }
    // A trait's default-bodied methods become ordinary function declarations,
    // compiled once per `impl` that doesn't override them.
    let default_decls: HashMap<String, Vec<FunctionDecl>> = traits
        .iter()
        .map(|t| {
            let decls = t
                .methods
                .iter()
                .filter_map(|m| {
                    m.default_body.as_ref().map(|body| FunctionDecl {
                        name: m.name.clone(),
                        is_pub: true,
                        generics: m.generics.clone(),
                        params: m.params.clone(),
                        return_type: m.return_type.clone(),
                        body: body.clone(),
                        span: t.span,
                        source_file: None,
                    })
                })
                .collect();
            (t.name.clone(), decls)
        })
        .collect();
    let generic_derives: Vec<(String, Vec<String>)> = generic_records
        .values()
        .map(|r| (r.name.clone(), r.derives.clone()))
        .chain(generic_enums.values().map(|e| (e.name.clone(), e.derives.clone())))
        .collect();
    let generic_impls: Vec<&ImplDecl> = impls.iter().filter(|im| generic_arity.contains_key(&im.type_name)).copied().collect();
    let records: Vec<&RecordDecl> = records.into_iter().filter(|r| r.generics.is_empty()).collect();
    let enums: Vec<&EnumDecl> = enums.into_iter().filter(|e| e.generics.is_empty()).collect();
    // `Ordering` is built into the language (the interpreter registers it
    // itself), so it isn't among `items` — a hand-written `impl Ord`'s
    // `compare` needs it as a real enum.
    let ordering_decl = EnumDecl {
        name: "Ordering".to_string(),
        module_path: Vec::new(),
        is_pub: true,
        generics: Vec::new(),
        derives: Vec::new(),
        variants: ["Less", "Equal", "Greater"].iter().map(|n| VariantDecl { name: n.to_string(), fields: Vec::new() }).collect(),
        span: Span::default(),
        source_file: None,
    };
    let mut enums = enums;
    if !enums.iter().any(|e| e.name == "Ordering") {
        enums.push(&ordering_decl);
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
        generic_records,
        generic_enums,
        generic_arity,
        generic_impls,
        generic_variant_owner,
        seen_instances: RefCell::new(Vec::new()),
        instances_done: HashSet::new(),
        instance_info: HashMap::new(),
        instance_order: Vec::new(),
        instance_variants: HashMap::new(),
        expected: None,
        show_queue: VecDeque::new(),
        checker_types,
        call_substs,
        literal_kinds: typed.map(|t| &t.literal_kinds),
        current_call_key: None,
        current_file: None,
        compare_enabled: false,
        type_report: NativeTypeReport::default(),
        generic_methods: HashMap::new(),
        quantity_impls: impls.iter().filter(|im| im.type_name == "Quantity").copied().collect(),
        quantity_done: HashSet::new(),
        subst_stack: Vec::new(),
        trait_defaults: default_decls.iter().map(|(t, ds)| (t.clone(), ds.iter().collect())).collect(),
        pending_colls: VecDeque::new(),
        coll_done: HashSet::new(),
        function_decls: functions.iter().map(|f| (f.name.clone(), *f)).collect(),
        op_queue: VecDeque::new(),
        op_done: HashSet::new(),
        derives: records.iter().map(|r| (r.name.clone(), r.derives.clone())).chain(enums.iter().map(|e| (e.name.clone(), e.derives.clone()))).chain(generic_derives).collect(),
        show_done: HashSet::new(),
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
        let self_subst: HashMap<String, CType> = HashMap::from([("Self".to_string(), self_ty.clone())]);
        codegen.register_impl_methods(im, &im.type_name, &self_ty, &self_subst, false);
    }

    codegen.flush_instances()?;

    let mut out = String::from(PRELUDE);
    let _ = &mut out;
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
        codegen.current_file = f.source_file.clone();
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
        // Methods of a generic instance were already queued for generation
        // (in `pending`) when the instance was registered.
        .filter(|info| match &info.self_ty {
            CType::Record(n) | CType::Enum(n) => !codegen.instance_info.contains_key(n),
            CType::Quantity(_) => false,
            _ => true,
        })
        .map(|info| (info.self_ty.clone(), info.param_types.clone(), info.return_type.clone(), info.c_name.clone(), info.decl))
        .collect();
    for (self_ty, param_types, return_type, c_name, decl) in method_infos {
        let params = render_params(&param_types, &decl.params);
        let signature = format!("{} {}({})", c_type_name(&return_type), c_name, params);
        let self_subst: HashMap<String, CType> = HashMap::from([("Self".to_string(), self_ty)]);
        let mut body = String::new();
        codegen.current_file = decl.source_file.clone();
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
    let mut list_typedefs = String::new();
    let mut array_blocks: Vec<String> = Vec::new();
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
            list_typedefs.push_str(&format!("typedef struct {struct_name} {struct_name};\n"));
            list_type_decls.push_str(&format!(
                "struct {struct_name} {{\n    {elem_c}* items;\n    int64_t length;\n    int64_t capacity;\n}};\n\n"
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
        while let Some((is_compare, ty)) = codegen.op_queue.pop_front() {
            progressed = true;
            let name = match &ty {
                CType::Record(n) | CType::Enum(n) => n.clone(),
                _ => unreachable!(),
            };
            let (prefix, ret) = if is_compare { ("cmp", "int") } else { ("eq", "bool") };
            let c = c_type_name(&ty);
            let signature = format!("static {ret} ostrin_{prefix}_{name}({c} a, {c} b)");
            let body = codegen.gen_op_body(is_compare, &ty)?;
            list_helper_prototypes.push(format!("{signature};"));
            bodies.push((signature, body));
        }
        while let Some(ty) = codegen.pending_colls.pop_front() {
            progressed = true;
            let name = mangle_ctype(&ty);
            list_typedefs.push_str(&format!("typedef struct {name} {name};\n"));
            if let CType::Array(elem) = &ty {
                let tc = c_type_name(elem);
                list_type_decls.push_str(&format!("struct {name} {{\n    {tc}* data;\n    int64_t* shape;\n    int64_t rank;\n    int64_t size;\n}};\n\n"));
                let lt = list_struct_name(elem);
                let rows = CType::List(elem.clone());
                let llt = list_struct_name(&rows);
                let lllt = list_struct_name(&CType::List(Box::new(rows)));
                let show_elem = codegen.show_expr("a->data[off]", elem)?;
                let (add, sub, mul, div, lt_macro) = match **elem {
                    CType::Int => ("((a) + (b))", "((a) - (b))", "((a) * (b))", "ostrin_idiv((a), (b))", "((a) < (b))"),
                    CType::Float => ("((a) + (b))", "((a) - (b))", "((a) * (b))", "((a) / (b))", "((a) < (b))"),
                    _ => ("((float)((a) + (b)))", "((float)((a) - (b)))", "((float)((a) * (b)))", "((float)((a) / (b)))", "((a) < (b))"),
                };
                let mut text = format!(
                    "#define OSTRIN_ADD(a, b) {add}\n#define OSTRIN_SUB(a, b) {sub}\n#define OSTRIN_MUL(a, b) {mul}\n#define OSTRIN_DIV(a, b) {div}\n#define OSTRIN_ELEM_LT(a, b) {lt_macro}\n"
                );
                text.push_str(
                    &ARRAY_RUNTIME
                        .replace("@SHOW_ELEM@", &show_elem)
                        .replace("@LLLT@", &lllt)
                        .replace("@LLT@", &llt)
                        .replace("@LT@", &lt)
                        .replace("@N@", &name)
                        .replace("@T@", &tc),
                );
                match **elem {
                    CType::Int => text.push_str(&format!(
                        "static double {name}_mean({name}* a) {{ return (double){name}_sum(a) / (double)a->size; }}\n\
                         static {name}* {name}_arange(int64_t lo, int64_t hi) {{\n\
                         \x20   if (hi <= lo) OSTRIN_FAIL(\"arange needs start < stop\");\n\
                         \x20   int64_t shape[1] = {{ hi - lo }};\n\
                         \x20   {name}* r = {name}_alloc(1, shape);\n\
                         \x20   for (int64_t i = 0; i < r->size; i++) r->data[i] = lo + i;\n\
                         \x20   return r;\n\
                         }}\n"
                    )),
                    CType::Float => text.push_str(&format!(
                        "static double {name}_mean({name}* a) {{ return {name}_sum(a) / (double)a->size; }}\n\
                         static {name}* {name}_linspace(double lo, double hi, int64_t n) {{\n\
                         \x20   if (n < 1) OSTRIN_FAIL(\"linspace needs at least one point\");\n\
                         \x20   int64_t shape[1] = {{ n }};\n\
                         \x20   {name}* r = {name}_alloc(1, shape);\n\
                         \x20   for (int64_t i = 0; i < n; i++) {{\n\
                         \x20       if (i + 1 == n && n > 1) r->data[i] = hi;\n\
                         \x20       else if (n == 1) r->data[i] = lo;\n\
                         \x20       else r->data[i] = lo + (hi - lo) * (double)i / (double)(n - 1);\n\
                         \x20   }}\n\
                         \x20   return r;\n\
                         }}\n"
                    )),
                    _ => text.push_str(&format!("static float {name}_mean({name}* a) {{ return (float)({name}_sum(a) / (float)a->size); }}\n")),
                }
                text.push_str("#undef OSTRIN_ADD\n#undef OSTRIN_SUB\n#undef OSTRIN_MUL\n#undef OSTRIN_DIV\n#undef OSTRIN_ELEM_LT\n\n");
                array_blocks.push(text);
                continue;
            }
            let oom = "if (!p) { fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }";
            let mut funcs: Vec<(String, String)> = Vec::new();
            match &ty {
                CType::Map(k, v) => {
                    let (kc, vc) = (c_type_name(k), c_type_name(v));
                    let opt = c_type_name(&CType::Option(v.clone()));
                    let list_k = c_type_name(&CType::List(k.clone()));
                    let list_v = c_type_name(&CType::List(v.clone()));
                    let (lk, lv) = (list_struct_name(k), list_struct_name(v));
                    let eq = codegen.eq_expr("m->keys[i]", "key", k)?;
                    list_type_decls.push_str(&format!("struct {name} {{\n    {kc}* keys;\n    {vc}* vals;\n    int64_t length;\n    int64_t capacity;\n}};\n\n"));
                    funcs.push((format!("static {name}* {name}_new(void)"), format!("    {name}* m = ({name}*)calloc(1, sizeof({name}));\n    {}\n    return m;\n", oom.replace("!p", "!m"))));
                    funcs.push((format!("static int64_t {name}_find({name}* m, {kc} key)"), format!("    for (int64_t i = 0; i < m->length; i++) {{ if ({eq}) return i; }}\n    return -1;\n")));
                    funcs.push((format!("static void {name}_set({name}* m, {kc} key, {vc} value)"), format!(
                        "    int64_t i = {name}_find(m, key);\n    if (i >= 0) {{ m->keys[i] = key; m->vals[i] = value; return; }}\n    if (m->length >= m->capacity) {{\n        m->capacity = m->capacity == 0 ? 4 : m->capacity * 2;\n        m->keys = ({kc}*)realloc(m->keys, sizeof({kc}) * (size_t)m->capacity);\n        m->vals = ({vc}*)realloc(m->vals, sizeof({vc}) * (size_t)m->capacity);\n        if (!m->keys || !m->vals) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }}\n    }}\n    m->keys[m->length] = key;\n    m->vals[m->length] = value;\n    m->length = m->length + 1;\n")));
                    funcs.push((format!("static {opt} {name}_get({name}* m, {kc} key)"), format!("    {opt} r;\n    memset(&r, 0, sizeof r);\n    int64_t i = {name}_find(m, key);\n    if (i >= 0) {{ r.has = true; r.value = m->vals[i]; }}\n    return r;\n")));
                    funcs.push((format!("static bool {name}_contains_key({name}* m, {kc} key)"), format!("    return {name}_find(m, key) >= 0;\n")));
                    funcs.push((format!("static int64_t {name}_count({name}* m)"), "    return m->length;\n".to_string()));
                    funcs.push((format!("static {opt} {name}_remove({name}* m, {kc} key)"), format!("    {opt} r;\n    memset(&r, 0, sizeof r);\n    int64_t i = {name}_find(m, key);\n    if (i < 0) return r;\n    r.has = true;\n    r.value = m->vals[i];\n    for (int64_t j = i; j < m->length - 1; j++) {{ m->keys[j] = m->keys[j + 1]; m->vals[j] = m->vals[j + 1]; }}\n    m->length = m->length - 1;\n    return r;\n")));
                    funcs.push((format!("static {list_k} {name}_keys({name}* m)"), format!("    return {lk}_new_from_array(m->keys, m->length);\n")));
                    funcs.push((format!("static {list_v} {name}_values({name}* m)"), format!("    return {lv}_new_from_array(m->vals, m->length);\n")));
                }
                CType::Task(t) => {
                    list_type_decls.push_str(&format!("struct {name} {{\n    {} value;\n}};\n\n", field_c_type(t)));
                }
                CType::Channel(t) => {
                    let tc = c_type_name(t);
                    let opt = c_type_name(&CType::Option(t.clone()));
                    list_type_decls.push_str(&format!("struct {name} {{\n    {tc}* items;\n    int64_t head;\n    int64_t length;\n    int64_t capacity;\n    bool closed;\n}};\n\n"));
                    funcs.push((format!("static {name}* {name}_new(void)"), format!("    {name}* c = ({name}*)calloc(1, sizeof({name}));\n    if (!c) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }}\n    return c;\n")));
                    funcs.push((format!("static void {name}_send({name}* c, {tc} item)"), format!(
                        "    if (c->length >= c->capacity) {{\n        c->capacity = c->capacity == 0 ? 4 : c->capacity * 2;\n        c->items = ({tc}*)realloc(c->items, sizeof({tc}) * (size_t)c->capacity);\n        if (!c->items) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }}\n    }}\n    c->items[c->length] = item;\n    c->length = c->length + 1;\n")));
                    funcs.push((format!("static {opt} {name}_receive({name}* c)"), format!("    {opt} r;\n    memset(&r, 0, sizeof r);\n    if (c->head < c->length) {{ r.has = true; r.value = c->items[c->head++]; }}\n    return r;\n")));
                }
                CType::Set(t) => {
                    let tc = c_type_name(t);
                    let eq = codegen.eq_expr("s->items[i]", "item", t)?;
                    list_type_decls.push_str(&format!("struct {name} {{\n    {tc}* items;\n    int64_t length;\n    int64_t capacity;\n}};\n\n"));
                    funcs.push((format!("static {name}* {name}_new(void)"), format!("    {name}* s = ({name}*)calloc(1, sizeof({name}));\n    {}\n    return s;\n", oom.replace("!p", "!s"))));
                    funcs.push((format!("static int64_t {name}_find({name}* s, {tc} item)"), format!("    for (int64_t i = 0; i < s->length; i++) {{ if ({eq}) return i; }}\n    return -1;\n")));
                    funcs.push((format!("static bool {name}_contains({name}* s, {tc} item)"), format!("    return {name}_find(s, item) >= 0;\n")));
                    funcs.push((format!("static void {name}_add({name}* s, {tc} item)"), format!(
                        "    if ({name}_find(s, item) >= 0) return;\n    if (s->length >= s->capacity) {{\n        s->capacity = s->capacity == 0 ? 4 : s->capacity * 2;\n        s->items = ({tc}*)realloc(s->items, sizeof({tc}) * (size_t)s->capacity);\n        if (!s->items) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }}\n    }}\n    s->items[s->length] = item;\n    s->length = s->length + 1;\n")));
                    funcs.push((format!("static void {name}_remove({name}* s, {tc} item)"), format!("    int64_t i = {name}_find(s, item);\n    if (i < 0) return;\n    for (int64_t j = i; j < s->length - 1; j++) {{ s->items[j] = s->items[j + 1]; }}\n    s->length = s->length - 1;\n")));
                    funcs.push((format!("static int64_t {name}_count({name}* s)"), "    return s->length;\n".to_string()));
                }
                _ => unreachable!(),
            }
            for (signature, body) in funcs {
                list_helper_prototypes.push(format!("{signature};"));
                bodies.push((signature, body));
            }
        }
        while let Some(ty) = codegen.show_queue.pop_front() {
            progressed = true;
            let name = mangle_ctype(&ty);
            let signature = format!("static const char* ostrin_show_{name}({} v)", c_type_name(&ty));
            let body = codegen.gen_show_body(&ty)?;
            list_helper_prototypes.push(format!("{signature};"));
            bodies.push((signature, body));
        }
        while let Some(job) = codegen.pending.pop_front() {
            progressed = true;
            let params = render_params(&job.param_types, &job.decl.params);
            let signature = format!("{} {}({})", c_type_name(&job.return_type), job.c_name, params);
            let mut body = String::new();
            codegen.current_file = job.decl.source_file.clone();
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

    // Every type declaration is written out here, after the drain loop, so
    // generic instantiations (only discovered from actual usage) are all
    // known. A record is only ever used as a pointer, so its typedef alone
    // suffices to sidestep ordering (including records referencing each
    // other); an enum is a by-value tagged union, so its full body must
    // precede anything embedding it by value — enums first (a generic
    // instance always comes after the instances its own arguments name),
    // then records.
    for r in &records {
        out.push_str(&format!("typedef struct {0} {0};\n", r.name));
    }
    for e in &enums {
        out.push_str(&format!("typedef struct {0} {0};\n", e.name));
    }
    for (_, name) in &codegen.instance_order {
        out.push_str(&format!("typedef struct {0} {0};\n", name));
    }
    out.push_str(&list_typedefs);
    out.push('\n');
    let emit_enum = |out: &mut String, name: &str, variants: &[VariantInfo]| {
        out.push_str(&format!("struct {name} {{\n    int tag;\n"));
        if variants.iter().any(|variant| !variant.fields.is_empty()) {
            out.push_str("    union {\n");
            for info in variants {
                if info.fields.is_empty() {
                    continue;
                }
                out.push_str("        struct {\n");
                for (field_name, field_ty) in &info.fields {
                    out.push_str(&format!("            {} {};\n", c_type_name(field_ty), field_name));
                }
                out.push_str(&format!("        }} {};\n", info.name));
            }
            out.push_str("    } data;\n");
        }
        out.push_str("};\n\n");
    };
    for e in &enums {
        let variants: Vec<VariantInfo> = e.variants.iter().map(|v| codegen.variants[&v.name].clone()).collect();
        emit_enum(&mut out, &e.name, &variants);
    }
    for (is_enum, name) in &codegen.instance_order {
        if *is_enum {
            emit_enum(&mut out, name, &codegen.instance_variants[name]);
        }
    }
    let emit_record = |out: &mut String, name: &str, fields: &[(String, CType)]| {
        out.push_str(&format!("struct {name} {{\n"));
        for (field_name, field_ty) in fields {
            out.push_str(&format!("    {} {};\n", c_type_name(field_ty), field_name));
        }
        out.push_str("};\n\n");
    };
    for r in &records {
        emit_record(&mut out, &r.name, &codegen.records[&r.name]);
    }
    for (is_enum, name) in &codegen.instance_order {
        if !*is_enum {
            emit_record(&mut out, name, &codegen.records[name]);
        }
    }

    // Every trait's vtable and fat-pointer types are declared eagerly (they
    // are cheap, and needed to even *state* a `dyn Trait` parameter's type);
    // only the actual vtable *instances* for a given (trait, record) pair
    // are lazy — see `PendingVTable` and the drain loop above.
    for (trait_name, methods) in &codegen.trait_methods {
        out.push_str("typedef struct {\n");
        for (method_name, (param_types, return_type)) in methods {
            let params = std::iter::once("void*".to_string()).chain(param_types.iter().map(c_type_name)).collect::<Vec<_>>().join(", ");
            out.push_str(&format!("    {} (*{})({});\n", c_type_name(return_type), method_name, params));
        }
        out.push_str(&format!("}} {trait_name}_VTable;\n\n"));
        out.push_str(&format!("typedef struct {{ void* self; const {trait_name}_VTable* vtable; }} {trait_name}_Dyn;\n\n"));
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

", field_c_type(&ok), field_c_type(&err)));
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

    // Array runtimes: full definitions, after every prototype they call.
    for block in &array_blocks {
        out.push_str(block);
    }
    for (signature, body) in bodies {
        out.push_str(&format!("{signature} {{\n{body}}}\n\n"));
    }
    out.push_str("int main(void) {\n    ostrin_main();\n    return 0;\n}\n");
    if out.contains("Qty") {
        out = out.replacen(PRELUDE, &format!("{PRELUDE}{QTY_RUNTIME}"), 1);
    }
    Ok((out, codegen.type_report.clone()))
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
