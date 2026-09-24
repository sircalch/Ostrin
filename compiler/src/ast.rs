/// Fixed-width integer types (`Int` is `Int64`, so it has no entry here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntKind {
    I8,
    I16,
    I32,
    U8,
    U16,
    U32,
    U64,
}

impl IntKind {
    pub fn name(self) -> &'static str {
        match self {
            IntKind::I8 => "Int8",
            IntKind::I16 => "Int16",
            IntKind::I32 => "Int32",
            IntKind::U8 => "UInt8",
            IntKind::U16 => "UInt16",
            IntKind::U32 => "UInt32",
            IntKind::U64 => "UInt64",
        }
    }

    pub fn from_name(name: &str) -> Option<IntKind> {
        Some(match name {
            "Int8" => IntKind::I8,
            "Int16" => IntKind::I16,
            "Int32" => IntKind::I32,
            "UInt8" => IntKind::U8,
            "UInt16" => IntKind::U16,
            "UInt32" => IntKind::U32,
            "UInt64" => IntKind::U64,
            _ => return None,
        })
    }

    /// `8u`-style literal suffixes: `i8`, `i16`, `i32`, `u8`, `u16`, `u32`, `u64`.
    pub fn from_suffix(suffix: &str) -> Option<IntKind> {
        Some(match suffix {
            "i8" => IntKind::I8,
            "i16" => IntKind::I16,
            "i32" => IntKind::I32,
            "u8" => IntKind::U8,
            "u16" => IntKind::U16,
            "u32" => IntKind::U32,
            "u64" => IntKind::U64,
            _ => return None,
        })
    }

    pub fn min(self) -> i128 {
        match self {
            IntKind::I8 => i8::MIN as i128,
            IntKind::I16 => i16::MIN as i128,
            IntKind::I32 => i32::MIN as i128,
            _ => 0,
        }
    }

    pub fn max(self) -> i128 {
        match self {
            IntKind::I8 => i8::MAX as i128,
            IntKind::I16 => i16::MAX as i128,
            IntKind::I32 => i32::MAX as i128,
            IntKind::U8 => u8::MAX as i128,
            IntKind::U16 => u16::MAX as i128,
            IntKind::U32 => u32::MAX as i128,
            IntKind::U64 => u64::MAX as i128,
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(self, IntKind::I8 | IntKind::I16 | IntKind::I32)
    }

    pub fn fits(self, value: i128) -> bool {
        value >= self.min() && value <= self.max()
    }

    pub fn c_type(self) -> &'static str {
        match self {
            IntKind::I8 => "int8_t",
            IntKind::I16 => "int16_t",
            IntKind::I32 => "int32_t",
            IntKind::U8 => "uint8_t",
            IntKind::U16 => "uint16_t",
            IntKind::U32 => "uint32_t",
            IntKind::U64 => "uint64_t",
        }
    }
}

/// What an untyped numeric literal was resolved to by its context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LitKind {
    Int(IntKind),
    F32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Named(String, Vec<Type>),
    Mul(Box<Type>, Box<Type>),
    Div(Box<Type>, Box<Type>),
    Pow(Box<Type>, i64),
    Fn(Vec<Type>, Box<Type>),
    Dyn(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub name: String,
    pub bounds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub default: Option<Expr>,
    pub is_mut: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDecl {
    pub name: String,
    pub is_pub: bool,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: Type,
    pub body: Block,
    pub span: Span,
    pub source_file: Option<String>,
}

/// Posición de origen conservada en el AST para que las herramientas puedan
/// señalar el lugar donde nació un diagnóstico. Las posiciones son 1-based,
/// igual que las que ya expone el lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceRange {
    pub start: Span,
    pub end: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub is_pub: bool,
    pub is_mut: bool,
    pub ty: Type,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordDecl {
    pub name: String,
    pub module_path: Vec<String>,
    pub is_pub: bool,
    pub generics: Vec<GenericParam>,
    pub derives: Vec<String>,
    pub fields: Vec<FieldDecl>,
    pub span: Span,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantFieldDecl {
    pub name: Option<String>,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariantDecl {
    pub name: String,
    pub fields: Vec<VariantFieldDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub module_path: Vec<String>,
    pub is_pub: bool,
    pub generics: Vec<GenericParam>,
    pub derives: Vec<String>,
    pub variants: Vec<VariantDecl>,
    pub span: Span,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImplDecl {
    pub generics: Vec<GenericParam>,
    pub trait_name: Option<String>,
    pub trait_args: Vec<Type>,
    pub type_name: String,
    pub type_args: Vec<Type>,
    pub module_path: Vec<String>,
    pub methods: Vec<FunctionDecl>,
    pub span: Span,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethodSig {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: Type,
    pub default_body: Option<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitDecl {
    pub name: String,
    pub module_path: Vec<String>,
    pub is_pub: bool,
    pub generics: Vec<GenericParam>,
    pub supertraits: Vec<String>,
    pub methods: Vec<TraitMethodSig>,
    pub span: Span,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub is_pub: bool,
    pub path: Vec<String>,
    pub alias: Option<String>,
    pub names: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FunctionDecl),
    Record(RecordDecl),
    Enum(EnumDecl),
    Impl(ImplDecl),
    Import(ImportDecl),
    Trait(TraitDecl),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<LocatedStmt>,
    pub tail: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocatedStmt {
    pub stmt: Stmt,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Binding {
        mut_: bool,
        name: String,
        ty: Option<Type>,
        value: Expr,
    },
    Assign {
        name: String,
        value: Expr,
    },
    Return(Option<Expr>),
    Break(Option<Expr>),
    Continue,
    For {
        pattern: String,
        iter: Expr,
        body: Block,
    },
    While {
        cond: Expr,
        body: Block,
    },
    FieldAssign {
        target: Expr,
        value: Expr,
    },
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard,
    Ident(String),
    Literal(Expr),
    Range(Expr, RangeKind, Expr),
    Variant(String, Vec<(String, Pattern)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Positional(Expr),
    Named(String, Expr),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RangeKind {
    To,
    Until,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Preserves the source position of an expression for diagnostics and
    /// editor tooling. The wrapped expression remains semantically unchanged.
    Located(Box<Expr>, SourceRange),
    IntLiteral(i64),
    /// `200u8`, `5i32`: an integer literal with an explicit fixed width.
    SizedIntLiteral(i128, IntKind),
    FloatLiteral(f64),
    /// `2.5f32`: a single-precision literal.
    Float32Literal(f32),
    StringLiteral(String),
    CharLiteral(char),
    BoolLiteral(bool),
    UnitLiteral(Box<Expr>, String),
    Ident(String),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Range(Box<Expr>, RangeKind, Box<Expr>, Option<Box<Expr>>),
    Call(Box<Expr>, Vec<Arg>),
    GenericCall(Box<Expr>, Vec<Type>, Vec<Arg>),
    FieldAccess(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    If(Box<Expr>, Block, Option<Block>),
    Block(Block),
    Lambda(Vec<String>, Block),
    ListLiteral(Vec<Expr>),
    SetLiteral(Vec<Expr>),
    MapLiteral(Vec<(Expr, Expr)>),
    /// `Map<K, V>()` / `Set<T>()`: an empty collection that keeps its written type arguments.
    EmptyCollection(String, Vec<Type>),
    Try(Box<Expr>, Option<Box<Expr>>),
    Within(Box<Expr>, Box<Expr>),
    Approximately(Box<Expr>, Box<Expr>, Box<Expr>),
    As(Box<Expr>, Box<Expr>),
    Loop(Block),
    RecordLiteral(String, Vec<(String, Expr)>),
    GenericRecordLiteral(String, Vec<Type>, Vec<(String, Expr)>),
    Match(Box<Expr>, Vec<MatchArm>),
    Spawn(Block),
    SpawnScope(Block),
    Channel(Type, Option<Box<Expr>>),
}

impl Expr {
    /// Returns the semantic expression after removing source-location
    /// wrappers. This keeps older compiler/runtime logic independent from the
    /// metadata used by diagnostics and editor tooling.
    pub fn unlocated(&self) -> &Expr {
        match self {
            Expr::Located(inner, _) => inner.unlocated(),
            other => other,
        }
    }
}
