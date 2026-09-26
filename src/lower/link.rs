//! Resolve symbolic module references after lowering and reachability.
//! Locals keep their names; import aliases avoid every binding that could
//! shadow them, including parameters of nested arrows and copied defaults.

use super::{LoweredModule, fresh_in};
use crate::js::{Expr, ExprKind, Function, JsxTag, Pattern, Prop, Stmt, StmtKind};
use rustc_span::def_id::LocalModDefId;
use std::collections::{HashMap, HashSet};

/// NUL cannot occur in a Rust/JS identifier. These temporary names exist
/// only inside lowering and are resolved before the JS AST leaves it.
pub(super) fn symbol(module: LocalModDefId) -> String {
    format!("\0module:{}", module.to_def_id().index.as_u32())
}

pub(super) fn resolve(
    module: &mut LoweredModule,
    imports: &[(String, String, Vec<String>)],
    mut names: HashSet<String>,
) {
    visit(module, &mut |name| {
        if !name.starts_with('\0') {
            names.insert(name.clone());
        }
    });
    let replacements: HashMap<_, _> = imports
        .iter()
        .map(|(symbol, base, path)| {
            let alias = fresh_in(&mut names, base);
            module.imports.push((alias.clone(), path.clone()));
            (symbol.as_str(), alias)
        })
        .collect();
    visit(module, &mut |name| {
        if name.starts_with('\0') {
            *name = replacements
                .get(name.as_str())
                .expect("every symbolic module reference has a dependency")
                .clone();
        }
    });
}

fn visit(module: &mut LoweredModule, name: &mut impl FnMut(&mut String)) {
    for function in &mut module.functions {
        function_names(function, name);
    }
    for namespace in &mut module.namespaces {
        name(&mut namespace.name);
        for function in &mut namespace.methods {
            function_names(function, name);
        }
    }
    for constant in &mut module.consts {
        name(&mut constant.name);
        expr(&mut constant.value, name);
    }
    for cache in &mut module.caches {
        name(cache);
    }
}

fn function_names(function: &mut Function, name: &mut impl FnMut(&mut String)) {
    name(&mut function.name);
    for param in &mut function.params {
        pattern(param, name);
    }
    block(&mut function.body, name);
}

fn pattern(p: &mut Pattern, name: &mut impl FnMut(&mut String)) {
    match p {
        Pattern::Name(n) => name(n),
        Pattern::Array(parts) => parts.iter_mut().flatten().for_each(name),
        Pattern::Object(parts) => parts.iter_mut().for_each(|(_, n)| name(n)),
    }
}

fn block(body: &mut [Stmt], name: &mut impl FnMut(&mut String)) {
    for statement in body {
        match &mut statement.kind {
            StmtKind::Const(n, e) => {
                name(n);
                expr(e, name);
            }
            StmtKind::Let(n, e) => {
                name(n);
                if let Some(e) = e {
                    expr(e, name);
                }
            }
            StmtKind::Destructure { pattern: p, value, .. } => {
                pattern(p, name);
                expr(value, name);
            }
            StmtKind::Assign(a, b) => {
                expr(a, name);
                expr(b, name);
            }
            StmtKind::Expr(e) | StmtKind::Throw(e) | StmtKind::Return(Some(e)) => expr(e, name),
            StmtKind::If(e, yes, no) => {
                expr(e, name);
                block(yes, name);
                if let Some(no) = no {
                    block(no, name);
                }
            }
            StmtKind::While { cond, body, .. } => {
                expr(cond, name);
                block(body, name);
            }
            StmtKind::ForOf {
                pattern: p,
                iterable,
                body,
                ..
            } => {
                pattern(p, name);
                expr(iterable, name);
                block(body, name);
            }
            StmtKind::For {
                name: n,
                start,
                test,
                body,
                ..
            } => {
                name(n);
                expr(start, name);
                expr(test, name);
                block(body, name);
            }
            StmtKind::Labeled(_, body) => block(body, name),
            StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => {}
        }
    }
}

fn expr(e: &mut Expr, name: &mut impl FnMut(&mut String)) {
    match &mut e.kind {
        ExprKind::Var(n) => name(n),
        ExprKind::Member(a, _) | ExprKind::Unary(_, a) | ExprKind::Await(a) => expr(a, name),
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
            expr(a, name);
            expr(b, name);
        }
        ExprKind::Cond(a, b, c) => {
            expr(a, name);
            expr(b, name);
            expr(c, name);
        }
        ExprKind::Call(f, args) | ExprKind::New(f, args) => {
            expr(f, name);
            for a in args {
                expr(a, name);
            }
        }
        ExprKind::Array(items) | ExprKind::Template(_, items) => {
            for item in items {
                expr(item, name);
            }
        }
        ExprKind::Object(props) => properties(props, name),
        ExprKind::Arrow(params, body) | ExprKind::AsyncArrow(params, body) => {
            for param in params {
                pattern(param, name);
            }
            block(body, name);
        }
        ExprKind::Jsx(jsx) => {
            if let JsxTag::Component(e) = &mut jsx.tag {
                expr(e, name);
            }
            properties(&mut jsx.props, name);
            for child in &mut jsx.children {
                expr(child, name);
            }
        }
        ExprKind::Num(_)
        | ExprKind::Bool(_)
        | ExprKind::Str(_)
        | ExprKind::Undefined
        | ExprKind::Null
        | ExprKind::Regex(_) => {}
    }
}

fn properties(props: &mut [Prop], name: &mut impl FnMut(&mut String)) {
    for prop in props {
        match prop {
            Prop::Field(_, e) | Prop::Spread(e) => expr(e, name),
        }
    }
}
