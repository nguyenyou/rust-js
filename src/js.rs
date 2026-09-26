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
    /// Imports from JS modules, from `#[link_name = "module#path"]` (ADR 0028).
    pub packages: Vec<Package>,
    /// `import * as <alias> from "<from>"`, one per module this one calls into.
    pub imports: Vec<Import>,
    /// Runtime helpers this module uses, as JS source.
    pub runtime: Vec<&'static str>,
    /// `const` items, with the values rustc computed (ADR 0031).
    pub consts: Vec<Const>,
    pub functions: Vec<Function>,
}

/// `const SIZE = 4096;`, maybe exported.
pub struct Const {
    pub name: String,
    pub value: Expr,
    pub export: bool,
    /// The whole `const` item.
    pub span: Span,
}

pub struct Import {
    pub alias: String,
    /// A relative specifier, like `./math.js` or `../lib.js`.
    pub from: String,
}

/// What one file imports from one JS module: its default export, named
/// exports as `(export, local)`, and the module itself.
pub struct Package {
    pub from: String,
    pub default: Option<String>,
    pub named: Vec<(String, String)>,
    pub namespace: Option<String>,
}

pub struct Function {
    pub name: String,
    pub params: Vec<Pattern>,
    pub body: Vec<Stmt>,
    pub export: bool,
    /// `async function`: an `async fn` (ADR 0029).
    pub is_async: bool,
    /// The whole `fn` item, and just its name.
    pub span: Span,
    pub name_span: Span,
}

/// What a parameter or declaration binds: a variable, or the parts of an
/// array or object, `[count, setCount]` or `{ initial, label }`.
#[derive(Clone)]
pub enum Pattern {
    Name(String),
    /// `None` skips an element: `[, b]`.
    Array(Vec<Option<String>>),
    /// Each field, and the variable it goes in.
    Object(Vec<(String, String)>),
}

impl From<String> for Pattern {
    fn from(name: String) -> Pattern {
        Pattern::Name(name)
    }
}

impl From<&str> for Pattern {
    fn from(name: &str) -> Pattern {
        Pattern::Name(name.to_string())
    }
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
    /// `const [a, b] = value;`, or `let` if one of them is reassigned.
    Destructure { pattern: Pattern, value: Expr, mutable: bool },
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
    /// `throw new Error(..)`: a panic (ADR 0012).
    Throw(Expr),
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
    /// Only to test against: `o != null` (ADR 0030).
    Null,
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
    Arrow(Vec<Pattern>, Vec<Stmt>),
    /// `async (a) => { .. }`: an async closure or block (ADR 0029).
    AsyncArrow(Vec<Pattern>, Vec<Stmt>),
    /// `await p`: `.await` (ADR 0029).
    Await(Box<Expr>),
    /// `<div className="hero">..</div>`, `<Counter initial={1} />` or `<>..</>` (ADR 0040).
    Jsx(Box<Jsx>),
}

/// A JSX element (ADR 0040).
#[derive(Clone)]
pub struct Jsx {
    pub tag: JsxTag,
    /// Its attributes, `className="hero"`, or `{...props}`.
    pub props: Vec<Prop>,
    pub children: Vec<Expr>,
}

#[derive(Clone)]
pub enum JsxTag {
    /// `<>`.
    Fragment,
    /// A DOM element: `<div>`.
    Intrinsic(String),
    /// A component: `<Counter>`, `<StrictMode>`, `<stats.Chart>`.
    Component(Expr),
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
    /// `a ?? b`: `unwrap_or` (ADR 0030).
    Coalesce,
    BitOr,
    BitXor,
    BitAnd,
    Eq,
    Ne,
    /// `==` and `!=`: only against `null`, for `None` (ADR 0030), and on
    /// options, where `null` and `undefined` are both `None`.
    LooseEq,
    LooseNe,
    /// `x instanceof C`: a `#[link_name = "instanceof C"]` binding.
    InstanceOf,
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

    pub fn null() -> Expr {
        Expr::new(ExprKind::Null)
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
        // `!(a === b)` is `a !== b`, for every `a` and `b`.
        if let (UnaryOp::Not, ExprKind::Binary(eq @ (Op::Eq | Op::Ne | Op::LooseEq | Op::LooseNe), a, b)) = (op, &arg.kind) {
            let ne = match eq {
                Op::Eq => Op::Ne,
                Op::Ne => Op::Eq,
                Op::LooseEq => Op::LooseNe,
                _ => Op::LooseEq,
            };
            return Expr { kind: ExprKind::Binary(ne, a.clone(), b.clone()), span: arg.span };
        }
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

    pub fn arrow(params: Vec<Pattern>, body: Vec<Stmt>) -> Expr {
        Expr::new(ExprKind::Arrow(params, body))
    }

    pub fn async_arrow(params: Vec<Pattern>, body: Vec<Stmt>) -> Expr {
        Expr::new(ExprKind::AsyncArrow(params, body))
    }

    pub fn await_(promise: Expr) -> Expr {
        Expr::new(ExprKind::Await(Box::new(promise)))
    }

    pub fn jsx(jsx: Jsx) -> Expr {
        Expr::new(ExprKind::Jsx(Box::new(jsx)))
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
            ExprKind::Num(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Undefined | ExprKind::Null
        )
    }

    /// Does oxc print this on several lines: an array of 3 or more items, an
    /// object of 2 or more fields, or a function with statements? JSX is laid
    /// out by us (ADR 0040), so it doesn't count, but what's in it does.
    pub fn prints_on_lines(&self) -> bool {
        match &self.kind {
            ExprKind::Array(items) => items.len() > 2 || items.iter().any(Expr::prints_on_lines),
            ExprKind::Object(props) => {
                props.len() > 1
                    || props.iter().any(|p| match p {
                        Prop::Field(_, value) | Prop::Spread(value) => value.prints_on_lines(),
                    })
            }
            ExprKind::Arrow(_, body) | ExprKind::AsyncArrow(_, body) => match body.as_slice() {
                [Stmt { kind: StmtKind::Return(Some(value)), .. }] => value.prints_on_lines(),
                _ => true,
            },
            ExprKind::Member(a, _) | ExprKind::Unary(_, a) | ExprKind::Await(a) => a.prints_on_lines(),
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => a.prints_on_lines() || b.prints_on_lines(),
            ExprKind::Cond(a, b, c) => a.prints_on_lines() || b.prints_on_lines() || c.prints_on_lines(),
            ExprKind::Call(f, args) | ExprKind::New(f, args) => f.prints_on_lines() || args.iter().any(Expr::prints_on_lines),
            ExprKind::Jsx(jsx) => {
                jsx.props.iter().any(|p| match p {
                    Prop::Field(_, value) | Prop::Spread(value) => value.prints_on_lines(),
                }) || jsx.children.iter().any(Expr::prints_on_lines)
            }
            ExprKind::Num(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Undefined
            | ExprKind::Null
            | ExprKind::Var(_) => false,
        }
    }

    /// Is there JSX in this, like `items.map((t) => <li>..</li>)`?
    pub fn contains_jsx(&self) -> bool {
        match &self.kind {
            ExprKind::Jsx(_) => true,
            ExprKind::Array(items) => items.iter().any(Expr::contains_jsx),
            ExprKind::Object(props) => props.iter().any(|p| match p {
                Prop::Field(_, value) | Prop::Spread(value) => value.contains_jsx(),
            }),
            ExprKind::Arrow(_, body) | ExprKind::AsyncArrow(_, body) => body.iter().any(|s| match &s.kind {
                StmtKind::Return(Some(value)) | StmtKind::Expr(value) => value.contains_jsx(),
                _ => false,
            }),
            ExprKind::Member(a, _) | ExprKind::Unary(_, a) | ExprKind::Await(a) => a.contains_jsx(),
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => a.contains_jsx() || b.contains_jsx(),
            ExprKind::Cond(a, b, c) => a.contains_jsx() || b.contains_jsx() || c.contains_jsx(),
            ExprKind::Call(f, args) | ExprKind::New(f, args) => f.contains_jsx() || args.iter().any(Expr::contains_jsx),
            ExprKind::Num(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Undefined
            | ExprKind::Null
            | ExprKind::Var(_) => false,
        }
    }

    /// Could evaluating this do something observable (call a function, throw)?
    pub fn has_effects(&self) -> bool {
        match &self.kind {
            ExprKind::Num(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::Undefined
            | ExprKind::Null
            | ExprKind::Var(_)
            | ExprKind::Arrow(..)
            | ExprKind::AsyncArrow(..) => false,
            // It lets other code run meanwhile.
            ExprKind::Await(_) => true,
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
            // Making an element runs nothing: a component runs when React renders it.
            ExprKind::Jsx(jsx) => {
                matches!(&jsx.tag, JsxTag::Component(c) if c.has_effects())
                    || jsx.props.iter().any(|p| match p {
                        Prop::Field(_, value) | Prop::Spread(value) => value.has_effects(),
                    })
                    || jsx.children.iter().any(Expr::has_effects)
            }
        }
    }
}
