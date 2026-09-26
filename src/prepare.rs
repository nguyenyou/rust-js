//! Readability preparation on the completed JS tree. No rustc types or APIs.
//! Expression movement is confined to an evaluation region; conditional and
//! repeated evaluations are barriers. The printer only lays out the result.

use crate::js::{self, Expr, ExprKind, JsxTag, Pattern, Prop, Stmt, StmtKind};
use std::collections::HashSet;

/// Does oxc print this on several lines: an array of 3 or more items, an
/// object of 2 or more fields, or a function with statements? JSX is laid
/// out by us (ADR 0040), so it doesn't count, but what's in it does.
fn prints_on_lines(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Array(items) => items.len() > 2 || items.iter().any(prints_on_lines),
        ExprKind::Object(props) => {
            props.len() > 1
                || props.iter().any(|p| match p {
                    Prop::Field(_, value) | Prop::Spread(value) => prints_on_lines(value),
                })
        }
        ExprKind::Arrow(_, body) | ExprKind::AsyncArrow(_, body) => match body.as_slice() {
            [
                Stmt {
                    kind: StmtKind::Return(Some(value)),
                    ..
                },
            ] => prints_on_lines(value),
            _ => true,
        },
        ExprKind::Member(a, _) | ExprKind::Unary(_, a) | ExprKind::Await(a) => prints_on_lines(a),
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => prints_on_lines(a) || prints_on_lines(b),
        ExprKind::Cond(a, b, c) => prints_on_lines(a) || prints_on_lines(b) || prints_on_lines(c),
        ExprKind::Call(f, args) | ExprKind::New(f, args) => prints_on_lines(f) || args.iter().any(prints_on_lines),
        ExprKind::Jsx(jsx) => {
            jsx.props.iter().any(|p| match p {
                Prop::Field(_, value) | Prop::Spread(value) => prints_on_lines(value),
            }) || jsx.children.iter().any(prints_on_lines)
        }
        ExprKind::Num(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Undefined
        | ExprKind::Null
        | ExprKind::Var(_) => false,
    }
}

pub fn module(module: &mut js::Module) {
    let mut names: HashSet<String> = module
        .functions
        .iter()
        .map(|f| f.name.clone())
        .chain(module.consts.iter().map(|c| c.name.clone()))
        .chain(module.namespaces.iter().map(|n| n.name.clone()))
        .chain(module.imports.iter().map(|i| i.alias.clone()))
        .collect();
    for package in &module.packages {
        names.extend(package.default.iter().chain(&package.namespace).cloned());
        names.extend(package.named.iter().map(|(_, name)| name.clone()));
    }
    let methods = module.namespaces.iter_mut().flat_map(|n| n.methods.iter_mut());
    for function in module.functions.iter_mut().chain(methods) {
        scope(&function.params, &mut function.body, &names);
    }
    // Top-level values can contain closures with JSX. Don't move their own
    // evaluations into another declaration or change module initialization.
    for constant in &mut module.consts {
        let mut cx = Context { names: names.clone() };
        cx.expr(&mut constant.value, &mut Vec::new(), true);
    }
}

struct Context {
    names: HashSet<String>,
}

fn scope(params: &[Pattern], body: &mut Vec<Stmt>, outer: &HashSet<String>) {
    let mut cx = Context { names: outer.clone() };
    for pattern in params {
        cx.pattern(pattern);
    }
    // Reserve names before introducing temporaries, including captured names.
    // Include nested statements so introduced names cannot shadow captures.
    for stmt in body.iter() {
        cx.reserve_stmt(stmt);
    }
    cx.block(body);
}

impl Context {
    fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Name(name) => {
                self.names.insert(name.clone());
            }
            Pattern::Array(items) => self.names.extend(items.iter().flatten().cloned()),
            Pattern::Object(fields) => self.names.extend(fields.iter().map(|(_, name)| name.clone())),
        }
    }
    fn reserve_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Const(name, e) => {
                self.names.insert(name.clone());
                self.reserve(e);
            }
            StmtKind::Let(name, e) => {
                self.names.insert(name.clone());
                if let Some(e) = e {
                    self.reserve(e);
                }
            }
            StmtKind::Destructure { pattern, value, .. } => {
                self.pattern(pattern);
                self.reserve(value);
            }
            StmtKind::Assign(a, b) => {
                self.reserve(a);
                self.reserve(b);
            }
            StmtKind::Expr(e) | StmtKind::Throw(e) | StmtKind::Return(Some(e)) => self.reserve(e),
            StmtKind::If(e, a, b) => {
                self.reserve(e);
                for s in a.iter().chain(b.iter().flatten()) {
                    self.reserve_stmt(s);
                }
            }
            StmtKind::While { cond, body, .. } => {
                self.reserve(cond);
                for s in body {
                    self.reserve_stmt(s);
                }
            }
            StmtKind::Labeled(_, body) => {
                for s in body {
                    self.reserve_stmt(s);
                }
            }
            StmtKind::ForOf {
                name, iterable, body, ..
            } => {
                self.names.insert(name.clone());
                self.reserve(iterable);
                for s in body {
                    self.reserve_stmt(s);
                }
            }
            StmtKind::For {
                name,
                start,
                test,
                body,
                ..
            } => {
                self.names.insert(name.clone());
                self.reserve(start);
                self.reserve(test);
                for s in body {
                    self.reserve_stmt(s);
                }
            }
            StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => {}
        }
    }
    fn reserve(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Var(name) => {
                self.names.insert(name.clone());
            }
            ExprKind::Member(a, _) | ExprKind::Unary(_, a) | ExprKind::Await(a) => self.reserve(a),
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
                self.reserve(a);
                self.reserve(b);
            }
            ExprKind::Cond(a, b, c) => {
                self.reserve(a);
                self.reserve(b);
                self.reserve(c);
            }
            ExprKind::Call(f, args) | ExprKind::New(f, args) => {
                self.reserve(f);
                for a in args {
                    self.reserve(a);
                }
            }
            ExprKind::Array(items) => {
                for e in items {
                    self.reserve(e);
                }
            }
            ExprKind::Object(props) => {
                for p in props {
                    self.reserve(prop_value(p));
                }
            }
            ExprKind::Jsx(jsx) => {
                if let JsxTag::Component(e) = &jsx.tag {
                    self.reserve(e);
                }
                for p in &jsx.props {
                    self.reserve(prop_value(p));
                }
                for e in &jsx.children {
                    self.reserve(e);
                }
            }
            ExprKind::Arrow(_, body) | ExprKind::AsyncArrow(_, body) => {
                for s in body {
                    self.reserve_stmt(s);
                }
            }
            _ => {}
        }
    }
    fn block(&mut self, body: &mut Vec<Stmt>) {
        let mut next = Vec::new();
        for mut stmt in std::mem::take(body) {
            match &mut stmt.kind {
                StmtKind::Const(_, e) | StmtKind::Expr(e) | StmtKind::Throw(e) | StmtKind::Return(Some(e)) => {
                    self.expr(e, &mut next, false)
                }
                StmtKind::Let(_, Some(e)) | StmtKind::Destructure { value: e, .. } => self.expr(e, &mut next, false),
                StmtKind::Assign(a, b) => {
                    self.expr(a, &mut next, true);
                    self.expr(b, &mut next, true);
                }
                StmtKind::If(e, a, b) => {
                    self.expr(e, &mut next, false);
                    self.block(a);
                    if let Some(b) = b {
                        self.block(b);
                    }
                }
                StmtKind::While { cond, body, .. } => {
                    self.expr(cond, &mut next, true);
                    self.block(body);
                }
                StmtKind::Labeled(_, body) => self.block(body),
                StmtKind::ForOf { iterable, body, .. } => {
                    self.expr(iterable, &mut next, false);
                    self.block(body);
                }
                StmtKind::For { start, test, body, .. } => {
                    self.expr(start, &mut next, false);
                    self.expr(test, &mut next, true);
                    self.block(body);
                }
                _ => {}
            }
            next.push(stmt);
        }
        *body = next;
    }
    fn expr(&mut self, e: &mut Expr, out: &mut Vec<Stmt>, blocked: bool) {
        match &mut e.kind {
            ExprKind::Arrow(params, body) | ExprKind::AsyncArrow(params, body) => {
                // Reserve captures, but let a closure use local temporary names.
                scope(params, body, &HashSet::new());
            }
            ExprKind::Jsx(jsx) => {
                let mut prior = blocked;
                if let JsxTag::Component(e) = &mut jsx.tag {
                    self.expr(e, out, prior);
                    prior |= may_run_code(e);
                }
                for prop in &mut jsx.props {
                    match prop {
                        Prop::Field(name, value) => {
                            let handler = name
                                .strip_prefix("on")
                                .is_some_and(|s| s.starts_with(|c: char| c.is_ascii_uppercase()));
                            // React ignores handler return values: keep single-call
                            // event handlers as concise arrow expressions (ADR 0040).
                            if handler
                                && let ExprKind::Arrow(_, body) = &mut value.kind
                                && let [
                                    Stmt {
                                        kind: StmtKind::Expr(e),
                                        span,
                                    },
                                ] = body.as_slice()
                            {
                                *body = vec![StmtKind::Return(Some(e.clone())).at(*span)];
                            }
                            self.expr(value, out, prior);
                            let effects = may_run_code(value);
                            if name.chars().all(|c| c.is_ascii_alphanumeric()) {
                                self.hoist(name, value, out, prior, blocked);
                            }
                            prior |= effects;
                        }
                        Prop::Spread(value) => {
                            self.expr(value, out, prior);
                            prior |= may_run_code(value);
                        }
                    }
                }
                for value in &mut jsx.children {
                    self.expr(value, out, prior);
                    let effects = may_run_code(value);
                    let map = matches!(&value.kind, ExprKind::Call(f, _) if matches!(&f.kind, ExprKind::Member(_, m) if m == "map"));
                    if !matches!(value.kind, ExprKind::Jsx(_)) {
                        self.hoist(if map { "items" } else { "children" }, value, out, prior, blocked);
                    }
                    prior |= effects;
                }
            }
            ExprKind::Member(a, _) | ExprKind::Unary(_, a) | ExprKind::Await(a) => self.expr(a, out, blocked),
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
                // Logical RHS is conditional; no expression may escape it.
                self.expr(a, out, blocked);
                self.expr(b, out, true);
            }
            ExprKind::Cond(a, b, c) => {
                self.expr(a, out, blocked);
                self.expr(b, out, true);
                self.expr(c, out, true);
            }
            ExprKind::Call(f, args) | ExprKind::New(f, args) => {
                self.expr(f, out, blocked);
                let mut prior = blocked || may_run_code(f);
                for a in args {
                    let effects = may_run_code(a);
                    self.expr(a, out, prior);
                    prior |= effects;
                }
            }
            ExprKind::Array(items) => {
                let mut prior = blocked;
                for e in items {
                    let effects = may_run_code(e);
                    self.expr(e, out, prior);
                    prior |= effects;
                }
            }
            ExprKind::Object(props) => {
                let mut prior = blocked;
                for prop in props {
                    let value = prop_value_mut(prop);
                    let effects = may_run_code(value);
                    self.expr(value, out, prior);
                    prior |= effects;
                }
            }
            _ => {}
        }
    }
    fn hoist(&mut self, base: &str, value: &mut Expr, out: &mut Vec<Stmt>, prior: bool, blocked: bool) {
        if prints_on_lines(value) && (position_independent(value) || (!blocked && !prior)) {
            let name = (0..)
                .map(|n| {
                    if n == 0 {
                        base.to_string()
                    } else {
                        format!("{base}${n}")
                    }
                })
                .find(|name| self.names.insert(name.clone()))
                .unwrap();
            let span = value.span;
            let value = std::mem::replace(value, Expr::var(&name));
            out.push(StmtKind::Const(name, value).at(span));
        }
    }
}
/// Is `e` the same wherever it's evaluated? Making a function evaluates
/// nothing, and neither do literals, so they can move out of a condition or
/// ahead of earlier code. A variable can't: earlier code may change it.
fn position_independent(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Arrow(..) | ExprKind::AsyncArrow(..) => true,
        ExprKind::Num(_) | ExprKind::Bool(_) | ExprKind::Str(_) | ExprKind::Undefined | ExprKind::Null => true,
        ExprKind::Array(items) => items.iter().all(position_independent),
        ExprKind::Object(props) => props
            .iter()
            .all(|p| matches!(p, Prop::Field(_, e) if position_independent(e))),
        _ => false,
    }
}

fn prop_value(prop: &Prop) -> &Expr {
    match prop {
        Prop::Field(_, e) | Prop::Spread(e) => e,
    }
}
fn prop_value_mut(prop: &mut Prop) -> &mut Expr {
    match prop {
        Prop::Field(_, e) | Prop::Spread(e) => e,
    }
}

/// JS getters and proxies can run code on a property read. Preparation must
/// be conservative even when lowering knows a Rust field is an ordinary value.
fn may_run_code(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Member(..) | ExprKind::Index(..) | ExprKind::Call(..) | ExprKind::New(..) | ExprKind::Await(..) => {
            true
        }
        ExprKind::Array(items) => items.iter().any(may_run_code),
        ExprKind::Object(props) => props.iter().any(|p| match p {
            Prop::Spread(_) => true,
            Prop::Field(_, e) => may_run_code(e),
        }),
        ExprKind::Unary(_, e) => may_run_code(e),
        ExprKind::Binary(_, a, b) => may_run_code(a) || may_run_code(b),
        ExprKind::Cond(a, b, c) => may_run_code(a) || may_run_code(b) || may_run_code(c),
        ExprKind::Jsx(jsx) => {
            matches!(&jsx.tag, JsxTag::Component(e) if may_run_code(e))
                || jsx.props.iter().any(|p| match p {
                    Prop::Spread(_) => true,
                    Prop::Field(_, e) => may_run_code(e),
                })
                || jsx.children.iter().any(may_run_code)
        }
        _ => false,
    }
}
