//! Our JS AST ──► oxc's AST ──► JS text + source map.
//!
//! This is the only file that uses oxc. oxc is pre-1.0 and its API changes
//! often, so keeping it here means an oxc upgrade touches one file.
//!
//! ```text
//!   js::Module ──convert──► oxc Program ──oxc_codegen──► code + map
//!                           (source_text = the .rs file)
//!
//!   final .js  =  header, imports, helpers  (plain text, no mappings)
//!              +  code, blank line between functions
//!                                             (map shifted to match)
//!              +  //# sourceMappingURL=...
//! ```
//!
//! The trick that makes the map point at Rust: oxc computes line/column for
//! each node from `program.source_text` and the node's span. We hand it the
//! *Rust* file as `source_text`, and spans that are byte offsets into it.

use std::cell::Cell;
use std::path::PathBuf;

use oxc_allocator::{Allocator, ArenaBox, ArenaVec};
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, ArrowFunctionBody, AssignmentTarget, BindingIdentifier, BindingPattern,
    BindingProperty, Declaration, Expression, ForStatementInit, ForStatementLeft, FormalParameter, FormalParameterKind,
    FormalParameters, FunctionBody, FunctionType, IdentifierName, JSXAttributeItem, JSXAttributeName,
    JSXAttributeValue, JSXChild, JSXClosingElement, JSXClosingFragment, JSXElementName, JSXExpression, JSXIdentifier,
    JSXMemberExpressionObject, JSXOpeningElement, JSXOpeningFragment, LabelIdentifier, ObjectPropertyKind, Program,
    PropertyKey, PropertyKind, SimpleAssignmentTarget, Statement, VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast::builder::AstBuilder;
use oxc_codegen::{Codegen, CodegenOptions, IndentChar};
use oxc_sourcemap::{SourceMap, SourceMapBuilder};
use oxc_span::{SPAN, SourceType, Span};
use oxc_syntax::number::NumberBase;
use oxc_syntax::operator::{AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator};

use crate::js::{self, ExprKind, JsxTag, Module, Op, Prop, StmtKind, UnaryOp};

pub struct Output {
    pub code: String,
    /// The source map, as JSON.
    pub map: String,
}

/// Print `module` as JS, with a source map pointing into `rust_source`.
///
/// `source_path` is how the map names the Rust file (relative to the map),
/// and `js_file_name` is the output's file name, for `sourceMappingURL`.
pub fn emit(module: &Module, rust_source: &str, source_path: &str, js_file_name: &str) -> Output {
    let allocator = Allocator::default();
    let cx = Cx {
        b: AstBuilder::new(&allocator),
        allocator: &allocator,
        depth: Cell::new(0),
        inline: Cell::new(false),
    };
    let b = &cx.b;

    let namespaces = module.namespaces.iter().map(|n| cx.namespace(n));
    let consts = module.consts.iter().map(|c| cx.constant(c));
    let body = ArenaVec::from_iter_in(
        namespaces
            .chain(consts)
            .chain(module.functions.iter().map(|f| cx.function(f))),
        b,
    );
    let program = Program::new(
        Span::new(0, rust_source.len() as u32),
        SourceType::mjs(),
        rust_source,
        ArenaVec::new_in(b), // comments
        None,                // hashbang
        ArenaVec::new_in(b), // directives
        body,
        b,
    );
    let options = CodegenOptions {
        indent_char: IndentChar::Space,
        indent_width: 2,
        source_map_path: Some(PathBuf::from(source_path)),
        ..CodegenOptions::default()
    };
    let generated = Codegen::new().with_options(options).build(&program);

    // The header, imports and runtime helpers are plain text above the
    // generated code. None of them map to Rust.
    let mut code = format!("{}\n", module.header);
    if !module.packages.is_empty() {
        code.push('\n');
        for package in &module.packages {
            let named: Vec<String> = package
                .named
                .iter()
                .map(|(export, local)| {
                    if export == local {
                        export.clone()
                    } else {
                        format!("{export} as {local}")
                    }
                })
                .collect();
            let named = (!named.is_empty()).then(|| format!("{{ {} }}", named.join(", ")));
            let clause: Vec<String> = package.default.iter().cloned().chain(named).collect();
            if !clause.is_empty() {
                code.push_str(&format!("import {} from {:?};\n", clause.join(", "), package.from));
            }
            if let Some(namespace) = &package.namespace {
                code.push_str(&format!("import * as {namespace} from {:?};\n", package.from));
            }
            if clause.is_empty() && package.namespace.is_none() {
                code.push_str(&format!("import {:?};\n", package.from));
            }
        }
    }
    if !module.imports.is_empty() {
        code.push('\n');
        for import in &module.imports {
            code.push_str(&format!("import * as {} from {:?};\n", import.alias, import.from));
        }
    }
    for helper in &module.runtime {
        code.push('\n');
        code.push_str(helper.trim_start());
    }
    if !module.caches.is_empty() {
        code.push_str(&format!("\nvar {};\n", module.caches.join(", ")));
    }
    code.push('\n');

    // oxc prints functions back to back; put a blank line between them.
    // oxc also puts an object of one property on one line, so a type with one
    // method comes out `const Tally = { doubled(tally) {`: lay that out as an
    // object of several, the method on its own lines. Record where each part
    // of each generated line ends up, to fix the map.
    let one_method: Vec<String> = module
        .namespaces
        .iter()
        .filter(|n| n.methods.len() == 1)
        .map(|n| format!("{}const {} = {{ ", if n.export { "export " } else { "" }, n.name))
        .collect();
    let mut out_line = code.matches('\n').count() as u32;
    let mut places: Vec<Vec<Place>> = Vec::new();
    let mut previous = None;
    let mut in_object = false;
    for (i, line) in generated.code.lines().enumerate() {
        // Only functions start at column 0, so this can't match nested code.
        let top_level = [
            "function ",
            "async function ",
            "export function ",
            "export async function ",
        ]
        .iter()
        .any(|p| line.starts_with(p));
        // And after a type's methods, which end the object that holds them.
        if i > 0 && (top_level || matches!(previous, Some("};" | "} };"))) {
            code.push('\n');
            out_line += 1;
        }
        let mut put = |text: &str, from_col: u32, delta: i64, parts: &mut Vec<Place>| {
            code.push_str(text);
            code.push('\n');
            parts.push(Place {
                from_col,
                line: out_line,
                delta,
            });
            out_line += 1;
        };
        let mut parts = Vec::new();
        let opening = one_method
            .iter()
            .find(|prefix| line.starts_with(prefix.as_str()) && line.ends_with('{'));
        match opening {
            Some(prefix) if !in_object => {
                let at = prefix.len() as u32;
                put(prefix.trim_end(), 0, 0, &mut parts);
                put(
                    &format!("  {}", &line[prefix.len()..]),
                    at,
                    2 - i64::from(at),
                    &mut parts,
                );
                in_object = true;
            }
            _ if in_object && line == "} };" => {
                put("  }", 0, 2, &mut parts);
                put("};", 1, -2, &mut parts);
                in_object = false;
            }
            _ if in_object => put(&format!("  {line}"), 0, 2, &mut parts),
            _ => put(line, 0, 0, &mut parts),
        }
        places.push(parts);
        previous = Some(line);
    }
    code.push_str(&format!("//# sourceMappingURL={js_file_name}.map\n"));

    let map = generated.map.expect("a source map, since source_map_path is set");
    Output {
        code,
        map: shift_lines(&map, &places, js_file_name),
    }
}

/// Where the part of a generated line from `from_col` on ends up: on output
/// line `line`, `delta` columns over.
struct Place {
    from_col: u32,
    line: u32,
    delta: i64,
}

/// Rebuild `map` with each part of each generated line where `places` says.
fn shift_lines(map: &SourceMap<'_>, places: &[Vec<Place>], js_file_name: &str) -> String {
    let mut out = SourceMapBuilder::default();
    out.set_file(js_file_name);
    for (source, content) in map.get_sources().zip(map.get_source_contents()) {
        out.set_source_and_content(source, content.unwrap_or_default());
    }
    // `add_name` deduplicates, so ids can change: translate them.
    let name_ids: Vec<u32> = map.get_names().map(|name| out.add_name(name)).collect();
    // Past the last line, lines keep the last one's shift.
    let last_shift = places
        .last()
        .and_then(|parts| parts.last())
        .map_or(0, |p| p.line + 1 - places.len() as u32);
    for t in map.get_tokens() {
        let (line, col) = (t.get_dst_line(), t.get_dst_col());
        let (line, col) = match places
            .get(line as usize)
            .and_then(|parts| parts.iter().rev().find(|p| p.from_col <= col))
        {
            Some(place) => (place.line, (i64::from(col) + place.delta).max(0) as u32),
            None => (line + last_shift, col),
        };
        out.add_token(
            line,
            col,
            t.get_src_line(),
            t.get_src_col(),
            t.get_source_id(),
            t.get_name_id().map(|id| name_ids[id as usize]),
        );
    }
    out.into_sourcemap().to_json_string()
}

struct Cx<'a> {
    b: AstBuilder<'a>,
    allocator: &'a Allocator,
    /// How many levels oxc indents what's being converted: one per block,
    /// and per array of 3 or more items or object of 2 or more fields, which
    /// oxc puts on several lines. JSX laid out on several lines indents to match.
    depth: Cell<u32>,
    /// Inside JSX that's on one line, where everything stays on it.
    inline: Cell<bool>,
}

impl<'a> Cx<'a> {
    fn params(&self, kind: FormalParameterKind, patterns: &[js::Pattern]) -> FormalParameters<'a> {
        let b = &self.b;
        let params = patterns.iter().map(|pattern| {
            FormalParameter::new(
                SPAN,
                ArenaVec::new_in(b),
                self.pattern(pattern),
                None,
                None,
                false,
                None,
                false,
                false,
                b,
            )
        });
        FormalParameters::new(SPAN, kind, ArenaVec::from_iter_in(params, b), None, b)
    }

    fn function(&self, f: &js::Function) -> Statement<'a> {
        let b = &self.b;
        let params = self.params(FormalParameterKind::FormalParameter, &f.params);
        let body = FunctionBody::new(SPAN, ArenaVec::new_in(b), self.stmts(&f.body), b);
        let decl = Declaration::new_function_declaration(
            span(f.span),
            FunctionType::FunctionDeclaration,
            Some(BindingIdentifier::new(span(f.name_span), self.name(&f.name), b)),
            false, // generator
            f.is_async,
            false, // declare
            None,  // type parameters
            None,  // this param
            ArenaBox::new_in(params, b),
            None, // return type
            Some(ArenaBox::new_in(body, b)),
            b,
        );
        if f.export {
            Statement::new_export_declaration(span(f.span), decl, b)
        } else {
            decl.into()
        }
    }

    /// `export const Counter = { new(step) { .. }, .. };`: each method in
    /// shorthand, as a hand-written object of functions has them.
    fn namespace(&self, n: &js::Namespace) -> Statement<'a> {
        let b = &self.b;
        let methods = self.nested(true, || {
            ArenaVec::from_iter_in(
                n.methods.iter().map(|f| {
                    let params = self.params(FormalParameterKind::FormalParameter, &f.params);
                    let body = FunctionBody::new(SPAN, ArenaVec::new_in(b), self.stmts(&f.body), b);
                    let value = Expression::new_function_expression(
                        span(f.span),
                        FunctionType::FunctionExpression,
                        None,
                        false, // generator
                        f.is_async,
                        false, // declare
                        None,  // type parameters
                        None,  // this param
                        ArenaBox::new_in(params, b),
                        None, // return type
                        Some(ArenaBox::new_in(body, b)),
                        b,
                    );
                    let key = PropertyKey::new_static_identifier(span(f.name_span), self.name(&f.name), b);
                    ObjectPropertyKind::new_object_property(
                        span(f.span),
                        PropertyKind::Init,
                        key,
                        value,
                        true,
                        false,
                        false,
                        b,
                    )
                }),
                b,
            )
        });
        let id = BindingPattern::new_binding_identifier(SPAN, self.name(&n.name), b);
        let object = Expression::new_object_expression(SPAN, methods, b);
        let declarator = VariableDeclarator::new(SPAN, id, None, Some(object), false, b);
        let decl = Declaration::new_variable_declaration(
            SPAN,
            VariableDeclarationKind::Const,
            ArenaVec::from_iter_in([declarator], b),
            false,
            b,
        );
        if n.export {
            Statement::new_export_declaration(SPAN, decl, b)
        } else {
            decl.into()
        }
    }

    fn constant(&self, c: &js::Const) -> Statement<'a> {
        let b = &self.b;
        let sp = span(c.span);
        let id = BindingPattern::new_binding_identifier(SPAN, self.name(&c.name), b);
        let declarator = VariableDeclarator::new(sp, id, None, Some(self.expr(&c.value)), false, b);
        let decl = Declaration::new_variable_declaration(
            sp,
            VariableDeclarationKind::Const,
            ArenaVec::from_iter_in([declarator], b),
            false,
            b,
        );
        if c.export {
            Statement::new_export_declaration(sp, decl, b)
        } else {
            decl.into()
        }
    }

    fn stmts(&self, stmts: &[js::Stmt]) -> ArenaVec<'a, Statement<'a>> {
        self.nested(true, || {
            ArenaVec::from_iter_in(stmts.iter().map(|s| self.stmt(s)), &self.b)
        })
    }

    /// Run `f` one level deeper, if `deeper`.
    fn nested<T>(&self, deeper: bool, f: impl FnOnce() -> T) -> T {
        let depth = self.depth.get();
        self.depth.set(depth + u32::from(deeper));
        let result = f();
        self.depth.set(depth);
        result
    }

    fn block(&self, stmts: &[js::Stmt]) -> Statement<'a> {
        Statement::new_block_statement(SPAN, self.stmts(stmts), &self.b)
    }

    fn stmt(&self, s: &js::Stmt) -> Statement<'a> {
        let b = &self.b;
        let sp = span(s.span);
        match &s.kind {
            StmtKind::Const(name, init) => self.declare(sp, VariableDeclarationKind::Const, name, Some(init)),
            StmtKind::Let(name, init) => self.declare(sp, VariableDeclarationKind::Let, name, init.as_ref()),
            StmtKind::Destructure {
                pattern,
                value,
                mutable,
            } => {
                let kind = if *mutable {
                    VariableDeclarationKind::Let
                } else {
                    VariableDeclarationKind::Const
                };
                let declarator =
                    VariableDeclarator::new(sp, self.pattern(pattern), None, Some(self.expr(value)), false, b);
                Statement::new_variable_declaration(sp, kind, ArenaVec::from_iter_in([declarator], b), false, b)
            }
            StmtKind::Assign(target, value) => {
                // `s = s + t` is `s += t`, as JS writes a string built up.
                let (operator, value) = match &value.kind {
                    ExprKind::Binary(Op::Add, lhs, rhs) if same_place(lhs, target) => {
                        (AssignmentOperator::Addition, &**rhs)
                    }
                    _ => (AssignmentOperator::Assign, value),
                };
                let assign = Expression::new_assignment_expression(
                    sp,
                    operator,
                    self.assignment_target(target),
                    self.expr(value),
                    b,
                );
                Statement::new_expression_statement(sp, assign, b)
            }
            StmtKind::Expr(e) => Statement::new_expression_statement(sp, self.expr(e), b),
            StmtKind::If(cond, then, els) => {
                let els = els.as_deref().map(|els| match els {
                    // A lone `if` in the `else` prints as `else if`.
                    [
                        only @ js::Stmt {
                            kind: StmtKind::If(..), ..
                        },
                    ] => self.stmt(only),
                    _ => self.block(els),
                });
                Statement::new_if_statement(sp, self.expr(cond), self.block(then), els, b)
            }
            StmtKind::Labeled(label, body) => {
                let block = Statement::new_block_statement(sp, self.stmts(body), b);
                self.labeled(sp, Some(label), block)
            }
            StmtKind::While { label, cond, body } => {
                let w = Statement::new_while_statement(sp, self.expr(cond), self.block(body), b);
                self.labeled(sp, label.as_deref(), w)
            }
            StmtKind::ForOf {
                label,
                name,
                iterable,
                body,
            } => {
                let id = BindingPattern::new_binding_identifier(SPAN, self.name(name), b);
                let declarator = VariableDeclarator::new(SPAN, id, None, None, false, b);
                let left = ForStatementLeft::new_variable_declaration(
                    SPAN,
                    VariableDeclarationKind::Const,
                    ArenaVec::from_iter_in([declarator], b),
                    false,
                    b,
                );
                let l = Statement::new_for_of_statement(sp, false, left, self.expr(iterable), self.block(body), b);
                self.labeled(sp, label.as_deref(), l)
            }
            StmtKind::For {
                label,
                name,
                start,
                test,
                body,
            } => {
                let id = BindingPattern::new_binding_identifier(SPAN, self.name(name), b);
                let declarator = VariableDeclarator::new(SPAN, id, None, Some(self.expr(start)), false, b);
                let init = ForStatementInit::new_variable_declaration(
                    SPAN,
                    VariableDeclarationKind::Let,
                    ArenaVec::from_iter_in([declarator], b),
                    false,
                    b,
                );
                let counter = SimpleAssignmentTarget::new_assignment_target_identifier(SPAN, self.name(name), b);
                let update = Expression::new_update_expression(SPAN, UpdateOperator::Increment, false, counter, b);
                let l = Statement::new_for_statement(
                    sp,
                    Some(init),
                    Some(self.expr(test)),
                    Some(update),
                    self.block(body),
                    b,
                );
                self.labeled(sp, label.as_deref(), l)
            }
            StmtKind::Break(label) => Statement::new_break_statement(sp, label.as_deref().map(|l| self.label(l)), b),
            StmtKind::Continue(label) => {
                Statement::new_continue_statement(sp, label.as_deref().map(|l| self.label(l)), b)
            }
            StmtKind::Throw(value) => Statement::new_throw_statement(sp, self.expr(value), b),
            StmtKind::Return(value) => Statement::new_return_statement(sp, value.as_ref().map(|v| self.expr(v)), b),
        }
    }

    fn pattern(&self, pattern: &js::Pattern) -> BindingPattern<'a> {
        let b = &self.b;
        let name = |name: &str| BindingPattern::new_binding_identifier(SPAN, self.name(name), b);
        match pattern {
            js::Pattern::Name(n) => name(n),
            js::Pattern::Array(items) => {
                let items = items.iter().map(|item| item.as_deref().map(name));
                BindingPattern::new_array_pattern(SPAN, ArenaVec::from_iter_in(items, b), None, b)
            }
            js::Pattern::Object(fields) => {
                let fields = fields.iter().map(|(field, var)| {
                    let key = PropertyKey::new_static_identifier(SPAN, self.name(field), b);
                    // `{ x }` for `{ x: x }`.
                    BindingProperty::new(SPAN, key, name(var), field == var, false, b)
                });
                BindingPattern::new_object_pattern(SPAN, ArenaVec::from_iter_in(fields, b), None, b)
            }
        }
    }

    fn assignment_target(&self, e: &js::Expr) -> AssignmentTarget<'a> {
        let b = &self.b;
        let sp = span(e.span);
        match &e.kind {
            ExprKind::Var(name) => AssignmentTarget::new_assignment_target_identifier(sp, self.name(name), b),
            ExprKind::Member(object, property) => AssignmentTarget::new_static_member_expression(
                sp,
                self.expr(object),
                IdentifierName::new(SPAN, self.name(property), b),
                false,
                b,
            ),
            ExprKind::Index(object, index) => {
                AssignmentTarget::new_computed_member_expression(sp, self.expr(object), self.expr(index), false, b)
            }
            _ => unreachable!("lowering only assigns to variables and fields"),
        }
    }

    fn labeled(&self, sp: Span, label: Option<&str>, s: Statement<'a>) -> Statement<'a> {
        match label {
            Some(l) => Statement::new_labeled_statement(sp, self.label(l), s, &self.b),
            None => s,
        }
    }

    fn declare(&self, sp: Span, kind: VariableDeclarationKind, name: &str, init: Option<&js::Expr>) -> Statement<'a> {
        let b = &self.b;
        let id = BindingPattern::new_binding_identifier(SPAN, self.name(name), b);
        let declarator = VariableDeclarator::new(sp, id, None, init.map(|e| self.expr(e)), false, b);
        Statement::new_variable_declaration(sp, kind, ArenaVec::from_iter_in([declarator], b), false, b)
    }

    fn expr(&self, e: &js::Expr) -> Expression<'a> {
        let b = &self.b;
        let sp = span(e.span);
        match &e.kind {
            ExprKind::Num(n) => self.number(sp, *n),
            ExprKind::Bool(v) => Expression::new_boolean_literal(sp, *v, b),
            ExprKind::Str(s) => Expression::new_string_literal(sp, self.allocator.alloc_str(s), None, b),
            ExprKind::Undefined => Expression::new_identifier(sp, "undefined", b),
            ExprKind::Null => Expression::new_null_literal(sp, b),
            ExprKind::Var(name) => Expression::new_identifier(sp, self.name(name), b),
            ExprKind::Member(object, property) => Expression::new_static_member_expression(
                sp,
                self.expr(object),
                IdentifierName::new(SPAN, self.name(property), b),
                false,
                b,
            ),
            ExprKind::Index(object, index) => {
                Expression::new_computed_member_expression(sp, self.expr(object), self.expr(index), false, b)
            }
            ExprKind::Array(items) => self.nested(items.len() > 2, || {
                let items = items.iter().map(|item| ArrayExpressionElement::from(self.expr(item)));
                Expression::new_array_expression(sp, ArenaVec::from_iter_in(items, b), b)
            }),
            ExprKind::Object(props) => self.nested(props.len() > 1, || {
                let props = props.iter().map(|prop| self.property(prop));
                Expression::new_object_expression(sp, ArenaVec::from_iter_in(props, b), b)
            }),
            ExprKind::Jsx(jsx) => self.jsx(sp, jsx),
            ExprKind::Unary(op, arg) => {
                let op = match op {
                    UnaryOp::Neg => UnaryOperator::UnaryNegation,
                    UnaryOp::Not => UnaryOperator::LogicalNot,
                    UnaryOp::BitNot => UnaryOperator::BitwiseNot,
                };
                Expression::new_unary_expression(sp, op, self.expr(arg), b)
            }
            ExprKind::Binary(op, l, r) => {
                let (l, r) = (self.expr(l), self.expr(r));
                match binary_op(*op) {
                    Ok(op) => Expression::new_binary_expression(sp, l, op, r, b),
                    Err(op) => Expression::new_logical_expression(sp, l, op, r, b),
                }
            }
            ExprKind::Cond(test, then, els) => {
                Expression::new_conditional_expression(sp, self.expr(test), self.expr(then), self.expr(els), b)
            }
            ExprKind::Arrow(params, body) | ExprKind::AsyncArrow(params, body) => {
                let is_async = matches!(e.kind, ExprKind::AsyncArrow(..));
                let params = ArenaBox::new_in(self.params(FormalParameterKind::ArrowFormalParameters, params), b);
                // `() => x` when the body only returns a value.
                let body = match body.as_slice() {
                    [
                        js::Stmt {
                            kind: StmtKind::Return(Some(value)),
                            ..
                        },
                    ] => ArrowFunctionBody::from(self.expr(value)),
                    _ => ArrowFunctionBody::new_function_body(SPAN, ArenaVec::new_in(b), self.stmts(body), b),
                };
                Expression::new_arrow_function_expression(sp, is_async, None, params, None, body, b)
            }
            ExprKind::Await(promise) => Expression::new_await_expression(sp, self.expr(promise), b),
            ExprKind::Call(callee, args) => {
                let args = args.iter().map(|a| Argument::from(self.expr(a)));
                Expression::new_call_expression(sp, self.expr(callee), None, ArenaVec::from_iter_in(args, b), false, b)
            }
            ExprKind::New(callee, args) => {
                let args = args.iter().map(|a| Argument::from(self.expr(a)));
                Expression::new_new_expression(sp, self.expr(callee), None, ArenaVec::from_iter_in(args, b), b)
            }
        }
    }

    /// A JSX element (ADR 0040), laid out as by hand: children that are all
    /// elements or expressions go on their own lines, one level deeper. With
    /// text among them they stay on one line, where JSX keeps every space.
    fn jsx(&self, sp: Span, jsx: &js::Jsx) -> Expression<'a> {
        let b = &self.b;
        let has_text = jsx.children.iter().any(|c| matches!(c.kind, ExprKind::Str(_)));
        let lines = !has_text && !self.inline.get() && jsx.children.iter().any(js::Expr::contains_jsx);
        let depth = self.depth.get();
        let mut children = ArenaVec::new_in(b);
        let inline = self.inline.replace(self.inline.get() || has_text);
        self.nested(lines, || {
            for child in &jsx.children {
                if lines {
                    children.push(self.jsx_newline(depth + 1));
                }
                children.push(self.jsx_child(child));
            }
        });
        self.inline.set(inline);
        if lines {
            children.push(self.jsx_newline(depth));
        }
        if let JsxTag::Fragment = jsx.tag {
            let (open, close) = (JSXOpeningFragment::new(SPAN, b), JSXClosingFragment::new(SPAN, b));
            return Expression::new_jsx_fragment(sp, open, children, close, b);
        }
        let attrs = jsx.props.iter().filter_map(|prop| self.jsx_attribute(prop));
        let opening =
            JSXOpeningElement::boxed(SPAN, self.jsx_name(&jsx.tag), None, ArenaVec::from_iter_in(attrs, b), b);
        // `<img />` has nothing to close.
        let closing = (!children.is_empty()).then(|| JSXClosingElement::boxed(SPAN, self.jsx_name(&jsx.tag), b));
        Expression::new_jsx_element(sp, opening, children, closing, b)
    }

    /// `div`, `Counter`, or `stats.Chart` from another module.
    fn jsx_name(&self, tag: &JsxTag) -> JSXElementName<'a> {
        let b = &self.b;
        let component = match tag {
            JsxTag::Intrinsic(tag) => return JSXElementName::new_identifier(SPAN, self.name(tag), b),
            JsxTag::Component(component) => component,
            JsxTag::Fragment => unreachable!("a fragment has no name"),
        };
        match &component.kind {
            ExprKind::Var(name) => JSXElementName::new_identifier_reference(SPAN, self.name(name), b),
            ExprKind::Member(object, property) => {
                let property = JSXIdentifier::new(SPAN, self.name(property), b);
                JSXElementName::new_member_expression(SPAN, self.jsx_object(object), property, b)
            }
            _ => unreachable!("lowering only makes components of names and paths"),
        }
    }

    fn jsx_object(&self, object: &js::Expr) -> JSXMemberExpressionObject<'a> {
        let b = &self.b;
        match &object.kind {
            ExprKind::Var(name) => JSXMemberExpressionObject::new_identifier_reference(SPAN, self.name(name), b),
            ExprKind::Member(inner, property) => {
                let property = JSXIdentifier::new(SPAN, self.name(property), b);
                JSXMemberExpressionObject::new_member_expression(SPAN, self.jsx_object(inner), property, b)
            }
            _ => unreachable!("lowering only makes components of names and paths"),
        }
    }

    /// `className="hero"`, `disabled` for `true`, `onClick={f}` or `{...props}`.
    /// An attribute that's `undefined` (a `None`) is left out, as React would.
    fn jsx_attribute(&self, prop: &Prop) -> Option<JSXAttributeItem<'a>> {
        let b = &self.b;
        let (name, value) = match prop {
            Prop::Spread(value) => return Some(JSXAttributeItem::new_spread_attribute(SPAN, self.expr(value), b)),
            Prop::Field(name, value) => (name, value),
        };
        let sp = span(value.span);
        let value = match &value.kind {
            ExprKind::Undefined => return None,
            ExprKind::Bool(true) => None,
            ExprKind::Str(s) if jsx_text_safe(s) && !s.contains('"') => {
                Some(JSXAttributeValue::new_string_literal(sp, self.name(s), None, b))
            }
            _ => Some(JSXAttributeValue::new_expression_container(
                sp,
                JSXExpression::from(self.expr(value)),
                b,
            )),
        };
        let name = JSXAttributeName::new_identifier(SPAN, self.name(name), b);
        Some(JSXAttributeItem::new_attribute(SPAN, name, value, b))
    }

    /// Text as text, `Count is `; anything else in braces, `{count}`.
    fn jsx_child(&self, child: &js::Expr) -> JSXChild<'a> {
        let b = &self.b;
        let sp = span(child.span);
        match &child.kind {
            ExprKind::Str(s) if jsx_text_safe(s) && !s.is_empty() => JSXChild::new_text(sp, self.name(s), None, b),
            ExprKind::Jsx(jsx) => match self.jsx(sp, jsx) {
                Expression::JSXElement(e) => JSXChild::Element(e),
                Expression::JSXFragment(f) => JSXChild::Fragment(f),
                _ => unreachable!("JSX converts to JSX"),
            },
            _ => JSXChild::new_expression_container(sp, JSXExpression::from(self.expr(child)), b),
        }
    }

    /// A line break and indentation between children, which JSX ignores.
    fn jsx_newline(&self, depth: u32) -> JSXChild<'a> {
        let text = format!("\n{}", "  ".repeat(depth as usize));
        JSXChild::new_text(SPAN, self.name(&text), None, &self.b)
    }

    fn property(&self, prop: &Prop) -> ObjectPropertyKind<'a> {
        let b = &self.b;
        match prop {
            Prop::Field(name, value) => {
                let value = self.expr(value);
                // `{ x: x }` reads better as `{ x }`.
                let shorthand = matches!(&value, Expression::Identifier(id) if id.name == name.as_str());
                // In a literal, a plain `__proto__:` key sets the prototype. Quoted
                // and computed, it's an ordinary field, like any other Rust field.
                let computed = name == "__proto__";
                // A key that isn't a JS name, like a CSS custom property's
                // `--gap`, is quoted.
                let identifier = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$')
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
                let key = if computed || !identifier {
                    PropertyKey::new_string_literal(SPAN, self.name(name), None, b)
                } else {
                    PropertyKey::new_static_identifier(SPAN, self.name(name), b)
                };
                ObjectPropertyKind::new_object_property(
                    SPAN,
                    PropertyKind::Init,
                    key,
                    value,
                    false,
                    shorthand,
                    computed,
                    b,
                )
            }
            Prop::Spread(value) => ObjectPropertyKind::new_spread_property(SPAN, self.expr(value), b),
        }
    }

    /// oxc prints numbers in their shortest form, like a minifier: `1000`
    /// becomes `1e3`. Whole numbers should read as written, so print their
    /// decimal digits verbatim (as an identifier, which oxc copies as-is); a
    /// negative one gets a real unary minus, so oxc still handles spacing and
    /// parentheses. Anything else (`0.1`) is already shortest.
    fn number(&self, sp: Span, n: f64) -> Expression<'a> {
        const EXACT: f64 = 9_007_199_254_740_992.0; // 2^53: every integer below is exact
        if n.fract() != 0.0 || n.abs() >= EXACT || (n == 0.0 && n.is_sign_negative()) {
            return Expression::new_numeric_literal(sp, n, None, NumberBase::Decimal, &self.b);
        }
        let digits = Expression::new_identifier(sp, self.name(&format!("{}", n.abs() as u64)), &self.b);
        if n < 0.0 {
            Expression::new_unary_expression(sp, UnaryOperator::UnaryNegation, digits, &self.b)
        } else {
            digits
        }
    }

    /// Copy a name into oxc's arena.
    fn name(&self, name: &str) -> &'a str {
        self.allocator.alloc_str(name)
    }

    fn label(&self, name: &str) -> LabelIdentifier<'a> {
        LabelIdentifier::new(SPAN, self.name(name), &self.b)
    }
}

/// Can `s` be JSX text as it is? Braces and angle brackets start JSX, `&` an
/// entity, and JSX drops whitespace at the start or end of a line.
fn jsx_text_safe(s: &str) -> bool {
    !s.contains(['{', '}', '<', '>', '&', '\n', '\r'])
}

fn span(s: js::Span) -> Span {
    Span::new(s.lo, s.hi)
}

/// JS splits binary operators in two: `&&`/`||` are "logical", the rest "binary".
fn binary_op(op: Op) -> Result<BinaryOperator, LogicalOperator> {
    Ok(match op {
        Op::And => return Err(LogicalOperator::And),
        Op::Or => return Err(LogicalOperator::Or),
        Op::Coalesce => return Err(LogicalOperator::Coalesce),
        Op::BitOr => BinaryOperator::BitwiseOR,
        Op::BitXor => BinaryOperator::BitwiseXOR,
        Op::BitAnd => BinaryOperator::BitwiseAnd,
        Op::Eq => BinaryOperator::StrictEquality,
        Op::Ne => BinaryOperator::StrictInequality,
        Op::LooseEq => BinaryOperator::Equality,
        Op::LooseNe => BinaryOperator::Inequality,
        Op::InstanceOf => BinaryOperator::Instanceof,
        Op::Lt => BinaryOperator::LessThan,
        Op::Le => BinaryOperator::LessEqualThan,
        Op::Gt => BinaryOperator::GreaterThan,
        Op::Ge => BinaryOperator::GreaterEqualThan,
        Op::Shl => BinaryOperator::ShiftLeft,
        Op::Shr => BinaryOperator::ShiftRight,
        Op::UShr => BinaryOperator::ShiftRightZeroFill,
        Op::Add => BinaryOperator::Addition,
        Op::Sub => BinaryOperator::Subtraction,
        Op::Mul => BinaryOperator::Multiplication,
        Op::Div => BinaryOperator::Division,
        Op::Rem => BinaryOperator::Remainder,
    })
}

/// Are `a` and `b` the same variable or property path: `s`, `a.b`?
fn same_place(a: &js::Expr, b: &js::Expr) -> bool {
    match (&a.kind, &b.kind) {
        (ExprKind::Var(a), ExprKind::Var(b)) => a == b,
        (ExprKind::Member(a, x), ExprKind::Member(b, y)) => x == y && same_place(a, b),
        _ => false,
    }
}
