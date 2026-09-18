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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    Binding { mut_: bool, name: String, ty: Option<Type>, value: Expr },
    Assign { name: String, value: Expr },
    Return(Option<Expr>),
    Break(Option<Expr>),
    Continue,
    For { pattern: String, iter: Expr, body: Block },
    While { cond: Expr, body: Block },
    FieldAssign { target: Expr, value: Expr },
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
    FloatLiteral(f64),
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
