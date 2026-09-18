use crate::ast::*;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub kind: &'static str,
    pub name: String,
    pub detail: String,
    pub span: Span,
    pub source_file: Option<String>,
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
