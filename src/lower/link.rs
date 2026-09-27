//! Resolve symbolic module references after lowering and reachability.
//! Locals keep their names; import aliases avoid every binding that could
//! shadow them, including parameters of nested arrows and copied defaults.

use super::{LoweredImport, LoweredModule, fresh_in};
use crate::js::{Expr, ExprKind, Function, JsxTag, Pattern, Prop, Stmt, StmtKind};
use rustc_span::def_id::LocalModDefId;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) fn symbol(module: LocalModDefId, export: &str) -> crate::js::Symbol {
    crate::js::Symbol {
        module: module.to_def_id().index.as_u32(),
        export: export.to_owned(),
    }
}

pub(super) fn resolve(
    module: &mut LoweredModule,
    imports: &[(crate::js::Symbol, String, Vec<String>)],
    mut names: HashSet<String>,
) {
    visit(
        module,
        &mut Visitor {
            names: &mut names,
            replacements: None,
        },
    );
    let mut grouped: BTreeMap<Vec<String>, Vec<(String, String)>> = BTreeMap::new();
    let replacements: HashMap<_, _> = imports
        .iter()
        .map(|(symbol, base, path)| {
            let alias = fresh_in(&mut names, base);
            grouped
                .entry(path.clone())
                .or_default()
                .push((base.clone(), alias.clone()));
            (symbol.clone(), alias)
        })
        .collect();
    module.imports = grouped
        .into_iter()
        .map(|(path, named)| LoweredImport { path, named })
        .collect();
    visit(
        module,
        &mut Visitor {
            names: &mut names,
            replacements: Some(&replacements),
        },
    );
}

struct Visitor<'a> {
    names: &'a mut HashSet<String>,
    replacements: Option<&'a HashMap<crate::js::Symbol, String>>,
}

impl Visitor<'_> {
    fn name(&mut self, name: &str) {
        if self.replacements.is_none() {
            self.names.insert(name.to_owned());
        }
    }
}

fn visit(module: &mut LoweredModule, visitor: &mut Visitor<'_>) {
    for function in &mut module.functions {
        function_names(function, visitor);
    }
    for namespace in &mut module.namespaces {
        visitor.name(&namespace.name);
        for function in &mut namespace.methods {
            function_names(function, visitor);
        }
    }
    for constant in &mut module.consts {
        visitor.name(&constant.name);
        expr(&mut constant.value, visitor);
    }
    for cache in &mut module.caches {
        visitor.name(cache);
    }
}

fn function_names(function: &mut Function, visitor: &mut Visitor<'_>) {
    visitor.name(&function.name);
    for param in &mut function.params {
        pattern(param, visitor);
    }
    block(&mut function.body, visitor);
}

fn pattern(p: &mut Pattern, visitor: &mut Visitor<'_>) {
    match p {
        Pattern::Name(n) => visitor.name(n),
        Pattern::Array(parts) => parts.iter_mut().flatten().for_each(|n| visitor.name(n)),
        Pattern::Object(parts) => parts.iter_mut().for_each(|(_, n)| visitor.name(n)),
    }
}

fn block(body: &mut [Stmt], visitor: &mut Visitor<'_>) {
    for statement in body {
        match &mut statement.kind {
            StmtKind::Const(n, e) => {
                visitor.name(n);
                expr(e, visitor);
            }
            StmtKind::Let(n, e) => {
                visitor.name(n);
                if let Some(e) = e {
                    expr(e, visitor);
                }
            }
            StmtKind::Destructure { pattern: p, value, .. } => {
                pattern(p, visitor);
                expr(value, visitor);
            }
            StmtKind::Assign(a, b) => {
                expr(a, visitor);
                expr(b, visitor);
            }
            StmtKind::Expr(e) | StmtKind::Throw(e) | StmtKind::Return(Some(e)) => expr(e, visitor),
            StmtKind::If(e, yes, no) => {
                expr(e, visitor);
                block(yes, visitor);
                if let Some(no) = no {
                    block(no, visitor);
                }
            }
            StmtKind::While { cond, body, .. } => {
                expr(cond, visitor);
                block(body, visitor);
            }
            StmtKind::ForOf {
                pattern: p,
                iterable,
                body,
                ..
            } => {
                pattern(p, visitor);
                expr(iterable, visitor);
                block(body, visitor);
            }
            StmtKind::For {
                name: n,
                start,
                test,
                body,
                ..
            } => {
                visitor.name(n);
                expr(start, visitor);
                expr(test, visitor);
                block(body, visitor);
            }
            StmtKind::Labeled(_, body) => block(body, visitor),
            StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue(_) => {}
        }
    }
}

fn expr(e: &mut Expr, visitor: &mut Visitor<'_>) {
    if let ExprKind::Symbol(symbol) = &e.kind
        && let Some(replacements) = visitor.replacements
    {
        e.kind = ExprKind::Var(
            replacements
                .get(symbol)
                .expect("every symbolic module reference has a dependency")
                .clone(),
        );
    }
    match &mut e.kind {
        ExprKind::Symbol(_) => {}
        ExprKind::Var(n) => visitor.name(n),
        ExprKind::Member(a, _) | ExprKind::Unary(_, a) | ExprKind::Await(a) => expr(a, visitor),
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
            expr(a, visitor);
            expr(b, visitor);
        }
        ExprKind::Cond(a, b, c) => {
            expr(a, visitor);
            expr(b, visitor);
            expr(c, visitor);
        }
        ExprKind::Call(f, args) | ExprKind::New(f, args) => {
            expr(f, visitor);
            for a in args {
                expr(a, visitor);
            }
        }
        ExprKind::Array(items) | ExprKind::Template(_, items) => {
            for item in items {
                expr(item, visitor);
            }
        }
        ExprKind::Object(props) => properties(props, visitor),
        ExprKind::Arrow(params, body) | ExprKind::AsyncArrow(params, body) => {
            for param in params {
                pattern(param, visitor);
            }
            block(body, visitor);
        }
        ExprKind::Jsx(jsx) => {
            if let JsxTag::Component(e) = &mut jsx.tag {
                expr(e, visitor);
            }
            properties(&mut jsx.props, visitor);
            for child in &mut jsx.children {
                expr(child, visitor);
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

fn properties(props: &mut [Prop], visitor: &mut Visitor<'_>) {
    for prop in props {
        match prop {
            Prop::Field(_, e) | Prop::Spread(e) => expr(e, visitor),
        }
    }
}
