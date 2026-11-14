//! Read-only search over the AST. Used by the interpreter to decide whether a loop body can
//! retain its frame. Only a closure (`fn(...) { ... }`) or a spawned task can keep the enclosing
//! environment alive after the iteration that created it, so a body without either may reuse one frame.

use crate::ast::{Arg, Block, Expr, MatchArm, Pattern, Stmt};

/// True when `pred` holds for `e` or for any expression nested inside it.
pub fn expr_any(e: &Expr, pred: &dyn Fn(&Expr) -> bool) -> bool {
    if pred(e) {
        return true;
    }
    match e {
        Expr::Located(inner, _) | Expr::UnitLiteral(inner, _) | Expr::Unary(_, inner) | Expr::FieldAccess(inner, _) => {
            expr_any(inner, pred)
        }
        Expr::Try(a, rest) => expr_any(a, pred) || rest.as_deref().is_some_and(|b| expr_any(b, pred)),
        Expr::Binary(_, a, b) | Expr::Index(a, b) | Expr::Within(a, b) | Expr::As(a, b) => {
            expr_any(a, pred) || expr_any(b, pred)
        }
        Expr::Range(a, _, b, rest) => {
            expr_any(a, pred) || expr_any(b, pred) || rest.as_deref().is_some_and(|c| expr_any(c, pred))
        }
        Expr::Approximately(a, b, c) => expr_any(a, pred) || expr_any(b, pred) || expr_any(c, pred),
        Expr::Call(callee, args) | Expr::GenericCall(callee, _, args) => {
            expr_any(callee, pred) || args.iter().any(|arg| arg_any(arg, pred))
        }
        Expr::If(cond, then, otherwise) => {
            expr_any(cond, pred) || block_any(then, pred) || otherwise.as_ref().is_some_and(|b| block_any(b, pred))
        }
        Expr::Block(b) | Expr::Loop(b) => block_any(b, pred),
        Expr::Lambda(_, body) => block_any(body, pred),
        Expr::Spawn(b) | Expr::SpawnScope(b) => block_any(b, pred),
        Expr::ListLiteral(items) | Expr::SetLiteral(items) => items.iter().any(|item| expr_any(item, pred)),
        Expr::MapLiteral(pairs) => pairs.iter().any(|(k, v)| expr_any(k, pred) || expr_any(v, pred)),
        Expr::RecordLiteral(_, fields) | Expr::GenericRecordLiteral(_, _, fields) => {
            fields.iter().any(|(_, v)| expr_any(v, pred))
        }
        Expr::Match(scrutinee, arms) => expr_any(scrutinee, pred) || arms.iter().any(|arm| arm_any(arm, pred)),
        Expr::Channel(_, capacity) => capacity.as_deref().is_some_and(|c| expr_any(c, pred)),
        Expr::IntLiteral(_)
        | Expr::SizedIntLiteral(..)
        | Expr::FloatLiteral(_)
        | Expr::Float32Literal(_)
        | Expr::StringLiteral(_)
        | Expr::CharLiteral(_)
        | Expr::BoolLiteral(_)
        | Expr::Ident(_)
        | Expr::EmptyCollection(..) => false,
    }
}

/// True when `pred` holds for any expression inside `block`, including its statements and tail.
pub fn block_any(block: &Block, pred: &dyn Fn(&Expr) -> bool) -> bool {
    block.stmts.iter().any(|s| stmt_any(&s.stmt, pred)) || block.tail.as_deref().is_some_and(|t| expr_any(t, pred))
}

fn stmt_any(stmt: &Stmt, pred: &dyn Fn(&Expr) -> bool) -> bool {
    match stmt {
        Stmt::Binding { value, .. } | Stmt::Assign { value, .. } => expr_any(value, pred),
        Stmt::Return(value) | Stmt::Break(value) => value.as_ref().is_some_and(|e| expr_any(e, pred)),
        Stmt::For { iter, body, .. } => expr_any(iter, pred) || block_any(body, pred),
        Stmt::While { cond, body } => expr_any(cond, pred) || block_any(body, pred),
        Stmt::FieldAssign { target, value } => expr_any(target, pred) || expr_any(value, pred),
        Stmt::Expr(e) => expr_any(e, pred),
        Stmt::Continue => false,
    }
}

fn arg_any(arg: &Arg, pred: &dyn Fn(&Expr) -> bool) -> bool {
    match arg {
        Arg::Positional(e) | Arg::Named(_, e) => expr_any(e, pred),
    }
}

fn arm_any(arm: &MatchArm, pred: &dyn Fn(&Expr) -> bool) -> bool {
    pattern_any(&arm.pattern, pred) || arm.guard.as_ref().is_some_and(|g| expr_any(g, pred)) || block_any(&arm.body, pred)
}

fn pattern_any(pattern: &Pattern, pred: &dyn Fn(&Expr) -> bool) -> bool {
    match pattern {
        Pattern::Literal(e) => expr_any(e, pred),
        Pattern::Range(a, _, b) => expr_any(a, pred) || expr_any(b, pred),
        Pattern::Variant(_, fields) => fields.iter().any(|(_, p)| pattern_any(p, pred)),
        Pattern::Wildcard | Pattern::Ident(_) => false,
    }
}
