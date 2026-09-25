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

use std::path::PathBuf;

use oxc_allocator::{Allocator, ArenaBox, ArenaVec};
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, AssignmentTarget, BindingIdentifier, BindingPattern,
    Declaration, Expression, FormalParameter, FormalParameterKind, FormalParameters, FunctionBody,
    FunctionType, IdentifierName, LabelIdentifier, ObjectPropertyKind, Program, PropertyKey,
    PropertyKind, Statement, VariableDeclarationKind, VariableDeclarator,
};
use oxc_ast::builder::AstBuilder;
use oxc_codegen::{Codegen, CodegenOptions, IndentChar};
use oxc_sourcemap::{SourceMap, SourceMapBuilder};
use oxc_span::{SPAN, SourceType, Span};
use oxc_syntax::number::NumberBase;
use oxc_syntax::operator::{AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator};

use crate::js::{self, ExprKind, Module, Op, Prop, StmtKind, UnaryOp};

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
    let cx = Cx { b: AstBuilder::new(&allocator), allocator: &allocator };
    let b = &cx.b;

    let body = ArenaVec::from_iter_in(module.functions.iter().map(|f| cx.function(f)), b);
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
    code.push('\n');

    // oxc prints functions back to back; put a blank line between them.
    // Record how far down each generated line ends up, to fix the map.
    let mut shift = code.matches('\n').count() as u32;
    let mut line_shift = Vec::new();
    for (i, line) in generated.code.lines().enumerate() {
        // Only functions start at column 0, so this can't match nested code.
        if i > 0 && (line.starts_with("function ") || line.starts_with("export function ")) {
            code.push('\n');
            shift += 1;
        }
        line_shift.push(shift);
        code.push_str(line);
        code.push('\n');
    }
    code.push_str(&format!("//# sourceMappingURL={js_file_name}.map\n"));

    let map = generated.map.expect("a source map, since source_map_path is set");
    Output { code, map: shift_lines(&map, &line_shift, js_file_name) }
}

/// Rebuild `map` with each generated line `l` moved down by `line_shift[l]`.
fn shift_lines(map: &SourceMap<'_>, line_shift: &[u32], js_file_name: &str) -> String {
    let mut out = SourceMapBuilder::default();
    out.set_file(js_file_name);
    for (source, content) in map.get_sources().zip(map.get_source_contents()) {
        out.set_source_and_content(source, content.unwrap_or_default());
    }
    // `add_name` deduplicates, so ids can change: translate them.
    let name_ids: Vec<u32> = map.get_names().map(|name| out.add_name(name)).collect();
    let last = line_shift.last().copied().unwrap_or_default();
    for t in map.get_tokens() {
        let line = t.get_dst_line();
        out.add_token(
            line + line_shift.get(line as usize).copied().unwrap_or(last),
            t.get_dst_col(),
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
}

impl<'a> Cx<'a> {
    fn function(&self, f: &js::Function) -> Statement<'a> {
        let b = &self.b;
        let params = f.params.iter().map(|name| {
            let pattern = BindingPattern::new_binding_identifier(SPAN, self.name(name), b);
            FormalParameter::new(SPAN, ArenaVec::new_in(b), pattern, None, None, false, None, false, false, b)
        });
        let params = FormalParameters::new(
            SPAN,
            FormalParameterKind::FormalParameter,
            ArenaVec::from_iter_in(params, b),
            None,
            b,
        );
        let body = FunctionBody::new(SPAN, ArenaVec::new_in(b), self.stmts(&f.body), b);
        let decl = Declaration::new_function_declaration(
            span(f.span),
            FunctionType::FunctionDeclaration,
            Some(BindingIdentifier::new(span(f.name_span), self.name(&f.name), b)),
            false, // generator
            false, // async
            false, // declare
            None,  // type parameters
            None,  // this param
            ArenaBox::new_in(params, b),
            None, // return type
            Some(ArenaBox::new_in(body, b)),
            b,
        );
        if f.export { Statement::new_export_declaration(span(f.span), decl, b) } else { decl.into() }
    }

    fn stmts(&self, stmts: &[js::Stmt]) -> ArenaVec<'a, Statement<'a>> {
        ArenaVec::from_iter_in(stmts.iter().map(|s| self.stmt(s)), &self.b)
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
            StmtKind::Assign(target, value) => {
                let assign = Expression::new_assignment_expression(
                    sp,
                    AssignmentOperator::Assign,
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
                    [only @ js::Stmt { kind: StmtKind::If(..), .. }] => self.stmt(only),
                    _ => self.block(els),
                });
                Statement::new_if_statement(sp, self.expr(cond), self.block(then), els, b)
            }
            StmtKind::While { label, cond, body } => {
                let w = Statement::new_while_statement(sp, self.expr(cond), self.block(body), b);
                match label {
                    Some(l) => Statement::new_labeled_statement(sp, self.label(l), w, b),
                    None => w,
                }
            }
            StmtKind::Break(label) => {
                Statement::new_break_statement(sp, label.as_deref().map(|l| self.label(l)), b)
            }
            StmtKind::Continue(label) => {
                Statement::new_continue_statement(sp, label.as_deref().map(|l| self.label(l)), b)
            }
            StmtKind::Return(value) => {
                Statement::new_return_statement(sp, value.as_ref().map(|v| self.expr(v)), b)
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

    fn declare(
        &self,
        sp: Span,
        kind: VariableDeclarationKind,
        name: &str,
        init: Option<&js::Expr>,
    ) -> Statement<'a> {
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
            ExprKind::Array(items) => {
                let items = items.iter().map(|item| ArrayExpressionElement::from(self.expr(item)));
                Expression::new_array_expression(sp, ArenaVec::from_iter_in(items, b), b)
            }
            ExprKind::Object(props) => {
                let props = props.iter().map(|prop| self.property(prop));
                Expression::new_object_expression(sp, ArenaVec::from_iter_in(props, b), b)
            }
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
            ExprKind::Call(callee, args) => {
                let args = args.iter().map(|a| Argument::from(self.expr(a)));
                Expression::new_call_expression(sp, self.expr(callee), None, ArenaVec::from_iter_in(args, b), false, b)
            }
        }
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
                let key = if computed {
                    PropertyKey::new_string_literal(SPAN, self.name(name), None, b)
                } else {
                    PropertyKey::new_static_identifier(SPAN, self.name(name), b)
                };
                ObjectPropertyKind::new_object_property(SPAN, PropertyKind::Init, key, value, false, shorthand, computed, b)
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

fn span(s: js::Span) -> Span {
    Span::new(s.lo, s.hi)
}

/// JS splits binary operators in two: `&&`/`||` are "logical", the rest "binary".
fn binary_op(op: Op) -> Result<BinaryOperator, LogicalOperator> {
    Ok(match op {
        Op::And => return Err(LogicalOperator::And),
        Op::Or => return Err(LogicalOperator::Or),
        Op::BitOr => BinaryOperator::BitwiseOR,
        Op::BitXor => BinaryOperator::BitwiseXOR,
        Op::BitAnd => BinaryOperator::BitwiseAnd,
        Op::Eq => BinaryOperator::StrictEquality,
        Op::Ne => BinaryOperator::StrictInequality,
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
