//! Indent JSX using the compiler's parser. Only leading whitespace before
//! tokens changes: comments, literal contents and line breaks stay intact.

use std::collections::BTreeMap;

use rustc_ast::mut_visit::{self, MutVisitor};
use rustc_ast::token::TokenKind;
use rustc_ast::tokenstream::{DelimSpan, TokenStream, TokenTree};
use rustc_ast::visit::AssocCtxt;
use rustc_ast::{self as ast, ExprKind};
use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface::Compiler;
use rustc_session::Session;
use rustc_span::{BytePos, ErrorGuaranteed, Span, Symbol};

use super::parser;

#[derive(Default)]
pub struct Formatter {
    pub output: Option<String>,
}

impl Callbacks for Formatter {
    fn after_crate_root_parsing(&mut self, compiler: &Compiler, krate: &mut ast::Crate) -> Compilation {
        let sess = &compiler.sess;
        let file = sess.source_map().lookup_source_file(krate.spans.inner_span.lo());
        let source = file.src.as_ref().expect("the parsed input has source text").to_string();
        let mut visitor = Format {
            sess,
            layout: Layout {
                lines: std::iter::once(0)
                    .chain(source.match_indices('\n').map(|(i, _)| i + 1))
                    .collect(),
                source,
                start: file.start_pos,
                indents: BTreeMap::new(),
            },
        };
        visitor.visit_crate(krate);
        if sess.dcx().has_errors().is_none() {
            self.output = Some(visitor.layout.finish());
        }
        Compilation::Stop
    }
}

struct Format<'a> {
    sess: &'a Session,
    layout: Layout,
}

impl Format<'_> {
    fn mac(&mut self, mac: &ast::MacCall) {
        if mac.path.segments.len() == 1 && mac.path.segments[0].ident.as_str() == "jsx" {
            let indent = self.layout.indent(mac.span());
            let _ = parser::formatted(
                self.sess,
                mac.args.tokens.clone(),
                mac.span(),
                indent + 4,
                &mut self.layout,
            );
            self.layout.mark(mac.args.dspan.close, indent);
        }
    }
}

impl MutVisitor for Format<'_> {
    fn visit_crate(&mut self, krate: &mut ast::Crate) {
        if !skip(&krate.attrs) {
            mut_visit::walk_crate(self, krate);
        }
    }

    fn visit_item(&mut self, item: &mut ast::Item) {
        if !skip(&item.attrs) {
            mut_visit::walk_item(self, item);
        }
    }

    fn visit_expr(&mut self, expr: &mut ast::Expr) {
        if skip(&expr.attrs) {
            return;
        }
        if let ExprKind::MacCall(mac) = &expr.kind {
            self.mac(mac);
        }
        mut_visit::walk_expr(self, expr);
    }

    fn visit_assoc_item(&mut self, item: &mut ast::AssocItem, ctxt: AssocCtxt) {
        if !skip(&item.attrs) {
            mut_visit::walk_assoc_item(self, item, ctxt);
        }
    }

    fn visit_block(&mut self, block: &mut ast::Block) {
        for stmt in &block.stmts {
            if let ast::StmtKind::MacCall(mac) = &stmt.kind
                && !skip(&mac.attrs)
            {
                self.mac(&mac.mac);
            }
        }
        mut_visit::walk_block(self, block);
    }
}

fn skip(attrs: &[ast::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path_matches(&[Symbol::intern("rustfmt"), Symbol::intern("skip")]))
}

pub(super) struct Layout {
    source: String,
    start: BytePos,
    lines: Vec<usize>,
    indents: BTreeMap<usize, usize>,
}

impl Layout {
    fn line(&self, span: Span) -> usize {
        let offset = (span.lo() - self.start).0 as usize;
        self.lines.partition_point(|&start| start <= offset) - 1
    }

    fn indent(&self, span: Span) -> usize {
        let line = self.line(span);
        self.indents.get(&line).copied().unwrap_or_else(|| {
            self.source[self.lines[line]..]
                .chars()
                .take_while(|c| matches!(c, ' ' | '\t'))
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum()
        })
    }

    pub(super) fn mark(&mut self, span: Span, indent: usize) -> usize {
        let line = self.line(span);
        let offset = (span.lo() - self.start).0 as usize;
        if self.source[self.lines[line]..offset]
            .bytes()
            .all(|b| matches!(b, b' ' | b'\t'))
        {
            self.indents.insert(line, indent);
        }
        self.indent(span)
    }

    pub(super) fn value(
        &mut self,
        sess: &Session,
        span: DelimSpan,
        tokens: &TokenStream,
        indent: usize,
    ) -> Result<(), ErrorGuaranteed> {
        let indent = self.mark(span.open, indent);
        // JSX braces delimit an expression. If it starts on this line, its
        // nested Rust blocks are relative to the prop/child, not another level.
        let inner = if tokens
            .get(0)
            .is_some_and(|t| self.line(t.span()) == self.line(span.open))
        {
            indent
        } else {
            indent + 4
        };
        self.rust(sess, tokens, inner)?;
        self.mark(span.close, indent);
        Ok(())
    }

    fn rust(&mut self, sess: &Session, tokens: &TokenStream, indent: usize) -> Result<(), ErrorGuaranteed> {
        let tokens: Vec<_> = tokens.iter().collect();
        let mut i = 0;
        while i < tokens.len() {
            let tree = tokens[i];
            let at = self.mark(tree.span(), indent);
            // Macro bodies have their own grammar. Recurse only into jsx!,
            // leaving stringify!, macro definitions, and other DSLs intact.
            if let TokenTree::Token(t, _) = tree
                && let TokenKind::Ident(name, _) = t.kind
                && matches!(tokens.get(i + 1), Some(TokenTree::Token(t, _)) if t.kind == TokenKind::Bang)
            {
                let group = i + if name.as_str() == "macro_rules" { 3 } else { 2 };
                if let Some(TokenTree::Delimited(span, _, _, inner)) = tokens.get(group) {
                    if name.as_str() == "jsx"
                        && !matches!(i.checked_sub(1).and_then(|at| tokens.get(at)), Some(TokenTree::Token(t, _)) if t.kind == TokenKind::PathSep)
                    {
                        parser::formatted(sess, inner.clone(), t.span.to(span.close), at + 4, self)?;
                        self.mark(span.close, at);
                    }
                    i = group + 1;
                    continue;
                }
            }
            if let TokenTree::Delimited(span, _, _, inner) = tree {
                self.rust(sess, inner, at + 4)?;
                self.mark(span.close, at);
            }
            i += 1;
        }
        Ok(())
    }

    fn finish(mut self) -> String {
        for (line, indent) in self.indents.into_iter().rev() {
            let start = self.lines[line];
            let length = self.source[start..]
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            self.source.replace_range(start..start + length, &" ".repeat(indent));
        }
        self.source
    }
}
