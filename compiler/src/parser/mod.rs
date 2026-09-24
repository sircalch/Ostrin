use crate::ast::*;
use crate::lexer::{Token, TokenKind};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    paren_depth: usize,
    no_struct_literal: bool,
}

type PResult<T> = Result<T, ParseError>;

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0, paren_depth: 0, no_struct_literal: false }
    }

    fn parse_expr_no_struct(&mut self) -> PResult<Expr> {
        let saved = self.no_struct_literal;
        self.no_struct_literal = true;
        let result = self.parse_expr();
        self.no_struct_literal = saved;
        result
    }

    /// Recuperación de errores a nivel de declaración: si un ítem de nivel
    /// superior falla al parsear, se registra el error y se sincroniza en la
    /// siguiente palabra clave que inicia una declaración, en vez de detener
    /// todo el archivo en el primer error — así se reportan de una vez todos
    /// los errores de sintaxis del archivo, no solo el primero.
    pub fn parse_program(mut self) -> (Vec<Item>, Vec<ParseError>) {
        let mut items = Vec::new();
        let mut errors = Vec::new();
        while !self.check(&TokenKind::Eof) {
            let checkpoint = self.pos;
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(e) => {
                    errors.push(e);
                    if self.pos == checkpoint {
                        self.advance();
                    }
                    self.synchronize_to_item_boundary();
                }
            }
        }
        (items, errors)
    }

    fn synchronize_to_item_boundary(&mut self) {
        while !self.check(&TokenKind::Eof) {
            if matches!(
                self.peek().kind,
                TokenKind::Fn | TokenKind::Record | TokenKind::Enum | TokenKind::Impl
                    | TokenKind::Trait | TokenKind::Import | TokenKind::Pub
            ) {
                return;
            }
            self.advance();
        }
    }

    fn parse_item(&mut self) -> PResult<Item> {
        let is_pub = self.eat(&TokenKind::Pub);
        if self.check(&TokenKind::Fn) {
            Ok(Item::Function(self.parse_function(is_pub)?))
        } else if self.check(&TokenKind::Record) {
            Ok(Item::Record(self.parse_record(is_pub)?))
        } else if self.check(&TokenKind::Enum) {
            Ok(Item::Enum(self.parse_enum(is_pub)?))
        } else if self.check(&TokenKind::Impl) {
            Ok(Item::Impl(self.parse_impl()?))
        } else if self.check(&TokenKind::Import) {
            Ok(Item::Import(self.parse_import(is_pub)?))
        } else if self.check(&TokenKind::Trait) {
            Ok(Item::Trait(self.parse_trait(is_pub)?))
        } else {
            Err(self.error("expected a top-level item (e.g. 'fn', 'record', 'enum', 'impl', 'import', 'trait')"))
        }
    }

    fn parse_trait(&mut self, is_pub: bool) -> PResult<TraitDecl> {
        let span = self.current_span();
        self.expect(&TokenKind::Trait)?;
        let name = self.expect_ident()?;
        let generics = if self.check(&TokenKind::Lt) { self.parse_generic_params()? } else { Vec::new() };
        let mut supertraits = Vec::new();
        if self.eat(&TokenKind::Colon) {
            supertraits.push(self.expect_ident()?);
            while self.eat(&TokenKind::Plus) {
                supertraits.push(self.expect_ident()?);
            }
        }
        self.expect(&TokenKind::LBrace)?;
        let mut methods = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            self.expect(&TokenKind::Fn)?;
            let mname = self.expect_ident()?;
            let mgenerics = if self.check(&TokenKind::Lt) { self.parse_generic_params()? } else { Vec::new() };
            self.expect(&TokenKind::LParen)?;
            let params = self.parse_param_list()?;
            self.expect(&TokenKind::RParen)?;
            self.expect(&TokenKind::Arrow)?;
            let return_type = self.parse_type()?;
            let default_body = if self.check(&TokenKind::LBrace) { Some(self.parse_block()?) } else { None };
            methods.push(TraitMethodSig { name: mname, generics: mgenerics, params, return_type, default_body });
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(TraitDecl { name, module_path: Vec::new(), is_pub, generics, supertraits, methods, span, source_file: None })
    }

    fn parse_import(&mut self, is_pub: bool) -> PResult<ImportDecl> {
        self.expect(&TokenKind::Import)?;
        let mut path = vec![self.expect_ident()?];
        while self.check(&TokenKind::Dot) && !self.check_at(1, &TokenKind::LBrace) {
            self.advance();
            path.push(self.expect_ident()?);
        }
        let mut names = None;
        if self.eat(&TokenKind::Dot) {
            self.expect(&TokenKind::LBrace)?;
            let mut list = Vec::new();
            loop {
                list.push(self.expect_ident()?);
                if !self.eat(&TokenKind::Comma) { break; }
            }
            self.expect(&TokenKind::RBrace)?;
            names = Some(list);
        }
        let alias = if names.is_none() && self.eat(&TokenKind::As) { Some(self.expect_ident()?) } else { None };
        Ok(ImportDecl { is_pub, path, alias, names })
    }

    fn parse_derive_list(&mut self) -> PResult<Vec<String>> {
        let mut derives = Vec::new();
        if self.eat(&TokenKind::Colon) {
            loop {
                derives.push(self.expect_ident()?);
                if !self.eat(&TokenKind::Comma) { break; }
            }
        }
        Ok(derives)
    }

    fn parse_record(&mut self, is_pub: bool) -> PResult<RecordDecl> {
        let span = self.current_span();
        self.expect(&TokenKind::Record)?;
        let name = self.expect_ident()?;
        let generics = if self.check(&TokenKind::Lt) { self.parse_generic_params()? } else { Vec::new() };
        let derives = self.parse_derive_list()?;
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let field_pub = self.eat(&TokenKind::Pub);
            let field_mut = self.eat(&TokenKind::Mut);
            let fname = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let default = if self.eat(&TokenKind::Eq) { Some(self.parse_expr()?) } else { None };
            fields.push(FieldDecl { name: fname, is_pub: field_pub, is_mut: field_mut, ty, default });
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(RecordDecl { name, module_path: Vec::new(), is_pub, generics, derives, fields, span, source_file: None })
    }

    fn parse_enum(&mut self, is_pub: bool) -> PResult<EnumDecl> {
        let span = self.current_span();
        self.expect(&TokenKind::Enum)?;
        let name = self.expect_ident()?;
        let generics = if self.check(&TokenKind::Lt) { self.parse_generic_params()? } else { Vec::new() };
        let derives = self.parse_derive_list()?;
        self.expect(&TokenKind::LBrace)?;
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let vname = self.expect_ident()?;
            let mut fields = Vec::new();
            if self.eat(&TokenKind::LParen) {
                if !self.check(&TokenKind::RParen) {
                    loop {
                        if matches!(self.peek().kind, TokenKind::Ident(_)) && self.check_at(1, &TokenKind::Colon) {
                            let fname = self.expect_ident()?;
                            self.advance();
                            fields.push(VariantFieldDecl { name: Some(fname), ty: self.parse_type()? });
                        } else {
                            fields.push(VariantFieldDecl { name: None, ty: self.parse_type()? });
                        }
                        if !self.eat(&TokenKind::Comma) { break; }
                    }
                }
                self.expect(&TokenKind::RParen)?;
            }
            variants.push(VariantDecl { name: vname, fields });
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(EnumDecl { name, module_path: Vec::new(), is_pub, generics, derives, variants, span, source_file: None })
    }

    fn parse_impl(&mut self) -> PResult<ImplDecl> {
        let span = self.current_span();
        self.expect(&TokenKind::Impl)?;
        let generics = if self.check(&TokenKind::Lt) { self.parse_generic_params()? } else { Vec::new() };
        let first = self.expect_ident()?;
        let first_args = if self.check(&TokenKind::Lt) { self.parse_type_args()? } else { Vec::new() };
        let (trait_name, trait_args, type_name, type_args) = if self.eat(&TokenKind::For) {
            let type_name = self.expect_ident()?;
            let type_args = if self.check(&TokenKind::Lt) { self.parse_type_args()? } else { Vec::new() };
            (Some(first), first_args, type_name, type_args)
        } else {
            (None, Vec::new(), first, first_args)
        };
        self.expect(&TokenKind::LBrace)?;
        let mut methods = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            methods.push(self.parse_function(false)?);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(ImplDecl { generics, trait_name, trait_args, type_name, type_args, module_path: Vec::new(), methods, span, source_file: None })
    }

    fn parse_function(&mut self, is_pub: bool) -> PResult<FunctionDecl> {
        let span = self.current_span();
        self.expect(&TokenKind::Fn)?;
        let name = self.expect_ident()?;
        let generics = if self.check(&TokenKind::Lt) { self.parse_generic_params()? } else { Vec::new() };
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_param_list()?;
        self.expect(&TokenKind::RParen)?;
        self.expect(&TokenKind::Arrow)?;
        let return_type = self.parse_type()?;
        let body = self.parse_block()?;
        Ok(FunctionDecl {
            name,
            is_pub,
            generics,
            params,
            return_type,
            body,
            span,
            source_file: None,
        })
    }

    fn parse_generic_params(&mut self) -> PResult<Vec<GenericParam>> {
        self.expect(&TokenKind::Lt)?;
        let mut params = Vec::new();
        loop {
            let name = self.expect_ident()?;
            let mut bounds = Vec::new();
            if self.eat(&TokenKind::Colon) {
                loop {
                    bounds.push(self.expect_bound()?);
                    if !self.eat(&TokenKind::Plus) { break; }
                }
            }
            params.push(GenericParam { name, bounds });
            if !self.eat(&TokenKind::Comma) { break; }
        }
        self.expect(&TokenKind::Gt)?;
        Ok(params)
    }

    fn expect_bound(&mut self) -> PResult<String> {
        if self.eat(&TokenKind::Dimension) {
            return Ok("Dimension".to_string());
        }
        self.expect_ident()
    }

    fn parse_param_list(&mut self) -> PResult<Vec<Param>> {
        let mut params = Vec::new();
        if self.check(&TokenKind::RParen) {
            return Ok(params);
        }
        if self.check(&TokenKind::SelfLower) {
            self.advance();
            params.push(Param { name: "self".to_string(), ty: Type::Named("Self".to_string(), vec![]), default: None, is_mut: false });
            if !self.eat(&TokenKind::Comma) {
                return Ok(params);
            }
        } else if self.check(&TokenKind::Mut) && self.check_at(1, &TokenKind::SelfLower) {
            self.advance();
            self.advance();
            params.push(Param { name: "self".to_string(), ty: Type::Named("Self".to_string(), vec![]), default: None, is_mut: true });
            if !self.eat(&TokenKind::Comma) {
                return Ok(params);
            }
        }
        loop {
            let name = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let default = if self.eat(&TokenKind::Eq) { Some(self.parse_expr()?) } else { None };
            params.push(Param { name, ty, default, is_mut: false });
            if !self.eat(&TokenKind::Comma) { break; }
        }
        Ok(params)
    }

    fn parse_type(&mut self) -> PResult<Type> {
        let mut ty = self.parse_type_atom()?;
        loop {
            if self.eat(&TokenKind::Star) {
                ty = Type::Mul(Box::new(ty), Box::new(self.parse_type_atom()?));
            } else if self.eat(&TokenKind::Slash) {
                ty = Type::Div(Box::new(ty), Box::new(self.parse_type_atom()?));
            } else if self.eat(&TokenKind::Caret) {
                let exp = self.expect_int()?;
                ty = Type::Pow(Box::new(ty), exp);
            } else {
                break;
            }
        }
        Ok(ty)
    }

    fn parse_type_atom(&mut self) -> PResult<Type> {
        if self.eat(&TokenKind::Dyn) {
            let mut names = vec![self.expect_ident()?];
            while self.eat(&TokenKind::Plus) {
                names.push(self.expect_ident()?);
            }
            return Ok(Type::Dyn(names));
        }
        if self.eat(&TokenKind::Fn) {
            self.expect(&TokenKind::LParen)?;
            let mut params = Vec::new();
            if !self.check(&TokenKind::RParen) {
                loop {
                    params.push(self.parse_type()?);
                    if !self.eat(&TokenKind::Comma) { break; }
                }
            }
            self.expect(&TokenKind::RParen)?;
            self.expect(&TokenKind::Arrow)?;
            let ret = self.parse_type()?;
            return Ok(Type::Fn(params, Box::new(ret)));
        }
        let name = if self.eat(&TokenKind::SelfUpper) {
            "Self".to_string()
        } else {
            self.expect_ident()?
        };
        let mut args = Vec::new();
        if self.eat(&TokenKind::Lt) {
            loop {
                args.push(self.parse_type()?);
                if !self.eat(&TokenKind::Comma) { break; }
            }
            self.expect(&TokenKind::Gt)?;
        }
        Ok(Type::Named(name, args))
    }

    fn parse_block(&mut self) -> PResult<Block> {
        self.expect(&TokenKind::LBrace)?;
        let saved_depth = self.paren_depth;
        self.paren_depth = 0;
        let mut stmts = Vec::new();
        let mut tail = None;
        while !self.check(&TokenKind::RBrace) {
            let stmt = self.parse_statement()?;
            if self.check(&TokenKind::RBrace) {
                if let Stmt::Expr(e) = &stmt.stmt {
                    tail = Some(Box::new(e.clone()));
                    break;
                }
            }
            stmts.push(stmt);
        }
        self.paren_depth = saved_depth;
        self.expect(&TokenKind::RBrace)?;
        Ok(Block { stmts, tail })
    }

    fn parse_statement(&mut self) -> PResult<LocatedStmt> {
        let span = self.current_span();
        if self.eat(&TokenKind::Return) {
            let value = if self.check(&TokenKind::RBrace) { None } else { Some(self.parse_expr()?) };
            return Ok(self.located(Stmt::Return(value), span));
        }
        if self.eat(&TokenKind::Break) {
            let value = if self.check(&TokenKind::RBrace) { None } else { Some(self.parse_expr()?) };
            return Ok(self.located(Stmt::Break(value), span));
        }
        if self.eat(&TokenKind::Continue) {
            return Ok(self.located(Stmt::Continue, span));
        }
        if self.eat(&TokenKind::For) {
            let pattern = self.expect_ident()?;
            self.expect(&TokenKind::In)?;
            let iter = self.parse_expr_no_struct()?;
            let body = self.parse_block()?;
            return Ok(self.located(Stmt::For { pattern, iter, body }, span));
        }
        if self.eat(&TokenKind::While) {
            let cond = self.parse_expr_no_struct()?;
            let body = self.parse_block()?;
            return Ok(self.located(Stmt::While { cond, body }, span));
        }
        if self.check(&TokenKind::Mut) {
            self.advance();
            let name = self.expect_ident()?;
            let ty = if self.eat(&TokenKind::Colon) { Some(self.parse_type()?) } else { None };
            self.expect(&TokenKind::Eq)?;
            let value = self.parse_expr()?;
            return Ok(self.located(Stmt::Binding { mut_: true, name, ty, value }, span));
        }
        if matches!(self.peek().kind, TokenKind::Ident(_)) && self.check_at(1, &TokenKind::Colon) {
            let name = self.expect_ident()?;
            self.advance();
            let ty = self.parse_type()?;
            self.expect(&TokenKind::Eq)?;
            let value = self.parse_expr()?;
            return Ok(self.located(Stmt::Binding { mut_: false, name, ty: Some(ty), value }, span));
        }
        if matches!(self.peek().kind, TokenKind::Ident(_)) && self.check_at(1, &TokenKind::Eq) {
            let name = self.expect_ident()?;
            self.advance();
            let value = self.parse_expr()?;
            return Ok(self.located(Stmt::Assign { name, value }, span));
        }
        // One parse only: re-parsing a statement as an expression after trying it as an
        // assignment target doubled the work at every nesting level (exponential time on
        // deeply nested blocks).
        let expr = self.parse_expr()?;
        if self.check(&TokenKind::Eq) && is_assignable_target(&expr) {
            self.advance();
            let target = match expr {
                Expr::Located(inner, _) => *inner,
                other => other,
            };
            let value = self.parse_expr()?;
            return Ok(self.located(Stmt::FieldAssign { target, value }, span));
        }
        Ok(self.located(Stmt::Expr(expr), span))
    }

    pub fn parse_expr(&mut self) -> PResult<Expr> {
        let start = self.current_span();
        let expression = self.parse_or()?;
        let end = self.previous_span();
        Ok(Expr::Located(Box::new(expression), SourceRange { start, end }))
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let mut left = self.parse_and()?;
        while self.eat(&TokenKind::Or) {
            let right = self.parse_and()?;
            left = Expr::Binary(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let mut left = self.parse_within_approx()?;
        while self.eat(&TokenKind::And) {
            let right = self.parse_within_approx()?;
            left = Expr::Binary(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_within_approx(&mut self) -> PResult<Expr> {
        let left = self.parse_comparison()?;
        if self.eat(&TokenKind::Within) {
            let range = self.parse_comparison()?;
            return Ok(Expr::Within(Box::new(left), Box::new(range)));
        }
        if self.eat(&TokenKind::Approximately) {
            let target = self.parse_comparison()?;
            self.expect(&TokenKind::Tolerance)?;
            let tol = self.parse_comparison()?;
            return Ok(Expr::Approximately(Box::new(left), Box::new(target), Box::new(tol)));
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> PResult<Expr> {
        let mut left = self.parse_range()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::NotEq => BinOp::NotEq,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::LtEq => BinOp::LtEq,
                TokenKind::GtEq => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_range()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_range(&mut self) -> PResult<Expr> {
        let left = self.parse_as_expr()?;
        let kind = if self.eat(&TokenKind::To) {
            RangeKind::To
        } else if self.eat(&TokenKind::Until) {
            RangeKind::Until
        } else {
            return Ok(left);
        };
        let end = self.parse_as_expr()?;
        let step = if self.eat(&TokenKind::Step) { Some(Box::new(self.parse_as_expr()?)) } else { None };
        Ok(Expr::Range(Box::new(left), kind, Box::new(end), step))
    }

    fn parse_as_expr(&mut self) -> PResult<Expr> {
        let mut left = self.parse_additive()?;
        while self.eat(&TokenKind::As) {
            let unit = match self.peek().kind.clone() {
                TokenKind::Ident(first) if crate::types::unit_info(&first).is_some() => {
                    // `v as km/h`, `a as m/s^2`: a compound unit on one line.
                    let line = self.peek().line;
                    self.advance();
                    let mut unit = first;
                    self.unit_exponent(&mut unit, line)?;
                    self.unit_tail(&mut unit, line)?;
                    Expr::Ident(unit)
                }
                _ => self.parse_multiplicative()?,
            };
            left = Expr::As(Box::new(left), Box::new(unit));
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> PResult<Expr> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus if !self.newline_breaks_statement() => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> PResult<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                TokenKind::At => {
                    // `a @ b` is `a.matmul(b)`: no new AST node, so every later stage already handles it.
                    self.advance();
                    let right = self.parse_unary()?;
                    left = Expr::Call(
                        Box::new(Expr::FieldAccess(Box::new(left), "matmul".to_string())),
                        vec![Arg::Positional(right)],
                    );
                    continue;
                }
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        if self.eat(&TokenKind::Minus) {
            return Ok(Expr::Unary(UnaryOp::Neg, Box::new(self.parse_unary()?)));
        }
        if self.eat(&TokenKind::Not) {
            return Ok(Expr::Unary(UnaryOp::Not, Box::new(self.parse_unary()?)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.eat(&TokenKind::Dot) {
                let field = self.expect_ident()?;
                expr = Expr::FieldAccess(Box::new(expr), field);
            } else if self.check(&TokenKind::Lt) && !self.newline_breaks_statement() {
                let checkpoint = self.pos;
                let type_args = match self.parse_type_args() {
                    Ok(type_args) if self.check(&TokenKind::LParen) => type_args,
                    _ => {
                        self.pos = checkpoint;
                        break;
                    }
                };
                let args = self.parse_call_args()?;
                expr = Expr::GenericCall(Box::new(expr), type_args, args);
            } else if self.check(&TokenKind::LParen) && !self.newline_breaks_statement() {
                let args = self.parse_call_args()?;
                expr = Expr::Call(Box::new(expr), args);
            } else if self.check(&TokenKind::LBracket) && !self.newline_breaks_statement() {
                self.advance();
                let index = self.parse_expr()?;
                self.expect(&TokenKind::RBracket)?;
                expr = Expr::Index(Box::new(expr), Box::new(index));
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_type_args(&mut self) -> PResult<Vec<Type>> {
        self.expect(&TokenKind::Lt)?;
        let mut args = Vec::new();
        loop {
            args.push(self.parse_type()?);
            if !self.eat(&TokenKind::Comma) { break; }
        }
        self.expect(&TokenKind::Gt)?;
        Ok(args)
    }

    fn parse_call_args(&mut self) -> PResult<Vec<Arg>> {
        self.expect(&TokenKind::LParen)?;
        let mut args = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                if matches!(self.peek().kind, TokenKind::Ident(_)) && self.check_at(1, &TokenKind::Colon) {
                    let name = self.expect_ident()?;
                    self.advance();
                    args.push(Arg::Named(name, self.parse_expr()?));
                } else {
                    args.push(Arg::Positional(self.parse_expr()?));
                }
                if !self.eat(&TokenKind::Comma) { break; }
            }
        }
        self.expect(&TokenKind::RParen)?;
        if !self.no_struct_literal && self.check(&TokenKind::LBrace) {
            let lambda = self.parse_trailing_closure()?;
            args.push(Arg::Positional(lambda));
        }
        Ok(args)
    }

    fn parse_trailing_closure(&mut self) -> PResult<Expr> {
        self.expect(&TokenKind::LBrace)?;
        let saved_depth = self.paren_depth;
        self.paren_depth = 0;
        let mut params = Vec::new();
        if matches!(self.peek().kind, TokenKind::Ident(_)) {
            let checkpoint = self.pos;
            let name = self.expect_ident()?;
            if self.eat(&TokenKind::Arrow) {
                params.push(name);
            } else {
                self.pos = checkpoint;
            }
        }
        let mut stmts = Vec::new();
        let mut tail = None;
        while !self.check(&TokenKind::RBrace) {
            let stmt = self.parse_statement()?;
            if self.check(&TokenKind::RBrace) {
                if let Stmt::Expr(e) = &stmt.stmt {
                    tail = Some(Box::new(e.clone()));
                    break;
                }
            }
            stmts.push(stmt);
        }
        self.paren_depth = saved_depth;
        self.expect(&TokenKind::RBrace)?;
        Ok(Expr::Lambda(params, Block { stmts, tail }))
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        match self.peek().kind.clone() {
            TokenKind::IntLiteral(n) => {
                // Located, so a context that fixes the literal's type can name this exact node.
                let start = self.current_span();
                self.advance();
                let end = self.previous_span();
                self.maybe_unit_literal(Expr::Located(Box::new(Expr::IntLiteral(n)), SourceRange { start, end }))
            }
            TokenKind::SizedIntLiteral(n, kind) => {
                self.advance();
                Ok(Expr::SizedIntLiteral(n, kind))
            }
            TokenKind::FloatLiteral(f) => {
                let start = self.current_span();
                self.advance();
                let end = self.previous_span();
                self.maybe_unit_literal(Expr::Located(Box::new(Expr::FloatLiteral(f)), SourceRange { start, end }))
            }
            TokenKind::Float32Literal(f) => {
                self.advance();
                Ok(Expr::Float32Literal(f))
            }
            TokenKind::StringLiteral(s) => { self.advance(); Ok(Expr::StringLiteral(s)) }
            TokenKind::CharLiteral(c) => { self.advance(); Ok(Expr::CharLiteral(c)) }
            TokenKind::True => { self.advance(); Ok(Expr::BoolLiteral(true)) }
            TokenKind::False => { self.advance(); Ok(Expr::BoolLiteral(false)) }
            TokenKind::Ident(name) => {
                self.advance();
                if (name == "Map" || name == "Set") && self.check(&TokenKind::Lt) {
                    self.advance();
                    let mut type_args = Vec::new();
                    loop {
                        type_args.push(self.parse_type()?);
                        if !self.eat(&TokenKind::Comma) { break; }
                    }
                    self.expect(&TokenKind::Gt)?;
                    self.expect(&TokenKind::LParen)?;
                    self.expect(&TokenKind::RParen)?;
                    return Ok(Expr::EmptyCollection(name, type_args));
                }
                if !self.no_struct_literal && self.check(&TokenKind::Lt) {
                    let checkpoint = self.pos;
                    if let Ok(type_args) = self.parse_type_args() {
                        if self.check(&TokenKind::LBrace) && self.looks_like_record_literal() {
                            return self.parse_generic_record_literal(name, type_args);
                        }
                    }
                    self.pos = checkpoint;
                }
                if !self.no_struct_literal && self.check(&TokenKind::LBrace) && self.looks_like_record_literal() {
                    self.parse_record_literal(name)
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            TokenKind::SelfLower => { self.advance(); Ok(Expr::Ident("self".to_string())) }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(inner)
            }
            TokenKind::LBracket => self.parse_bracket_literal(),
            TokenKind::LBrace => {
                self.advance();
                if self.eat(&TokenKind::RBrace) {
                    return Ok(Expr::Block(Block { stmts: vec![], tail: None }));
                }
                let mut items = vec![self.parse_expr()?];
                while self.eat(&TokenKind::Comma) {
                    if self.check(&TokenKind::RBrace) { break; }
                    items.push(self.parse_expr()?);
                }
                self.expect(&TokenKind::RBrace)?;
                Ok(Expr::SetLiteral(items))
            }
            TokenKind::If => self.parse_if_expr(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::Spawn => { self.advance(); Ok(Expr::Spawn(self.parse_block()?)) }
            TokenKind::SpawnScope => { self.advance(); Ok(Expr::SpawnScope(self.parse_block()?)) }
            TokenKind::Channel => {
                self.advance();
                self.expect(&TokenKind::Lt)?;
                let ty = self.parse_type()?;
                self.expect(&TokenKind::Gt)?;
                let args = self.parse_call_args()?;
                let capacity = args.into_iter().find_map(|a| match a {
                    Arg::Named(name, e) if name == "capacity" => Some(Box::new(e)),
                    _ => None,
                });
                Ok(Expr::Channel(ty, capacity))
            }
            TokenKind::Loop => { self.advance(); Ok(Expr::Loop(self.parse_block()?)) }
            TokenKind::Fn => {
                self.advance();
                self.expect(&TokenKind::LParen)?;
                let mut params = Vec::new();
                if !self.check(&TokenKind::RParen) {
                    loop {
                        params.push(self.expect_ident()?);
                        if !self.eat(&TokenKind::Comma) { break; }
                    }
                }
                self.expect(&TokenKind::RParen)?;
                let body = self.parse_block()?;
                Ok(Expr::Lambda(params, body))
            }
            TokenKind::Try => {
                self.advance();
                let inner = self.parse_expr()?;
                let handler = if self.eat(&TokenKind::Catch) {
                    Some(Box::new(self.parse_primary()?))
                } else {
                    None
                };
                Ok(Expr::Try(Box::new(inner), handler))
            }
            _ => Err(self.error("expected an expression")),
        }
    }

    fn parse_bracket_literal(&mut self) -> PResult<Expr> {
        self.expect(&TokenKind::LBracket)?;
        if self.eat(&TokenKind::RBracket) {
            return Ok(Expr::ListLiteral(vec![]));
        }
        let first = self.parse_expr()?;
        if self.eat(&TokenKind::Colon) {
            let first_value = self.parse_expr()?;
            let mut pairs = vec![(first, first_value)];
            while self.eat(&TokenKind::Comma) {
                if self.check(&TokenKind::RBracket) { break; }
                let k = self.parse_expr()?;
                self.expect(&TokenKind::Colon)?;
                let v = self.parse_expr()?;
                pairs.push((k, v));
            }
            self.expect(&TokenKind::RBracket)?;
            return Ok(Expr::MapLiteral(pairs));
        }
        let mut items = vec![first];
        while self.eat(&TokenKind::Comma) {
            if self.check(&TokenKind::RBracket) { break; }
            items.push(self.parse_expr()?);
        }
        self.expect(&TokenKind::RBracket)?;
        Ok(Expr::ListLiteral(items))
    }

    fn parse_if_expr(&mut self) -> PResult<Expr> {
        self.expect(&TokenKind::If)?;
        let cond = self.parse_expr_no_struct()?;
        let then_block = self.parse_block()?;
        let else_block = if self.eat(&TokenKind::Else) {
            if self.check(&TokenKind::If) {
                let nested = self.parse_if_expr()?;
                Some(Block { stmts: vec![], tail: Some(Box::new(nested)) })
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        Ok(Expr::If(Box::new(cond), then_block, else_block))
    }

    fn looks_like_record_literal(&self) -> bool {
        matches!(self.peek_at(1).kind, TokenKind::Ident(_)) && self.check_at(2, &TokenKind::Colon)
    }

    fn parse_record_literal(&mut self, name: String) -> PResult<Expr> {
        self.expect(&TokenKind::LBrace)?;
        let fields = self.parse_record_fields()?;
        Ok(Expr::RecordLiteral(name, fields))
    }

    fn parse_generic_record_literal(&mut self, name: String, type_args: Vec<Type>) -> PResult<Expr> {
        self.expect(&TokenKind::LBrace)?;
        let fields = self.parse_record_fields()?;
        Ok(Expr::GenericRecordLiteral(name, type_args, fields))
    }

    fn parse_record_fields(&mut self) -> PResult<Vec<(String, Expr)>> {
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let fname = self.expect_ident()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_expr()?;
            fields.push((fname, value));
            if !self.eat(&TokenKind::Comma) { break; }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(fields)
    }

    fn parse_match_expr(&mut self) -> PResult<Expr> {
        self.expect(&TokenKind::Match)?;
        let scrutinee = self.parse_expr_no_struct()?;
        self.expect(&TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let pattern = self.parse_pattern()?;
            let guard = if self.eat(&TokenKind::If) { Some(self.parse_expr()?) } else { None };
            self.expect(&TokenKind::FatArrow)?;
            let body = if self.check(&TokenKind::LBrace) {
                self.parse_block()?
            } else {
                let e = self.parse_expr()?;
                Block { stmts: vec![], tail: Some(Box::new(e)) }
            };
            arms.push(MatchArm { pattern, guard, body });
            if !self.eat(&TokenKind::Comma) { break; }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Expr::Match(Box::new(scrutinee), arms))
    }

    fn parse_pattern(&mut self) -> PResult<Pattern> {
        if self.eat(&TokenKind::Underscore) {
            return Ok(Pattern::Wildcard);
        }
        match self.peek().kind.clone() {
            TokenKind::IntLiteral(_) | TokenKind::SizedIntLiteral(..) | TokenKind::FloatLiteral(_) | TokenKind::Float32Literal(_) | TokenKind::StringLiteral(_)
            | TokenKind::CharLiteral(_) | TokenKind::True | TokenKind::False => {
                let lit = self.parse_primary()?;
                if self.eat(&TokenKind::To) {
                    let end = self.parse_primary()?;
                    Ok(Pattern::Range(lit, RangeKind::To, end))
                } else if self.eat(&TokenKind::Until) {
                    let end = self.parse_primary()?;
                    Ok(Pattern::Range(lit, RangeKind::Until, end))
                } else {
                    Ok(Pattern::Literal(lit))
                }
            }
            TokenKind::Ident(name) => {
                self.advance();
                if self.check(&TokenKind::LParen) {
                    self.parse_variant_pattern(name)
                } else {
                    Ok(Pattern::Ident(name))
                }
            }
            _ => Err(self.error("expected a pattern")),
        }
    }

    fn parse_variant_pattern(&mut self, name: String) -> PResult<Pattern> {
        self.expect(&TokenKind::LParen)?;
        let mut fields = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let position = fields.len();
                if self.check(&TokenKind::Underscore)
                    || matches!(
                        self.peek().kind,
                        TokenKind::IntLiteral(_)
                            | TokenKind::SizedIntLiteral(..)
                            | TokenKind::FloatLiteral(_)
                            | TokenKind::Float32Literal(_)
                            | TokenKind::StringLiteral(_)
                            | TokenKind::CharLiteral(_)
                            | TokenKind::True
                            | TokenKind::False
                    )
                {
                    fields.push((format!("@{position}"), self.parse_pattern()?));
                } else {
                    let field_name = self.expect_ident()?;
                    let sub = if self.eat(&TokenKind::Colon) {
                        self.parse_pattern()?
                    } else if self.check(&TokenKind::LParen) {
                        // `Some(Ok(value))`: the identifier is itself the
                        // nested constructor name.
                        self.parse_variant_pattern(field_name.clone())?
                    } else {
                        // The short form `Circle(radius)` is resolved against
                        // the declared field name by the checker/interpreter.
                        Pattern::Ident(field_name.clone())
                    };
                    fields.push((field_name, sub));
                }
                if !self.eat(&TokenKind::Comma) { break; }
            }
        }
        self.expect(&TokenKind::RParen)?;
        Ok(Pattern::Variant(name, fields))
    }

    fn maybe_unit_literal(&mut self, number: Expr) -> PResult<Expr> {
        let number_line = self.tokens[self.pos - 1].line;
        if !matches!(self.peek().kind, TokenKind::Ident(_)) || self.peek().line != number_line {
            return Ok(number);
        }
        let mut unit = self.expect_ident()?;
        self.unit_exponent(&mut unit, number_line)?;
        self.unit_tail(&mut unit, number_line)?;
        Ok(Expr::UnitLiteral(Box::new(number), unit))
    }

    /// `^2` / `^-1` right after a unit atom on the same line.
    fn unit_exponent(&mut self, unit: &mut String, line: usize) -> PResult<()> {
        if self.check(&TokenKind::Caret) && self.peek().line == line {
            self.advance();
            let negative = self.eat(&TokenKind::Minus);
            let exp = self.expect_int()?;
            unit.push('^');
            if negative {
                unit.push('-');
            }
            unit.push_str(&exp.to_string());
        }
        Ok(())
    }

    /// `*atom` / `/atom` continuations of a unit. Only known unit symbols are
    /// absorbed, so `5 m / t` still divides by the variable `t`.
    fn unit_tail(&mut self, unit: &mut String, line: usize) -> PResult<()> {
        loop {
            let op = match self.peek().kind {
                TokenKind::Star => "*",
                TokenKind::Slash => "/",
                _ => break,
            };
            let is_unit = matches!(&self.peek_at(1).kind, TokenKind::Ident(atom) if crate::types::unit_info(atom).is_some());
            if !is_unit || self.peek().line != line || self.peek_at(1).line != line {
                break;
            }
            self.advance();
            let atom = self.expect_ident()?;
            unit.push_str(op);
            unit.push_str(&atom);
            self.unit_exponent(unit, line)?;
        }
        Ok(())
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn current_span(&self) -> Span {
        Span { line: self.peek().line, col: self.peek().col }
    }

    fn previous_span(&self) -> Span {
        let tok = self.tokens.get(self.pos.saturating_sub(1)).unwrap_or_else(|| self.peek());
        Span { line: tok.line, col: tok.col + tok.lexeme.chars().count() }
    }

    fn located(&self, stmt: Stmt, span: Span) -> LocatedStmt {
        LocatedStmt { stmt, span }
    }

    fn peek_at(&self, offset: usize) -> &Token {
        self.tokens.get(self.pos + offset).unwrap_or(&self.tokens[self.tokens.len() - 1])
    }

    fn advance(&mut self) -> &Token {
        match self.tokens[self.pos].kind {
            TokenKind::LParen | TokenKind::LBracket => self.paren_depth += 1,
            TokenKind::RParen | TokenKind::RBracket => self.paren_depth = self.paren_depth.saturating_sub(1),
            _ => {}
        }
        let tok = &self.tokens[self.pos];
        if !matches!(tok.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        tok
    }

    /// Un salto de línea termina una sentencia solo para los operadores
    /// genuinamente ambiguos ('-', '(' de llamada, '[' de índice) y solo
    /// a nivel de sentencia — dentro de un '(' o '[' ya abierto, nunca importa
    /// (documento de referencia: decisión "saltos de línea significativos").
    fn newline_breaks_statement(&self) -> bool {
        self.paren_depth == 0 && self.peek().newline_before
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }

    fn check_at(&self, offset: usize, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek_at(offset).kind) == std::mem::discriminant(kind)
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) { self.advance(); true } else { false }
    }

    fn expect(&mut self, kind: &TokenKind) -> PResult<&Token> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error(&format!("expected {kind:?}, found {:?}", self.peek().kind)))
        }
    }

    fn expect_ident(&mut self) -> PResult<String> {
        match self.peek().kind.clone() {
            TokenKind::Ident(name) => { self.advance(); Ok(name) }
            other => Err(self.error(&format!("expected identifier, found {other:?}"))),
        }
    }

    fn expect_int(&mut self) -> PResult<i64> {
        match self.peek().kind.clone() {
            TokenKind::IntLiteral(n) => { self.advance(); Ok(n) }
            other => Err(self.error(&format!("expected integer literal, found {other:?}"))),
        }
    }

    fn error(&self, message: &str) -> ParseError {
        let tok = self.peek();
        ParseError { message: message.to_string(), line: tok.line, col: tok.col }
    }
}

fn is_assignable_target(expr: &Expr) -> bool {
    match expr {
        Expr::Located(inner, _) => is_assignable_target(inner),
        Expr::FieldAccess(..) | Expr::Index(..) => true,
        _ => false,
    }
}
