use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::ast::*;
use crate::lexer::Lexer;
use crate::parser::Parser;

struct Module {
    items: Vec<Item>,
    exported: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct ModuleDiagnostic {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub col: Option<usize>,
}

impl ModuleDiagnostic {
    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            file: None,
            line: None,
            col: None,
        }
    }

    fn at(file: &Path, line: usize, col: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            file: Some(file.display().to_string()),
            line: Some(line),
            col: Some(col),
        }
    }
}

impl std::fmt::Display for ModuleDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub fn load_project_with_deps(
    entry_path: &Path,
    deps: &HashMap<String, PathBuf>,
) -> Result<Vec<Item>, Vec<ModuleDiagnostic>> {
    load_project(entry_path, deps, &HashMap::new())
}

/// Igual que `load_project_with_deps`, pero permite sustituir el contenido en
/// disco de ciertos archivos por texto en memoria (`overrides`, indexado por
/// ruta canónica). Lo usa el servidor LSP para resolver imports y workspace
/// completo usando el buffer sin guardar del editor en vez del último archivo
/// escrito a disco.
pub fn load_project(
    entry_path: &Path,
    deps: &HashMap<String, PathBuf>,
    overrides: &HashMap<PathBuf, String>,
) -> Result<Vec<Item>, Vec<ModuleDiagnostic>> {
    // Units declared by a previous compilation (the LSP reuses the process) don't carry over.
    crate::types::reset_user_units();
    let root = entry_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let mut cache: HashMap<Vec<String>, Module> = HashMap::new();
    let mut in_progress: Vec<Vec<String>> = Vec::new();
    let mut errors: Vec<ModuleDiagnostic> = Vec::new();
    if let Err(e) = load_module_file(
        entry_path,
        &[],
        &root,
        deps,
        overrides,
        &mut cache,
        &mut in_progress,
        &mut errors,
    ) {
        errors.push(e);
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut merged = Vec::new();
    let paths: Vec<Vec<String>> = cache.keys().cloned().collect();
    for module_path in paths {
        let (resolve_map, alias_map) = build_resolution_maps(&module_path, &cache)
            .map_err(|e| vec![ModuleDiagnostic::message(e)])?;
        let module = cache.get(&module_path).unwrap();
        let ctx = RewriteCtx {
            resolve_map: &resolve_map,
            alias_map: &alias_map,
            cache: &cache,
        };
        for item in &module.items {
            let mut item = item.clone();
            rewrite_item(&mut item, &module_path, &ctx)
                .map_err(|e| vec![ModuleDiagnostic::message(e)])?;
            if !matches!(item, Item::Import(_)) {
                merged.push(item);
            }
        }
    }
    Ok(merged)
}

/// Si el primer segmento de la ruta del módulo coincide con el alias local
/// de una dependencia declarada en 'ostrin.toml' (documento 14, §4), el resto
/// de la ruta se resuelve dentro de la raíz de esa dependencia en vez de la
/// del propio proyecto — así 'import local_utils.helpers' funciona igual de
/// transparente que importar un módulo propio.
fn file_path_of(module_path: &[String], root: &Path, deps: &HashMap<String, PathBuf>) -> PathBuf {
    if let Some((first, rest)) = module_path.split_first() {
        if first == "std" && !deps.contains_key("std") {
            let mut p = PathBuf::from(STD_ROOT);
            for segment in rest {
                p.push(segment);
            }
            p.set_extension("ostrin");
            return p;
        }
        if let Some(dep_root) = deps.get(first) {
            let mut p = dep_root.clone();
            if rest.is_empty() {
                p.push("mod");
            } else {
                for segment in rest {
                    p.push(segment);
                }
            }
            p.set_extension("ostrin");
            return p;
        }
    }
    let mut p = root.to_path_buf();
    for segment in module_path {
        p.push(segment);
    }
    p.set_extension("ostrin");
    p
}

/// Virtual directory of the standard library. Its modules are written in Ostrin, embedded in the
/// compiler and imported like any other module (`import std.math`).
const STD_ROOT: &str = "<ostrin-std>";

const STD_MODULES: &[(&str, &str)] = &[
    ("math", include_str!("../std/math.ostrin")),
    ("lists", include_str!("../std/lists.ostrin")),
    ("strings", include_str!("../std/strings.ostrin")),
    ("time", include_str!("../std/time.ostrin")),
    ("json", include_str!("../std/json.ostrin")),
    ("args", include_str!("../std/args.ostrin")),
    ("env", include_str!("../std/env.ostrin")),
    ("maps", include_str!("../std/maps.ostrin")),
    ("viz", include_str!("../std/viz.ostrin")),
    ("numeric", include_str!("../std/numeric.ostrin")),
];

fn std_module_source(file_path: &Path) -> Option<io::Result<String>> {
    if !file_path.starts_with(STD_ROOT) {
        return None;
    }
    let name = file_path.file_stem()?.to_str()?;
    Some(
        match STD_MODULES.iter().find(|(module, _)| *module == name) {
            Some((_, source)) => Ok((*source).to_string()),
            None => {
                let available: Vec<&str> = STD_MODULES.iter().map(|(module, _)| *module).collect();
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "the standard library has no module '{name}' (available: {})",
                        available.join(", ")
                    ),
                ))
            }
        },
    )
}

fn read_module_source(
    file_path: &Path,
    overrides: &HashMap<PathBuf, String>,
) -> io::Result<String> {
    if let Some(source) = std_module_source(file_path) {
        return source;
    }
    let canonical = fs::canonicalize(file_path).unwrap_or_else(|_| file_path.to_path_buf());
    if let Some(text) = overrides.get(&canonical) {
        return Ok(text.clone());
    }
    fs::read_to_string(file_path)
}

fn load_module_file(
    file_path: &Path,
    module_path: &[String],
    root: &Path,
    deps: &HashMap<String, PathBuf>,
    overrides: &HashMap<PathBuf, String>,
    cache: &mut HashMap<Vec<String>, Module>,
    in_progress: &mut Vec<Vec<String>>,
    errors: &mut Vec<ModuleDiagnostic>,
) -> Result<(), ModuleDiagnostic> {
    if cache.contains_key(module_path) {
        return Ok(());
    }
    if in_progress.iter().any(|p| p == module_path) {
        let label = |p: &[String]| {
            if p.is_empty() {
                "(entry)".to_string()
            } else {
                p.join(".")
            }
        };
        let mut chain: Vec<String> = in_progress.iter().map(|p| label(p)).collect();
        chain.push(label(module_path));
        return Err(ModuleDiagnostic::message(format!(
            "Error OSTRIN-E1081\nCircular import detected:\n    {}",
            chain.join(" → ")
        )));
    }
    in_progress.push(module_path.to_vec());

    let source = read_module_source(file_path, overrides).map_err(|e| {
        ModuleDiagnostic::at(
            file_path,
            1,
            1,
            format!(
                "could not read module '{}' ({}): {e}",
                module_path.join("."),
                file_path.display()
            ),
        )
    })?;
    let tokens = Lexer::new(&source).tokenize().map_err(|e| {
        ModuleDiagnostic::at(
            file_path,
            e.line,
            e.col,
            format!("OSTRIN-E0002: lex error: {}", e.message),
        )
    })?;
    let (items, parse_errors) = Parser::new(tokens).parse_program();
    for e in parse_errors {
        errors.push(ModuleDiagnostic::at(
            file_path,
            e.line,
            e.col,
            format!("OSTRIN-E0001: parse error: {}", e.message),
        ));
    }

    for item in &items {
        if let Item::Import(imp) = item {
            if !cache.contains_key(&imp.path) {
                let dep_file = file_path_of(&imp.path, root, deps);
                load_module_file(
                    &dep_file,
                    &imp.path,
                    root,
                    deps,
                    overrides,
                    cache,
                    in_progress,
                    errors,
                )?;
            }
        }
    }

    in_progress.pop();
    let mut items = items;
    annotate_source_files(&mut items, file_path);
    let exported = compute_exports(&items);
    cache.insert(module_path.to_vec(), Module { items, exported });
    Ok(())
}

fn annotate_source_files(items: &mut [Item], file_path: &Path) {
    let source_file = Some(file_path.display().to_string());
    for item in items {
        match item {
            Item::Function(function) => function.source_file = source_file.clone(),
            Item::Record(record) => record.source_file = source_file.clone(),
            Item::Enum(enum_decl) => enum_decl.source_file = source_file.clone(),
            Item::Impl(implementation) => {
                implementation.source_file = source_file.clone();
                for method in &mut implementation.methods {
                    method.source_file = source_file.clone();
                }
            }
            Item::Trait(trait_decl) => trait_decl.source_file = source_file.clone(),
            _ => {}
        }
    }
}

fn compute_exports(items: &[Item]) -> HashSet<String> {
    let mut exported = HashSet::new();
    for item in items {
        match item {
            Item::Function(f) if f.is_pub => {
                exported.insert(f.name.clone());
            }
            Item::Record(r) if r.is_pub => {
                exported.insert(r.name.clone());
            }
            Item::Enum(e) if e.is_pub => {
                exported.insert(e.name.clone());
            }
            Item::Trait(t) if t.is_pub => {
                exported.insert(t.name.clone());
            }
            Item::Import(imp) if imp.is_pub => {
                if let Some(names) = &imp.names {
                    for n in names {
                        exported.insert(n.clone());
                    }
                } else if let Some(alias) = &imp.alias {
                    exported.insert(alias.clone());
                }
            }
            _ => {}
        }
    }
    exported
}

fn mangled(module_path: &[String], name: &str) -> String {
    if module_path.is_empty() {
        name.to_string()
    } else {
        format!("{}::{}", module_path.join("."), name)
    }
}

/// Sigue una cadena de 'pub import' hasta encontrar dónde vive de verdad el
/// símbolo, verificando visibilidad en cada salto (documento 07, §3 y §4).
fn resolve_export(
    target_path: &[String],
    name: &str,
    cache: &HashMap<Vec<String>, Module>,
) -> Result<String, String> {
    let Some(module) = cache.get(target_path) else {
        return Err(format!(
            "internal error: module '{}' was not loaded",
            target_path.join(".")
        ));
    };
    if !module.exported.contains(name) {
        return Err(format!(
            "Error OSTRIN-E1080\n'{name}' is private to module '{}'.\nOnly symbols marked 'pub' are accessible from other modules.",
            target_path.join(".")
        ));
    }
    let declared_here = module.items.iter().any(|it| match it {
        Item::Function(f) => f.name == name,
        Item::Record(r) => r.name == name,
        Item::Enum(e) => e.name == name,
        Item::Trait(t) => t.name == name,
        _ => false,
    });
    if declared_here {
        return Ok(mangled(target_path, name));
    }
    for item in &module.items {
        if let Item::Import(imp) = item {
            if imp.is_pub {
                if let Some(names) = &imp.names {
                    if names.iter().any(|n| n == name) {
                        return resolve_export(&imp.path, name, cache);
                    }
                }
            }
        }
    }
    Err(format!(
        "internal error: exported name '{name}' not found in module '{}'",
        target_path.join(".")
    ))
}

fn build_resolution_maps(
    module_path: &[String],
    cache: &HashMap<Vec<String>, Module>,
) -> Result<(HashMap<String, String>, HashMap<String, Vec<String>>), String> {
    let module = cache.get(module_path).unwrap();
    let mut resolve_map = HashMap::new();
    let mut alias_map = HashMap::new();
    for item in &module.items {
        match item {
            Item::Function(f) => {
                resolve_map.insert(f.name.clone(), mangled(module_path, &f.name));
            }
            Item::Record(r) => {
                resolve_map.insert(r.name.clone(), mangled(module_path, &r.name));
            }
            Item::Enum(e) => {
                resolve_map.insert(e.name.clone(), mangled(module_path, &e.name));
            }
            Item::Trait(t) => {
                resolve_map.insert(t.name.clone(), mangled(module_path, &t.name));
            }
            Item::Import(imp) => {
                if let Some(names) = &imp.names {
                    for n in names {
                        resolve_map.insert(n.clone(), resolve_export(&imp.path, n, cache)?);
                    }
                } else {
                    let alias = imp
                        .alias
                        .clone()
                        .unwrap_or_else(|| imp.path.last().cloned().unwrap());
                    alias_map.insert(alias, imp.path.clone());
                }
            }
            Item::Impl(_) => {}
        }
    }
    Ok((resolve_map, alias_map))
}

struct RewriteCtx<'a> {
    resolve_map: &'a HashMap<String, String>,
    alias_map: &'a HashMap<String, Vec<String>>,
    cache: &'a HashMap<Vec<String>, Module>,
}

fn rewrite_item(item: &mut Item, module_path: &[String], ctx: &RewriteCtx) -> Result<(), String> {
    match item {
        Item::Function(f) => {
            f.name = mangled(module_path, &f.name);
            rewrite_signature(&mut f.params, &mut f.return_type, ctx)?;
            let bound: HashSet<String> = f.params.iter().map(|p| p.name.clone()).collect();
            rewrite_block(&mut f.body, ctx, &bound)?;
        }
        Item::Record(r) => {
            r.module_path = module_path.to_vec();
            r.name = mangled(module_path, &r.name);
            for field in &mut r.fields {
                rewrite_type(&mut field.ty, ctx)?;
                if let Some(default) = &mut field.default {
                    rewrite_expr(default, ctx, &HashSet::new())?;
                }
            }
        }
        Item::Enum(e) => {
            e.module_path = module_path.to_vec();
            e.name = mangled(module_path, &e.name);
            for variant in &mut e.variants {
                for field in &mut variant.fields {
                    rewrite_type(&mut field.ty, ctx)?;
                }
            }
        }
        Item::Impl(im) => {
            im.module_path = module_path.to_vec();
            if let Some(trait_name) = &mut im.trait_name {
                if let Some(resolved) = ctx.resolve_map.get(trait_name) {
                    *trait_name = resolved.clone();
                }
            }
            for trait_arg in &mut im.trait_args {
                rewrite_type(trait_arg, ctx)?;
            }
            if let Some(resolved) = ctx.resolve_map.get(&im.type_name) {
                im.type_name = resolved.clone();
            }
            for type_arg in &mut im.type_args {
                rewrite_type(type_arg, ctx)?;
            }
            for m in &mut im.methods {
                rewrite_signature(&mut m.params, &mut m.return_type, ctx)?;
                let mut bound: HashSet<String> = m.params.iter().map(|p| p.name.clone()).collect();
                bound.insert("self".to_string());
                rewrite_block(&mut m.body, ctx, &bound)?;
            }
        }
        Item::Import(_) => {}
        Item::Trait(t) => {
            t.module_path = module_path.to_vec();
            for supertrait in &mut t.supertraits {
                if let Some(resolved) = ctx.resolve_map.get(supertrait) {
                    *supertrait = resolved.clone();
                }
            }
            t.name = mangled(module_path, &t.name);
            for method in &mut t.methods {
                rewrite_signature(&mut method.params, &mut method.return_type, ctx)?;
                if let Some(body) = &mut method.default_body {
                    let mut bound: HashSet<String> =
                        method.params.iter().map(|p| p.name.clone()).collect();
                    bound.insert("self".to_string());
                    rewrite_block(body, ctx, &bound)?;
                }
            }
        }
    }
    Ok(())
}

/// Types written in a signature name items of the module (or things it imported): they must be
/// spelled the way the merged program knows them, exactly like names in a body.
fn rewrite_signature(
    params: &mut [Param],
    return_type: &mut Type,
    ctx: &RewriteCtx,
) -> Result<(), String> {
    for param in params {
        rewrite_type(&mut param.ty, ctx)?;
        if let Some(default) = &mut param.default {
            rewrite_expr(default, ctx, &HashSet::new())?;
        }
    }
    rewrite_type(return_type, ctx)
}

/// Names bound locally (parameters, bindings, loop and pattern variables,
/// lambda parameters) shadow the module's own items: `fn f(light: Float)`
/// keeps `light` a parameter even when the module also defines `fn light`.
fn rewrite_block(
    block: &mut Block,
    ctx: &RewriteCtx,
    bound: &HashSet<String>,
) -> Result<(), String> {
    let mut local = bound.clone();
    for stmt in &mut block.stmts {
        rewrite_stmt(&mut stmt.stmt, ctx, &mut local)?;
    }
    if let Some(tail) = &mut block.tail {
        rewrite_expr(tail, ctx, &local)?;
    }
    Ok(())
}

fn pattern_names(pattern: &Pattern, bound: &mut HashSet<String>) {
    match pattern {
        Pattern::Ident(name) => {
            bound.insert(name.clone());
        }
        Pattern::Variant(_, fields) => {
            for (_, field) in fields {
                pattern_names(field, bound);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) | Pattern::Range(..) => {}
    }
}

fn rewrite_stmt(
    stmt: &mut Stmt,
    ctx: &RewriteCtx,
    bound: &mut HashSet<String>,
) -> Result<(), String> {
    match stmt {
        Stmt::Binding {
            ty, value, name, ..
        } => {
            if let Some(ty) = ty {
                rewrite_type(ty, ctx)?;
            }
            rewrite_expr(value, ctx, bound)?;
            bound.insert(name.clone());
            Ok(())
        }
        Stmt::Assign { value, name } => {
            rewrite_expr(value, ctx, bound)?;
            bound.insert(name.clone());
            Ok(())
        }
        Stmt::Return(Some(e)) | Stmt::Break(Some(e)) => rewrite_expr(e, ctx, bound),
        Stmt::Return(None) | Stmt::Break(None) | Stmt::Continue => Ok(()),
        Stmt::For {
            pattern,
            iter,
            body,
        } => {
            rewrite_expr(iter, ctx, bound)?;
            let mut inner = bound.clone();
            inner.insert(pattern.clone());
            rewrite_block(body, ctx, &inner)
        }
        Stmt::While { cond, body } => {
            rewrite_expr(cond, ctx, bound)?;
            rewrite_block(body, ctx, bound)
        }
        Stmt::FieldAssign { target, value } => {
            rewrite_expr(target, ctx, bound)?;
            rewrite_expr(value, ctx, bound)
        }
        Stmt::Expr(e) => rewrite_expr(e, ctx, bound),
    }
}

fn rewrite_expr(expr: &mut Expr, ctx: &RewriteCtx, bound: &HashSet<String>) -> Result<(), String> {
    match expr {
        Expr::Located(inner, _) => rewrite_expr(inner, ctx, bound),
        Expr::FieldAccess(obj, member) => {
            if let Expr::Ident(alias) = obj.as_ref() {
                if let Some(target_path) =
                    ctx.alias_map.get(alias).filter(|_| !bound.contains(alias))
                {
                    let resolved = resolve_export(target_path, member, ctx.cache)?;
                    *expr = Expr::Ident(resolved);
                    return Ok(());
                }
            }
            rewrite_expr(obj, ctx, bound)
        }
        Expr::Ident(name) => {
            if let Some(resolved) = ctx.resolve_map.get(name).filter(|_| !bound.contains(name)) {
                *name = resolved.clone();
            }
            Ok(())
        }
        Expr::UnitLiteral(n, _) => rewrite_expr(n, ctx, bound),
        Expr::Loop(b) => rewrite_block(b, ctx, bound),
        Expr::Unary(_, e) => rewrite_expr(e, ctx, bound),
        Expr::Binary(_, l, r) => {
            rewrite_expr(l, ctx, bound)?;
            rewrite_expr(r, ctx, bound)
        }
        Expr::Range(s, _, e, step) => {
            rewrite_expr(s, ctx, bound)?;
            rewrite_expr(e, ctx, bound)?;
            if let Some(st) = step {
                rewrite_expr(st, ctx, bound)?;
            }
            Ok(())
        }
        Expr::Call(callee, args) => {
            rewrite_expr(callee, ctx, bound)?;
            for a in args {
                match a {
                    Arg::Positional(e) | Arg::Named(_, e) => rewrite_expr(e, ctx, bound)?,
                }
            }
            Ok(())
        }
        Expr::GenericCall(callee, type_args, args) => {
            rewrite_expr(callee, ctx, bound)?;
            for type_arg in type_args {
                rewrite_type(type_arg, ctx)?;
            }
            for a in args {
                match a {
                    Arg::Positional(e) | Arg::Named(_, e) => rewrite_expr(e, ctx, bound)?,
                }
            }
            Ok(())
        }
        Expr::Index(obj, idx) => {
            rewrite_expr(obj, ctx, bound)?;
            rewrite_expr(idx, ctx, bound)
        }
        Expr::If(cond, then_b, else_b) => {
            rewrite_expr(cond, ctx, bound)?;
            rewrite_block(then_b, ctx, bound)?;
            if let Some(b) = else_b {
                rewrite_block(b, ctx, bound)?;
            }
            Ok(())
        }
        Expr::Block(b) => rewrite_block(b, ctx, bound),
        Expr::Lambda(params, b) => {
            let mut inner = bound.clone();
            inner.extend(params.iter().cloned());
            rewrite_block(b, ctx, &inner)
        }
        Expr::ListLiteral(items) | Expr::SetLiteral(items) => {
            for it in items {
                rewrite_expr(it, ctx, bound)?;
            }
            Ok(())
        }
        Expr::EmptyCollection(_, type_args) => {
            for type_arg in type_args {
                rewrite_type(type_arg, ctx)?;
            }
            Ok(())
        }
        Expr::SizedIntLiteral(..) | Expr::Float32Literal(_) => Ok(()),
        Expr::MapLiteral(pairs) => {
            for (k, v) in pairs {
                rewrite_expr(k, ctx, bound)?;
                rewrite_expr(v, ctx, bound)?;
            }
            Ok(())
        }
        Expr::Try(inner, catch) => {
            rewrite_expr(inner, ctx, bound)?;
            if let Some(c) = catch {
                rewrite_expr(c, ctx, bound)?;
            }
            Ok(())
        }
        Expr::Within(a, r) => {
            rewrite_expr(a, ctx, bound)?;
            rewrite_expr(r, ctx, bound)
        }
        Expr::Approximately(a, b, t) => {
            rewrite_expr(a, ctx, bound)?;
            rewrite_expr(b, ctx, bound)?;
            rewrite_expr(t, ctx, bound)
        }
        Expr::As(e, _) => rewrite_expr(e, ctx, bound),
        Expr::RecordLiteral(name, fields) => {
            if let Some(resolved) = ctx.resolve_map.get(name.as_str()) {
                *name = resolved.clone();
            }
            for (_, v) in fields {
                rewrite_expr(v, ctx, bound)?;
            }
            Ok(())
        }
        Expr::GenericRecordLiteral(name, type_args, fields) => {
            if let Some(resolved) = ctx.resolve_map.get(name.as_str()) {
                *name = resolved.clone();
            }
            for type_arg in type_args {
                rewrite_type(type_arg, ctx)?;
            }
            for (_, v) in fields {
                rewrite_expr(v, ctx, bound)?;
            }
            Ok(())
        }
        Expr::Match(scrutinee, arms) => {
            rewrite_expr(scrutinee, ctx, bound)?;
            for arm in arms {
                let mut inner = bound.clone();
                pattern_names(&arm.pattern, &mut inner);
                if let Some(g) = &mut arm.guard {
                    rewrite_expr(g, ctx, &inner)?;
                }
                rewrite_block(&mut arm.body, ctx, &inner)?;
            }
            Ok(())
        }
        Expr::Spawn(b) | Expr::SpawnScope(b) => rewrite_block(b, ctx, bound),
        Expr::Channel(_, cap) => {
            if let Some(c) = cap {
                rewrite_expr(c, ctx, bound)?;
            }
            Ok(())
        }
        Expr::IntLiteral(_)
        | Expr::FloatLiteral(_)
        | Expr::StringLiteral(_)
        | Expr::CharLiteral(_)
        | Expr::BoolLiteral(_) => Ok(()),
    }
}

fn rewrite_type(ty: &mut Type, ctx: &RewriteCtx) -> Result<(), String> {
    match ty {
        Type::Named(name, args) => {
            if let Some(resolved) = ctx.resolve_map.get(name) {
                *name = resolved.clone();
            }
            for arg in args {
                rewrite_type(arg, ctx)?;
            }
        }
        Type::Mul(left, right) | Type::Div(left, right) => {
            rewrite_type(left, ctx)?;
            rewrite_type(right, ctx)?;
        }
        Type::Pow(inner, _) => rewrite_type(inner, ctx)?,
        Type::Fn(params, ret) => {
            for param in params {
                rewrite_type(param, ctx)?;
            }
            rewrite_type(ret, ctx)?;
        }
        Type::Dyn(names) => {
            for name in names {
                if let Some(resolved) = ctx.resolve_map.get(name) {
                    *name = resolved.clone();
                }
            }
        }
    }
    Ok(())
}
