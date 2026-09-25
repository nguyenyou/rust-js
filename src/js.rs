//! A tiny JavaScript AST: exactly the constructs rust-js emits.
//!
//! `lower.rs` builds it; `to_oxc.rs` converts it to oxc's AST, which prints
//! it (with correct parentheses) and builds the source map. Keeping our own
//! small tree means the lowering never touches oxc's large, fast-changing API.
//!
//! Every node carries a `Span`: byte offsets into the Rust source file. That
//! is what lets the source map point from JS back to Rust.

/// Byte offsets `lo..hi` into the Rust source file. `Span::NONE` (empty)
/// means "no mapping": oxc skips empty spans when building the source map.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Span {
    pub lo: u32,
    pub hi: u32,
}

impl Span {
    pub const NONE: Span = Span { lo: 0, hi: 0 };

    pub fn is_none(self) -> bool {
        self.lo == self.hi
    }
}

pub struct Module {
    pub header: String,
    /// `import * as <alias> from "<from>"`, one per module this one calls into.
    pub imports: Vec<Import>,
    /// Runtime helpers this module uses, as JS source.
    pub runtime: Vec<&'static str>,
    pub functions: Vec<Function>,
}

pub struct Import {
    pub alias: String,
    /// A relative specifier, like `./math.js` or `../lib.js`.
    pub from: String,
}

pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub export: bool,
    /// The whole `fn` item, and just its name.
    pub span: Span,
    pub name_span: Span,
}

#[derive(Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Clone)]
pub enum StmtKind {
    Const(String, Expr),
    Let(String, Option<Expr>),
    /// `target = value`, where `target` is a variable, `a.b` or `a[0]`.
    Assign(Expr, Expr),
    Expr(Expr),
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    While { label: Option<String>, cond: Expr, body: Vec<Stmt> },
    /// `for (const name of iterable) { .. }`: a `for` over a sequence (ADR 0025).
    ForOf { label: Option<String>, name: String, iterable: Expr, body: Vec<Stmt> },
    /// `for (let name = start; test; name++) { .. }`: a `for` over a range.
    For { label: Option<String>, name: String, start: Expr, test: Expr, body: Vec<Stmt> },
    Break(Option<String>),
    Continue(Option<String>),
    Return(Option<Expr>),
}

impl StmtKind {
    pub fn at(self, span: Span) -> Stmt {
        Stmt { kind: self, span }
    }
}

#[derive(Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Clone)]
pub enum ExprKind {
    Num(f64),
    Bool(bool),
    Str(String),
    Undefined,
    Var(String),
    /// `object.property`, e.g. `Math.imul` or `math.add`.
    Member(Box<Expr>, String),
    /// `object[index]`, e.g. `pair[0]`.
    Index(Box<Expr>, Box<Expr>),
    /// `[a, b]`: a tuple or tuple struct (ADR 0020).
    Array(Vec<Expr>),
    /// `{ x: a, y: b }`: a struct (ADR 0020).
    Object(Vec<Prop>),
    Unary(UnaryOp, Box<Expr>),
    Binary(Op, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    /// `new Event(t)`: a JS constructor (ADR 0024).
    New(Box<Expr>, Vec<Expr>),
    /// `(a, b) => { .. }`: a closure (ADR 0022).
    Arrow(Vec<String>, Vec<Stmt>),
}

#[derive(Clone)]
pub enum Prop {
    /// `name: value`, printed as `name` when `value` is a variable of that name.
    Field(String, Expr),
    /// `...value`: copy every field of `value`.
    Spread(Expr),
}

#[derive(Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Clone, Copy)]
pub enum Op {
    Or,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Shl,
    Shr,
    UShr,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl Expr {
    fn new(kind: ExprKind) -> Expr {
        Expr { kind, span: Span::NONE }
    }

    pub fn num(n: impl Into<f64>) -> Expr {
        Expr::new(ExprKind::Num(n.into()))
    }

    pub fn int(n: i128) -> Expr {
        // Every integer rust-js supports fits in 32 bits, so this is exact.
        Expr::num(n as f64)
    }

    pub fn bool(b: bool) -> Expr {
        Expr::new(ExprKind::Bool(b))
    }

    pub fn str(s: impl Into<String>) -> Expr {
        Expr::new(ExprKind::Str(s.into()))
    }

    pub fn undefined() -> Expr {
        Expr::new(ExprKind::Undefined)
    }

    pub fn var(name: &str) -> Expr {
        Expr::new(ExprKind::Var(name.to_string()))
    }

    pub fn member(object: Expr, property: impl Into<String>) -> Expr {
        Expr::new(ExprKind::Member(Box::new(object), property.into()))
    }

    pub fn index(object: Expr, index: Expr) -> Expr {
        Expr::new(ExprKind::Index(Box::new(object), Box::new(index)))
    }

    pub fn array(items: Vec<Expr>) -> Expr {
        Expr::new(ExprKind::Array(items))
    }

    pub fn object(props: Vec<Prop>) -> Expr {
        Expr::new(ExprKind::Object(props))
    }

    pub fn unary(op: UnaryOp, arg: Expr) -> Expr {
        Expr::new(ExprKind::Unary(op, Box::new(arg)))
    }

    pub fn bin(op: Op, lhs: Expr, rhs: Expr) -> Expr {
        Expr::new(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)))
    }

    pub fn cond(test: Expr, then: Expr, els: Expr) -> Expr {
        Expr::new(ExprKind::Cond(Box::new(test), Box::new(then), Box::new(els)))
    }

    pub fn new_(callee: Expr, args: Vec<Expr>) -> Expr {
        Expr::new(ExprKind::New(Box::new(callee), args))
    }

    pub fn arrow(params: Vec<String>, body: Vec<Stmt>) -> Expr {
        Expr::new(ExprKind::Arrow(params, body))
    }

    pub fn call(callee: Expr, args: Vec<Expr>) -> Expr {
        Expr::new(ExprKind::Call(Box::new(callee), args))
    }

    /// Give this node a span, unless it already has one. Lowering calls
    /// this on every result, so the outermost node made for a Rust
    /// expression gets that expression's span.
    pub fn or_at(mut self, span: Span) -> Expr {
        if self.span.is_none() {
            self.span = span;
        }
        self
    }

    /// The integer value, if this is an integer literal.
    pub fn as_int(&self) -> Option<i128> {
        match self.kind {
            ExprKind::Num(n) if n.fract() == 0.0 => Some(n as i128),
            _ => None,
        }
    }

    /// Literals can be evaluated at any time, so they never need a temporary.
    pub fn is_constant(&self) -> bool {
        matches!(
            self.kind,
            ExprKind::Num(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Undefined
        )
    }

    /// Could evaluating this do something observable (call a function, throw)?
    pub fn has_effects(&self) -> bool {
        match &self.kind {
            ExprKind::Num(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Undefined
            | ExprKind::Var(_)
            | ExprKind::Arrow(..) => false,
            ExprKind::Member(object, _) => object.has_effects(),
            ExprKind::Index(object, index) => object.has_effects() || index.has_effects(),
            ExprKind::Array(items) => items.iter().any(Expr::has_effects),
            ExprKind::Object(props) => props.iter().any(|p| match p {
                Prop::Field(_, value) | Prop::Spread(value) => value.has_effects(),
            }),
            ExprKind::Unary(_, a) => a.has_effects(),
            ExprKind::Binary(_, a, b) => a.has_effects() || b.has_effects(),
            ExprKind::Cond(a, b, c) => a.has_effects() || b.has_effects() || c.has_effects(),
            ExprKind::Call(..) | ExprKind::New(..) => true,
        }
    }
}
