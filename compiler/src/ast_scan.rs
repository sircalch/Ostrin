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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Item;

    /// The body of the first `for` loop of `main` in `source`.
    fn first_loop_body(source: &str) -> Block {
        let tokens = crate::lexer::Lexer::new(source).tokenize().expect("lexes");
        let (items, errors) = crate::parser::Parser::new(tokens).parse_program();
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let Item::Function(main) = items.into_iter().find(|i| matches!(i, Item::Function(f) if f.name == "main")).expect("main") else {
            unreachable!()
        };
        for stmt in main.body.stmts {
            if let Stmt::For { body, .. } = stmt.stmt {
                return body;
            }
        }
        panic!("no for loop in main");
    }

    fn may_capture(body: &Block) -> bool {
        block_any(body, &|e| matches!(e, Expr::Lambda(..) | Expr::Spawn(_) | Expr::SpawnScope(_)))
    }

    #[test]
    fn a_closure_in_any_position_of_the_body_is_detected() {
        let positions = [
            "lv: fn(Int) -> Int = fn(a) { a }",
            "mut acc = [fn(a) { a }]",
            "call(fn(a) { a })",
            "    if true { f = fn(a) { a } }",
            "    while false { g = fn(a) { a } }",
            "    for k in 0 until 2 { h = fn(a) { a } }",
            "    match v { 1 => { q = fn(a) { a } }, _ => { } }",
            "    r = Rec { f: fn(a) { a } }",
            "    l = [1, 2].map(fn(a) { a })",
            "    m = a + (fn(a) { a })(1)",
            "    nested = (if true { fn(a) { a } } else { fn(a) { a } })",
            "    spawn { s = fn(a) { a } }",
        ];
        for position in positions {
            let source = format!("fn main() -> Void {{\n    for i in 0 until 3 {{\n        {position}\n    }}\n}}\n");
            let body = first_loop_body(&source);
            assert!(may_capture(&body), "a closure or task in `{position}` must disable frame reuse");
        }
    }

    #[test]
    fn a_body_without_closures_or_tasks_allows_reuse() {
        let body = first_loop_body(
            "fn main() -> Void {\n    for i in 0 until 3 {\n        t = i * 2\n        if t > 1 {\n            print(t.to_string())\n        }\n    }\n}\n",
        );
        assert!(!may_capture(&body));
    }
}
