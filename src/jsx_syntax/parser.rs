//! Parse only JSX tokens. Rust inside braces stays a Rust token stream,
//! with its original spans, for rustc to parse and type-check.

use rustc_ast::token::{Delimiter, TokenKind};
use rustc_ast::tokenstream::{DelimSpacing, DelimSpan, Spacing, TokenStream, TokenTree};
use rustc_ast::{self as ast, FnRetTy, ItemKind, TyKind};
use rustc_parse::parser::{AllowConstBlockItems, ForceCollect, Parser};
use rustc_session::Session;
use rustc_span::{ErrorGuaranteed, Span};

use super::template;

type R<T> = Result<T, ErrorGuaranteed>;

pub(super) fn jsx(sess: &Session, tokens: TokenStream, span: Span) -> R<TokenStream> {
    let mut p = Jsx {
        sess,
        tokens: tokens.iter().cloned().collect(),
        at: 0,
        span,
    };
    let element = p.element(0)?;
    if p.at != p.tokens.len() {
        return Err(p.error("wrap adjacent JSX elements in <>...</>"));
    }
    Ok(element)
}

struct Jsx<'a> {
    sess: &'a Session,
    tokens: Vec<TokenTree>,
    at: usize,
    span: Span,
}

impl Jsx<'_> {
    fn error(&self, message: &str) -> ErrorGuaranteed {
        self.sess.dcx().span_err(
            self.tokens
                .get(self.at)
                .map_or(self.span.shrink_to_hi(), TokenTree::span),
            format!("jsx: {message}"),
        )
    }

    fn is(&self, kind: TokenKind) -> bool {
        matches!(self.tokens.get(self.at), Some(TokenTree::Token(t, _)) if t.kind == kind)
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.is(kind) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn need(&mut self, kind: TokenKind, message: &str) -> R<()> {
        if self.eat(kind) {
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    fn ident(&mut self) -> R<String> {
        match self.tokens.get(self.at) {
            Some(TokenTree::Token(t, _)) if let TokenKind::Ident(name, _) = t.kind => {
                self.at += 1;
                Ok(name.to_string())
            }
            _ => Err(self.error("expected a name")),
        }
    }

    fn value(&mut self) -> R<TokenStream> {
        match self.tokens.get(self.at).cloned() {
            Some(TokenTree::Delimited(span, spacing, Delimiter::Brace, value)) => {
                self.at += 1;
                // JSX braces delimit an expression; they are not necessarily a
                // Rust block. Keep a block only when it actually has statements.
                let mut parser = Parser::new(&self.sess.psess, value.clone(), Some("JSX expression"));
                let expression = match parser.parse_expr() {
                    Ok(_) => parser.token.kind == TokenKind::Eof,
                    Err(e) => {
                        e.cancel();
                        false
                    }
                };
                if expression {
                    return Ok(value);
                }
                Ok(TokenStream::new(vec![TokenTree::Delimited(
                    span,
                    spacing,
                    Delimiter::Brace,
                    value,
                )]))
            }
            Some(TokenTree::Token(ref token, _)) if matches!(token.kind, TokenKind::Literal(_)) => {
                let value = self.tokens[self.at].clone();
                self.at += 1;
                Ok(TokenStream::new(vec![value]))
            }
            _ => Err(self.error("expected a literal or a Rust expression in {braces}")),
        }
    }

    fn element(&mut self, depth: usize) -> R<TokenStream> {
        if depth > 128 {
            return Err(self.error("JSX nesting exceeds 128 elements"));
        }
        let span = self.tokens.get(self.at).map_or(self.span, TokenTree::span);
        self.need(TokenKind::Lt, "expected <tag> or <>fragment</>")?;
        let mut name = String::new();
        if !self.is(TokenKind::Gt) {
            name = self.ident()?;
            while self.eat(TokenKind::PathSep) || self.eat(TokenKind::Dot) {
                name.push_str("::");
                name.push_str(&self.ident()?);
            }
        }
        let intrinsic = !name.is_empty() && !name.contains("::") && name.starts_with(char::is_lowercase);
        let builtin = match name.as_str() {
            "Fragment" => Some("keyed_fragment"),
            "StrictMode" => Some("strict_mode"),
            "Suspense" => Some("suspense"),
            "Activity" => Some("activity"),
            _ => None,
        };
        let mut attrs: Vec<(String, TokenStream, Span)> = Vec::new();
        let mut spread = None;
        while !self.is(TokenKind::Gt) && !self.is(TokenKind::Slash) {
            let attr_span = self.tokens.get(self.at).map_or(span, TokenTree::span);
            if let Some(TokenTree::Delimited(_, _, Delimiter::Brace, tokens)) = self.tokens.get(self.at)
                && matches!(tokens.get(0), Some(TokenTree::Token(t, _)) if matches!(t.kind, TokenKind::DotDot | TokenKind::DotDotDot))
            {
                if spread.is_some() {
                    return Err(self.error("only one props spread is supported"));
                }
                spread = Some(TokenStream::new(tokens.iter().skip(1).cloned().collect()));
                self.at += 1;
                if !self.is(TokenKind::Gt) && !self.is(TokenKind::Slash) {
                    return Err(self.error("put the props spread last"));
                }
                break;
            }
            let mut attr = self.ident()?;
            while self.eat(TokenKind::Minus) {
                attr.push('-');
                attr.push_str(&self.ident()?);
            }
            if attrs.iter().any(|(n, _, _)| n == &attr) {
                return Err(self.error("duplicate attribute"));
            }
            let value = if self.eat(TokenKind::Eq) {
                self.value()?
            } else {
                template(self.sess, "true".into(), attr_span)
            };
            attrs.push((attr, value, attr_span));
        }
        let closed = self.eat(TokenKind::Slash);
        self.need(TokenKind::Gt, "expected > or />")?;
        let mut children = Vec::new();
        if !closed {
            loop {
                if self.is(TokenKind::Lt)
                    && matches!(self.tokens.get(self.at + 1), Some(TokenTree::Token(t, _)) if t.kind == TokenKind::Slash)
                {
                    self.at += 2;
                    let mut closing = String::new();
                    if !self.is(TokenKind::Gt) {
                        closing = self.ident()?;
                        while self.eat(TokenKind::PathSep) || self.eat(TokenKind::Dot) {
                            closing.push_str("::");
                            closing.push_str(&self.ident()?);
                        }
                    }
                    if closing != name {
                        return Err(self.error(&format!("expected </{name}>, found </{closing}>")));
                    }
                    self.need(TokenKind::Gt, "expected > after closing tag")?;
                    break;
                }
                if self.at == self.tokens.len() {
                    return Err(self.error(&format!("missing closing tag </{name}>")));
                }
                if self.is(TokenKind::Lt) {
                    children.push(self.element(depth + 1)?);
                } else if matches!(self.tokens.get(self.at), Some(TokenTree::Delimited(_, _, Delimiter::Brace, ts)) if ts.is_empty())
                {
                    self.at += 1; // JSX comment: {/* ... */}
                } else {
                    children.push(self.value()?);
                }
            }
        }
        let has_children = !children.is_empty();
        let children = tuple(children, span);
        if name.is_empty() {
            if !attrs.is_empty() || spread.is_some() || closed {
                return Err(self.error("a fragment is <>children</>"));
            }
            return Ok(call(
                template(self.sess, "::react::fragment".into(), span),
                vec![children],
                span,
            ));
        }
        if intrinsic || builtin.is_some() {
            let function = builtin.map_or_else(
                || format!("::react::html::r#{}", snake(&name)),
                |n| format!("::react::{n}"),
            );
            let mut expr = call(
                template(self.sess, function, span),
                if name == "StrictMode" {
                    vec![arguments(vec![], span)]
                } else {
                    vec![]
                },
                span,
            );
            for (attr, value, at) in attrs {
                let (method, args) = if attr.contains('-') {
                    ("attr".into(), vec![template(self.sess, format!("{attr:?}"), at), value])
                } else {
                    (snake(&attr), vec![value])
                };
                expr = method_call(self.sess, expr, &method, args, at);
            }
            if let Some(props) = spread {
                expr = method_call(self.sess, expr, "props", vec![props], span);
            }
            if has_children {
                expr = method_call(self.sess, expr, "children", vec![children], span);
            }
            return Ok(expr);
        }
        // Evaluate attributes in written order, including `key`, before
        // constructing props. The match bindings cannot capture user names:
        // every user expression is in the scrutinee, outside their scope.
        let mut values = Vec::new();
        let mut bindings = Vec::new();
        let mut bind = |value: TokenStream, at: Span| {
            let name = template(self.sess, format!("__jsx{}", values.len()), at);
            values.push(value);
            bindings.push(name.clone());
            name
        };
        for (_, value, at) in &mut attrs {
            *value = bind(value.clone(), *at);
        }
        if let Some(value) = &mut spread {
            *value = bind(value.clone(), span);
        }
        let children = if has_children { bind(children, span) } else { children };
        let mut key = None;
        attrs.retain(|(name, value, at)| {
            if name == "key" {
                key = Some((value.clone(), *at));
                false
            } else {
                true
            }
        });
        if spread.is_some() && !attrs.is_empty() {
            return Err(self.error("a component takes either named props or a props spread; use a Rust struct update inside the spread to override fields"));
        }
        let mut expr = if attrs.is_empty()
            && !has_children
            && let Some(props) = spread.clone()
        {
            call(
                template(self.sess, "::react::component".into(), span),
                vec![template(self.sess, name.clone(), span), props],
                span,
            )
        } else {
            if has_children {
                if attrs.iter().any(|(n, _, _)| n == "children") {
                    return Err(self.error("children were provided twice"));
                }
                attrs.push(("children".into(), children, span));
            }
            let mut fields = Vec::new();
            for (attr, value, at) in attrs {
                fields.extend(template(self.sess, format!("r#{}:", snake(&attr)), at).iter().cloned());
                fields.extend(value.iter().cloned());
                fields.extend(template(self.sess, ",".into(), at).iter().cloned());
            }
            if let Some(base) = spread {
                fields.extend(template(self.sess, "..".into(), span).iter().cloned());
                fields.extend(base.iter().cloned());
            }
            let mut tokens: Vec<_> = template(self.sess, format!("{name}!"), span).iter().cloned().collect();
            tokens.push(group(Delimiter::Brace, TokenStream::new(fields), span));
            TokenStream::new(tokens)
        };
        if let Some((key, at)) = key {
            expr = method_call(self.sess, expr, "key", vec![key], at);
        }
        if values.is_empty() {
            return Ok(expr);
        }
        let mut tokens: Vec<_> = template(self.sess, "match".into(), span).iter().cloned().collect();
        tokens.extend(arguments(values, span).iter().cloned());
        let mut arm: Vec<_> = arguments(bindings, span).iter().cloned().collect();
        arm.extend(template(self.sess, "=>".into(), span).iter().cloned());
        arm.extend(expr.iter().cloned());
        tokens.push(group(Delimiter::Brace, TokenStream::new(arm), span));
        Ok(TokenStream::new(tokens))
    }
}

fn snake(name: &str) -> String {
    let mut result = String::new();
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            result.push('_');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

fn group(delimiter: Delimiter, tokens: TokenStream, span: Span) -> TokenTree {
    TokenTree::Delimited(
        DelimSpan::from_single(span),
        DelimSpacing::new(Spacing::Alone, Spacing::Alone),
        delimiter,
        tokens,
    )
}

fn tuple(mut parts: Vec<TokenStream>, span: Span) -> TokenStream {
    // Pair tuples have no fixed sibling limit in the Node trait, and JSX
    // lowering flattens them without allocating arrays in the output.
    let mut tail = parts
        .pop()
        .unwrap_or_else(|| TokenStream::new(vec![group(Delimiter::Parenthesis, TokenStream::default(), span)]));
    while let Some(head) = parts.pop() {
        tail = arguments(vec![head, tail], span);
    }
    tail
}

fn arguments(parts: Vec<TokenStream>, span: Span) -> TokenStream {
    let mut tokens = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        if i > 0 {
            tokens.push(TokenTree::token_alone(TokenKind::Comma, span));
        }
        tokens.extend(part.iter().cloned());
    }
    // A one-value match input/pattern must be a tuple, not redundant
    // parentheses (which would trigger unused_parens in the user's crate).
    // A trailing comma is also valid for ordinary function arguments.
    if !tokens.is_empty() {
        tokens.push(TokenTree::token_alone(TokenKind::Comma, span));
    }
    TokenStream::new(vec![group(Delimiter::Parenthesis, TokenStream::new(tokens), span)])
}

fn call(function: TokenStream, args: Vec<TokenStream>, span: Span) -> TokenStream {
    TokenStream::new(function.iter().chain(arguments(args, span).iter()).cloned().collect())
}

fn method_call(sess: &Session, receiver: TokenStream, method: &str, args: Vec<TokenStream>, span: Span) -> TokenStream {
    let method = template(sess, format!(".r#{method}"), span);
    let function = TokenStream::new(receiver.iter().chain(method.iter()).cloned().collect());
    call(function, args, span)
}

/// A hygienic props constructor beside a component. Rust resolves its props
/// type in the definition's module, including aliases and private imports.
/// Calling it through an imported/renamed component uses Rust's macro namespace.
pub(super) fn component(sess: &Session, item: &ast::Item) -> Option<Box<ast::Item>> {
    let ItemKind::Fn(f) = &item.kind else { return None };
    let FnRetTy::Ty(ret) = &f.sig.decl.output else {
        return None;
    };
    let TyKind::Path(_, path) = &ret.kind else { return None };
    if !f.ident.as_str().starts_with(char::is_uppercase)
        || path.segments.last()?.ident.as_str() != "Element"
        || !f.generics.params.is_empty()
        || f.sig.decl.inputs.len() > 1
    {
        return None;
    }
    let props = if let Some(param) = f.sig.decl.inputs.first() {
        let TyKind::Path(_, path) = &param.ty.kind else {
            return None;
        };
        if path.segments.iter().any(|s| s.args.is_some()) {
            return None;
        }
        path.segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::")
    } else {
        String::new()
    };
    let body = if props.is_empty() {
        format!("() {{ ::react::component({}, ()) }}", f.ident)
    } else {
        format!(
            "{{ ($($field:ident: $value:expr,)* $(..$base:expr)?) => {{ ::react::component({}, {props} {{ $($field: $value,)* $(..$base)? }}) }} }}",
            f.ident
        )
    };
    let tokens = template(sess, format!("macro {} {body}", f.ident), item.span);
    let mut parser = Parser::new(&sess.psess, tokens, Some("JSX component props"));
    match parser.parse_item(ForceCollect::No, AllowConstBlockItems::No) {
        Ok(Some(mut companion)) => {
            companion.vis = item.vis.clone();
            Some(companion)
        }
        Ok(None) => None,
        Err(e) => {
            e.emit();
            None
        }
    }
}
