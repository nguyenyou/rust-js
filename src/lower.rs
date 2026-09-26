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

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rustc_ast::{LitKind, Mutability};
use rustc_hir as hir;
use rustc_hir::def::CtorKind;
use rustc_hir::{BindingMode, ByRef, CoroutineDesugaring, CoroutineKind, CoroutineSource, HirId, LangItem, RangeEnd};
use rustc_middle::middle::region;
use rustc_middle::mir::{AssignOp, BinOp, BorrowKind, UnOp};
use rustc_middle::thir::{
    self as thir, AdtExprBase, ArmId, BlockId, BodyTy, ExprId, ExprKind, LocalVarId, LogicalOp, Pat, PatKind,
    PatRangeBoundary, Thir,
};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::{DefId, LocalDefId, LocalModDefId};
use rustc_span::{BytePos, DesugaringKind, ErrorGuaranteed, SourceFile, Span, Symbol, sym};

use crate::js::{self, Expr, Op, Prop, Stmt, StmtKind, UnaryOp};

mod analysis;
mod bindings;
mod calls;
mod combinators;
mod display;
mod format_spec;
mod jsx;
mod maps;
mod numbers;
mod ordering;
mod representation;
mod std_impls;
mod stdlib;
mod text;
mod traits;

use crate::runtime::Helper;
pub use analysis::{collect_bodies, lower_crate};
use bindings::{Export, JsForm, is_binding, js_form, js_name};
use representation::{
    Num, char_value, const_js, eval_const, is_fieldless_enum, num_literal, ordering_value, variant_field,
};
use stdlib::Std;

type R<T> = Result<T, ErrorGuaranteed>;

/// A function's THIR, copied out of rustc before borrowck steals it.
pub struct Body<'tcx> {
    def_id: LocalDefId,
    thir: Thir<'tcx>,
    expr: ExprId,
}

/// One Rust module's functions: a future JS file (ADR 0019).
pub struct LoweredModule {
    /// The module's path below the crate root: `[]` for the root itself,
    /// `["math", "stats"]` for `crate::math::stats`.
    pub path: Vec<String>,
    /// The `.rs` file the module's code lives in.
    pub file: Arc<SourceFile>,
    /// What it imports from JS modules (ADR 0028), with the modules' names
    /// as written in `#[link_name]`.
    pub packages: Vec<js::Package>,
    /// The modules this one calls into, as `(alias, path)`.
    pub imports: Vec<(String, Vec<String>)>,
    pub namespaces: Vec<js::Namespace>,
    pub consts: Vec<js::Const>,
    pub functions: Vec<js::Function>,
    pub caches: Vec<String>,
    /// Runtime helpers its functions use.
    pub runtime: Vec<Helper>,
    /// Whether it has JSX, so it's a `.jsx` file (ADR 0040).
    pub jsx: bool,
}

/// A `#[test]` function (ADR 0026).
pub struct TestFn {
    /// The module it's in, and its JS name there.
    pub module: Vec<String>,
    pub name: String,
    /// What the runner calls it: `tests::adds`.
    pub label: String,
    /// `#[should_panic]`, with its `expected` substring if any.
    pub should_panic: Option<Option<String>>,
    pub ignore: bool,
}

/// The crate as JS: one module per Rust module, and the tests in test mode.
pub struct Lowered {
    pub modules: Vec<LoweredModule>,
    pub tests: Vec<TestFn>,
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

/// A variable bound by a pattern, and the place in the subject it matched.
struct Binding<'tcx> {
    var: LocalVarId,
    name: String,
    mutable: bool,
    /// `ref mut`, or bound through a `&mut` subject: writes through it
    /// write the place it matched.
    by_ref_mut: bool,
    place: Expr,
    ty: Ty<'tcx>,
}

/// The parts of a `for pat in head { body }` (ADR 0025).
struct ForLoop<'a, 'tcx> {
    head: ExprId,
    pat: &'a Pat<'tcx>,
    body: ExprId,
    /// The `loop` inside, which `break` and `continue` refer to.
    scope: region::Scope,
    hir_id: HirId,
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

/// Crate facts and dependencies recorded while lowering function bodies.
struct CrateFacts<'a, 'tcx> {
    mutated: &'a HashSet<Ty<'tcx>>,
    changed_vecs: &'a HashSet<Ty<'tcx>>,
    closures: &'a HashMap<LocalDefId, &'a Body<'tcx>>,
    bodies: &'a HashMap<DefId, &'a Body<'tcx>>,
    fns: &'a HashMap<DefId, FnInfo>,
    imports: &'a HashMap<Export, String>,
    trait_impls: &'a [DefId],
    references: RefCell<HashSet<(LocalModDefId, DefId)>>,
    package_uses: RefCell<HashSet<(LocalModDefId, Export)>>,
    /// Each item, and a function it names (ADR 0060).
    uses: RefCell<Vec<(DefId, DefId)>>,
}

struct FnCx<'a, 'tcx> {
    krate: &'a CrateFacts<'a, 'tcx>,
    tcx: TyCtxt<'tcx>,
    typing_env: ty::TypingEnv<'tcx>,
    evidence: Vec<(ty::TraitRef<'tcx>, Expr)>,
    /// In a trait's default body copied into an impl (ADR 0049): the impl's
    /// arguments for the trait's parameters, `Self` among them.
    self_args: Option<ty::GenericArgsRef<'tcx>>,
    /// While lowering a closure: the places it captured into snapshots.
    captures: HashMap<(LocalVarId, Vec<usize>), Var>,
    /// The range, in rustc's global source map, of the `.rs` file this
    /// function's module lives in, for `js_span`.
    file_start: BytePos,
    file_end: BytePos,
    thir: &'a Thir<'tcx>,
    /// The module being lowered, and its import aliases for other modules.
    module: LocalModDefId,
    aliases: &'a HashMap<LocalModDefId, String>,
    vars: HashMap<LocalVarId, Var>,
    /// JS names already taken in this function.
    names: HashSet<String>,
    /// Those taken by the module: its functions, imports and globals.
    module_names: &'a HashSet<String>,
    labels: HashSet<String>,
    loops: Vec<Loop>,
    runtime: HashSet<Helper>,
    /// Whether this function makes JSX.
    jsx: bool,
    /// In a function that writes to a `Formatter` (ADR 0054): its variable,
    /// and the JS string that stands for it.
    writer: Option<(Option<LocalVarId>, String)>,
    /// A `&mut` to a map's value that's a primitive, `if let Some(n) =
    /// m.get_mut(&k)`: a copy of it, and the map and key a write puts it back
    /// in (ADR 0059). While it lives, nothing else can change that entry.
    slots: HashMap<LocalVarId, (Expr, Expr)>,
    /// The call being lowered is a statement of its own: its value isn't used,
    /// so a map's `insert` is `m.set(k, v)` (ADR 0059).
    discarded: bool,
    /// The item being lowered: what `fn_ref` records as using its target.
    item: DefId,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    fn lower_fn(&mut self, body: &Body<'tcx>) -> R<LoweredFn> {
        let def_id = body.def_id.to_def_id();
        let mut out = Vec::new();
        let thir = self.thir;
        let evidence = self.evidence_params(def_id);
        let (mut params, is_async) = self.lower_signature(def_id, &thir.params.raw, body.expr, &mut out)?;
        params.extend(evidence);

        Ok(LoweredFn {
            function: js::Function {
                name: self.krate.fns[&def_id].name.clone(),
                params,
                body: out,
                export: self.tcx.visibility(def_id).is_public()
                    && (self.tcx.def_kind(def_id) != rustc_hir::def::DefKind::AssocFn
                        || self.tcx.inherent_impl_of_assoc(def_id).is_some()),
                is_async,
                span: self.js_span(self.tcx.def_span(def_id)),
                name_span: self
                    .tcx
                    .def_ident_span(def_id)
                    .map_or(js::Span::NONE, |s| self.js_span(s)),
            },
            runtime: std::mem::take(&mut self.runtime),
            jsx: self.jsx,
        })
    }

    /// A function's parameters and body, in `out`, and whether it's `async`.
    /// One that writes to a `Formatter` returns the string (ADR 0054).
    fn lower_signature(
        &mut self,
        def_id: DefId,
        params: &[thir::Param<'tcx>],
        body: ExprId,
        out: &mut Vec<Stmt>,
    ) -> R<(Vec<js::Pattern>, bool)> {
        let span = self.tcx.def_span(def_id);
        if let Some(formatter) = self.formatter_param(def_id) {
            return Ok((self.lower_writer(params, formatter, body, span, out)?, false));
        }
        let params = self.lower_params(params, span, out)?;
        let BodyTy::Fn(sig) = self.thir.body_type else {
            return Err(self.unsupported(span, "this kind of body"));
        };
        self.check_value_ty(sig.output(), span)?;
        let dest = if sig.output().is_unit() {
            Dest::Discard
        } else {
            Dest::Return
        };
        let is_async = self.lower_body(body, &dest, out)?;
        Ok((params, is_async))
    }

    /// Name the parameters. One with a pattern (`(x, y): (i32, i32)`) is
    /// taken whole, then taken apart at the start of the body in `out`.
    fn lower_params(&mut self, params: &[thir::Param<'tcx>], span: Span, out: &mut Vec<Stmt>) -> R<Vec<js::Pattern>> {
        let mut names = Vec::new();
        for param in params {
            let span = param.ty_span.unwrap_or(span);
            self.check_value_ty(param.ty, span)?;
            // `|&x|`: a reference is the value (ADR 0023), so the parameter is `x`.
            let mut inner = param.pat.as_deref();
            while let Some(Pat {
                kind: PatKind::Deref { subpattern, .. },
                ..
            }) = inner
            {
                inner = Some(subpattern);
            }
            let binding = |p: &Pat<'tcx>| {
                matches!(
                    p.kind,
                    PatKind::Binding {
                        mode: BindingMode(ByRef::No, Mutability::Not),
                        subpattern: None,
                        ..
                    }
                )
            };
            let peeled = if inner.is_some_and(binding) {
                inner
            } else {
                param.pat.as_deref()
            };
            // `Props { initial, label }: Props` is `{ initial, label }`, as a
            // React component takes its props.
            if let Some(pat) = peeled
                && let Some((pattern, _)) = self.js_pattern(pat)
            {
                names.push(pattern);
                continue;
            }
            let name = match peeled {
                Some(pat) => match &pat.kind {
                    PatKind::Binding {
                        name,
                        var,
                        mode,
                        subpattern: None,
                        ..
                    } => {
                        self.check_by_value(*mode, pat.ty, pat.span)?;
                        // `async fn f((a, b): ..)` takes `__arg0`, and takes it
                        // apart in its body (ADR 0029): named as in a plain `fn`.
                        let generated = name
                            .as_str()
                            .strip_prefix("__arg")
                            .is_some_and(|n| n.parse::<u32>().is_ok());
                        // A method's `self` is named after its type, `counter`
                        // for a `Counter`, as a JS function of one would name it.
                        let receiver = match pat.ty.peel_refs().kind() {
                            ty::Adt(adt, _) if name.as_str() == "self" => {
                                Some(lower_first(self.tcx.item_name(adt.did()).as_str()))
                            }
                            _ => None,
                        };
                        let rust_name = match &receiver {
                            Some(receiver) => receiver.as_str(),
                            None if generated => "param",
                            None => name.as_str(),
                        };
                        self.bind(*var, rust_name, mode.1 == Mutability::Mut)
                    }
                    PatKind::Wild => self.fresh("_"),
                    // `(x, y): (i32, i32)`: take the whole value, then take it apart.
                    _ => {
                        let name = self.fresh("param");
                        self.destructure(pat, Expr::var(&name), true, out)?;
                        name
                    }
                },
                None => self.fresh("_"),
            };
            names.push(name.into());
        }
        Ok(names)
    }

    /// A tuple or struct pattern of plain variables and `_`s, as JS
    /// destructuring: `[count, setCount]`, `{ initial, label }`. Binds the
    /// variables, and says whether one is `mut`. `None`, binding nothing, if a
    /// part is anything else, or needs a copy of its own (ADR 0020).
    fn js_pattern(&mut self, pat: &Pat<'tcx>) -> Option<(js::Pattern, bool)> {
        let PatKind::Leaf { subpatterns } = &pat.kind else {
            return None;
        };
        // `(i, &x)`: a reference is the value (ADR 0023), so that part is `x`.
        let parts: Vec<_> = subpatterns
            .iter()
            .map(|field| match without_refs(&field.pattern).kind {
                PatKind::Wild => Some((field.field.as_usize(), None)),
                PatKind::Binding {
                    name,
                    var,
                    mode: BindingMode(ByRef::No, mutability),
                    subpattern: None,
                    ty,
                    ..
                } if self.unsupported_part(ty).is_none() && !(self.contains_mutated(ty) && self.is_copy(ty)) => {
                    Some((field.field.as_usize(), Some((name, var, mutability == Mutability::Mut))))
                }
                _ => None,
            })
            .collect::<Option<_>>()?;
        let mutable = parts.iter().any(|(_, part)| part.is_some_and(|(_, _, m)| m));
        let pattern = match self.shape(pat.ty) {
            Shape::Array(tys) => {
                let mut items = vec![None; tys.len()];
                for (i, part) in parts {
                    items[i] = part.map(|(name, var, m)| self.bind(var, name.as_str(), m));
                }
                // `[a, b, , ]` is `[a, b]`.
                while items.last().is_some_and(Option::is_none) {
                    items.pop();
                }
                js::Pattern::Array(items)
            }
            Shape::Object(fields) => js::Pattern::Object(
                parts
                    .into_iter()
                    .filter_map(|(i, part)| {
                        part.map(|(name, var, m)| (fields[i].0.clone(), self.bind(var, name.as_str(), m)))
                    })
                    .collect(),
            ),
            Shape::Other => return None,
        };
        Some((pattern, mutable))
    }

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
                if let ExprKind::Loop { body } = self.thir[value].kind {
                    self.lower_loop(region_scope, hir_id, body, dest, span, out)
                } else {
                    self.stmt(value, dest, out)
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
                let cond = match self.thir[self.strip(cond)].kind {
                    ExprKind::Let {
                        expr: scrutinee,
                        ref pat,
                    } => self.if_let(scrutinee, pat, &mut then_out, out)?,
                    _ => self.expr(cond, out)?,
                };
                self.stmt(then, dest, &mut then_out)?;
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
            ExprKind::Match { .. } if let Some(for_loop) = self.as_for(e) => self.lower_for(for_loop, span, out),
            ExprKind::Match {
                scrutinee, ref arms, ..
            } if self.as_await(e).is_none() && self.as_question(e).is_none() && !self.is_matches(arms) => {
                self.lower_match(scrutinee, arms, dest, out)
            }
            // A function that writes to a `Formatter` returns what it wrote (ADR 0054).
            ExprKind::Return { value } if let Some((_, name)) = self.writer.clone() => {
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
            ExprKind::Assign { lhs, rhs } if self.slots_write(lhs) => {
                let value = self.expr(rhs, out)?;
                self.slot_write(lhs, &|_, _| Ok(value.clone()), expr.span, out)
                    .map(|_| ())
            }
            ExprKind::AssignOp { op, lhs, rhs } if self.slots_write(lhs) => {
                let rhs_js = self.expr(rhs, out)?;
                let ty = self.thir[lhs].ty;
                let span = expr.span;
                let write =
                    |this: &mut Self, current| this.binary(assign_op(op), current, rhs_js.clone(), None, ty, span);
                self.slot_write(lhs, &write, span, out).map(|_| ())
            }
            ExprKind::Assign { lhs, rhs } if let Some(slot) = self.map_slot(lhs) => {
                let value = self.expr(rhs, out)?;
                self.map_slot_write(slot, &|_, _| Ok(value.clone()), expr.span, out)
            }
            ExprKind::AssignOp { op, lhs, rhs } if let Some(slot) = self.map_slot(lhs) => {
                let rhs_js = self.expr(rhs, out)?;
                let ty = self.thir[lhs].ty;
                let known = self.known_int(rhs);
                let span = expr.span;
                self.map_slot_write(
                    slot,
                    &|this, current| this.binary(assign_op(op), current, rhs_js.clone(), known, ty, span),
                    span,
                    out,
                )
            }
            // `v[i] = x` or `v[i].x = y`: Rust runs the right side first.
            ExprKind::Assign { lhs, rhs } if self.place(lhs).is_none() && self.in_element(lhs) => {
                let value = self.expr(rhs, out)?;
                let target = self.element_target(lhs, out)?;
                out.push(StmtKind::Assign(target, value).at(span));
                Ok(())
            }
            ExprKind::Assign { lhs, rhs } => {
                let target = self.assignee(lhs)?;
                match &target.kind {
                    js::ExprKind::Var(name) if !self.is_simple(rhs) => {
                        let name = name.clone();
                        self.stmt(rhs, &Dest::Assign(name), out)
                    }
                    _ => {
                        let value = self.expr(rhs, out)?;
                        out.push(StmtKind::Assign(target, value).at(span));
                        Ok(())
                    }
                }
            }
            ExprKind::AssignOp { op, lhs, rhs } => {
                let rhs_js = self.expr(rhs, out)?;
                let target = match self.place(lhs) {
                    Some(_) => self.assignee(lhs)?,
                    None => self.element_target(lhs, out)?,
                };
                let ty = self.thir[lhs].ty;
                let current = target.clone().or_at(self.js_span(self.thir[lhs].span));
                let known = self.known_int(rhs);
                let value = self
                    .binary(assign_op(op), current, rhs_js, known, ty, expr.span)?
                    .or_at(span);
                out.push(StmtKind::Assign(target, value).at(span));
                Ok(())
            }
            _ => {
                // A call whose value goes nowhere says so, for `insert` (ADR 0059).
                self.discarded =
                    matches!(dest, Dest::Discard) && matches!(self.thir[self.strip(e)].kind, ExprKind::Call { .. });
                let value = match (dest, self.place(e)) {
                    // Returning a place of this function's own hands its value
                    // over without a copy: every local dies here, so nothing is
                    // left to share it. One reached through a reference, or a
                    // closure's capture, outlives the call, so it's copied.
                    (Dest::Return, Some((place, _))) if self.is_local_place(e) => place.or_at(span),
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
        self.block_stmts(block, out)?;
        match self.thir[block].expr {
            Some(tail) => self.stmt(tail, dest, out),
            None => Ok(()),
        }
    }

    /// A block's statements, without its tail expression.
    fn block_stmts(&mut self, block: BlockId, out: &mut Vec<Stmt>) -> R<()> {
        let block = &self.thir[block];
        if block.targeted_by_break {
            return Err(self.unsupported(block.span, "labeled blocks"));
        }
        // Every local gets a unique JS name, so a Rust block needs no JS
        // block of its own: its statements go straight into `out`.
        for &stmt in &block.stmts {
            match &self.thir[stmt].kind {
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
                        continue;
                    }
                    self.lower_let(pattern, *initializer, *span, out)?;
                }
            }
        }
        Ok(())
    }

    fn lower_let(&mut self, pat: &Pat<'tcx>, init: Option<ExprId>, span: Span, out: &mut Vec<Stmt>) -> R<()> {
        // `let f = { let c = ..; move |n| .. };`: the block's statements
        // first, then `const f = ..` of its value. Every local has a JS name
        // of its own, so none of them can clash where they now are.
        if let Some(init) = init
            && let ExprKind::Block { block } = self.thir[self.strip(init)].kind
            && let thir::Block {
                targeted_by_break: false,
                expr: Some(value),
                safety_mode: thir::BlockSafety::Safe,
                span: block_span,
                ..
            } = self.thir[block]
            && !block_span.from_expansion()
        {
            self.block_stmts(block, out)?;
            return self.lower_let(pat, Some(value), span, out);
        }
        // `async fn f(x)` moves `x` into its body with `let x = x;` (ADR 0029).
        // In JS the body is the function's, so they're one variable.
        // `format_args!` holds its values in `super let args = (&a, &b);`, then
        // `super let args = [Argument::new_display(args.0), ..];` (ADR 0034).
        // Both are only read from, so they name their parts where they are:
        // `format!("{} ms", t)` is `t + " ms"`, with no arrays in between.
        if self.in_format_args(span)
            && let PatKind::Binding {
                var,
                mode: BindingMode(ByRef::No, Mutability::Not),
                subpattern: None,
                ..
            } = pat.kind
            && let Some(init) = init
        {
            let parts = match self.thir[self.strip(init)].kind {
                ExprKind::Tuple { ref fields } => self.tuple_parts(fields, "arg", true, out)?,
                _ => self.expr(init, out)?,
            };
            self.vars.insert(
                var,
                Var {
                    place: parts,
                    mutable: false,
                    depth: self.loops.len(),
                },
            );
            return Ok(());
        }
        // `let a = f()?;` on an option: the value is `a` itself, so it's kept
        // under that name: `const a = f(); if (a == null) { return undefined; }`.
        if let PatKind::Binding {
            name,
            var,
            mode: BindingMode(ByRef::No, Mutability::Not),
            subpattern: None,
            ..
        } = pat.kind
            && let Some(init) = init
            && let Some(tried) = self.as_question(init)
            && self.option_of(self.thir[tried].ty).is_some()
        {
            let value = self.question(init, tried, Some(name.as_str()), out)?;
            self.vars.insert(
                var,
                Var {
                    place: value,
                    mutable: false,
                    depth: self.loops.len(),
                },
            );
            return Ok(());
        }
        if span.is_desugaring(DesugaringKind::Async)
            && let PatKind::Binding {
                var,
                mode: BindingMode(ByRef::No, mutability),
                subpattern: None,
                ..
            } = pat.kind
            && let Some(init) = init
            && let ExprKind::UpvarRef { var_hir_id, .. } = self.thir[self.strip(init)].kind
            && let Some(outer) = self.vars.get(&var_hir_id)
        {
            let alias = Var {
                place: outer.place.clone(),
                mutable: mutability == Mutability::Mut,
                depth: outer.depth,
            };
            self.vars.insert(var, alias);
            return Ok(());
        }
        let span = self.js_span(span);
        match &pat.kind {
            PatKind::Binding {
                name,
                var,
                mode,
                subpattern: None,
                ty,
                ..
            } => {
                self.check_by_value(*mode, *ty, pat.span)?;
                self.check_value_ty(*ty, pat.span)?;
                let mutable = mode.1 == Mutability::Mut;
                match init {
                    // Only control flow needs `let x;` and then assignments in
                    // its branches. Anything else (a closure, say) computes its
                    // statements first and then has a value.
                    Some(init) if self.is_simple(init) || !self.is_control_flow(init) => {
                        let value = self.expr(init, out)?;
                        let name = self.bind(*var, name.as_str(), mutable);
                        let kind = if mutable {
                            StmtKind::Let(name, Some(value))
                        } else {
                            StmtKind::Const(name, value)
                        };
                        out.push(kind.at(span));
                    }
                    Some(init) => {
                        let name = self.bind(*var, name.as_str(), mutable);
                        out.push(StmtKind::Let(name.clone(), None).at(span));
                        self.stmt(init, &Dest::Assign(name), out)?;
                    }
                    None => {
                        let name = self.bind(*var, name.as_str(), mutable);
                        out.push(StmtKind::Let(name, None).at(span));
                    }
                }
                Ok(())
            }
            PatKind::Wild => match init {
                Some(init) => self.stmt(init, &Dest::Discard, out),
                None => Ok(()),
            },
            // `let (q, r) = divmod(a, b);`
            _ => {
                let Some(init) = init else {
                    return Err(self.unsupported(pat.span, "this `let` pattern without a value"));
                };
                // `let (count, set_count) = use_state(0);` is
                // `const [count, setCount] = useState(0);`. A place is taken
                // apart where it is, below.
                if self.place(init).is_none() && !self.is_control_flow(init) {
                    let value = self.expr(init, out)?;
                    if let Some((pattern, mutable)) = self.js_pattern(pat) {
                        out.push(
                            StmtKind::Destructure {
                                pattern,
                                value,
                                mutable,
                            }
                            .at(span),
                        );
                        return Ok(());
                    }
                    let subject = self.spill("tmp", value, out);
                    return self.destructure(pat, subject, true, out);
                }
                let (subject, stable) = self.subject(init, "tmp", out)?;
                self.destructure(pat, subject, stable, out)
            }
        }
    }

    /// Bind the variables of an irrefutable pattern to the parts of `subject`.
    fn destructure(&mut self, pat: &Pat<'tcx>, subject: Expr, stable: bool, out: &mut Vec<Stmt>) -> R<()> {
        let mut bindings = Vec::new();
        if self.pattern_test(pat, &subject, &mut bindings)?.is_some() {
            return Err(self.unsupported(pat.span, "this refutable pattern"));
        }
        self.bind_all(bindings, stable, self.js_span(pat.span), out);
        Ok(())
    }

    /// Where a `match` or `let` finds the value it takes apart, and whether
    /// that stays unchanged while the pattern's variables live.
    ///
    /// A place is used where it is, and may be stable (see `stable_place`).
    /// A tuple of stable places (`match (a, b)`) is used without building
    /// the array. Anything else is computed once into a `const` named `base`,
    /// which is stable: no Rust variable can move or change it.
    fn subject(&mut self, e: ExprId, base: &str, out: &mut Vec<Stmt>) -> R<(Expr, bool)> {
        if let Some(place) = self.stable_place(e) {
            return Ok((place, true));
        }
        // `&x`: a reference is the value (ADR 0023), and a borrowed `x` stays put.
        if let ExprKind::Borrow {
            borrow_kind: BorrowKind::Shared,
            arg,
        } = self.thir[self.strip(e)].kind
            && let Some(place) = self.stable_place(arg)
        {
            return Ok((place, true));
        }
        if let Some((place, _)) = self.place(e) {
            return Ok((place, false));
        }
        // `match (a, b)` tests `a` and `b` where they are. A part that isn't a
        // place that stays put goes in a `const` of its own, in order.
        if let ExprKind::Tuple { ref fields } = self.thir[self.strip(e)].kind
            && !fields.is_empty()
        {
            return Ok((self.tuple_parts(fields, base, false, out)?, true));
        }
        let value = self.expr(e, out)?;
        let name = self.fresh(base);
        out.push(StmtKind::Const(name.clone(), value).at(self.js_span(self.thir[e].span)));
        Ok((Expr::var(&name), true))
    }

    /// `[a, b]` for a tuple `(a, b)` that's only taken apart: each part a
    /// place that stays put, a constant, or else a `const` of its own, in order.
    /// With `used_once`, a part without effects is written in place too.
    fn tuple_parts(&mut self, fields: &[ExprId], base: &str, used_once: bool, out: &mut Vec<Stmt>) -> R<Expr> {
        let mut parts = Vec::new();
        for &f in fields {
            // `format_args!`'s parts are references: `&a` is `a`.
            let part = match self.stable_place(self.strip_refs(f)) {
                Some(place) => place,
                None => {
                    let value = self.expr(f, out)?;
                    if value.is_constant() || (used_once && !value.has_effects()) {
                        value
                    } else {
                        self.spill(base, value, out)
                    }
                }
            };
            parts.push(part);
        }
        Ok(Expr::array(parts))
    }

    /// Is `span` rustc's lowering of a `format_args!` (so `format!`, `panic!`, ..)?
    fn in_format_args(&self, span: Span) -> bool {
        matches!(span.desugaring_kind(), Some(DesugaringKind::FormatLiteral { .. }))
    }

    /// Give a pattern's variables their JS meaning. Immutable ones bound into
    /// a stable subject just name the place they matched, as ReScript does:
    /// `P { x, y } => x + y` becomes `p.x + p.y`. The rest get a variable
    /// holding their own value.
    fn bind_all(&mut self, bindings: Vec<Binding<'tcx>>, stable: bool, span: js::Span, out: &mut Vec<Stmt>) {
        for b in bindings {
            // A place that's computed, like `$someValue(o)`, goes in a `const`.
            // A `ref mut` one always does: `*r = x` writes the place it names.
            if (stable || b.by_ref_mut) && !b.mutable && !b.place.has_effects() {
                self.vars.insert(
                    b.var,
                    Var {
                        place: b.place,
                        mutable: false,
                        depth: self.loops.len(),
                    },
                );
                continue;
            }
            let value = self.copy_if_needed(b.place, b.ty).or_at(span);
            let name = self.bind(b.var, &b.name, b.mutable);
            let kind = if b.mutable {
                StmtKind::Let(name, Some(value))
            } else {
                StmtKind::Const(name, value)
            };
            out.push(kind.at(span));
        }
    }

    fn lower_loop(
        &mut self,
        scope: region::Scope,
        hir_id: HirId,
        body: ExprId,
        dest: &Dest,
        span: js::Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let label_base = match self.tcx.hir_expect_expr(hir_id).kind {
            hir::ExprKind::Loop(_, Some(label), ..) => label.ident.name.as_str().trim_start_matches('\'').to_string(),
            _ => "loop".to_string(),
        };
        self.loops.push(Loop {
            scope,
            label_base,
            label: None,
            dest: dest.clone(),
        });

        let mut body_out = Vec::new();
        // `while c { .. }` reaches us desugared as `loop { if c { .. } else { break } }`.
        // Put the `while` back.
        let cond = match self.as_while(body, scope) {
            Some((cond, then)) => {
                let cond = self.expr(cond, &mut body_out)?; // simple, so no statements
                self.stmt(then, &Dest::Discard, &mut body_out)?;
                cond
            }
            None => {
                self.stmt(body, &Dest::Discard, &mut body_out)?;
                Expr::bool(true)
            }
        };

        let label = self.loops.pop().unwrap().label;
        out.push(
            StmtKind::While {
                label,
                cond,
                body: body_out,
            }
            .at(span),
        );
        Ok(())
    }

    /// Recognize the `for` desugaring (ADR 0025):
    ///
    /// ```text
    /// match IntoIterator::into_iter(head) {
    ///     mut iter => loop {
    ///         match Iterator::next(&mut iter) { None => break, Some(pat) => body }
    ///     }
    /// }
    /// ```
    fn as_for(&self, e: ExprId) -> Option<ForLoop<'a, 'tcx>> {
        let thir: &'a Thir<'tcx> = self.thir;
        let is_call_to = |e: ExprId, item: LangItem| match thir[strip(thir, e)].kind {
            ExprKind::Call { fun, ref args, .. } => {
                matches!(thir[strip(thir, fun)].ty.kind(), &ty::FnDef(d, _) if self.tcx.is_lang_item(d, item))
                    .then(|| args[0])
            }
            _ => None,
        };
        let ExprKind::Match {
            scrutinee, ref arms, ..
        } = thir[strip(thir, e)].kind
        else {
            return None;
        };
        let head = is_call_to(scrutinee, LangItem::IntoIterIntoIter)?;
        let [arm] = &arms[..] else { return None };
        let ExprKind::Scope {
            value,
            region_scope,
            hir_id,
        } = thir[thir[*arm].body].kind
        else {
            return None;
        };
        let ExprKind::Loop { body } = thir[value].kind else {
            return None;
        };
        let ExprKind::Block { block } = thir[strip(thir, body)].kind else {
            return None;
        };
        let ([stmt], None) = (&*thir[block].stmts, thir[block].expr) else {
            return None;
        };
        let thir::StmtKind::Expr { expr, .. } = thir[*stmt].kind else {
            return None;
        };
        let ExprKind::Match {
            scrutinee: next,
            ref arms,
            ..
        } = thir[strip(thir, expr)].kind
        else {
            return None;
        };
        is_call_to(next, LangItem::IteratorNext)?;
        let some = arms.iter().find_map(|&a| match &thir[a].pattern.kind {
            PatKind::Variant { subpatterns, .. } if subpatterns.len() == 1 => {
                Some((&subpatterns[0].pattern, thir[a].body))
            }
            _ => None,
        })?;
        Some(ForLoop {
            head,
            pat: some.0,
            body: some.1,
            scope: region_scope,
            hir_id,
        })
    }

    /// Recognize `.await`'s desugaring, and return what's awaited (ADR 0029):
    ///
    /// ```text
    /// match IntoFuture::into_future(e) {
    ///     mut __awaitee => loop { match Future::poll(..) { Ready(r) => break r, Pending => {} } yield }
    /// }
    /// ```
    fn as_await(&self, e: ExprId) -> Option<ExprId> {
        let thir = self.thir;
        let ExprKind::Match {
            scrutinee, ref arms, ..
        } = thir[strip(thir, e)].kind
        else {
            return None;
        };
        let ExprKind::Call { fun, ref args, .. } = thir[strip(thir, scrutinee)].kind else {
            return None;
        };
        let &ty::FnDef(into_future, _) = thir[strip(thir, fun)].ty.kind() else {
            return None;
        };
        let [arm] = &arms[..] else { return None };
        let is_loop = matches!(thir[strip(thir, thir[*arm].body)].kind, ExprKind::Loop { .. })
            || matches!(thir[thir[*arm].body].kind, ExprKind::Scope { value, .. } if matches!(thir[value].kind, ExprKind::Loop { .. }));
        (self.tcx.is_lang_item(into_future, LangItem::IntoFutureIntoFuture) && is_loop).then(|| args[0])
    }

    /// `for x in &v` is `for (const x of v)`; `for i in a..b` is
    /// `for (let i = a; i < b; i++)`.
    fn lower_for(&mut self, f: ForLoop<'a, 'tcx>, span: js::Span, out: &mut Vec<Stmt>) -> R<()> {
        let label_base = match self.tcx.hir_expect_expr(f.hir_id).kind {
            hir::ExprKind::Loop(_, Some(label), ..) => label.ident.name.as_str().trim_start_matches('\'').to_string(),
            _ => "loop".to_string(),
        };
        let head_ty = self.reveal(self.thir[f.head].ty);
        let head_span = self.thir[f.head].span;
        let inclusive = self.inclusive_range(f.head);
        let range = self.is_lang_adt(head_ty, LangItem::Range) || inclusive.is_some();

        // What to loop over: a range's bounds, or a sequence.
        let (iterable, start_end) = if range {
            let (start, end) = match inclusive {
                Some(bounds) => bounds,
                None => {
                    let ExprKind::Adt(ref adt) = self.thir[self.strip(f.head)].kind else {
                        return Err(self.unsupported(head_span, "this range"));
                    };
                    let bound = |i: usize| {
                        adt.fields
                            .iter()
                            .find(|field| field.name.as_usize() == i)
                            .map(|field| field.expr)
                    };
                    let (Some(start), Some(end)) = (bound(0), bound(1)) else {
                        unreachable!("a range has a start and an end")
                    };
                    (start, end)
                }
            };
            self.num(self.thir[start].ty, head_span)?;
            let [start_js, end_js] = self.operands(&[start, end], out)?.try_into().ok().unwrap();
            // Rust works out the end once; JS would test it again each time round.
            let end_js = if end_js.is_constant() || self.stable_place(end).is_some() {
                end_js
            } else {
                let name = self.fresh("end");
                out.push(StmtKind::Const(name.clone(), end_js).at(span));
                Expr::var(&name)
            };
            (None, Some((start_js, end_js)))
        } else {
            let peeled = head_ty.peel_refs();
            let sequence = peeled.is_array()
                || peeled.is_slice()
                || self.is_vec_like(peeled)
                || self.is_std_adt(peeled, Symbol::intern("SliceIter"))
                || self.is_str_split(peeled)
                || self.is_array_iter(peeled)
                || self.is_lazy_iter(peeled)
                || self.is_map(peeled)
                // A generic one, an array or a JS iterator: `for .. of` takes either (ADR 0061).
                || self.bounded_by(peeled, sym::IntoIterator)
                || matches!(self.thir[self.strip(f.head)].kind, ExprKind::Call { fun, .. } if self.std_fn(fun) == Some(Std::Same));
            if !sequence {
                return Err(self.unsupported(head_span, &format!("iterating over `{head_ty}`")));
            }
            let head = self.expr(f.head, out)?;
            let head = self.in_order_of(head, head_ty, head_span)?;
            (Some(self.iter_source(head, head_ty, head_span)?), None)
        };

        // The loop variable: the pattern's own name if it's a plain
        // immutable binding, a JS pattern for a tuple's or a struct's parts,
        // else a fresh one that the body takes apart.
        let mut body = Vec::new();
        let mut mutable = false;
        // `for &x in &v`: a reference is the value (ADR 0023).
        let pat = without_refs(f.pat);
        let name = match &pat.kind {
            PatKind::Binding {
                name,
                var,
                mode,
                subpattern: None,
                ty,
                ..
            } if mode.1 == Mutability::Not && mode.0 == ByRef::No && !self.contains_mutated(*ty) => {
                self.check_value_ty(*ty, f.pat.span)?;
                js::Pattern::Name(self.bind(*var, name.as_str(), false))
            }
            _ if let Some((pattern, is_mut)) = self.js_pattern(pat) => {
                mutable = is_mut;
                pattern
            }
            _ => {
                let name = self.fresh(if range { "i" } else { "item" });
                self.destructure(f.pat, Expr::var(&name), true, &mut body)?;
                js::Pattern::Name(name)
            }
        };
        // `for (i, x) in v.iter().enumerate()` of an array: its `entries()`.
        let iterable = iterable.map(|it| match it.kind {
            js::ExprKind::Call(ref callee, ref args)
                if matches!(args.as_slice(), [f] if is_enumerate_pair(f))
                    && let js::ExprKind::Member(ref items, ref method) = callee.kind
                    && method == "map"
                    && !self.is_lazy_iter(head_ty) =>
            {
                Expr::call(Expr::member((**items).clone(), "entries"), vec![])
            }
            _ => it,
        });

        self.loops.push(Loop {
            scope: f.scope,
            label_base,
            label: None,
            dest: Dest::Discard,
        });
        self.stmt(f.body, &Dest::Discard, &mut body)?;
        let label = self.loops.pop().unwrap().label;
        out.push(
            match (iterable, start_end) {
                (Some(iterable), _) => StmtKind::ForOf {
                    label,
                    pattern: name,
                    mutable,
                    iterable,
                    body,
                },
                (None, Some((start, end))) => {
                    let js::Pattern::Name(name) = name else {
                        unreachable!("a range's item is a number, bound by name")
                    };
                    // `1..=n` includes its end.
                    let op = if inclusive.is_some() { Op::Le } else { Op::Lt };
                    let test = Expr::bin(op, Expr::var(&name), end);
                    StmtKind::For {
                        label,
                        name,
                        start,
                        test,
                        body,
                    }
                }
                (None, None) => unreachable!("a range or a sequence"),
            }
            .at(span),
        );
        Ok(())
    }

    /// Recognize the `while` desugaring; returns `(cond, body)`.
    fn as_while(&self, body: ExprId, scope: region::Scope) -> Option<(ExprId, ExprId)> {
        let ExprKind::Block { block } = self.thir[self.strip(body)].kind else {
            return None;
        };
        let block = &self.thir[block];
        let (true, Some(tail)) = (block.stmts.is_empty(), block.expr) else {
            return None;
        };
        let ExprKind::If {
            cond,
            then,
            else_opt: Some(els),
            ..
        } = self.thir[self.strip(tail)].kind
        else {
            return None;
        };
        let ExprKind::Block { block: els } = self.thir[self.strip(els)].kind else {
            return None;
        };
        let els = &self.thir[els];
        let ([stmt], None) = (&*els.stmts, els.expr) else {
            return None;
        };
        let thir::StmtKind::Expr { expr, .. } = self.thir[*stmt].kind else {
            return None;
        };
        let ExprKind::Break { label, value: None } = self.thir[self.strip(expr)].kind else {
            return None;
        };
        (label == scope && self.is_simple(cond)).then_some((cond, then))
    }

    fn lower_match(&mut self, scrutinee: ExprId, arms: &[ArmId], dest: &Dest, out: &mut Vec<Stmt>) -> R<()> {
        // A `fmt::Result` is nothing in JS (ADR 0054): there's no `Err` to match.
        if self.is_fmt_result(self.thir[scrutinee].ty) {
            return Err(self.unsupported(self.thir[scrutinee].span, "matching a `fmt::Result`"));
        }
        // Evaluate the scrutinee once, unless it's a place that can be
        // tested where it is.
        let (subject, stable) = self.subject(scrutinee, "match", out)?;

        let mut chain: Vec<(Option<Expr>, Vec<Stmt>, js::Span)> = Vec::new();
        for (i, &arm_id) in arms.iter().enumerate() {
            let arm = &self.thir[arm_id];
            let arm_span = self.js_span(arm.span);
            let pat_span = self.js_span(arm.pattern.span);
            let mut bindings = Vec::new();
            let mut test = self
                .pattern_test(&arm.pattern, &subject, &mut bindings)?
                .map(|t| t.or_at(pat_span));

            // A guard is tested before the arm's body, where the variables
            // with their own `const` are declared. Only named places work there.
            if arm.guard.is_some() && bindings.iter().any(|b| !stable || b.mutable) {
                return Err(self.unsupported(arm.pattern.span, "this binding in a guarded arm"));
            }
            let mut body = Vec::new();
            self.bind_all(bindings, stable, pat_span, &mut body);
            if let Some(guard) = arm.guard {
                if !self.is_simple(guard) {
                    return Err(self.unsupported(self.thir[guard].span, "this guard"));
                }
                let guard = self.expr(guard, out)?;
                test = Some(match test {
                    Some(t) => Expr::bin(Op::And, t, guard),
                    None => guard,
                });
            }
            // Rust checked the match is exhaustive, so if we reach the last
            // unguarded arm, it matches. No need to test it.
            if i == arms.len() - 1 && arm.guard.is_none() {
                test = None;
            }
            self.stmt(arm.body, dest, &mut body)?;
            let done = test.is_none();
            chain.push((test, body, arm_span));
            if done {
                break; // Later arms are unreachable.
            }
        }

        // Fold into `if (..) {..} else if (..) {..} else {..}`.
        let mut rest: Option<Vec<Stmt>> = None;
        for (test, body, span) in chain.into_iter().rev() {
            rest = match test {
                // An arm that does nothing, `Dot => {}`, before others:
                // `if (s !== "Dot") { .. }`, not `if (s === "Dot") {} else ..`.
                Some(t) if body.is_empty() && rest.is_some() => Some(vec![
                    StmtKind::If(std_impls::negate(t), rest.unwrap_or_default(), None).at(span),
                ]),
                Some(t) => Some(vec![StmtKind::If(t, body, rest).at(span)]),
                // A last arm that does nothing, `None => {}`: no `else {}`.
                None if body.is_empty() => None,
                None => Some(body),
            };
        }
        out.extend(rest.unwrap_or_default());
        Ok(())
    }

    /// An `if`'s condition as the parts joined by `&&`, when there are
    /// several and one is a `let`: a let chain (ADR 0048).
    fn let_chain(&self, cond: ExprId) -> Option<Vec<ExprId>> {
        fn parts(cx: &FnCx<'_, '_>, e: ExprId, found: &mut Vec<ExprId>) {
            let e = cx.strip(e);
            match cx.thir[e].kind {
                ExprKind::LogicalOp {
                    op: LogicalOp::And,
                    lhs,
                    rhs,
                } => {
                    parts(cx, lhs, found);
                    parts(cx, rhs, found);
                }
                _ => found.push(e),
            }
        }
        let mut found = Vec::new();
        parts(self, cond, &mut found);
        let has_let = found.iter().any(|&p| matches!(self.thir[p].kind, ExprKind::Let { .. }));
        (found.len() > 1 && has_let).then_some(found)
    }

    /// `if let Some(h) = half(n) && h > 2 && let Some(q) = f(h) { .. } else { .. }`.
    /// Each part runs only if the ones before it held, and may read what an
    /// earlier `let` bound. Parts that need no statements of their own join
    /// one test: `const h = half(n); if (h != null && h > 2) { .. }`. One that
    /// does, like a `let` of a call, opens an `if` inside. With more than one,
    /// the `else` follows them all in a labeled block, which the `then` leaves.
    fn lower_let_chain(
        &mut self,
        parts: Vec<ExprId>,
        then: ExprId,
        else_opt: Option<ExprId>,
        dest: &Dest,
        span: js::Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        // Each level: what runs before its test, its test, and what its body
        // starts with (a `let`'s bindings).
        let mut levels: Vec<(Vec<Stmt>, Vec<Expr>, Vec<Stmt>)> = vec![(Vec::new(), Vec::new(), Vec::new())];
        for part in parts {
            let (mut before, mut bindings) = (Vec::new(), Vec::new());
            let test = match self.thir[part].kind {
                ExprKind::Let { expr, ref pat } => self.if_let(expr, pat, &mut bindings, &mut before)?,
                _ => self.expr(part, &mut before)?,
            };
            let level = levels.last_mut().expect("a level");
            if level.1.is_empty() || (before.is_empty() && level.2.is_empty()) {
                level.0.extend(before);
                level.1.push(test);
                level.2.extend(bindings);
            } else {
                levels.push((before, vec![test], bindings));
            }
        }
        let mut then_out = Vec::new();
        self.stmt(then, dest, &mut then_out)?;
        let mut else_out = match else_opt {
            Some(els) => {
                let mut else_out = Vec::new();
                self.stmt(els, dest, &mut else_out)?;
                Some(else_out)
            }
            None => None,
        };
        let label = (levels.len() > 1 && else_out.is_some()).then(|| fresh_in(&mut self.labels, "chain"));
        let leaves = matches!(
            then_out.last().map(|s| &s.kind),
            Some(StmtKind::Return(_) | StmtKind::Throw(_) | StmtKind::Break(_) | StmtKind::Continue(_))
        );
        if let Some(label) = &label
            && !leaves
        {
            then_out.push(StmtKind::Break(Some(label.clone())).at(span));
        }
        // Built from the innermost level out; only a lone level has the `else`.
        let single = levels.len() == 1;
        let mut body = then_out;
        for (before, tests, bindings) in levels.into_iter().rev() {
            let test = tests
                .into_iter()
                .reduce(|a, b| Expr::bin(Op::And, a, b))
                .unwrap_or_else(|| Expr::bool(true));
            let mut inner = bindings;
            inner.extend(body);
            let els = if single { else_out.take() } else { None };
            body = before;
            body.push(StmtKind::If(test, inner, els).at(span));
        }
        match (label, else_out) {
            (Some(label), Some(else_out)) => {
                body.extend(else_out);
                out.push(StmtKind::Labeled(label, body).at(span));
            }
            _ => out.extend(body),
        }
        Ok(())
    }

    /// `if let pat = scrutinee`: the test, with the pattern's variables
    /// bound at the start of the `then` branch. `if let Some(el) = find()`
    /// keeps the value in a `const` named like the variable, which is then
    /// just that `const`: `const el = find(); if (el != null) { .. }`.
    fn if_let(&mut self, scrutinee: ExprId, pat: &Pat<'tcx>, then_out: &mut Vec<Stmt>, out: &mut Vec<Stmt>) -> R<Expr> {
        if let Some(test) = self.slot_binding(scrutinee, pat, out)? {
            return Ok(test);
        }
        let base = match &pat.kind {
            PatKind::Variant { subpatterns, .. } if subpatterns.len() == 1 => match &subpatterns[0].pattern.kind {
                PatKind::Binding { name, .. } => name.to_string(),
                _ => "value".to_string(),
            },
            _ => "value".to_string(),
        };
        let (subject, stable) = self.subject(scrutinee, &base, out)?;
        let mut bindings = Vec::new();
        let test = self.pattern_test(pat, &subject, &mut bindings)?;
        self.bind_all(bindings, stable, self.js_span(pat.span), then_out);
        Ok(test.unwrap_or_else(|| Expr::bool(true)))
    }

    /// `if let Some(n) = m.get_mut(&k)` of a map whose values are primitives:
    /// `let n = m.get(k)`, which a write through `n` puts back (`slot_write`).
    fn slot_binding(&mut self, scrutinee: ExprId, pat: &Pat<'tcx>, out: &mut Vec<Stmt>) -> R<Option<Expr>> {
        let ExprKind::Call { fun, ref args, .. } = self.thir[self.strip(scrutinee)].kind else {
            return Ok(None);
        };
        let PatKind::Variant { subpatterns, .. } = &pat.kind else {
            return Ok(None);
        };
        let [field] = subpatterns.as_slice() else {
            return Ok(None);
        };
        let PatKind::Binding {
            name,
            var,
            mode: BindingMode(ByRef::No, Mutability::Not),
            subpattern: None,
            ty,
            ..
        } = &field.pattern.kind
        else {
            return Ok(None);
        };
        let &ty::FnDef(get, _) = self.thir[self.strip(fun)].ty.kind() else {
            return Ok(None);
        };
        let slot = self.std_fn(fun) == Some(Std::Map(maps::MapOp::Get))
            && self.tcx.item_name(get).as_str() == "get_mut"
            && matches!(ty.kind(), ty::Ref(_, value, Mutability::Mut) if self.is_primitive_key(*value));
        if !slot {
            return Ok(None);
        }
        let args = args.clone();
        let [map, key]: [Expr; 2] = self.operands(&args, out)?.try_into().ok().expect("a map and a key");
        let map = if map.reads_same() {
            map
        } else {
            self.spill("map", map, out)
        };
        let key = if key.reads_same() {
            key
        } else {
            self.spill("key", key, out)
        };
        let name = self.bind(*var, name.as_str(), true);
        let there = Expr::call(Expr::member(map.clone(), "get"), vec![key.clone()]);
        out.push(StmtKind::Let(name.clone(), Some(there)).at(self.js_span(pat.span)));
        self.slots.insert(*var, (map, key));
        Ok(Some(Expr::bin(Op::LooseNe, Expr::var(&name), Expr::null())))
    }

    /// Is `lhs` `*n`, with `n` a `&mut` from `slot_binding`?
    fn slots_write(&self, lhs: ExprId) -> bool {
        matches!(self.thir[self.strip(lhs)].kind, ExprKind::Deref { arg }
            if matches!(self.thir[self.strip(arg)].kind, ExprKind::VarRef { id } if self.slots.contains_key(&id)))
    }

    /// `*n = v` or `*n += v` through a `&mut` from `slot_binding`: the copy,
    /// and the map's entry, written. `None` if `lhs` isn't one.
    fn slot_write(
        &mut self,
        lhs: ExprId,
        value: &dyn Fn(&mut Self, Expr) -> R<Expr>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Option<()>> {
        let ExprKind::Deref { arg } = self.thir[self.strip(lhs)].kind else {
            return Ok(None);
        };
        let ExprKind::VarRef { id } = self.thir[self.strip(arg)].kind else {
            return Ok(None);
        };
        let Some((map, key)) = self.slots.get(&id).cloned() else {
            return Ok(None);
        };
        let place = self.vars[&id].place.clone();
        let written = value(self, place.clone())?;
        let js_span = self.js_span(span);
        out.push(StmtKind::Assign(place.clone(), written).at(js_span));
        let set = Expr::call(Expr::member(map, "set"), vec![key, place]);
        out.push(StmtKind::Expr(set).at(js_span));
        Ok(Some(()))
    }

    /// The shape `as_matches` takes: `pat => true, _ => false`.
    fn is_matches(&self, arms: &[ArmId]) -> bool {
        let is_bool = |arm: ArmId, want: bool| matches!(self.thir[self.strip(self.thir[arm].body)].kind, ExprKind::Literal { lit, .. } if lit.node == LitKind::Bool(want));
        matches!(arms, &[first, rest] if is_bool(first, true) && is_bool(rest, false)
            && matches!(self.thir[rest].pattern.kind, PatKind::Wild) && self.thir[rest].guard.is_none())
    }

    /// `matches!(x, pat)`, or `match x { pat if guard => true, _ => false }`:
    /// just the test, `x.TAG === "Circle"`, when the pattern binds nothing
    /// the guard can't read where it is.
    fn as_matches(&mut self, scrutinee: ExprId, arms: &[ArmId], out: &mut Vec<Stmt>) -> R<Option<Expr>> {
        let is_bool = |arm: ArmId, want: bool| matches!(self.thir[self.strip(self.thir[arm].body)].kind, ExprKind::Literal { lit, .. } if lit.node == LitKind::Bool(want));
        let &[first, rest] = arms else { return Ok(None) };
        if !is_bool(first, true)
            || !is_bool(rest, false)
            || !matches!(self.thir[rest].pattern.kind, PatKind::Wild)
            || self.thir[rest].guard.is_some()
        {
            return Ok(None);
        }
        let (subject, stable) = self.subject(scrutinee, "match", out)?;
        let mut bindings = Vec::new();
        let test = self.pattern_test(&self.thir[first].pattern, &subject, &mut bindings)?;
        if !bindings.is_empty() && (!stable || bindings.iter().any(|b| b.mutable)) {
            return Err(self.unsupported(self.thir[first].pattern.span, "this binding in `matches!`"));
        }
        let span = self.js_span(self.thir[first].span);
        self.bind_all(bindings, stable, span, out);
        let guard = match self.thir[first].guard {
            Some(guard) if self.is_simple(guard) => Some(self.expr(guard, out)?),
            Some(guard) => return Err(self.unsupported(self.thir[guard].span, "this guard")),
            None => None,
        };
        let test = match (test, guard) {
            (Some(t), Some(g)) => Expr::bin(Op::And, t, g),
            (Some(t), None) | (None, Some(t)) => t,
            (None, None) => Expr::bool(true),
        };
        Ok(Some(test))
    }

    /// A JS boolean test for "`subject` matches `pat`" (`None`: always matches).
    fn pattern_test(&mut self, pat: &Pat<'tcx>, subject: &Expr, bindings: &mut Vec<Binding<'tcx>>) -> R<Option<Expr>> {
        match &pat.kind {
            PatKind::Wild => Ok(None),
            PatKind::Binding {
                name,
                var,
                mode,
                subpattern,
                ty,
                ..
            } => {
                // A `ref mut` binding names the place it matched (`bind_all`),
                // so even a number's can be written through.
                let by_ref_mut = matches!(mode.0, ByRef::Yes(_, Mutability::Mut));
                if !by_ref_mut {
                    self.check_by_value(*mode, *ty, pat.span)?;
                }
                bindings.push(Binding {
                    var: *var,
                    name: name.to_string(),
                    mutable: mode.1 == Mutability::Mut,
                    by_ref_mut,
                    place: subject.clone(),
                    ty: *ty,
                });
                // `x @ 1..=9`: bound, and tested by what's after the `@`.
                match subpattern {
                    Some(inner) => self.pattern_test(inner, subject, bindings),
                    None => Ok(None),
                }
            }
            PatKind::Constant { value } => {
                let value = self.const_value(*value, pat.span)?;
                Ok(Some(Expr::bin(Op::Eq, subject.clone(), value)))
            }
            // `1..=9`, `i32::MIN..0`, `'a'..='z'`: between its bounds, as `<`
            // compares numbers, and `char`s by their UTF-16 units (ADR 0034).
            PatKind::Range(range) => {
                let bound = |boundary: &PatRangeBoundary<'tcx>| match *boundary {
                    PatRangeBoundary::Finite(valtree) => {
                        let value = ty::Value { ty: range.ty, valtree };
                        const_js(self.tcx, value)
                            .map(Some)
                            .ok_or_else(|| self.unsupported(pat.span, "this range"))
                    }
                    PatRangeBoundary::NegInfinity | PatRangeBoundary::PosInfinity => Ok(None),
                };
                let (mut lo, mut hi) = (bound(&range.lo)?, bound(&range.hi)?);
                let below = if range.end == RangeEnd::Included {
                    Op::Le
                } else {
                    Op::Lt
                };
                // A bound at the type's own end always holds: `n >= 0` of a `u32`.
                if let Some(num) = Num::of(range.ty).filter(|&n| n != Num::F64) {
                    let (min, max) = num.range();
                    lo = lo.filter(|lo| lo.as_int() != Some(min));
                    hi = hi.filter(|hi| !(range.end == RangeEnd::Included && hi.as_int() == Some(max)));
                }
                let tests: Vec<Expr> = lo
                    .map(|lo| Expr::bin(Op::Ge, subject.clone(), lo))
                    .into_iter()
                    .chain(hi.map(|hi| Expr::bin(below, subject.clone(), hi)))
                    .collect();
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::And, a, b)))
            }
            // `Some(p)`: not `null` or `undefined`, and the value itself matches `p`.
            // A constant needs no `!= null`: `o === 0` already says it. So does
            // one through a reference, like every string literal: `o === "a"`.
            PatKind::Variant {
                adt_def,
                variant_index,
                subpatterns,
                ..
            } if self.tcx.is_lang_item(adt_def.did(), LangItem::Option) => {
                let Some(field) = subpatterns.first() else {
                    return Ok(Some(Expr::bin(Op::LooseEq, subject.clone(), Expr::null())));
                };
                debug_assert!(
                    self.tcx
                        .is_lang_item(adt_def.variant(*variant_index).def_id, LangItem::OptionSome)
                );
                // A generic `T`'s value may be boxed (ADR 0051): the pattern is on what's inside.
                let value = match self.option_of(pat.ty) {
                    Some(inner) if self.boxed_payload(inner) => self.some_value(subject.clone()),
                    _ => subject.clone(),
                };
                let inner = self.pattern_test(&field.pattern, &value, bindings)?;
                let present = Expr::bin(Op::LooseNe, subject.clone(), Expr::null());
                let mut value = &field.pattern;
                while let PatKind::Deref { subpattern, .. } = &value.kind {
                    value = subpattern;
                }
                Ok(Some(match inner {
                    // `undefined >= 1` is false too.
                    Some(test) if matches!(value.kind, PatKind::Constant { .. } | PatKind::Range(_)) => test,
                    Some(test) => Expr::bin(Op::And, present, test),
                    None => present,
                }))
            }
            // A variant (ADR 0013, 0033): its name, or its `TAG`, then its fields.
            // An enum with one variant needs no test.
            PatKind::Variant {
                adt_def,
                variant_index,
                subpatterns,
                ..
            } => {
                let variant = adt_def.variant(*variant_index);
                if let Some(n) = ordering_value(self.tcx, adt_def.did(), variant.name) {
                    return Ok(Some(Expr::bin(Op::Eq, subject.clone(), Expr::int(n))));
                }
                let name = Expr::str(bindings::variant_name(self.tcx, variant));
                let mut tests = Vec::new();
                if adt_def.variants().len() > 1 {
                    tests.push(match variant.fields.is_empty() {
                        true => Expr::bin(Op::Eq, subject.clone(), name),
                        false => Expr::bin(Op::Eq, Expr::member(subject.clone(), "TAG"), name),
                    });
                }
                for field in subpatterns {
                    let part = Expr::member(
                        subject.clone(),
                        variant_field(self.tcx, variant, field.field.as_usize()),
                    );
                    tests.extend(self.pattern_test(&field.pattern, &part, bindings)?);
                }
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::And, a, b)))
            }
            // Matching through a reference: the reference is the value (ADR 0023).
            PatKind::Deref { subpattern, .. } => self.pattern_test(subpattern, subject, bindings),
            // A struct or tuple: every field must match.
            PatKind::Leaf { subpatterns } => {
                let mut tests = Vec::new();
                for field in subpatterns {
                    let part = self.project(subject.clone(), pat.ty, field.field.as_usize());
                    tests.extend(self.pattern_test(&field.pattern, &part, bindings)?);
                }
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::And, a, b)))
            }
            PatKind::Or { pats } => {
                let before = bindings.len();
                let mut tests = Vec::new();
                for p in pats {
                    match self.pattern_test(p, subject, bindings)? {
                        Some(t) => tests.push(t),
                        None => return Ok(None),
                    }
                }
                if bindings.len() != before {
                    return Err(self.unsupported(pat.span, "bindings inside `|` patterns"));
                }
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::Or, a, b)))
            }
            _ => Err(self.unsupported(pat.span, "this pattern")),
        }
    }

    // ── Expression mode ─────────────────────────────────────────────────

    /// Lower `e` to a JS expression. Any statements it needs first (a
    /// block's `let`s, a `match` computing a temporary) are pushed to `out`.
    ///
    /// The result carries `e`'s span, unless a more precise one was set
    /// deeper down (a `Scope` passes its inner expression through, say).
    fn expr(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let span = self.js_span(self.thir[e].span);
        Ok(self.expr_inner(e, out)?.or_at(span))
    }

    fn expr_inner(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let expr = &self.thir[e];
        let span = expr.span;
        let js_span = self.js_span(span);
        let ty = expr.ty;
        match expr.kind {
            ExprKind::Scope { value, .. } if !matches!(self.thir[value].kind, ExprKind::Loop { .. }) => {
                self.expr(value, out)
            }
            ExprKind::Use { source }
            | ExprKind::ValueTypeAscription { source, .. }
            | ExprKind::PlaceTypeAscription { source, .. } => self.expr(source, out),
            ExprKind::Block { .. } if let Some(f) = self.as_format_args(e) => self.lower_format_args(f, span, out),
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
            ExprKind::Borrow {
                borrow_kind: BorrowKind::Shared,
                arg,
            } => match self.place(arg) {
                Some((place, _)) => Ok(place),
                None => self.referent(arg, out),
            },
            // `&mut` to a JS object is the object (ADR 0025).
            ExprKind::Borrow {
                borrow_kind: BorrowKind::Mut { .. },
                arg,
            } if self.is_object(self.thir[arg].ty) => match self.place(arg) {
                Some((place, _)) => Ok(place),
                None => self.referent(arg, out),
            },
            ExprKind::Borrow { arg, .. } => {
                Err(self.unsupported(span, &format!("`&mut` to a `{}`", self.thir[arg].ty)))
            }
            ExprKind::Array { ref fields } => Ok(Expr::array(self.operands(fields, out)?)),
            ExprKind::Index { lhs, index } => {
                let values = self.indexed(lhs, index, out)?;
                self.runtime.insert(Helper::Index);
                Ok(self.copy_if_needed(Expr::call(Expr::var("$index"), values), ty))
            }
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
            // A function as a value, `component(Card, props)`: its JS name.
            ExprKind::ZstLiteral { .. }
                if let &ty::FnDef(def_id, args) = ty.kind()
                    && self.krate.fns.contains_key(&def_id) =>
            {
                if self.tcx.trait_of_assoc(def_id).is_some() {
                    let count = self
                        .tcx
                        .fn_sig(def_id)
                        .instantiate(self.tcx, args)
                        .skip_binder()
                        .inputs()
                        .len();
                    let params: Vec<String> = (0..count).map(|i| self.fresh(&format!("arg{i}"))).collect();
                    let values = params.iter().map(|name| Expr::var(name)).collect();
                    let call = self
                        .trait_call(def_id, args, values, span, out)?
                        .ok_or_else(|| self.unsupported(span, "this trait function value"))?;
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
            ExprKind::ZstLiteral { .. } if let Some(Std::MaxOf(max)) = self.std_fn(e) => {
                self.runtime.insert(if max { Helper::F64Max } else { Helper::F64Min });
                Ok(Expr::var(if max { "$f64Max" } else { "$f64Min" }))
            }
            ExprKind::Closure(ref closure) => self.closure(closure, out),
            ExprKind::Tuple { ref fields } if fields.is_empty() => Ok(Expr::undefined()),
            ExprKind::Tuple { ref fields } => Ok(Expr::array(self.operands(fields, out)?)),
            ExprKind::Adt(ref adt) => self.adt(adt, ty, span, out),
            ExprKind::Binary { op, lhs, rhs } => {
                let [l, r] = self.operands(&[lhs, rhs], out)?.try_into().ok().unwrap();
                self.binary(op, l, r, self.known_int(rhs), self.thir[lhs].ty, span)
            }
            ExprKind::LogicalOp { op, lhs, rhs } => {
                let l = self.expr(lhs, out)?;
                let js_op = match op {
                    LogicalOp::And => Op::And,
                    LogicalOp::Or => Op::Or,
                };
                if self.is_simple(rhs) {
                    let r = self.expr(rhs, out)?;
                    return Ok(Expr::bin(js_op, l, r));
                }
                // `a && { .. }`: only run the right side's statements if needed.
                let tmp = self.fresh("tmp");
                out.push(StmtKind::Let(tmp.clone(), Some(l)).at(js_span));
                let mut rhs_out = Vec::new();
                self.stmt(rhs, &Dest::Assign(tmp.clone()), &mut rhs_out)?;
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
            ExprKind::Call { fun, ref args, .. } => self.call(fun, args, span, out),
            ExprKind::NamedConst { def_id, args, .. } => self.named_const(def_id, args, ty, span),
            ExprKind::Match { .. } if let Some(awaited) = self.as_await(e) => {
                Ok(Expr::await_(self.expr(awaited, out)?))
            }
            ExprKind::Match { .. } if let Some(tried) = self.as_question(e) => self.question(e, tried, None, out),
            ExprKind::Match {
                scrutinee, ref arms, ..
            } if let Some(test) = self.as_matches(scrutinee, arms, out)? => Ok(test),
            ExprKind::If {
                cond,
                then,
                else_opt: Some(els),
                ..
            } if self.is_simple(then) && self.is_simple(els) && self.let_chain(cond).is_none() => {
                let c = self.expr(cond, out)?;
                let t = self.expr(then, out)?;
                let f = self.expr(els, out)?;
                Ok(Expr::cond(c, t, f))
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

    /// Lower operands left to right. If a later operand needs statements,
    /// earlier ones are saved in temporaries first, so Rust's evaluation
    /// order is kept.
    fn operands(&mut self, list: &[ExprId], out: &mut Vec<Stmt>) -> R<Vec<Expr>> {
        let last_complex = list.iter().rposition(|&e| !self.is_simple(e));
        let mut values = Vec::new();
        for (i, &e) in list.iter().enumerate() {
            let v = self.expr(e, out)?;
            // Constants, and places that can't change, read the same later.
            // So are places that are borrowed: nothing can change or reassign
            // them until the call, not even the later operands (`v.push(f(v.len()))`
            // is a borrow error, and `&mut v` a two-phase borrow).
            let borrowed =
                matches!(self.thir[self.strip(e)].kind, ExprKind::Borrow { arg, .. } if self.place(arg).is_some());
            let settled = v.is_constant()
                || borrowed
                || self.stable_place(self.strip_refs(e)).is_some()
                || self.ref_place(e).is_some_and(|(_, mutable)| !mutable);
            if last_complex.is_some_and(|k| i < k) && !settled {
                let tmp = self.fresh("tmp");
                let span = v.span;
                out.push(StmtKind::Const(tmp.clone(), v).at(span));
                values.push(Expr::var(&tmp));
            } else {
                values.push(v);
            }
        }
        Ok(values)
    }

    /// Does calling `fun` become an assignment statement?
    fn is_assignment_call(&self, fun: ExprId) -> bool {
        if matches!(
            self.std_fn(fun),
            Some(Std::CellSet | Std::Clear | Std::Panic | Std::PanicFmt | Std::PushStr | Std::AssignOperator(_))
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
                self.as_await(e).is_none() && self.as_question(e).is_none() && !self.is_matches(arms)
            }
            ExprKind::If { .. } | ExprKind::Block { .. } | ExprKind::Loop { .. } => true,
            _ => false,
        }
    }

    /// Can `e` become a JS expression with no statements before it?
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
            ExprKind::Match { .. } if let Some(awaited) = self.as_await(e) => self.is_simple(awaited),
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

    // ── Operators ───────────────────────────────────────────────────────

    /// `known` is `r`'s value, when rustc knows it and the JS doesn't show
    /// it: a named `const` (ADR 0031).
    fn binary(&mut self, op: BinOp, l: Expr, r: Expr, known: Option<i128>, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let comparison = match op {
            BinOp::Eq => Some(Op::Eq),
            BinOp::Ne => Some(Op::Ne),
            BinOp::Lt => Some(Op::Lt),
            BinOp::Le => Some(Op::Le),
            BinOp::Gt => Some(Op::Gt),
            BinOp::Ge => Some(Op::Ge),
            _ => None,
        };
        if let Some(js_op) = comparison {
            return Ok(Expr::bin(js_op, l, r));
        }

        if ty.is_bool() {
            // `&`, `|`, `^` on bools evaluate both sides and give a bool.
            return match op {
                BinOp::BitXor => Ok(Expr::bin(Op::Ne, l, r)),
                BinOp::BitAnd => Ok(Expr::unary(
                    UnaryOp::Not,
                    Expr::unary(UnaryOp::Not, Expr::bin(Op::BitAnd, l, r)),
                )),
                BinOp::BitOr => Ok(Expr::unary(
                    UnaryOp::Not,
                    Expr::unary(UnaryOp::Not, Expr::bin(Op::BitOr, l, r)),
                )),
                _ => Err(self.unsupported(span, "this operator on `bool`")),
            };
        }

        let num = self.num(ty, span)?;
        if num == Num::F64 {
            let js_op = match op {
                BinOp::Add => Op::Add,
                BinOp::Sub => Op::Sub,
                BinOp::Mul => Op::Mul,
                BinOp::Div => Op::Div,
                BinOp::Rem => Op::Rem,
                _ => return Err(self.unsupported(span, "this operator on `f64`")),
            };
            return Ok(Expr::bin(js_op, l, r));
        }

        // Integers: compute exactly in JS, then wrap back into range.
        Ok(match op {
            BinOp::Add => num.wrap(Expr::bin(Op::Add, l, r)),
            BinOp::Sub => num.wrap(Expr::bin(Op::Sub, l, r)),
            // A 32-bit product can exceed 2^53 and lose bits; `Math.imul` can't.
            BinOp::Mul if num.bits() == 32 => {
                let product = Expr::call(Expr::member(Expr::var("Math"), "imul"), vec![l, r]);
                if num.signed() { product } else { num.wrap(product) }
            }
            BinOp::Mul => num.wrap(Expr::bin(Op::Mul, l, r)),
            BinOp::Div | BinOp::Rem => {
                let (js_op, helper, name) = match op {
                    BinOp::Div => (Op::Div, Helper::Div, "$div"),
                    _ => (Op::Rem, Helper::Rem, "$rem"),
                };
                // A literal divisor that can't panic stays inline: `a / 3 | 0`.
                let safe = known
                    .or_else(|| r.as_int())
                    .is_some_and(|d| d != 0 && !(num.signed() && d == -1));
                let quotient = if safe {
                    Expr::bin(js_op, l, r)
                } else {
                    self.runtime.insert(helper);
                    let mut args = vec![l, r];
                    if num.signed() {
                        args.push(Expr::int(num.range().0));
                    }
                    Expr::call(Expr::var(name), args)
                };
                // The remainder of in-range integers is already in range.
                if op == BinOp::Rem && safe {
                    quotient
                } else {
                    num.wrap(quotient)
                }
            }
            BinOp::BitAnd => self.bitwise(Op::BitAnd, l, r, num),
            BinOp::BitOr => self.bitwise(Op::BitOr, l, r, num),
            BinOp::BitXor => self.bitwise(Op::BitXor, l, r, num),
            // Rust (without overflow checks) masks the shift amount to the
            // type's width. JS masks to 32, which is only right for 32 bits.
            BinOp::Shl if num.bits() == 32 => num.wrap(Expr::bin(Op::Shl, l, r)),
            BinOp::Shl => num.wrap(Expr::bin(Op::Shl, l, mask_shift(r, num))),
            BinOp::Shr => {
                let js_op = if num.signed() { Op::Shr } else { Op::UShr };
                let r = if num.bits() == 32 { r } else { mask_shift(r, num) };
                Expr::bin(js_op, l, r)
            }
            _ => return Err(self.unsupported(span, "this operator")),
        })
    }

    fn bitwise(&self, op: Op, l: Expr, r: Expr, num: Num) -> Expr {
        // JS bitwise ops return signed 32-bit results; only u32 needs fixing.
        let e = Expr::bin(op, l, r);
        if num == Num::U32 { num.wrap(e) } else { e }
    }

    fn unary(&mut self, op: UnOp, a: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        match op {
            UnOp::Not if ty.is_bool() => Ok(Expr::unary(UnaryOp::Not, a)),
            UnOp::Not => {
                let num = self.num(ty, span)?;
                if num == Num::F64 {
                    return Err(self.unsupported(span, "`!` on `f64`"));
                }
                let e = Expr::unary(UnaryOp::BitNot, a);
                Ok(if num.signed() { e } else { num.wrap(e) })
            }
            UnOp::Neg => {
                let num = self.num(ty, span)?;
                // `-x` of a literal is just a negative literal.
                if let Some(n) = a.as_int() {
                    return Ok(Expr::int(-n));
                }
                Ok(num.wrap(Expr::unary(UnaryOp::Neg, a)))
            }
            UnOp::PtrMetadata => Err(self.unsupported(span, "pointer metadata")),
        }
    }

    fn cast(&mut self, v: Expr, from: Ty<'tcx>, to: Ty<'tcx>, span: Span) -> R<Expr> {
        let target = self.num(to, span)?;
        if from.is_bool() && target != Num::F64 {
            return Ok(Expr::cond(v, Expr::num(1), Expr::num(0)));
        }
        // A `char` is its code point (ADR 0063), and a `u8` as a `char` its
        // character.
        if from.is_char() {
            let code = Expr::call(Expr::member(v, "codePointAt"), vec![Expr::int(0)]);
            let (lo, hi) = target.range();
            return Ok(if target == Num::F64 || (lo <= 0 && hi >= 0x10ffff) {
                code
            } else {
                target.wrap(code)
            });
        }
        // An `Ordering` is -1, 0 or 1 already (ADR 0036).
        let (v, source) = if self.is_lang_adt(from, LangItem::OrderingEnum) {
            (v, Num::I8)
        } else if let ty::Adt(adt, _) = from.kind()
            && is_fieldless_enum(*adt)
        {
            // A fieldless enum is its variant's name (ADR 0013): its
            // discriminant, `["Red", "Green"].indexOf(color)` when they count
            // up from 0, and looked up by name otherwise.
            let discriminants: Vec<(String, i128)> = adt
                .discriminants(self.tcx)
                .map(|(index, d)| {
                    let variant = adt.variant(index);
                    let value = d.val as i128;
                    let value = if d.ty.is_signed() {
                        let bits = d.ty.primitive_size(self.tcx).bits();
                        (value << (128 - bits)) >> (128 - bits)
                    } else {
                        value
                    };
                    (bindings::variant_name(self.tcx, variant), value)
                })
                .collect();
            let counting = discriminants.iter().enumerate().all(|(i, &(_, d))| d == i as i128);
            // Each one fits the target type, so there's nothing to wrap.
            let (lo, hi) = target.range();
            let fits = target == Num::F64 || discriminants.iter().all(|&(_, d)| lo <= d && d <= hi);
            let value = if counting {
                let names = Expr::array(discriminants.into_iter().map(|(n, _)| Expr::str(n)).collect());
                Expr::call(Expr::member(names, "indexOf"), vec![v])
            } else {
                let table = Expr::object(
                    discriminants
                        .into_iter()
                        .map(|(n, d)| Prop::Field(n, Expr::int(d)))
                        .collect(),
                );
                Expr::index(table, v)
            };
            if fits {
                return Ok(value);
            }
            let repr = rustc_middle::ty::util::IntTypeExt::to_ty(&adt.repr().discr_type(), self.tcx);
            (value, Num::of(repr).unwrap_or(Num::I32))
        } else {
            (v, self.num(from, span)?)
        };
        match (source, target) {
            (Num::F64, Num::F64) => Ok(v),
            // `as` from float to int saturates; we don't do that yet.
            (Num::F64, _) => Err(self.unsupported(span, "casting `f64` to an integer")),
            // Every integer we support fits exactly in an f64.
            (_, Num::F64) => Ok(v),
            _ => {
                let (lo, hi) = source.range();
                let (tlo, thi) = target.range();
                Ok(if tlo <= lo && hi <= thi { v } else { target.wrap(v) })
            }
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
                self.num(ty, span)?;
                let n = n.get() as i128;
                Ok(Expr::int(if neg { -n } else { n }))
            }
            LitKind::Byte(b) => Ok(Expr::int(b.into())),
            LitKind::Float(sym, _) if Num::of(ty) == Some(Num::F64) => {
                let x: f64 = sym
                    .as_str()
                    .replace('_', "")
                    .parse()
                    .expect("rustc validated the literal");
                Ok(Expr::num(if neg { -x } else { x }))
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

    // ── Closures (ADR 0022) ─────────────────────────────────────────────

    /// A closure is an arrow function, lowered right where it's created.
    ///
    /// JS closures capture *variables*, which is what a Rust capture by
    /// reference means, and the borrow checker has made sure nothing else
    /// uses them meanwhile. A capture by value is a copy: for an immutable
    /// variable that's the same thing, so only mutable ones get a snapshot.
    fn closure(&mut self, closure: &thir::ClosureExpr<'tcx>, out: &mut Vec<Stmt>) -> R<Expr> {
        let body: &'a Body<'tcx> = self.krate.closures[&closure.closure_id];
        let mut shadowed = Vec::new();
        for &upvar in closure.upvars.iter() {
            if !self.needs_snapshot(upvar) {
                continue;
            }
            // Since Rust 2021 a closure may capture part of a variable
            // (`p.x`), so the snapshot stands for that place.
            let span = self.thir[upvar].span;
            let Some(path) = self.place_path(upvar) else {
                return Err(self.unsupported(span, "capturing this place by value"));
            };
            let value = self.read(upvar, out)?;
            // Named after what it copies, from the Rust name: `n` gives `n$1`.
            let base = match self.place(upvar) {
                Some((
                    Expr {
                        kind: js::ExprKind::Member(_, field),
                        ..
                    },
                    _,
                )) => field,
                Some((
                    Expr {
                        kind: js::ExprKind::Var(name),
                        ..
                    },
                    _,
                )) => name,
                _ => "capture".to_string(),
            };
            let name = self.fresh(base.split('$').next().unwrap_or_default());
            out.push(StmtKind::Let(name.clone(), Some(value)).at(self.js_span(span)));
            let snapshot = Var {
                place: Expr::var(&name),
                mutable: true,
                depth: self.loops.len(),
            };
            shadowed.push((path.clone(), self.captures.insert(path, snapshot)));
        }

        // Lower the body as if it were a function of its own, then come back.
        // Its names are its own: once it's lowered, a sibling closure or later
        // code may use them again (`v.some((x) => ..)`, `v.every((x) => ..)`).
        // Like a JS arrow's, they may reuse an outer name, `(count) => count + 1`,
        // unless the closure uses what that name holds: a capture.
        let mut inner = self.module_names.clone();
        let mut known = true;
        for &upvar in closure.upvars.iter() {
            match self.place(upvar) {
                Some((place, _)) => inner.extend(root_var(&place).map(str::to_string)),
                None => known = false,
            }
        }
        let thir = std::mem::replace(&mut self.thir, &body.thir);
        let loops = std::mem::take(&mut self.loops);
        let names = if known {
            std::mem::replace(&mut self.names, inner)
        } else {
            self.names.clone()
        };
        let mut stmts = Vec::new();
        // An `async` block takes no arguments, and runs as soon as it's
        // made: an async arrow, called right away (ADR 0029).
        let block = matches!(
            self.tcx.coroutine_kind(closure.closure_id),
            Some(CoroutineKind::Desugared(
                CoroutineDesugaring::Async,
                CoroutineSource::Block
            ))
        );
        // The first parameter is the closure itself, which JS doesn't need.
        let params = if block {
            Vec::new()
        } else {
            // JS ignores extra arguments, so `|_| ..` is `() => ..`.
            let mut params = &body.thir.params.raw[1..];
            while let [rest @ .., last] = params
                && last.pat.as_deref().is_some_and(|p| matches!(p.kind, PatKind::Wild))
            {
                params = rest;
            }
            self.lower_params(params, self.tcx.def_span(body.def_id), &mut stmts)?
        };
        let BodyTy::Fn(sig) = body.thir.body_type else {
            unreachable!("a closure body is a function")
        };
        let dest = if sig.output().is_unit() {
            Dest::Discard
        } else {
            Dest::Return
        };
        let is_async = if block {
            self.stmt(body.expr, &Dest::Return, &mut stmts)?;
            true
        } else {
            self.lower_body(body.expr, &dest, &mut stmts)?
        };
        self.thir = thir;
        self.loops = loops;
        self.names = names;
        for (path, previous) in shadowed {
            match previous {
                Some(var) => self.captures.insert(path, var),
                None => self.captures.remove(&path),
            };
        }
        Ok(match (block, is_async) {
            (true, _) => Expr::call(Expr::async_arrow(params, stmts), Vec::new()),
            (false, true) => Expr::async_arrow(params, stmts),
            (false, false) => Expr::arrow(params, stmts),
        })
    }

    /// Lower a function's or a closure's body. For an `async fn` or an
    /// `async` closure, that's the body of the coroutine it returns: in JS,
    /// an `async` function's body. Says whether it was async (ADR 0029).
    fn lower_body(&mut self, e: ExprId, dest: &Dest, out: &mut Vec<Stmt>) -> R<bool> {
        let coroutine = match self.thir[self.strip(e)].kind {
            ExprKind::Closure(ref closure)
                if matches!(
                    self.tcx.coroutine_kind(closure.closure_id),
                    Some(CoroutineKind::Desugared(
                        CoroutineDesugaring::Async,
                        CoroutineSource::Fn | CoroutineSource::Closure
                    ))
                ) =>
            {
                closure.closure_id
            }
            _ => {
                self.stmt(e, dest, out)?;
                return Ok(false);
            }
        };
        // Its captures are this function's parameters and variables, so no snapshots.
        let body: &'a Body<'tcx> = self.krate.closures[&coroutine];
        let thir = std::mem::replace(&mut self.thir, &body.thir);
        let lowered = self.stmt(body.expr, &Dest::Return, out);
        self.thir = thir;
        lowered.map(|()| true)
    }

    /// A place as a variable and a path of fields, like `p.x` as `(p, [0])`.
    fn place_path(&self, e: ExprId) -> Option<(LocalVarId, Vec<usize>)> {
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => Some((id, Vec::new())),
            ExprKind::Field { lhs, name, .. } => {
                let (id, mut path) = self.place_path(lhs)?;
                path.push(name.as_usize());
                Some((id, path))
            }
            _ => None,
        }
    }

    /// Does capturing `upvar` need a snapshot? Only a by-value capture of a
    /// mutable variable does, and not when the capture is the variable's
    /// only use, outside any loop the variable isn't also in.
    fn needs_snapshot(&self, upvar: ExprId) -> bool {
        let u = self.strip(upvar);
        if matches!(self.thir[u].kind, ExprKind::Borrow { .. }) {
            return false;
        }
        let Some(var) = self.root_var(u).and_then(|id| self.vars.get(&id)) else {
            return false;
        };
        if !var.mutable {
            return false;
        }
        let only_use = match self.thir[u].kind {
            ExprKind::VarRef { id } => {
                let uses = self
                    .thir
                    .exprs
                    .iter()
                    .filter(|e| matches!(e.kind, ExprKind::VarRef { id: i } if i == id));
                uses.count() == 1 && var.depth == self.loops.len()
            }
            _ => false,
        };
        !only_use
    }

    /// The variable a place starts from.
    fn root_var(&self, e: ExprId) -> Option<LocalVarId> {
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => Some(id),
            ExprKind::Field { lhs, .. } | ExprKind::Deref { arg: lhs } => self.root_var(lhs),
            _ => None,
        }
    }

    // ── Structs and tuples (ADR 0020) ───────────────────────────────────

    /// A struct literal: `{ x: 1, y: 2 }`, or `[1, 2]` for a tuple struct.
    fn adt(&mut self, adt: &thir::AdtExpr<'tcx>, ty: Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let variant = adt.adt_def.variant(adt.variant_index);
        // A `fmt::Result` is always `Ok`, and nothing (ADR 0054).
        if self.is_fmt_result(ty) {
            return match variant.name.as_str() {
                "Ok" => Ok(Expr::undefined()),
                _ => Err(self.unsupported(span, "a `fmt::Error`")),
            };
        }
        // `Some(x)` is `x`, and `None` is `undefined` (ADR 0030).
        if let Some(inner) = self.option_of(ty) {
            // `Some(x)` of a `()`, or an `Option`, would be `None` (ADR 0030):
            // checked where it's made, since a temporary has no type check of its own.
            if !adt.fields.is_empty() && self.can_be_nullish(inner) && !self.boxed_payload(inner) {
                return Err(self.unsupported(span, &format!("values of type `{ty}`")));
            }
            return match adt.fields.first() {
                // Of a generic `T`, which might look like `None` (ADR 0051).
                Some(field) if self.boxed_payload(inner) => {
                    let value = self.expr(field.expr, out)?;
                    Ok(self.some(value))
                }
                Some(field) => self.expr(field.expr, out),
                None => Ok(Expr::undefined()),
            };
        }
        // A variant without fields is its name (ADR 0013). One with fields is an
        // object tagged with it, `{ TAG: "Circle", _0: r }` (ADR 0033), built
        // below like a struct.
        if let Some(n) = ordering_value(self.tcx, adt.adt_def.did(), variant.name) {
            return Ok(Expr::int(n));
        }
        if adt.adt_def.is_enum() && variant.fields.is_empty() {
            return Ok(Expr::str(bindings::variant_name(self.tcx, variant)));
        }
        if adt.adt_def.is_union() {
            return Err(self.unsupported(span, "unions"));
        }
        // `struct Marker;` holds nothing, like `()`.
        if variant.ctor_kind() == Some(CtorKind::Const) {
            return Ok(Expr::undefined());
        }
        // `P { x, ..base }`: the fields not written come from `base`.
        let base = match &adt.base {
            AdtExprBase::None => None,
            AdtExprBase::Base(fru) => match self.place(fru.base) {
                Some((place, _)) => Some(place),
                None => return Err(self.unsupported(self.thir[fru.base].span, "`..` with this base")),
            },
            AdtExprBase::DefaultFields(_) => return Err(self.unsupported(span, "default field values")),
        };

        // Rust evaluates the fields in the order they're written. JS lists
        // them in declaration order, so every object of a type has the same
        // shape. If that reorders two calls, they go into `const`s first.
        let exprs: Vec<ExprId> = adt.fields.iter().map(|f| f.expr).collect();
        let mut values = self.operands(&exprs, out)?;
        let reordered = !adt.fields.is_sorted_by_key(|f| f.name);
        if reordered && values.iter().filter(|v| v.has_effects()).count() > 1 {
            for (field, value) in adt.fields.iter().zip(&mut values) {
                if value.has_effects() {
                    let name = self.fresh(variant.fields[field.name].name.as_str());
                    let v = std::mem::replace(value, Expr::var(&name));
                    let span = v.span;
                    out.push(StmtKind::Const(name, v).at(span));
                }
            }
        }
        let mut given: HashMap<usize, Expr> = adt.fields.iter().map(|f| f.name.as_usize()).zip(values).collect();

        let tag = adt.adt_def.is_enum().then(|| bindings::variant_name(self.tcx, variant));
        let shape = match tag {
            Some(_) => Shape::Object(self.variant_fields(variant, adt.args)),
            None => self.shape(ty),
        };
        let field_tys = match &shape {
            Shape::Object(fields) => fields.iter().map(|&(_, t)| t).collect(),
            Shape::Array(tys) => tys.clone(),
            Shape::Other => unreachable!("a struct with fields"),
        };
        let mut items = Vec::new();
        for (i, field_ty) in field_tys.into_iter().enumerate() {
            items.push(match (given.remove(&i), &base) {
                (Some(value), _) => value,
                (None, Some(base)) => self.copy_if_needed(self.project(base.clone(), ty, i), field_ty),
                (None, None) => unreachable!("rustc checked that every field is given"),
            });
        }
        Ok(match shape {
            Shape::Object(fields) => {
                let tag = tag.map(|name| Prop::Field("TAG".into(), Expr::str(name)));
                let fields = fields.into_iter().zip(items).map(|((name, _), v)| Prop::Field(name, v));
                Expr::object(tag.into_iter().chain(fields).collect())
            }
            _ => Expr::array(items),
        })
    }

    /// `e` as a place, a variable and some of its fields, without reading it.
    /// Also says whether that variable is mutable.
    fn place(&self, e: ExprId) -> Option<(Expr, bool)> {
        // Inside a closure, a place it captured by value is its snapshot.
        if !self.captures.is_empty()
            && let Some(var) = self.place_path(e).and_then(|path| self.captures.get(&path))
        {
            return Some((var.place.clone(), var.mutable));
        }
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => {
                let var = &self.vars[&id];
                Some((var.place.clone(), var.mutable))
            }
            ExprKind::Field { lhs, name, .. } => {
                let (base, mutable) = self.place(lhs)?;
                Some((self.project(base, self.thir[lhs].ty, name.as_usize()), mutable))
            }
            // A reference is the value it points to, so `*r` is where `r` is.
            // (A static is reached through a pointer to it.)
            ExprKind::Deref { arg }
                if matches!(self.thir[arg].ty.kind(), ty::Ref(..) | ty::RawPtr(..)) || self.thir[arg].ty.is_box() =>
            {
                self.place(arg).or_else(|| self.ref_place(arg))
            }
            // A JS global (ADR 0021).
            ExprKind::StaticRef { def_id, .. } if self.tcx.is_foreign_item(def_id) => {
                Some((self.js_ref(&js_name(self.tcx, def_id)), false))
            }
            _ => None,
        }
    }

    /// `e` as a place whose value can't change while it's still in scope, so
    /// a pattern's variables can just name parts of it.
    ///
    /// Its variable must be immutable. That's not enough on its own: `let mut
    /// s = r;` moves `r`, and then `s.origin.x = 0` changes the object `r`
    /// still names. So the variable must also be `Copy` (read, never moved:
    /// the read copies it if needed) or hold nothing changed in place.
    fn stable_place(&self, e: ExprId) -> Option<Expr> {
        let (place, mutable) = self.place(e)?;
        let mut root = self.strip(e);
        while let ExprKind::Field { lhs, .. } | ExprKind::Deref { arg: lhs } = self.thir[root].kind {
            root = self.strip(lhs);
        }
        let ty = self.thir[root].ty;
        let unchanging = self.is_copy(ty) || !self.contains_mutated(ty);
        (!mutable && unchanging).then_some(place)
    }

    /// Where a reference made by a call points: `c.borrow_mut()` points at
    /// the cell's `value`, and so does the guard's `deref_mut()` (ADR 0025).
    fn ref_place(&self, e: ExprId) -> Option<(Expr, bool)> {
        match self.thir[self.strip(e)].kind {
            ExprKind::Borrow { arg, .. } => self.place(arg).or_else(|| self.ref_place(arg)),
            ExprKind::Call { fun, ref args, .. } => match self.std_fn(fun)? {
                Std::Same => self.ref_place(args[0]),
                Std::Borrow => {
                    let (cell, _) = self.ref_place(args[0])?;
                    Some((Expr::member(cell, "value"), true))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// What a reference made with `&` or `&mut` refers to, which is the JS
    /// value itself: `&v[i]` is the element, not a copy of it.
    fn referent(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        match self.thir[self.strip(e)].kind {
            // `&*f()`: the reference `f` returned.
            ExprKind::Deref { arg } => self.expr(arg, out),
            ExprKind::Index { lhs, index } => {
                let values = self.indexed(lhs, index, out)?;
                self.runtime.insert(Helper::Index);
                Ok(Expr::call(Expr::var("$index"), values))
            }
            _ => self.expr(e, out),
        }
    }

    /// An array or slice and an index into it: the array itself, not a
    /// copy, since only the element is read or written.
    fn indexed(&mut self, items: ExprId, index: ExprId, out: &mut Vec<Stmt>) -> R<Vec<Expr>> {
        match self.place(items) {
            Some((place, _)) => Ok(vec![place, self.expr(index, out)?]),
            None => self.operands(&[items, index], out),
        }
    }

    /// An element of an array, a slice or a `Vec`, as its collection and
    /// its index: `a[i]`, or `*IndexMut::index_mut(&mut v, i)`.
    fn element(&self, e: ExprId) -> Option<(ExprId, ExprId)> {
        match self.thir[self.strip(e)].kind {
            ExprKind::Index { lhs, index } => Some((lhs, index)),
            ExprKind::Deref { arg } => match self.thir[self.strip(arg)].kind {
                ExprKind::Call { fun, ref args, .. } if self.std_fn(fun) == Some(Std::Index) => {
                    Some((args[0], args[1]))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// An element, or a field of one: what `element_target` writes.
    fn in_element(&self, e: ExprId) -> bool {
        self.element(e).is_some()
            || matches!(self.thir[self.strip(e)].kind, ExprKind::Field { lhs, .. } if self.in_element(lhs))
    }

    /// An element as the target of an assignment, `v[$at(v, i)]`, checked
    /// first since JS would make the array longer. A field of one is
    /// `$index(v, i).x`.
    fn element_target(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        if let Some((items, index)) = self.element(e) {
            let [items, index]: [Expr; 2] = self.indexed(items, index, out)?.try_into().ok().unwrap();
            // `items` is read twice.
            let items = if items.reads_same() {
                items
            } else {
                self.spill("items", items, out)
            };
            self.runtime.insert(Helper::At);
            return Ok(Expr::index(
                items.clone(),
                Expr::call(Expr::var("$at"), vec![items, index]),
            ));
        }
        match self.thir[self.strip(e)].kind {
            ExprKind::Field { lhs, name, .. } => {
                let base = match (self.place(lhs), self.element(lhs)) {
                    (Some((place, _)), _) => place,
                    (None, Some(_)) => self.referent(lhs, out)?,
                    (None, None) => self.element_target(lhs, out)?,
                };
                Ok(self.project(base, self.thir[lhs].ty, name.as_usize()))
            }
            _ => self.assignee(e),
        }
    }

    /// `a..=b`: `RangeInclusive::new(a, b)`, with its bounds.
    pub(super) fn inclusive_range(&self, e: ExprId) -> Option<(ExprId, ExprId)> {
        match self.thir[self.strip(e)].kind {
            ExprKind::Call { fun, ref args, .. } if matches!(self.thir[self.strip(fun)].ty.kind(), &ty::FnDef(d, _) if self.tcx.is_lang_item(d, LangItem::RangeInclusiveNew)) => {
                Some((args[0], args[1]))
            }
            _ => None,
        }
    }

    /// A local variable of this function, or a field of one: not reached
    /// through a reference, nor captured by a closure.
    fn is_local_place(&self, e: ExprId) -> bool {
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { .. } => true,
            ExprKind::Field { lhs, .. } => self.is_local_place(lhs),
            _ => false,
        }
    }

    /// The place an assignment writes to.
    fn assignee(&self, e: ExprId) -> R<Expr> {
        // `*r = v` with a `&mut` variable `r` would only rebind the JS variable.
        // One that names a place, as a `ref mut` binding does, writes it.
        let names_place = |arg: ExprId| match self.thir[self.strip(arg)].kind {
            ExprKind::VarRef { id } => self
                .vars
                .get(&id)
                .is_some_and(|v| matches!(v.place.kind, js::ExprKind::Member(..) | js::ExprKind::Index(..))),
            _ => false,
        };
        if let ExprKind::Deref { arg } = self.thir[self.strip(e)].kind
            && matches!(self.thir[arg].ty.kind(), ty::Ref(..))
            && matches!(
                self.thir[self.strip(arg)].kind,
                ExprKind::VarRef { .. } | ExprKind::Field { .. }
            )
            && !names_place(arg)
        {
            return Err(self.unsupported(self.thir[e].span, "assigning a whole value through a `&mut`"));
        }
        self.place(e)
            .map(|(place, _)| place)
            .ok_or_else(|| self.unsupported(self.thir[e].span, "assigning to this place"))
    }

    /// Read a variable or field's value.
    fn read(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let ty = self.thir[e].ty;
        if let Some((place, _)) = self.place(e) {
            return Ok(self.copy_if_needed(place, ty));
        }
        match self.thir[self.strip(e)].kind {
            // A field of a temporary, like `f().x`: nothing else can see the rest.
            ExprKind::Field { lhs, name, .. } => {
                // A field of an element, `v[i].x`, or of what a reference points
                // at, `f().unwrap().x`: that itself, and a copy of just the
                // field if it needs one.
                if self.element(lhs).is_some() || matches!(self.thir[self.strip(lhs)].kind, ExprKind::Deref { .. }) {
                    let base = self.referent(lhs, out)?;
                    let field = self.project(base, self.thir[lhs].ty, name.as_usize());
                    return Ok(self.copy_if_needed(field, ty));
                }
                let base = self.expr(lhs, out)?;
                Ok(self.project(base, self.thir[lhs].ty, name.as_usize()))
            }
            // `*f()`, including `Deref::deref` on a `String` or `Rc`: a
            // reference is its value. Reading a `Copy` one copies it, as
            // reading a place does: `*v.first().unwrap()` isn't `v[0]` itself.
            ExprKind::Deref { arg } => {
                let value = self.expr(arg, out)?;
                Ok(self.copy_if_needed(value, ty))
            }
            _ => Err(self.unsupported(self.thir[e].span, "reading this")),
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    /// A rustc span as byte offsets into the root file, for the source map.
    ///
    /// rustc numbers bytes across *all* loaded files, so subtract the root
    /// file's start. Code from a macro or desugaring maps to where it was
    /// written (`source_callsite`). Anything outside the root file (say, a
    /// `std` macro) gets no mapping.
    fn js_span(&self, span: Span) -> js::Span {
        let span = span.source_callsite();
        if span.lo() < self.file_start || span.hi() > self.file_end {
            return js::Span::NONE;
        }
        js::Span {
            lo: (span.lo() - self.file_start).0,
            hi: (span.hi() - self.file_start).0,
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

    fn loop_index(&self, label: region::Scope, span: Span) -> R<usize> {
        self.loops
            .iter()
            .rposition(|l| l.scope == label)
            .ok_or_else(|| self.unsupported(span, "breaking out of a labeled block"))
    }

    /// JS needs a label only when jumping past the innermost loop.
    fn jump_label(&mut self, i: usize) -> Option<String> {
        if i == self.loops.len() - 1 {
            return None;
        }
        if self.loops[i].label.is_none() {
            let base = self.loops[i].label_base.clone();
            let label = fresh_in(&mut self.labels, &base);
            self.loops[i].label = Some(label);
        }
        self.loops[i].label.clone()
    }

    fn fresh(&mut self, base: &str) -> String {
        fresh_in(&mut self.names, base)
    }

    fn bind(&mut self, var: LocalVarId, name: &str, mutable: bool) -> String {
        let name = self.fresh(&camel_case(name));
        self.vars.insert(
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
    /// Anyone else's, like `u32::MAX`, is its value, written in place.
    fn named_const(&self, def_id: DefId, args: ty::GenericArgsRef<'tcx>, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        if self.krate.fns.contains_key(&def_id) {
            // `fn_ref` also records the use, which is what imports its module.
            let place = self.fn_ref(def_id);
            return Ok(if self.contains_mutated(ty) {
                self.copy(place, ty)
            } else {
                place
            });
        }
        eval_const(self.tcx, self.typing_env, def_id, args, span)
            .and_then(|value| const_js(self.tcx, value))
            .ok_or_else(|| self.unsupported(span, "this constant"))
    }

    /// An integer `const`'s value: `x / SIZE` can't divide by zero.
    fn known_int(&self, e: ExprId) -> Option<i128> {
        let ExprKind::NamedConst { def_id, args, .. } = self.thir[self.strip(e)].kind else {
            return None;
        };
        let value = eval_const(self.tcx, self.typing_env, def_id, args, self.thir[e].span)?;
        const_js(self.tcx, value)?.as_int()
    }

    /// Recognize `?`'s desugaring (ADR 0035), and return what's tried:
    ///
    /// ```text
    /// match Try::branch(e) { Continue(v) => v, Break(r) => return FromResidual::from_residual(r) }
    /// ```
    fn as_question(&self, e: ExprId) -> Option<ExprId> {
        let thir = self.thir;
        let ExprKind::Match { scrutinee, .. } = thir[strip(thir, e)].kind else {
            return None;
        };
        let ExprKind::Call { fun, ref args, .. } = thir[strip(thir, scrutinee)].kind else {
            return None;
        };
        let &ty::FnDef(branch, _) = thir[strip(thir, fun)].ty.kind() else {
            return None;
        };
        self.tcx.is_lang_item(branch, LangItem::TryTraitBranch).then(|| args[0])
    }

    /// `e?`: the value inside, after returning early with an `Err` or `None`.
    /// Only when the `Err` is returned as it is: a `From` conversion isn't
    /// supported yet.
    fn question(&mut self, question: ExprId, tried: ExprId, base: Option<&str>, out: &mut Vec<Stmt>) -> R<Expr> {
        let span = self.thir[question].span;
        let ty = self.thir[tried].ty;
        // A write never fails (ADR 0054).
        if self.is_fmt_result(ty) {
            self.stmt(tried, &Dest::Discard, out)?;
            return Ok(Expr::undefined());
        }
        let is_option = self.option_of(ty).is_some();
        if !is_option && !self.is_std_adt(ty, sym::Result) {
            return Err(self.unsupported(span, &format!("`?` on a `{ty}`")));
        }
        // The function's error type: the same as this one's, and the `Err` is
        // returned as it is, or one with a `From` of the crate's own, and it's
        // `{ TAG: "Err", _0: from(error) }`.
        let mut from = None;
        if !is_option {
            let ExprKind::Match { ref arms, .. } = self.thir[self.strip(question)].kind else {
                unreachable!("checked")
            };
            let returned = arms
                .iter()
                .find_map(|&arm| match self.thir[self.strip(self.thir[arm].body)].kind {
                    ExprKind::Return { value: Some(v) } => Some(self.thir[v].ty),
                    _ => None,
                });
            let error = |t: Ty<'tcx>| match t.kind() {
                ty::Adt(_, args) => args.types().nth(1),
                _ => None,
            };
            let (to, from_ty) = (returned.and_then(error), error(ty));
            // A `&str` error to a `String` one: the same JS string.
            let same_string = to
                .zip(from_ty)
                .is_some_and(|(to, from_ty)| self.is_string_like(to) && self.is_string_like(from_ty));
            if to != from_ty && !same_string {
                let (Some(to), Some(from_ty)) = (to, from_ty) else {
                    return Err(self.unsupported(span, "this `?`"));
                };
                from = Some(
                    self.error_from(to, from_ty)?
                        .ok_or_else(|| self.unsupported(span, "`?` that converts the error with this `From`"))?,
                );
            }
        }
        let (subject, _) = self.subject(tried, base.unwrap_or(if is_option { "value" } else { "result" }), out)?;
        let js_span = self.js_span(span);
        let (failed, ret, value) = if is_option {
            let boxed = self.option_of(ty).is_some_and(|inner| self.boxed_payload(inner));
            let value = if boxed {
                self.some_value(subject.clone())
            } else {
                subject.clone()
            };
            (Expr::bin(Op::LooseEq, subject, Expr::null()), Expr::undefined(), value)
        } else {
            let failed = Expr::bin(Op::Eq, Expr::member(subject.clone(), "TAG"), Expr::str("Err"));
            let ret = match from {
                Some(from) => Expr::object(vec![
                    Prop::Field("TAG".into(), Expr::str("Err")),
                    Prop::Field("_0".into(), Expr::call(from, vec![Expr::member(subject.clone(), "_0")])),
                ]),
                None => subject.clone(),
            };
            (failed, ret, Expr::member(subject, "_0"))
        };
        out.push(StmtKind::If(failed, vec![StmtKind::Return(Some(ret)).at(js_span)], None).at(js_span));
        Ok(value)
    }

    /// The function `?` converts an error with, `<to as From<from>>::from`,
    /// if it's one of the crate's own (ADR 0052).
    fn error_from(&mut self, to: Ty<'tcx>, from: Ty<'tcx>) -> R<Option<Expr>> {
        let Some(from_trait) = self.tcx.get_diagnostic_item(sym::From) else {
            return Ok(None);
        };
        let method = self.tcx.associated_item_def_ids(from_trait)[0];
        let args = self.tcx.mk_args(&[to.into(), from.into()]);
        let Some(instance) = ty::Instance::try_resolve(self.tcx, self.typing_env, method, args)? else {
            return Ok(None);
        };
        if !self.krate.fns.contains_key(&instance.def_id()) {
            return Ok(None);
        }
        let evidence = self.evidence_args(instance.def_id(), instance.args, self.tcx.def_span(instance.def_id()))?;
        let callee = self.fn_ref(instance.def_id());
        Ok(Some(if evidence.is_empty() {
            callee
        } else {
            let mut values = vec![Expr::var("error")];
            values.extend(evidence);
            Expr::arrow(
                vec!["error".into()],
                vec![StmtKind::Return(Some(Expr::call(callee, values))).at(js::Span::NONE)],
            )
        }))
    }

    /// `Some(value)` of a generic `T` (ADR 0051): `$some(value)`.
    fn some(&mut self, value: Expr) -> Expr {
        self.runtime.insert(Helper::Some);
        Expr::call(Expr::var("$some"), vec![value])
    }

    /// What's in an `Option` of a generic `T` (ADR 0051): `$someValue(option)`.
    fn some_value(&mut self, option: Expr) -> Expr {
        self.runtime.insert(Helper::SomeValue);
        Expr::call(Expr::var("$someValue"), vec![option])
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

/// Skip THIR's wrapper nodes that don't change meaning.
fn strip(thir: &Thir<'_>, mut e: ExprId) -> ExprId {
    loop {
        match thir[e].kind {
            ExprKind::Scope { value: inner, .. }
            | ExprKind::Use { source: inner }
            | ExprKind::NeverToAny { source: inner }
            | ExprKind::ValueTypeAscription { source: inner, .. }
            | ExprKind::PlaceTypeAscription { source: inner, .. } => e = inner,
            _ => return e,
        }
    }
}

fn mask_shift(r: Expr, num: Num) -> Expr {
    Expr::bin(Op::BitAnd, r, Expr::num(num.bits() - 1))
}

fn assign_op(op: AssignOp) -> BinOp {
    match op {
        AssignOp::AddAssign => BinOp::Add,
        AssignOp::SubAssign => BinOp::Sub,
        AssignOp::MulAssign => BinOp::Mul,
        AssignOp::DivAssign => BinOp::Div,
        AssignOp::RemAssign => BinOp::Rem,
        AssignOp::BitXorAssign => BinOp::BitXor,
        AssignOp::BitAndAssign => BinOp::BitAnd,
        AssignOp::BitOrAssign => BinOp::BitOr,
        AssignOp::ShlAssign => BinOp::Shl,
        AssignOp::ShrAssign => BinOp::Shr,
    }
}

/// Pick an unused name: `x`, then `x$1`, `x$2`, ... Rust identifiers can't
/// contain `$`, so these never clash with a user's name.
fn fresh_in(taken: &mut HashSet<String>, base: &str) -> String {
    let base = js_ident(base);
    if taken.insert(base.clone()) {
        return base;
    }
    (1..)
        .map(|k| format!("{base}${k}"))
        .find(|name| taken.insert(name.clone()))
        .unwrap()
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

/// Rust names that mean something else in JS get a `$` suffix.
fn js_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "arguments",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "eval",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "implements",
        "import",
        "in",
        "instanceof",
        "interface",
        "let",
        "new",
        "null",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        "yield",
        "undefined",
        "NaN",
        "Infinity",
        "Math",
        "Error",
        "String",
        "WeakMap",
        "DataView",
        "ArrayBuffer",
        "Number",
        "BigInt",
        "Object",
    ];
    if RESERVED.contains(&name) {
        format!("{name}$")
    } else {
        name.to_string()
    }
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
