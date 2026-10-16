//! Experimental, conservative effect inference for the source program.
//!
//! This is not yet the complete effect checker described in design document 26.
//! It uses checker types to resolve calls, composes known rows and callback
//! variables, and carries those signatures through HIR and IR without exposing
//! public effect contracts. Unresolved operations remain open and an empty row
//! is not a proof of purity.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{
    Arg, Block, Expr, FunctionDecl, ImplDecl, Item, MatchArm, Pattern, Stmt, TraitDecl,
    TraitMethodSig, Type,
};
use crate::lexer::{Lexer, Token, TokenKind};
use crate::typeck::TypedProgram;
use crate::types::Ty;

/// Typed internal effect labels carried through HIR and IR. Public syntax and
/// effect contracts remain intentionally unset.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EffectKind {
    Clock,
    Concurrency,
    External,
    Filesystem,
    Io,
    LocalState,
    Measurement,
    Network,
    Provenance,
    RandomSeed,
    Unsafe,
    Unknown(String),
}

/// Internal effect-bearing function signature. `open_calls` means inference is
/// still conservative; variables remain symbolic until a caller substitutes a
/// concrete callback. This is not public type syntax.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectSignature {
    pub effects: BTreeSet<EffectKind>,
    pub variables: BTreeSet<String>,
    pub open_calls: BTreeSet<String>,
    /// A row inferred from a program with type errors cannot be advertised as
    /// closed, even if its effect-call inventory is otherwise empty.
    pub type_errors: usize,
}

impl EffectSignature {
    pub fn is_closed(&self) -> bool {
        self.type_errors == 0 && self.variables.is_empty() && self.open_calls.is_empty()
    }
}

impl EffectKind {
    pub fn parse(label: &str) -> Self {
        match label {
            "clock" => Self::Clock,
            "concurrency" => Self::Concurrency,
            "external" => Self::External,
            "filesystem" => Self::Filesystem,
            "io" => Self::Io,
            "state(local)" => Self::LocalState,
            "measurement" => Self::Measurement,
            "network" => Self::Network,
            "provenance" => Self::Provenance,
            "random(seed)" => Self::RandomSeed,
            "unsafe" => Self::Unsafe,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Clock => "clock",
            Self::Concurrency => "concurrency",
            Self::External => "external",
            Self::Filesystem => "filesystem",
            Self::Io => "io",
            Self::LocalState => "state(local)",
            Self::Measurement => "measurement",
            Self::Network => "network",
            Self::Provenance => "provenance",
            Self::RandomSeed => "random(seed)",
            Self::Unsafe => "unsafe",
            Self::Unknown(label) => label,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSite {
    pub effect: String,
    pub operation: String,
    pub function: String,
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

/// Source location for a call whose effect remains unresolved. Multiple
/// entries may refer to the same distinct function-operation edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedCallSite {
    pub operation: String,
    pub function: String,
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

/// Source location for an invocation through a function-valued expression.
/// The call can still be open or symbolically tracked; this records only that
/// the invocation is indirect, not that its effects are known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndirectCallSite {
    pub operation: String,
    pub function: String,
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

/// A local callsite substitution from one callback effect variable to another.
/// For a caller `wrapper(action) { invoke(action) }`, this records
/// `invoke.callback <- wrapper.action`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EffectVariableSubstitution {
    pub callee: String,
    pub callee_parameter: String,
    pub caller_parameter: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectReport {
    pub effects: BTreeSet<String>,
    /// Typed labels retained for the future HIR/IR effect row.
    pub kinds: BTreeSet<EffectKind>,
    /// Per-function effect rows after propagating effects across known local calls.
    pub function_kinds: BTreeMap<String, BTreeSet<EffectKind>>,
    /// First-class internal signatures after callback and local-call inference.
    pub function_effect_signatures: BTreeMap<String, EffectSignature>,
    /// Every function visited by the inventory, including rows with no known
    /// effect. An empty row is evidence for a candidate only; it is not a
    /// proof of purity because unresolved calls remain outside this report.
    pub function_names: BTreeSet<String>,
    pub free_function_names: BTreeSet<String>,
    /// Local callees resolved to a canonical function key, per caller.
    pub resolved_function_calls: BTreeMap<String, BTreeSet<String>>,
    /// Source spellings corresponding to the resolved edges, per caller.
    pub resolved_call_operations: BTreeMap<String, BTreeSet<String>>,
    /// Calls to typed compiler intrinsics with no host effect in the current
    /// effect model. This does not establish referential transparency.
    pub known_effect_free_calls: BTreeMap<String, BTreeSet<String>>,
    /// Calls whose operation has a resolved effect classification, including
    /// host effects. They are separate from locally resolved function edges.
    pub classified_calls: BTreeMap<String, BTreeSet<String>>,
    /// Impl methods available for a concrete receiver owner type.
    pub impl_methods_by_type: BTreeMap<String, BTreeMap<String, String>>,
    /// Receiver node types supplied by the type checker; absent in syntax-only reports.
    pub receiver_types: BTreeMap<usize, String>,
    /// Types of AST expressions, used to determine whether builtin destruction
    /// recursively releases task/channel handles with observable scheduler effects.
    pub expression_types: BTreeMap<usize, Ty>,
    /// Source record/enum payloads used by the conservative drop-effect summary.
    drop_type_definitions: BTreeMap<String, DropTypeDefinition>,
    /// Receiver expression nodes known by the checker to be lists.
    pub list_receiver_nodes: BTreeSet<usize>,
    pub option_receiver_nodes: BTreeSet<usize>,
    pub result_receiver_nodes: BTreeSet<usize>,
    /// Function-valued identifiers supplied by the type checker.
    pub function_value_nodes: BTreeSet<usize>,
    /// Source parameter names and which of them accept function values.
    pub function_parameters: BTreeMap<String, Vec<String>>,
    pub callback_parameters: BTreeMap<String, BTreeSet<String>>,
    /// Experimental internal effect variables for invoked callback parameters.
    pub function_effect_variables: BTreeMap<String, BTreeSet<String>>,
    /// Per-caller substitutions that preserve callback effect-variable identity
    /// across resolved local calls.
    pub effect_variable_substitutions: BTreeMap<String, BTreeSet<EffectVariableSubstitution>>,
    /// Callback contracts whose effects are substituted at a callsite because
    /// the actual function value has a known local body.
    pub closed_callback_parameters: BTreeMap<String, BTreeMap<String, BTreeSet<String>>>,
    /// Callback parameters captured by each lambda body, keyed by the lambda
    /// source key and then by their lexical owner and binding name.
    pub captured_callback_parameters: BTreeMap<String, BTreeSet<(String, String)>>,
    /// Lambda bodies returned with one statically resolved identity. This
    /// distinguishes closure creation from the later invocation of its body.
    returned_closure_keys: BTreeSet<String>,
    returned_closures_by_function: BTreeMap<String, BTreeSet<String>>,
    ambiguous_returned_closure_functions: BTreeSet<String>,
    pub function_calls: BTreeMap<String, BTreeSet<String>>,
    /// Compiler-known higher-order calls resolved from checked receiver types.
    pub builtin_callback_calls: BTreeMap<String, BTreeSet<String>>,
    /// Calls that could not be matched to a local function or a classified
    /// host operation. These make the conservative boundary explicit: a
    /// function with no known effect may still call one of these operations.
    pub unresolved_calls: BTreeMap<String, BTreeSet<String>>,
    /// All source locations corresponding to unresolved operation edges.
    /// Unlike `unresolved_calls`, this preserves duplicate callsites.
    pub unresolved_call_sites: Vec<UnresolvedCallSite>,
    unresolved_call_site_inventory: Vec<UnresolvedCallSite>,
    /// Source occurrences that invoke a function value rather than a named
    /// top-level function. Their effect status remains in the call maps.
    pub indirect_call_sites: Vec<IndirectCallSite>,
    /// Indirect invocations through function-valued bindings. Their effects
    /// are not present in the current function type, so they stay open.
    pub indirect_calls: BTreeMap<String, BTreeSet<String>>,
    /// Exact indirect-call markers accounted for by a symbolic callback row.
    pub symbolic_effect_calls: BTreeMap<String, BTreeSet<String>>,
    /// Direct and transitively inherited unresolved calls for each function.
    /// A non-empty set means the inferred row is open-world and cannot prove purity.
    pub open_effect_calls: BTreeMap<String, BTreeSet<String>>,
    pub sites: Vec<EffectSite>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct DropTypeDefinition {
    generics: Vec<String>,
    fields: Vec<Ty>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DropEffect {
    NoHostEffect,
    Concurrency,
    Unknown,
}

impl EffectReport {
    /// Return (concrete, symbolic, unresolved) distinct function-operation
    /// edge counts. The report stores operation names in sets, so repeated
    /// calls with the same spelling in one function count once. A symbolic
    /// callback edge is accounted for by its effect-row variable; it is not
    /// concrete/pure, but it is no longer an unmodeled edge.
    pub fn call_coverage(&self) -> (usize, usize, usize) {
        let total = self
            .function_calls
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        let symbolic = self
            .symbolic_effect_calls
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        let unresolved = self
            .unresolved_calls
            .values()
            .map(BTreeSet::len)
            .sum::<usize>();
        (
            total.saturating_sub(symbolic + unresolved),
            symbolic,
            unresolved,
        )
    }

    pub fn sort_sites(&mut self) {
        self.sites.sort_by(|left, right| {
            left.file
                .cmp(&right.file)
                .then(left.line.cmp(&right.line))
                .then(left.col.cmp(&right.col))
                .then(left.function.cmp(&right.function))
                .then(left.effect.cmp(&right.effect))
                .then(left.operation.cmp(&right.operation))
        });
    }
}

pub fn analyze_with_types(items: &[Item], typed: &TypedProgram) -> EffectReport {
    analyze_with_types_internal(items, typed, false)
}

/// Analyze typed effects and recover exact source callsite spans for reports.
/// Normal compilation uses `analyze_with_types` and does not reread or lex the
/// source files after parsing.
pub fn analyze_with_types_for_report(items: &[Item], typed: &TypedProgram) -> EffectReport {
    analyze_with_types_internal(items, typed, true)
}

fn analyze_with_types_internal(
    items: &[Item],
    typed: &TypedProgram,
    resolve_report_spans: bool,
) -> EffectReport {
    let mut report = EffectReport {
        expression_types: typed
            .node_types
            .iter()
            .map(|(node, ty)| (*node, ty.clone()))
            .collect(),
        drop_type_definitions: collect_drop_type_definitions(items),
        function_value_nodes: typed
            .node_types
            .iter()
            .filter_map(|(node, ty)| matches!(ty, Ty::Fn(..)).then_some(*node))
            .collect(),
        receiver_types: typed
            .node_types
            .iter()
            .filter_map(|(node, ty)| {
                let owner = match ty {
                    Ty::Named(name) => name.clone(),
                    Ty::Int => "@Int".to_string(),
                    Ty::Float => "@Float".to_string(),
                    Ty::Float32 => "@Float32".to_string(),
                    Ty::Bool => "@Bool".to_string(),
                    Ty::Char => "@Char".to_string(),
                    Ty::Sized(kind) => format!("@Sized({kind:?})"),
                    Ty::Fn(..) => "@Function".to_string(),
                    Ty::Applied(name, _)
                        if matches!(name.as_str(), "Array" | "Option" | "Result") =>
                    {
                        format!("@{name}")
                    }
                    Ty::Applied(name, _) => name.clone(),
                    Ty::List(_) => "@List".to_string(),
                    Ty::Map(_, _) => "@Map".to_string(),
                    Ty::Set(_) => "@Set".to_string(),
                    Ty::String => "@String".to_string(),
                    Ty::Quantity(_) => "@Quantity".to_string(),
                    _ => return None,
                };
                Some((*node, owner))
            })
            .collect(),
        list_receiver_nodes: typed
            .node_types
            .iter()
            .filter_map(|(node, ty)| matches!(ty, Ty::List(_)).then_some(*node))
            .collect(),
        option_receiver_nodes: typed
            .node_types
            .iter()
            .filter_map(|(node, ty)| {
                matches!(ty, Ty::Named(name) | Ty::Applied(name, _) if name == "Option")
                    .then_some(*node)
            })
            .collect(),
        result_receiver_nodes: typed
            .node_types
            .iter()
            .filter_map(|(node, ty)| {
                matches!(ty, Ty::Named(name) | Ty::Applied(name, _) if name == "Result")
                    .then_some(*node)
            })
            .collect(),
        ..EffectReport::default()
    };
    // Index every declaration before walking any body so forward calls resolve
    // the same way as backward calls.
    for item in items {
        match item {
            Item::Function(function) => {
                report.free_function_names.insert(function.name.clone());
            }
            Item::Impl(implementation) => {
                register_impl_methods(implementation, items, &mut report);
            }
            _ => {}
        }
    }
    for item in items {
        match item {
            Item::Function(function) => walk_function(function, &function.name, &mut report),
            Item::Impl(implementation) => walk_impl(implementation, items, &mut report),
            _ => {}
        }
    }
    classify_unresolved_calls(&mut report);
    propagate_function_effects(&mut report);
    infer_callback_effects(items, &mut report);
    propagate_function_effects(&mut report);
    let mut signature_open_calls = BTreeMap::<String, BTreeSet<String>>::new();
    for function in &report.function_names {
        let mut calls = report
            .unresolved_calls
            .get(function)
            .cloned()
            .unwrap_or_default();
        let variables = report
            .function_effect_variables
            .get(function)
            .cloned()
            .unwrap_or_default();
        calls.retain(|call| !variables.contains(call));
        for call in report.indirect_calls.get(function).into_iter().flatten() {
            let represented_by_variable = report
                .symbolic_effect_calls
                .get(function)
                .is_some_and(|symbolic| symbolic.contains(call));
            if !represented_by_variable {
                calls.insert(call.clone());
            }
        }
        if !calls.is_empty() {
            signature_open_calls.insert(function.clone(), calls);
        }
    }
    loop {
        let mut next = signature_open_calls.clone();
        for (caller, callees) in &report.resolved_function_calls {
            for callee in callees {
                let Some(calls) = signature_open_calls.get(callee) else {
                    continue;
                };
                let closed = report
                    .closed_callback_parameters
                    .get(caller)
                    .and_then(|targets| targets.get(callee));
                for call in calls {
                    if !closed.is_some_and(|parameters| parameters.contains(call)) {
                        next.entry(caller.clone()).or_default().insert(call.clone());
                    }
                }
            }
        }
        if next == signature_open_calls {
            break;
        }
        signature_open_calls = next;
    }
    report.function_effect_signatures = report
        .function_names
        .iter()
        .map(|function| {
            (
                function.clone(),
                EffectSignature {
                    effects: report
                        .function_kinds
                        .get(function)
                        .cloned()
                        .unwrap_or_default(),
                    variables: report
                        .function_effect_variables
                        .get(function)
                        .cloned()
                        .unwrap_or_default(),
                    open_calls: signature_open_calls
                        .get(function)
                        .cloned()
                        .unwrap_or_default(),
                    type_errors: typed.errors.len(),
                },
            )
        })
        .collect();
    report.sort_sites();
    collect_unresolved_call_sites(&mut report);
    if resolve_report_spans {
        resolve_unresolved_call_site_spans(&mut report);
    }
    report
}

fn collect_drop_type_definitions(items: &[Item]) -> BTreeMap<String, DropTypeDefinition> {
    let mut qualified = BTreeMap::new();
    let mut unqualified = BTreeMap::<String, Vec<DropTypeDefinition>>::new();
    for item in items {
        let (name, module_path, generics, fields) = match item {
            Item::Record(record) => (
                &record.name,
                &record.module_path,
                &record.generics,
                record
                    .fields
                    .iter()
                    .map(|field| &field.ty)
                    .collect::<Vec<_>>(),
            ),
            Item::Enum(enumeration) => (
                &enumeration.name,
                &enumeration.module_path,
                &enumeration.generics,
                enumeration
                    .variants
                    .iter()
                    .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
                    .collect::<Vec<_>>(),
            ),
            _ => continue,
        };
        let generic_parameters = generics
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<Vec<_>>();
        let generic_names = generic_parameters.iter().cloned().collect::<BTreeSet<_>>();
        let definition = DropTypeDefinition {
            generics: generic_parameters,
            fields: fields
                .iter()
                .map(|ty| declared_type_for_drop(ty, &generic_names))
                .collect(),
        };
        let key = if module_path.is_empty() {
            name.clone()
        } else {
            format!("{}::{name}", module_path.join("::"))
        };
        qualified.insert(key, definition.clone());
        unqualified
            .entry(name.clone())
            .or_default()
            .push(definition);
    }
    for (name, definitions) in unqualified {
        if let [definition] = definitions.as_slice() {
            qualified.insert(name, definition.clone());
        }
    }
    qualified
}

fn declared_type_for_drop(ty: &Type, generic_names: &BTreeSet<String>) -> Ty {
    match ty {
        Type::Named(name, args) if args.is_empty() && generic_names.contains(name) => {
            Ty::Generic(name.clone())
        }
        Type::Named(name, args) if args.is_empty() => match name.as_str() {
            "Int" => Ty::Int,
            "Float" => Ty::Float,
            "Float32" => Ty::Float32,
            "Bool" => Ty::Bool,
            "Char" => Ty::Char,
            "String" => Ty::String,
            "Void" => Ty::Void,
            name => Ty::Named(name.to_string()),
        },
        Type::Named(name, args) => Ty::Applied(
            name.clone(),
            args.iter()
                .map(|arg| declared_type_for_drop(arg, generic_names))
                .collect(),
        ),
        Type::Fn(params, result) => Ty::Fn(
            params
                .iter()
                .map(|param| declared_type_for_drop(param, generic_names))
                .collect(),
            Box::new(declared_type_for_drop(result, generic_names)),
        ),
        Type::Dyn(traits) => Ty::Dyn(traits.first().cloned().unwrap_or_default()),
        // Dimensional type expressions have scalar runtime representation and
        // no user-defined destructor.
        Type::Mul(_, _) | Type::Div(_, _) | Type::Pow(_, _) => Ty::Float,
    }
}

fn drop_effect_for_type(
    ty: &Ty,
    report: &EffectReport,
    visiting: &mut BTreeSet<String>,
) -> DropEffect {
    let key = ty.describe();
    if !visiting.insert(key.clone()) {
        // Recursive managed structures are safe to summarize once their
        // already-visited fields are being traversed; sibling fields continue.
        return DropEffect::NoHostEffect;
    }
    let effect = match ty {
        Ty::List(element) | Ty::Set(element) => drop_effect_for_type(element, report, visiting),
        Ty::Map(key, value) => combine_drop_effects(
            drop_effect_for_type(key, report, visiting),
            drop_effect_for_type(value, report, visiting),
        ),
        Ty::Applied(name, _args) if matches!(name.as_str(), "Task" | "Channel") => {
            DropEffect::Concurrency
        }
        Ty::Applied(name, args)
            if matches!(
                name.as_str(),
                "Array" | "List" | "Map" | "Set" | "Option" | "Result"
            ) =>
        {
            args.iter().fold(DropEffect::NoHostEffect, |effect, arg| {
                combine_drop_effects(effect, drop_effect_for_type(arg, report, visiting))
            })
        }
        Ty::Applied(name, _) if name == "Quantity" => DropEffect::NoHostEffect,
        Ty::Applied(name, args) => drop_effect_for_declared_type(name, args, report, visiting),
        Ty::Named(name) if matches!(name.as_str(), "Task" | "Channel") => DropEffect::Concurrency,
        Ty::Named(name) if name == "Rng" => DropEffect::NoHostEffect,
        Ty::Named(name) => drop_effect_for_declared_type(name, &[], report, visiting),
        Ty::Int
        | Ty::Float
        | Ty::Float32
        | Ty::Bool
        | Ty::Char
        | Ty::String
        | Ty::Void
        | Ty::Quantity(_)
        | Ty::TemperaturePoint
        | Ty::Sized(_) => DropEffect::NoHostEffect,
        Ty::Fn(..) | Ty::Generic(_) | Ty::Dyn(_) | Ty::Unknown => DropEffect::Unknown,
    };
    visiting.remove(&key);
    effect
}

fn drop_effect_for_declared_type(
    name: &str,
    args: &[Ty],
    report: &EffectReport,
    visiting: &mut BTreeSet<String>,
) -> DropEffect {
    let Some(definition) = report.drop_type_definitions.get(name) else {
        return DropEffect::Unknown;
    };
    if definition.generics.len() != args.len() {
        return DropEffect::Unknown;
    }
    let substitutions = definition
        .generics
        .iter()
        .cloned()
        .zip(args.iter().cloned())
        .collect::<BTreeMap<_, _>>();
    definition
        .fields
        .iter()
        .fold(DropEffect::NoHostEffect, |effect, field| {
            combine_drop_effects(
                effect,
                drop_effect_for_type(
                    &substitute_drop_generics(field, &substitutions),
                    report,
                    visiting,
                ),
            )
        })
}

fn substitute_drop_generics(ty: &Ty, substitutions: &BTreeMap<String, Ty>) -> Ty {
    match ty {
        Ty::Generic(name) => substitutions
            .get(name)
            .cloned()
            .unwrap_or_else(|| ty.clone()),
        Ty::List(element) => Ty::List(Box::new(substitute_drop_generics(element, substitutions))),
        Ty::Set(element) => Ty::Set(Box::new(substitute_drop_generics(element, substitutions))),
        Ty::Map(key, value) => Ty::Map(
            Box::new(substitute_drop_generics(key, substitutions)),
            Box::new(substitute_drop_generics(value, substitutions)),
        ),
        Ty::Applied(name, args) => Ty::Applied(
            name.clone(),
            args.iter()
                .map(|arg| substitute_drop_generics(arg, substitutions))
                .collect(),
        ),
        _ => ty.clone(),
    }
}

fn combine_drop_effects(left: DropEffect, right: DropEffect) -> DropEffect {
    match (left, right) {
        (DropEffect::Unknown, _) | (_, DropEffect::Unknown) => DropEffect::Unknown,
        (DropEffect::Concurrency, _) | (_, DropEffect::Concurrency) => DropEffect::Concurrency,
        _ => DropEffect::NoHostEffect,
    }
}

fn classify_unresolved_calls(report: &mut EffectReport) {
    for (caller, calls) in &report.function_calls {
        for operation in calls {
            if !report
                .classified_calls
                .get(caller)
                .is_some_and(|calls| calls.contains(operation))
                && !report
                    .known_effect_free_calls
                    .get(caller)
                    .is_some_and(|calls| calls.contains(operation))
                && !report
                    .resolved_call_operations
                    .get(caller)
                    .is_some_and(|resolved| resolved.contains(operation))
                && !report
                    .builtin_callback_calls
                    .get(caller)
                    .is_some_and(|calls| calls.contains(operation))
            {
                report
                    .unresolved_calls
                    .entry(caller.clone())
                    .or_default()
                    .insert(operation.clone());
            }
        }
    }
}

fn collect_unresolved_call_sites(report: &mut EffectReport) {
    report.unresolved_call_sites = report
        .unresolved_call_site_inventory
        .iter()
        .filter(|site| {
            report
                .unresolved_calls
                .get(&site.function)
                .is_some_and(|calls| calls.contains(&site.operation))
        })
        .cloned()
        .collect();
    report.unresolved_call_sites.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.line.cmp(&right.line))
            .then(left.col.cmp(&right.col))
            .then(left.function.cmp(&right.function))
            .then(left.operation.cmp(&right.operation))
    });
}

/// Recover the callee token location for unresolved calls nested inside a
/// larger located expression. The parser currently attaches some source spans
/// to the enclosing expression, which can otherwise leave these callsites at
/// 0:0. We only replace a group when lexical and AST occurrence counts agree;
/// ambiguous groups retain their original conservative locations.
fn resolve_unresolved_call_site_spans(report: &mut EffectReport) {
    let mut files = BTreeSet::new();
    for site in &report.unresolved_call_sites {
        if let Some(file) = &site.file {
            files.insert(file.clone());
        }
    }

    let mut candidates = BTreeMap::<(String, String), Vec<(usize, usize)>>::new();
    for file in files {
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(tokens) = Lexer::new(&source).tokenize() else {
            continue;
        };
        for (index, token) in tokens.iter().enumerate() {
            let TokenKind::Ident(_) = &token.kind else {
                continue;
            };
            if index > 0 && matches!(tokens[index - 1].kind, TokenKind::Fn) {
                continue;
            }
            if !follows_call_arguments(&tokens, index + 1) {
                continue;
            }
            let mut start = index;
            while start >= 2
                && matches!(
                    tokens[start - 1].kind,
                    TokenKind::Dot | TokenKind::ColonColon
                )
                && matches!(tokens[start - 2].kind, TokenKind::Ident(_))
            {
                start -= 2;
            }
            let first = &tokens[start];
            let operation = tokens[start..=index]
                .iter()
                .map(|token| token.lexeme.as_str())
                .collect::<String>();
            candidates
                .entry((file.clone(), operation))
                .or_default()
                .push((first.line, first.col));
        }
    }

    let mut groups = BTreeMap::<(String, String), Vec<usize>>::new();
    for (index, site) in report.unresolved_call_sites.iter().enumerate() {
        let Some(file) = &site.file else {
            continue;
        };
        groups
            .entry((file.clone(), site.operation.clone()))
            .or_default()
            .push(index);
    }

    for (key, mut site_indices) in groups {
        let Some(spans) = candidates.get(&key) else {
            continue;
        };
        if spans.len() != site_indices.len() {
            continue;
        }
        site_indices.sort_by_key(|index| {
            let site = &report.unresolved_call_sites[*index];
            (site.line, site.col)
        });
        for (site_index, (line, col)) in site_indices.into_iter().zip(spans) {
            let site = &mut report.unresolved_call_sites[site_index];
            site.line = *line;
            site.col = *col;
        }
    }
    report.unresolved_call_sites.sort_by(|left, right| {
        left.file
            .cmp(&right.file)
            .then(left.line.cmp(&right.line))
            .then(left.col.cmp(&right.col))
            .then(left.function.cmp(&right.function))
            .then(left.operation.cmp(&right.operation))
    });
}

fn follows_call_arguments(tokens: &[Token], mut index: usize) -> bool {
    if matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::LParen)
    ) {
        return true;
    }
    if !matches!(
        tokens.get(index).map(|token| &token.kind),
        Some(TokenKind::Lt)
    ) {
        return false;
    }
    let mut depth = 0usize;
    while let Some(token) = tokens.get(index) {
        match token.kind {
            TokenKind::Lt => depth += 1,
            TokenKind::Gt => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return matches!(
                        tokens.get(index + 1).map(|token| &token.kind),
                        Some(TokenKind::LParen)
                    );
                }
            }
            TokenKind::Eof | TokenKind::RBrace => return false,
            _ => {}
        }
        index += 1;
    }
    false
}

fn walk_impl(implementation: &ImplDecl, items: &[Item], report: &mut EffectReport) {
    for method in &implementation.methods {
        let key = impl_method_key(&implementation.type_name, &method.name);
        walk_function(method, &key, report);
    }
    for (trait_decl, method) in default_methods(implementation, items) {
        let function = trait_default_as_function(trait_decl, method, implementation);
        let key = impl_method_key(&implementation.type_name, &method.name);
        walk_function(&function, &key, report);
    }
}

fn register_impl_methods(implementation: &ImplDecl, items: &[Item], report: &mut EffectReport) {
    for method in &implementation.methods {
        report
            .impl_methods_by_type
            .entry(implementation.type_name.clone())
            .or_default()
            .insert(
                method.name.clone(),
                impl_method_key(&implementation.type_name, &method.name),
            );
    }
    for (_, method) in default_methods(implementation, items) {
        report
            .impl_methods_by_type
            .entry(implementation.type_name.clone())
            .or_default()
            .insert(
                method.name.clone(),
                impl_method_key(&implementation.type_name, &method.name),
            );
    }
}

fn default_methods<'a>(
    implementation: &ImplDecl,
    items: &'a [Item],
) -> Vec<(&'a TraitDecl, &'a TraitMethodSig)> {
    let Some(trait_name) = &implementation.trait_name else {
        return Vec::new();
    };
    let Some(trait_decl) = items.iter().find_map(|item| match item {
        Item::Trait(decl) if &decl.name == trait_name => Some(decl),
        _ => None,
    }) else {
        return Vec::new();
    };
    trait_decl
        .methods
        .iter()
        .filter(|method| {
            method.default_body.is_some()
                && !implementation
                    .methods
                    .iter()
                    .any(|overridden| overridden.name == method.name)
        })
        .map(|method| (trait_decl, method))
        .collect()
}

fn trait_default_as_function(
    trait_decl: &TraitDecl,
    method: &TraitMethodSig,
    implementation: &ImplDecl,
) -> FunctionDecl {
    let mut generics = trait_decl.generics.clone();
    generics.extend(implementation.generics.clone());
    generics.extend(method.generics.clone());
    FunctionDecl {
        name: impl_method_key(&implementation.type_name, &method.name),
        is_pub: false,
        generics,
        params: method.params.clone(),
        return_type: method.return_type.clone(),
        body: method
            .default_body
            .as_ref()
            .expect("default method selected above")
            .clone(),
        span: trait_decl.span,
        source_file: trait_decl.source_file.clone(),
    }
}

pub(crate) fn impl_method_key(owner: &str, method: &str) -> String {
    format!("{owner}.{method}")
}

fn walk_function(function: &FunctionDecl, function_key: &str, report: &mut EffectReport) {
    report.function_names.insert(function_key.to_string());
    register_function_parameters(&function.params, function_key, report);
    for parameter in &function.params {
        if let Some(default) = &parameter.default {
            walk_expr(
                default,
                function_key,
                function.source_file.as_deref(),
                report,
            );
        }
    }
    walk_block(
        &function.body,
        function_key,
        function.source_file.as_deref(),
        report,
    );
}

fn walk_block(block: &Block, function: &str, file: Option<&str>, report: &mut EffectReport) {
    for located in &block.stmts {
        match &located.stmt {
            Stmt::Binding { value, .. } | Stmt::Assign { value, .. } => {
                walk_expr(value, function, file, report)
            }
            Stmt::Return(value) | Stmt::Break(value) => {
                if let Some(value) = value {
                    walk_expr(value, function, file, report);
                }
            }
            Stmt::Continue => {}
            Stmt::For { iter, body, .. } => {
                walk_expr(iter, function, file, report);
                walk_block(body, function, file, report);
            }
            Stmt::While { cond, body } => {
                walk_expr(cond, function, file, report);
                walk_block(body, function, file, report);
            }
            Stmt::FieldAssign { target, value } => {
                record(
                    report,
                    "state(local)",
                    "field_assignment",
                    function,
                    file,
                    located.span.line,
                    located.span.col,
                );
                walk_expr(target, function, file, report);
                walk_expr(value, function, file, report);
            }
            Stmt::Expr(expr) => walk_expr(expr, function, file, report),
        }
    }
    if let Some(tail) = &block.tail {
        walk_expr(tail, function, file, report);
    }
}

fn walk_expr(expr: &Expr, function: &str, file: Option<&str>, report: &mut EffectReport) {
    let mut expr = expr;
    let mut line = 0;
    let mut col = 0;
    while let Expr::Located(inner, range) = expr {
        if line == 0 {
            line = range.start.line;
            col = range.start.col;
        }
        expr = inner;
    }
    match expr {
        Expr::Located(inner, _) => walk_expr(inner, function, file, report),
        Expr::Call(callee, args) | Expr::GenericCall(callee, _, args) => {
            if let Some(operation) = expression_path(callee) {
                report
                    .unresolved_call_site_inventory
                    .push(UnresolvedCallSite {
                        operation: operation.clone(),
                        function: function.to_string(),
                        file: file.map(str::to_string),
                        line,
                        col,
                    });
                report
                    .function_calls
                    .entry(function.to_string())
                    .or_default()
                    .insert(operation.clone());
                if let Some(target) = resolve_local_target(callee, report) {
                    report
                        .resolved_function_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(target.clone());
                    report
                        .resolved_call_operations
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                } else if builtin_callback_target(callee, report).is_some() {
                    report
                        .builtin_callback_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                }
                if let Some(effect) = classify_effect_call(callee, &operation, report) {
                    report
                        .classified_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                    add_effect(report, function, effect);
                    report.sites.push(EffectSite {
                        effect: effect.to_string(),
                        operation: operation.clone(),
                        function: function.to_string(),
                        file: file.map(str::to_string),
                        line,
                        col,
                    });
                } else if let Some(drop_effect) = classify_drop_call(callee, args, report) {
                    report
                        .classified_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                    match drop_effect {
                        DropEffect::NoHostEffect => {
                            report
                                .known_effect_free_calls
                                .entry(function.to_string())
                                .or_default()
                                .insert(operation.clone());
                        }
                        DropEffect::Concurrency => {
                            add_effect(report, function, "concurrency");
                            report.sites.push(EffectSite {
                                effect: "concurrency".to_string(),
                                operation: operation.clone(),
                                function: function.to_string(),
                                file: file.map(str::to_string),
                                line,
                                col,
                            });
                        }
                        DropEffect::Unknown => unreachable!("unknown drops remain unresolved"),
                    }
                } else if is_known_local_mutation_call(callee, report) {
                    report
                        .classified_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                    record(
                        report,
                        "state(local)",
                        &operation,
                        function,
                        file,
                        line,
                        col,
                    );
                } else if is_known_effect_free_call(callee, report) {
                    report
                        .classified_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                    report
                        .known_effect_free_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                }
                if is_indirect_function_call(callee, report) {
                    report
                        .indirect_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                    report.indirect_call_sites.push(IndirectCallSite {
                        operation: operation.clone(),
                        function: function.to_string(),
                        file: file.map(str::to_string),
                        line,
                        col,
                    });
                }
            }
            walk_expr(callee, function, file, report);
            for arg in args {
                match arg {
                    Arg::Positional(value) | Arg::Named(_, value) => {
                        walk_expr(value, function, file, report)
                    }
                }
            }
        }
        Expr::Spawn(block) | Expr::SpawnScope(block) => {
            record(report, "concurrency", "spawn", function, file, line, col);
            walk_block(block, function, file, report);
        }
        Expr::Channel(_, capacity) => {
            record(report, "concurrency", "channel", function, file, line, col);
            if let Some(capacity) = capacity {
                walk_expr(capacity, function, file, report);
            }
        }
        Expr::IntLiteral(_)
        | Expr::SizedIntLiteral(_, _)
        | Expr::FloatLiteral(_)
        | Expr::Float32Literal(_)
        | Expr::StringLiteral(_)
        | Expr::CharLiteral(_)
        | Expr::BoolLiteral(_)
        | Expr::Ident(_) => {}
        Expr::UnitLiteral(value, _) | Expr::Unary(_, value) => {
            walk_expr(value, function, file, report)
        }
        Expr::Try(value, catch) => {
            walk_expr(value, function, file, report);
            if let Some(catch) = catch {
                walk_expr(catch, function, file, report);
            }
        }
        Expr::Binary(_, left, right)
        | Expr::Range(left, _, right, _)
        | Expr::Within(left, right)
        | Expr::Approximately(left, right, _) => {
            walk_expr(left, function, file, report);
            walk_expr(right, function, file, report);
            if let Expr::Range(_, _, _, Some(step)) = expr {
                walk_expr(step, function, file, report);
            }
            if let Expr::Approximately(_, _, tolerance) = expr {
                walk_expr(tolerance, function, file, report);
            }
        }
        Expr::FieldAccess(receiver, _) | Expr::Index(receiver, _) => {
            walk_expr(receiver, function, file, report);
            if let Expr::Index(_, index) = expr {
                walk_expr(index, function, file, report);
            }
        }
        Expr::If(cond, yes, no) => {
            walk_expr(cond, function, file, report);
            walk_block(yes, function, file, report);
            if let Some(no) = no {
                walk_block(no, function, file, report);
            }
        }
        Expr::Block(block) | Expr::Loop(block) => walk_block(block, function, file, report),
        // Creating a closure does not execute its body. The current function
        // type omits effect rows, so keep the closure's own effects in a
        // separate inventory entry instead of attributing them to its creator.
        Expr::Lambda(params, block) => {
            let lambda_key = format!("{function}::<lambda@{line}:{col}>");
            report.function_names.insert(lambda_key.clone());
            register_lambda_parameters(params, &lambda_key, report);
            walk_block(block, &lambda_key, file, report);
        }
        Expr::ListLiteral(values) | Expr::SetLiteral(values) => {
            for value in values {
                walk_expr(value, function, file, report);
            }
        }
        Expr::MapLiteral(entries) => {
            for (key, value) in entries {
                walk_expr(key, function, file, report);
                walk_expr(value, function, file, report);
            }
        }
        Expr::EmptyCollection(_, _) => {}
        Expr::As(value, target) => {
            walk_expr(value, function, file, report);
            walk_expr(target, function, file, report);
        }
        Expr::RecordLiteral(_, fields) | Expr::GenericRecordLiteral(_, _, fields) => {
            for (_, value) in fields {
                walk_expr(value, function, file, report);
            }
        }
        Expr::Match(value, arms) => {
            walk_expr(value, function, file, report);
            for arm in arms {
                walk_arm(arm, function, file, report);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CallbackSource {
    Function(String),
    Parameter {
        function: String,
        name: String,
    },
    ReturnedClosure {
        function: String,
        captures: BTreeMap<String, Box<CallbackSource>>,
    },
    Unknown,
}

#[derive(Debug, Clone, Default)]
struct CallbackScope {
    names: BTreeSet<String>,
    aliases: BTreeMap<String, CallbackSource>,
}

#[derive(Debug, Clone)]
struct CallbackCallsite {
    caller: String,
    target: String,
    target_parameter: String,
    source: CallbackSource,
    operation: String,
    file: Option<String>,
    line: usize,
    col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CallbackParameter {
    function: String,
    name: String,
}

fn register_function_parameters(
    params: &[crate::ast::Param],
    function: &str,
    report: &mut EffectReport,
) {
    report.function_parameters.insert(
        function.to_string(),
        params
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect(),
    );
    let callback_parameters = params
        .iter()
        .filter_map(|parameter| {
            matches!(&parameter.ty, Type::Fn(_, _)).then(|| parameter.name.clone())
        })
        .collect::<BTreeSet<_>>();
    if !callback_parameters.is_empty() {
        report
            .callback_parameters
            .insert(function.to_string(), callback_parameters);
    }
}

fn register_lambda_parameters(params: &[String], function: &str, report: &mut EffectReport) {
    // Lambda parameters are contextually typed, so their declared function
    // type is not retained in the AST. The checker validates each use before
    // this analysis runs; only invocation/forwarding evidence below treats a
    // parameter as a callback.
    report
        .function_parameters
        .insert(function.to_string(), params.to_vec());
    if !params.is_empty() {
        report
            .callback_parameters
            .insert(function.to_string(), params.iter().cloned().collect());
    }
}

fn register_directly_returned_closures(block: &Block, function: &str, report: &mut EffectReport) {
    fn closure_keys_from_expr(
        expression: &Expr,
        function: &str,
        aliases: &BTreeMap<String, String>,
    ) -> (BTreeSet<String>, bool) {
        match expression.unlocated() {
            Expr::Lambda(..) => (
                BTreeSet::from([lambda_function_key(expression, function)]),
                true,
            ),
            Expr::Ident(name) => aliases
                .get(name)
                .cloned()
                .map(|key| (BTreeSet::from([key]), true))
                .unwrap_or_default(),
            Expr::Block(block) => closure_keys_from_block(block, function, aliases),
            Expr::If(_, then_block, else_block) => {
                let (mut keys, mut complete) =
                    closure_keys_from_block(then_block, function, aliases);
                if let Some(else_block) = else_block {
                    let (else_keys, else_complete) =
                        closure_keys_from_block(else_block, function, aliases);
                    keys.extend(else_keys);
                    complete &= else_complete;
                } else {
                    complete = false;
                }
                (keys, complete)
            }
            Expr::Match(_, arms) => {
                let mut keys = BTreeSet::new();
                let has_unguarded_catch_all = arms.iter().any(|arm| {
                    arm.guard.is_none()
                        && matches!(arm.pattern, Pattern::Wildcard | Pattern::Ident(_))
                });
                let mut complete = !arms.is_empty() && has_unguarded_catch_all;
                for arm in arms {
                    let (arm_keys, arm_complete) =
                        closure_keys_from_block(&arm.body, function, aliases);
                    keys.extend(arm_keys);
                    complete &= arm.guard.is_none() && arm_complete;
                }
                (keys, complete)
            }
            _ => (BTreeSet::new(), false),
        }
    }

    fn closure_keys_from_block(
        block: &Block,
        function: &str,
        inherited_aliases: &BTreeMap<String, String>,
    ) -> (BTreeSet<String>, bool) {
        let mut aliases = inherited_aliases.clone();
        let mut returned = BTreeSet::<String>::new();
        let mut complete = true;
        for located in &block.stmts {
            match &located.stmt {
                Stmt::Binding { name, value, .. } | Stmt::Assign { name, value } => {
                    let (keys, value_complete) = closure_keys_from_expr(value, function, &aliases);
                    if value_complete && keys.len() == 1 {
                        aliases.insert(name.clone(), keys.into_iter().next().unwrap());
                    } else {
                        aliases.remove(name);
                    }
                }
                Stmt::Return(Some(value)) => {
                    let (keys, value_complete) = closure_keys_from_expr(value, function, &aliases);
                    returned.extend(keys);
                    complete &= value_complete;
                }
                Stmt::Return(None) => complete = false,
                Stmt::For { .. } | Stmt::While { .. } => {
                    // Returns nested in these constructs are not yet associated
                    // with a complete function result on every path.
                    complete = false;
                }
                Stmt::Expr(value)
                    if matches!(
                        value.unlocated(),
                        Expr::If(..) | Expr::Block(..) | Expr::Loop(..) | Expr::Match(..)
                    ) =>
                {
                    // The parser wraps expressions with source locations, so
                    // inspect the semantic node to catch nested control flow.
                    complete = false;
                }
                Stmt::FieldAssign { .. } | Stmt::Expr(_) | Stmt::Break(_) | Stmt::Continue => {}
            }
        }
        if let Some(tail) = &block.tail {
            let (keys, tail_complete) = closure_keys_from_expr(tail, function, &aliases);
            returned.extend(keys);
            complete &= tail_complete;
        } else if returned.is_empty() {
            complete = false;
        }
        (returned, complete)
    }

    let (returned, complete) = closure_keys_from_block(block, function, &BTreeMap::new());
    if !complete {
        report
            .ambiguous_returned_closure_functions
            .insert(function.to_string());
    }
    if complete && returned.len() == 1 {
        report
            .returned_closure_keys
            .extend(returned.iter().cloned());
    }
    for lambda in returned {
        report
            .returned_closures_by_function
            .entry(function.to_string())
            .or_default()
            .insert(lambda);
    }
}

fn infer_callback_effects(items: &[Item], report: &mut EffectReport) {
    report.symbolic_effect_calls.clear();
    report.returned_closure_keys.clear();
    report.returned_closures_by_function.clear();
    report.ambiguous_returned_closure_functions.clear();
    for item in items {
        if let Item::Function(function) = item {
            register_directly_returned_closures(&function.body, &function.name, report);
        }
    }
    let mut invoked = BTreeSet::<CallbackParameter>::new();
    let mut forwarded = Vec::<(CallbackParameter, CallbackParameter)>::new();
    let mut callsites = Vec::<CallbackCallsite>::new();
    loop {
        let captures_before = report.captured_callback_parameters.clone();
        invoked.clear();
        forwarded.clear();
        callsites.clear();
        for item in items {
            match item {
                Item::Function(function) => collect_callback_block(
                    &function.body,
                    &function.name,
                    function.source_file.as_deref(),
                    report,
                    &mut Vec::new(),
                    &mut invoked,
                    &mut forwarded,
                    &mut callsites,
                ),
                Item::Impl(implementation) => {
                    for method in &implementation.methods {
                        collect_callback_block(
                            &method.body,
                            &impl_method_key(&implementation.type_name, &method.name),
                            method.source_file.as_deref(),
                            report,
                            &mut Vec::new(),
                            &mut invoked,
                            &mut forwarded,
                            &mut callsites,
                        );
                    }
                    for (trait_decl, method) in default_methods(implementation, items) {
                        let function =
                            trait_default_as_function(trait_decl, method, implementation);
                        collect_callback_block(
                            &function.body,
                            &impl_method_key(&implementation.type_name, &method.name),
                            function.source_file.as_deref(),
                            report,
                            &mut Vec::new(),
                            &mut invoked,
                            &mut forwarded,
                            &mut callsites,
                        );
                    }
                }
                _ => {}
            }
        }
        if report.captured_callback_parameters == captures_before {
            break;
        }
    }

    // A parameter is effectively invoked if the function calls it directly or
    // forwards it to another callable parameter that is itself invoked. This
    // fixed point handles wrappers and recursive forwarding without guessing
    // from the call site's argument position alone.
    loop {
        let before = invoked.len();
        for (source, target) in &forwarded {
            if invoked.contains(target) {
                invoked.insert(source.clone());
            }
        }
        if invoked.len() == before {
            break;
        }
    }

    // Preserve callback parameter identity as an internal effect variable
    // (conceptually ε_parameter), without defining public syntax.
    report.function_effect_variables.clear();
    for parameter in &invoked {
        if report.function_names.contains(&parameter.function) {
            report
                .function_effect_variables
                .entry(parameter.function.clone())
                .or_default()
                .insert(parameter.name.clone());
        }
    }

    // A parameter may be substituted only when every observed callsite from
    // this caller to this callee/formal supplies a known local function body.
    // One known call must never mask a sibling callsite with an open callback.
    let mut callback_callsite_resolution = BTreeMap::<(String, String, String), bool>::new();
    for callsite in &callsites {
        if invoked.contains(&CallbackParameter {
            function: callsite.target.clone(),
            name: callsite.target_parameter.clone(),
        }) {
            let key = (
                callsite.caller.clone(),
                callsite.target.clone(),
                callsite.target_parameter.clone(),
            );
            let known = matches!(
                &callsite.source,
                CallbackSource::Function(_) | CallbackSource::ReturnedClosure { .. }
            );
            callback_callsite_resolution
                .entry(key)
                .and_modify(|all_known| *all_known &= known)
                .or_insert(known);
        }
    }
    report.closed_callback_parameters.clear();
    for ((caller, target, parameter), all_known) in callback_callsite_resolution {
        if all_known {
            report
                .closed_callback_parameters
                .entry(caller)
                .or_default()
                .entry(target)
                .or_default()
                .insert(parameter);
        }
    }

    report.effect_variable_substitutions.clear();
    for callsite in &callsites {
        let CallbackSource::Parameter { function, name } = &callsite.source else {
            continue;
        };
        // Built-in combinators do not have standalone HIR/IR function bodies;
        // their callback contract is validated by typed builtin-call analysis.
        if callsite.target.starts_with("@builtin.")
            || function != &callsite.caller
            || !invoked.contains(&CallbackParameter {
                function: callsite.target.clone(),
                name: callsite.target_parameter.clone(),
            })
            || !invoked.contains(&CallbackParameter {
                function: function.clone(),
                name: name.clone(),
            })
        {
            continue;
        }
        report
            .effect_variable_substitutions
            .entry(callsite.caller.clone())
            .or_default()
            .insert(EffectVariableSubstitution {
                callee: callsite.target.clone(),
                callee_parameter: callsite.target_parameter.clone(),
                caller_parameter: name.clone(),
            });
    }

    let mut recorded_sites = BTreeSet::<(String, usize, usize, String)>::new();
    loop {
        let rows_before = report.function_kinds.clone();
        let open_before = report.open_effect_calls.clone();
        for callsite in &callsites {
            if !invoked.contains(&CallbackParameter {
                function: callsite.target.clone(),
                name: callsite.target_parameter.clone(),
            }) {
                continue;
            }
            match &callsite.source {
                CallbackSource::Function(source) => {
                    if let Some(effects) = report.function_kinds.get(source).cloned() {
                        report
                            .function_kinds
                            .entry(callsite.caller.clone())
                            .or_default()
                            .extend(effects.iter().cloned());
                        for effect in effects {
                            let key = (
                                callsite.caller.clone(),
                                callsite.line,
                                callsite.col,
                                effect.label().to_string(),
                            );
                            if recorded_sites.insert(key) {
                                report.sites.push(EffectSite {
                                    effect: effect.label().to_string(),
                                    operation: callsite.operation.clone(),
                                    function: callsite.caller.clone(),
                                    file: callsite.file.clone(),
                                    line: callsite.line,
                                    col: callsite.col,
                                });
                            }
                        }
                    }
                    if let Some(open) = report.open_effect_calls.get(source).cloned() {
                        report
                            .indirect_calls
                            .entry(callsite.caller.clone())
                            .or_default()
                            .extend(open);
                    }
                }
                CallbackSource::Parameter { name, .. } => {
                    if callsite.target.starts_with("@builtin.") {
                        let marker = format!("{}.{}", callsite.operation, name);
                        report
                            .indirect_calls
                            .entry(callsite.caller.clone())
                            .or_default()
                            .insert(marker.clone());
                        if let CallbackSource::Parameter { function, .. } = &callsite.source {
                            if function == &callsite.caller {
                                report
                                    .symbolic_effect_calls
                                    .entry(callsite.caller.clone())
                                    .or_default()
                                    .insert(marker);
                            }
                        }
                    }
                }
                CallbackSource::Unknown => {
                    report
                        .indirect_calls
                        .entry(callsite.caller.clone())
                        .or_default()
                        .insert(format!(
                            "{}.{}",
                            callsite.operation, callsite.target_parameter
                        ));
                }
                CallbackSource::ReturnedClosure { .. } => {
                    report
                        .indirect_calls
                        .entry(callsite.caller.clone())
                        .or_default()
                        .insert(format!(
                            "{}.{}",
                            callsite.operation, callsite.target_parameter
                        ));
                }
            }
        }
        propagate_function_effects(report);
        if report.function_kinds == rows_before && report.open_effect_calls == open_before {
            break;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_callback_block(
    block: &Block,
    function: &str,
    file: Option<&str>,
    report: &mut EffectReport,
    shadowed: &mut Vec<CallbackScope>,
    invoked: &mut BTreeSet<CallbackParameter>,
    forwarded: &mut Vec<(CallbackParameter, CallbackParameter)>,
    callsites: &mut Vec<CallbackCallsite>,
) {
    shadowed.push(CallbackScope::default());
    for located in &block.stmts {
        match &located.stmt {
            Stmt::Binding { name, value, .. } => {
                let source = callback_source(value, function, shadowed, report);
                collect_callback_expr(
                    value, function, file, report, shadowed, invoked, forwarded, callsites,
                );
                shadowed
                    .last_mut()
                    .expect("scope exists")
                    .names
                    .insert(name.clone());
                shadowed
                    .last_mut()
                    .expect("scope exists")
                    .aliases
                    .insert(name.clone(), source);
            }
            Stmt::Assign { name, value } => {
                let source = callback_source(value, function, shadowed, report);
                collect_callback_expr(
                    value, function, file, report, shadowed, invoked, forwarded, callsites,
                );
                if let Some(index) = shadowed
                    .iter_mut()
                    .rposition(|scope| scope.names.contains(name))
                {
                    let current = index + 1 == shadowed.len();
                    shadowed[index].aliases.insert(
                        name.clone(),
                        if current {
                            source
                        } else {
                            CallbackSource::Unknown
                        },
                    );
                } else {
                    let scope = shadowed.last_mut().expect("scope exists");
                    scope.names.insert(name.clone());
                    scope.aliases.insert(name.clone(), CallbackSource::Unknown);
                }
            }
            Stmt::Expr(value) => {
                collect_callback_expr(
                    value, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
            Stmt::Return(value) | Stmt::Break(value) => {
                if let Some(value) = value {
                    collect_callback_expr(
                        value, function, file, report, shadowed, invoked, forwarded, callsites,
                    );
                }
            }
            Stmt::For {
                pattern,
                iter,
                body,
            } => {
                collect_callback_expr(
                    iter, function, file, report, shadowed, invoked, forwarded, callsites,
                );
                shadowed.push(CallbackScope {
                    names: BTreeSet::from([pattern.clone()]),
                    aliases: BTreeMap::from([(pattern.clone(), CallbackSource::Unknown)]),
                });
                collect_callback_block(
                    body, function, file, report, shadowed, invoked, forwarded, callsites,
                );
                shadowed.pop();
            }
            Stmt::While { cond, body } => {
                collect_callback_expr(
                    cond, function, file, report, shadowed, invoked, forwarded, callsites,
                );
                collect_callback_block(
                    body, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
            Stmt::FieldAssign { target, value } => {
                collect_callback_expr(
                    target, function, file, report, shadowed, invoked, forwarded, callsites,
                );
                collect_callback_expr(
                    value, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
            Stmt::Continue => {}
        }
    }
    if let Some(tail) = &block.tail {
        collect_callback_expr(
            tail, function, file, report, shadowed, invoked, forwarded, callsites,
        );
    }
    shadowed.pop();
}

#[allow(clippy::too_many_arguments)]
fn collect_callback_expr(
    expr: &Expr,
    function: &str,
    file: Option<&str>,
    report: &mut EffectReport,
    shadowed: &mut Vec<CallbackScope>,
    invoked: &mut BTreeSet<CallbackParameter>,
    forwarded: &mut Vec<(CallbackParameter, CallbackParameter)>,
    callsites: &mut Vec<CallbackCallsite>,
) {
    let (line, col) = expression_position(expr);
    match expr.unlocated() {
        Expr::Call(callee, args) | Expr::GenericCall(callee, _, args) => {
            let callee_source = callback_source(callee, function, shadowed, report);
            let callback_callee = match callee_source.clone() {
                CallbackSource::Parameter {
                    function: parameter_function,
                    name,
                } => {
                    let operation = expression_path(callee).unwrap_or_default();
                    report
                        .indirect_calls
                        .entry(function.to_string())
                        .or_default()
                        .remove(&operation);
                    report
                        .unresolved_calls
                        .entry(function.to_string())
                        .or_default()
                        .remove(&operation);
                    if parameter_function == function {
                        report
                            .indirect_calls
                            .entry(function.to_string())
                            .or_default()
                            .insert(name.clone());
                        report
                            .symbolic_effect_calls
                            .entry(function.to_string())
                            .or_default()
                            .insert(name.clone());
                    }
                    if parameter_function != function {
                        invoked.insert(CallbackParameter {
                            function: function.to_string(),
                            name: name.clone(),
                        });
                        report
                            .captured_callback_parameters
                            .entry(function.to_string())
                            .or_default()
                            .insert((parameter_function.clone(), name.clone()));
                    }
                    if !report.returned_closure_keys.contains(function) {
                        invoked.insert(CallbackParameter {
                            function: parameter_function,
                            name,
                        });
                    }
                    true
                }
                _ => false,
            };
            let target = if callback_callee {
                None
            } else {
                resolve_local_target(callee, report)
                    .or_else(|| builtin_callback_target(callee, report))
            };
            if let Some(target) = target {
                if let Some(callback) = report
                    .callback_parameters
                    .get(&target)
                    .and_then(|parameters| parameters.iter().next())
                    .filter(|_| target.starts_with("@builtin."))
                {
                    invoked.insert(CallbackParameter {
                        function: target.clone(),
                        name: callback.clone(),
                    });
                }
                let names = report.function_parameters.get(&target);
                let callback_params = report.callback_parameters.get(&target);
                let positional_offset =
                    usize::from(matches!(callee.unlocated(), Expr::FieldAccess(_, _)));
                let mut positional = positional_offset;
                for argument in args {
                    let (index, value) = match argument {
                        Arg::Positional(value) => {
                            let index = positional;
                            positional += 1;
                            (Some(index), value)
                        }
                        Arg::Named(name, value) => (
                            names.and_then(|names| {
                                names.iter().position(|parameter| parameter == name)
                            }),
                            value,
                        ),
                    };
                    let Some(index) = index else { continue };
                    let Some(parameter) = names.and_then(|names| names.get(index)) else {
                        continue;
                    };
                    if !callback_params.is_some_and(|params| params.contains(parameter)) {
                        continue;
                    }
                    let source = callback_source(value, function, shadowed, report);
                    let target_parameter = CallbackParameter {
                        function: target.clone(),
                        name: parameter.clone(),
                    };
                    if let CallbackSource::Parameter {
                        function: source_function,
                        name: source_name,
                    } = &source
                    {
                        forwarded.push((
                            CallbackParameter {
                                function: source_function.clone(),
                                name: source_name.clone(),
                            },
                            target_parameter,
                        ));
                    }
                    callsites.push(CallbackCallsite {
                        caller: function.to_string(),
                        target: target.clone(),
                        target_parameter: parameter.clone(),
                        source,
                        operation: expression_path(callee).unwrap_or_else(|| "<call>".to_string()),
                        file: file.map(str::to_string),
                        line,
                        col,
                    });
                }
            } else if !callback_callee {
                let operation = expression_path(callee).unwrap_or_else(|| "<call>".to_string());
                let local_alias_target = if !is_callback_alias(callee, shadowed) {
                    None
                } else {
                    match &callee_source {
                        CallbackSource::Function(source)
                            if report.function_names.contains(source)
                                && report.captured_callback_parameters.get(source).is_none_or(
                                    |captures| {
                                        captures.iter().all(|(owner, parameter)| {
                                            matches!(
                                                callback_source(
                                                    &Expr::Ident(parameter.clone()),
                                                    function,
                                                    shadowed,
                                                    report,
                                                ),
                                                CallbackSource::Parameter {
                                                    function: visible_owner,
                                                    name: visible_parameter,
                                                } if visible_owner == *owner
                                                    && visible_parameter == *parameter
                                            )
                                        })
                                    },
                                ) =>
                        {
                            Some((source.clone(), None))
                        }
                        CallbackSource::ReturnedClosure {
                            function: target,
                            captures,
                        } if report.function_names.contains(target) => {
                            Some((target.clone(), Some(captures.clone())))
                        }
                        _ => None,
                    }
                };
                if let Some((target, returned_captures)) = local_alias_target {
                    report
                        .resolved_function_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(target.clone());
                    report
                        .resolved_call_operations
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                    report
                        .indirect_calls
                        .entry(function.to_string())
                        .or_default()
                        .remove(&operation);
                    report
                        .unresolved_calls
                        .entry(function.to_string())
                        .or_default()
                        .remove(&operation);
                    if let Some(captures) =
                        report.captured_callback_parameters.get(&target).cloned()
                    {
                        for (owner, parameter) in captures {
                            let source = if let Some(returned_captures) = &returned_captures {
                                let Some(source) = returned_captures.get(&parameter) else {
                                    continue;
                                };
                                (**source).clone()
                            } else {
                                let visible_source = callback_source(
                                    &Expr::Ident(parameter.clone()),
                                    function,
                                    shadowed,
                                    report,
                                );
                                if !matches!(
                                    visible_source,
                                    CallbackSource::Parameter {
                                        function: visible_owner,
                                        name: visible_parameter,
                                    } if visible_owner == *owner
                                        && visible_parameter == *parameter
                                ) {
                                    continue;
                                }
                                if owner != function {
                                    report
                                        .captured_callback_parameters
                                        .entry(function.to_string())
                                        .or_default()
                                        .insert((owner.clone(), parameter.clone()));
                                }
                                CallbackSource::Parameter {
                                    function: function.to_string(),
                                    name: parameter.clone(),
                                }
                            };
                            invoked.insert(CallbackParameter {
                                function: target.clone(),
                                name: parameter.clone(),
                            });
                            callsites.push(CallbackCallsite {
                                caller: function.to_string(),
                                target: target.clone(),
                                target_parameter: parameter.clone(),
                                source,
                                operation: operation.clone(),
                                file: file.map(str::to_string),
                                line,
                                col,
                            });
                        }
                    }
                } else if is_function_value(callee, report) {
                    report
                        .indirect_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(operation.clone());
                }
                if args.iter().any(|argument| match argument {
                    Arg::Positional(value) | Arg::Named(_, value) => {
                        is_function_value(value, report)
                    }
                }) {
                    report
                        .indirect_calls
                        .entry(function.to_string())
                        .or_default()
                        .insert(format!("{operation}.callback"));
                }
            }
            collect_callback_expr(
                callee, function, file, report, shadowed, invoked, forwarded, callsites,
            );
            for argument in args {
                match argument {
                    Arg::Positional(value) | Arg::Named(_, value) => {
                        collect_callback_expr(
                            value, function, file, report, shadowed, invoked, forwarded, callsites,
                        );
                    }
                }
            }
        }
        Expr::Lambda(params, body) => {
            let lambda = format!("{function}::<lambda@{line}:{col}>");
            let mut lambda_shadowed = vec![CallbackScope::default()];
            if let Some(callbacks) = report.callback_parameters.get(function) {
                for name in callbacks {
                    lambda_shadowed[0].names.insert(name.clone());
                    lambda_shadowed[0].aliases.insert(
                        name.clone(),
                        CallbackSource::Parameter {
                            function: function.to_string(),
                            name: name.clone(),
                        },
                    );
                }
            }
            lambda_shadowed.extend(shadowed.iter().cloned());
            let mut lambda_parameters = CallbackScope::default();
            for name in params {
                lambda_parameters.names.insert(name.clone());
                lambda_parameters
                    .aliases
                    .insert(name.clone(), CallbackSource::Unknown);
            }
            lambda_shadowed.push(lambda_parameters);
            collect_callback_block(
                body,
                &lambda,
                file,
                report,
                &mut lambda_shadowed,
                invoked,
                forwarded,
                callsites,
            );
        }
        Expr::Located(inner, _)
        | Expr::Unary(_, inner)
        | Expr::As(inner, _)
        | Expr::UnitLiteral(inner, _) => {
            collect_callback_expr(
                inner, function, file, report, shadowed, invoked, forwarded, callsites,
            );
        }
        Expr::Binary(_, left, right)
        | Expr::Within(left, right)
        | Expr::Approximately(left, right, _) => {
            collect_callback_expr(
                left, function, file, report, shadowed, invoked, forwarded, callsites,
            );
            collect_callback_expr(
                right, function, file, report, shadowed, invoked, forwarded, callsites,
            );
        }
        Expr::FieldAccess(receiver, _) | Expr::Index(receiver, _) => {
            collect_callback_expr(
                receiver, function, file, report, shadowed, invoked, forwarded, callsites,
            );
            if let Expr::Index(_, index) = expr.unlocated() {
                collect_callback_expr(
                    index, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
        }
        Expr::If(condition, yes, no) => {
            collect_callback_expr(
                condition, function, file, report, shadowed, invoked, forwarded, callsites,
            );
            collect_callback_block(
                yes, function, file, report, shadowed, invoked, forwarded, callsites,
            );
            if let Some(no) = no {
                collect_callback_block(
                    no, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
        }
        Expr::Block(body) | Expr::Loop(body) | Expr::Spawn(body) | Expr::SpawnScope(body) => {
            collect_callback_block(
                body, function, file, report, shadowed, invoked, forwarded, callsites,
            );
        }
        Expr::ListLiteral(values) | Expr::SetLiteral(values) => {
            for value in values {
                collect_callback_expr(
                    value, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
        }
        Expr::MapLiteral(values) => {
            for (key, value) in values {
                collect_callback_expr(
                    key, function, file, report, shadowed, invoked, forwarded, callsites,
                );
                collect_callback_expr(
                    value, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
        }
        Expr::Try(value, handler) => {
            collect_callback_expr(
                value, function, file, report, shadowed, invoked, forwarded, callsites,
            );
            if let Some(handler) = handler {
                collect_callback_expr(
                    handler, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
        }
        Expr::RecordLiteral(_, fields) | Expr::GenericRecordLiteral(_, _, fields) => {
            for (_, value) in fields {
                collect_callback_expr(
                    value, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
        }
        Expr::Match(value, arms) => {
            collect_callback_expr(
                value, function, file, report, shadowed, invoked, forwarded, callsites,
            );
            for arm in arms {
                let mut arm_shadowed = BTreeSet::new();
                collect_pattern_names(&arm.pattern, &mut arm_shadowed);
                shadowed.push(CallbackScope {
                    aliases: arm_shadowed
                        .iter()
                        .map(|name| (name.clone(), CallbackSource::Unknown))
                        .collect(),
                    names: arm_shadowed,
                });
                if let Some(guard) = &arm.guard {
                    collect_callback_expr(
                        guard, function, file, report, shadowed, invoked, forwarded, callsites,
                    );
                }
                collect_callback_block(
                    &arm.body, function, file, report, shadowed, invoked, forwarded, callsites,
                );
                shadowed.pop();
            }
        }
        Expr::Range(start, _, end, step) => {
            collect_callback_expr(
                start, function, file, report, shadowed, invoked, forwarded, callsites,
            );
            collect_callback_expr(
                end, function, file, report, shadowed, invoked, forwarded, callsites,
            );
            if let Some(step) = step {
                collect_callback_expr(
                    step, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
        }
        Expr::Channel(_, capacity) => {
            if let Some(capacity) = capacity {
                collect_callback_expr(
                    capacity, function, file, report, shadowed, invoked, forwarded, callsites,
                );
            }
        }
        Expr::IntLiteral(_)
        | Expr::SizedIntLiteral(_, _)
        | Expr::FloatLiteral(_)
        | Expr::Float32Literal(_)
        | Expr::StringLiteral(_)
        | Expr::CharLiteral(_)
        | Expr::BoolLiteral(_)
        | Expr::Ident(_)
        | Expr::EmptyCollection(_, _) => {}
    }
}

fn callback_source(
    expr: &Expr,
    caller: &str,
    shadowed: &[CallbackScope],
    report: &EffectReport,
) -> CallbackSource {
    match expr.unlocated() {
        Expr::Lambda(_, _) => CallbackSource::Function(lambda_function_key(expr, caller)),
        Expr::Call(callee, args) | Expr::GenericCall(callee, _, args) => {
            returned_closure_source(callee, args, caller, shadowed, report)
                .unwrap_or(CallbackSource::Unknown)
        }
        Expr::Ident(name) => {
            if let Some(source) = shadowed
                .iter()
                .rev()
                .find_map(|scope| scope.aliases.get(name))
            {
                source.clone()
            } else if is_shadowed(shadowed, name) {
                CallbackSource::Unknown
            } else if report
                .callback_parameters
                .get(caller)
                .is_some_and(|params| params.contains(name))
            {
                CallbackSource::Parameter {
                    function: caller.to_string(),
                    name: name.clone(),
                }
            } else {
                resolve_local_target(expr, report)
                    .map(CallbackSource::Function)
                    .unwrap_or(CallbackSource::Unknown)
            }
        }
        _ => resolve_local_target(expr, report)
            .map(CallbackSource::Function)
            .unwrap_or(CallbackSource::Unknown),
    }
}

fn returned_closure_source(
    callee: &Expr,
    args: &[Arg],
    caller: &str,
    shadowed: &[CallbackScope],
    report: &EffectReport,
) -> Option<CallbackSource> {
    if !matches!(callee.unlocated(), Expr::Ident(_)) {
        return None;
    }
    let producer = resolve_local_target(callee, report)?;
    if report
        .ambiguous_returned_closure_functions
        .contains(&producer)
    {
        return None;
    }
    let returned = report.returned_closures_by_function.get(&producer)?;
    if returned.len() != 1 {
        return None;
    }
    let closure = returned.iter().next()?;
    let mut captures = BTreeMap::new();
    if let Some(parameters) = report.captured_callback_parameters.get(closure) {
        for (owner, parameter) in parameters {
            if owner != &producer {
                continue;
            }
            let Some(actual) = call_argument_for_parameter(
                args,
                report.function_parameters.get(&producer)?,
                parameter,
            ) else {
                return Some(CallbackSource::Unknown);
            };
            captures.insert(
                parameter.clone(),
                Box::new(callback_source(actual, caller, shadowed, report)),
            );
        }
    }
    Some(CallbackSource::ReturnedClosure {
        function: closure.clone(),
        captures,
    })
}

fn call_argument_for_parameter<'a>(
    args: &'a [Arg],
    parameters: &[String],
    parameter: &str,
) -> Option<&'a Expr> {
    let index = parameters.iter().position(|name| name == parameter)?;
    let mut positional = 0;
    for argument in args {
        match argument {
            Arg::Positional(value) => {
                if positional == index {
                    return Some(value);
                }
                positional += 1;
            }
            Arg::Named(name, value) if name == parameter => return Some(value),
            Arg::Named(_, _) => {}
        }
    }
    None
}

fn is_callback_alias(expr: &Expr, scopes: &[CallbackScope]) -> bool {
    let Expr::Ident(name) = expr.unlocated() else {
        return false;
    };
    scopes
        .iter()
        .rev()
        .any(|scope| scope.aliases.contains_key(name))
}

/// Collection/result combinators are compiler-known higher-order operations.
/// Their callback positions are fixed by the typed receiver and native
/// lowering contract; user-defined methods win because they resolve first.
fn builtin_callback_target(callee: &Expr, report: &mut EffectReport) -> Option<String> {
    let Expr::FieldAccess(receiver, method) = callee.unlocated() else {
        return None;
    };
    let receiver_node = receiver.as_ref() as *const Expr as usize;
    let receiver_unlocated = receiver.unlocated() as *const Expr as usize;
    let list_receiver = report.list_receiver_nodes.contains(&receiver_node)
        || report.list_receiver_nodes.contains(&receiver_unlocated);
    let option_receiver = report.option_receiver_nodes.contains(&receiver_node)
        || report.option_receiver_nodes.contains(&receiver_unlocated);
    let result_receiver = report.result_receiver_nodes.contains(&receiver_node)
        || report.result_receiver_nodes.contains(&receiver_unlocated);
    let (container, parameters) = if list_receiver {
        let parameters = match method.as_str() {
            "map" | "filter" | "any" | "all" | "find" => vec!["self", "callback"],
            "fold" => vec!["self", "initial", "callback"],
            _ => return None,
        };
        ("List", parameters)
    } else if option_receiver && matches!(method.as_str(), "map" | "then") {
        ("Option", vec!["self", "callback"])
    } else if result_receiver && matches!(method.as_str(), "map" | "map_err" | "then") {
        ("Result", vec!["self", "callback"])
    } else {
        return None;
    };
    let target = format!("@builtin.{container}.{method}");
    report
        .function_parameters
        .entry(target.clone())
        .or_insert_with(|| parameters.iter().map(|name| (*name).to_string()).collect());
    report
        .callback_parameters
        .entry(target.clone())
        .or_default()
        .insert("callback".to_string());
    Some(target)
}

fn lambda_function_key(expr: &Expr, parent: &str) -> String {
    let (line, col) = expression_position(expr);
    format!("{parent}::<lambda@{line}:{col}>")
}

fn expression_position(expr: &Expr) -> (usize, usize) {
    if let Expr::Located(_, range) = expr {
        (range.start.line, range.start.col)
    } else {
        (0, 0)
    }
}

fn is_function_value(expr: &Expr, report: &EffectReport) -> bool {
    matches!(expr.unlocated(), Expr::Lambda(_, _))
        || report
            .function_value_nodes
            .contains(&(expr as *const Expr as usize))
        || report
            .function_value_nodes
            .contains(&(expr.unlocated() as *const Expr as usize))
}

fn is_shadowed(shadowed: &[CallbackScope], name: &str) -> bool {
    shadowed
        .iter()
        .rev()
        .any(|scope| scope.names.contains(name))
}

fn collect_pattern_names(pattern: &Pattern, names: &mut BTreeSet<String>) {
    match pattern {
        Pattern::Ident(name) => {
            names.insert(name.clone());
        }
        Pattern::Variant(_, fields) => {
            for (_, field) in fields {
                collect_pattern_names(field, names);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::Range(_, _, _) => {}
    }
}

fn walk_arm(arm: &MatchArm, function: &str, file: Option<&str>, report: &mut EffectReport) {
    walk_pattern(&arm.pattern, function, file, report);
    if let Some(guard) = &arm.guard {
        walk_expr(guard, function, file, report);
    }
    walk_block(&arm.body, function, file, report);
}

fn walk_pattern(pattern: &Pattern, function: &str, file: Option<&str>, report: &mut EffectReport) {
    match pattern {
        Pattern::Wildcard | Pattern::Ident(_) => {}
        Pattern::Literal(value) => walk_expr(value, function, file, report),
        Pattern::Range(start, _, end) => {
            walk_expr(start, function, file, report);
            walk_expr(end, function, file, report);
        }
        Pattern::Variant(_, fields) => {
            for (_, field) in fields {
                walk_pattern(field, function, file, report);
            }
        }
    }
}

fn expression_path(expr: &Expr) -> Option<String> {
    match expr.unlocated() {
        Expr::Ident(name) => Some(name.clone()),
        Expr::FieldAccess(receiver, field) => {
            Some(format!("{}.{}", expression_path(receiver)?, field))
        }
        _ => None,
    }
}

fn resolve_local_target(callee: &Expr, report: &EffectReport) -> Option<String> {
    match callee.unlocated() {
        Expr::Ident(name) if report.free_function_names.contains(name) => Some(name.clone()),
        Expr::FieldAccess(receiver, method) => {
            let owner = report
                .receiver_types
                .get(&(receiver.as_ref() as *const Expr as usize))?;
            report
                .impl_methods_by_type
                .get(owner.strip_prefix('@').unwrap_or(owner))
                .and_then(|methods| methods.get(method))
                .cloned()
        }
        _ => None,
    }
}

fn is_indirect_function_call(callee: &Expr, report: &EffectReport) -> bool {
    let Expr::Ident(name) = callee.unlocated() else {
        return false;
    };
    // A named top-level function is resolved by the direct call graph. An
    // identifier that is not one of those names may be a function-valued
    // parameter or local, which the present effect-free Fn type cannot close.
    !report.free_function_names.contains(name)
        && report
            .function_value_nodes
            .contains(&(callee as *const Expr as usize))
}

fn classify_effect_call(
    callee: &Expr,
    operation: &str,
    report: &EffectReport,
) -> Option<&'static str> {
    let leaf = operation
        .rsplit(|character| character == '.' || character == ':')
        .find(|segment| !segment.is_empty())
        .unwrap_or(operation);
    match callee.unlocated() {
        Expr::Ident(name) => {
            if name.starts_with("std.measurements::") {
                return Some("measurement");
            }
            if name.starts_with("std.viz::") && leaf == "provenance" {
                return Some("provenance");
            }
            // A declaration in the program shadows a same-named builtin. Its
            // effects come from the resolved function body and call graph; do
            // not also assign the host effect associated with the builtin
            // spelling (for example, a local pure `read_file`).
            if report.free_function_names.contains(name) {
                return None;
            }
            if !is_function_value(callee, report)
                && matches!(
                    name.as_str(),
                    "args"
                        | "env"
                        | "cwd"
                        | "file_exists"
                        | "read_file"
                        | "write_file"
                        | "print"
                        | "select"
                        | "yield"
                )
            {
                return classify_effectful_builtin(name);
            }
            if report.free_function_names.contains(name) || is_function_value(callee, report) {
                return None;
            }
            classify_effectful_builtin(name)
        }
        Expr::FieldAccess(receiver, method) => {
            let owner = report
                .receiver_types
                .get(&(receiver.as_ref() as *const Expr as usize))?;
            match owner.as_str() {
                "Rng"
                    if matches!(
                        method.as_str(),
                        "rand" | "randn" | "normal" | "uniform" | "sample" | "choice"
                    ) =>
                {
                    Some("random(seed)")
                }
                "Channel" if matches!(method.as_str(), "send" | "recv" | "close") => {
                    Some("concurrency")
                }
                "Task" if matches!(method.as_str(), "join" | "cancel") => Some("concurrency"),
                _ => None,
            }
        }
        _ => None,
    }
}

fn classify_effectful_builtin(operation: &str) -> Option<&'static str> {
    if matches!(
        operation,
        "rng" | "rand" | "randn" | "uniform" | "normal" | "sample" | "choice"
    ) {
        return Some("random(seed)");
    }
    if matches!(operation, "now" | "clock" | "today" | "sleep") {
        return Some("clock");
    }
    if matches!(operation, "read_file" | "write_file" | "file_exists") {
        return Some("filesystem");
    }
    if matches!(operation, "args" | "env" | "cwd") {
        return Some("external");
    }
    if matches!(operation, "print" | "println") {
        return Some("io");
    }
    if matches!(
        operation,
        "http" | "https" | "fetch" | "request" | "socket" | "tcp" | "udp"
    ) {
        return Some("network");
    }
    if matches!(
        operation,
        "spawn"
            | "spawn_scope"
            | "channel"
            | "select"
            | "send"
            | "recv"
            | "join"
            | "cancel"
            | "yield"
    ) {
        return Some("concurrency");
    }
    None
}

/// Global compiler intrinsics known to have no host effect in the current
/// model. Keep this exact: a qualified `host.sqrt()` is not the `sqrt` builtin.
fn is_known_effect_free_global(operation: &str) -> bool {
    matches!(
        operation,
        "abs"
            | "acos"
            | "arange"
            | "array"
            | "asin"
            | "atan"
            | "atan2"
            | "ceil"
            | "char_from_codepoint"
            | "clone"
            | "cos"
            | "cosh"
            | "corr"
            | "cov"
            | "erf"
            | "exp"
            | "floor"
            | "format_float_value"
            | "full"
            | "hash"
            | "histogram"
            | "det"
            | "eigvals"
            | "eye"
            | "inv"
            | "linfit"
            | "is_finite"
            | "linspace"
            | "ln"
            | "log10"
            | "norm"
            | "norm_cdf"
            | "norm_pdf"
            | "ones"
            | "pi"
            | "polyfit"
            | "polyval"
            | "pow"
            | "round"
            | "solve"
            | "sin"
            | "sinh"
            | "sqrt"
            | "tan"
            | "tanh"
            | "temperature_point"
            | "temperature_point_to"
            | "trace"
            | "where"
            | "zeros"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
    )
}

/// Methods are known only when the type checker attached a built-in receiver
/// type to this exact receiver node. User-defined or host receivers keep their
/// methods open even when they share a familiar leaf name.
fn is_known_effect_free_method(owner: &str, method: &str) -> bool {
    if method == "to_string" {
        // The checker reserves this conversion as a compiler operation for
        // every concrete value type; dyn Trait receivers are not indexed here.
        return true;
    }
    match owner {
        "@Array" => matches!(
            method,
            "shape"
                | "rank"
                | "size"
                | "length"
                | "count"
                | "sum"
                | "min"
                | "max"
                | "mean"
                | "to_list"
                | "transpose"
                | "var"
                | "std"
                | "sample_var"
                | "sample_std"
                | "median"
                | "cumsum"
                | "sort"
                | "to_float"
                | "any"
                | "all"
                | "count_true"
                | "reshape"
                | "sum_axis"
                | "dot"
                | "matmul"
                | "percentile"
                | "row"
                | "col"
                | "get"
                | "unit"
                | "values"
        ),
        "@List" => matches!(
            method,
            "length" | "count" | "join" | "map" | "filter" | "find" | "any" | "all" | "fold"
        ),
        "@Map" => matches!(method, "count" | "keys" | "contains_key" | "values" | "get"),
        "@Set" => matches!(method, "count" | "contains"),
        "@String" => matches!(
            method,
            "length"
                | "is_empty"
                | "char_at"
                | "slice"
                | "codepoint"
                | "trim"
                | "to_upper"
                | "to_lower"
                | "contains"
                | "starts_with"
                | "ends_with"
                | "replace"
                | "split"
                | "lines"
                | "to_int"
                | "to_float"
        ),
        "@Option" => matches!(
            method,
            "is_some" | "is_none" | "unwrap" | "unwrap_or" | "ok_or" | "map" | "and_then"
        ),
        "@Result" => matches!(
            method,
            "is_ok" | "is_err" | "unwrap" | "unwrap_or" | "map" | "map_err" | "and_then"
        ),
        "@Quantity" => matches!(
            method,
            "value" | "unit" | "unit_scale" | "dimension_key" | "is_exact" | "is_known"
        ),
        _ => false,
    }
}

fn is_known_effect_free_call(callee: &Expr, report: &EffectReport) -> bool {
    match callee.unlocated() {
        Expr::Ident(name) => {
            !report.free_function_names.contains(name)
                && !is_function_value(callee, report)
                && is_known_effect_free_global(name)
        }
        Expr::FieldAccess(receiver, method) => report
            .receiver_types
            .get(&(receiver.as_ref() as *const Expr as usize))
            .is_some_and(|owner| is_known_effect_free_method(owner, method)),
        _ => false,
    }
}

fn classify_drop_call(callee: &Expr, args: &[Arg], report: &EffectReport) -> Option<DropEffect> {
    let Expr::Ident(name) = callee.unlocated() else {
        return None;
    };
    if name != "drop"
        || report.free_function_names.contains(name)
        || is_function_value(callee, report)
        || args.len() != 1
    {
        return None;
    }
    let value = match &args[0] {
        Arg::Positional(value) | Arg::Named(_, value) => value,
    };
    let ty = expression_type(value, report)?;
    match drop_effect_for_type(ty, report, &mut BTreeSet::new()) {
        DropEffect::Unknown => None,
        effect => Some(effect),
    }
}

fn expression_type<'a>(expr: &'a Expr, report: &'a EffectReport) -> Option<&'a Ty> {
    report
        .expression_types
        .get(&(expr as *const Expr as usize))
        .or_else(|| match expr {
            Expr::Located(inner, _) => expression_type(inner, report),
            _ => None,
        })
}

fn is_known_local_mutation_call(callee: &Expr, report: &EffectReport) -> bool {
    let Expr::FieldAccess(receiver, method) = callee.unlocated() else {
        return false;
    };
    let Some(owner) = report
        .receiver_types
        .get(&(receiver.as_ref() as *const Expr as usize))
    else {
        return false;
    };
    match owner.as_str() {
        "@Array" => method == "set",
        "@List" => matches!(method.as_str(), "push" | "remove_at"),
        "@Map" => matches!(method.as_str(), "set" | "remove"),
        "@Set" => matches!(method.as_str(), "add" | "remove"),
        _ => false,
    }
}

fn record(
    report: &mut EffectReport,
    effect: &str,
    operation: &str,
    function: &str,
    file: Option<&str>,
    line: usize,
    col: usize,
) {
    add_effect(report, function, effect);
    report.sites.push(EffectSite {
        effect: effect.to_string(),
        operation: operation.to_string(),
        function: function.to_string(),
        file: file.map(str::to_string),
        line,
        col,
    });
}

fn add_effect(report: &mut EffectReport, function: &str, effect: &str) {
    let kind = EffectKind::parse(effect);
    report.effects.insert(effect.to_string());
    report.kinds.insert(kind.clone());
    report
        .function_kinds
        .entry(function.to_string())
        .or_default()
        .insert(kind);
}

fn propagate_function_effects(report: &mut EffectReport) {
    // Snapshot direct rows first so the fixed-point pass can distinguish source
    // evidence from inherited effects and remains deterministic for recursive SCCs.
    let mut rows = report.function_kinds.clone();
    loop {
        let mut next = rows.clone();
        for (caller, callees) in &report.resolved_function_calls {
            let row = next.entry(caller.clone()).or_default();
            for callee in callees {
                if let Some(inherited) = rows.get(callee) {
                    row.extend(inherited.iter().cloned());
                }
            }
        }
        if next == rows {
            break;
        }
        rows = next;
    }
    for row in rows.values() {
        for effect in row {
            report.kinds.insert(effect.clone());
            report.effects.insert(effect.label().to_string());
        }
    }
    report.function_kinds = rows;

    let mut open_calls = report.unresolved_calls.clone();
    for (caller, calls) in &report.indirect_calls {
        open_calls
            .entry(caller.clone())
            .or_default()
            .extend(calls.iter().cloned());
    }
    loop {
        let mut next = open_calls.clone();
        for (caller, callees) in &report.resolved_function_calls {
            let inherited = callees
                .iter()
                .filter_map(|callee| {
                    open_calls.get(callee).map(|calls| {
                        let closed = report
                            .closed_callback_parameters
                            .get(caller)
                            .and_then(|targets| targets.get(callee));
                        calls
                            .iter()
                            .filter(|call| !closed.is_some_and(|closed| closed.contains(*call)))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                })
                .flatten();
            next.entry(caller.clone()).or_default().extend(inherited);
        }
        if next == open_calls {
            break;
        }
        open_calls = next;
    }
    report.open_effect_calls = open_calls;
}

pub fn effect_description(effect: &str) -> &'static str {
    match effect {
        "clock" => "observa reloj o tiempo",
        "concurrency" => "usa tareas, canales o scheduler",
        "external" => "observa argumentos o variables del entorno del proceso",
        "filesystem" => "lee o modifica el filesystem del host",
        "io" => "observa o modifica E/S del host",
        "measurement" => "propaga mediciones o incertidumbre",
        "network" => "contacta un recurso de red",
        "provenance" => "registra metadatos de procedencia",
        "random(seed)" => "consume azar con un flujo explícito o detectable",
        _ => "efecto desconocido",
    }
}

#[cfg(test)]
mod tests {
    use super::{is_known_effect_free_global, is_known_effect_free_method};
    use crate::ast::Expr;

    #[test]
    fn known_intrinsics_are_exact_and_do_not_match_qualified_host_calls() {
        for operation in [
            "sin",
            "array",
            "sqrt",
            "zeros",
            "hash",
            "histogram",
            "clone",
        ] {
            assert!(is_known_effect_free_global(operation), "{operation}");
        }
        for operation in [
            "host.sqrt",
            "std.strings::trim",
            "unknown_host_operation",
            "normal",
        ] {
            assert!(!is_known_effect_free_global(operation), "{operation}");
        }
    }

    #[test]
    fn known_methods_require_a_builtin_receiver_type_and_exclude_mutators() {
        assert!(is_known_effect_free_method("@Array", "mean"));
        assert!(is_known_effect_free_method("@String", "trim"));
        assert!(is_known_effect_free_method("@Map", "get"));
        assert!(is_known_effect_free_method("UserRecord", "to_string"));
        assert!(!is_known_effect_free_method("Host", "get"));
        assert!(!is_known_effect_free_method("@Array", "set"));
        assert!(!is_known_effect_free_method("@List", "push"));
    }

    #[test]
    fn indirect_function_invocation_requires_typed_function_value_evidence() {
        let mut report = super::EffectReport::default();
        let callee = Expr::Ident("action".to_string());
        let key = &callee as *const Expr as usize;
        report.function_value_nodes.insert(key);
        assert!(super::is_indirect_function_call(&callee, &report));
        report.function_value_nodes.clear();
        assert!(!super::is_indirect_function_call(&callee, &report));
    }
}
