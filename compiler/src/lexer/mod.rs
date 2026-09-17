mod token;

pub use token::{Token, TokenKind};
use token::keyword_from_str;

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer<'a> {
    _source: &'a str,
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    last_token_start: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Lexer {
            _source: source,
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            last_token_start: 0,
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let newline_before = self.skip_whitespace_and_comments();
            let (line, col) = (self.line, self.col);
            let Some(c) = self.peek() else {
                tokens.push(Token { kind: TokenKind::Eof, lexeme: String::new(), line, col, newline_before });
                break;
            };

            let kind = if c.is_ascii_digit() {
                self.lex_number()?
            } else if c == '"' {
                self.lex_string()?
            } else if c == '\'' {
                self.lex_char()?
            } else if is_ident_start(c) {
                self.lex_ident_or_keyword()
            } else {
                self.lex_operator_or_punct()?
            };

            let lexeme: String = self.chars[self.token_start(line, col)..self.pos].iter().collect();
            tokens.push(Token { kind, lexeme, line, col, newline_before });
        }
        Ok(tokens)
    }

    fn token_start(&self, _line: usize, _col: usize) -> usize {
        self.last_token_start
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_whitespace_and_comments(&mut self) -> bool {
        let mut saw_newline = false;
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    if c == '\n' { saw_newline = true; }
                    self.advance();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' { break; }
                        self.advance();
                    }
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    self.advance();
                    self.advance();
                    loop {
                        match self.peek() {
                            None => break,
                            Some('*') if self.peek_at(1) == Some('/') => {
                                self.advance();
                                self.advance();
                                break;
                            }
                            Some('\n') => { saw_newline = true; self.advance(); }
                            _ => { self.advance(); }
                        }
                    }
                }
                _ => break,
            }
        }
        saw_newline
    }

    fn lex_number(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '_') {
            self.advance();
        }
        let mut is_float = false;
        if self.peek() == Some('.') && matches!(self.peek_at(1), Some(c) if c.is_ascii_digit()) {
            is_float = true;
            self.advance();
            while matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '_') {
                self.advance();
            }
        }
        self.last_token_start = start;
        let text: String = self.chars[start..self.pos].iter().filter(|c| **c != '_').collect();
        if is_float {
            let value: f64 = text.parse().map_err(|_| self.error("invalid float literal"))?;
            Ok(TokenKind::FloatLiteral(value))
        } else {
            let value: i64 = text.parse().map_err(|_| self.error("invalid integer literal"))?;
            Ok(TokenKind::IntLiteral(value))
        }
    }

    fn lex_string(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        self.last_token_start = start;
        self.advance();
        let mut value = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated string literal")),
                Some('"') => { self.advance(); break; }
                Some('\\') => {
                    self.advance();
                    value.push(self.lex_escape()?);
                }
                Some(c) => { value.push(c); self.advance(); }
            }
        }
        Ok(TokenKind::StringLiteral(value))
    }

    fn lex_char(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        self.last_token_start = start;
        self.advance();
        let c = match self.peek() {
            None => return Err(self.error("unterminated char literal")),
            Some('\\') => { self.advance(); self.lex_escape()? }
            Some(c) => { self.advance(); c }
        };
        if self.peek() != Some('\'') {
            return Err(self.error("unterminated char literal"));
        }
        self.advance();
        Ok(TokenKind::CharLiteral(c))
    }

    fn lex_escape(&mut self) -> Result<char, LexError> {
        match self.advance() {
            Some('n') => Ok('\n'),
            Some('t') => Ok('\t'),
            Some('\\') => Ok('\\'),
            Some('"') => Ok('"'),
            Some('\'') => Ok('\''),
            Some('u') => {
                if self.peek() != Some('{') {
                    return Err(self.error("expected '{' after \\u"));
                }
                self.advance();
                let mut hex = String::new();
                while matches!(self.peek(), Some(c) if c != '}') {
                    hex.push(self.advance().unwrap());
                }
                if self.peek() != Some('}') {
                    return Err(self.error("unterminated unicode escape"));
                }
                self.advance();
                let code = u32::from_str_radix(&hex, 16).map_err(|_| self.error("invalid unicode escape"))?;
                char::from_u32(code).ok_or_else(|| self.error("invalid unicode code point"))
            }
            _ => Err(self.error("invalid escape sequence")),
        }
    }

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        self.last_token_start = start;
        while matches!(self.peek(), Some(c) if is_ident_continue(c)) {
            self.advance();
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        keyword_from_str(&text).unwrap_or(TokenKind::Ident(text))
    }

    fn lex_operator_or_punct(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        self.last_token_start = start;
        let c = self.advance().unwrap();
        use TokenKind::*;
        let kind = match c {
            '+' => Plus,
            '-' => if self.peek() == Some('>') { self.advance(); Arrow } else { Minus },
            '*' => Star,
            '/' => Slash,
            '^' => Caret,
            '%' => Percent,
            '|' => Pipe,
            '&' => Amp,
            '=' => {
                if self.peek() == Some('=') { self.advance(); EqEq }
                else if self.peek() == Some('>') { self.advance(); FatArrow }
                else { Eq }
            }
            '!' => {
                if self.peek() == Some('=') { self.advance(); NotEq }
                else { return Err(self.error("unexpected character '!'")); }
            }
            '<' => if self.peek() == Some('=') { self.advance(); LtEq } else { Lt },
            '>' => if self.peek() == Some('=') { self.advance(); GtEq } else { Gt },
            ':' => if self.peek() == Some(':') { self.advance(); ColonColon } else { Colon },
            ',' => Comma,
            '.' => Dot,
            '(' => LParen,
            ')' => RParen,
            '{' => LBrace,
            '}' => RBrace,
            '[' => LBracket,
            ']' => RBracket,
            other => return Err(self.error(&format!("unexpected character '{other}'"))),
        };
        Ok(kind)
    }

    fn error(&self, message: &str) -> LexError {
        LexError { message: message.to_string(), line: self.line, col: self.col }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
