//! Lowering: rustc's THIR  ──►  our JS AST.
//!
//! The one idea to hold on to: Rust is *expression*-oriented (`if`, `match`,
//! `loop` and blocks all produce values), while JS separates statements from
//! expressions. So every THIR expression is lowered in one of two modes:
//!
//! - `expr()` wants a JS *expression*. Simple things (`a + f(b)`, `c ? x : y`)
//!   map directly; anything else is computed into a temporary first.
//! - `stmt()` emits JS *statements* and hands the value to a `Dest`ination:
//!   `return` it, assign it to a variable, or throw it away.
//!
//! Semantics follow Rust with `overflow-checks = off` (the release profile):
//! integer arithmetic wraps. Division by zero and `MIN / -1` still panic,
//! because Rust panics on those in every profile.
//!
//! This module owns dispatch, destinations and evaluation sequencing. `bodies`
//! owns function/nested-body lifecycle, `patterns` bindings and matches, `loops`
//! iteration, `places` reads and prepared writes, and `numbers` arithmetic.
//! `body_queries` has no emission context; its cached facts belong to `Body`.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rustc_ast::LitKind;
use rustc_hir::def::{CtorKind, DefKind};
use rustc_middle::middle::region;
use rustc_middle::mir::BorrowKind;
use rustc_middle::thir::{
    self as thir, AdtExprBase, BlockId, ExprId, ExprKind, LocalVarId, LogicalOp, Pat, PatKind, Thir,
};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{self, Ty, TyCtxt, TypeVisitableExt};
use rustc_span::def_id::{DefId, LocalDefId, LocalModDefId};
use rustc_span::{ErrorGuaranteed, SourceFile, Span};

use crate::js::{self, Expr, Op, Prop, Stmt, StmtKind, UnaryOp};

mod aggregates;
mod analysis;
mod bindings;
mod bodies;
mod body_queries;
mod calls;
mod cells;
mod combinators;
mod copies;
mod display;
mod drops;
mod effects;
mod format_args;
mod format_spec;
mod items;
mod iterators;
mod jsx;
mod jsx_api;
mod library;
mod loops;
mod maps;
mod mut_refs;
mod numbers;
mod options;
mod ordering;
mod patterns;
mod pipeline;
mod places;
mod ranges;
mod recognition;
mod representation;
mod results;
mod serde;
mod shortcuts;
mod sources;
mod std_impls;
mod support;
mod text;
mod traits;
mod vecs;

use crate::names::{fresh_in, js_ident};
use crate::program::TestFn;
use crate::runtime::Helper;
pub use analysis::collect_bodies;
use bindings::{Export, JsForm, is_binding, js_form, js_name};
pub use pipeline::lower_crate;
use recognition::Std;
use representation::{
    Num, char_value, const_js, eval_const, f32_literal, is_fieldless_enum, num_literal, ordering_value, static_value,
    variant_field,
};
pub use serde::{SerdeAttributes, attributes as serde_attributes};

use body_queries::strip;

type R<T> = Result<T, ErrorGuaranteed>;

/// A function's THIR, copied out of rustc before borrowck steals it.
pub struct Body<'tcx> {
    def_id: LocalDefId,
    thir: Thir<'tcx>,
    expr: ExprId,
    facts: body_queries::BodyFacts,
}

/// Where a function or a `const` ends up in the JS: its module's file,
/// under this name.
struct FnInfo {
    module: LocalModDefId,
    name: String,
    /// A method's type's object of methods (ADR 0047): `Counter` for `Counter.tick`.
    owner: Option<String>,
}

/// A module's path below the crate root, e.g. `["math", "stats"]`.
fn module_path(tcx: TyCtxt<'_>, module: LocalModDefId) -> Vec<String> {
    if module == LocalModDefId::CRATE_DEF_ID {
        return Vec::new();
    }
    let mut path = module_path(tcx, tcx.parent_module_from_def_id(module.to_local_def_id()));
    path.push(tcx.item_name(module.to_def_id()).to_string());
    path
}

/// The `.rs` file a module's code lives in: its own file for `mod foo;`,
/// the parent's file for an inline `mod foo { .. }`.
fn module_file(tcx: TyCtxt<'_>, module: LocalModDefId) -> Arc<SourceFile> {
    let inner = tcx.hir_get_module(module).0.spans.inner_span;
    tcx.sess.source_map().lookup_source_file(inner.lo())
}

pub struct LoweredFn {
    pub function: js::Function,
    pub runtime: HashSet<Helper>,
    pub jsx: bool,
    dependencies: Dependencies,
}

/// An expression's prerequisite statements stay in its evaluation region.
struct Evaluation {
    statements: Vec<Stmt>,
    value: Expr,
}

/// Where the value of a statement-lowered expression goes.
#[derive(Clone)]
enum Dest {
    Return,
    Assign(String),
    Discard,
}

struct Var {
    /// Usually the variable's JS name. An immutable binding into a pattern
    /// can instead just name the place it matched: `let (q, r) = t` makes
    /// `q` mean `t[0]`, with no JS variable at all.
    place: Expr,
    mutable: bool,
    /// How many loops enclose its declaration (ADR 0022).
    depth: usize,
}

/// What a body knows of its variables: what each is in JS, and what its
/// lowering found of some. A closure's body and a coroutine's share their
/// enclosing body's, since they name its variables; a trait's default body,
/// copied into an impl, has its own (`enter_body`). Something known of a
/// variable, by its `LocalVarId`, goes here, and so goes with it.
#[derive(Default)]
struct Locals {
    vars: HashMap<LocalVarId, Var>,
    /// A `&mut` to a map's value that's a primitive, `if let Some(n) =
    /// m.get_mut(&k)`: a copy of it, and the map and key a write puts it back
    /// in (ADR 0059). While it lives, nothing else can change that entry.
    slots: HashMap<LocalVarId, (Expr, Expr)>,
    /// Locals `next()` is called on that are bound as a `$iter`, which
    /// `next()` can step (ADR 0071).
    iterators: HashSet<LocalVarId>,
    /// Parameters that are a `&mut` to a value JS can't change in place, a
    /// `String` or a number: a `{ value }` box the caller copies back (ADR 0072).
    boxes: HashSet<LocalVarId>,
    /// Variables bound once to a `&mut` of a value that isn't an object:
    /// each names the place it borrowed, so `*y = 5` writes it (ADR 0099).
    aliases: HashSet<LocalVarId>,
    /// The `let`s a temporary a `&mut` is to has as its home, `&mut Some(3)`
    /// matched: places its `ref mut` bindings write (ADR 0099).
    temporaries: HashSet<String>,
    /// The std calls a pattern matches whose `&mut`s to values JS can't
    /// change in place are the items, `m.get_mut(&k)`'s: bound, each is
    /// the item, not a cell (ADR 0099).
    item_calls: HashSet<ExprId>,
    /// Their bindings: `*v` reads the item, but `v` has no place to give
    /// as a `&mut`, which would write the binding's copy (ADR 0099).
    items: HashSet<LocalVarId>,
}

/// A variable bound by a pattern, and the place in the subject it matched.
#[derive(Clone)]
struct Binding<'tcx> {
    var: LocalVarId,
    name: String,
    mutable: bool,
    /// `ref mut`, or bound through a `&mut` subject: writes through it
    /// write the place it matched.
    by_ref_mut: bool,
    /// `x @ ..`: it binds the whole value its subpattern binds parts of.
    whole: bool,
    place: Expr,
    /// Its place is a choice of an `|` pattern's alternatives, `p[0] === 0 ?
    /// p[1] : p[0]`, the first that matched (ADR 0124).
    chosen: bool,
    ty: Ty<'tcx>,
}

/// How a Rust struct or tuple type is represented in JS (ADR 0020).
enum Shape<'tcx> {
    /// A struct with named fields: `{ x: 1, y: 2 }`.
    Object(Vec<(String, Ty<'tcx>)>),
    /// A tuple or tuple struct: `[1, 2]`.
    Array(Vec<Ty<'tcx>>),
    /// Anything else: numbers, `bool`, unit and unit structs (`undefined`), enums.
    Other,
}

struct Loop {
    scope: region::Scope,
    /// Rust's label (`'outer`), or `loop`.
    label_base: String,
    /// Assigned on first use by a `break`/`continue` from an inner loop.
    label: Option<String>,
    /// Where `break value` delivers its value.
    dest: Dest,
}

/// A body lowered inside the one being lowered, and what it starts from
/// (`enter_body`). Each has its own THIR, owner and stepped locals; what
/// else isn't said here, it shares with the enclosing body.
enum Nested<'tcx> {
    /// A closure's, an arrow: the enclosing `Locals` and captures, loops
    /// of its own, and these names to start from.
    Closure { names: HashSet<String> },
    /// An `async fn`'s coroutine, which in JS is its function's own body:
    /// everything else is the function's.
    Coroutine,
    /// A trait's default body, copied into an impl (ADR 0049): `Locals` of
    /// its own, a copy of the names, and the impl's evidence, arguments,
    /// typing environment and drops.
    Default {
        evidence: Vec<(ty::TraitRef<'tcx>, Expr)>,
        self_args: ty::GenericArgsRef<'tcx>,
        typing_env: ty::TypingEnv<'tcx>,
        /// The drop, by name, for each of its trait's type parameters the
        /// impl's argument for has one (ADR 0098), and those rust-js can't
        /// make one for, and why.
        drops: HashMap<u32, String>,
        unsupported: HashMap<u32, (Ty<'tcx>, &'static str)>,
    },
}

/// What a nested body took of the enclosing one's state, given back when
/// it's left (`leave_body`): what every body has of its own, and what its
/// kind has too, as `Nested` says.
struct Enclosing<'a, 'tcx> {
    thir: &'a Thir<'tcx>,
    body_facts: &'a body_queries::BodyFacts,
    body_owner: DefId,
    stepped: HashSet<LocalVarId>,
    scopes: drops::BodyScopes<'tcx>,
    kind: EnclosingKind<'tcx>,
}

/// What each kind of nested body took, by `Nested`'s kinds.
enum EnclosingKind<'tcx> {
    Closure { loops: Vec<Loop>, names: HashSet<String> },
    Coroutine,
    Default(Box<ItemScope<'tcx>>),
}

/// What a trait's default body, copied into an impl, has of its own that
/// another body lowers with: the item's names, `Locals`, evidence, arguments,
/// typing environment and drops.
struct ItemScope<'tcx> {
    names: HashSet<String>,
    locals: Locals,
    evidence: Vec<(ty::TraitRef<'tcx>, Expr)>,
    self_args: Option<ty::GenericArgsRef<'tcx>>,
    typing_env: ty::TypingEnv<'tcx>,
    drops: drops::SwappedDrops<'tcx>,
}

/// Immutable analysis inputs shared by function lowering.
struct CrateFacts<'a, 'tcx> {
    sources: &'a sources::CapturedSources,
    mutated: &'a HashSet<Ty<'tcx>>,
    /// The trait constants generic code reads (ADR 0106).
    generic_consts: &'a HashSet<DefId>,
    changed_vecs: &'a HashSet<Ty<'tcx>>,
    /// Each generic function's type parameters it's given a drop for (ADR 0098).
    drop_params: &'a HashMap<DefId, Vec<u32>>,
    closures: &'a HashMap<LocalDefId, &'a Body<'tcx>>,
    bodies: &'a HashMap<DefId, &'a Body<'tcx>>,
    fns: &'a HashMap<DefId, FnInfo>,
    imports: &'a HashMap<Export, String>,
    /// What the crate's libraries export (ADR 0100).
    foreign: &'a library::Foreign<'a, 'tcx>,
    /// Is this crate compiled as a library, for others to use (ADR 0100)?
    library: bool,
    trait_impls: &'a [DefId],
    /// `#[serde(..)]` attributes, from the expanded crate (ADR 0077).
    serde_attrs: &'a serde::SerdeAttributes,
    /// Do its `Debug` functions take whether to be pretty (ADR 0137)?
    pretty_debug: bool,
}

/// Dependencies recorded by one function (including copied trait bodies and
/// closures), returned with its JS. They never mutate the crate's inputs.
#[derive(Default)]
struct Dependencies {
    references: HashSet<(LocalModDefId, DefId)>,
    package_uses: HashSet<(LocalModDefId, Export)>,
    uses: Vec<(DefId, DefId)>,
}

struct FnCx<'a, 'tcx> {
    krate: &'a CrateFacts<'a, 'tcx>,
    dependencies: RefCell<Dependencies>,
    tcx: TyCtxt<'tcx>,
    typing_env: ty::TypingEnv<'tcx>,
    /// What the generic item being lowered is given, besides its arguments.
    given: Given<'tcx>,
    /// While lowering a closure: the places it captured into snapshots.
    captures: HashMap<(LocalVarId, Vec<usize>), Var>,
    thir: &'a Thir<'tcx>,
    body_facts: &'a body_queries::BodyFacts,
    /// The module receiving this function and its recorded dependencies.
    module: LocalModDefId,
    /// What this body knows of its variables.
    locals: Locals,
    /// JS names already taken in this function.
    names: HashSet<String>,
    /// Those taken by the module: its functions, imports and globals.
    module_names: &'a HashSet<String>,
    labels: HashSet<String>,
    loops: Vec<Loop>,
    runtime: HashSet<Helper>,
    /// Whether this function makes JSX.
    jsx: bool,
    /// What writing to a `Formatter` knows (ADRs 0054, 0137).
    writing: display::Writing,
    /// What this function's iterator chains are, beyond their types.
    chains: iterators::Chains,
    /// Locals that `next()` is called on (ADR 0071): an iterator over an
    /// array that's stepped through, a `$iter` object that knows where it is.
    stepped: HashSet<LocalVarId>,
    /// The recursive types being cloned, and the function each one's clone
    /// is (`clone_value`), which a clone inside it calls.
    cloning: Vec<(Ty<'tcx>, String)>,
    /// The item being lowered: what `fn_ref` records as using its target.
    item: DefId,
    /// What walks of types found, each walked once.
    walks: TypeWalks<'tcx>,
    /// What's dropped, and where (ADR 0098).
    drop_state: drops::DropState<'tcx>,
    /// Whose body `thir` is: its scope tree says where temporaries end.
    body_owner: DefId,
}

/// What the generic item being lowered is given, besides its arguments.
#[derive(Default)]
struct Given<'tcx> {
    /// Its dictionaries, for its bounds (ADR 0049).
    evidence: Vec<(ty::TraitRef<'tcx>, Expr)>,
    /// Each const parameter it's given, by its index: `N` (ADR 0107).
    const_params: Vec<(u32, Expr)>,
    /// In a trait's default body copied into an impl (ADR 0049): the impl's
    /// arguments for the trait's parameters, `Self` among them.
    self_args: Option<ty::GenericArgsRef<'tcx>>,
    /// In a generic type's derived `serialize` or `deserialize` (ADR
    /// 0080): each type parameter, and the parameter that writes or reads it.
    codec_params: Vec<(Ty<'tcx>, String)>,
}

/// What walks of types found, kept so a type met again, along another path
/// through a type, isn't walked again: `Foo2(Foo1, Foo1)` of `Foo1(Foo0,
/// Foo0)` is walked once, not 2^n times.
struct TypeWalks<'tcx> {
    /// What `unsupported_in` found of each struct and enum it looked into.
    representable: RefCell<HashMap<Ty<'tcx>, Option<Ty<'tcx>>>>,
    /// While `unsupported_in` walks a type: how far out, among the types it's
    /// inside, is the one a walk took as fine, being inside itself.
    assumed: Cell<usize>,
    /// What `contains_mutated` and `needs_clone_in` found of each type, and
    /// how far out `needs_clone_in` assumed.
    mutated: RefCell<HashMap<Ty<'tcx>, bool>>,
    clones: RefCell<HashMap<Ty<'tcx>, bool>>,
    clone_assumed: Cell<usize>,
}

impl Default for TypeWalks<'_> {
    fn default() -> Self {
        TypeWalks {
            representable: RefCell::new(HashMap::new()),
            assumed: Cell::new(usize::MAX),
            mutated: RefCell::new(HashMap::new()),
            clones: RefCell::new(HashMap::new()),
            clone_assumed: Cell::new(usize::MAX),
        }
    }
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    // ── Statement mode ──────────────────────────────────────────────────

    /// Emit statements that compute `e` and deliver its value to `dest`.
    fn stmt(&mut self, e: ExprId, dest: &Dest, out: &mut Vec<Stmt>) -> R<()> {
        let expr = &self.thir[e];
        // A unit value carries no information, and a never value never
        // arrives. Either way, there is nothing to deliver.
        let dest = if expr.ty.is_unit() || expr.ty.is_never() {
            &Dest::Discard
        } else {
            dest
        };
        let span = self.js_span(expr.span);

        match expr.kind {
            ExprKind::Scope {
                value,
                hir_id,
                region_scope,
            } => {
                if let Some(body) = self.body_query().scoped_loop(value) {
                    self.lower_loop(region_scope, hir_id, body, dest, span, out)
                } else {
                    // What ends with it is dropped once it's delivered (ADR 0098).
                    // One that fails fails its whole item, open scopes and all.
                    let mark = out.len();
                    self.begin_scope(region_scope);
                    self.stmt(value, dest, out)?;
                    self.end_scope(mark, expr.span, out)
                }
            }
            ExprKind::Use { source }
            | ExprKind::NeverToAny { source }
            | ExprKind::ValueTypeAscription { source, .. }
            | ExprKind::PlaceTypeAscription { source, .. } => self.stmt(source, dest, out),
            ExprKind::Block { block } => self.block(block, dest, out),
            ExprKind::If {
                cond, then, else_opt, ..
            } if let Some(parts) = self.let_chain(cond) => self.lower_let_chain(parts, then, else_opt, dest, span, out),
            ExprKind::If {
                cond, then, else_opt, ..
            } => {
                let mut then_out = Vec::new();
                // What an `if let` binds is its `then`'s, dropped as it ends.
                let mark = self.owned_mark();
                let cond = match self.thir[self.strip(cond)].kind {
                    ExprKind::Let {
                        expr: scrutinee,
                        ref pat,
                    } => self.if_let(scrutinee, pat, &mut then_out, out)?,
                    _ => self.expr(cond, out)?,
                };
                // Its bindings first, then the body their `finally` drops them after.
                let mut body = Vec::new();
                self.stmt(then, dest, &mut body)?;
                self.close_scope(mark, body, self.thir[then].span, &mut then_out)?;
                let else_out = match else_opt {
                    Some(els) => {
                        let mut else_out = Vec::new();
                        self.stmt(els, dest, &mut else_out)?;
                        Some(else_out)
                    }
                    None => None,
                };
                out.push(StmtKind::If(cond, then_out, else_out).at(span));
                Ok(())
            }
            ExprKind::Match { .. } if let Some(for_loop) = self.body_query().as_for(e) => {
                self.lower_for(for_loop, span, out)
            }
            ExprKind::Match {
                scrutinee, ref arms, ..
            } if self.body_query().as_await(e).is_none()
                && self.body_query().as_question(e).is_none()
                && !self.is_matches(arms) =>
            {
                self.lower_match(scrutinee, arms, dest, out)
            }
            // A function that writes to a `Formatter` returns what it wrote (ADR 0054).
            ExprKind::Return { value } if let Some((_, name)) = self.writing.writer.clone() => {
                if let Some(v) = value {
                    self.stmt(v, &Dest::Discard, out)?;
                }
                out.push(StmtKind::Return(Some(Expr::var(&name))).at(span));
                Ok(())
            }
            ExprKind::Return { value } => {
                match value {
                    Some(v) if !self.thir[v].ty.is_unit() => self.stmt(v, &Dest::Return, out)?,
                    Some(v) => {
                        self.stmt(v, &Dest::Discard, out)?;
                        out.push(StmtKind::Return(None).at(span));
                    }
                    None => out.push(StmtKind::Return(None).at(span)),
                }
                Ok(())
            }
            ExprKind::Break { label, value } => {
                let i = self.loop_index(label, expr.span)?;
                if let Some(v) = value {
                    let loop_dest = self.loops[i].dest.clone();
                    self.stmt(v, &loop_dest, out)?;
                    if matches!(loop_dest, Dest::Return) {
                        return Ok(()); // `return` already left the loop.
                    }
                }
                let label = self.jump_label(i);
                out.push(StmtKind::Break(label).at(span));
                Ok(())
            }
            ExprKind::Continue { label } => {
                let i = self.loop_index(label, expr.span)?;
                let label = self.jump_label(i);
                out.push(StmtKind::Continue(label).at(span));
                Ok(())
            }
            // Rust evaluates the right side of an assignment first. The target
            // is a variable or its fields, which reading can't change.
            // A value in a map: `m.set(k, v)` (ADR 0059).
            ExprKind::Assign { lhs, rhs } if self.has_drops(self.thir[lhs].ty) => {
                self.assign_dropping(lhs, rhs, span, out)
            }
            ExprKind::Assign { lhs, rhs } => self.assign(lhs, rhs, expr.span, out),
            ExprKind::AssignOp { op, lhs, rhs } => self.assign_op(op, lhs, rhs, expr.span, out),
            _ => {
                let value = match (dest, self.place(e)) {
                    // Only this call's value goes nowhere, not its arguments'
                    // values, so a map's `insert` is `m.set(k, v)` (ADR 0059).
                    (Dest::Discard, _) if let ExprKind::Call { fun, ref args, .. } = expr.kind => {
                        let value = self.call(fun, args, true, expr.span, out)?;
                        self.generic_result(fun, value, expr.span)?.or_at(span)
                    }
                    // Returning a place of this function's own hands its value
                    // over without a copy: every local dies here, so nothing is
                    // left to share it. One reached through a reference, or a
                    // closure's capture, outlives the call, so it's copied.
                    (Dest::Return, Some((place, _))) if self.is_local_place(e) => {
                        self.moved(e, out)?;
                        place.or_at(span)
                    }
                    _ => self.expr(e, out)?,
                };
                match dest {
                    Dest::Return => out.push(StmtKind::Return(Some(value)).at(span)),
                    Dest::Assign(name) => out.push(StmtKind::Assign(Expr::var(name), value).at(span)),
                    Dest::Discard if value.has_effects() => out.push(StmtKind::Expr(value).at(span)),
                    Dest::Discard => {}
                }
                Ok(())
            }
        }
    }

    fn block(&mut self, block: BlockId, dest: &Dest, out: &mut Vec<Stmt>) -> R<()> {
        self.block_rest(block, 0, Some(dest), out)
    }

    /// A block's statements, without its tail expression.
    fn block_stmts(&mut self, block: BlockId, out: &mut Vec<Stmt>) -> R<()> {
        self.block_rest(block, 0, None, out)
    }

    /// A block's statements from `from` on, then its tail, to `tail` if
    /// that's given. Once a `let` binds what has a destructor, the rest is
    /// a `try` whose `finally` drops it (ADR 0098).
    fn block_rest(&mut self, block_id: BlockId, from: usize, tail: Option<&Dest>, out: &mut Vec<Stmt>) -> R<()> {
        let block = &self.thir[block_id];
        if block.targeted_by_break {
            return Err(self.unsupported(block.span, "labeled blocks"));
        }
        // Every local gets a unique JS name, so a Rust block needs no JS
        // block of its own: its statements go straight into `out`.
        for (i, &stmt) in block.stmts.iter().enumerate().skip(from) {
            let mark = self.owned_mark();
            self.statement(stmt, out)?;
            if self.owned_mark() > mark {
                // The `let`s after it that can't leave early share its `try`.
                let mut next = i + 1;
                while let Some(&stmt) = block.stmts.get(next)
                    && let thir::StmtKind::Let {
                        initializer: Some(init),
                        else_block: None,
                        ..
                    } = &self.thir[stmt].kind
                    && self.cannot_leave(*init)
                {
                    self.statement(stmt, out)?;
                    next += 1;
                }
                let mut rest = Vec::new();
                self.block_rest(block_id, next, tail, &mut rest)?;
                return self.close_scope(mark, rest, block.span, out);
            }
        }
        if let Some(dest) = tail
            && let Some(value) = block.expr
        {
            self.stmt(value, dest, out)?;
        }
        Ok(())
    }

    /// One statement, whose temporaries end with it (ADR 0098).
    fn statement(&mut self, stmt: thir::StmtId, out: &mut Vec<Stmt>) -> R<()> {
        let (scopes, span) = match &self.thir[stmt].kind {
            thir::StmtKind::Expr { scope, expr } => ((*scope, None), self.thir[*expr].span),
            thir::StmtKind::Let {
                init_scope,
                remainder_scope,
                span,
                ..
            } => ((*init_scope, Some(*remainder_scope)), *span),
        };
        let outer = self.begin_statement(scopes);
        let mut lowered = Vec::new();
        self.statement_body(stmt, &mut lowered)?;
        self.end_statement(outer, lowered, span, out)
    }

    fn statement_body(&mut self, stmt: thir::StmtId, out: &mut Vec<Stmt>) -> R<()> {
        {
            match &self.thir[stmt].kind {
                // A statement's value that has a destructor is dropped at once.
                thir::StmtKind::Expr { expr, .. } if self.has_drops(self.thir[*expr].ty) => {
                    let ty = self.thir[*expr].ty;
                    let span = self.thir[*expr].span;
                    let value = self.expr(*expr, out)?;
                    let value = self.droppable(value, ty, out);
                    self.drop_value(value, ty, span, out)?;
                }
                thir::StmtKind::Expr { expr, .. } => self.stmt(*expr, &Dest::Discard, out)?,
                thir::StmtKind::Let {
                    pattern,
                    initializer,
                    else_block,
                    span,
                    ..
                } => {
                    // `let Some(x) = e else { return .. };`: the test, the
                    // `else` that leaves when it fails, then the bindings.
                    if let Some(else_block) = *else_block {
                        let init = initializer.ok_or_else(|| self.unsupported(*span, "`let ... else`"))?;
                        let mut bindings = Vec::new();
                        let test = self.if_let(init, pattern, &mut bindings, out)?;
                        let mut failed = Vec::new();
                        self.block(else_block, &Dest::Discard, &mut failed)?;
                        let js_span = self.js_span(*span);
                        out.push(StmtKind::If(std_impls::negate(test), failed, None).at(js_span));
                        out.extend(bindings);
                        return Ok(());
                    }
                    self.lower_let(pattern, *initializer, *span, out)?;
                }
            }
        }
        Ok(())
    }

    // ── Expression mode ─────────────────────────────────────────────────

    /// Lower `e` to a JS expression. Any statements it needs first (a
    /// block's `let`s, a `match` computing a temporary) are pushed to `out`.
    ///
    /// The result carries `e`'s span, unless a more precise one was set
    /// deeper down (a `Scope` passes its inner expression through, say).
    fn expr(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let span = self.js_span(self.thir[e].span);
        let value = self.expr_inner(e, out)?.or_at(span);
        // A value with a destructor in a temporary is in a `const` its
        // scope drops (ADR 0098).
        match self.temp_kind(e)? {
            Some(kind) => self.temporary(e, kind, value, out),
            None => Ok(value),
        }
    }

    fn expr_inner(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let expr = &self.thir[e];
        let span = expr.span;
        let js_span = self.js_span(span);
        let ty = expr.ty;
        match expr.kind {
            ExprKind::Scope {
                value, region_scope, ..
            } if self.body_query().scoped_loop(value).is_none() => {
                let mark = out.len();
                self.begin_scope(region_scope);
                let value = self.expr(value, out)?;
                // Its value is computed before what ends with it is dropped (ADR 0098).
                let value = match self.scope_has_temps() && !value.is_constant() {
                    true if ty.is_unit() => {
                        out.push(StmtKind::Expr(value).at(js_span));
                        Expr::undefined()
                    }
                    true => self.spill("value", value, out),
                    false => value,
                };
                self.end_scope(mark, span, out)?;
                Ok(value)
            }
            ExprKind::Use { source }
            | ExprKind::ValueTypeAscription { source, .. }
            | ExprKind::PlaceTypeAscription { source, .. } => self.expr(source, out),
            ExprKind::Block { .. } if let Some(f) = self.as_format_args(e) => self.lower_format_args(f, span, out),
            // One that owns what it drops computes its value before the drops.
            ExprKind::Block { block } if !self.thir[block].targeted_by_break && self.block_owns(block)? => {
                let name = self.fresh("value");
                out.push(StmtKind::Let(name.clone(), None).at(js_span));
                self.block(block, &Dest::Assign(name.clone()), out)?;
                Ok(Expr::var(&name))
            }
            ExprKind::Block { block } if !self.thir[block].targeted_by_break => {
                self.block_stmts(block, out)?;
                match self.thir[block].expr {
                    Some(tail) => self.expr(tail, out),
                    None => Ok(Expr::undefined()),
                }
            }
            ExprKind::Literal { lit, neg } => self.literal(&lit.node, neg, ty, span),
            ExprKind::NonHirLiteral { lit, .. } => {
                if ty.is_bool() {
                    return Ok(Expr::bool(lit.to_bits_unchecked() != 0));
                }
                let num = self.num(ty, span)?;
                Ok(num_literal(lit.to_bits_unchecked(), num))
            }
            ExprKind::VarRef { .. }
            | ExprKind::UpvarRef { .. }
            | ExprKind::Field { .. }
            | ExprKind::Deref { .. }
            | ExprKind::StaticRef { .. } => self.read(e, out),
            // A shared reference is the value it points to (ADR 0023): JS
            // shares objects anyway, and nothing can change through it.
            // `&y` of a `&mut` to a number that `y` names the place of: as `y`
            // is, a handle on it (ADR 0099). Also `&*&y`, how `contains(&y)`
            // is reborrowed.
            ExprKind::Borrow {
                borrow_kind: BorrowKind::Shared,
                arg,
            } if self.is_cell(self.thir[arg].ty)
                && self.thir[self.strip_refs(arg)].ty == self.thir[arg].ty
                && matches!(self.thir[self.strip_refs(arg)].kind, ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } if !self.locals.boxes.contains(&id)) =>
            {
                self.read(self.strip_refs(arg), out)
            }
            ExprKind::Borrow {
                borrow_kind: BorrowKind::Shared,
                arg,
            } => match self.place(arg) {
                Some((place, _)) => Ok(place),
                None => self.referent(arg, out),
            },
            // `&mut *out` of a box, handed on: the box (ADR 0072).
            ExprKind::Borrow {
                borrow_kind: BorrowKind::Mut { .. },
                arg,
            } if let ExprKind::Deref { arg: inner } = self.thir[self.strip(arg)].kind
                && let ExprKind::VarRef { id } = self.thir[self.strip(inner)].kind
                && self.locals.boxes.contains(&id) =>
            {
                Ok(self.locals.vars[&id].place.clone())
            }
            // `&mut` to a JS object is the object (ADR 0025), and to a closure
            // the closure (ADR 0099).
            ExprKind::Borrow {
                borrow_kind: BorrowKind::Mut { .. },
                arg,
            } if self.is_object(self.thir[arg].ty) || self.is_callable(self.thir[arg].ty) => match self.place(arg) {
                Some((place, _)) => Ok(place),
                None => self.referent(arg, out),
            },
            // `&mut *e` of a `&mut` that isn't a variable's: `e`'s own, a cell
            // kept in a field or given back by a block, a branch or a call of
            // the crate's (ADR 0099). Not a std call's, as `v[i]`'s `index_mut`
            // is: that's the item, whose `&mut` is a handle on it.
            ExprKind::Borrow {
                borrow_kind: BorrowKind::Mut { .. },
                arg,
            } if let ExprKind::Deref { arg: inner } = self.thir[self.strip(arg)].kind
                && self.is_cell(self.thir[inner].ty)
                && match self.thir[self.strip(inner)].kind {
                    ExprKind::VarRef { .. } | ExprKind::UpvarRef { .. } => false,
                    ExprKind::Call { .. } => self.is_cell_value(inner),
                    _ => true,
                } =>
            {
                self.expr(inner, out)
            }
            // `&mut x` kept, of a value JS can't change in place: a handle on
            // `x`, fixed where it's borrowed (ADR 0099).
            ExprKind::Borrow {
                borrow_kind: BorrowKind::Mut { .. },
                arg,
            } if self.makes_cell(self.thir[arg].ty) => {
                // `&mut *&mut v[0]`, a reborrow: of `v[0]`.
                let place = self.mut_borrowed(e).unwrap_or(arg);
                // `&mut 42`: a box of it, which nothing else sees.
                if self.is_temporary(place) {
                    let value = self.expr(place, out)?;
                    return Ok(Expr::object(vec![Prop::Field("value".into(), value)]));
                }
                Ok(Expr::handle(self.fixed_place(place, self.thir[place].span, out)?))
            }
            ExprKind::Borrow { arg, .. } => {
                Err(self.unsupported(span, &format!("`&mut` to a `{}`", self.thir[arg].ty)))
            }
            ExprKind::Array { ref fields } => Ok(Expr::array(self.operands(fields, out)?)),
            // `[x; N]`: `x` runs once, even for none, and the array is `N`
            // copies of it: `Array(N).fill(x)` where copies can't be told
            // apart, else each its own, `Array.from({ length: N }, () => ..)`.
            ExprKind::Repeat { value, count } => {
                let item_ty = self.thir[value].ty;
                let count = self
                    .tcx
                    .normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(count));
                // A caller's `N` (ADR 0107), or the number.
                let n = count.try_to_target_usize(self.tcx);
                let length = match n {
                    Some(n) => Expr::int(n as i128),
                    None => self.const_arg(count, span)?,
                };
                let item = self.expr(value, out)?;
                // A `Copy` value's copies are copies of its bits, which a
                // value that nothing changes needs none of; one that isn't
                // `Copy` is a constant's, made again for each.
                let copied = if self.is_copy(item_ty) {
                    self.contains_mutated(item_ty)
                } else if self.needs_clone(item_ty) {
                    return Err(self.unsupported(span, "`[x; N]` of a value that isn't `Copy`"));
                } else {
                    false
                };
                if !copied {
                    if let Some(n) = n
                        && n <= 4
                        && item.is_constant()
                    {
                        return Ok(Expr::array(vec![item; n as usize]));
                    }
                    let array = Expr::new_(Expr::var("Array"), vec![length]);
                    return Ok(Expr::call(Expr::member(array, "fill"), vec![item]));
                }
                let item = if item.reads_same() {
                    item
                } else {
                    self.spill("item", item, out)
                };
                let copy = self.copy(item, item_ty);
                let length = Expr::object(vec![Prop::Field("length".into(), length)]);
                let from = Expr::member(Expr::var("Array"), "from");
                Ok(Expr::call(
                    from,
                    vec![
                        length,
                        Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(copy)).at(js::Span::NONE)]),
                    ],
                ))
            }
            ExprKind::Index { lhs, index } => {
                let values = self.indexed(lhs, index, out)?;
                let item = self.checked_index(lhs, values);
                Ok(self.copy_if_needed(item, ty))
            }
            // A `dyn Iterator` is a JS iterator.
            ExprKind::PointerCoercion {
                cast: PointerCoercion::Unsize,
                source,
                ..
            } if self.recognition().is_dyn_iter(ty) => self.dyn_iterator(source, span, out),
            // `Box<closure>` to `Box<dyn FnMut()>`: the same JS function.
            ExprKind::PointerCoercion {
                cast: PointerCoercion::Unsize,
                source,
                ..
            } => {
                let value = self.expr(source, out)?;
                self.unsize_trait(self.thir[source].ty, ty, value, span, out)
            }
            ExprKind::PointerCoercion {
                cast: PointerCoercion::ReifyFnPointer(_),
                source,
                ..
            } => self.expr(source, out),
            // A closure that captures nothing, as a `fn`: a JS function already
            // (ADR 0125).
            ExprKind::PointerCoercion {
                cast: PointerCoercion::ClosureFnPointer(_),
                source,
                ..
            } => self.expr(source, out),
            ExprKind::ZstLiteral { .. }
                if let &ty::FnDef(id, _) = ty.kind()
                    && let Some(why) = self.krate.foreign.unlisted(id) =>
            {
                Err(self.tcx.dcx().span_err(span, why))
            }
            // A constructor as a value, `.map(Some)`: an arrow making what its
            // call makes (ADR 0125).
            ExprKind::ZstLiteral { .. }
                if let &ty::FnDef(def_id, args) = ty.kind()
                    && matches!(self.tcx.def_kind(def_id), DefKind::Ctor(_, CtorKind::Fn)) =>
            {
                self.constructor_value(def_id, args, span)
            }
            // `Self` of a `struct Marker;`, which holds nothing, like `()`.
            ExprKind::ZstLiteral { .. }
                if let ty::Adt(adt, _) = ty.kind()
                    && adt.is_struct()
                    && adt.non_enum_variant().ctor_kind() == Some(CtorKind::Const) =>
            {
                Ok(Expr::undefined())
            }
            // A function as a value, `component(Card, props)`: its JS name, or
            // a library's import of it (ADR 0100), given its dictionaries.
            ExprKind::ZstLiteral { .. }
                if let &ty::FnDef(def_id, args) = ty.kind()
                    && (self.is_rust_fn(def_id) || self.is_rust_trait_fn(def_id, args)) =>
            {
                if self.tcx.trait_of_assoc(def_id).is_some() {
                    let count = self
                        .tcx
                        .fn_sig(def_id)
                        .instantiate(self.tcx, args)
                        .skip_normalization()
                        .skip_binder()
                        .inputs()
                        .len();
                    let params: Vec<String> = (0..count).map(|i| self.fresh(&format!("arg{i}"))).collect();
                    let values = params.iter().map(|name| Expr::var(name)).collect();
                    let call = self
                        .trait_call(def_id, args, values, span, out)?
                        .ok_or_else(|| self.unsupported(span, "this trait function value"))?;
                    // `(arg0) => shapeArea_area(arg0)` is `shapeArea_area`: a
                    // function by name, not a dictionary's method, read off it.
                    if let js::ExprKind::Call(callee, list) = &call.kind
                        && matches!(callee.kind, js::ExprKind::Var(_))
                        && list.len() == params.len()
                        && list
                            .iter()
                            .zip(&params)
                            .all(|(value, name)| matches!(&value.kind, js::ExprKind::Var(v) if v == name))
                        && params.iter().all(|name| !callee.mentions_var(name))
                    {
                        return Ok((**callee).clone());
                    }
                    return Ok(Expr::arrow(
                        params.into_iter().map(Into::into).collect(),
                        vec![StmtKind::Return(Some(call)).at(js_span)],
                    ));
                }
                let callee = self.fn_ref(def_id);
                let evidence = self.evidence_args(def_id, args, span)?;
                if evidence.is_empty() {
                    Ok(callee)
                } else {
                    let count = self
                        .tcx
                        .fn_sig(def_id)
                        .instantiate(self.tcx, args)
                        .skip_normalization()
                        .skip_binder()
                        .inputs()
                        .len();
                    let params: Vec<String> = (0..count).map(|i| format!("arg{i}")).collect();
                    let values = params.iter().map(|name| Expr::var(name)).chain(evidence).collect();
                    Ok(Expr::arrow(
                        params.into_iter().map(Into::into).collect(),
                        vec![StmtKind::Return(Some(Expr::call(callee, values))).at(js_span)],
                    ))
                }
            }
            // `.map(str::trim)`: `(s) => s.trim()`, as a closure would be.
            ExprKind::ZstLiteral { .. }
                if let Some(known) = self.std_fn(e)
                    && let Some(f) = self.std_fn_value(known, ty, span)? =>
            {
                Ok(f)
            }
            ExprKind::ZstLiteral { .. } if let Some(Std::MaxOf(max)) = self.std_fn(e) => {
                self.runtime.insert(if max { Helper::F64Max } else { Helper::F64Min });
                Ok(Expr::var(if max { "$f64Max" } else { "$f64Min" }))
            }
            ExprKind::ZstLiteral { .. }
                if let &ty::FnDef(def_id, args) = ty.kind()
                    && is_binding(self.tcx, def_id) =>
            {
                self.binding_value(def_id, args, span)
            }
            ExprKind::Closure(ref closure) => self.closure(closure, out),
            ExprKind::Tuple { ref fields } if fields.is_empty() => Ok(Expr::undefined()),
            ExprKind::Tuple { ref fields } => Ok(Expr::array(self.operands(fields, out)?)),
            ExprKind::Adt(ref adt) => self.adt(adt, ty, span, out),
            ExprKind::Binary { op, lhs, rhs } => {
                let [l, r] = self.operands(&[lhs, rhs], out)?.try_into().ok().unwrap();
                let r = self.shift_amount(op, r, lhs, rhs);
                self.binary(op, l, r, self.known_int(rhs), self.thir[lhs].ty, span)
            }
            ExprKind::LogicalOp { op, lhs, rhs } => {
                let l = self.expr(lhs, out)?;
                let js_op = match op {
                    LogicalOp::And => Op::And,
                    LogicalOp::Or => Op::Or,
                };
                // `a && { .. }`: only run the right side's statements if needed,
                // as its JS has them, not as Rust looks: `f(&mut y)` of a
                // number has its write-back.
                let mut rhs_out = Vec::new();
                let simple = if self.is_simple(rhs) {
                    let r = self.expr(rhs, &mut rhs_out)?;
                    if rhs_out.is_empty() {
                        return Ok(Expr::bin(js_op, l, r));
                    }
                    Some(r)
                } else {
                    None
                };
                let tmp = self.fresh("tmp");
                out.push(StmtKind::Let(tmp.clone(), Some(l)).at(js_span));
                match simple {
                    Some(r) => rhs_out.push(StmtKind::Assign(Expr::var(&tmp), r).at(js_span)),
                    None => self.stmt(rhs, &Dest::Assign(tmp.clone()), &mut rhs_out)?,
                }
                let test = match op {
                    LogicalOp::And => Expr::var(&tmp),
                    LogicalOp::Or => Expr::unary(UnaryOp::Not, Expr::var(&tmp)),
                };
                out.push(StmtKind::If(test, rhs_out, None).at(js_span));
                Ok(Expr::var(&tmp))
            }
            ExprKind::Unary { op, arg } => {
                let a = self.expr(arg, out)?;
                self.unary(op, a, ty, span)
            }
            // `n as char`, of a `u8` (ADR 0063).
            ExprKind::Cast { source } if ty.is_char() => {
                let v = self.expr(source, out)?;
                Ok(Expr::call(Expr::member(Expr::var("String"), "fromCharCode"), vec![v]))
            }
            ExprKind::Cast { source } => {
                let v = self.expr(source, out)?;
                self.cast(v, self.thir[source].ty, ty, span)
            }
            ExprKind::Call {
                fun,
                ref args,
                from_hir_call,
                ..
            } => {
                // An operator's, `v[i]`'s `*index_mut(&mut v, i)`: its `&mut` is the
                // item, read or written where it is and never kept (ADR 0099).
                if !from_hir_call {
                    self.locals.item_calls.insert(fun);
                }
                let value = self.call(fun, args, false, span, out)?;
                self.generic_result(fun, value, span)
            }
            ExprKind::NamedConst { def_id, args, .. } => self.named_const(def_id, args, ty, span),
            // `const { square(7) + 1 }`: its value, as rustc computes it, as a
            // named constant's is (ADR 0127).
            ExprKind::ConstBlock { did, args } => eval_const(self.tcx, self.typing_env, did, args, span)
                .and_then(|value| const_js(self.tcx, value))
                .ok_or_else(|| self.unsupported(span, "this `const` block")),
            ExprKind::ConstParam { param, .. } => self.const_arg(ty::Const::new_param(self.tcx, param), span),
            ExprKind::Match { .. } if let Some(awaited) = self.body_query().as_await(e) => {
                Ok(Expr::await_(self.expr(awaited, out)?))
            }
            ExprKind::Match { .. } if let Some(tried) = self.body_query().as_question(e) => {
                self.question(e, tried, None, out)
            }
            ExprKind::Match {
                scrutinee, ref arms, ..
            } if let Some(test) = self.as_matches(scrutinee, arms, out)? => Ok(test),
            ExprKind::If {
                cond,
                then,
                else_opt: Some(els),
                ..
            } if self.is_simple(then)
                && self.is_simple(els)
                && self.let_chain(cond).is_none()
                && !matches!(self.thir[self.strip(cond)].kind, ExprKind::Let { .. }) =>
            {
                let c = self.expr(cond, out)?;
                let t = self.evaluated(then)?;
                let f = self.evaluated(els)?;
                Ok(self.conditional(c, t, f, js_span, out))
            }
            // `if let` as a value: `test ? then : else`, where the pattern
            // names its variables' places and needs no `const`s for them;
            // `{ true } else { false }` is the test itself, and a pattern that
            // always matches is its `then`.
            ExprKind::If {
                cond,
                then,
                else_opt: Some(els),
                ..
            } if self.is_simple(then)
                && self.is_simple(els)
                && let ExprKind::Let {
                    expr: scrutinee,
                    ref pat,
                } = self.thir[self.strip(cond)].kind =>
            {
                let (mut bound, mut before) = (Vec::new(), Vec::new());
                let test = self.if_let(scrutinee, pat, &mut bound, &mut before)?;
                if !bound.is_empty() {
                    let tmp = self.fresh("tmp");
                    out.push(StmtKind::Let(tmp.clone(), None).at(js_span));
                    self.stmt(e, &Dest::Assign(tmp.clone()), out)?;
                    return Ok(Expr::var(&tmp));
                }
                out.extend(before);
                let (yes, no) = (self.evaluated(then)?, self.evaluated(els)?);
                let plain = yes.statements.is_empty() && no.statements.is_empty();
                Ok(match (&yes.value.kind, &no.value.kind) {
                    _ if matches!(test.kind, js::ExprKind::Bool(true)) && yes.statements.is_empty() => yes.value,
                    (js::ExprKind::Bool(true), js::ExprKind::Bool(false)) if plain => test,
                    (js::ExprKind::Bool(false), js::ExprKind::Bool(true)) if plain => Expr::unary(UnaryOp::Not, test),
                    _ => self.conditional(test, yes, no, js_span, out),
                })
            }
            // Control flow: run it as statements, then read the result.
            ExprKind::Scope { .. }
            | ExprKind::If { .. }
            | ExprKind::Match { .. }
            | ExprKind::Block { .. }
            | ExprKind::NeverToAny { .. }
            | ExprKind::Return { .. }
            | ExprKind::Break { .. }
            | ExprKind::Continue { .. }
            | ExprKind::Assign { .. }
            | ExprKind::AssignOp { .. } => {
                if ty.is_unit() || ty.is_never() {
                    self.stmt(e, &Dest::Discard, out)?;
                    return Ok(Expr::undefined());
                }
                let tmp = self.fresh("tmp");
                out.push(StmtKind::Let(tmp.clone(), None).at(js_span));
                self.stmt(e, &Dest::Assign(tmp.clone()), out)?;
                Ok(Expr::var(&tmp))
            }
            _ => Err(self.unsupported(span, "this expression")),
        }
    }

    fn evaluated(&mut self, e: ExprId) -> R<Evaluation> {
        let mut statements = Vec::new();
        let value = self.expr(e, &mut statements)?;
        Ok(Evaluation { statements, value })
    }

    /// Branch prerequisites belong to the selected branch. `is_simple` is
    /// only a readability heuristic: even a call can need setup and copy-back.
    fn conditional(
        &mut self,
        cond: Expr,
        mut yes: Evaluation,
        mut no: Evaluation,
        span: js::Span,
        out: &mut Vec<Stmt>,
    ) -> Expr {
        if yes.statements.is_empty() && no.statements.is_empty() {
            return Expr::cond(cond, yes.value, no.value);
        }
        let name = self.fresh("tmp");
        out.push(StmtKind::Let(name.clone(), None).at(span));
        yes.statements
            .push(StmtKind::Assign(Expr::var(&name), yes.value).at(span));
        no.statements
            .push(StmtKind::Assign(Expr::var(&name), no.value).at(span));
        out.push(StmtKind::If(cond, yes.statements, Some(no.statements)).at(span));
        Expr::var(&name)
    }

    /// Sequence actual lowering results, not a prediction of their effects.
    /// Earlier operands are captured before a later operand's prerequisites.
    fn operands(&mut self, list: &[ExprId], out: &mut Vec<Stmt>) -> R<Vec<Expr>> {
        let moves = self.defer_moves(list)?;
        let mut values = self.operands_in_order(list, out)?;
        self.end_moves(&moves, &mut values, out);
        Ok(values)
    }

    fn operands_in_order(&mut self, list: &[ExprId], out: &mut Vec<Stmt>) -> R<Vec<Expr>> {
        let mut values: Vec<(Expr, bool)> = Vec::new();
        for &e in list {
            let evaluated = self.evaluated(e)?;
            if !evaluated.statements.is_empty() {
                for (value, settled) in &mut values {
                    if !*settled {
                        if !self.capture_jsx(value, out) {
                            let original = std::mem::replace(value, Expr::undefined());
                            *value = self.spill("tmp", original, out);
                        }
                        *settled = true;
                    }
                }
            }
            out.extend(evaluated.statements);
            // Borrowed or immutable places cannot change before the call. A
            // reference they're reached through can: `&mut *cur` of
            // `index_mut(&mut *cur, { cur = &mut b; 0 })` is the `Vec` `cur`
            // held then (ADR 0099).
            let borrowed = matches!(self.thir[self.strip(e)].kind, ExprKind::Borrow { arg, .. }
                if self.place(arg).is_some() && !self.through_rebound(arg, false));
            let settled = evaluated.value.is_constant()
                || borrowed
                || self.stable_place(self.strip_refs(e)).is_some()
                || self.ref_place(e).is_some_and(|(_, mutable)| !mutable);
            values.push((evaluated.value, settled));
        }
        Ok(values.into_iter().map(|(value, _)| value).collect())
    }

    /// Does calling `fun` become an assignment statement?
    fn is_assignment_call(&self, fun: ExprId) -> bool {
        if matches!(
            self.std_fn(fun),
            Some(
                Std::CellSet
                    | Std::Clear
                    | Std::Panic
                    | Std::PanicFmt
                    | Std::BeginPanic
                    | Std::PushStr
                    | Std::AssignOperator(_)
            )
        ) {
            return true;
        }
        let &ty::FnDef(def_id, _) = self.thir[self.strip(fun)].ty.kind() else {
            return false;
        };
        is_binding(self.tcx, def_id) && matches!(js_form(self.tcx, def_id), JsForm::Set(_))
    }

    /// Is `e` a Rust expression that JS can only write as statements?
    fn is_control_flow(&self, e: ExprId) -> bool {
        match self.thir[self.strip(e)].kind {
            ExprKind::Match { ref arms, .. } => {
                self.body_query().as_await(e).is_none()
                    && self.body_query().as_question(e).is_none()
                    && !self.is_matches(arms)
            }
            ExprKind::If { .. } | ExprKind::Block { .. } | ExprKind::Loop { .. } => true,
            _ => false,
        }
    }

    /// Prefer expression-shaped output for `e`? This is a syntax heuristic,
    /// not proof that lowering emits no prerequisites. Inspect `Evaluation`
    /// before moving a value across an evaluation region.
    fn is_simple(&self, e: ExprId) -> bool {
        match self.thir[e].kind {
            ExprKind::Scope { value, .. } => {
                !matches!(self.thir[value].kind, ExprKind::Loop { .. }) && self.is_simple(value)
            }
            ExprKind::Use { source }
            | ExprKind::ValueTypeAscription { source, .. }
            | ExprKind::PlaceTypeAscription { source, .. }
            | ExprKind::Cast { source }
            | ExprKind::PointerCoercion { source, .. }
            | ExprKind::Borrow { arg: source, .. }
            | ExprKind::Deref { arg: source }
            | ExprKind::Unary { arg: source, .. } => self.is_simple(source),
            ExprKind::Literal { .. }
            | ExprKind::NonHirLiteral { .. }
            | ExprKind::VarRef { .. }
            | ExprKind::UpvarRef { .. }
            | ExprKind::StaticRef { .. }
            | ExprKind::NamedConst { .. }
            | ExprKind::ZstLiteral { .. } => true,
            ExprKind::Field { lhs, .. } => self.is_simple(lhs),
            ExprKind::Index { lhs, index } => self.is_simple(lhs) && self.is_simple(index),
            ExprKind::Match { .. } if let Some(awaited) = self.body_query().as_await(e) => self.is_simple(awaited),
            // Its body's statements go inside the arrow; only snapshots come first.
            ExprKind::Closure(ref closure) => closure.upvars.iter().all(|&u| !self.needs_snapshot(u)),
            ExprKind::Tuple { ref fields } | ExprKind::Array { ref fields } => {
                fields.iter().all(|&f| self.is_simple(f))
            }
            ExprKind::Adt(ref adt) => {
                let base_simple = match &adt.base {
                    AdtExprBase::Base(fru) => self.is_simple(fru.base),
                    _ => true,
                };
                // Fields written out of declaration order may need temporaries.
                base_simple
                    && adt.fields.is_sorted_by_key(|f| f.name)
                    && adt.fields.iter().all(|f| self.is_simple(f.expr))
            }
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::LogicalOp { lhs, rhs, .. } => {
                self.is_simple(lhs) && self.is_simple(rhs)
            }
            // `cell.set(v)` and JS property setters are assignment statements.
            ExprKind::Call { fun, ref args, .. } => {
                !self.is_assignment_call(fun) && self.is_simple(fun) && args.iter().all(|&a| self.is_simple(a))
            }
            ExprKind::If {
                cond,
                then,
                else_opt: Some(els),
                ..
            } => self.is_simple(cond) && self.is_simple(then) && self.is_simple(els),
            // `format_args!`, whose arguments are written in place if they can be.
            // Out of order, its arguments may need `const`s (`lower_format_args`).
            ExprKind::Block { .. } if let Some(f) = self.as_format_args(e) => {
                self.in_order(&f, self.thir[e].span) && f.values.iter().all(|&v| self.is_simple(v))
            }
            ExprKind::Block { block } => {
                let block = &self.thir[block];
                !block.targeted_by_break && block.stmts.is_empty() && block.expr.is_none_or(|t| self.is_simple(t))
            }
            _ => false,
        }
    }

    // ── Leaves ──────────────────────────────────────────────────────────

    fn literal(&self, lit: &LitKind, neg: bool, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        match *lit {
            LitKind::Bool(b) => Ok(Expr::bool(b)),
            LitKind::Str(s, _) => Ok(Expr::str(s.as_str())),
            // A `char` is a string of one character (ADR 0034).
            LitKind::Char(c) => Ok(Expr::str(c.to_string())),
            LitKind::Int(n, _) => {
                let num = self.num(ty, span)?;
                let n = n.get() as i128;
                // One too big for its type, where `overflowing_literals` is
                // allowed, wraps to it as rustc's does: `256u8` is 0.
                Ok(num.wrap(num.literal(if neg { -n } else { n })))
            }
            LitKind::Byte(b) => Ok(Expr::int(b.into())),
            // `b"GET"`, a `&[u8; 3]`: its bytes, `[71, 69, 84]`, as an array of
            // `u8`s is (ADR 0126).
            LitKind::ByteStr(ref bytes, _) => Ok(Expr::array(
                bytes.as_byte_str().iter().map(|&b| Expr::int(b.into())).collect(),
            )),
            LitKind::Float(sym, _) if Num::of(ty) == Some(Num::F64) => {
                let x: f64 = sym
                    .as_str()
                    .replace('_', "")
                    .parse()
                    .expect("rustc validated the literal");
                Ok(Expr::num(if neg { -x } else { x }))
            }
            // Read as an `f32`, as rustc reads it: nearest to the digits,
            // rounded once (ADR 0122).
            LitKind::Float(sym, _) if Num::of(ty) == Some(Num::F32) => {
                let x: f32 = sym
                    .as_str()
                    .replace('_', "")
                    .parse()
                    .expect("rustc validated the literal");
                Ok(f32_literal(if neg { -x } else { x }))
            }
            _ => Err(self.unsupported(span, "this literal")),
        }
    }

    fn const_value(&self, value: ty::Value<'tcx>, span: Span) -> R<Expr> {
        if let Some(b) = value.try_to_bool() {
            return Ok(Expr::bool(b));
        }
        if let Some(c) = char_value(value) {
            return Ok(Expr::str(c.to_string()));
        }
        // A string literal pattern is a `str` constant under a `Deref`. A
        // reference's valtree is its pointee's, so rustc reads the bytes as a
        // `&str`'s. It's a JS string (ADR 0034): `===` compares the contents.
        if value.ty.is_str() {
            let as_ref = ty::Value {
                ty: Ty::new_imm_ref(self.tcx, self.tcx.lifetimes.re_static, value.ty),
                valtree: value.valtree,
            };
            if let Some(bytes) = as_ref.try_to_raw_bytes(self.tcx) {
                return Ok(Expr::str(str::from_utf8(bytes).expect("a `str` constant is UTF-8")));
            }
        }
        let (Some(num), Some(leaf)) = (Num::of(value.ty), value.try_to_leaf()) else {
            return Err(self.unsupported(span, "this constant pattern"));
        };
        Ok(num_literal(leaf.to_bits_unchecked(), num))
    }

    // ── Structs and tuples (ADR 0020) ───────────────────────────────────

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Map the original callsite into the compiler-owned source arena.
    fn js_span(&self, span: Span) -> js::Span {
        self.krate.sources.span(span)
    }

    fn body_query(&self) -> body_queries::BodyQuery<'a, 'tcx> {
        body_queries::BodyQuery {
            tcx: self.tcx,
            thir: self.thir,
        }
    }

    fn strip(&self, e: ExprId) -> ExprId {
        strip(self.thir, e)
    }

    /// Also skip borrows and derefs: `&*x` to `x`.
    fn strip_refs(&self, e: ExprId) -> ExprId {
        match self.thir[self.strip(e)].kind {
            ExprKind::Borrow { arg, .. } | ExprKind::Deref { arg } => self.strip_refs(arg),
            _ => self.strip(e),
        }
    }

    fn fresh(&mut self, base: &str) -> String {
        fresh_in(&mut self.names, base)
    }

    fn bind(&mut self, var: LocalVarId, name: &str, mutable: bool) -> String {
        let name = self.fresh(&camel_case(name));
        self.locals.vars.insert(
            var,
            Var {
                place: Expr::var(&name),
                mutable,
                depth: self.loops.len(),
            },
        );
        name
    }

    /// A `const` (ADR 0031). One of ours is its name, `SIZE` or `util.SIZE`,
    /// copied where a use might change it: each use is a value of its own.
    /// Anyone else's, like `u32::MAX`, is its value, written in place. A
    /// trait's in generic code, `S::SIDES`, is its impl's dictionary's (ADR 0106).
    fn named_const(&mut self, def_id: DefId, args: ty::GenericArgsRef<'tcx>, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        // One holding a `Cell`, which changes through a shared reference, is
        // its value, written in place: `{ value: 5 }` each time it's used.
        if self.krate.fns.contains_key(&def_id) && ty.is_freeze(self.tcx, self.typing_env) {
            // `fn_ref` also records the use, which is what imports its module.
            let place = self.fn_ref(def_id);
            return Ok(if self.contains_mutated(ty) {
                self.copy(place, ty)
            } else {
                place
            });
        }
        if let Some(value) =
            eval_const(self.tcx, self.typing_env, def_id, args, span).and_then(|value| const_js(self.tcx, value))
        {
            return Ok(value);
        }
        match self.tcx.trait_of_assoc(def_id) {
            Some(trait_id) if args.has_non_region_param() => {
                let dictionary = self.dictionary(ty::TraitRef::from_assoc(self.tcx, trait_id, args), span)?;
                Ok(Expr::member(dictionary, bindings::fn_name(self.tcx, def_id)))
            }
            _ => Err(self.unsupported(span, "this constant")),
        }
    }

    /// An integer `const`'s value: `x / SIZE` can't divide by zero.
    fn known_int(&self, e: ExprId) -> Option<i128> {
        let ExprKind::NamedConst { def_id, args, .. } = self.thir[self.strip(e)].kind else {
            return None;
        };
        let value = eval_const(self.tcx, self.typing_env, def_id, args, self.thir[e].span)?;
        const_js(self.tcx, value)?.as_int()
    }

    /// `const <base> = value;`, so it's evaluated here, then its name.
    fn spill(&mut self, base: &str, value: Expr, out: &mut Vec<Stmt>) -> Expr {
        let name = self.fresh(base);
        let span = value.span;
        out.push(StmtKind::Const(name.clone(), value).at(span));
        Expr::var(&name)
    }

    fn unsupported(&self, span: Span, what: &str) -> ErrorGuaranteed {
        self.tcx
            .dcx()
            .span_err(span, format!("rust-js does not support {what} yet"))
    }
}

/// The variable a place starts from: `p` for `p.x[0]`.
fn root_var(place: &Expr) -> Option<&str> {
    match &place.kind {
        js::ExprKind::Var(name) => Some(name),
        js::ExprKind::Member(object, _) | js::ExprKind::Index(object, _) => root_var(object),
        _ => None,
    }
}

/// A JS global, like `document`, or a path from one, like `console.log`.
fn global(name: &str) -> Expr {
    let mut parts = name.split('.');
    let first = Expr::var(parts.next().unwrap_or_default());
    parts.fold(first, Expr::member)
}

/// A fieldless enum's variants, each its name and its discriminant, in the
/// order they're declared: `Red = 1` is `("Red", 1)`.
fn discriminants<'tcx>(tcx: TyCtxt<'tcx>, adt: ty::AdtDef<'tcx>) -> Vec<(String, i128)> {
    adt.discriminants(tcx)
        .map(|(index, d)| {
            let value = d.val as i128;
            let value = if d.ty.is_signed() {
                let bits = d.ty.primitive_size(tcx).bits();
                (value << (128 - bits)) >> (128 - bits)
            } else {
                value
            };
            (bindings::variant_name(tcx, adt.variant(index)), value)
        })
        .collect()
}

/// A struct's or a variant's value, of its fields' values in declared order:
/// an object, tagged `{ TAG: "Circle", _0: r }` for a variant (ADR 0033), or
/// an array for a tuple struct (ADR 0020).
fn assembled(shape: Shape<'_>, tag: Option<String>, items: Vec<Expr>) -> Expr {
    match shape {
        Shape::Object(fields) => {
            let tag = tag.map(|name| Prop::Field("TAG".into(), Expr::str(name)));
            let fields = fields.into_iter().zip(items).map(|((name, _), v)| Prop::Field(name, v));
            Expr::object(tag.into_iter().chain(fields).collect())
        }
        _ => Expr::array(items),
    }
}

fn is_union(ty: Ty<'_>) -> bool {
    ty.ty_adt_def().is_some_and(|adt| adt.is_union())
}

/// A Rust variable's name as JS code writes it (ADR 0038): `set_count` is
/// `setCount`. Leading and trailing underscores stay (`_unused`, `type_`),
/// and so does a name with no lowercase letter, like a constant's.
fn camel_case(name: &str) -> String {
    let core = name.trim_matches('_');
    if !core.contains('_') || !core.contains(|c: char| c.is_ascii_lowercase()) {
        return name.to_string();
    }
    let lead = &name[..name.len() - name.trim_start_matches('_').len()];
    let trail = &name[name.trim_end_matches('_').len()..];
    let mut out = lead.to_string();
    for (i, word) in core.split('_').filter(|w| !w.is_empty()).enumerate() {
        let mut chars = word.chars();
        if i > 0
            && let Some(first) = chars.next()
        {
            out.push(first.to_ascii_uppercase());
        }
        out.extend(chars);
    }
    out.push_str(trail);
    out
}

/// `Counter` → `counter`: a value of the type, named after it.
fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    chars
        .next()
        .map(|c| c.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `(x, i) => [i, x]`, what `enumerate()` maps with.
fn is_enumerate_pair(f: &Expr) -> bool {
    let js::ExprKind::Arrow(params, body) = &f.kind else {
        return false;
    };
    let [js::Pattern::Name(x), js::Pattern::Name(i)] = params.as_slice() else {
        return false;
    };
    let [
        Stmt {
            kind: StmtKind::Return(Some(pair)),
            ..
        },
    ] = body.as_slice()
    else {
        return false;
    };
    let js::ExprKind::Array(items) = &pair.kind else {
        return false;
    };
    matches!(items.as_slice(), [a, b] if matches!(&a.kind, js::ExprKind::Var(n) if n == i) && matches!(&b.kind, js::ExprKind::Var(n) if n == x))
}

/// `&x` is `x`: a reference is the value (ADR 0023).
fn without_refs<'p, 'tcx>(mut pat: &'p Pat<'tcx>) -> &'p Pat<'tcx> {
    while let PatKind::Deref { subpattern, .. } = &pat.kind {
        pat = subpattern;
    }
    pat
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    fn recognition(&self) -> recognition::Recognition<'_, 'tcx> {
        recognition::Recognition {
            tcx: self.tcx,
            typing_env: self.typing_env,
            trait_impls: self.krate.trait_impls,
            foreign: self.krate.foreign,
        }
    }
}

/// Translate a frontend module identity into an owned link symbol.
fn module_symbol(module: LocalModDefId, export: &str) -> crate::js::Symbol {
    crate::js::Symbol {
        module: module.to_def_id().index.as_u32(),
        export: export.to_owned(),
    }
}
