use crate::ast::*;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub kind: &'static str,
    pub name: String,
    pub detail: String,
    pub span: Span,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MemberSymbol {
    pub owner: String,
    pub kind: &'static str,
    pub name: String,
    pub detail: String,
}

pub fn collect(items: &[Item]) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    for item in items {
        match item {
            Item::Function(function) => symbols.push(function_symbol(function, "function", &function.name)),
            Item::Record(record) => {
                symbols.push(Symbol {
                    kind: "record",
                    name: record.name.clone(),
                    detail: format!("record {}{}", record.name, generic_suffix(&record.generics)),
                    span: record.span,
                    source_file: record.source_file.clone(),
                });
                for field in &record.fields {
                    symbols.push(Symbol {
                        kind: "field",
                        name: format!("{}.{}", record.name, field.name),
                        detail: format!("{}: {}", field.name, type_to_string(&field.ty)),
                        span: record.span,
                        source_file: record.source_file.clone(),
                    });
                }
            }
            Item::Enum(enum_decl) => {
                symbols.push(Symbol {
                    kind: "enum",
                    name: enum_decl.name.clone(),
                    detail: format!("enum {}{}", enum_decl.name, generic_suffix(&enum_decl.generics)),
                    span: enum_decl.span,
                    source_file: enum_decl.source_file.clone(),
                });
                for variant in &enum_decl.variants {
                    symbols.push(Symbol {
                        kind: "enumMember",
                        name: variant.name.clone(),
                        detail: format!("{}::{}", enum_decl.name, variant.name),
                        span: enum_decl.span,
                        source_file: enum_decl.source_file.clone(),
                    });
                }
            }
            Item::Trait(trait_decl) => {
                symbols.push(Symbol {
                    kind: "trait",
                    name: trait_decl.name.clone(),
                    detail: format!("trait {}{}", trait_decl.name, generic_suffix(&trait_decl.generics)),
                    span: trait_decl.span,
                    source_file: trait_decl.source_file.clone(),
                });
                for method in &trait_decl.methods {
                    symbols.push(Symbol {
                        kind: "method",
                        name: format!("{}.{}", trait_decl.name, method.name),
                        detail: method_signature(&method.name, &method.generics, &method.params, &method.return_type),
                        span: trait_decl.span,
                        source_file: trait_decl.source_file.clone(),
                    });
                }
            }
            Item::Impl(implementation) => {
                let target = if let Some(trait_name) = &implementation.trait_name {
                    format!("impl {} for {}", trait_name, implementation.type_name)
                } else {
                    format!("impl {}", implementation.type_name)
                };
                symbols.push(Symbol {
                    kind: "implementation",
                    name: implementation.type_name.clone(),
                    detail: target,
                    span: implementation.span,
                    source_file: implementation.source_file.clone(),
                });
                for method in &implementation.methods {
                    symbols.push(function_symbol(
                        method,
                        "method",
                        &format!("{}.{}", implementation.type_name, method.name),
                    ));
                }
            }
            Item::Import(_) => {}
        }
    }
    symbols
}

/// Collects members that editor tooling can offer after a receiver expression.
/// User declarations are combined with the stable members exposed by the core
/// collection/result types so a language client does not need to duplicate the
/// language surface in JavaScript or another host language.
pub fn collect_members(items: &[Item]) -> Vec<MemberSymbol> {
    let mut members = core_members();
    for item in items {
        match item {
            Item::Record(record) => {
                for field in &record.fields {
                    members.push(MemberSymbol {
                        owner: record.name.clone(),
                        kind: "field",
                        name: field.name.clone(),
                        detail: format!("{}: {}", field.name, type_to_string(&field.ty)),
                    });
                }
            }
            Item::Enum(enum_decl) => {
                for variant in &enum_decl.variants {
                    members.push(MemberSymbol {
                        owner: enum_decl.name.clone(),
                        kind: "enumMember",
                        name: variant.name.clone(),
                        detail: format!("{}::{}", enum_decl.name, variant.name),
                    });
                }
            }
            Item::Trait(trait_decl) => {
                for method in &trait_decl.methods {
                    members.push(MemberSymbol {
                        owner: trait_decl.name.clone(),
                        kind: "method",
                        name: method.name.clone(),
                        detail: method_signature(
                            &method.name,
                            &method.generics,
                            &method.params,
                            &method.return_type,
                        ),
                    });
                }
            }
            Item::Impl(implementation) => {
                for method in &implementation.methods {
                    members.push(MemberSymbol {
                        owner: implementation.type_name.clone(),
                        kind: "method",
                        name: method.name.clone(),
                        detail: method_signature(
                            &method.name,
                            &method.generics,
                            &method.params,
                            &method.return_type,
                        ),
                    });
                }
            }
            Item::Function(_) | Item::Import(_) => {}
        }
    }
    members
}

fn core_members() -> Vec<MemberSymbol> {
    let definitions: &[(&str, &str, &str)] = &[
        ("List", "length", "length() -> Int"),
        ("List", "count", "count() -> Int"),
        ("List", "push", "push(value: T) -> Void"),
        ("List", "remove_at", "remove_at(index: Int) -> T"),
        ("List", "map", "map(transform: fn(T) -> U) -> List<U>"),
        ("List", "filter", "filter(predicate: fn(T) -> Bool) -> List<T>"),
        ("List", "fold", "fold(initial: U, combine: fn(U, T) -> U) -> U"),
        ("List", "find", "find(predicate: fn(T) -> Bool) -> Option<T>"),
        ("List", "any", "any(predicate: fn(T) -> Bool) -> Bool"),
        ("List", "all", "all(predicate: fn(T) -> Bool) -> Bool"),
        ("Map", "count", "count() -> Int"),
        ("Map", "keys", "keys() -> List<K>"),
        ("Map", "values", "values() -> List<V>"),
        ("Map", "get", "get(key: K) -> Option<V>"),
        ("Map", "remove", "remove(key: K) -> Option<V>"),
        ("Map", "contains_key", "contains_key(key: K) -> Bool"),
        ("Map", "set", "set(key: K, value: V) -> Void"),
        ("Set", "count", "count() -> Int"),
        ("Set", "contains", "contains(value: T) -> Bool"),
        ("Set", "add", "add(value: T) -> Void"),
        ("Set", "remove", "remove(value: T) -> Void"),
        ("Option", "is_some", "is_some() -> Bool"),
        ("Option", "is_none", "is_none() -> Bool"),
        ("Option", "unwrap", "unwrap() -> T"),
        ("Option", "unwrap_or", "unwrap_or(default: T) -> T"),
        ("Option", "ok_or", "ok_or(error: E) -> Result<T, E>"),
        ("Option", "map", "map(transform: fn(T) -> U) -> Option<U>"),
        ("Option", "then", "then(transform: fn(T) -> Option<U>) -> Option<U>"),
        ("Result", "is_ok", "is_ok() -> Bool"),
        ("Result", "is_err", "is_err() -> Bool"),
        ("Result", "unwrap", "unwrap() -> T"),
        ("Result", "unwrap_or", "unwrap_or(default: T) -> T"),
        ("Result", "ok", "ok() -> Option<T>"),
        ("Result", "map", "map(transform: fn(T) -> U) -> Result<U, E>"),
        ("Result", "map_err", "map_err(transform: fn(E) -> F) -> Result<T, F>"),
        ("Result", "then", "then(transform: fn(T) -> Result<U, E>) -> Result<U, E>"),
        ("Task", "join", "join() -> T"),
        ("Channel", "send", "send(value: T) -> Void"),
        ("Channel", "receive", "receive() -> Option<T>"),
        ("Channel", "close", "close() -> Void"),
    ];
    definitions
        .iter()
        .map(|(owner, name, detail)| MemberSymbol {
            owner: (*owner).to_string(),
            kind: "method",
            name: (*name).to_string(),
            detail: (*detail).to_string(),
        })
        .collect()
}

fn function_symbol(function: &FunctionDecl, kind: &'static str, name: &str) -> Symbol {
    Symbol {
        kind,
        name: name.to_string(),
        detail: method_signature(&function.name, &function.generics, &function.params, &function.return_type),
        span: function.span,
        source_file: function.source_file.clone(),
    }
}

fn method_signature(name: &str, generics: &[GenericParam], params: &[Param], return_type: &Type) -> String {
    let parameters = params
        .iter()
        .map(|param| {
            let mut text = String::new();
            if param.is_mut { text.push_str("mut "); }
            text.push_str(&param.name);
            text.push_str(": ");
            text.push_str(&type_to_string(&param.ty));
            if param.default.is_some() { text.push_str(" = …"); }
            text
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("fn {}{}({}) -> {}", name, generic_suffix(generics), parameters, type_to_string(return_type))
}

fn generic_suffix(generics: &[GenericParam]) -> String {
    if generics.is_empty() { return String::new(); }
    let params = generics
        .iter()
        .map(|generic| {
            if generic.bounds.is_empty() {
                generic.name.clone()
            } else {
                format!("{}: {}", generic.name, generic.bounds.join(" + "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{}>", params)
}

pub fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::Named(name, args) if args.is_empty() => name.clone(),
        Type::Named(name, args) => format!("{}<{}>", name, args.iter().map(type_to_string).collect::<Vec<_>>().join(", ")),
        Type::Mul(left, right) => format!("{} * {}", type_to_string(left), type_to_string(right)),
        Type::Div(left, right) => format!("{} / {}", type_to_string(left), type_to_string(right)),
        Type::Pow(base, exponent) => format!("{}^{}", type_to_string(base), exponent),
        Type::Fn(params, result) => format!(
            "fn({}) -> {}",
            params.iter().map(type_to_string).collect::<Vec<_>>().join(", "),
            type_to_string(result)
        ),
        Type::Dyn(traits) => format!("dyn {}", traits.join(" + ")),
    }
}
