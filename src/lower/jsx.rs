//! Lower JSX bindings into the framework-independent JS tree.

use super::{FnCx, R, Shape, camel_case, js_ident};
use crate::js;
use crate::js::{Expr, Prop, Stmt, StmtKind};
use rustc_ast::LitKind;
use rustc_hir::def::DefKind;
use rustc_middle::thir::{ExprId, ExprKind};
use rustc_middle::ty;
use rustc_middle::ty::Ty;
use rustc_span::Span;

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// Element construction is inert (ADR 0040). Capture its inputs at the
    /// original evaluation point while leaving the element in the JSX tree.
    pub(super) fn capture_jsx(&mut self, value: &mut Expr, out: &mut Vec<Stmt>) -> bool {
        match &mut value.kind {
            js::ExprKind::Jsx(jsx) => {
                if let js::JsxTag::Component(tag) = &mut jsx.tag {
                    self.capture_jsx_input("Component", tag, out);
                }
                for prop in &mut jsx.props {
                    match prop {
                        Prop::Field(name, value) => self.capture_jsx_input(&js_ident(name), value, out),
                        Prop::Spread(value) => {
                            // JSX spreads read properties now, not when the
                            // later element is built. Preserve getters too.
                            let old = std::mem::replace(value, Expr::undefined());
                            *value = self.spill("props", Expr::object(vec![Prop::Spread(old)]), out);
                        }
                    }
                }
                for child in &mut jsx.children {
                    self.capture_jsx_input("children", child, out);
                }
                true
            }
            js::ExprKind::Cond(test, yes, no) if jsx_tree(yes) || jsx_tree(no) => {
                self.capture_jsx_input("condition", test, out);
                let mut then = Vec::new();
                let mut otherwise = Vec::new();
                self.capture_jsx_input("children", yes, &mut then);
                self.capture_jsx_input("children", no, &mut otherwise);
                // Keep branch computations conditional. Only their result
                // bindings are visible to the deferred JSX expression.
                for statement in then.iter_mut().chain(&mut otherwise) {
                    match &mut statement.kind {
                        StmtKind::Const(name, value) | StmtKind::Let(name, Some(value)) => {
                            out.push(StmtKind::Let(name.clone(), None).at(statement.span));
                            statement.kind = StmtKind::Assign(Expr::var(name), value.clone());
                        }
                        StmtKind::Let(name, None) => {
                            out.push(StmtKind::Let(name.clone(), None).at(statement.span));
                        }
                        _ => {}
                    }
                }
                then.retain(|s| !matches!(s.kind, StmtKind::Let(_, None)));
                otherwise.retain(|s| !matches!(s.kind, StmtKind::Let(_, None)));
                if !then.is_empty() || !otherwise.is_empty() {
                    out.push(StmtKind::If(*test.clone(), then, Some(otherwise)).at(value.span));
                }
                true
            }
            js::ExprKind::Array(items) if items.iter().any(jsx_tree) => {
                for item in items {
                    self.capture_jsx_input("children", item, out);
                }
                true
            }
            _ => false,
        }
    }

    fn capture_jsx_input(&mut self, base: &str, value: &mut Expr, out: &mut Vec<Stmt>) {
        let stable = match &value.kind {
            js::ExprKind::Symbol(_) | js::ExprKind::Arrow(..) | js::ExprKind::AsyncArrow(..) => true,
            js::ExprKind::Var(name) => {
                self.locals
                    .vars
                    .values()
                    .any(|var| !var.mutable && matches!(&var.place.kind, js::ExprKind::Var(n) if n == name))
                    || out
                        .iter()
                        .any(|s| matches!(&s.kind, StmtKind::Const(n, _) if n == name))
                    || self.krate.fns.iter().any(|(id, f)| {
                        f.module == self.module
                            && &f.name == name
                            && matches!(self.tcx.def_kind(*id), DefKind::Fn | DefKind::AssocFn)
                    })
            }
            _ => value.is_constant(),
        };
        if !stable && !self.capture_jsx(value, out) {
            let map = matches!(&value.kind, js::ExprKind::Call(f, _) if matches!(&f.kind, js::ExprKind::Member(_, name) if name == "map"));
            let old = std::mem::replace(value, Expr::undefined());
            *value = self.spill(if map { "items" } else { base }, old, out);
        }
    }

    // ── JSX (ADR 0040) ──────────────────────────────────────────────────

    /// A JSX element, from a binding whose `link_name` is its tag: `<div>`
    /// takes nothing, `<>` and an imported component (`<react#StrictMode>`)
    /// take their children, and `<*>` takes a component and its props.
    pub(super) fn jsx(&mut self, tag: &str, args: &[ExprId], span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        self.jsx = true;
        let (tag, props, children) = match (tag, args) {
            ("*", &[component, props]) => {
                // A JS module's component is its import, as a tag must be.
                let tag = match self.binding_component(component) {
                    Some(tag) => tag,
                    None => self.expr(component, out)?,
                };
                if !matches!(
                    tag.kind,
                    js::ExprKind::Var(_) | js::ExprKind::Symbol(_) | js::ExprKind::Member(..)
                ) {
                    return Err(self.unsupported(
                        self.thir[component].span,
                        "a JSX component other than a named function or module member",
                    ));
                }
                // JSX reads a lowercase name as a DOM element's, and Fast
                // Refresh only keeps the state of a capitalized component.
                if let Some(name) = match &tag.kind {
                    js::ExprKind::Var(name) => Some(name.as_str()),
                    js::ExprKind::Symbol(symbol) => Some(symbol.export.as_str()),
                    _ => None,
                } && !name.starts_with(|c: char| c.is_ascii_uppercase())
                {
                    let message =
                        format!("rust-js: a React component's name starts with an uppercase letter, not `{name}`");
                    return Err(self.tcx.dcx().span_err(self.thir[component].span, message));
                }
                let (props, children) = self.jsx_props(props, out)?;
                (js::JsxTag::Component(tag), props, children)
            }
            (tag, [] | [_]) if tag != "*" => {
                let children = match args {
                    &[children] => self.jsx_children(children, out)?,
                    _ => Vec::new(),
                };
                let tag = match tag {
                    "" => js::JsxTag::Fragment,
                    t if t.contains('#') => js::JsxTag::Component(self.js_ref(t)),
                    t => js::JsxTag::Intrinsic(t.to_string()),
                };
                (tag, Vec::new(), children)
            }
            _ => return Err(self.unsupported(span, "this JSX binding's signature")),
        };
        Ok(Expr::jsx(js::Jsx { tag, props, children }))
    }

    /// A component's props as attributes: a struct's fields, with its
    /// `children` as the element's; `()` for none; anything else spread,
    /// `{...props}`.
    pub(super) fn jsx_props(&mut self, props: ExprId, out: &mut Vec<Stmt>) -> R<(Vec<Prop>, Vec<Expr>)> {
        let ty = self.thir[props].ty;
        let value = self.expr(props, out)?;
        let mut fields = match value.kind {
            js::ExprKind::Undefined => return Ok((Vec::new(), Vec::new())),
            js::ExprKind::Object(fields) => fields,
            _ => return Ok((vec![Prop::Spread(value)], Vec::new())),
        };
        if fields
            .iter()
            .position(|p| matches!(p, Prop::Field(name, _) if name == "children"))
            .is_some_and(|i| i + 1 < fields.len())
        {
            for prop in &mut fields {
                let (name, value) = match prop {
                    Prop::Field(name, value) => (name.as_str(), value),
                    Prop::Spread(value) => ("props", value),
                };
                if !value.is_constant() {
                    let old = std::mem::replace(value, Expr::undefined());
                    *value = self.spill(&camel_case(&js_ident(name)), old, out);
                }
            }
        }
        let mut attrs = Vec::new();
        let mut children = Vec::new();
        for field in fields {
            match field {
                Prop::Field(name, value) if name == "children" => {
                    let Shape::Object(types) = self.shape(ty) else {
                        unreachable!("a struct's fields")
                    };
                    let child_ty = types.into_iter().find(|(n, _)| *n == name).expect("the field").1;
                    children = self.spread_children(value, child_ty, out);
                }
                other => attrs.push(other),
            }
        }
        Ok((attrs, children))
    }

    pub(super) fn jsx_children(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Vec<Expr>> {
        let value = self.expr(e, out)?;
        Ok(self.spread_children(value, self.thir[e].ty, out))
    }

    /// A tuple of children is several, `("Count is ", count)`: `Count is {count}`.
    /// Anything else is one: a `Vec` is `{items}`, which React renders item by item.
    pub(super) fn spread_children(&mut self, value: Expr, ty: Ty<'tcx>, out: &mut Vec<Stmt>) -> Vec<Expr> {
        let ty::Tuple(tys) = *ty.kind() else { return vec![value] };
        let parts = match value.kind {
            js::ExprKind::Array(items) => items,
            _ if tys.is_empty() => Vec::new(),
            _ => {
                let tuple = if value.has_effects() {
                    self.spill("children", value, out)
                } else {
                    value
                };
                (0..tys.len())
                    .map(|i| Expr::index(tuple.clone(), Expr::int(i as i128)))
                    .collect()
            }
        };
        parts
            .into_iter()
            .zip(tys.iter())
            .flat_map(|(part, t)| self.spread_children(part, t, out))
            .collect()
    }

    /// `{}` or `{__html}`: an object literal, its fields the arguments.
    pub(super) fn object_binding(
        &mut self,
        keys: &[String],
        args: &[ExprId],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        if keys.len() != args.len() {
            return Err(self.unsupported(span, "an object binding whose fields don't match its arguments"));
        }
        let values = self.operands(args, out)?;
        Ok(Expr::object(
            keys.iter()
                .cloned()
                .zip(values)
                .map(|(k, v)| Prop::Field(k, v))
                .collect(),
        ))
    }

    /// `style.color("red")` on an object being built: one more field. Its
    /// earlier fields go in `const`s first when this one's value needs
    /// statements, so they're still evaluated first.
    fn object_field(&mut self, mut object: Expr, name: String, value: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let js::ExprKind::Object(fields) = &mut object.kind else {
            unreachable!("checked by the caller")
        };
        if !self.is_simple(value) {
            for prop in fields.iter_mut() {
                let (base, value) = match prop {
                    Prop::Field(name, value) => (name.as_str(), value),
                    Prop::Spread(value) => ("props", value),
                };
                if !value.is_constant() {
                    let old = std::mem::replace(value, Expr::undefined());
                    *value = self.spill(&camel_case(&js_ident(base)), old, out);
                }
            }
        }
        let value = self.expr(value, out)?;
        let js::ExprKind::Object(fields) = &mut object.kind else {
            unreachable!("checked by the caller")
        };
        fields.push(Prop::Field(name, value));
        Ok(object)
    }

    /// `element.class_name("hero")`, a binding like `#[rust_js::link_name =
    /// "prop className"]`: the attribute, on the element being built.
    pub(super) fn jsx_prop(&mut self, name: Option<&str>, args: &[ExprId], span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let (name, value) = match (name, args) {
            (Some(name), &[_, value]) => (name.to_string(), value),
            (None, &[_, name, value]) => match self.thir[self.strip_refs(name)].kind {
                ExprKind::Literal { lit, neg: false } if let LitKind::Str(s, _) = lit.node => (s.to_string(), value),
                _ => {
                    let message = "rust-js: an attribute's name is a string literal";
                    return Err(self.tcx.dcx().span_err(self.thir[name].span, message));
                }
            },
            _ => return Err(self.unsupported(span, "this JSX attribute binding's signature")),
        };
        let mut element = self.expr(args[0], out)?;
        if let js::ExprKind::Object(_) = element.kind {
            return self.object_field(element, name, value, out);
        }
        if !matches!(element.kind, js::ExprKind::Jsx(_)) {
            let message = "rust-js makes JSX from one expression: set an element's props in the chain that makes it";
            return Err(self.tcx.dcx().span_err(self.thir[args[0]].span, message));
        }
        // The receiver is evaluated before the argument. JSX prints attributes
        // before children; later attributes must not jump ahead of earlier
        // children. Likewise, statements introduced by an argument must not
        // jump ahead of any deferred receiver evaluations. These temporaries
        // preserve Rust evaluation order, independently of output formatting.
        let js::ExprKind::Jsx(jsx) = &mut element.kind else {
            unreachable!("checked above")
        };
        if !self.is_simple(value) || (name != "children" && !jsx.children.is_empty()) {
            for prop in &mut jsx.props {
                let (base, value) = match prop {
                    Prop::Field(name, value) => (name.as_str(), value),
                    Prop::Spread(value) => ("props", value),
                };
                if !value.is_constant() {
                    let old = std::mem::replace(value, Expr::undefined());
                    *value = self.spill(&camel_case(&js_ident(base)), old, out);
                }
            }
            for value in &mut jsx.children {
                if !value.is_constant() {
                    let old = std::mem::replace(value, Expr::undefined());
                    *value = self.spill("children", old, out);
                }
            }
        }
        if name == "children" {
            let children = self.jsx_children(value, out)?;
            let js::ExprKind::Jsx(jsx) = &mut element.kind else {
                unreachable!("checked above")
            };
            jsx.children.extend(children);
        } else {
            if name == "..." && !matches!(self.shape(self.thir[value].ty), Shape::Object(_)) {
                return Err(self.unsupported(self.thir[value].span, "JSX props spread of a non-struct value"));
            }
            let value = self.expr(value, out)?;
            let js::ExprKind::Jsx(jsx) = &mut element.kind else {
                unreachable!("checked above")
            };
            jsx.props.push(if name == "..." {
                Prop::Spread(value)
            } else {
                Prop::Field(name, value)
            });
        }
        Ok(element)
    }
}

/// JSX syntax evaluated here, rather than inside a callback passed elsewhere.
fn jsx_tree(value: &Expr) -> bool {
    match &value.kind {
        js::ExprKind::Jsx(_) => true,
        js::ExprKind::Cond(_, yes, no) => jsx_tree(yes) || jsx_tree(no),
        js::ExprKind::Array(items) => items.iter().any(jsx_tree),
        _ => false,
    }
}
