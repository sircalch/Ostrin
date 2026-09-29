//! Experimental, conservative effect inventory for the source program.
//!
//! This is deliberately not the effect checker described in design document 26.
//! It records syntactic evidence so the compiler can expose a first inventory
//! before effect rows become part of HIR and function signatures.

use std::collections::BTreeSet;

use crate::ast::{Arg, Block, Expr, FunctionDecl, ImplDecl, Item, MatchArm, Pattern, Stmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectSite {
    pub effect: String,
    pub operation: String,
    pub function: String,
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectReport {
    pub effects: BTreeSet<String>,
    pub sites: Vec<EffectSite>,
}

impl EffectReport {
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

pub fn analyze(items: &[Item]) -> EffectReport {
    let mut report = EffectReport::default();
    for item in items {
        match item {
            Item::Function(function) => walk_function(function, &mut report),
            Item::Impl(implementation) => walk_impl(implementation, &mut report),
            _ => {}
        }
    }
    report.sort_sites();
    report
}

fn walk_impl(implementation: &ImplDecl, report: &mut EffectReport) {
    for method in &implementation.methods {
        walk_function(method, report);
    }
}

fn walk_function(function: &FunctionDecl, report: &mut EffectReport) {
    for parameter in &function.params {
        if let Some(default) = &parameter.default {
            walk_expr(
                default,
                &function.name,
                function.source_file.as_deref(),
                report,
            );
        }
    }
    walk_block(
        &function.body,
        &function.name,
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
                if let Some(effect) = classify(&operation) {
                    report.effects.insert(effect.to_string());
                    report.sites.push(EffectSite {
                        effect: effect.to_string(),
                        operation,
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
        Expr::Lambda(_, block) => walk_block(block, function, file, report),
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

fn classify(operation: &str) -> Option<&'static str> {
    let segments: Vec<&str> = operation.split('.').collect();
    let leaf = operation
        .rsplit(|character| character == '.' || character == ':')
        .find(|segment| !segment.is_empty())
        .unwrap_or(operation);
    if segments.first() == Some(&"measurements") || operation.contains("measurements::") {
        return Some("measurement");
    }
    if (segments.first() == Some(&"viz") || operation.contains("viz::")) && leaf == "provenance" {
        return Some("provenance");
    }
    if matches!(
        leaf,
        "rng" | "rand" | "randn" | "uniform" | "normal" | "sample" | "choice"
    ) {
        return Some("random(seed)");
    }
    if matches!(leaf, "now" | "clock" | "today" | "sleep") {
        return Some("clock");
    }
    if matches!(
        leaf,
        "print" | "println" | "read_file" | "write_file" | "args" | "env" | "save" | "load"
    ) {
        return Some("io");
    }
    if matches!(
        leaf,
        "http" | "https" | "fetch" | "request" | "socket" | "tcp" | "udp"
    ) {
        return Some("network");
    }
    if matches!(
        leaf,
        "spawn" | "spawn_scope" | "channel" | "select" | "send" | "recv" | "join" | "cancel"
    ) {
        return Some("concurrency");
    }
    None
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
    report.effects.insert(effect.to_string());
    report.sites.push(EffectSite {
        effect: effect.to_string(),
        operation: operation.to_string(),
        function: function.to_string(),
        file: file.map(str::to_string),
        line,
        col,
    });
}

pub fn effect_description(effect: &str) -> &'static str {
    match effect {
        "clock" => "observa reloj o tiempo",
        "concurrency" => "usa tareas, canales o scheduler",
        "io" => "observa o modifica E/S del host",
        "measurement" => "propaga mediciones o incertidumbre",
        "network" => "contacta un recurso de red",
        "provenance" => "registra metadatos de procedencia",
        "random(seed)" => "consume azar con un flujo explícito o detectable",
        _ => "efecto desconocido",
    }
}
