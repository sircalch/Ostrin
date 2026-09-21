use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::types::*;

#[derive(Debug, Clone)]
pub struct TypeError {
    pub code: &'static str,
    pub message: String,
    pub span: Option<Span>,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EditorBinding {
    pub name: String,
    pub type_name: String,
    pub function: String,
    pub scope_depth: usize,
    pub span: Span,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EditorExpression {
    pub type_name: String,
    pub function: String,
    pub span: Span,
    pub end: Span,
    pub source_file: Option<String>,
}

/// Identifies an expression node across the whole program: the file it was
/// parsed from plus its exact source range. (The parser wraps each expression
/// exactly once in `Expr::Located`, so this is unique per node.)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExprKey {
    pub file: Option<String>,
    pub start: Span,
    pub end: Span,
}

/// The type/dimension arguments the checker resolved for one call of a generic
/// function (`identity(5)` ↦ `T = Int`).
#[derive(Debug, Clone, Default)]
pub struct CallSubst {
    pub types: HashMap<String, Ty>,
    pub dims: HashMap<String, Dimension>,
}

/// Everything the checker learned, in structured form: the foundation for a
/// typed AST/HIR that backends can consume instead of re-inferring types.
pub struct TypedProgram {
    pub errors: Vec<TypeError>,
    /// The type of every located expression. Generic bodies are checked once,
    /// so their expressions carry `Ty::Generic` types; a type the checker
    /// could not determine is `Ty::Unknown`.
    pub expr_types: HashMap<ExprKey, Ty>,
    /// The resolved generic arguments of every call to a generic function.
    pub call_substs: HashMap<ExprKey, CallSubst>,
    /// Untyped integer literals (or negated literals) that the context typed as a
    /// fixed-width integer: backends read the literal's real type from here.
    pub literal_kinds: HashMap<ExprKey, LitKind>,
    /// The type of *every* expression node, keyed by the node's address in the
    /// (immutable) AST the checker was given — including operands, which have no
    /// source range of their own. Lowering to HIR reads types from here.
    pub node_types: HashMap<usize, Ty>,
    /// Resolved generic arguments of a call, keyed by the call node's address (works for
    /// calls with no source range of their own, e.g. a receiver `wrap(5).is_just()`).
    pub call_substs_by_node: HashMap<usize, CallSubst>,
}

#[derive(Clone)]
struct FnSig {
    params: Vec<Param>,
    return_type: Type,
    generics: Vec<GenericParam>,
}

#[derive(Clone)]
struct ConcreteMethod {
    generics: Vec<GenericParam>,
    params: Vec<Param>,
    return_type: Type,
    owner: String,
    impl_substitutions: HashMap<String, Ty>,
}

pub struct Checker {
    functions: HashMap<String, FnSig>,
    traits: HashMap<String, TraitDecl>,
    implementations: Vec<ImplDecl>,
    trait_impls: HashSet<(String, String)>,
    type_origins: HashMap<String, Vec<String>>,
    enum_variants: HashMap<String, Vec<String>>,
    variant_owners: HashMap<String, String>,
    variant_fields: HashMap<(String, String), Vec<Type>>,
    variant_field_names: HashMap<(String, String), Vec<Option<String>>>,
    enum_generics: HashMap<String, Vec<GenericParam>>,
    record_fields: HashMap<String, Vec<(String, Type)>>,
    record_field_mutability: HashMap<String, HashMap<String, bool>>,
    record_derives: HashMap<String, Vec<String>>,
    enum_derives: HashMap<String, Vec<String>>,
    record_generics: HashMap<String, Vec<GenericParam>>,
    current_generic_bounds: HashMap<String, Vec<String>>,
    current_return_type: Option<Ty>,
    current_span: Option<Span>,
    current_source_file: Option<String>,
    current_function_name: Option<String>,
    editor_scope_depth: usize,
    editor_bindings: Vec<EditorBinding>,
    editor_expressions: Vec<EditorExpression>,
    expr_types: HashMap<ExprKey, Ty>,
    call_substs: HashMap<ExprKey, CallSubst>,
    literal_kinds: HashMap<ExprKey, LitKind>,
    node_types: HashMap<usize, Ty>,
    call_substs_by_node: HashMap<usize, CallSubst>,
    call_node_stack: Vec<usize>,
    call_key_stack: Vec<ExprKey>,
    collection_bound_diagnostics: HashSet<String>,
    errors: Vec<TypeError>,
}

type Scope = HashMap<String, (Ty, bool)>;

const MAX_PATTERN_SHAPES: usize = 1024;

impl Checker {
    pub fn new() -> Self {
        let mut enum_variants = HashMap::new();
        let mut variant_owners = HashMap::new();
        let mut variant_fields = HashMap::new();
        let mut variant_field_names = HashMap::new();
        let mut enum_generics = HashMap::new();
        for (enum_name, variants) in [
            ("Option", vec![("Some", vec![Type::Named("T".to_string(), vec![])]), ("None", vec![])]),
            ("Result", vec![("Ok", vec![Type::Named("T".to_string(), vec![])]), ("Err", vec![Type::Named("E".to_string(), vec![])]),]),
            ("Ordering", vec![("Less", vec![]), ("Equal", vec![]), ("Greater", vec![])]),
        ] {
            enum_variants.insert(enum_name.to_string(), variants.iter().map(|(name, _)| (*name).to_string()).collect());
            enum_generics.insert(
                enum_name.to_string(),
                match enum_name {
                    "Option" => vec![GenericParam { name: "T".to_string(), bounds: vec![] }],
                    "Result" => vec![
                        GenericParam { name: "T".to_string(), bounds: vec![] },
                        GenericParam { name: "E".to_string(), bounds: vec![] },
                    ],
                    _ => Vec::new(),
                },
            );
            for (variant_name, fields) in variants {
                variant_owners.insert(variant_name.to_string(), enum_name.to_string());
                variant_fields.insert((enum_name.to_string(), variant_name.to_string()), fields);
                variant_field_names.insert(
                    (enum_name.to_string(), variant_name.to_string()),
                    vec![None; variant_fields[&(enum_name.to_string(), variant_name.to_string())].len()],
                );
            }
        }
        Checker {
            functions: HashMap::new(),
            traits: HashMap::new(),
            implementations: Vec::new(),
            trait_impls: HashSet::new(),
            type_origins: HashMap::new(),
            enum_variants,
            variant_owners,
            variant_fields,
            variant_field_names,
            enum_generics,
            record_fields: HashMap::new(),
            record_field_mutability: HashMap::new(),
            record_derives: HashMap::new(),
            enum_derives: HashMap::new(),
            record_generics: HashMap::new(),
            current_generic_bounds: HashMap::new(),
            current_return_type: None,
            current_span: None,
            current_source_file: None,
            current_function_name: None,
            editor_scope_depth: 0,
            editor_bindings: Vec::new(),
            editor_expressions: Vec::new(),
            expr_types: HashMap::new(),
            call_substs: HashMap::new(),
            literal_kinds: HashMap::new(),
            node_types: HashMap::new(),
            call_substs_by_node: HashMap::new(),
            call_node_stack: Vec::new(),
            call_key_stack: Vec::new(),
            collection_bound_diagnostics: HashSet::new(),
            errors: Vec::new(),
        }
    }

    pub fn check_program(self, items: &[Item]) -> Vec<TypeError> {
        self.check_program_with_bindings(items).0
    }

    pub fn check_program_with_bindings(self, items: &[Item]) -> (Vec<TypeError>, Vec<EditorBinding>) {
        let (errors, bindings, _) = self.check_program_with_editor_data(items);
        (errors, bindings)
    }

    pub fn check_program_with_editor_data(
        self,
        items: &[Item],
    ) -> (Vec<TypeError>, Vec<EditorBinding>, Vec<EditorExpression>) {
        let (errors, bindings, expressions, _, _, _, _, _) = self.check_all(items);
        (errors, bindings, expressions)
    }

    /// Like `check_program`, but also returns the type of every expression.
    pub fn check_program_typed(self, items: &[Item]) -> TypedProgram {
        let (errors, _, _, expr_types, call_substs, literal_kinds, node_types, call_substs_by_node) = self.check_all(items);
        TypedProgram { errors, expr_types, call_substs, literal_kinds, node_types, call_substs_by_node }
    }

    fn check_all(
        mut self,
        items: &[Item],
    ) -> (Vec<TypeError>, Vec<EditorBinding>, Vec<EditorExpression>, HashMap<ExprKey, Ty>, HashMap<ExprKey, CallSubst>, HashMap<ExprKey, LitKind>, HashMap<usize, Ty>, HashMap<usize, CallSubst>) {
        for item in items {
            match item {
                Item::Enum(e) => {
                    self.type_origins.insert(e.name.clone(), e.module_path.clone());
                    self.enum_generics.insert(e.name.clone(), e.generics.clone());
                    self.enum_derives.insert(e.name.clone(), e.derives.clone());
                    self.enum_variants.insert(
                        e.name.clone(),
                        e.variants.iter().map(|v| v.name.clone()).collect(),
                    );
                    for variant in &e.variants {
                        self.variant_owners.insert(variant.name.clone(), e.name.clone());
                        self.variant_fields.insert(
                            (e.name.clone(), variant.name.clone()),
                            variant.fields.iter().map(|field| field.ty.clone()).collect(),
                        );
                        self.variant_field_names.insert(
                            (e.name.clone(), variant.name.clone()),
                            variant.fields.iter().map(|field| field.name.clone()).collect(),
                        );
                    }
                }
                Item::Record(record) => {
                    self.type_origins.insert(record.name.clone(), record.module_path.clone());
                    self.record_generics.insert(record.name.clone(), record.generics.clone());
                    self.record_derives.insert(record.name.clone(), record.derives.clone());
                    self.record_fields.insert(
                        record.name.clone(),
                        record
                            .fields
                            .iter()
                            .map(|field| (field.name.clone(), field.ty.clone()))
                            .collect(),
                    );
                    self.record_field_mutability.insert(
                        record.name.clone(),
                        record
                            .fields
                            .iter()
                            .map(|field| (field.name.clone(), field.is_mut))
                            .collect(),
                    );
                }
                Item::Trait(t) => {
                    self.traits.insert(t.name.clone(), t.clone());
                }
                Item::Impl(im) => {
                    self.implementations.push(im.clone());
                    if let Some(trait_name) = &im.trait_name {
                        if !self.trait_impls.insert((trait_name.clone(), im.type_name.clone())) {
                            self.push(
                                "E1054",
                                format!("Duplicate implementation of '{}' for '{}'.", trait_name, im.type_name),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        for item in items {
            match item {
                Item::Record(record) => {
                    for field in &record.fields {
                        self.validate_declared_collection_type(&field.ty, &record.generics);
                    }
                }
                Item::Enum(enumeration) => {
                    for variant in &enumeration.variants {
                        for field in &variant.fields {
                            self.validate_declared_collection_type(&field.ty, &enumeration.generics);
                        }
                    }
                }
                _ => {}
            }
        }
        for item in items {
            if let Item::Trait(trait_decl) = item {
                self.validate_trait_decl(trait_decl);
            }
        }
        for item in items {
            if let Item::Trait(trait_decl) = item {
                self.check_trait_defaults(trait_decl);
            }
        }
        for item in items {
            if let Item::Impl(im) = item {
                self.validate_impl(im);
            }
        }
        for item in items {
            if let Item::Function(f) = item {
                if self.functions.contains_key(&f.name) {
                    self.push("E1040", format!("Function '{}' is already defined.", f.name));
                    continue;
                }
                self.functions.insert(
                    f.name.clone(),
                    FnSig {
                        params: f.params.clone(),
                        return_type: f.return_type.clone(),
                        generics: f.generics.clone(),
                    },
                );
            }
        }
        for item in items {
            match item {
                Item::Function(f) => self.check_function(f),
                Item::Impl(im) => {
                    for method in &im.methods {
                        self.check_impl_method(method, im);
                    }
                }
                // Records e imports todavía no se verifican semánticamente de forma completa.
                Item::Record(_) | Item::Enum(_) | Item::Import(_) | Item::Trait(_) => {}
            }
        }
        (self.errors, self.editor_bindings, self.editor_expressions, self.expr_types, self.call_substs, self.literal_kinds, self.node_types, self.call_substs_by_node)
    }

    fn check_function(&mut self, f: &FunctionDecl) {
        self.check_function_with_extra_bounds(f, &HashMap::new());
    }

    fn check_function_with_extra_bounds(
        &mut self,
        f: &FunctionDecl,
        extra_bounds: &HashMap<String, Vec<String>>,
    ) {
        self.check_function_with_body(f, extra_bounds, &f.body);
    }

    /// Checks `f` (whose parameters/generics may have been rewritten, e.g. `Self` replaced)
    /// against `body`, which must be the *original* AST block so the node-type table
    /// keys (node addresses) match the tree that later stages read.
    fn check_function_with_body(
        &mut self,
        f: &FunctionDecl,
        extra_bounds: &HashMap<String, Vec<String>>,
        body: &Block,
    ) {
        let previous_span = self.current_span;
        self.current_span = Some(f.span);
        let previous_source_file = self.current_source_file.clone();
        self.current_source_file = f.source_file.clone();
        let previous_function_name = self.current_function_name.clone();
        self.current_function_name = Some(f.name.clone());
        let previous_scope_depth = self.editor_scope_depth;
        self.editor_scope_depth = 0;
        let previous_bounds = std::mem::take(&mut self.current_generic_bounds);
        self.current_generic_bounds = f
            .generics
            .iter()
            .map(|g| (g.name.clone(), g.bounds.clone()))
            .collect();
        self.current_generic_bounds.extend(extra_bounds.clone());
        let mut scope: Scope = HashMap::new();
        for p in &f.params {
            let parameter_type = self.resolve_type_in_context(&p.ty);
            self.validate_collection_bounds(&parameter_type);
            self.editor_bindings.push(EditorBinding {
                name: p.name.clone(),
                type_name: parameter_type.describe(),
                function: f.name.clone(),
                scope_depth: self.editor_scope_depth,
                span: f.span,
                source_file: f.source_file.clone(),
            });
            scope.insert(p.name.clone(), (parameter_type, p.is_mut));
        }
        let expected = self.resolve_type_in_context(&f.return_type);
        self.validate_collection_bounds(&expected);
        for param in &f.params {
            let Some(default) = &param.default else { continue };
            let actual = self.infer_expr(default, &mut scope);
            let declared = self.resolve_type_in_context(&param.ty);
            if !matches!(declared, Ty::Generic(_)) && !compatible(&declared, &actual) {
                self.push(
                    "E1041",
                    format!(
                        "Default value for '{}' expects '{}', got '{}'.",
                        param.name,
                        declared.describe(),
                        actual.describe()
                    ),
                );
            }
        }
        let previous_return_type = self.current_return_type.replace(expected.clone());
        let actual = self.check_block_expecting(body, &mut scope, Some(&expected));
        self.note_expected_block(body, &expected);
        let tail_adapted = match &body.tail {
            Some(tail) => self.adapt_literals(tail, &expected, &actual),
            None => false,
        };
        if !tail_adapted && !block_always_returns(body) && !compatible(&expected, &actual) {
            self.push(
                "E1041",
                format!(
                    "Function '{}' declared to return '{}' but its body evaluates to '{}'.",
                    f.name,
                    expected.describe(),
                    actual.describe()
                ),
            );
        }
        self.current_return_type = previous_return_type;
        self.current_generic_bounds = previous_bounds;
        self.current_span = previous_span;
        self.current_source_file = previous_source_file;
        self.current_function_name = previous_function_name;
        self.editor_scope_depth = previous_scope_depth;
    }

    fn check_trait_defaults(&mut self, trait_decl: &TraitDecl) {
        let mut self_bounds = vec![trait_decl.name.clone()];
        let mut visiting = Vec::new();
        let mut seen = HashSet::new();
        collect_supertraits(
            &trait_decl.name,
            &self.traits,
            &mut visiting,
            &mut seen,
            &mut self_bounds,
        );
        let extra_bounds = HashMap::from([("Self".to_string(), self_bounds)]);

        for method in &trait_decl.methods {
            let Some(body) = &method.default_body else { continue };
            let mut generics = trait_decl.generics.clone();
            generics.extend(method.generics.clone());
            let function = FunctionDecl {
                name: format!("{}.{}", trait_decl.name, method.name),
                is_pub: false,
                generics,
                params: method.params.clone(),
                return_type: method.return_type.clone(),
                body: body.clone(),
                span: Span::default(),
                source_file: None,
            };
            self.check_function_with_body(&function, &extra_bounds, body);
        }
    }

    fn check_impl_method(&mut self, original: &FunctionDecl, implementation: &ImplDecl) {
        let mut method = original.clone();
        if !implementation.generics.is_empty() {
            let mut generics = implementation.generics.clone();
            generics.extend(method.generics);
            method.generics = generics;
        }
        let owner = Type::Named(implementation.type_name.clone(), implementation.type_args.clone());
        for param in &mut method.params {
            replace_self_type_with_type(&mut param.ty, &owner);
        }
        replace_self_type_with_type(&mut method.return_type, &owner);
        self.check_function_with_body(&method, &HashMap::new(), &original.body);
    }

    fn validate_impl(&mut self, implementation: &ImplDecl) {
        self.validate_generic_bounds(&implementation.generics, "impl");
        let Some(trait_name) = &implementation.trait_name else { return };
        let trait_decl = self.traits.get(trait_name).cloned();
        if trait_decl.is_none() && !is_builtin_trait(trait_name) {
            self.push(
                "E1054",
                format!("Trait '{}' is not declared and is not a standard trait.", trait_name),
            );
            return;
        }
        if !self.impl_respects_coherence(implementation, trait_decl.as_ref()) {
            self.push(
                "E1056",
                format!(
                    "Implementation of '{}' for '{}' violates the orphan rule: either the trait or the type must be defined in this module.",
                    trait_name, implementation.type_name
                ),
            );
            return;
        }
        self.validate_impl_type_arguments(implementation, trait_decl.as_ref());
        let Some(trait_decl) = trait_decl else {
            return;
        };
        let trait_methods = trait_decl
            .methods
            .iter()
            .map(|method| specialize_trait_method(method, &trait_decl.generics, &implementation.trait_args))
            .collect::<Vec<_>>();

        let mut supertraits = Vec::new();
        let mut visiting = Vec::new();
        let mut seen = HashSet::new();
        collect_supertraits(trait_name, &self.traits, &mut visiting, &mut seen, &mut supertraits);
        for supertrait in &supertraits {
            if !self
                .trait_impls
                .contains(&(supertrait.clone(), implementation.type_name.clone()))
            {
                self.push(
                    "E1050",
                    format!(
                        "Cannot implement '{}' for '{}': missing required supertrait '{}'.",
                        trait_name, implementation.type_name, supertrait
                    ),
                );
            }
        }

        for required in &trait_methods {
            if required.default_body.is_none() && !implementation.methods.iter().any(|method| method.name == required.name) {
                self.push(
                    "E1055",
                    format!(
                        "Implementation of '{}' for '{}' is missing required method '{}'.",
                        trait_name, implementation.type_name, required.name
                    ),
                );
            }
        }

        for method in &implementation.methods {
            if let Some(expected) = trait_methods.iter().find(|expected| expected.name == method.name) {
                if !impl_method_signature_matches(expected, method, &implementation.type_name) {
                    self.push(
                        "E1054",
                        format!(
                            "Method '{}.{}' does not match the signature declared by trait '{}'.",
                            implementation.type_name, method.name, trait_name
                        ),
                    );
                }
            }
        }
    }

    fn validate_generic_bounds(&mut self, generics: &[GenericParam], owner: &str) {
        for generic in generics {
            for bound in &generic.bounds {
                if bound != "Dimension" && !self.traits.contains_key(bound) && !is_builtin_trait(bound) {
                    self.push(
                        "E1054",
                        format!(
                            "Unknown trait bound '{}' on generic parameter '{}' in {}.",
                            bound, generic.name, owner
                        ),
                    );
                }
            }
        }
    }

    fn validate_trait_decl(&mut self, trait_decl: &TraitDecl) {
        let mut seen_supertraits = HashSet::new();
        for supertrait in &trait_decl.supertraits {
            if !seen_supertraits.insert(supertrait.clone()) {
                self.push(
                    "E1057",
                    format!(
                        "Trait '{}' lists supertrait '{}' more than once.",
                        trait_decl.name, supertrait
                    ),
                );
            }
            if !self.traits.contains_key(supertrait) && !is_builtin_trait(supertrait) {
                self.push(
                    "E1054",
                    format!(
                        "Trait '{}' extends unknown trait '{}'.",
                        trait_decl.name, supertrait
                    ),
                );
            }
        }

        let mut closure = Vec::new();
        let mut visiting = Vec::new();
        let mut seen = HashSet::new();
        let has_cycle = collect_trait_closure(
            &trait_decl.name,
            &self.traits,
            &mut visiting,
            &mut seen,
            &mut closure,
        );
        if has_cycle {
            self.push(
                "E1057",
                format!("Trait inheritance cycle detected involving '{}'.", trait_decl.name),
            );
        }

        let mut methods: HashMap<String, (TraitMethodSig, String)> = HashMap::new();
        for source_name in closure {
            let Some(source) = self.traits.get(&source_name).cloned() else { continue };
            for method in source.methods {
                if let Some((previous, previous_source)) = methods.get(&method.name) {
                    if previous_source == &source_name {
                        self.push(
                            "E1057",
                            format!(
                                "Trait '{}' declares method '{}' more than once.",
                                source_name, method.name
                            ),
                        );
                    } else if !trait_method_signature_matches(previous, &method) {
                        self.push(
                            "E1057",
                            format!(
                                "Trait '{}' inherits incompatible declarations of method '{}' from '{}' and '{}'.",
                                trait_decl.name, method.name, previous_source, source_name
                            ),
                        );
                    }
                } else {
                    methods.insert(method.name.clone(), (method, source_name.clone()));
                }
            }
        }
    }

    fn impl_respects_coherence(&self, implementation: &ImplDecl, trait_decl: Option<&TraitDecl>) -> bool {
        let trait_is_local = trait_decl
            .is_some_and(|decl| decl.module_path == implementation.module_path);
        let type_is_local = self
            .type_origins
            .get(&implementation.type_name)
            .is_some_and(|origin| origin == &implementation.module_path);
        trait_is_local || type_is_local
    }

    fn validate_impl_type_arguments(
        &mut self,
        implementation: &ImplDecl,
        trait_decl: Option<&TraitDecl>,
    ) {
        if let Some(trait_decl) = trait_decl {
            if implementation.trait_args.len() != trait_decl.generics.len() {
                self.push(
                    "E1042",
                    format!(
                        "Trait '{}' expects {} type argument(s) in this impl, got {}.",
                        implementation.trait_name.as_deref().unwrap_or("<unknown>"),
                        trait_decl.generics.len(),
                        implementation.trait_args.len()
                    ),
                );
            }
        }

        let expected_type_arity = self
            .record_generics
            .get(&implementation.type_name)
            .map(Vec::len)
            .or_else(|| self.enum_generics.get(&implementation.type_name).map(Vec::len))
            .or_else(|| builtin_generic_type_arity(&implementation.type_name));
        match expected_type_arity {
            Some(arity) if implementation.type_args.len() != arity => {
                self.push(
                    "E1042",
                    format!(
                        "Type '{}' expects {} type argument(s) in this impl, got {}.",
                        implementation.type_name,
                        arity,
                        implementation.type_args.len()
                    ),
                );
            }
            None if !implementation.type_args.is_empty() => {
                self.push(
                    "E1042",
                    format!(
                        "Type '{}' is not a declared generic type and cannot receive impl type arguments.",
                        implementation.type_name
                    ),
                );
            }
            _ => {}
        }
    }

    fn check_block(&mut self, block: &Block, scope: &mut Scope) -> Ty {
        self.check_block_expecting(block, scope, None)
    }

    /// Like `check_block`, but the tail expression is checked against the type the block is expected to
    /// have (so a lambda in tail position learns its parameter types).
    fn check_block_expecting(&mut self, block: &Block, scope: &mut Scope, expected: Option<&Ty>) -> Ty {
        let previous_scope_depth = self.editor_scope_depth;
        self.editor_scope_depth += 1;
        for stmt in &block.stmts {
            let previous_span = self.current_span;
            self.current_span = Some(stmt.span);
            self.check_stmt(&stmt.stmt, scope);
            self.current_span = previous_span;
        }
        let result = match &block.tail {
            Some(e) => self.infer_expr_with_expected(e, expected, scope),
            None => Ty::Void,
        };
        self.editor_scope_depth = previous_scope_depth;
        result
    }

    fn check_stmt(&mut self, stmt: &Stmt, scope: &mut Scope) {
        match stmt {
            Stmt::Binding { mut_, name, ty, value } => {
                let declared_early = ty.as_ref().map(|t| self.resolve_type_in_context(t));
                let value_ty = self.infer_expr_with_expected(value, declared_early.as_ref().filter(|d| matches!(d, Ty::Fn(..))), scope);
                let final_ty = match ty {
                    Some(t) => {
                        let declared = self.resolve_type_in_context(t);
                        self.note_expected(value, &declared);
                        if !compatible(&declared, &value_ty) && !self.adapt_literals(value, &declared, &value_ty) {
                            self.push(
                                "E1041",
                                format!(
                                    "Cannot assign a value of type '{}' to '{}: {}'.",
                                    value_ty.describe(),
                                    name,
                                    declared.describe()
                                ),
                            );
                        }
                        declared
                    }
                    None => value_ty,
                };
                self.editor_bindings.push(EditorBinding {
                    name: name.clone(),
                    type_name: final_ty.describe(),
                    function: self.current_function_name.clone().unwrap_or_default(),
                    scope_depth: self.editor_scope_depth,
                    span: self.current_span.unwrap_or_default(),
                    source_file: self.current_source_file.clone(),
                });
                scope.insert(name.clone(), (final_ty, *mut_));
            }
            Stmt::Assign { name, value } => {
                let value_ty = self.infer_expr(value, scope);
                match scope.get(name) {
                    Some((_, false)) => {
                        self.push(
                            "E1001",
                            format!(
                                "Cannot reassign immutable binding '{name}'. Declare it as 'mut {name} = ...' if reassignment is intended."
                            ),
                        );
                    }
                    Some((_, true)) => {
                        scope.insert(name.clone(), (value_ty, true));
                    }
                    None => {
                        self.editor_bindings.push(EditorBinding {
                            name: name.clone(),
                            type_name: value_ty.describe(),
                            function: self.current_function_name.clone().unwrap_or_default(),
                            scope_depth: self.editor_scope_depth,
                            span: self.current_span.unwrap_or_default(),
                            source_file: self.current_source_file.clone(),
                        });
                        scope.insert(name.clone(), (value_ty, false));
                    }
                }
            }
            Stmt::Return(Some(e)) => {
                let actual = self.infer_expr(e, scope);
                if let Some(expected) = self.current_return_type.clone() {
                    self.note_expected(e, &expected);
                }
                let adapted = match self.current_return_type.clone() {
                    Some(expected) => self.adapt_literals(e, &expected, &actual),
                    None => false,
                };
                if let Some(expected) = &self.current_return_type {
                    if !adapted && !compatible(expected, &actual) {
                        self.push(
                            "E1041",
                            format!(
                                "Return expression expects '{}', got '{}'.",
                                expected.describe(),
                                actual.describe()
                            ),
                        );
                    }
                }
            }
            Stmt::Return(None) => {
                if let Some(expected) = &self.current_return_type {
                    if *expected != Ty::Void {
                        self.push(
                            "E1041",
                            format!(
                                "Empty return expects function return type 'Void', got '{}'.",
                                expected.describe()
                            ),
                        );
                    }
                }
            }
            Stmt::Continue => {}
            Stmt::Break(Some(e)) => { self.infer_expr(e, scope); }
            Stmt::Break(None) => {}
            Stmt::For { pattern, iter, body } => {
                let iter_ty = self.infer_expr(iter, scope);
                // A user type with `impl Iterator<T>` yields `T` (its `next` returns `Option<T>`).
                let user_iterator_item = match &iter_ty {
                    Ty::Named(n) | Ty::Applied(n, _) => self
                        .implementations
                        .iter()
                        .find(|im| im.trait_name.as_deref() == Some("Iterator") && im.type_name == *n)
                        .and_then(|im| im.trait_args.first().cloned())
                        .map(|t| self.resolve_type_in_context(&t)),
                    _ => None,
                };
                let elem_ty = iterator_element_type(&iter_ty).or(user_iterator_item).unwrap_or(iter_ty);
                self.editor_bindings.push(EditorBinding {
                    name: pattern.clone(),
                    type_name: elem_ty.describe(),
                    function: self.current_function_name.clone().unwrap_or_default(),
                    scope_depth: self.editor_scope_depth,
                    span: self.current_span.unwrap_or_default(),
                    source_file: self.current_source_file.clone(),
                });
                let mut inner = scope.clone();
                inner.insert(pattern.clone(), (elem_ty, false));
                self.check_block(body, &mut inner);
            }
            Stmt::While { cond, body } => {
                let cond_ty = self.infer_expr(cond, scope);
                if !compatible(&Ty::Bool, &cond_ty) {
                    self.push(
                        "E1041",
                        format!("While condition expects 'Bool', got '{}'.", cond_ty.describe()),
                    );
                }
                let mut inner = scope.clone();
                self.check_block(body, &mut inner);
            }
            Stmt::FieldAssign { target, value } => {
                let target_ty = self.infer_expr(target, scope);
                let value_ty = self.infer_expr(value, scope);
                self.note_expected(value, &target_ty);
                if let Expr::FieldAccess(receiver, field) = target {
                    self.check_field_assignment_target(receiver, field, &target_ty, scope);
                }
                if !compatible(&target_ty, &value_ty) && !self.adapt_literals(value, &target_ty, &value_ty) {
                    self.push(
                        "E1041",
                        format!(
                            "Cannot assign a value of type '{}' to field target of type '{}'.",
                            value_ty.describe(),
                            target_ty.describe()
                        ),
                    );
                }
            }
            Stmt::Expr(e) => { self.infer_expr(e, scope); }
        }
    }

    /// Types an untyped integer literal (or a negated one) as `kind` when the
    /// context demands it. Records the choice for the backends and reports an
    /// out-of-range literal. Returns false when `expr` is not such a literal.
    fn adapt_int_literal(&mut self, expr: &Expr, kind: IntKind) -> bool {
        // `-5` is a negated literal: the literal itself is what gets typed.
        let (literal, negated) = match expr.unlocated() {
            Expr::Unary(UnaryOp::Neg, operand) => (operand.as_ref(), true),
            _ => (expr, false),
        };
        // Peel every `Located` layer; they all share the innermost range.
        let mut node = literal;
        let mut range = None;
        while let Expr::Located(inner, r) = node {
            range = Some(*r);
            node = inner;
        }
        let (Expr::IntLiteral(n), Some(range)) = (node, range) else { return false };
        let value = if negated { -(*n as i128) } else { *n as i128 };
        if !kind.fits(value) {
            self.push("E1070", format!("Literal {value} does not fit in '{}' (range {}..={}).", kind.name(), kind.min(), kind.max()));
            return true;
        }
        let key = ExprKey { file: self.current_source_file.clone(), start: range.start, end: range.end };
        self.literal_kinds.insert(key.clone(), LitKind::Int(kind));
        self.expr_types.insert(key, Ty::Sized(kind));
        // Every layer around the literal (and a negation around them) has its type too.
        let mut layer = literal;
        loop {
            self.node_types.insert(layer as *const Expr as usize, Ty::Sized(kind));
            match layer {
                Expr::Located(inner, _) => layer = inner,
                _ => break,
            }
        }
        if negated {
            self.node_types.insert(expr as *const Expr as usize, Ty::Sized(kind));
            self.node_types.insert(expr.unlocated() as *const Expr as usize, Ty::Sized(kind));
        }
        // The negation wrapping the literal has the literal's type too.
        if negated {
            if let Expr::Located(_, outer) = expr {
                let outer_key = ExprKey { file: self.current_source_file.clone(), start: outer.start, end: outer.end };
                self.expr_types.insert(outer_key, Ty::Sized(kind));
            }
        }
        true
    }

    fn adapt_scalar_operand(&mut self, expr: &Expr, elem: &Ty, ty: &mut Ty) {
        match (elem, &*ty) {
            (Ty::Float32, Ty::Float) if self.adapt_float_literal(expr) => *ty = Ty::Float32,
            (Ty::Sized(kind), Ty::Int) if self.adapt_int_literal(expr, *kind) => *ty = elem.clone(),
            _ => {}
        }
    }

    /// `array`, `zeros`, `ones`, `full`, `arange`, `linspace`.
    fn check_array_call(&mut self, name: &str, arg_types: &[Ty]) -> Option<Ty> {
        let arity = match name {
            "array" | "zeros" | "ones" => 1,
            "full" | "arange" | "cov" | "corr" => 2,
            "where" => 3,
            "linspace" => 3,
            _ => return None,
        };
        if arg_types.len() != arity {
            self.push("E1041", format!("'{name}' expects {arity} argument(s), got {}.", arg_types.len()));
            return Some(Ty::Unknown);
        }
        let is_shape = |t: &Ty| matches!(t, Ty::List(e) if **e == Ty::Int || **e == Ty::Unknown);
        let array_of = |t: Ty| Ty::Applied("Array".to_string(), vec![t]);
        match name {
            "where" => {
                let bools = Ty::Applied("Array".to_string(), vec![Ty::Bool]);
                if arg_types[0] != bools && arg_types[0] != Ty::Unknown {
                    self.push("E1041", format!("'where' expects an Array<Bool> mask first, got '{}'.", arg_types[0].describe()));
                    return Some(Ty::Unknown);
                }
                let element = |t: &Ty| array_elem(t).unwrap_or_else(|| t.clone());
                let (a, b) = (element(&arg_types[1]), element(&arg_types[2]));
                if (a != b && a != Ty::Unknown && b != Ty::Unknown) || !is_array_scalar(&a) {
                    self.push("E1041", format!("'where' needs two values of the same element type, got '{}' and '{}'.", arg_types[1].describe(), arg_types[2].describe()));
                    return Some(Ty::Unknown);
                }
                Some(array_of(a))
            }
            "cov" | "corr" => {
                let (a, b) = (&arg_types[0], &arg_types[1]);
                match (array_elem(a), array_elem(b)) {
                    (Some(x), Some(y)) if x == y && matches!(x, Ty::Float | Ty::Float32) => Some(x),
                    _ if *a == Ty::Unknown || *b == Ty::Unknown => Some(Ty::Unknown),
                    _ => {
                        self.push("E1041", format!("'{name}' expects two arrays of the same float type, got '{}' and '{}'.", a.describe(), b.describe()));
                        Some(Ty::Unknown)
                    }
                }
            }
            "array" => {
                let mut ty = &arg_types[0];
                let mut depth = 0;
                while let Ty::List(inner) = ty {
                    ty = inner;
                    depth += 1;
                }
                if depth == 0 || !is_array_scalar(ty) {
                    if *ty != Ty::Unknown {
                        self.push("E1041", format!("'array' expects a (nested) List of numbers, got '{}'.", arg_types[0].describe()));
                    }
                    return Some(Ty::Unknown);
                }
                Some(array_of(ty.clone()))
            }
            "zeros" | "ones" => {
                if !is_shape(&arg_types[0]) {
                    self.push("E1041", format!("'{name}' expects a shape List<Int>, got '{}'.", arg_types[0].describe()));
                }
                Some(array_of(Ty::Float))
            }
            "full" => {
                if !is_shape(&arg_types[0]) {
                    self.push("E1041", format!("'full' expects a shape List<Int>, got '{}'.", arg_types[0].describe()));
                }
                if !is_array_scalar(&arg_types[1]) && arg_types[1] != Ty::Unknown {
                    self.push("E1041", format!("'full' expects a numeric fill value, got '{}'.", arg_types[1].describe()));
                    return Some(Ty::Unknown);
                }
                Some(array_of(arg_types[1].clone()))
            }
            "arange" => {
                if arg_types.iter().any(|t| *t != Ty::Int && *t != Ty::Unknown) {
                    self.push("E1041", "'arange' expects two Int arguments (start, stop).".to_string());
                }
                Some(array_of(Ty::Int))
            }
            _ => {
                if arg_types[..2].iter().any(|t| *t != Ty::Float && *t != Ty::Unknown) || (arg_types[2] != Ty::Int && arg_types[2] != Ty::Unknown) {
                    self.push("E1041", "'linspace' expects (Float, Float, Int).".to_string());
                }
                Some(array_of(Ty::Float))
            }
        }
    }

    /// `linfit polyfit polyval solve histogram norm_pdf norm_cdf` (on `Array<Float>`).
    fn check_science_call(&mut self, name: &str, arg_types: &[Ty]) -> Option<Ty> {
        let arity = match name {
            "linfit" | "solve" | "polyval" => 2,
            "det" | "inv" | "trace" | "eye" | "norm" | "eigvals" => 1,
            "polyfit" | "norm_pdf" | "norm_cdf" => 3,
            "histogram" => 4,
            _ => return None,
        };
        if arg_types.len() != arity {
            self.push("E1041", format!("'{name}' expects {arity} argument(s), got {}.", arg_types.len()));
            return Some(Ty::Unknown);
        }
        let floats = Ty::Applied("Array".to_string(), vec![Ty::Float]);
        let ints = Ty::Applied("Array".to_string(), vec![Ty::Int]);
        let is_float_array = |t: &Ty| *t == floats || *t == Ty::Unknown;
        let is_float = |t: &Ty| *t == Ty::Float || *t == Ty::Unknown;
        let (ok, result) = match name {
            "linfit" | "solve" => (is_float_array(&arg_types[0]) && is_float_array(&arg_types[1]), floats.clone()),
            "det" | "trace" | "norm" => (is_float_array(&arg_types[0]), Ty::Float),
            "eigvals" => (is_float_array(&arg_types[0]), floats.clone()),
            "inv" => (is_float_array(&arg_types[0]), floats.clone()),
            "eye" => (arg_types[0] == Ty::Int || arg_types[0] == Ty::Unknown, floats.clone()),
            "polyfit" => (is_float_array(&arg_types[0]) && is_float_array(&arg_types[1]) && (arg_types[2] == Ty::Int || arg_types[2] == Ty::Unknown), floats.clone()),
            "polyval" => {
                let result = if is_float(&arg_types[1]) && arg_types[1] != Ty::Unknown { Ty::Float } else { floats.clone() };
                (is_float_array(&arg_types[0]) && (is_float(&arg_types[1]) || is_float_array(&arg_types[1])), result)
            }
            "histogram" => (is_float_array(&arg_types[0]) && (arg_types[1] == Ty::Int || arg_types[1] == Ty::Unknown) && is_float(&arg_types[2]) && is_float(&arg_types[3]), ints),
            _ => {
                let result = if is_float(&arg_types[0]) && arg_types[0] != Ty::Unknown { Ty::Float } else { floats.clone() };
                ((is_float(&arg_types[0]) || is_float_array(&arg_types[0])) && is_float(&arg_types[1]) && is_float(&arg_types[2]), result)
            }
        };
        if !ok {
            let got: Vec<String> = arg_types.iter().map(|t| t.describe()).collect();
            self.push("E1041", format!("'{name}' was called with unsupported argument types ({}); it works on Array<Float> and Float values.", got.join(", ")));
            return Some(Ty::Unknown);
        }
        Some(result)
    }

    /// Methods of `Rng`.
    /// Methods of `String` (`trim`, `split`, `to_float`, …).
    fn check_string_method(&mut self, method: &str, arg_types: &[Ty]) -> Ty {
        let list_of_strings = Ty::List(Box::new(Ty::String));
        let result_of = |ok: Ty| Ty::Applied("Result".to_string(), vec![ok, Ty::String]);
        let (expected, result): (Vec<Ty>, Ty) = match method {
            "length" => (vec![], Ty::Int),
            "is_empty" => (vec![], Ty::Bool),
            "trim" | "to_upper" | "to_lower" => (vec![], Ty::String),
            "contains" | "starts_with" | "ends_with" => (vec![Ty::String], Ty::Bool),
            "replace" => (vec![Ty::String, Ty::String], Ty::String),
            "split" => (vec![Ty::String], list_of_strings),
            "lines" => (vec![], list_of_strings),
            "to_int" => (vec![], result_of(Ty::Int)),
            "to_float" => (vec![], result_of(Ty::Float)),
            other => {
                self.push("E1042", format!("String has no method '{other}'."));
                return Ty::Unknown;
            }
        };
        let ok = arg_types.len() == expected.len() && expected.iter().zip(arg_types).all(|(e, a)| compatible(e, a));
        if !ok {
            self.push("E1041", format!("String method '{method}' was called with arguments of the wrong number or type."));
        }
        result
    }

    fn check_rng_method(&mut self, method: &str, arg_types: &[Ty]) -> Ty {
        let shape = |t: &Ty| matches!(t, Ty::List(e) if **e == Ty::Int || **e == Ty::Unknown);
        let ints = |ts: &[Ty]| ts.iter().all(|t| *t == Ty::Int || *t == Ty::Unknown);
        let (ok, result) = match method {
            "next_float" | "normal" => (arg_types.is_empty(), Ty::Float),
            "next_int" => (arg_types.len() == 2 && ints(arg_types), Ty::Int),
            "rand" | "randn" => (arg_types.len() == 1 && shape(&arg_types[0]), Ty::Applied("Array".to_string(), vec![Ty::Float])),
            "randint" => (arg_types.len() == 3 && ints(&arg_types[..2]) && shape(&arg_types[2]), Ty::Applied("Array".to_string(), vec![Ty::Int])),
            "permutation" => (arg_types.len() == 1 && ints(arg_types), Ty::Applied("Array".to_string(), vec![Ty::Int])),
            other => {
                self.push("E1042", format!("Rng has no method '{other}'."));
                return Ty::Unknown;
            }
        };
        if !ok {
            self.push("E1041", format!("Rng method '{method}' was called with arguments of the wrong number or type."));
        }
        result
    }

    /// `sin`, `cos`, `sqrt`, …, `abs`, `pow`, `atan2`, `pi`.
    fn check_math_call(&mut self, name: &str, arg_types: &[Ty]) -> Option<Ty> {
        const UNARY: &[&str] = &["sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "ln", "log10", "sqrt", "floor", "ceil", "round", "erf"];
        let arity = match name {
            "pi" => 0,
            "pow" | "atan2" => 2,
            n if UNARY.contains(&n) || n == "abs" => 1,
            _ => return None,
        };
        if arg_types.len() != arity {
            self.push("E1041", format!("'{name}' expects {arity} argument(s), got {}.", arg_types.len()));
            return Some(Ty::Unknown);
        }
        match name {
            "pi" => Some(Ty::Float),
            "pow" | "atan2" => match (&arg_types[0], &arg_types[1]) {
                (Ty::Float, Ty::Float) => Some(Ty::Float),
                (Ty::Float32, Ty::Float32) => Some(Ty::Float32),
                (Ty::Unknown, _) | (_, Ty::Unknown) => Some(Ty::Unknown),
                (a, b) => {
                    self.push("E1041", format!("'{name}' expects two Float or two Float32 arguments, got '{}' and '{}'.", a.describe(), b.describe()));
                    Some(Ty::Unknown)
                }
            },
            _ => {
                let arg = &arg_types[0];
                let elem = array_elem(arg).unwrap_or_else(|| arg.clone());
                let ok = match name {
                    "abs" => matches!(elem, Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_)),
                    _ => matches!(elem, Ty::Float | Ty::Float32),
                };
                if elem == Ty::Unknown {
                    return Some(Ty::Unknown);
                }
                if !ok {
                    let hint = if matches!(elem, Ty::Int | Ty::Sized(_)) { " (convert with 'as Float' first)" } else { "" };
                    self.push("E1041", format!("'{name}' isn't defined for '{}'{hint}.", arg.describe()));
                    return Some(Ty::Unknown);
                }
                Some(arg.clone())
            }
        }
    }

    /// Methods of `Array<T>`.
    fn check_array_method(&mut self, receiver: &Ty, method: &str, arg_types: &[Ty]) -> Ty {
        let elem = array_elem(receiver).expect("called for arrays only");
        let list_int = Ty::List(Box::new(Ty::Int));
        let expected_count = match method {
            "shape" | "rank" | "size" | "length" | "count" | "sum" | "min" | "max" | "mean" | "to_list" | "transpose" | "var" | "std" | "sample_var" | "sample_std" | "median" | "cumsum" | "sort" | "to_float" | "any" | "all" | "count_true" => Some(0),
            "reshape" | "sum_axis" | "dot" | "matmul" | "percentile" | "row" | "col" => Some(1),
            _ => None,
        };
        if let Some(count) = expected_count {
            if arg_types.len() != count {
                self.push("E1041", format!("Array method '{method}' expects {count} argument(s), got {}.", arg_types.len()));
                return Ty::Unknown;
            }
        }
        // Bool arrays are masks: only structural methods and any/all/count_true apply.
        let bool_ok = matches!(method, "shape" | "rank" | "size" | "length" | "count" | "to_list" | "reshape" | "transpose" | "get" | "set" | "any" | "all" | "count_true" | "row" | "col");
        if elem == Ty::Bool && !bool_ok {
            self.push("E1041", format!("'{method}' isn't defined for arrays of Bool."));
            return Ty::Unknown;
        }
        if elem != Ty::Bool && matches!(method, "any" | "all" | "count_true") {
            self.push("E1041", format!("'{method}' needs an Array<Bool>, got '{}'.", receiver.describe()));
            return Ty::Unknown;
        }
        match method {
            "shape" => list_int,
            "rank" | "size" | "length" | "count" => Ty::Int,
            "sum" | "min" | "max" => elem,
            "mean" => match elem {
                Ty::Int => Ty::Float,
                Ty::Float | Ty::Float32 => elem,
                other => {
                    self.push("E1041", format!("'mean' isn't defined for arrays of '{}'.", other.describe()));
                    Ty::Unknown
                }
            },
            "to_list" => Ty::List(Box::new(elem)),
            "any" | "all" => Ty::Bool,
            "count_true" => Ty::Int,
            "row" | "col" => {
                if arg_types[0] != Ty::Int && arg_types[0] != Ty::Unknown {
                    self.push("E1041", format!("'{method}' expects an Int index."));
                }
                Ty::Applied("Array".to_string(), vec![elem])
            }
            "cumsum" | "sort" => receiver.clone(),
            "to_float" => {
                if elem != Ty::Int {
                    self.push("E1041", format!("'to_float' converts an Array<Int>, got '{}'.", receiver.describe()));
                    return Ty::Unknown;
                }
                Ty::Applied("Array".to_string(), vec![Ty::Float])
            }
            "var" | "std" | "sample_var" | "sample_std" | "median" | "percentile" => {
                if !matches!(elem, Ty::Float | Ty::Float32) {
                    self.push("E1041", format!("'{method}' needs an array of Float or Float32, got '{}' (use to_float() on an Int array).", receiver.describe()));
                    return Ty::Unknown;
                }
                if method == "percentile" && arg_types[0] != Ty::Float && arg_types[0] != Ty::Unknown {
                    self.push("E1041", "'percentile' expects a Float p between 0.0 and 100.0.".to_string());
                }
                elem
            }
            "transpose" => receiver.clone(),
            "reshape" => {
                if !matches!(&arg_types[0], Ty::List(e) if **e == Ty::Int || **e == Ty::Unknown) {
                    self.push("E1041", "'reshape' expects a shape List<Int>.".to_string());
                }
                receiver.clone()
            }
            "sum_axis" => {
                if arg_types[0] != Ty::Int && arg_types[0] != Ty::Unknown {
                    self.push("E1041", "'sum_axis' expects an Int axis.".to_string());
                }
                receiver.clone()
            }
            "dot" | "matmul" => {
                if !compatible(receiver, &arg_types[0]) {
                    self.push("E1041", format!("'{method}' expects '{}', got '{}'.", receiver.describe(), arg_types[0].describe()));
                }
                if method == "dot" { elem } else { receiver.clone() }
            }
            "get" if !arg_types.is_empty() => {
                if arg_types.iter().any(|t| *t != Ty::Int && *t != Ty::Unknown) {
                    self.push("E1041", "'get' expects Int indices.".to_string());
                }
                elem
            }
            "set" if arg_types.len() >= 2 => {
                let (value, indices) = arg_types.split_last().expect("checked above");
                if indices.iter().any(|t| *t != Ty::Int && *t != Ty::Unknown) {
                    self.push("E1041", "'set' expects Int indices followed by the new value.".to_string());
                }
                if !compatible(&elem, value) {
                    self.push("E1041", format!("'set' expects a '{}' value, got '{}'.", elem.describe(), value.describe()));
                }
                Ty::Void
            }
            "get" | "set" => {
                self.push("E1041", format!("Array method '{method}' needs at least one index."));
                Ty::Unknown
            }
            other => {
                self.push("E1042", format!("Array has no method '{other}'."));
                Ty::Unknown
            }
        }
    }

    /// Types an untyped float literal (or a negated one) as `Float32`.
    fn adapt_float_literal(&mut self, expr: &Expr) -> bool {
        let (literal, negated) = match expr.unlocated() {
            Expr::Unary(UnaryOp::Neg, operand) => (operand.as_ref(), true),
            _ => (expr, false),
        };
        let mut node = literal;
        let mut range = None;
        while let Expr::Located(inner, r) = node {
            range = Some(*r);
            node = inner;
        }
        let (Expr::FloatLiteral(_), Some(range)) = (node, range) else { return false };
        let key = ExprKey { file: self.current_source_file.clone(), start: range.start, end: range.end };
        self.literal_kinds.insert(key.clone(), LitKind::F32);
        self.expr_types.insert(key, Ty::Float32);
        if negated {
            if let Expr::Located(_, outer) = expr {
                let outer_key = ExprKey { file: self.current_source_file.clone(), start: outer.start, end: outer.end };
                self.expr_types.insert(outer_key, Ty::Float32);
            }
        }
        true
    }

    /// Records `ty` for `expr` and every `Located` layer around it.
    fn set_node_type_layers(&mut self, expr: &Expr, ty: &Ty) {
        let mut layer = expr;
        loop {
            self.node_types.insert(layer as *const Expr as usize, ty.clone());
            match layer {
                Expr::Located(inner, _) => layer = inner,
                _ => break,
            }
        }
    }

    /// If `expected` is a fixed-width integer (or a list of them) and `expr`
    /// is made of untyped integer literals, adapts them and returns true.
    fn adapt_literals(&mut self, expr: &Expr, expected: &Ty, actual: &Ty) -> bool {
        match (expected, actual) {
            (Ty::Sized(kind), Ty::Int) => self.adapt_int_literal(expr, *kind),
            (Ty::Float32, Ty::Float) => self.adapt_float_literal(expr),
            (Ty::List(want), Ty::List(have)) if matches!(**want, Ty::Float32) && **have == Ty::Float => {
                let Expr::ListLiteral(items) = expr.unlocated() else { return false };
                let mut all = true;
                for item in items {
                    all &= self.adapt_float_literal(item);
                }
                if all {
                    if let Expr::Located(_, range) = expr {
                        let key = ExprKey { file: self.current_source_file.clone(), start: range.start, end: range.end };
                        self.expr_types.insert(key, expected.clone());
                    }
                    self.set_node_type_layers(expr, expected);
                }
                all
            }
            (Ty::List(want), Ty::List(have)) if matches!(**want, Ty::Sized(_)) && **have == Ty::Int => {
                let Expr::ListLiteral(items) = expr.unlocated() else { return false };
                let mut all = true;
                for item in items {
                    all &= self.adapt_literals(item, want, &Ty::Int);
                }
                if all {
                    if let Expr::Located(_, range) = expr {
                        let key = ExprKey { file: self.current_source_file.clone(), start: range.start, end: range.end };
                        self.expr_types.insert(key, expected.clone());
                    }
                    self.set_node_type_layers(expr, expected);
                }
                all
            }
            _ => false,
        }
    }

    /// Feeds a known expected type back into an expression whose own type
    /// came out only partially determined (`None`, `Ok(x)`, `Nothing`, a
    /// nested `Just(Good(3))`): the recorded type is refined to the expected
    /// one and the expectation is pushed down into branches and constructor
    /// arguments. Purely additive — it never reports errors and never
    /// replaces a fully known type.
    fn note_expected(&mut self, expr: &Expr, expected: &Ty) {
        if ty_contains_unknown(expected) {
            return;
        }
        match expr {
            Expr::Located(inner, range) => {
                let key = ExprKey { file: self.current_source_file.clone(), start: range.start, end: range.end };
                if let Some(recorded) = self.expr_types.get(&key) {
                    if ty_contains_unknown(recorded) && compatible(expected, recorded) {
                        self.expr_types.insert(key, expected.clone());
                        self.node_types.insert(expr as *const Expr as usize, expected.clone());
                        self.node_types.insert(&**inner as *const Expr as usize, expected.clone());
                    }
                }
                self.note_expected_inner(inner, expected);
            }
            other => self.note_expected_inner(other, expected),
        }
    }

    fn note_expected_block(&mut self, block: &Block, expected: &Ty) {
        if let Some(tail) = &block.tail {
            self.note_expected(tail, expected);
        }
    }

    fn note_expected_inner(&mut self, expr: &Expr, expected: &Ty) {
        match expr {
            Expr::Block(block) => self.note_expected_block(block, expected),
            Expr::If(_, then_block, else_block) => {
                self.note_expected_block(then_block, expected);
                if let Some(else_block) = else_block {
                    self.note_expected_block(else_block, expected);
                }
            }
            Expr::Match(_, arms) => {
                for arm in arms {
                    self.note_expected_block(&arm.body, expected);
                }
            }
            Expr::ListLiteral(items) => {
                if let Ty::List(element) = expected {
                    for item in items {
                        self.note_expected(item, element);
                    }
                }
            }
            Expr::Call(callee, args) | Expr::GenericCall(callee, _, args) => {
                let Expr::Ident(name) = callee.unlocated() else { return };
                let Some(enum_name) = self.variant_owners.get(name).cloned() else { return };
                let type_args: Vec<Ty> = match expected {
                    Ty::Applied(n, targs) if *n == enum_name => targs.clone(),
                    Ty::Named(n) if *n == enum_name => Vec::new(),
                    _ => return,
                };
                let generics = self.enum_generics.get(&enum_name).cloned().unwrap_or_default();
                let subst: HashMap<String, Ty> = generics.iter().map(|g| g.name.clone()).zip(type_args).collect();
                let field_types = self.variant_fields.get(&(enum_name.clone(), name.clone())).cloned().unwrap_or_default();
                let field_names = self.variant_field_names.get(&(enum_name, name.clone())).cloned().unwrap_or_default();
                for (position, arg) in args.iter().enumerate() {
                    let (index, value) = match arg {
                        Arg::Positional(value) => (Some(position), value),
                        Arg::Named(field, value) => (field_names.iter().position(|n| n.as_deref() == Some(field.as_str())), value),
                    };
                    if let Some(field_ty) = index.and_then(|i| field_types.get(i)) {
                        let field_ty = resolve_type_with_type_subst(field_ty, &subst, &HashMap::new());
                        self.note_expected(value, &field_ty);
                    }
                }
            }
            _ => {}
        }
    }

    fn infer_expr(&mut self, expr: &Expr, scope: &mut Scope) -> Ty {
        let is_call = matches!(expr, Expr::Call(..) | Expr::GenericCall(..));
        if is_call {
            self.call_node_stack.push(expr as *const Expr as usize);
        }
        let ty = self.infer_expr_inner(expr, scope);
        if is_call {
            self.call_node_stack.pop();
        }
        // A later, less-informed visit (a lambda is checked twice) never erases a known type.
        let key = expr as *const Expr as usize;
        if ty != Ty::Unknown || !self.node_types.contains_key(&key) {
            self.node_types.insert(key, ty.clone());
        }
        ty
    }

    fn infer_expr_inner(&mut self, expr: &Expr, scope: &mut Scope) -> Ty {
        match expr {
            Expr::Located(inner, range) => {
                let previous_span = self.current_span;
                self.current_span = Some(range.start);
                let is_call = matches!(inner.as_ref(), Expr::Call(..) | Expr::GenericCall(..));
                if is_call {
                    self.call_key_stack.push(ExprKey { file: self.current_source_file.clone(), start: range.start, end: range.end });
                }
                let ty = self.infer_expr(inner, scope);
                if is_call {
                    self.call_key_stack.pop();
                }
                self.current_span = previous_span;
                // A body can be inferred more than once (a lambda is first
                // checked in isolation); never let a later `Unknown` erase a
                // type that was already determined.
                let key = ExprKey { file: self.current_source_file.clone(), start: range.start, end: range.end };
                if ty != Ty::Unknown || !self.expr_types.contains_key(&key) {
                    self.expr_types.insert(key, ty.clone());
                }
                self.editor_expressions.push(EditorExpression {
                    type_name: ty.describe(),
                    function: self.current_function_name.clone().unwrap_or_default(),
                    span: range.start,
                    end: range.end,
                    source_file: self.current_source_file.clone(),
                });
                ty
            }
            Expr::IntLiteral(_) => Ty::Int,
            Expr::SizedIntLiteral(value, kind) => {
                if !kind.fits(*value) {
                    self.push("E1070", format!("Literal {value} does not fit in '{}' (range {}..={}).", kind.name(), kind.min(), kind.max()));
                }
                Ty::Sized(*kind)
            }
            Expr::FloatLiteral(_) => Ty::Float,
            Expr::Float32Literal(_) => Ty::Float32,
            Expr::StringLiteral(_) => Ty::String,
            Expr::CharLiteral(_) => Ty::Char,
            Expr::BoolLiteral(_) => Ty::Bool,
            Expr::UnitLiteral(num, unit) => {
                self.infer_expr(num, scope);
                match resolve_unit_expr(unit) {
                    Ok(dim) => Ty::Quantity(dim),
                    Err(bad) => {
                        self.push("E1010", format!("Unknown unit '{bad}' in '{unit}'."));
                        Ty::Unknown
                    }
                }
            }
            Expr::Ident(name) => {
                if let Some((ty, _)) = scope.get(name) {
                    ty.clone()
                } else if let Some(sig) = self.functions.get(name) {
                    Ty::Fn(
                        sig.params
                            .iter()
                            .map(|param| self.resolve_type_in_context(&param.ty))
                            .collect(),
                        Box::new(self.resolve_type_in_context(&sig.return_type)),
                    )
                } else if let Some(enum_name) = self.variant_owners.get(name) {
                    self.variant_type(enum_name, &[])
                } else {
                    Ty::Unknown
                }
            }
            Expr::Unary(UnaryOp::Neg, e)
                if matches!(e.unlocated(), Expr::SizedIntLiteral(v, k) if k.is_signed() && *v == -k.min()) =>
            {
                // `-128i8`: the magnitude alone doesn't fit, the negated value does.
                let Expr::SizedIntLiteral(_, kind) = e.unlocated() else { unreachable!() };
                let mut layer: &Expr = e;
                loop {
                    self.node_types.insert(layer as *const Expr as usize, Ty::Sized(*kind));
                    match layer {
                        Expr::Located(inner, _) => layer = inner,
                        _ => break,
                    }
                }
                Ty::Sized(*kind)
            }
            Expr::Unary(op, e) => {
                let t = self.infer_expr(e, scope);
                match op {
                    UnaryOp::Not if array_elem(&t) == Some(Ty::Bool) => t,
                    UnaryOp::Not => Ty::Bool,
                    UnaryOp::Neg => {
                        if let Ty::Sized(kind) = &t {
                            if !kind.is_signed() {
                                self.push("E1041", format!("Cannot negate an unsigned '{}'.", kind.name()));
                            }
                        }
                        t
                    }
                }
            }
            Expr::Binary(op, l, r) => {
                let mut lt = self.infer_expr(l, scope);
                let mut rt = self.infer_expr(r, scope);
                // An untyped integer literal next to a fixed-width operand takes its type.
                if let Ty::Sized(kind) = &lt {
                    if rt == Ty::Int && self.adapt_int_literal(r, *kind) {
                        rt = lt.clone();
                    }
                } else if let Ty::Sized(kind) = &rt {
                    if lt == Ty::Int && self.adapt_int_literal(l, *kind) {
                        lt = rt.clone();
                    }
                }
                // A scalar literal next to an array takes the array's element type.
                if let Some(elem) = array_elem(&lt) {
                    self.adapt_scalar_operand(r, &elem, &mut rt);
                } else if let Some(elem) = array_elem(&rt) {
                    self.adapt_scalar_operand(l, &elem, &mut lt);
                }
                if lt == Ty::Float32 && rt == Ty::Float && self.adapt_float_literal(r) {
                    rt = Ty::Float32;
                } else if rt == Ty::Float32 && lt == Ty::Float && self.adapt_float_literal(l) {
                    lt = Ty::Float32;
                }
                self.check_binary(*op, lt, rt)
            }
            Expr::Range(start, _kind, end, step) => {
                let st = self.infer_expr(start, scope);
                let et = self.infer_expr(end, scope);
                if let Some(s) = step { self.infer_expr(s, scope); }
                if let (Ty::Quantity(d1), Ty::Quantity(d2)) = (&st, &et) {
                    if d1 != d2 {
                        self.push(
                            "E1024",
                            format!(
                                "Invalid dimensional operation. Range endpoints have different dimensions: {} vs {}.",
                                dim_to_string(d1),
                                dim_to_string(d2)
                            ),
                        );
                    }
                }
                st
            }
            Expr::Call(callee, args) => self.check_call(callee, args, scope, None),
            Expr::GenericCall(callee, type_args, args) => self.check_call(callee, args, scope, Some(type_args)),
            Expr::FieldAccess(obj, field) => {
                let receiver_ty = self.infer_expr(obj, scope);
                if let Some(field_ty) = self.record_field_type(&receiver_ty, field) {
                    field_ty
                } else {
                    if self.is_concrete_user_type(&receiver_ty) {
                        self.push(
                            "E1043",
                            format!(
                                "Type '{}' has no field '{}'.",
                                receiver_ty.describe(),
                                field
                            ),
                        );
                    }
                    Ty::Unknown
                }
            }
            Expr::Index(obj, idx) => {
                let index_ty = self.infer_expr(idx, scope);
                let is_range = matches!(idx.unlocated(), Expr::Range(..));
                match self.infer_expr(obj, scope) {
                    Ty::List(t) => *t,
                    // `a[lo until hi]` and `a[mask]` give a new array; `a[i]` an element.
                    array if array_elem(&array).is_some() && (is_range || array_elem(&index_ty) == Some(Ty::Bool)) => array,
                    array if array_elem(&array).is_some() => array_elem(&array).unwrap(),
                    _ => Ty::Unknown,
                }
            }
            Expr::If(cond, then_block, else_block) => {
                let cond_ty = self.infer_expr(cond, scope);
                if !compatible(&Ty::Bool, &cond_ty) {
                    self.push(
                        "E1041",
                        format!("If condition expects 'Bool', got '{}'.", cond_ty.describe()),
                    );
                }
                let mut then_scope = scope.clone();
                let then_ty = self.check_block(then_block, &mut then_scope);
                match else_block {
                    Some(b) => {
                        let mut else_scope = scope.clone();
                        let else_ty = self.check_block(b, &mut else_scope);
                        if compatible(&then_ty, &else_ty) {
                            if then_ty == Ty::Unknown { else_ty } else { then_ty }
                        } else {
                            self.push(
                                "E1041",
                                format!(
                                    "'if' branches have different types: '{}' vs '{}'.",
                                    then_ty.describe(),
                                    else_ty.describe()
                                ),
                            );
                            Ty::Unknown
                        }
                    }
                    None => Ty::Void,
                }
            }
            Expr::Block(b) => {
                let mut inner = scope.clone();
                self.check_block(b, &mut inner)
            }
            Expr::Lambda(params, body) => {
                self.infer_lambda(params, body, None, None, scope)
            }
            Expr::ListLiteral(items) => {
                let mut elem = Ty::Unknown;
                for it in items {
                    let t = self.infer_expr(it, scope);
                    if elem == Ty::Unknown { elem = t; }
                }
                Ty::List(Box::new(elem))
            }
            Expr::SetLiteral(items) => {
                let mut elem = Ty::Unknown;
                for it in items {
                    let t = self.infer_expr(it, scope);
                    if elem == Ty::Unknown { elem = t; }
                }
                let ty = Ty::Set(Box::new(elem));
                self.validate_collection_bounds(&ty);
                ty
            }
            Expr::EmptyCollection(name, type_args) => {
                // The written type arguments are the collection's real type.
                let arg = |index: usize| type_args.get(index).map_or(Ty::Unknown, |t| self.resolve_type_in_context(t));
                let ty = if name == "Map" { Ty::Map(Box::new(arg(0)), Box::new(arg(1))) } else { Ty::Set(Box::new(arg(0))) };
                self.validate_collection_bounds(&ty);
                ty
            }
            Expr::MapLiteral(pairs) => {
                let mut key = Ty::Unknown;
                let mut value = Ty::Unknown;
                for (k, v) in pairs {
                    let kt = self.infer_expr(k, scope);
                    let vt = self.infer_expr(v, scope);
                    if key == Ty::Unknown { key = kt; }
                    if value == Ty::Unknown { value = vt; }
                }
                let ty = Ty::Map(Box::new(key), Box::new(value));
                self.validate_collection_bounds(&ty);
                ty
            }
            Expr::Try(inner, catch) => {
                let inner_ty = self.infer_expr(inner, scope);
                if inner_ty == Ty::Unknown {
                    if let Some(c) = catch { self.infer_expr(c, scope); }
                    return Ty::Unknown;
                }
                let Some(enclosing_return) = self.current_return_type.clone() else {
                    self.push(
                        "E1041",
                        "'try' can only be used inside a function returning Option or Result.".to_string(),
                    );
                    if let Some(c) = catch { self.infer_expr(c, scope); }
                    return Ty::Unknown;
                };
                match (&inner_ty, &enclosing_return) {
                    (Ty::Applied(name, args), Ty::Applied(expected_name, expected_args))
                        if name == "Option" && expected_name == "Option" && args.len() == 1 && expected_args.len() == 1 =>
                    {
                        if catch.is_some() {
                            self.push(
                                "E1041",
                                "Option 'try' does not accept 'catch'; handle absence with Option methods instead.".to_string(),
                            );
                        }
                        args[0].clone()
                    }
                    (Ty::Applied(name, args), Ty::Applied(expected_name, expected_args))
                        if name == "Result" && expected_name == "Result" && args.len() == 2 && expected_args.len() == 2 =>
                    {
                        if catch.is_none() && !compatible(&expected_args[1], &args[1]) {
                            self.push(
                                "E1041",
                                format!(
                                    "'try' propagates error type '{}', but the enclosing function returns '{}'.",
                                    args[1].describe(),
                                    expected_args[1].describe()
                                ),
                            );
                        }
                        if let Some(catch_expr) = catch {
                            let expected_catch = Ty::Fn(
                                vec![args[1].clone()],
                                Box::new(expected_args[1].clone()),
                            );
                            let catch_ty = self.infer_expr_with_expected(catch_expr, Some(&expected_catch), scope);
                            if !compatible(&expected_catch, &catch_ty) {
                                self.push(
                                    "E1041",
                                    format!(
                                        "'try catch' expects '{}', got '{}'.",
                                        expected_catch.describe(),
                                        catch_ty.describe()
                                    ),
                                );
                            }
                        }
                        args[0].clone()
                    }
                    _ => {
                        if let Some(c) = catch { self.infer_expr(c, scope); }
                        self.push(
                            "E1041",
                            format!(
                                "'try' expects a result compatible with the enclosing Option/Result return type, got '{}'.",
                                inner_ty.describe()
                            ),
                        );
                        Ty::Unknown
                    }
                }
            }
            Expr::Within(a, r) => {
                self.infer_expr(a, scope);
                self.infer_expr(r, scope);
                Ty::Bool
            }
            Expr::Approximately(a, b, tol) => {
                let ta = self.infer_expr(a, scope);
                let tb = self.infer_expr(b, scope);
                let tt = self.infer_expr(tol, scope);
                for (x, y) in [(&ta, &tb), (&ta, &tt)] {
                    if let (Ty::Quantity(d1), Ty::Quantity(d2)) = (x, y) {
                        if d1 != d2 {
                            self.push(
                                "E1091",
                                format!(
                                    "'approximately'/'tolerance' dimension mismatch: {} vs {}.",
                                    dim_to_string(d1),
                                    dim_to_string(d2)
                                ),
                            );
                        }
                    }
                }
                Ty::Bool
            }
            Expr::As(expr, unit_expr) => {
                let source_ty = self.infer_expr(expr, scope);
                // `x as UInt8` / `x as Int` / `x as Float`: an explicit numeric conversion.
                if let Expr::Ident(sym) = unit_expr.as_ref().unlocated() {
                    let target = match sym.as_str() {
                        "Int" | "Int64" => Some(Ty::Int),
                        "Float" | "Float64" => Some(Ty::Float),
                        "Float32" => Some(Ty::Float32),
                        other => IntKind::from_name(other).map(Ty::Sized),
                    };
                    if let Some(target) = target {
                        if !matches!(source_ty, Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_) | Ty::Unknown) {
                            self.push("E1041", format!("Cannot convert '{}' to '{}' with 'as'.", source_ty.describe(), target.describe()));
                        }
                        return target;
                    }
                }
                if let Expr::Ident(sym) = unit_expr.as_ref() {
                    if let Some(dim) = unit_dimension(sym) {
                        return Ty::Quantity(dim);
                    }
                }
                Ty::Unknown
            }
            Expr::Loop(block) => {
                let mut inner = scope.clone();
                self.check_block(block, &mut inner);
                Ty::Unknown
            }
            Expr::RecordLiteral(name, fields) => self.check_record_literal(name, fields, None, scope),
            Expr::GenericRecordLiteral(name, type_args, fields) => {
                self.check_record_literal(name, fields, Some(type_args), scope)
            }
            Expr::Match(scrutinee, arms) => {
                let scrutinee_ty = self.infer_expr(scrutinee, scope);
                self.check_match_exhaustiveness(&scrutinee_ty, arms);
                let mut result = Ty::Unknown;
                for arm in arms {
                    self.check_pattern(&arm.pattern, &scrutinee_ty);
                    let mut arm_scope = scope.clone();
                    self.bind_pattern_vars_typed(&arm.pattern, &scrutinee_ty, &mut arm_scope);
                    if let Some(g) = &arm.guard { self.infer_expr(g, &mut arm_scope); }
                    let t = self.check_block(&arm.body, &mut arm_scope);
                    if result == Ty::Unknown { result = t; }
                }
                result
            }
            Expr::Spawn(block) => {
                let free = free_vars_in_block(block);
                for name in &free {
                    if let Some((_, true)) = scope.get(name) {
                        self.push(
                            "E1100",
                            format!(
                                "Cannot capture mutable binding '{name}' in 'spawn'.\nMutable state cannot be shared directly between tasks.\nSend it through a channel instead."
                            ),
                        );
                    }
                }
                let mut inner = scope.clone();
                let result_ty = self.check_block(block, &mut inner);
                Ty::Applied("Task".to_string(), vec![result_ty])
            }
            Expr::SpawnScope(block) => {
                let mut inner = scope.clone();
                self.check_block(block, &mut inner)
            }
            Expr::Channel(element_type, capacity) => {
                if let Some(c) = capacity { self.infer_expr(c, scope); }
                Ty::Applied(
                    "Channel".to_string(),
                    vec![self.resolve_type_in_context(element_type)],
                )
            }
        }
    }

    fn check_record_literal(
        &mut self,
        name: &str,
        fields: &[(String, Expr)],
        explicit_type_args: Option<&[Type]>,
        scope: &mut Scope,
    ) -> Ty {
        let value_types: HashMap<String, Ty> = fields
            .iter()
            .map(|(field_name, value)| {
                let declared_fn = self
                    .record_fields
                    .get(name)
                    .and_then(|fs| fs.iter().find(|(n, _)| n == field_name).map(|(_, t)| t.clone()))
                    .map(|t| self.resolve_type_in_context(&t))
                    .filter(|t| matches!(t, Ty::Fn(..)));
                (field_name.clone(), self.infer_expr_with_expected(value, declared_fn.as_ref(), scope))
            })
            .collect();
        // Untyped integer literals in a field whose declared type is a
        // fixed-width integer (or a list of them) take that type.
        if let Some(declared_fields) = self.record_fields.get(name).cloned() {
            for (field_name, field_type) in declared_fields {
                let Some((_, value)) = fields.iter().find(|(n, _)| *n == field_name) else { continue };
                let declared = self.resolve_type_in_context(&field_type);
                if matches!(declared, Ty::Sized(_) | Ty::Float32 | Ty::List(_)) {
                    let actual = value_types.get(&field_name).cloned().unwrap_or(Ty::Unknown);
                    self.adapt_literals(value, &declared, &actual);
                }
            }
        }
        let Some(generics) = self.record_generics.get(name).cloned() else {
            return Ty::Named(name.to_string());
        };
        if generics.is_empty() {
            if explicit_type_args.is_some() {
                self.push(
                    "E1042",
                    format!("Record '{}' is not generic but received explicit type arguments.", name),
                );
            }
            return Ty::Named(name.to_string());
        }

        let declared_fields = self.record_fields.get(name).cloned().unwrap_or_default();
        let generic_names: HashSet<String> = generics.iter().map(|generic| generic.name.clone()).collect();
        let mut type_subst = HashMap::new();
        if let Some(explicit) = explicit_type_args {
            if explicit.len() != generics.len() {
                self.push(
                    "E1042",
                    format!(
                        "Record '{}' expects {} explicit generic argument(s), got {}.",
                        name,
                        generics.len(),
                        explicit.len()
                    ),
                );
            }
            for (generic, explicit_ty) in generics.iter().zip(explicit.iter()) {
                type_subst.insert(generic.name.clone(), self.resolve_type_in_context(explicit_ty));
            }
        }
        for (field_name, field_type) in declared_fields {
            if let Some(value_type) = value_types.get(&field_name) {
                if let Err(message) = unify_generic_type(&field_type, value_type, &generic_names, &mut type_subst) {
                    let code = if explicit_type_args.is_some() { "E1042" } else { "E1041" };
                    self.push(code, format!("Invalid value for field '{}.{}': {message}", name, field_name));
                }
            }
        }
        Ty::Applied(
            name.to_string(),
            generics
                .iter()
                .map(|generic| type_subst.get(&generic.name).cloned().unwrap_or(Ty::Unknown))
                .collect(),
        )
    }

    fn record_field_type(&self, receiver_ty: &Ty, field: &str) -> Option<Ty> {
        let (record_name, type_args) = match receiver_ty {
            Ty::Named(name) => (name, &[][..]),
            Ty::Applied(name, args) => (name, args.as_slice()),
            _ => return None,
        };
        let declared_fields = self.record_fields.get(record_name)?;
        let generic_subst = self
            .record_generics
            .get(record_name)
            .map(|generics| generic_substitution(generics, type_args))
            .unwrap_or_default();
        let (_, declared_type) = declared_fields.iter().find(|(name, _)| name == field)?;
        Some(resolve_type_with_type_subst(
            declared_type,
            &generic_subst,
            &HashMap::new(),
        ))
    }

    fn check_field_assignment_target(
        &mut self,
        receiver: &Expr,
        field: &str,
        target_ty: &Ty,
        scope: &Scope,
    ) {
        let receiver_ty = self.infer_expr(receiver, &mut scope.clone());
        let Some(record_name) = record_type_name(&receiver_ty) else { return };
        let Some(fields) = self.record_field_mutability.get(record_name) else { return };
        if !fields.get(field).copied().unwrap_or(false) {
            self.push(
                "E1002",
                format!(
                    "Cannot assign to immutable field '{}.{}'. Declare the field with 'mut'.",
                    record_name, field
                ),
            );
        }

        if let Some(binding) = root_binding_name(receiver) {
            if let Some((_, is_mut)) = scope.get(binding) {
                if !is_mut {
                    self.push(
                        "E1001",
                        format!(
                            "Cannot assign field '{}' through immutable binding '{}'. Declare it as 'mut {} = ...'.",
                            field, binding, binding
                        ),
                    );
                }
            }
        }

        if *target_ty == Ty::Unknown {
            return;
        }
    }

    fn check_pattern(&mut self, pattern: &Pattern, expected: &Ty) -> bool {
        match pattern {
            Pattern::Wildcard => true,
            Pattern::Ident(name) => {
                if let Some(owner) = self.variant_owners.get(name) {
                    let fields = self.variant_fields.get(&(owner.clone(), name.clone()));
                    let belongs = matches!(
                        expected,
                        Ty::Named(type_name) | Ty::Applied(type_name, _) if type_name == owner
                    )
                        || *expected == Ty::Unknown;
                    if !belongs {
                        self.push(
                            "E1061",
                            format!("Variant '{}' does not belong to type '{}'.", name, expected.describe()),
                        );
                        return false;
                    }
                    if fields.is_some_and(|fields| !fields.is_empty()) {
                        self.push(
                            "E1061",
                            format!("Variant '{}' carries data and must destructure its field(s).", name),
                        );
                        return false;
                    }
                }
                true
            }
            Pattern::Literal(literal) => {
                let literal_ty = pattern_literal_type(literal);
                if !compatible(expected, &literal_ty) {
                    self.push(
                        "E1061",
                        format!(
                            "Pattern literal has type '{}', but the match value has type '{}'.",
                            literal_ty.describe(),
                            expected.describe()
                        ),
                    );
                    return false;
                }
                true
            }
            Pattern::Range(start, _, end) => {
                let start_ty = pattern_literal_type(start);
                let end_ty = pattern_literal_type(end);
                if !compatible(expected, &start_ty) || !compatible(expected, &end_ty) || !compatible(&start_ty, &end_ty) {
                    self.push(
                        "E1061",
                        format!(
                            "Range pattern types '{}' and '{}' do not match '{}'.",
                            start_ty.describe(),
                            end_ty.describe(),
                            expected.describe()
                        ),
                    );
                    return false;
                }
                true
            }
            Pattern::Variant(name, fields) => {
                let Some((field_names, field_types)) = self.constructor_fields(expected, name) else {
                    self.push(
                        "E1061",
                        format!("Pattern constructor '{}' does not match '{}'.", name, expected.describe()),
                    );
                    return false;
                };
                let indices = pattern_field_indices(&field_names, fields);
                let mut valid = true;
                if fields.len() != field_types.len() {
                    self.push(
                        "E1061",
                        format!(
                            "Pattern '{}' provides {} field(s), but the constructor has {}.",
                            name,
                            fields.len(),
                            field_types.len()
                        ),
                    );
                    valid = false;
                }
                for (index, (_, subpattern)) in fields.iter().enumerate() {
                    let Some(field_index) = indices.get(index).and_then(|index| *index) else {
                        self.push(
                            "E1061",
                            format!("Pattern field '{}' is not declared by constructor '{}'.", fields[index].0, name),
                        );
                        valid = false;
                        continue;
                    };
                    if let Some(field_type) = field_types.get(field_index) {
                        valid &= self.check_pattern(subpattern, field_type);
                    }
                }
                valid
            }
        }
    }

    fn constructor_fields(&self, expected: &Ty, constructor: &str) -> Option<(Vec<Option<String>>, Vec<Ty>)> {
        if let Some(owner) = self.variant_owners.get(constructor) {
            let type_subst = match expected {
                Ty::Named(type_name) if type_name == owner => HashMap::new(),
                Ty::Applied(type_name, args) if type_name == owner => {
                    generic_substitution(self.enum_generics.get(owner)?, args)
                }
                Ty::Unknown => HashMap::new(),
                _ => return None,
            };
            let key = (owner.clone(), constructor.to_string());
            return Some((
                self.variant_field_names.get(&key).cloned().unwrap_or_default(),
                self.variant_fields
                    .get(&key)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|ty| resolve_type_with_type_subst(ty, &type_subst, &HashMap::new()))
                    .collect(),
            ));
        }
        let type_subst = match expected {
            Ty::Named(expected_name) if expected_name == constructor => HashMap::new(),
            Ty::Applied(expected_name, args) if expected_name == constructor => {
                generic_substitution(self.record_generics.get(constructor)?, args)
            }
            _ => return None,
        };
        let fields = self.record_fields.get(constructor)?;
        Some((
            fields.iter().map(|(name, _)| Some(name.clone())).collect(),
            fields
                .iter()
                .map(|(_, ty)| resolve_type_with_type_subst(ty, &type_subst, &HashMap::new()))
                .collect(),
        ))
    }

    fn bind_pattern_vars_typed(&self, pattern: &Pattern, expected: &Ty, scope: &mut Scope) {
        match pattern {
            Pattern::Ident(name) => {
                if !self.variant_owners.contains_key(name) {
                    scope.insert(name.clone(), (expected.clone(), false));
                }
            }
            Pattern::Variant(constructor, fields) => {
                let Some((field_names, field_types)) = self.constructor_fields(expected, constructor) else { return };
                let indices = pattern_field_indices(&field_names, fields);
                for (index, (_, subpattern)) in fields.iter().enumerate() {
                    let Some(field_index) = indices.get(index).and_then(|index| *index) else { continue };
                    if let Some(field_type) = field_types.get(field_index) {
                        self.bind_pattern_vars_typed(subpattern, field_type, scope);
                    }
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) | Pattern::Range(..) => {}
        }
    }

    fn check_binary(&mut self, op: BinOp, lt: Ty, rt: Ty) -> Ty {
        use BinOp::*;
        if lt == Ty::Unknown || rt == Ty::Unknown {
            return match op {
                Eq | NotEq | Lt | Gt | LtEq | GtEq | And | Or => Ty::Bool,
                _ => Ty::Unknown,
            };
        }
        if array_elem(&lt).is_some() || array_elem(&rt).is_some() {
            let (left_elem, right_elem) = (array_elem(&lt), array_elem(&rt));
            let same = match (&left_elem, &right_elem) {
                (Some(a), Some(b)) => a == b,
                (Some(a), None) => rt == *a,
                (None, Some(b)) => lt == *b,
                _ => false,
            };
            if !same {
                self.push(
                    "E1041",
                    format!("Cannot apply this operator to '{}' and '{}': array elements and scalars must have the same type.", lt.describe(), rt.describe()),
                );
                return Ty::Unknown;
            }
            let elem = left_elem.or(right_elem).expect("one side is an array");
            let array = if array_elem(&lt).is_some() { lt.clone() } else { rt.clone() };
            let bools = Ty::Applied("Array".to_string(), vec![Ty::Bool]);
            return match op {
                Add | Sub | Mul | Div if elem != Ty::Bool => array,
                Eq | NotEq | Lt | Gt | LtEq | GtEq if elem != Ty::Bool => bools,
                And | Or if elem == Ty::Bool => bools,
                _ => {
                    self.push("E1041", format!("This operator isn't defined on arrays of '{}' (arithmetic and comparisons need numbers, and/or need Bool).", elem.describe()));
                    Ty::Unknown
                }
            };
        }
        if matches!(lt, Ty::Sized(_) | Ty::Float32) || matches!(rt, Ty::Sized(_) | Ty::Float32) {
            if matches!(op, And | Or) {
                self.push("E1041", format!("Logical operators expect 'Bool' operands, got '{}' and '{}'.", lt.describe(), rt.describe()));
                return Ty::Bool;
            }
            return match (&lt, &rt) {
                (Ty::Sized(a), Ty::Sized(b)) if a == b => match op {
                    Eq | NotEq | Lt | Gt | LtEq | GtEq => Ty::Bool,
                    _ => lt.clone(),
                },
                (Ty::Float32, Ty::Float32) => match op {
                    Eq | NotEq | Lt | Gt | LtEq | GtEq => Ty::Bool,
                    _ => Ty::Float32,
                },
                _ => {
                    self.push(
                        "E1041",
                        format!(
                            "Cannot apply this operator to '{}' and '{}': fixed-width numbers never mix implicitly; convert one side with 'as'.",
                            lt.describe(),
                            rt.describe()
                        ),
                    );
                    match op {
                        Eq | NotEq | Lt | Gt | LtEq | GtEq => Ty::Bool,
                        _ => Ty::Unknown,
                    }
                }
            };
        }
        // A user type with an operator method accepts other right-hand types (`v * 2.0`), and a scalar on
        // the left uses the type's reflected method (`2.0 * v` -> `rmul`).
        if matches!(op, Add | Sub | Mul | Div) {
            let name = match op {
                Add => "add",
                Sub => "sub",
                Mul => "mul",
                _ => "div",
            };
            if let (Ty::Named(_), false) = (&lt, lt == rt) {
                if let Some(ret) = self.operator_method_return(&lt, name) {
                    return ret;
                }
            }
            if let (true, Ty::Named(_)) = (lt.is_numeric_scalar(), &rt) {
                if let Some(ret) = self.operator_method_return(&rt, &format!("r{name}")) {
                    return ret;
                }
            }
        }
        if let (Ty::Named(left_name), Ty::Named(right_name)) = (&lt, &rt) {
            if left_name == right_name {
                let trait_name = match op {
                    Add => Some("Add"),
                    Sub => Some("Sub"),
                    Mul => Some("Mul"),
                    Div => Some("Div"),
                    _ => None,
                };
                if trait_name.is_some_and(|name| {
                    self.trait_impls.contains(&(name.to_string(), left_name.clone()))
                }) {
                    return Ty::Named(left_name.clone());
                }
            }
        }
        match op {
            Add | Sub => match (&lt, &rt) {
                (Ty::Quantity(d1), Ty::Quantity(d2)) => {
                    if d1 == d2 {
                        Ty::Quantity(d1.clone())
                    } else {
                        self.push(
                            "E1024",
                            format!(
                                "Invalid dimensional operation. Cannot add/subtract {} and {}.",
                                dim_to_string(d1),
                                dim_to_string(d2)
                            ),
                        );
                        Ty::Unknown
                    }
                }
                (Ty::Quantity(_), t) | (t, Ty::Quantity(_)) if t.is_numeric_scalar() => {
                    self.push(
                        "E1025",
                        "Cannot combine a Quantity with a plain scalar without an explicit unit ('as <unit>')."
                            .to_string(),
                    );
                    Ty::Unknown
                }
                (Ty::String, Ty::String) => Ty::String,
                (Ty::Float, _) | (_, Ty::Float) if lt.is_numeric_scalar() && rt.is_numeric_scalar() => Ty::Float,
                (Ty::Int, Ty::Int) => Ty::Int,
                _ => {
                    self.push("E1041", format!("Cannot apply '+'/'-' to '{}' and '{}'.", lt.describe(), rt.describe()));
                    Ty::Unknown
                }
            },
            Mul | Div => match (&lt, &rt) {
                (Ty::Quantity(d1), Ty::Quantity(d2)) => {
                    let combined = if op == Mul { dim_mul(d1, d2) } else { dim_div(d1, d2) };
                    if op == Div && dim_is_dimensionless(&combined) { Ty::Float } else { Ty::Quantity(combined) }
                }
                (Ty::Quantity(d), t) if t.is_numeric_scalar() => Ty::Quantity(d.clone()),
                (t, Ty::Quantity(d)) if t.is_numeric_scalar() && op == Mul => Ty::Quantity(d.clone()),
                (t, Ty::Quantity(d)) if t.is_numeric_scalar() && op == Div => Ty::Quantity(dim_pow(d, -1)),
                (Ty::Int, Ty::Int) => Ty::Int,
                (a, b) if a.is_numeric_scalar() && b.is_numeric_scalar() => Ty::Float,
                _ => {
                    self.push("E1041", format!("Cannot apply '*'//'/' to '{}' and '{}'.", lt.describe(), rt.describe()));
                    Ty::Unknown
                }
            },
            Rem => match (&lt, &rt) {
                (Ty::Int, Ty::Int) => Ty::Int,
                (a, b) if a.is_numeric_scalar() && b.is_numeric_scalar() => Ty::Float,
                _ => {
                    self.push("E1041", format!("Cannot apply '%' to '{}' and '{}'.", lt.describe(), rt.describe()));
                    Ty::Unknown
                }
            },
            Eq | NotEq | Lt | Gt | LtEq | GtEq => {
                if let (Ty::Quantity(d1), Ty::Quantity(d2)) = (&lt, &rt) {
                    if d1 != d2 {
                        self.push(
                            "E1024",
                            format!("Cannot compare {} and {}.", dim_to_string(d1), dim_to_string(d2)),
                        );
                    }
                }
                Ty::Bool
            }
            And | Or => {
                if !compatible(&Ty::Bool, &lt) || !compatible(&Ty::Bool, &rt) {
                    self.push(
                        "E1041",
                        format!(
                            "Logical operators expect 'Bool' operands, got '{}' and '{}'.",
                            lt.describe(),
                            rt.describe()
                        ),
                    );
                }
                Ty::Bool
            }
        }
    }

    fn constructor_shapes(&self, expected: &Ty, constructor: &str, depth: usize) -> Vec<Pattern> {
        let Some((field_names, field_types)) = self.constructor_fields(expected, constructor) else {
            return Vec::new();
        };
        if field_types.is_empty() {
            return vec![Pattern::Ident(constructor.to_string())];
        }

        let mut combinations: Vec<Vec<(String, Pattern)>> = vec![Vec::new()];
        for (index, field_type) in field_types.iter().enumerate() {
            let choices = self.type_shapes(field_type, depth.saturating_sub(1));
            let choices = if choices.is_empty() { vec![Pattern::Wildcard] } else { choices };
            let label = field_names
                .get(index)
                .and_then(|name| name.clone())
                .unwrap_or_else(|| format!("@{index}"));
            let mut next = Vec::new();
            for combination in &combinations {
                for choice in &choices {
                    let mut extended = combination.clone();
                    extended.push((label.clone(), choice.clone()));
                    next.push(extended);
                    if next.len() > MAX_PATTERN_SHAPES {
                        return vec![Pattern::Wildcard];
                    }
                }
            }
            combinations = next;
        }
        combinations
            .into_iter()
            .map(|fields| Pattern::Variant(constructor.to_string(), fields))
            .collect()
    }

    fn type_shapes(&self, expected: &Ty, depth: usize) -> Vec<Pattern> {
        if depth == 0 {
            return vec![Pattern::Wildcard];
        }
        let type_name = match expected {
            Ty::Named(name) | Ty::Applied(name, _) => name,
            _ => return vec![Pattern::Wildcard],
        };
        if let Some(variants) = self.enum_variants.get(type_name) {
            let mut shapes = Vec::new();
            for variant in variants {
                shapes.extend(self.constructor_shapes(expected, variant, depth));
                if shapes.len() > MAX_PATTERN_SHAPES {
                    return vec![Pattern::Wildcard];
                }
            }
            return shapes;
        }
        if self.record_fields.contains_key(type_name) {
            return self.constructor_shapes(expected, type_name, depth);
        }
        vec![Pattern::Wildcard]
    }

    fn pattern_covers_shape(&self, expected: &Ty, pattern: &Pattern, shape: &Pattern) -> bool {
        match pattern {
            Pattern::Wildcard => true,
            Pattern::Ident(name) => {
                if self.variant_owners.contains_key(name) {
                    matches!(shape, Pattern::Ident(shape_name) if shape_name == name)
                } else {
                    true
                }
            }
            Pattern::Literal(_) | Pattern::Range(..) => false,
            Pattern::Variant(name, fields) => {
                let Some((field_names, field_types)) = self.constructor_fields(expected, name) else {
                    return false;
                };
                if let Pattern::Ident(shape_name) = shape {
                    return shape_name == name && field_types.is_empty() && fields.is_empty();
                }
                let Pattern::Variant(shape_name, shape_fields) = shape else { return false };
                if shape_name != name || fields.len() != field_types.len() || shape_fields.len() != field_types.len() {
                    return false;
                }
                let pattern_indices = pattern_field_indices(&field_names, fields);
                let shape_indices = pattern_field_indices(&field_names, shape_fields);
                for field_index in 0..field_types.len() {
                    let pattern_position = pattern_indices.iter().position(|index| *index == Some(field_index));
                    let shape_position = shape_indices.iter().position(|index| *index == Some(field_index));
                    let (Some(pattern_position), Some(shape_position)) = (pattern_position, shape_position) else {
                        return false;
                    };
                    if !self.pattern_covers_shape(
                        &field_types[field_index],
                        &fields[pattern_position].1,
                        &shape_fields[shape_position].1,
                    ) {
                        return false;
                    }
                }
                true
            }
        }
    }

    fn check_match_exhaustiveness(&mut self, scrutinee_ty: &Ty, arms: &[MatchArm]) {
        let enum_name = match scrutinee_ty {
            Ty::Named(enum_name) | Ty::Applied(enum_name, _) => enum_name,
            _ => return,
        };
        let Some(variants) = self.enum_variants.get(enum_name).cloned() else { return };
        let patterns: Vec<&Pattern> = arms
            .iter()
            .filter(|arm| arm.guard.is_none())
            .map(|arm| &arm.pattern)
            .collect();

        let covered: HashSet<String> = variants
            .iter()
            .filter(|variant| {
                let shapes = self.constructor_shapes(scrutinee_ty, variant, 6);
                !shapes.is_empty()
                    && shapes.iter().all(|shape| {
                        patterns
                            .iter()
                            .any(|pattern| self.pattern_covers_shape(scrutinee_ty, pattern, shape))
                    })
            })
            .cloned()
            .collect();

        let missing: Vec<&str> = variants
            .iter()
            .filter(|variant| !covered.contains(*variant))
            .map(String::as_str)
            .collect();
        if missing.is_empty() { return; }

        let (label, list) = if missing.len() == 1 {
            ("Missing variant", format!("'{}'", missing[0]))
        } else {
            ("Missing variants", missing.iter().map(|v| format!("'{v}'")).collect::<Vec<_>>().join(", "))
        };
        self.push(
            "E1060",
            format!(
                "Non-exhaustive match on '{enum_name}'.\n{label}: {list}.\nAdd a case for the missing variant(s), or use '_' to cover remaining variants explicitly."
            ),
        );
    }

    fn infer_expr_with_expected(
        &mut self,
        expr: &Expr,
        expected: Option<&Ty>,
        scope: &mut Scope,
    ) -> Ty {
        if let (Expr::Lambda(params, body), Some(Ty::Fn(expected_params, expected_ret))) = (expr.unlocated(), expected) {
            let inferred = self.infer_lambda(params, body, Some(expected_params), Some(expected_ret.as_ref()), scope);
            let expected_type = Ty::Fn(expected_params.clone(), expected_ret.clone());
            let ty = refine_expected_type(&inferred, &expected_type);
            // This path bypasses `infer_expr`, so record the lambda's type here (every layer around it).
            let mut layer = expr;
            loop {
                self.node_types.insert(layer as *const Expr as usize, ty.clone());
                match layer {
                    Expr::Located(inner, range) => {
                        let key = ExprKey { file: self.current_source_file.clone(), start: range.start, end: range.end };
                        self.expr_types.insert(key, ty.clone());
                        layer = inner
                    }
                    _ => break,
                }
            }
            return ty;
        }
        let inferred = self.infer_expr(expr, scope);
        let Some(expected) = expected else { return inferred };
        let refined = refine_expected_type(&inferred, expected);
        if refined != inferred {
            self.set_node_type_layers(expr, &refined);
            let mut layer = expr;
            loop {
                if let Expr::Located(_, range) = layer {
                    let key = ExprKey { file: self.current_source_file.clone(), start: range.start, end: range.end };
                    self.expr_types.insert(key, refined.clone());
                    if let Expr::Located(inner, _) = layer {
                        layer = inner;
                        continue;
                    }
                }
                break;
            }
        }
        refined
    }

    fn infer_lambda(
        &mut self,
        params: &[String],
        body: &Block,
        expected_params: Option<&[Ty]>,
        expected_ret: Option<&Ty>,
        scope: &mut Scope,
    ) -> Ty {
        let mut inner = scope.clone();
        let mut parameter_types = Vec::with_capacity(params.len());
        for (index, parameter) in params.iter().enumerate() {
            let parameter_type = expected_params
                .and_then(|types| types.get(index).cloned())
                .unwrap_or(Ty::Unknown);
            parameter_types.push(parameter_type.clone());
            inner.insert(parameter.clone(), (parameter_type, false));
        }
        let previous_return_type = self.current_return_type.take();
        let ret = self.check_block_expecting(body, &mut inner, expected_ret.filter(|r| !matches!(r, Ty::Unknown)));
        self.current_return_type = previous_return_type;
        Ty::Fn(parameter_types, Box::new(ret))
    }

    fn check_call(
        &mut self,
        callee: &Expr,
        args: &[Arg],
        scope: &mut Scope,
        explicit_type_args: Option<&[Type]>,
    ) -> Ty {
        // A member call can provide the missing context for a lambda argument.
        // For example, `numbers.map(fn(x) { x * 2 })` should type `x` as the
        // element type of `numbers`, instead of degrading the whole expression
        // to `List<?>` because the lambda was initially inferred in isolation.
        let member_receiver = if let Expr::FieldAccess(receiver, method) = callee.unlocated() {
            Some((self.infer_expr(receiver, scope), method.as_str()))
        } else {
            None
        };

        let mut arg_types = Vec::with_capacity(args.len());
        for (index, arg) in args.iter().enumerate() {
            let expected = member_receiver.as_ref().and_then(|(receiver_ty, method)| {
                collection_method_expected_args(receiver_ty, method, arg_types.first())
                    .or_else(|| option_result_method_expected_args(receiver_ty, method))
                    .and_then(|expected_args| expected_args.get(index).cloned())
            });
            let expr = match arg {
                Arg::Positional(e) | Arg::Named(_, e) => e,
            };
            // A function-typed parameter of a plain (non-generic) user function tells a lambda argument its types.
            let expected = expected.or_else(|| {
                let Expr::Ident(name) = callee.unlocated() else { return None };
                let sig = self.functions.get(name)?.clone();
                if !sig.generics.is_empty() {
                    return None;
                }
                let param = match arg {
                    Arg::Positional(_) => sig.params.get(index)?,
                    Arg::Named(n, _) => sig.params.iter().find(|p| &p.name == n)?,
                };
                Some(self.resolve_type_in_context(&param.ty)).filter(|t| matches!(t, Ty::Fn(..)))
            });
            arg_types.push(self.infer_expr_with_expected(expr, expected.as_ref(), scope));
        }

        let arg_exprs: Vec<&Expr> = args
            .iter()
            .map(|arg| match arg {
                Arg::Positional(e) | Arg::Named(_, e) => e,
            })
            .collect();

        if let Expr::Ident(name) = callee.unlocated() {
            if let Some(return_type) = check_builtin_call(
                name,
                &arg_types,
                &mut self.errors,
                &self.record_derives,
                &self.record_fields,
                &self.enum_derives,
                &self.enum_variants,
                &self.variant_fields,
            ) {
                return return_type;
            }
            if self.variant_owners.contains_key(name) {
                return self.check_variant_constructor(name, &arg_types, &arg_exprs, explicit_type_args);
            }
            if let Some(sig) = self.functions.get(name) {
                return self.check_function_call(&sig.clone(), args, &arg_types, explicit_type_args);
            }
            if let Some(result) = self.check_array_call(name, &arg_types) {
                return result;
            }
            if let Some(result) = self.check_math_call(name, &arg_types) {
                return result;
            }
            if let Some(result) = self.check_science_call(name, &arg_types) {
                return result;
            }
            if name == "rng" {
                if arg_types.len() != 1 || (arg_types[0] != Ty::Int && arg_types[0] != Ty::Unknown) {
                    self.push("E1041", "'rng' expects one Int seed.".to_string());
                }
                return Ty::Named("Rng".to_string());
            }
        }

        if let Expr::FieldAccess(receiver, method) = callee.unlocated() {
            let receiver_ty = member_receiver
                .as_ref()
                .map(|(receiver_ty, _)| receiver_ty.clone())
                .unwrap_or_else(|| self.infer_expr(receiver, scope));
            if collection_method_requires_mut(&receiver_ty, method) {
                if let Expr::Ident(name) = receiver.as_ref() {
                    if let Some((_, false)) = scope.get(name) {
                        self.push(
                            "E1053",
                            format!(
                                "Cannot call '{method}' (requires 'mut self') on immutable binding '{name}'.\nDeclare it as 'mut {name} = ...' to allow calling mutating methods."
                            ),
                        );
                    }
                }
            }
            if let Ty::Dyn(trait_name) = &receiver_ty {
                let signature = self
                    .traits
                    .get(trait_name)
                    .and_then(|decl| decl.methods.iter().find(|m| m.name == *method))
                    .cloned();
                if let Some(signature) = signature {
                    return self.resolve_type_in_context(&signature.return_type);
                }
            }
            if receiver_ty == Ty::String && STRING_METHOD_NAMES.contains(&method.as_str()) {
                return self.check_string_method(method, &arg_types);
            }
            if receiver_ty == Ty::Named("Rng".to_string()) && self.functions.get("Rng").is_none() {
                return self.check_rng_method(method, &arg_types);
            }
            if array_elem(&receiver_ty).is_some() && method != "to_string" {
                return self.check_array_method(&receiver_ty, method, &arg_types);
            }
            if let Some(return_type) = self.check_generic_method_call(
                &receiver_ty,
                method,
                &arg_types,
                explicit_type_args,
            ) {
                return return_type;
            }
            if method == "to_string" {
                // `to_string` is a core operation provided for every runtime
                // value, including concrete user types and quantities.
                return Ty::String;
            }
            if let Some(return_type) = self.check_concrete_method_call(
                &receiver_ty,
                method,
                &arg_types,
                &arg_exprs,
                explicit_type_args,
            ) {
                return return_type;
            }
            if let Some(return_type) = check_concurrency_method(&receiver_ty, method, &arg_types, &mut self.errors) {
                return return_type;
            }
            if let Some(return_type) = check_option_result_method(&receiver_ty, method, &arg_types, &mut self.errors) {
                return return_type;
            }
            if let Some(return_type) = check_collection_method(&receiver_ty, method, &arg_types, &mut self.errors) {
                return return_type;
            }
            if self.is_concrete_user_type(&receiver_ty) {
                self.push(
                    "E1042",
                    format!(
                        "Type '{}' has no method '{}'.",
                        receiver_ty.describe(),
                        method
                    ),
                );
                return Ty::Unknown;
            }
            if explicit_type_args.is_some() {
                self.push(
                    "E1042",
                    "Explicit generic method arguments require a generic method available through the receiver's trait bound."
                        .to_string(),
                );
            }
            return collection_method_return_type(&receiver_ty, method, &arg_types);
        }

        if explicit_type_args.is_some() {
            self.push(
                "E1042",
                "Explicit generic arguments require a named generic function."
                    .to_string(),
            );
        }

        let callee_ty = self.infer_expr(callee, scope);
        match callee_ty {
            Ty::Fn(_, ret) => *ret,
            _ => Ty::Unknown,
        }
    }

    fn variant_type(&self, enum_name: &str, type_args: &[Ty]) -> Ty {
        let Some(generics) = self.enum_generics.get(enum_name) else {
            return Ty::Named(enum_name.to_string());
        };
        if generics.is_empty() {
            Ty::Named(enum_name.to_string())
        } else {
            let args = if type_args.is_empty() {
                vec![Ty::Unknown; generics.len()]
            } else {
                type_args.to_vec()
            };
            Ty::Applied(enum_name.to_string(), args)
        }
    }

    fn check_variant_constructor(
        &mut self,
        variant_name: &str,
        arg_types: &[Ty],
        arg_exprs: &[&Expr],
        explicit_type_args: Option<&[Type]>,
    ) -> Ty {
        let Some(enum_name) = self.variant_owners.get(variant_name).cloned() else {
            return Ty::Unknown;
        };
        let field_types = self
            .variant_fields
            .get(&(enum_name.clone(), variant_name.to_string()))
            .cloned()
            .unwrap_or_default();
        let generic_params = self.enum_generics.get(&enum_name).cloned().unwrap_or_default();
        if field_types.len() != arg_types.len() {
            self.push(
                "E1061",
                format!(
                    "Constructor '{}' expects {} argument(s), got {}.",
                    variant_name,
                    field_types.len(),
                    arg_types.len()
                ),
            );
        }
        let generic_names: HashSet<String> = generic_params.iter().map(|generic| generic.name.clone()).collect();
        let mut type_subst = HashMap::new();
        if let Some(explicit) = explicit_type_args {
            if generic_params.is_empty() {
                self.push(
                    "E1042",
                    format!("Constructor '{}' is not generic but received explicit type arguments.", variant_name),
                );
            } else {
                if explicit.len() != generic_params.len() {
                    self.push(
                        "E1042",
                        format!(
                            "Constructor '{}' expects {} explicit generic argument(s), got {}.",
                            variant_name,
                            generic_params.len(),
                            explicit.len()
                        ),
                    );
                }
                for (generic, explicit_ty) in generic_params.iter().zip(explicit.iter()) {
                    type_subst.insert(generic.name.clone(), self.resolve_type_in_context(explicit_ty));
                }
            }
        }
        for (index, (field_type, arg_type)) in field_types.iter().zip(arg_types.iter()).enumerate() {
            // An untyped integer literal for a fixed-width field takes the field's type.
            let declared = resolve_type_with_type_subst(field_type, &type_subst, &HashMap::new());
            let adapted = match arg_exprs.get(index) {
                Some(expr) if matches!(declared, Ty::Sized(_) | Ty::Float32 | Ty::List(_)) => self.adapt_literals(expr, &declared, arg_type),
                _ => false,
            };
            let arg_type = if adapted { &declared } else { arg_type };
            if let Err(message) = unify_generic_type(field_type, arg_type, &generic_names, &mut type_subst) {
                self.push("E1061", format!("Invalid argument for constructor '{}': {message}", variant_name));
            }
        }
        let type_args: Vec<Ty> = generic_params
            .iter()
            .map(|generic| type_subst.get(&generic.name).cloned().unwrap_or(Ty::Unknown))
            .collect();
        self.variant_type(&enum_name, &type_args)
    }

    fn bind_function_arguments(
        &mut self,
        sig: &FnSig,
        args: &[Arg],
        arg_types: &[Ty],
    ) -> Vec<Option<Ty>> {
        let mut bound = vec![None; sig.params.len()];
        let mut next_positional = 0usize;
        let mut saw_named = false;

        for (arg, arg_ty) in args.iter().zip(arg_types.iter()) {
            match arg {
                Arg::Positional(_) => {
                    if saw_named {
                        self.push(
                            "E1041",
                            "Positional arguments must come before named arguments.".to_string(),
                        );
                        continue;
                    }
                    if next_positional >= sig.params.len() {
                        self.push(
                            "E1041",
                            format!(
                                "Function expects at most {} argument(s), got {}.",
                                sig.params.len(),
                                args.len()
                            ),
                        );
                        continue;
                    }
                    bound[next_positional] = Some(arg_ty.clone());
                    next_positional += 1;
                }
                Arg::Named(name, _) => {
                    saw_named = true;
                    let Some(index) = sig.params.iter().position(|param| param.name == *name) else {
                        self.push(
                            "E1041",
                            format!("Function has no parameter named '{}'.", name),
                        );
                        continue;
                    };
                    if bound[index].is_some() {
                        self.push(
                            "E1041",
                            format!("Parameter '{}' was supplied more than once.", name),
                        );
                        continue;
                    }
                    bound[index] = Some(arg_ty.clone());
                }
            }
        }

        for (index, param) in sig.params.iter().enumerate() {
            if bound[index].is_none() && param.default.is_none() {
                self.push(
                    "E1041",
                    format!("Missing required argument '{}'.", param.name),
                );
            }
        }
        bound
    }

    fn check_function_call(
        &mut self,
        sig: &FnSig,
        args: &[Arg],
        arg_types: &[Ty],
        explicit_type_args: Option<&[Type]>,
    ) -> Ty {
        let bound_args = self.bind_function_arguments(sig, args, arg_types);
        let mut dim_subst: HashMap<String, Dimension> = HashMap::new();
        let mut type_subst: HashMap<String, Ty> = HashMap::new();

        if let Some(explicit) = explicit_type_args {
            if sig.generics.is_empty() {
                self.push(
                    "E1042",
                    "Explicit generic arguments were supplied to a non-generic function."
                        .to_string(),
                );
            } else {
                if explicit.len() != sig.generics.len() {
                    self.push(
                        "E1042",
                        format!(
                            "Function expects {} explicit generic argument(s), got {}.",
                            sig.generics.len(),
                            explicit.len()
                        ),
                    );
                }
                for (generic, explicit_ty) in sig.generics.iter().zip(explicit.iter()) {
                    if generic.bounds.iter().any(|bound| bound == "Dimension") {
                        dim_subst.insert(
                            generic.name.clone(),
                            resolve_dimension_with_subst(explicit_ty, &HashMap::new()),
                        );
                    } else {
                        type_subst.insert(
                            generic.name.clone(),
                            self.resolve_type_in_context(explicit_ty),
                        );
                    }
                }
            }
        }

        for (param, arg_ty) in sig.params.iter().zip(bound_args.iter()) {
            let Some(arg_ty) = arg_ty else { continue };
            if let (Type::Named(n, dim_args), Ty::Quantity(actual_dim)) = (&param.ty, arg_ty) {
                if n == "Quantity" && dim_args.len() == 1 {
                    if let Type::Named(dim_name, empty) = &dim_args[0] {
                        if empty.is_empty() && !is_known_base_dimension(dim_name) {
                            if let Some(expected_dim) = dim_subst.get(dim_name) {
                                if expected_dim != actual_dim {
                                    self.push(
                                        "E1042",
                                        format!(
                                            "Explicit dimension argument '{}' does not match the argument's dimension '{}'.",
                                            dim_to_string(expected_dim),
                                            dim_to_string(actual_dim)
                                        ),
                                    );
                                }
                            } else {
                                dim_subst.insert(dim_name.clone(), actual_dim.clone());
                            }
                        }
                    }
                }
            }
        }

        if sig.generics.is_empty() {
            for (index, (param, arg_ty)) in sig.params.iter().zip(bound_args.iter()).enumerate() {
                let Some(arg_ty) = arg_ty else { continue };
                let expected = resolve_type_with_subst(&param.ty, &dim_subst);
                if let Some(expr) = arg_expr_for_param(sig, args, index) {
                    self.note_expected(expr, &expected);
                }
                let adapted = match arg_expr_for_param(sig, args, index) {
                    Some(expr) => self.adapt_literals(expr, &expected, arg_ty),
                    None => false,
                };
                if !adapted && !compatible(&expected, arg_ty) {
                    self.push(
                        "E1041",
                        format!(
                            "Argument '{}' expects '{}', got '{}'.",
                            param.name,
                            expected.describe(),
                            arg_ty.describe()
                        ),
                    );
                }
            }
            return resolve_type_with_subst(&sig.return_type, &dim_subst);
        }

        let generic_names: HashSet<String> = sig
            .generics
            .iter()
            .filter(|g| !g.bounds.iter().any(|bound| bound == "Dimension"))
            .map(|g| g.name.clone())
            .collect();
        for (param, arg_ty) in sig.params.iter().zip(bound_args.iter()) {
            let Some(arg_ty) = arg_ty else { continue };
            if let Err(message) = unify_generic_type(&param.ty, arg_ty, &generic_names, &mut type_subst) {
                self.push("E1042", message);
            }
        }

        for generic in &sig.generics {
            if generic.bounds.iter().any(|bound| bound == "Dimension") {
                continue;
            }
            let Some(actual_ty) = type_subst.get(&generic.name) else {
                self.push(
                    "E1042",
                    format!("Cannot infer generic parameter '{}'.", generic.name),
                );
                continue;
            };
            for bound in &generic.bounds {
                if !self.type_satisfies_trait(actual_ty, bound) {
                    self.push(
                        "E1042",
                        format!(
                            "Type '{}' does not satisfy bound '{}' for generic parameter '{}'.",
                            actual_ty.describe(),
                            bound,
                            generic.name
                        ),
                    );
                }
            }
        }

        for (index, param) in sig.params.iter().enumerate() {
            let expected = resolve_type_with_type_subst(&param.ty, &type_subst, &dim_subst);
            if let Some(expr) = arg_expr_for_param(sig, args, index) {
                self.note_expected(expr, &expected);
            }
        }

        // Remember what this call instantiated, for backends.
        let complete = sig.generics.iter().all(|g| type_subst.contains_key(&g.name) || dim_subst.contains_key(&g.name));
        if complete {
            let recorded = CallSubst { types: type_subst.clone(), dims: dim_subst.clone() };
            if let Some(key) = self.call_key_stack.last().cloned() {
                self.call_substs.insert(key, recorded.clone());
            }
            if let Some(node) = self.call_node_stack.last().copied() {
                self.call_substs_by_node.insert(node, recorded);
            }
        }

        resolve_type_with_type_subst(&sig.return_type, &type_subst, &dim_subst)
    }

    fn check_generic_method_call(
        &mut self,
        receiver_ty: &Ty,
        method: &str,
        arg_types: &[Ty],
        explicit_type_args: Option<&[Type]>,
    ) -> Option<Ty> {
        let Ty::Generic(generic_name) = receiver_ty else { return None };
        let bounds = self.current_generic_bounds.get(generic_name).cloned().unwrap_or_default();
        let method_sig = bounds.iter().find_map(|bound| {
            self.traits
                .get(bound)
                .and_then(|trait_decl| trait_decl.methods.iter().find(|m| m.name == method))
                .cloned()
        });
        let Some(method_sig) = method_sig else {
            self.push(
                "E1042",
                format!(
                    "Generic type '{}' has no method '{}'; add a trait bound that provides it.",
                    generic_name, method
                ),
            );
            return Some(Ty::Unknown);
        };

        let method_generic_names: HashSet<String> = method_sig
            .generics
            .iter()
            .map(|generic| generic.name.clone())
            .collect();
        let mut method_subst = HashMap::new();
        match explicit_type_args {
            Some(explicit) => {
                if method_sig.generics.is_empty() {
                    self.push(
                        "E1042",
                        format!("Method '{}' is not generic but received explicit type arguments.", method),
                    );
                } else {
                    if explicit.len() != method_sig.generics.len() {
                        self.push(
                            "E1042",
                            format!(
                                "Method '{}' expects {} explicit generic argument(s), got {}.",
                                method,
                                method_sig.generics.len(),
                                explicit.len()
                            ),
                        );
                    }
                    for (generic, explicit_ty) in method_sig.generics.iter().zip(explicit.iter()) {
                        method_subst.insert(
                            generic.name.clone(),
                            self.resolve_type_in_context(explicit_ty),
                        );
                    }
                }
            }
            None if !method_sig.generics.is_empty() => {
                for (param, actual) in method_sig.params.iter().skip(1).zip(arg_types.iter()) {
                    if let Err(message) = unify_generic_type(
                        &param.ty,
                        actual,
                        &method_generic_names,
                        &mut method_subst,
                    ) {
                        self.push("E1042", message);
                    }
                }
                for generic in &method_sig.generics {
                    if !method_subst.contains_key(&generic.name) {
                        self.push(
                            "E1042",
                            format!("Cannot infer generic method parameter '{}'.", generic.name),
                        );
                    }
                }
            }
            None => {}
        }

        for generic in &method_sig.generics {
            let Some(actual_ty) = method_subst.get(&generic.name) else { continue };
            for bound in &generic.bounds {
                if !self.type_satisfies_trait(actual_ty, bound) {
                    self.push(
                        "E1042",
                        format!(
                            "Type '{}' does not satisfy bound '{}' for generic method parameter '{}'.",
                            actual_ty.describe(),
                            bound,
                            generic.name
                        ),
                    );
                }
            }
        }

        let mut substitutions = method_subst;
        substitutions.insert("Self".to_string(), receiver_ty.clone());

        let expected_args = method_sig.params.iter().skip(1);
        for (param, actual) in expected_args.zip(arg_types.iter()) {
            let expected = resolve_type_with_type_subst(
                &param.ty,
                &substitutions,
                &HashMap::new(),
            );
            if !compatible(&expected, actual) {
                self.push(
                    "E1042",
                    format!(
                        "Argument for '{}' expects '{}', got '{}'.",
                        method,
                        expected.describe(),
                        actual.describe()
                    ),
                );
            }
        }
        if method_sig.params.len().saturating_sub(1) != arg_types.len() {
            self.push(
                "E1042",
                format!(
                    "Method '{}' expects {} argument(s), got {}.",
                    method,
                    method_sig.params.len().saturating_sub(1),
                    arg_types.len()
                ),
            );
        }
        Some(resolve_type_with_type_subst(
            &method_sig.return_type,
            &substitutions,
            &HashMap::new(),
        ))
    }

    fn check_concrete_method_call(
        &mut self,
        receiver_ty: &Ty,
        method: &str,
        arg_types: &[Ty],
        arg_exprs: &[&Expr],
        explicit_type_args: Option<&[Type]>,
    ) -> Option<Ty> {
        let candidate = self.concrete_method_candidate(receiver_ty, method)?;
        let method_generic_names: HashSet<String> = candidate
            .generics
            .iter()
            .map(|generic| generic.name.clone())
            .collect();
        let mut method_substitutions = HashMap::new();

        if let Some(explicit) = explicit_type_args {
            if candidate.generics.is_empty() {
                self.push(
                    "E1042",
                    format!("Method '{}' is not generic but received explicit type arguments.", method),
                );
            } else {
                if explicit.len() != candidate.generics.len() {
                    self.push(
                        "E1042",
                        format!(
                            "Method '{}' expects {} explicit generic argument(s), got {}.",
                            method,
                            candidate.generics.len(),
                            explicit.len()
                        ),
                    );
                }
                for (generic, explicit_ty) in candidate.generics.iter().zip(explicit.iter()) {
                    method_substitutions.insert(
                        generic.name.clone(),
                        self.resolve_type_in_context(explicit_ty),
                    );
                }
            }
        } else if !candidate.generics.is_empty() {
            for (param, actual) in candidate.params.iter().skip(1).zip(arg_types.iter()) {
                let mut expected = param.ty.clone();
                replace_self_type(&mut expected, &candidate.owner);
                substitute_impl_type_parameters(&mut expected, &candidate.impl_substitutions);
                if let Err(message) = unify_generic_type(
                    &expected,
                    actual,
                    &method_generic_names,
                    &mut method_substitutions,
                ) {
                    self.push("E1042", message);
                }
            }
            for generic in &candidate.generics {
                if !method_substitutions.contains_key(&generic.name) {
                    self.push(
                        "E1042",
                        format!("Cannot infer generic method parameter '{}'.", generic.name),
                    );
                }
            }
        }

        for generic in &candidate.generics {
            let Some(actual_ty) = method_substitutions.get(&generic.name) else { continue };
            for bound in &generic.bounds {
                if !self.type_satisfies_trait(actual_ty, bound) {
                    self.push(
                        "E1042",
                        format!(
                            "Type '{}' does not satisfy bound '{}' for generic method parameter '{}'.",
                            actual_ty.describe(),
                            bound,
                            generic.name
                        ),
                    );
                }
            }
        }

        let mut substitutions = candidate.impl_substitutions;
        substitutions.extend(method_substitutions);
        let expected_args: Vec<Ty> = candidate
            .params
            .iter()
            .skip(1)
            .map(|param| {
                let mut expected = param.ty.clone();
                replace_self_type(&mut expected, &candidate.owner);
                substitute_impl_type_parameters(&mut expected, &substitutions);
                resolve_type_with_type_subst(&expected, &substitutions, &HashMap::new())
            })
            .collect();

        for (index, (expected, actual)) in expected_args.iter().zip(arg_types.iter()).enumerate() {
            let adapted = match arg_exprs.get(index) {
                Some(expr) => self.adapt_literals(expr, expected, actual),
                None => false,
            };
            if !adapted && !compatible(expected, actual) {
                self.push(
                    "E1042",
                    format!(
                        "Argument for '{}' expects '{}', got '{}'.",
                        method,
                        expected.describe(),
                        actual.describe()
                    ),
                );
            }
        }
        if expected_args.len() != arg_types.len() {
            self.push(
                "E1042",
                format!(
                    "Method '{}' expects {} argument(s), got {}.",
                    method,
                    expected_args.len(),
                    arg_types.len()
                ),
            );
        }

        let mut return_type = candidate.return_type;
        replace_self_type(&mut return_type, &candidate.owner);
        substitute_impl_type_parameters(&mut return_type, &substitutions);
        Some(resolve_type_with_type_subst(
            &return_type,
            &substitutions,
            &HashMap::new(),
        ))
    }

    /// The result type of `receiver.method(..)` for an operator method of a user type.
    fn operator_method_return(&self, receiver: &Ty, method: &str) -> Option<Ty> {
        let found = self.concrete_method_candidate(receiver, method)?;
        Some(match &found.return_type {
            Type::Named(n, a) if n == "Self" && a.is_empty() => receiver.clone(),
            other => resolve_type(other),
        })
    }

    fn concrete_method_candidate(&self, receiver_ty: &Ty, method: &str) -> Option<ConcreteMethod> {
        for implementation in &self.implementations {
            let Some(impl_substitutions) = implementation_type_substitutions(receiver_ty, implementation) else {
                continue;
            };
            if !self.implementation_bounds_satisfied(implementation, &impl_substitutions) {
                continue;
            }
            if let Some(declared) = implementation.methods.iter().find(|candidate| candidate.name == method) {
                return Some(ConcreteMethod {
                    generics: declared.generics.clone(),
                    params: declared.params.clone(),
                    return_type: declared.return_type.clone(),
                    owner: implementation.type_name.clone(),
                    impl_substitutions,
                });
            }
            if let Some(trait_name) = &implementation.trait_name {
                if let Some(trait_decl) = self.traits.get(trait_name) {
                    if let Some(declared) = trait_decl.methods.iter().find(|candidate| candidate.name == method) {
                        let declared = specialize_trait_method(
                            declared,
                            &trait_decl.generics,
                            &implementation.trait_args,
                        );
                        return Some(ConcreteMethod {
                            generics: declared.generics.clone(),
                            params: declared.params.clone(),
                            return_type: declared.return_type.clone(),
                            owner: implementation.type_name.clone(),
                            impl_substitutions,
                        });
                    }
                }
            }
        }
        None
    }

    fn implementation_bounds_satisfied(
        &self,
        implementation: &ImplDecl,
        substitutions: &HashMap<String, Ty>,
    ) -> bool {
        implementation.generics.iter().all(|generic| {
            let Some(actual_ty) = substitutions.get(&generic.name) else { return true };
            generic
                .bounds
                .iter()
                .all(|bound| self.type_satisfies_impl_bound(actual_ty, bound))
        })
    }

    fn type_satisfies_impl_bound(&self, actual_ty: &Ty, bound: &str) -> bool {
        if *actual_ty == Ty::Unknown {
            return true;
        }
        if bound == "Dimension" {
            return matches!(actual_ty, Ty::Named(name)
                if is_dimension_name(name)
                    || self
                        .current_generic_bounds
                        .get(name)
                        .is_some_and(|bounds| bounds.iter().any(|candidate| candidate == "Dimension")));
        }
        self.type_satisfies_trait(actual_ty, bound)
    }

    fn validate_collection_bounds(&mut self, ty: &Ty) {
        match ty {
            Ty::Map(key, value) => {
                self.require_collection_bound(key, "Map key");
                self.validate_collection_bounds(key);
                self.validate_collection_bounds(value);
            }
            Ty::Set(element) => {
                self.require_collection_bound(element, "Set element");
                self.validate_collection_bounds(element);
            }
            Ty::List(element) => self.validate_collection_bounds(element),
            Ty::Applied(_, args) => {
                for arg in args {
                    self.validate_collection_bounds(arg);
                }
            }
            _ => {}
        }
    }

    fn validate_declared_collection_type(&mut self, ty: &Type, generics: &[GenericParam]) {
        let substitutions: HashMap<String, Ty> = generics
            .iter()
            .map(|generic| (generic.name.clone(), Ty::Generic(generic.name.clone())))
            .collect();
        let resolved = resolve_type_with_type_subst(ty, &substitutions, &HashMap::new());
        self.validate_collection_bounds(&resolved);
    }

    fn require_collection_bound(&mut self, ty: &Ty, role: &str) {
        if ty == &Ty::Unknown
            || (self.collection_trait_satisfied(ty, "Hash", &mut HashSet::new())
                && self.collection_trait_satisfied(ty, "Eq", &mut HashSet::new()))
        {
            return;
        }
        let missing: Vec<&str> = ["Hash", "Eq"]
            .into_iter()
            .filter(|trait_name| !self.collection_trait_satisfied(ty, trait_name, &mut HashSet::new()))
            .collect();
        let key = format!("{role}:{}", ty.describe());
        if self.collection_bound_diagnostics.insert(key) {
            self.push(
                "E1042",
                format!(
                    "{role} type '{}' must satisfy Hash + Eq; missing {}.",
                    ty.describe(),
                    missing.join(" + ")
                ),
            );
        }
    }

    fn collection_trait_satisfied(&self, ty: &Ty, trait_name: &str, visiting: &mut HashSet<String>) -> bool {
        if ty == &Ty::Unknown {
            return true;
        }
        if let Ty::Generic(name) = ty {
            return self
                .current_generic_bounds
                .get(name)
                .is_some_and(|bounds| bounds.iter().any(|bound| bound == trait_name));
        }
        match (trait_name, ty) {
            ("Hash" | "Eq", Ty::Int | Ty::Float | Ty::Bool | Ty::String | Ty::Sized(_) | Ty::Float32) => true,
            (_, Ty::List(element) | Ty::Set(element)) => self.collection_trait_satisfied(element, trait_name, visiting),
            (_, Ty::Map(key, value)) => {
                self.collection_trait_satisfied(key, trait_name, visiting)
                    && self.collection_trait_satisfied(value, trait_name, visiting)
            }
            (_, Ty::Applied(name, args)) if name == "Option" && args.len() == 1 => {
                self.collection_trait_satisfied(&args[0], trait_name, visiting)
            }
            (_, Ty::Applied(name, args)) if name == "Result" && args.len() == 2 => {
                self.collection_trait_satisfied(&args[0], trait_name, visiting)
                    && self.collection_trait_satisfied(&args[1], trait_name, visiting)
            }
            ("Hash" | "Eq", Ty::Named(name) | Ty::Applied(name, _)) => {
                let visit_key = format!("{trait_name}:{name}");
                if !visiting.insert(visit_key.clone()) {
                    return false;
                }
                let derives = self
                    .record_derives
                    .get(name)
                    .or_else(|| self.enum_derives.get(name));
                let direct = match trait_name {
                    "Hash" => derives.is_some_and(|traits| traits.iter().any(|candidate| candidate == "Hash")),
                    "Eq" => {
                        derives.is_some_and(|traits| traits.iter().any(|candidate| candidate == "Eq"))
                            || self.type_satisfies_trait(ty, "Eq")
                    }
                    _ => false,
                };
                let generic_subst: HashMap<String, Ty> = match ty {
                    Ty::Applied(_, args) => self
                        .record_generics
                        .get(name)
                        .into_iter()
                        .flat_map(|generics| generics.iter().zip(args.iter()))
                        .map(|(generic, arg)| (generic.name.clone(), arg.clone()))
                        .collect(),
                    _ => HashMap::new(),
                };
                let field_types: Vec<Ty> = if let Some(fields) = self.record_fields.get(name) {
                    fields
                        .iter()
                        .map(|(_, field)| resolve_type_with_type_subst(field, &generic_subst, &HashMap::new()))
                        .collect()
                } else if let Some(variants) = self.enum_variants.get(name) {
                    variants
                        .iter()
                        .flat_map(|variant| self.variant_fields.get(&(name.clone(), variant.clone())).into_iter().flatten())
                        .map(|field| resolve_type_with_type_subst(field, &generic_subst, &HashMap::new()))
                        .collect()
                } else {
                    Vec::new()
                };
                let fields_satisfy = field_types
                    .iter()
                    .all(|field| self.collection_trait_satisfied(field, trait_name, visiting));
                visiting.remove(&visit_key);
                direct && fields_satisfy
            }
            _ => false,
        }
    }

    fn is_concrete_user_type(&self, receiver_ty: &Ty) -> bool {
        match receiver_ty {
            Ty::Named(name) | Ty::Applied(name, _) => {
                self.record_fields.contains_key(name) || self.enum_generics.contains_key(name)
            }
            Ty::Quantity(_) => true,
            _ => false,
        }
    }

    fn type_satisfies_trait(&self, actual_ty: &Ty, trait_name: &str) -> bool {
        if let Ty::Generic(generic_name) = actual_ty {
            return self
                .current_generic_bounds
                .get(generic_name)
                .is_some_and(|bounds| bounds.iter().any(|bound| bound == trait_name));
        }
        if trait_name == "Dimension" {
            return matches!(actual_ty, Ty::Named(name) if is_dimension_name(name));
        }
        if builtin_type_satisfies_trait(actual_ty, trait_name) {
            return true;
        }
        self.implementations.iter().any(|implementation| {
            implementation.trait_name.as_deref() == Some(trait_name)
                && implementation_type_substitutions(actual_ty, implementation)
                    .is_some_and(|substitutions| {
                        self.implementation_bounds_satisfied(implementation, &substitutions)
                    })
        })
    }

    fn resolve_type_in_context(&self, ty: &Type) -> Ty {
        match ty {
            Type::Named(name, args)
                if args.is_empty()
                    && self.current_generic_bounds.contains_key(name)
                    && !is_dimension_generic(name, &self.current_generic_bounds) =>
            {
                Ty::Generic(name.clone())
            }
            Type::Named(name, args) if name == "List" && args.len() == 1 => {
                Ty::List(Box::new(self.resolve_type_in_context(&args[0])))
            }
            Type::Named(name, args) if name == "Quantity" && args.len() == 1 => {
                Ty::Quantity(resolve_dimension_with_subst(&args[0], &HashMap::new()))
            }
            Type::Named(name, args) if name == "Map" && args.len() == 2 => Ty::Map(
                Box::new(self.resolve_type_in_context(&args[0])),
                Box::new(self.resolve_type_in_context(&args[1])),
            ),
            Type::Named(name, args) if name == "Set" && args.len() == 1 => {
                Ty::Set(Box::new(self.resolve_type_in_context(&args[0])))
            }
            Type::Named(name, args) if !args.is_empty() => Ty::Applied(
                name.clone(),
                args.iter().map(|arg| self.resolve_type_in_context(arg)).collect(),
            ),
            Type::Fn(params, ret) => Ty::Fn(
                params.iter().map(|param| self.resolve_type_in_context(param)).collect(),
                Box::new(self.resolve_type_in_context(ret)),
            ),
            _ => resolve_type_with_subst(ty, &HashMap::new()),
        }
    }

    fn push(&mut self, code: &'static str, message: String) {
        self.errors.push(TypeError {
            code,
            message,
            span: self.current_span,
            source_file: self.current_source_file.clone(),
        });
    }
}

fn is_known_base_dimension(name: &str) -> bool {
    matches!(
        name,
        "Length" | "Mass" | "Time" | "Temperature" | "ElectricCurrent" | "AmountOfSubstance" | "LuminousIntensity"
            | "Currency" | "Information" | "Charge" | "Pressure"
    )
}

fn is_dimension_name(name: &str) -> bool {
    if name == "Dimensionless" || is_known_base_dimension(name) {
        return true;
    }
    name.split('*').all(|part| {
        let base = part.split_once('^').map_or(part, |(base, _)| base);
        is_known_base_dimension(base)
    })
}

pub fn resolve_type(ty: &Type) -> Ty {
    resolve_type_with_subst(ty, &HashMap::new())
}

fn resolve_type_with_subst(ty: &Type, subst: &HashMap<String, Dimension>) -> Ty {
    resolve_type_with_type_subst(ty, &HashMap::new(), subst)
}

fn resolve_type_with_type_subst(
    ty: &Type,
    type_subst: &HashMap<String, Ty>,
    dim_subst: &HashMap<String, Dimension>,
) -> Ty {
    match ty {
        Type::Named(name, args) => {
            if args.is_empty() {
                if let Some(resolved) = type_subst.get(name) {
                    return resolved.clone();
                }
            }
            match name.as_str() {
            "Int" | "Int64" => Ty::Int,
            other if args.is_empty() && IntKind::from_name(other).is_some() => Ty::Sized(IntKind::from_name(other).unwrap()),
            "Float" | "Float64" => Ty::Float,
            "Float32" => Ty::Float32,
            "Bool" => Ty::Bool,
            "Char" => Ty::Char,
            "String" => Ty::String,
            "Void" => Ty::Void,
            "Quantity" if args.len() == 1 => Ty::Quantity(resolve_dimension_with_subst(&args[0], dim_subst)),
            "List" if args.len() == 1 => Ty::List(Box::new(resolve_type_with_type_subst(&args[0], type_subst, dim_subst))),
            "Map" if args.len() == 2 => Ty::Map(
                Box::new(resolve_type_with_type_subst(&args[0], type_subst, dim_subst)),
                Box::new(resolve_type_with_type_subst(&args[1], type_subst, dim_subst)),
            ),
            "Set" if args.len() == 1 => Ty::Set(Box::new(resolve_type_with_type_subst(&args[0], type_subst, dim_subst))),
            _ if !args.is_empty() => Ty::Applied(
                name.clone(),
                args.iter()
                    .map(|arg| resolve_type_with_type_subst(arg, type_subst, dim_subst))
                    .collect(),
            ),
            _ => Ty::Named(name.clone()),
            }
        }
        Type::Fn(params, ret) => Ty::Fn(
            params.iter().map(|t| resolve_type_with_type_subst(t, type_subst, dim_subst)).collect(),
            Box::new(resolve_type_with_type_subst(ret, type_subst, dim_subst)),
        ),
        Type::Dyn(traits) if traits.len() == 1 => Ty::Dyn(traits[0].clone()),
        _ => Ty::Unknown,
    }
}

fn is_dimension_generic(name: &str, bounds: &HashMap<String, Vec<String>>) -> bool {
    bounds
        .get(name)
        .is_some_and(|generic_bounds| generic_bounds.iter().any(|bound| bound == "Dimension"))
}

fn generic_substitution(generics: &[GenericParam], args: &[Ty]) -> HashMap<String, Ty> {
    generics
        .iter()
        .zip(args.iter())
        .map(|(generic, arg)| (generic.name.clone(), arg.clone()))
        .collect()
}

fn unify_generic_type(
    param: &Type,
    actual: &Ty,
    generic_names: &HashSet<String>,
    subst: &mut HashMap<String, Ty>,
) -> Result<(), String> {
    if *actual == Ty::Unknown {
        return Ok(());
    }
    match param {
        Type::Named(name, args) if args.is_empty() && generic_names.contains(name) => {
            if let Some(previous) = subst.get(name) {
                // An earlier, only partly known binding (`Maybe<?>` from a bare
                // `Nothing`) yields to a fully known one.
                if ty_contains_unknown(previous) && !ty_contains_unknown(actual) && compatible(previous, actual) {
                    subst.insert(name.clone(), actual.clone());
                    return Ok(());
                }
                if compatible(previous, actual) || compatible(actual, previous) {
                    return Ok(());
                }
                return Err(format!(
                    "Generic parameter '{}' was inferred as both '{}' and '{}'.",
                    name,
                    previous.describe(),
                    actual.describe()
                ));
            }
            subst.insert(name.clone(), actual.clone());
            Ok(())
        }
        Type::Named(name, args) if name == "List" && args.len() == 1 => match actual {
            Ty::List(elem) => unify_generic_type(&args[0], elem, generic_names, subst),
            _ => Err(format!("Expected '{}', got '{}'.", resolve_type(param).describe(), actual.describe())),
        },
        Type::Named(name, args) if name == "Map" && args.len() == 2 => match actual {
            Ty::Map(key, value) => {
                unify_generic_type(&args[0], key, generic_names, subst)?;
                unify_generic_type(&args[1], value, generic_names, subst)
            }
            _ => Err(format!("Expected '{}', got '{}'.", resolve_type(param).describe(), actual.describe())),
        },
        Type::Named(name, args) if name == "Set" && args.len() == 1 => match actual {
            Ty::Set(elem) => unify_generic_type(&args[0], elem, generic_names, subst),
            _ => Err(format!("Expected '{}', got '{}'.", resolve_type(param).describe(), actual.describe())),
        },
        Type::Named(name, args) if name == "Quantity" && args.len() == 1 => {
            if matches!(actual, Ty::Quantity(_)) {
                Ok(())
            } else {
                Err(format!("Expected 'Quantity', got '{}'.", actual.describe()))
            }
        }
        Type::Named(name, args) if !args.is_empty() => match actual {
            Ty::Applied(actual_name, actual_args) if name == actual_name && args.len() == actual_args.len() => {
                for (expected, actual) in args.iter().zip(actual_args) {
                    unify_generic_type(expected, actual, generic_names, subst)?;
                }
                Ok(())
            }
            _ => Err(format!("Expected '{}', got '{}'.", resolve_type(param).describe(), actual.describe())),
        },
        Type::Named(_, _) => {
            let expected = resolve_type(param);
            if compatible(&expected, actual) {
                Ok(())
            } else {
                Err(format!("Expected '{}', got '{}'.", expected.describe(), actual.describe()))
            }
        }
        Type::Fn(params, ret) => match actual {
            Ty::Fn(actual_params, actual_ret) if params.len() == actual_params.len() => {
                for (expected, actual) in params.iter().zip(actual_params) {
                    unify_generic_type(expected, actual, generic_names, subst)?;
                }
                unify_generic_type(ret, actual_ret, generic_names, subst)
            }
            _ => Err(format!("Expected function, got '{}'.", actual.describe())),
        },
        Type::Mul(_, _) | Type::Div(_, _) | Type::Pow(_, _) | Type::Dyn(_) => Ok(()),
    }
}

/// The scalar types implement the operator traits themselves, so `fn max<T: Ord>(a: T, b: T)`
/// accepts `Int`, `Float`, `String`, ... without user `impl`s.
fn builtin_type_satisfies_trait(ty: &Ty, trait_name: &str) -> bool {
    let numeric = matches!(ty, Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_));
    match trait_name {
        "Add" => numeric || *ty == Ty::String,
        "Sub" | "Mul" | "Div" => numeric,
        "Ord" => numeric || matches!(ty, Ty::String | Ty::Char),
        "Eq" | "Hash" | "Printable" => numeric || matches!(ty, Ty::String | Ty::Char | Ty::Bool),
        _ => false,
    }
}

/// A block that ends in `return` (or in an `if`/`else` whose branches all end in `return`) never
/// falls off its end, so its missing tail value is not a type mismatch.
fn block_always_returns(block: &Block) -> bool {
    if block.tail.as_deref().is_some_and(expr_always_returns) {
        return true;
    }
    if block.tail.is_some() {
        return false;
    }
    match block.stmts.last().map(|located| &located.stmt) {
        Some(Stmt::Return(_)) => true,
        Some(Stmt::Expr(expr)) => expr_always_returns(expr),
        _ => false,
    }
}

pub(crate) fn expr_always_returns(expr: &Expr) -> bool {
    match expr.unlocated() {
        Expr::If(_, then_block, Some(else_block)) => block_always_returns(then_block) && block_always_returns(else_block),
        _ => false,
    }
}

fn collection_method_requires_mut(receiver_ty: &Ty, method: &str) -> bool {
    match receiver_ty {
        Ty::List(_) => matches!(method, "push" | "remove_at"),
        Ty::Map(_, _) => matches!(method, "set" | "remove"),
        Ty::Set(_) => matches!(method, "add" | "remove"),
        _ => false,
    }
}

fn record_type_name(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::Named(name) | Ty::Applied(name, _) => Some(name),
        _ => None,
    }
}

fn root_binding_name(expr: &Expr) -> Option<&str> {
    match expr.unlocated() {
        Expr::Ident(name) => Some(name),
        Expr::FieldAccess(receiver, _) | Expr::Index(receiver, _) => root_binding_name(receiver),
        _ => None,
    }
}

fn iterator_element_type(ty: &Ty) -> Option<Ty> {
    match ty {
        Ty::List(element) | Ty::Set(element) => Some((**element).clone()),
        Ty::Applied(name, args) if name == "Channel" && args.len() == 1 => Some(args[0].clone()),
        _ => None,
    }
}

fn check_concurrency_method(
    receiver_ty: &Ty,
    method: &str,
    arg_types: &[Ty],
    errors: &mut Vec<TypeError>,
) -> Option<Ty> {
    let Ty::Applied(type_name, type_args) = receiver_ty else { return None };
    let Some(element_type) = type_args.first().cloned() else { return None };

    let expected_count = match (type_name.as_str(), method) {
        ("Task", "join") | ("Task", "cancel") | ("Channel", "receive") | ("Channel", "close") => Some(0),
        ("Channel", "send") => Some(1),
        _ => None,
    }?;
    if arg_types.len() != expected_count {
        errors.push(TypeError {
            code: "E1041",
            message: format!(
                "Method '{}' expects {} argument(s), got {}.",
                method,
                expected_count,
                arg_types.len()
            ),
            span: None,
            source_file: None,
        });
        return Some(Ty::Unknown);
    }

    match (type_name.as_str(), method) {
        ("Task", "join") => Some(element_type),
        ("Task", "cancel") => Some(Ty::Bool),
        ("Channel", "receive") => Some(Ty::Applied(
            "Option".to_string(),
            vec![element_type],
        )),
        ("Channel", "close") => Some(Ty::Void),
        ("Channel", "send") => {
            if !compatible(&element_type, &arg_types[0]) {
                errors.push(TypeError {
                    code: "E1041",
                    message: format!(
                        "Method 'send' expects '{}', got '{}'.",
                        element_type.describe(),
                        arg_types[0].describe()
                    ),
                    span: None,
                    source_file: None,
                });
            }
            Some(Ty::Void)
        }
        _ => None,
    }
}

fn check_collection_method(
    receiver_ty: &Ty,
    method: &str,
    arg_types: &[Ty],
    errors: &mut Vec<TypeError>,
) -> Option<Ty> {
    let expected_args = collection_method_expected_args(receiver_ty, method, arg_types.first())?;

    if expected_args.len() != arg_types.len() {
        errors.push(TypeError {
            code: "E1041",
            message: format!(
                "Method '{}' expects {} argument(s), got {}.",
                method,
                expected_args.len(),
                arg_types.len()
            ),
            span: None,
            source_file: None,
        });
        return Some(Ty::Unknown);
    }
    for (index, (expected, actual)) in expected_args.iter().zip(arg_types.iter()).enumerate() {
        if !compatible(expected, actual) {
            errors.push(TypeError {
                code: "E1041",
                message: format!(
                    "Method '{}' argument #{} expects '{}', got '{}'.",
                    method,
                    index + 1,
                    expected.describe(),
                    actual.describe()
                ),
                span: None,
                source_file: None,
            });
        }
    }
    Some(collection_method_return_type(receiver_ty, method, arg_types))
}

fn collection_method_expected_args(
    receiver_ty: &Ty,
    method: &str,
    first_arg: Option<&Ty>,
) -> Option<Vec<Ty>> {
    match (receiver_ty, method) {
        (Ty::List(_), "length" | "count") => Some(Vec::new()),
        (Ty::List(element), "push") => Some(vec![(**element).clone()]),
        (Ty::List(_), "remove_at") => Some(vec![Ty::Int]),
        (Ty::List(element), "join") if **element == Ty::String => Some(vec![Ty::String]),
        (Ty::List(element), "map") => Some(vec![Ty::Fn(
            vec![(**element).clone()],
            Box::new(Ty::Unknown),
        )]),
        (Ty::List(element), "filter" | "find" | "any" | "all") => {
            Some(vec![Ty::Fn(vec![(**element).clone()], Box::new(Ty::Bool))])
        }
        (Ty::List(element), "fold") => Some(vec![
            Ty::Unknown,
            Ty::Fn(
                // `fold(initial, fn(accumulator, element) { … })`: the accumulator comes first.
                vec![
                    first_arg.cloned().unwrap_or(Ty::Unknown),
                    (**element).clone(),
                ],
                Box::new(Ty::Unknown),
            ),
        ]),
        (Ty::Map(_, _), "count") => Some(Vec::new()),
        (Ty::Map(key, _), "keys" | "contains_key") => {
            if method == "keys" {
                Some(Vec::new())
            } else {
                Some(vec![(**key).clone()])
            }
        }
        (Ty::Map(_, _), "values") => Some(Vec::new()),
        (Ty::Map(key, _), "get" | "remove") => Some(vec![(**key).clone()]),
        (Ty::Map(key, value), "set") => Some(vec![(**key).clone(), (**value).clone()]),
        (Ty::Set(_), "count") => Some(Vec::new()),
        (Ty::Set(element), "contains" | "add" | "remove") => Some(vec![(**element).clone()]),
        _ => None,
    }
}

fn check_option_result_method(
    receiver_ty: &Ty,
    method: &str,
    arg_types: &[Ty],
    errors: &mut Vec<TypeError>,
) -> Option<Ty> {
    let Ty::Applied(type_name, type_args) = receiver_ty else { return None };
    let (value_type, error_type) = match type_name.as_str() {
        "Option" if type_args.len() == 1 => (type_args[0].clone(), None),
        "Result" if type_args.len() == 2 => (type_args[0].clone(), Some(type_args[1].clone())),
        _ => return None,
    };

    let (expected_args, return_type) = match (type_name.as_str(), method) {
        ("Option", "is_some" | "is_none") => (Vec::new(), Ty::Bool),
        ("Option", "unwrap") => (Vec::new(), value_type.clone()),
        ("Option", "unwrap_or") => (vec![value_type.clone()], value_type.clone()),
        ("Option", "ok_or") => {
            let error = arg_types.first().cloned().unwrap_or(Ty::Unknown);
            (vec![Ty::Unknown], Ty::Applied("Result".to_string(), vec![value_type.clone(), error]))
        }
        ("Option", "map") => {
            let return_type = function_return_type(arg_types.first()).unwrap_or(Ty::Unknown);
            (
                vec![Ty::Fn(vec![value_type.clone()], Box::new(Ty::Unknown))],
                Ty::Applied("Option".to_string(), vec![return_type]),
            )
        }
        ("Option", "then") => (
            vec![Ty::Fn(
                vec![value_type.clone()],
                Box::new(Ty::Applied("Option".to_string(), vec![Ty::Unknown])),
            )],
            function_return_type(arg_types.first())
                .unwrap_or_else(|| Ty::Applied("Option".to_string(), vec![Ty::Unknown])),
        ),
        ("Result", "is_ok" | "is_err") => (Vec::new(), Ty::Bool),
        ("Result", "unwrap") => (Vec::new(), value_type.clone()),
        ("Result", "unwrap_or") => (vec![value_type.clone()], value_type.clone()),
        ("Result", "ok") => (
            Vec::new(),
            Ty::Applied("Option".to_string(), vec![value_type.clone()]),
        ),
        ("Result", "map") => {
            let return_type = function_return_type(arg_types.first()).unwrap_or(Ty::Unknown);
            (
                vec![Ty::Fn(vec![value_type.clone()], Box::new(Ty::Unknown))],
                Ty::Applied(
                    "Result".to_string(),
                    vec![return_type, error_type.clone().unwrap_or(Ty::Unknown)],
                ),
            )
        }
        ("Result", "map_err") => {
            let return_type = function_return_type(arg_types.first()).unwrap_or(Ty::Unknown);
            (
                vec![Ty::Fn(
                    vec![error_type.clone().unwrap_or(Ty::Unknown)],
                    Box::new(Ty::Unknown),
                )],
                Ty::Applied("Result".to_string(), vec![value_type.clone(), return_type]),
            )
        }
        ("Result", "then") => (
            vec![Ty::Fn(
                vec![value_type.clone()],
                Box::new(Ty::Applied(
                    "Result".to_string(),
                    vec![Ty::Unknown, error_type.clone().unwrap_or(Ty::Unknown)],
                )),
            )],
            function_return_type(arg_types.first()).unwrap_or_else(|| {
                Ty::Applied(
                    "Result".to_string(),
                    vec![Ty::Unknown, error_type.clone().unwrap_or(Ty::Unknown)],
                )
            }),
        ),
        _ => return None,
    };

    if expected_args.len() != arg_types.len() {
        errors.push(TypeError {
            code: "E1041",
            message: format!(
                "Method '{}' expects {} argument(s), got {}.",
                method,
                expected_args.len(),
                arg_types.len()
            ),
            span: None,
            source_file: None,
        });
        return Some(Ty::Unknown);
    }
    for (index, (expected, actual)) in expected_args.iter().zip(arg_types.iter()).enumerate() {
        if !compatible(expected, actual) {
            errors.push(TypeError {
                code: "E1041",
                message: format!(
                    "Method '{}' argument #{} expects '{}', got '{}'.",
                    method,
                    index + 1,
                    expected.describe(),
                    actual.describe()
                ),
                span: None,
                source_file: None,
            });
        }
    }
    Some(return_type)
}

fn option_result_method_expected_args(receiver_ty: &Ty, method: &str) -> Option<Vec<Ty>> {
    let Ty::Applied(type_name, type_args) = receiver_ty else { return None };
    let (value_type, error_type) = match type_name.as_str() {
        "Option" if type_args.len() == 1 => (type_args[0].clone(), None),
        "Result" if type_args.len() == 2 => (type_args[0].clone(), Some(type_args[1].clone())),
        _ => return None,
    };

    match (type_name.as_str(), method) {
        ("Option", "is_some" | "is_none" | "unwrap") => Some(Vec::new()),
        ("Option", "unwrap_or") => Some(vec![value_type]),
        ("Option", "ok_or") => Some(vec![Ty::Unknown]),
        ("Option", "map") => Some(vec![Ty::Fn(vec![value_type], Box::new(Ty::Unknown))]),
        ("Option", "then") => Some(vec![Ty::Fn(
            vec![value_type],
            Box::new(Ty::Applied("Option".to_string(), vec![Ty::Unknown])),
        )]),
        ("Result", "is_ok" | "is_err" | "unwrap" | "ok") => Some(Vec::new()),
        ("Result", "unwrap_or") => Some(vec![value_type]),
        ("Result", "map") => Some(vec![Ty::Fn(vec![value_type], Box::new(Ty::Unknown))]),
        ("Result", "map_err") => Some(vec![Ty::Fn(
            vec![error_type.unwrap_or(Ty::Unknown)],
            Box::new(Ty::Unknown),
        )]),
        ("Result", "then") => Some(vec![Ty::Fn(
            vec![value_type],
            Box::new(Ty::Applied(
                "Result".to_string(),
                vec![Ty::Unknown, error_type.unwrap_or(Ty::Unknown)],
            )),
        )]),
        _ => None,
    }
}

fn function_return_type(ty: Option<&Ty>) -> Option<Ty> {
    match ty {
        Some(Ty::Fn(_, return_type)) => Some((**return_type).clone()),
        _ => None,
    }
}

fn is_builtin_hashable(
    ty: &Ty,
    record_derives: &HashMap<String, Vec<String>>,
    record_fields: &HashMap<String, Vec<(String, Type)>>,
    enum_derives: &HashMap<String, Vec<String>>,
    enum_variants: &HashMap<String, Vec<String>>,
    variant_fields: &HashMap<(String, String), Vec<Type>>,
) -> bool {
    fn visit(
        ty: &Ty,
        record_derives: &HashMap<String, Vec<String>>,
        record_fields: &HashMap<String, Vec<(String, Type)>>,
        enum_derives: &HashMap<String, Vec<String>>,
        enum_variants: &HashMap<String, Vec<String>>,
        variant_fields: &HashMap<(String, String), Vec<Type>>,
        visiting: &mut HashSet<String>,
    ) -> bool {
    match ty {
        Ty::Int | Ty::Sized(_) | Ty::Float | Ty::Float32 | Ty::Bool | Ty::String | Ty::Unknown => true,
        Ty::List(inner) | Ty::Set(inner) => {
            visit(inner, record_derives, record_fields, enum_derives, enum_variants, variant_fields, visiting)
        }
        Ty::Map(key, value) => {
            visit(key, record_derives, record_fields, enum_derives, enum_variants, variant_fields, visiting)
                && visit(value, record_derives, record_fields, enum_derives, enum_variants, variant_fields, visiting)
        }
        Ty::Applied(name, args) if name == "Option" && args.len() == 1 => {
            visit(&args[0], record_derives, record_fields, enum_derives, enum_variants, variant_fields, visiting)
        }
        Ty::Applied(name, args) if name == "Result" && args.len() == 2 => {
            visit(&args[0], record_derives, record_fields, enum_derives, enum_variants, variant_fields, visiting)
                && visit(&args[1], record_derives, record_fields, enum_derives, enum_variants, variant_fields, visiting)
        }
        Ty::Named(name) => {
            if !visiting.insert(name.clone()) {
                return false;
            }
            let record_hashable = record_derives
                .get(name)
                .is_some_and(|derives| derives.iter().any(|derive| derive == "Hash"))
                && record_fields.get(name).is_some_and(|fields| {
                    fields.iter().all(|(_, field)| {
                        visit(&resolve_type(field), record_derives, record_fields, enum_derives, enum_variants, variant_fields, visiting)
                    })
                });
            let enum_hashable = enum_derives
                .get(name)
                .is_some_and(|derives| derives.iter().any(|derive| derive == "Hash"))
                && enum_variants.get(name).is_some_and(|variants| {
                    variants.iter().all(|variant| {
                        variant_fields.get(&(name.clone(), variant.clone())).is_some_and(|fields| {
                            fields.iter().all(|field| {
                                visit(&resolve_type(field), record_derives, record_fields, enum_derives, enum_variants, variant_fields, visiting)
                            })
                        })
                    })
                });
            visiting.remove(name);
            record_hashable || enum_hashable
        }
        _ => false,
    }
    }

    visit(ty, record_derives, record_fields, enum_derives, enum_variants, variant_fields, &mut HashSet::new())
}

fn check_builtin_call(
    name: &str,
    arg_types: &[Ty],
    errors: &mut Vec<TypeError>,
    record_derives: &HashMap<String, Vec<String>>,
    record_fields: &HashMap<String, Vec<(String, Type)>>,
    enum_derives: &HashMap<String, Vec<String>>,
    enum_variants: &HashMap<String, Vec<String>>,
    variant_fields: &HashMap<(String, String), Vec<Type>>,
) -> Option<Ty> {
    let expected_args = match name {
        "args" => vec![],
        "yield" => vec![],
        "env" => vec![Ty::String],
        "path_join" => vec![Ty::String, Ty::String],
        "cwd" => vec![],
        "file_exists" => vec![Ty::String],
        "hash" => vec![Ty::Unknown],
        "format" => vec![Ty::String, Ty::List(Box::new(Ty::String))],
        "select" => vec![Ty::Unknown],
        // Ownership primitives are intentionally generic. `clone` creates a
        // new native reference to the same identity-managed value; `drop`
        // releases one native reference and returns unit.
        "clone" | "drop" => vec![Ty::Unknown],
        "print" => vec![Ty::Unknown],
        "sum" => vec![Ty::List(Box::new(Ty::Unknown))],
        "read_file" | "parse_int" | "parse_csv" => vec![Ty::String],
        "write_file" => vec![Ty::String, Ty::String],
        "panic" => vec![Ty::String],
        "assert" => vec![Ty::Bool],
        "assert_eq" => vec![Ty::Unknown, Ty::Unknown],
        _ => return None,
    };
    if expected_args.len() != arg_types.len() {
        errors.push(TypeError {
            code: "E1041",
            message: format!(
                "Builtin '{}' expects {} argument(s), got {}.",
                name,
                expected_args.len(),
                arg_types.len()
            ),
            span: None,
            source_file: None,
        });
        return Some(Ty::Unknown);
    }
    for (index, (expected, actual)) in expected_args.iter().zip(arg_types.iter()).enumerate() {
        if !compatible(expected, actual) {
            errors.push(TypeError {
                code: "E1041",
                message: format!(
                    "Builtin '{}' argument #{} expects '{}', got '{}'.",
                    name,
                    index + 1,
                    expected.describe(),
                    actual.describe()
                ),
                span: None,
                source_file: None,
            });
        }
    }
    match name {
        "args" => Some(Ty::List(Box::new(Ty::String))),
        "yield" => Some(Ty::Void),
        "env" => Some(Ty::Applied("Option".to_string(), vec![Ty::String])),
        "path_join" => Some(Ty::String),
        "cwd" => Some(Ty::String),
        "file_exists" => Some(Ty::Bool),
        "hash" => {
            let supported = arg_types
                .first()
                .is_some_and(|ty| {
                    is_builtin_hashable(
                        ty,
                        record_derives,
                        record_fields,
                        enum_derives,
                        enum_variants,
                        variant_fields,
                    )
                });
            if !supported {
                errors.push(TypeError {
                    code: "E1041",
                    message: format!(
                        "Builtin 'hash' supports scalar values, hashable Option/Result/collection values, or records/enums with derive(Hash), got '{}'.",
                        arg_types[0].describe()
                    ),
                    span: None,
                    source_file: None,
                });
            }
            Some(Ty::Int)
        }
        "format" => Some(Ty::String),
        "select" => {
            let Some(Ty::List(channel_ty)) = arg_types.first() else {
                errors.push(TypeError {
                    code: "E1041",
                    message: "Builtin 'select' expects a List<Channel<T>>.".to_string(),
                    span: None,
                    source_file: None,
                });
                return Some(Ty::Unknown);
            };
            let Ty::Applied(channel_name, channel_args) = channel_ty.as_ref() else {
                errors.push(TypeError {
                    code: "E1041",
                    message: format!(
                        "Builtin 'select' expects a List<Channel<T>>, got List<{}>.",
                        channel_ty.describe()
                    ),
                    span: None,
                    source_file: None,
                });
                return Some(Ty::Unknown);
            };
            if channel_name != "Channel" || channel_args.len() != 1 {
                errors.push(TypeError {
                    code: "E1041",
                    message: format!(
                        "Builtin 'select' expects a List<Channel<T>>, got List<{}>.",
                        channel_ty.describe()
                    ),
                    span: None,
                    source_file: None,
                });
                return Some(Ty::Unknown);
            }
            Some(Ty::Applied("Option".to_string(), vec![channel_args[0].clone()]))
        }
        "clone" => Some(arg_types.first().cloned().unwrap_or(Ty::Unknown)),
        "drop" => Some(Ty::Void),
        "print" | "panic" | "assert" | "assert_eq" => Some(Ty::Void),
        "parse_csv" => Some(Ty::List(Box::new(Ty::List(Box::new(Ty::String))))),
        "read_file" | "write_file" => Some(Ty::Applied(
            "Result".to_string(),
            vec![
                if name == "read_file" { Ty::String } else { Ty::Void },
                Ty::String,
            ],
        )),
        "parse_int" => Some(Ty::Applied(
            "Result".to_string(),
            vec![Ty::Int, Ty::String],
        )),
        "sum" => match &arg_types[0] {
            Ty::List(element) => Some((**element).clone()),
            _ => Some(Ty::Unknown),
        },
        _ => None,
    }
}

fn collection_method_return_type(receiver_ty: &Ty, method: &str, arg_types: &[Ty]) -> Ty {
    match (receiver_ty, method) {
        (Ty::List(_), "length" | "count") => Ty::Int,
        (Ty::List(_), "map") => match arg_types.first() {
            Some(Ty::Fn(_, return_type)) => Ty::List(return_type.clone()),
            _ => Ty::List(Box::new(Ty::Unknown)),
        },
        (Ty::List(elem), "filter") => Ty::List(elem.clone()),
        (Ty::List(elem), "remove_at") => (**elem).clone(),
        (Ty::List(_), "push") => Ty::Void,
        (Ty::List(_), "join") => Ty::String,
        (Ty::List(_), "fold") => arg_types.first().cloned().unwrap_or(Ty::Unknown),
        (Ty::List(elem), "find") => Ty::Applied("Option".to_string(), vec![(**elem).clone()]),
        (Ty::List(_), "any" | "all") => Ty::Bool,
        (Ty::Map(_, _), "count") => Ty::Int,
        (Ty::Map(key, _), "keys") => Ty::List(Box::new((**key).clone())),
        (Ty::Map(_, value), "values") => Ty::List(Box::new((**value).clone())),
        (Ty::Map(_, value), "get" | "remove") => {
            Ty::Applied("Option".to_string(), vec![(**value).clone()])
        }
        (Ty::Map(_, _), "contains_key") => Ty::Bool,
        (Ty::Map(_, _), "set") => Ty::Void,
        (Ty::Set(_), "count") => Ty::Int,
        (Ty::Set(_), "contains") => Ty::Bool,
        (Ty::Set(_), "add" | "remove") => Ty::Void,
        _ => Ty::Unknown,
    }
}

fn pattern_literal_type(literal: &Expr) -> Ty {
    match literal.unlocated() {
        Expr::IntLiteral(_) => Ty::Int,
        Expr::FloatLiteral(_) => Ty::Float,
        Expr::Float32Literal(_) => Ty::Float32,
        Expr::StringLiteral(_) => Ty::String,
        Expr::CharLiteral(_) => Ty::Char,
        Expr::BoolLiteral(_) => Ty::Bool,
        Expr::UnitLiteral(number, unit) => {
            if matches!(number.unlocated(), Expr::IntLiteral(_) | Expr::FloatLiteral(_)) {
                resolve_unit_expr(unit).map(Ty::Quantity).unwrap_or(Ty::Unknown)
            } else {
                Ty::Unknown
            }
        }
        _ => Ty::Unknown,
    }
}

fn pattern_field_indices(field_names: &[Option<String>], fields: &[(String, Pattern)]) -> Vec<Option<usize>> {
    let all_named = field_names.iter().all(Option::is_some);
    let mut used = HashSet::new();
    fields
        .iter()
        .enumerate()
        .map(|(position, (label, _))| {
            let candidate = if let Some(index) = label.strip_prefix('@').and_then(|index| index.parse::<usize>().ok()) {
                Some(index)
            } else if let Some(index) = field_names
                .iter()
                .position(|name| name.as_deref() == Some(label.as_str()))
            {
                Some(index)
            } else if !all_named {
                Some(position)
            } else {
                None
            };
            candidate.filter(|index| *index < field_names.len() && used.insert(*index))
        })
        .collect()
}

fn resolve_dimension_with_subst(ty: &Type, subst: &HashMap<String, Dimension>) -> Dimension {
    match ty {
        Type::Named(name, _) => subst.get(name).cloned().unwrap_or_else(|| dim_single(name)),
        Type::Mul(a, b) => dim_mul(&resolve_dimension_with_subst(a, subst), &resolve_dimension_with_subst(b, subst)),
        Type::Div(a, b) => dim_div(&resolve_dimension_with_subst(a, subst), &resolve_dimension_with_subst(b, subst)),
        Type::Pow(a, n) => dim_pow(&resolve_dimension_with_subst(a, subst), *n as i32),
        _ => HashMap::new(),
    }
}

/// Variables referenciadas dentro de un bloque de 'spawn' que no se declaran
/// dentro de él mismo — necesario para el error E1100 (documento 09, §1.1):
/// capturar un binding 'mut' del scope que lanzó la tarea está prohibido.
/// Sobreaproxima deliberadamente el "bound" hacia adelante dentro de cada
/// sub-scope (no es un análisis de flujo completo), suficiente para esta
/// comprobación de seguridad.
fn free_vars_in_block(block: &Block) -> HashSet<String> {
    let mut free = HashSet::new();
    walk_block(block, &HashSet::new(), &mut free);
    free
}

fn walk_block(block: &Block, bound: &HashSet<String>, free: &mut HashSet<String>) {
    let mut local = bound.clone();
    for stmt in &block.stmts { walk_stmt(&stmt.stmt, &mut local, free); }
    if let Some(e) = &block.tail { walk_expr(e, &local, free); }
}

fn walk_stmt(stmt: &Stmt, bound: &mut HashSet<String>, free: &mut HashSet<String>) {
    match stmt {
        Stmt::Binding { name, value, .. } => { walk_expr(value, bound, free); bound.insert(name.clone()); }
        Stmt::Assign { name, value } => {
            walk_expr(value, bound, free);
            if !bound.contains(name) { free.insert(name.clone()); }
            bound.insert(name.clone());
        }
        Stmt::Return(Some(e)) | Stmt::Break(Some(e)) => walk_expr(e, bound, free),
        Stmt::Return(None) | Stmt::Break(None) | Stmt::Continue => {}
        Stmt::For { pattern, iter, body } => {
            walk_expr(iter, bound, free);
            let mut inner = bound.clone();
            inner.insert(pattern.clone());
            walk_block(body, &inner, free);
        }
        Stmt::While { cond, body } => { walk_expr(cond, bound, free); walk_block(body, bound, free); }
        Stmt::FieldAssign { target, value } => { walk_expr(target, bound, free); walk_expr(value, bound, free); }
        Stmt::Expr(e) => walk_expr(e, bound, free),
    }
}

fn pattern_binds(pattern: &Pattern, out: &mut HashSet<String>) {
    match pattern {
        Pattern::Ident(name) => { out.insert(name.clone()); }
        Pattern::Variant(_, fields) => { for (_, sub) in fields { pattern_binds(sub, out); } }
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::Range(..) => {}
    }
}

fn collect_supertraits(
    trait_name: &str,
    traits: &HashMap<String, TraitDecl>,
    visiting: &mut Vec<String>,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
) {
    if visiting.iter().any(|name| name == trait_name) || !seen.insert(trait_name.to_string()) {
        return;
    }
    visiting.push(trait_name.to_string());
    if let Some(trait_decl) = traits.get(trait_name) {
        for supertrait in &trait_decl.supertraits {
            if !seen.contains(supertrait) {
                out.push(supertrait.clone());
                if traits.contains_key(supertrait) {
                    collect_supertraits(supertrait, traits, visiting, seen, out);
                } else {
                    seen.insert(supertrait.clone());
                }
            }
        }
    }
    visiting.pop();
}

fn collect_trait_closure(
    trait_name: &str,
    traits: &HashMap<String, TraitDecl>,
    visiting: &mut Vec<String>,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
) -> bool {
    if visiting.iter().any(|name| name == trait_name) {
        return true;
    }
    if !seen.insert(trait_name.to_string()) {
        return false;
    }
    visiting.push(trait_name.to_string());
    out.push(trait_name.to_string());
    let mut has_cycle = false;
    if let Some(trait_decl) = traits.get(trait_name) {
        for supertrait in &trait_decl.supertraits {
            if traits.contains_key(supertrait) {
                has_cycle |= collect_trait_closure(supertrait, traits, visiting, seen, out);
            }
        }
    }
    visiting.pop();
    has_cycle
}

fn trait_method_signature_matches(expected: &TraitMethodSig, actual: &TraitMethodSig) -> bool {
    expected.generics == actual.generics
        && expected.params.len() == actual.params.len()
        && expected.params.iter().zip(&actual.params).all(|(left, right)| {
            left.name == right.name
                && left.is_mut == right.is_mut
                && left.ty == right.ty
        })
        && expected.return_type == actual.return_type
}

fn specialize_trait_method(
    method: &TraitMethodSig,
    trait_generics: &[GenericParam],
    trait_args: &[Type],
) -> TraitMethodSig {
    let substitutions: HashMap<String, Type> = trait_generics
        .iter()
        .zip(trait_args.iter())
        .map(|(generic, argument)| (generic.name.clone(), argument.clone()))
        .collect();
    let mut specialized = method.clone();
    for param in &mut specialized.params {
        replace_type_parameters(&mut param.ty, &substitutions);
    }
    replace_type_parameters(&mut specialized.return_type, &substitutions);
    specialized
}

fn replace_type_parameters(ty: &mut Type, substitutions: &HashMap<String, Type>) {
    match ty {
        Type::Named(name, args) if args.is_empty() => {
            if let Some(replacement) = substitutions.get(name) {
                *ty = replacement.clone();
            }
        }
        Type::Named(_, args) => {
            for arg in args {
                replace_type_parameters(arg, substitutions);
            }
        }
        Type::Mul(left, right) | Type::Div(left, right) => {
            replace_type_parameters(left, substitutions);
            replace_type_parameters(right, substitutions);
        }
        Type::Pow(base, _) => replace_type_parameters(base, substitutions),
        Type::Fn(params, return_type) => {
            for param in params {
                replace_type_parameters(param, substitutions);
            }
            replace_type_parameters(return_type, substitutions);
        }
        Type::Dyn(_) => {}
    }
}

fn is_builtin_trait(name: &str) -> bool {
    matches!(name, "Add" | "Sub" | "Mul" | "Div" | "Eq" | "Ord" | "Iterator" | "Printable" | "Default" | "Hash" | "Drop")
}

fn builtin_generic_type_arity(name: &str) -> Option<usize> {
    match name {
        "Quantity" | "List" | "Set" => Some(1),
        "Map" => Some(2),
        _ => None,
    }
}

fn implementation_type_substitutions(
    receiver_ty: &Ty,
    implementation: &ImplDecl,
) -> Option<HashMap<String, Ty>> {
    let Some((receiver_name, receiver_args)) = type_parts_for_impl_matching(receiver_ty) else {
        return None;
    };
    if receiver_name != implementation.type_name || receiver_args.len() != implementation.type_args.len() {
        return None;
    }
    let generic_names: HashSet<String> = implementation
        .generics
        .iter()
        .map(|generic| generic.name.clone())
        .collect();
    let mut substitutions = HashMap::new();
    let matches = implementation
        .type_args
        .iter()
        .zip(receiver_args.iter())
        .all(|(pattern, actual)| impl_type_pattern_matches(pattern, actual, &generic_names, &mut substitutions));
    matches.then_some(substitutions)
}

fn substitute_impl_type_parameters(ty: &mut Type, substitutions: &HashMap<String, Ty>) {
    let type_substitutions: HashMap<String, Type> = substitutions
        .iter()
        .map(|(name, ty)| (name.clone(), type_from_ty(ty)))
        .collect();
    replace_type_parameters(ty, &type_substitutions);
}

fn type_from_ty(ty: &Ty) -> Type {
    match ty {
        Ty::Int => Type::Named("Int".to_string(), Vec::new()),
        Ty::Float => Type::Named("Float".to_string(), Vec::new()),
        Ty::Bool => Type::Named("Bool".to_string(), Vec::new()),
        Ty::Char => Type::Named("Char".to_string(), Vec::new()),
        Ty::String => Type::Named("String".to_string(), Vec::new()),
        Ty::Void => Type::Named("Void".to_string(), Vec::new()),
        Ty::Quantity(dimension) => Type::Named(
            "Quantity".to_string(),
            vec![Type::Named(dim_to_string(dimension), Vec::new())],
        ),
        Ty::List(element) => Type::Named("List".to_string(), vec![type_from_ty(element)]),
        Ty::Map(key, value) => Type::Named(
            "Map".to_string(),
            vec![type_from_ty(key), type_from_ty(value)],
        ),
        Ty::Set(element) => Type::Named("Set".to_string(), vec![type_from_ty(element)]),
        Ty::Named(name) => Type::Named(name.clone(), Vec::new()),
        Ty::Applied(name, args) => Type::Named(
            name.clone(),
            args.iter().map(type_from_ty).collect(),
        ),
        Ty::Generic(name) => Type::Named(name.clone(), Vec::new()),
        Ty::Dyn(name) => Type::Dyn(vec![name.clone()]),
        Ty::Sized(kind) => Type::Named(kind.name().to_string(), Vec::new()),
        Ty::Float32 => Type::Named("Float32".to_string(), Vec::new()),
        Ty::Fn(params, return_type) => Type::Fn(
            params.iter().map(type_from_ty).collect(),
            Box::new(type_from_ty(return_type)),
        ),
        Ty::Unknown => Type::Named("Unknown".to_string(), Vec::new()),
    }
}

fn type_parts_for_impl_matching(ty: &Ty) -> Option<(String, Vec<Ty>)> {
    match ty {
        Ty::Named(name) => Some((name.clone(), Vec::new())),
        Ty::Applied(name, args) => Some((name.clone(), args.clone())),
        Ty::Quantity(dimension) => Some((
            "Quantity".to_string(),
            vec![Ty::Named(dim_to_string(dimension))],
        )),
        Ty::List(element) => Some(("List".to_string(), vec![*element.clone()])),
        Ty::Map(key, value) => Some(("Map".to_string(), vec![*key.clone(), *value.clone()])),
        Ty::Set(element) => Some(("Set".to_string(), vec![*element.clone()])),
        _ => None,
    }
}

fn impl_type_pattern_matches(
    pattern: &Type,
    actual: &Ty,
    generic_names: &HashSet<String>,
    substitutions: &mut HashMap<String, Ty>,
) -> bool {
    if *actual == Ty::Unknown {
        return true;
    }
    match pattern {
        Type::Named(name, args) if args.is_empty() && generic_names.contains(name) => {
            match substitutions.get(name) {
                Some(previous) => compatible(previous, actual),
                None => {
                    substitutions.insert(name.clone(), actual.clone());
                    true
                }
            }
        }
        Type::Named(name, args) if name == "List" && args.len() == 1 => {
            let Ty::List(element) = actual else { return false };
            impl_type_pattern_matches(&args[0], element, generic_names, substitutions)
        }
        Type::Named(name, args) if name == "Map" && args.len() == 2 => {
            let Ty::Map(key, value) = actual else { return false };
            impl_type_pattern_matches(&args[0], key, generic_names, substitutions)
                && impl_type_pattern_matches(&args[1], value, generic_names, substitutions)
        }
        Type::Named(name, args) if name == "Set" && args.len() == 1 => {
            let Ty::Set(element) = actual else { return false };
            impl_type_pattern_matches(&args[0], element, generic_names, substitutions)
        }
        Type::Named(name, args) if name == "Quantity" && args.len() == 1 => {
            let Ty::Quantity(dimension) = actual else { return false };
            let actual_dimension = Ty::Named(dim_to_string(dimension));
            impl_type_pattern_matches(&args[0], &actual_dimension, generic_names, substitutions)
        }
        Type::Named(name, args) if !args.is_empty() => {
            let Ty::Applied(actual_name, actual_args) = actual else { return false };
            name == actual_name
                && args.len() == actual_args.len()
                && args.iter().zip(actual_args.iter()).all(|(pattern, actual)| {
                    impl_type_pattern_matches(pattern, actual, generic_names, substitutions)
                })
        }
        Type::Named(name, args) if args.is_empty() => {
            compatible(&resolve_type(pattern), actual)
        }
        Type::Fn(params, return_type) => {
            let Ty::Fn(actual_params, actual_return) = actual else { return false };
            params.len() == actual_params.len()
                && params.iter().zip(actual_params.iter()).all(|(pattern, actual)| {
                    impl_type_pattern_matches(pattern, actual, generic_names, substitutions)
                })
                && impl_type_pattern_matches(return_type, actual_return, generic_names, substitutions)
        }
        _ => false,
    }
}

fn impl_method_signature_matches(expected: &TraitMethodSig, actual: &FunctionDecl, owner: &str) -> bool {
    if expected.generics != actual.generics || expected.params.len() != actual.params.len() {
        return false;
    }
    for (expected_param, actual_param) in expected.params.iter().zip(&actual.params) {
        if expected_param.name != actual_param.name || expected_param.is_mut != actual_param.is_mut {
            return false;
        }
        if !signature_type_matches(&expected_param.ty, &actual_param.ty, owner) {
            return false;
        }
    }
    signature_type_matches(&expected.return_type, &actual.return_type, owner)
}

fn signature_type_matches(expected: &Type, actual: &Type, owner: &str) -> bool {
    let mut expected = expected.clone();
    let mut actual = actual.clone();
    replace_self_type(&mut expected, owner);
    replace_self_type(&mut actual, owner);
    expected == actual
}

fn walk_expr(expr: &Expr, bound: &HashSet<String>, free: &mut HashSet<String>) {
    match expr {
        Expr::Located(inner, _) => walk_expr(inner, bound, free),
        Expr::Ident(name) => { if !bound.contains(name) { free.insert(name.clone()); } }
        Expr::Lambda(params, body) => {
            let mut inner = bound.clone();
            for p in params { inner.insert(p.clone()); }
            walk_block(body, &inner, free);
        }
        Expr::Spawn(b) | Expr::SpawnScope(b) | Expr::Loop(b) | Expr::Block(b) => walk_block(b, bound, free),
        Expr::If(c, t, e) => {
            walk_expr(c, bound, free);
            walk_block(t, bound, free);
            if let Some(e) = e { walk_block(e, bound, free); }
        }
        Expr::Match(s, arms) => {
            walk_expr(s, bound, free);
            for arm in arms {
                let mut inner = bound.clone();
                pattern_binds(&arm.pattern, &mut inner);
                if let Some(g) = &arm.guard { walk_expr(g, &inner, free); }
                walk_block(&arm.body, &inner, free);
            }
        }
        Expr::Binary(_, l, r) | Expr::Within(l, r) => { walk_expr(l, bound, free); walk_expr(r, bound, free); }
        Expr::Unary(_, e) | Expr::As(e, _) | Expr::UnitLiteral(e, _) => walk_expr(e, bound, free),
        Expr::Range(s, _, e, step) => {
            walk_expr(s, bound, free);
            walk_expr(e, bound, free);
            if let Some(st) = step { walk_expr(st, bound, free); }
        }
        Expr::Call(callee, args) => {
            walk_expr(callee, bound, free);
            for a in args {
                match a { Arg::Positional(e) | Arg::Named(_, e) => walk_expr(e, bound, free) }
            }
        }
        Expr::GenericCall(callee, _, args) => {
            walk_expr(callee, bound, free);
            for a in args {
                match a { Arg::Positional(e) | Arg::Named(_, e) => walk_expr(e, bound, free) }
            }
        }
        Expr::FieldAccess(o, _) => walk_expr(o, bound, free),
        Expr::Index(o, i) => { walk_expr(o, bound, free); walk_expr(i, bound, free); }
        Expr::ListLiteral(items) | Expr::SetLiteral(items) => { for it in items { walk_expr(it, bound, free); } }
        Expr::EmptyCollection(..) => {}
        Expr::MapLiteral(pairs) => { for (k, v) in pairs { walk_expr(k, bound, free); walk_expr(v, bound, free); } }
        Expr::Try(i, c) => { walk_expr(i, bound, free); if let Some(c) = c { walk_expr(c, bound, free); } }
        Expr::Approximately(a, b, t) => { walk_expr(a, bound, free); walk_expr(b, bound, free); walk_expr(t, bound, free); }
        Expr::RecordLiteral(_, fields) => { for (_, v) in fields { walk_expr(v, bound, free); } }
        Expr::GenericRecordLiteral(_, _, fields) => { for (_, v) in fields { walk_expr(v, bound, free); } }
        Expr::Channel(_, cap) => { if let Some(c) = cap { walk_expr(c, bound, free); } }
        Expr::IntLiteral(_) | Expr::SizedIntLiteral(..) | Expr::FloatLiteral(_) | Expr::Float32Literal(_) | Expr::StringLiteral(_) | Expr::CharLiteral(_) | Expr::BoolLiteral(_) => {}
    }
}

/// The argument expression bound to parameter `index` (positional by
/// position, named by name), mirroring `bind_function_arguments`.
fn arg_expr_for_param<'a>(sig: &FnSig, args: &'a [Arg], index: usize) -> Option<&'a Expr> {
    let name = &sig.params.get(index)?.name;
    let mut positional = 0usize;
    for arg in args {
        match arg {
            Arg::Positional(expr) => {
                if positional == index {
                    return Some(expr);
                }
                positional += 1;
            }
            Arg::Named(n, expr) if n == name => return Some(expr),
            Arg::Named(..) => {}
        }
    }
    None
}

/// `Array<T>` gives `T`.
fn array_elem(ty: &Ty) -> Option<Ty> {
    match ty {
        Ty::Applied(name, args) if name == "Array" && args.len() == 1 => Some(args[0].clone()),
        _ => None,
    }
}

fn is_array_scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_) | Ty::Bool)
}

/// Merge a partially inferred expression with a contextual type. Generic
/// constructors such as `Ok(value)` often know their success payload before
/// their surrounding `Result` supplies the error type; preserving the known
/// side while filling only `Unknown` slots keeps callbacks fully typed.
fn refine_expected_type(actual: &Ty, expected: &Ty) -> Ty {
    if !compatible(expected, actual) {
        return actual.clone();
    }
    match (actual, expected) {
        (Ty::Unknown, expected) => expected.clone(),
        (Ty::List(actual), Ty::List(expected)) => Ty::List(Box::new(refine_expected_type(actual, expected))),
        (Ty::Set(actual), Ty::Set(expected)) => Ty::Set(Box::new(refine_expected_type(actual, expected))),
        (Ty::Map(actual_key, actual_value), Ty::Map(expected_key, expected_value)) => Ty::Map(
            Box::new(refine_expected_type(actual_key, expected_key)),
            Box::new(refine_expected_type(actual_value, expected_value)),
        ),
        (Ty::Applied(actual_name, actual_args), Ty::Applied(expected_name, expected_args))
            if actual_name == expected_name && actual_args.len() == expected_args.len() => Ty::Applied(
                actual_name.clone(),
                actual_args
                    .iter()
                    .zip(expected_args)
                    .map(|(actual, expected)| refine_expected_type(actual, expected))
                    .collect(),
            ),
        (Ty::Fn(actual_params, actual_return), Ty::Fn(expected_params, expected_return))
            if actual_params.len() == expected_params.len() => Ty::Fn(
                actual_params
                    .iter()
                    .zip(expected_params)
                    .map(|(actual, expected)| refine_expected_type(actual, expected))
                    .collect(),
                Box::new(refine_expected_type(actual_return, expected_return)),
            ),
        _ => actual.clone(),
    }
}

fn compatible(expected: &Ty, actual: &Ty) -> bool {
    if expected == &Ty::Unknown || actual == &Ty::Unknown {
        return true;
    }
    match (expected, actual) {
        // A concrete type coerces to `dyn Trait` (whether it implements the
        // trait is checked where the value is boxed, not here).
        (Ty::Dyn(_), _) | (_, Ty::Dyn(_)) => true,
        (Ty::List(expected), Ty::List(actual)) | (Ty::Set(expected), Ty::Set(actual)) => {
            compatible(expected, actual)
        }
        (Ty::Map(expected_key, expected_value), Ty::Map(actual_key, actual_value)) => {
            compatible(expected_key, actual_key) && compatible(expected_value, actual_value)
        }
        (Ty::Applied(expected_name, expected_args), Ty::Applied(actual_name, actual_args)) => {
            expected_name == actual_name
                && expected_args.len() == actual_args.len()
                && expected_args
                    .iter()
                    .zip(actual_args)
                    .all(|(expected, actual)| compatible(expected, actual))
        }
        (Ty::Named(expected_name), Ty::Applied(actual_name, _))
        | (Ty::Applied(actual_name, _), Ty::Named(expected_name)) => expected_name == actual_name,
        (Ty::Fn(expected_params, expected_ret), Ty::Fn(actual_params, actual_ret)) => {
            expected_params.len() == actual_params.len()
                && expected_params
                    .iter()
                    .zip(actual_params)
                    .all(|(expected, actual)| compatible(expected, actual))
                && compatible(expected_ret, actual_ret)
        }
        _ => expected == actual,
    }
}

fn replace_self_type(ty: &mut Type, owner: &str) {
    match ty {
        Type::Named(name, args) => {
            if name == "Self" && args.is_empty() {
                *name = owner.to_string();
            } else {
                for arg in args {
                    replace_self_type(arg, owner);
                }
            }
        }
        Type::Mul(a, b) | Type::Div(a, b) => {
            replace_self_type(a, owner);
            replace_self_type(b, owner);
        }
        Type::Pow(a, _) => replace_self_type(a, owner),
        Type::Fn(params, ret) => {
            for param in params {
                replace_self_type(param, owner);
            }
            replace_self_type(ret, owner);
        }
        Type::Dyn(_) => {}
    }
}

fn replace_self_type_with_type(ty: &mut Type, owner: &Type) {
    match ty {
        Type::Named(name, args) if name == "Self" && args.is_empty() => {
            *ty = owner.clone();
        }
        Type::Named(_, args) => {
            for arg in args {
                replace_self_type_with_type(arg, owner);
            }
        }
        Type::Mul(left, right) | Type::Div(left, right) => {
            replace_self_type_with_type(left, owner);
            replace_self_type_with_type(right, owner);
        }
        Type::Pow(base, _) => replace_self_type_with_type(base, owner),
        Type::Fn(params, return_type) => {
            for param in params {
                replace_self_type_with_type(param, owner);
            }
            replace_self_type_with_type(return_type, owner);
        }
        Type::Dyn(_) => {}
    }
}

/// Methods of `String` (kept in step with `interpreter/strings.rs`).
const STRING_METHOD_NAMES: &[&str] = &[
    "length", "is_empty", "trim", "to_upper", "to_lower", "contains", "starts_with", "ends_with", "replace", "split", "lines", "to_int", "to_float",
];
