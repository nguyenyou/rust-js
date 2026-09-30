//! JSX is a compiler-owned syntax expansion, shared by native and WASM.
//! Expand into typed React bindings before name resolution. Original tokens
//! keep their spans; there are no intermediate source files to map through.

pub mod formatting;
mod parser;

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rustc_ast::ast_traits::{HasAttrs, HasTokens};
use rustc_ast::mut_visit::{self, MutVisitor};
use rustc_ast::token::{IdentIsRaw, TokenKind};
use rustc_ast::tokenstream::{DelimSpacing, LazyAttrTokenStream, Spacing, TokenStream, TokenTree};
use rustc_ast::{self as ast, ExprKind, Inline, ItemKind, ModKind};
use rustc_data_structures::stable_hash::{
    RawDefId, RawDefPathHash, RawSpan, StableHashControls, StableHashCtxt, StableHasher,
};
use rustc_expand::config::StripUnconfigured;
use rustc_expand::module::{DirOwnership, default_submod_path};
use rustc_parse::lexer::StripTokens;
use rustc_parse::parser::Parser;
use rustc_parse::{exp, new_parser_from_file};
use rustc_session::Session;
use rustc_span::def_id::StableCrateId;
use rustc_span::hygiene::{ExpnData, ExpnKind, LocalExpnId, MacroKind, Transparency};
use rustc_span::{BytePos, ErrorGuaranteed, FileName, Span, Symbol, sym};

/// Expand JSX within a Rust expression before placing it in a component's
/// props macro. An AST visit selects real expression/statement macros, so
/// tokens inside `stringify!`, macro definitions, etc. remain untouched.
/// Replace only those calls in the original token tree: no pretty-printing
/// round trip, and no loss of the surrounding Rust tokens' source spans.
fn rust_expression(sess: &Session, tokens: TokenStream) -> Result<TokenStream, ErrorGuaranteed> {
    struct Calls<'a> {
        sess: &'a Session,
        replacements: BTreeMap<BytePos, (Span, TokenStream)>,
        error: Option<ErrorGuaranteed>,
    }
    impl Calls<'_> {
        fn mac(&mut self, mac: &ast::MacCall, needs_semicolon: bool) {
            if mac.path.segments.len() == 1
                && mac.path.segments[0].ident.as_str() == "jsx"
                && !expanded_already(&mac.args.tokens)
            {
                match parser::jsx(self.sess, mac.args.tokens.clone(), mac.span()) {
                    Ok(rust) => {
                        // The call itself, `jsx! { @rust_js .. }` (ADR 0113).
                        let path = mac.path.segments[0].ident;
                        let spacing = DelimSpacing::new(Spacing::Alone, Spacing::Alone);
                        let mut tokens = TokenStream::new(vec![
                            TokenTree::token_alone(TokenKind::Ident(path.name, IdentIsRaw::No), path.span),
                            TokenTree::token_alone(TokenKind::Bang, path.span),
                            TokenTree::Delimited(mac.args.dspan, spacing, mac.args.delim, arm(rust, mac.span())),
                        ]);
                        if needs_semicolon {
                            tokens = TokenStream::new(
                                tokens
                                    .iter()
                                    .cloned()
                                    .chain([TokenTree::token_alone(TokenKind::Semi, mac.span().shrink_to_hi())])
                                    .collect(),
                            );
                        }
                        self.replacements.insert(mac.span().lo(), (mac.span(), tokens));
                    }
                    Err(error) => self.error = Some(error),
                }
            }
        }
        fn replace(&self, tokens: &TokenStream) -> TokenStream {
            let mut result = Vec::new();
            let mut it = tokens.iter().peekable();
            while let Some(tree) = it.next() {
                if matches!(tree, TokenTree::Token(..))
                    && let Some((span, value)) = self.replacements.get(&tree.span().lo())
                {
                    result.extend(value.iter().cloned());
                    while it.peek().is_some_and(|t| t.span().hi() <= span.hi()) {
                        it.next();
                    }
                } else if let TokenTree::Delimited(span, spacing, delimiter, inner) = tree {
                    result.push(TokenTree::Delimited(*span, *spacing, *delimiter, self.replace(inner)));
                } else {
                    result.push(tree.clone());
                }
            }
            TokenStream::new(result)
        }
    }
    impl MutVisitor for Calls<'_> {
        fn visit_expr(&mut self, expr: &mut ast::Expr) {
            if let ExprKind::MacCall(mac) = &expr.kind {
                self.mac(mac, false);
            }
            mut_visit::walk_expr(self, expr);
        }
        fn visit_block(&mut self, block: &mut ast::Block) {
            for (i, stmt) in block.stmts.iter().enumerate() {
                if let ast::StmtKind::MacCall(mac) = &stmt.kind {
                    // A braced macro statement can omit `;`, but what it expands
                    // to, a function call, can't. Keep the block's last value intact.
                    self.mac(
                        &mac.mac,
                        mac.style == ast::MacStmtStyle::Braces && i + 1 < block.stmts.len(),
                    );
                }
            }
            mut_visit::walk_block(self, block);
        }
    }
    let mut p = Parser::new(&sess.psess, tokens.clone(), Some("JSX Rust expression"));
    let mut expr = p.parse_expr().map_err(|e| e.emit())?;
    p.expect(exp!(Eof)).map_err(|e| e.emit())?;
    let mut calls = Calls {
        sess,
        replacements: Default::default(),
        error: None,
    };
    calls.visit_expr(&mut expr);
    if let Some(error) = calls.error {
        return Err(error);
    }
    Ok(if calls.replacements.is_empty() {
        tokens
    } else {
        calls.replace(&tokens)
    })
}

/// `crate_id` is the crate's `StableCrateId`, which JSX's expansions are
/// hashed with, as rustc hashes a macro's (ADR 0110).
pub fn expand(sess: &Session, krate: &mut ast::Crate, crate_id: StableCrateId) {
    *CRATE_ID.lock().expect("the crate's id") = Some(crate_id);
    if configured_attrs(sess, &krate.attrs).is_none() {
        return;
    }
    let Some(path) = sess.io.input.opt_path() else { return };
    let mut visitor = Expand {
        sess,
        dir: path.parent().unwrap_or(Path::new("")).to_path_buf(),
        ownership: DirOwnership::Owned { relative: None },
        files: vec![path.to_path_buf()],
    };
    visitor.visit_crate(krate);
}

struct Expand<'a> {
    sess: &'a Session,
    dir: PathBuf,
    ownership: DirOwnership,
    files: Vec<PathBuf>,
}

impl Expand<'_> {
    // rustc owns boxed items in both crate and module ASTs.
    #[allow(clippy::vec_box)]
    fn items(&mut self, items: &mut Vec<Box<ast::Item>>) {
        let mut companions = Vec::new();
        for item in items.iter_mut() {
            // Expand cfg_attr when finding #[path], and never open a cfg'd-out
            // file. Leave actual cfg removal and feature validation to rustc.
            let Some(attrs) = configured_attrs(self.sess, &item.attrs) else {
                continue;
            };
            self.item(item, &attrs);
            companions.extend(parser::thread_local_components(self.sess, item));
            if let Some(companion) = parser::component(self.sess, item) {
                companions.push(companion);
            }
        }
        items.extend(companions);
    }

    fn item(&mut self, item: &mut ast::Item, attrs: &[ast::Attribute]) {
        let ItemKind::Mod(_, ident, kind) = &mut item.kind else {
            mut_visit::walk_item(self, item);
            return;
        };
        let path_attr = attrs.iter().find(|a| a.has_name(sym::path)).and_then(|a| a.value_str());
        let old_dir = self.dir.clone();
        let old_ownership = self.ownership;
        let mut loaded = false;
        match kind {
            ModKind::Unloaded => {
                let path = if let Some(path) = path_attr {
                    self.ownership = DirOwnership::Owned { relative: None };
                    self.dir.join(path.as_str())
                } else {
                    let DirOwnership::Owned { relative } = self.ownership else {
                        self.sess
                            .dcx()
                            .span_err(item.span, "an out-of-line module in a block needs #[path]");
                        return;
                    };
                    match default_submod_path(&self.sess.psess, *ident, relative, &self.dir) {
                        Ok(found) => {
                            self.ownership = found.dir_ownership;
                            found.file_path
                        }
                        Err(_) => {
                            self.sess.dcx().span_err(
                                item.span,
                                format!("cannot locate an unambiguous source file for module `{ident}`"),
                            );
                            return;
                        }
                    }
                };
                if self.files.len() >= 128 || self.files.contains(&path) {
                    self.sess
                        .dcx()
                        .span_err(item.span, "circular or excessively nested module inclusion");
                    self.ownership = old_ownership;
                    return;
                }
                let parsed = new_parser_from_file(
                    &self.sess.psess,
                    &path,
                    StripTokens::ShebangAndFrontmatter,
                    Some(item.span),
                );
                let result = match parsed {
                    Ok(mut p) => p.parse_mod(exp!(Eof)).map_err(|e| e.emit()),
                    Err(errors) => {
                        for e in errors {
                            e.emit();
                        }
                        self.ownership = old_ownership;
                        return;
                    }
                };
                match result {
                    Ok((attrs, items, spans)) => {
                        item.attrs.extend(attrs);
                        *kind = ModKind::Loaded(
                            items,
                            Inline::No {
                                had_parse_error: Ok(()),
                            },
                            spans,
                        );
                    }
                    Err(_) => {
                        self.ownership = old_ownership;
                        return;
                    }
                }
                self.dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
                self.files.push(path);
                loaded = true;
            }
            ModKind::Loaded(..) => {
                if let Some(path) = path_attr {
                    self.dir.push(path.as_str());
                    self.ownership = DirOwnership::Owned { relative: None };
                } else {
                    if let DirOwnership::Owned {
                        relative: Some(relative),
                    } = self.ownership
                    {
                        self.dir.push(relative.as_str());
                        self.ownership = DirOwnership::Owned { relative: None };
                    }
                    self.dir.push(ident.as_str());
                }
            }
        }
        // A module's inner cfg may disable it after it has been read.
        if configured_attrs(self.sess, &item.attrs).is_some()
            && let ItemKind::Mod(_, _, ModKind::Loaded(items, _, _)) = &mut item.kind
        {
            let mut vec = std::mem::take(items).into_iter().collect();
            self.items(&mut vec);
            *items = vec.into_iter().collect();
        }
        if loaded {
            self.files.pop();
        }
        self.dir = old_dir;
        self.ownership = old_ownership;
    }
}

impl Expand<'_> {
    fn statement(&mut self, stmt: &mut ast::Stmt) {
        let span = stmt.span;
        if let ast::StmtKind::MacCall(mac) = &mut stmt.kind
            && mac.mac.path.segments.len() == 1
            && mac.mac.path.segments[0].ident.as_str() == "jsx"
            && !expanded_already(&mac.mac.args.tokens)
            && let Ok(rust) = parser::jsx(self.sess, mac.mac.args.tokens.clone(), span)
        {
            mac.mac.args.tokens = arm(rust, span);
        }
    }
}

impl MutVisitor for Expand<'_> {
    fn visit_crate(&mut self, krate: &mut ast::Crate) {
        let mut items = std::mem::take(&mut krate.items).into_iter().collect();
        self.items(&mut items);
        krate.items = items.into_iter().collect();
    }

    fn visit_item(&mut self, item: &mut ast::Item) {
        if let Some(attrs) = configured_attrs(self.sess, &item.attrs) {
            self.item(item, &attrs);
        }
    }

    fn visit_block(&mut self, block: &mut ast::Block) {
        let old = self.ownership;
        self.ownership = DirOwnership::UnownedViaBlock;
        for stmt in &mut block.stmts {
            self.statement(stmt);
        }
        mut_visit::walk_block(self, block);
        self.ownership = old;
    }

    fn visit_expr(&mut self, expr: &mut ast::Expr) {
        let span = expr.span;
        if let ExprKind::MacCall(mac) = &mut expr.kind
            && mac.path.segments.len() == 1
            && mac.path.segments[0].ident.as_str() == "jsx"
            && !expanded_already(&mac.args.tokens)
            && let Ok(rust) = parser::jsx(self.sess, mac.args.tokens.clone(), span)
        {
            mac.args.tokens = arm(rust, span);
        }
        mut_visit::walk_expr(self, expr);
    }
}

// Check cfg without cloning entire module trees or changing the attributes
// rustc will subsequently validate. Also works for the crate root.
pub fn configured_attrs(sess: &Session, attrs: &ast::AttrVec) -> Option<ast::AttrVec> {
    struct Attributes(ast::AttrVec);
    impl HasAttrs for Attributes {
        const SUPPORTS_CUSTOM_INNER_ATTRS: bool = true;
        fn attrs(&self) -> &[ast::Attribute] {
            &self.0
        }
        fn visit_attrs(&mut self, f: impl FnOnce(&mut ast::AttrVec)) {
            f(&mut self.0);
        }
    }
    impl HasTokens for Attributes {
        fn tokens(&self) -> Option<&LazyAttrTokenStream> {
            None
        }
        fn tokens_mut(&mut self) -> Option<&mut Option<LazyAttrTokenStream>> {
            None
        }
    }
    // A lint this raises, such as `unexpected_cfgs`, is rustc's to raise when
    // it configures the crate itself, for the node it's about. Raised here,
    // for none, it's never emitted, and rustc's check that each was fails.
    let raised = sess.psess.buffered_lints.with_lock(|lints| lints.len());
    let configured = StripUnconfigured {
        sess,
        features: None,
        config_tokens: false,
        lint_node_id: ast::DUMMY_NODE_ID,
    }
    .configure(Attributes(attrs.clone()))
    .map(|attrs| attrs.0);
    sess.psess.buffered_lints.with_lock(|lints| lints.truncate(raised));
    configured
}

/// `@rust_js ..`: a `jsx!` call's JSX, as the Rust rust-js writes for it,
/// which react's `jsx!` passes on as it is, where a plain rustc's is its
/// placeholder (ADR 0113). The call stays, so `use react::jsx;` is used, and
/// the macro is found as a plain rustc finds it.
fn arm(rust: TokenStream, span: Span) -> TokenStream {
    let marker = [
        TokenTree::token_alone(TokenKind::At, span),
        TokenTree::token_alone(TokenKind::Ident(Symbol::intern("rust_js"), IdentIsRaw::No), span),
    ];
    TokenStream::new(marker.into_iter().chain(rust.iter().cloned()).collect())
}

/// Is it a `jsx!` rust-js has expanded, `jsx! { @rust_js .. }`?
fn expanded_already(tokens: &TokenStream) -> bool {
    let mut trees = tokens.iter();
    matches!(trees.next(), Some(TokenTree::Token(t, _)) if t.kind == TokenKind::At)
        && matches!(trees.next(), Some(TokenTree::Token(t, _)) if matches!(t.kind, TokenKind::Ident(name, _) if name.as_str() == "rust_js"))
}

/// The span of code JSX expands to, which may use what it needs of rustc's
/// unstable features, as std's macros do: attributes on expressions,
/// `#[rust_js::jsx] f(..)`, and a component's props `macro`, which its own
/// expansions may too. A program's own code may not (ADR 0110). Transparent,
/// so names resolve as they're written.
fn expanded(sess: &Session, span: Span) -> Span {
    // Formatting JSX, `--format-jsx`, compiles no crate: its spans are only printed.
    if CRATE_ID.lock().expect("the crate's id").is_none() {
        return span;
    }
    let allowed: Arc<[Symbol]> = Arc::from([sym::stmt_expr_attributes, sym::decl_macro, sym::allow_internal_unstable]);
    let kind = ExpnKind::Macro(MacroKind::Bang, Symbol::intern("jsx"));
    let data = ExpnData::allow_unstable(kind, span, sess.edition(), allowed, None, None);
    let expansion = LocalExpnId::fresh(data, ExpansionHash);
    span.apply_mark(expansion.to_expn_id(), Transparency::Transparent)
}

/// The crate's `StableCrateId`, rustc's once it has the crate's context,
/// which an expansion's hash starts with: a crate using this one's macros
/// finds their spans' expansions by it.
static CRATE_ID: Mutex<Option<StableCrateId>> = Mutex::new(None);

/// Is `rustc`'s `StableCrateId` for the crate the one JSX's expansions were
/// hashed with? A crate using its macros wouldn't find their spans otherwise.
pub fn check_crate_id(rustc: StableCrateId) {
    let ours = *CRATE_ID.lock().expect("the crate's id");
    assert!(
        ours.is_none_or(|ours| ours == rustc),
        "rust-js: JSX's expansions were hashed with another crate id"
    );
}

/// What a JSX expansion's data is hashed with: its span, as it names no item
/// but the crate, whose id the expansion's hash starts with. The rest only
/// tells expansions apart within the session, which rustc's disambiguator
/// does too.
struct ExpansionHash;

impl StableHashCtxt for ExpansionHash {
    fn stable_hash_span(&mut self, span: RawSpan, hasher: &mut StableHasher) {
        hasher.write_u32(span.0);
        hasher.write_u16(span.1);
        hasher.write_u16(span.2);
    }

    /// The crate's own, whose crate half alone rustc reads.
    fn def_path_hash(&self, def_id: RawDefId) -> RawDefPathHash {
        assert!(
            def_id.0 == 0 && def_id.1 == 0,
            "a JSX expansion names no item but the crate"
        );
        let id = CRATE_ID.lock().expect("the crate's id").expect("set by `expand`");
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&id.as_u64().to_le_bytes());
        RawDefPathHash(bytes)
    }

    fn stable_hash_controls(&self) -> StableHashControls {
        StableHashControls { hash_spans: true }
    }

    fn assert_default_stable_hash_controls(&self, _: &str) {}
}

fn template(sess: &Session, source: String, span: Span) -> rustc_ast::tokenstream::TokenStream {
    let span = expanded(sess, span);
    let mut hash = std::hash::DefaultHasher::new();
    source.hash(&mut hash);
    rustc_parse::source_str_to_stream(
        &sess.psess,
        FileName::Custom(format!("jsx expansion {:x}", hash.finish())),
        source,
        Some(span),
    )
    .unwrap_or_else(|errors| {
        for e in errors {
            e.emit();
        }
        Default::default()
    })
}
