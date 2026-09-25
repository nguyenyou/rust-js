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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rustc_ast::{LitKind, Mutability};
use rustc_hir as hir;
use rustc_hir::def::DefKind;
use rustc_hir::{BindingMode, ByRef, HirId};
use rustc_middle::middle::region;
use rustc_middle::mir::{AssignOp, BinOp, UnOp};
use rustc_middle::thir::{
    self as thir, ArmId, BlockId, BodyTy, ExprId, ExprKind, LocalVarId, LogicalOp, Pat, PatKind, Thir,
};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::{CRATE_DEF_ID, DefId, LocalDefId};
use rustc_span::{BytePos, ErrorGuaranteed, SourceFile, Span};

use crate::js::{self, Expr, Op, Stmt, StmtKind, UnaryOp};

type R<T> = Result<T, ErrorGuaranteed>;

/// A function's THIR, copied out of rustc before borrowck steals it.
pub struct Body<'tcx> {
    def_id: LocalDefId,
    thir: Thir<'tcx>,
    expr: ExprId,
}

/// Copy the THIR of every function in the crate.
///
/// Must run *before* `analysis`: building MIR for borrowck consumes ("steals")
/// the THIR, so this is our only chance to read it.
pub fn collect_bodies(tcx: TyCtxt<'_>) -> Vec<Body<'_>> {
    tcx.hir_crate_items(())
        .definitions()
        .filter(|&def_id| tcx.def_kind(def_id) == DefKind::Fn)
        .filter_map(|def_id| {
            let (thir, expr) = tcx.thir_body(def_id).ok()?;
            let thir = (*thir.borrow()).clone();
            Some(Body { def_id, thir, expr })
        })
        .collect()
}

/// The crate's root source file: the `.rs` file rsjs was given.
pub fn root_file(tcx: TyCtxt<'_>) -> Arc<SourceFile> {
    tcx.sess.source_map().lookup_source_file(tcx.def_span(CRATE_DEF_ID).lo())
}

/// Lower every function. Reports all unsupported features as rustc errors.
pub fn lower_crate<'tcx>(
    tcx: TyCtxt<'tcx>,
    bodies: &[Body<'tcx>],
    file: &SourceFile,
) -> Option<Vec<LoweredFn>> {
    let mut failed = false;
    for def_id in tcx.hir_crate_items(()).definitions() {
        let what = match tcx.def_kind(def_id) {
            DefKind::AssocFn => "methods",
            DefKind::Const { .. } | DefKind::AssocConst { .. } => "constants",
            DefKind::Static { .. } => "statics",
            DefKind::Fn if tcx.parent_module_from_def_id(def_id).to_local_def_id() != CRATE_DEF_ID => {
                "functions inside modules"
            }
            _ => continue,
        };
        tcx.dcx().span_err(tcx.def_span(def_id), format!("rsjs does not support {what} yet"));
        failed = true;
    }
    if failed {
        return None;
    }

    let fns: HashMap<DefId, String> = bodies
        .iter()
        .map(|b| (b.def_id.to_def_id(), js_ident(tcx.item_name(b.def_id.to_def_id()).as_str())))
        .collect();
    let results: Vec<R<LoweredFn>> = bodies
        .iter()
        .map(|body| {
            let mut cx = FnCx {
                tcx,
                file_start: file.start_pos,
                file_end: file.end_position(),
                thir: &body.thir,
                fns: &fns,
                vars: HashMap::new(),
                // Locals must never shadow the functions they call.
                names: fns.values().cloned().collect(),
                labels: HashSet::new(),
                loops: Vec::new(),
                runtime: HashSet::new(),
            };
            cx.lower_fn(body)
        })
        .collect();
    results.into_iter().collect::<R<Vec<_>>>().ok()
}

pub struct LoweredFn {
    pub function: js::Function,
    pub runtime: HashSet<Helper>,
}

/// Runtime helpers, emitted into the module only when used.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Helper {
    Div,
    Rem,
}

impl Helper {
    pub fn source(self) -> &'static str {
        match self {
            Helper::Div => {
                r#"
function $div(a, b, min) {
  if (b === 0) {
    throw new Error("attempt to divide by zero");
  }
  if (a === min && b === -1) {
    throw new Error("attempt to divide with overflow");
  }
  return a / b;
}
"#
            }
            Helper::Rem => {
                r#"
function $rem(a, b, min) {
  if (b === 0) {
    throw new Error("attempt to calculate the remainder with a divisor of zero");
  }
  if (a === min && b === -1) {
    throw new Error("attempt to calculate the remainder with overflow");
  }
  return a % b;
}
"#
            }
        }
    }
}

/// Where the value of a statement-lowered expression goes.
#[derive(Clone)]
enum Dest {
    Return,
    Assign(String),
    Discard,
}

struct Var {
    name: String,
    mutable: bool,
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

/// Number representations. Every one of them is a plain JS number; the
/// difference is how results are wrapped back into range.
#[derive(Clone, Copy, PartialEq)]
enum Num {
    I8,
    I16,
    I32,
    U8,
    U16,
    U32,
    F64,
}

impl Num {
    fn of(ty: Ty<'_>) -> Option<Num> {
        Some(match ty.kind() {
            ty::Int(ty::IntTy::I8) => Num::I8,
            ty::Int(ty::IntTy::I16) => Num::I16,
            ty::Int(ty::IntTy::I32) => Num::I32,
            ty::Uint(ty::UintTy::U8) => Num::U8,
            ty::Uint(ty::UintTy::U16) => Num::U16,
            ty::Uint(ty::UintTy::U32) => Num::U32,
            ty::Float(ty::FloatTy::F64) => Num::F64,
            _ => return None,
        })
    }

    fn bits(self) -> u32 {
        match self {
            Num::I8 | Num::U8 => 8,
            Num::I16 | Num::U16 => 16,
            Num::I32 | Num::U32 => 32,
            Num::F64 => 64,
        }
    }

    fn signed(self) -> bool {
        matches!(self, Num::I8 | Num::I16 | Num::I32)
    }

    /// The inclusive value range, for integers.
    fn range(self) -> (i128, i128) {
        let bits = self.bits();
        if self.signed() {
            (-(1 << (bits - 1)), (1 << (bits - 1)) - 1)
        } else {
            (0, (1 << bits) - 1)
        }
    }

    /// Wrap an exact JS result back into this type's range, like Rust's
    /// wrapping arithmetic: `x | 0` for i32, `x >>> 0` for u32, and so on.
    fn wrap(self, e: Expr) -> Expr {
        match self {
            Num::I32 => Expr::bin(Op::BitOr, e, Expr::num(0)),
            Num::U32 => Expr::bin(Op::UShr, e, Expr::num(0)),
            Num::I8 | Num::I16 => {
                let shift = 32 - self.bits();
                Expr::bin(Op::Shr, Expr::bin(Op::Shl, e, Expr::num(shift)), Expr::num(shift))
            }
            Num::U8 | Num::U16 => Expr::bin(Op::BitAnd, e, Expr::int(self.range().1)),
            Num::F64 => e,
        }
    }
}

struct FnCx<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    /// The root file's range in rustc's global source map, for `js_span`.
    file_start: BytePos,
    file_end: BytePos,
    thir: &'a Thir<'tcx>,
    fns: &'a HashMap<DefId, String>,
    vars: HashMap<LocalVarId, Var>,
    /// JS names already taken in this function.
    names: HashSet<String>,
    labels: HashSet<String>,
    loops: Vec<Loop>,
    runtime: HashSet<Helper>,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    fn lower_fn(&mut self, body: &Body<'tcx>) -> R<LoweredFn> {
        let def_id = body.def_id.to_def_id();
        let mut params = Vec::new();
        for param in &self.thir.params {
            let span = param.ty_span.unwrap_or(self.tcx.def_span(def_id));
            self.check_value_ty(param.ty, span)?;
            let name = match param.pat.as_deref() {
                Some(pat) => match &pat.kind {
                    PatKind::Binding { name, var, mode, subpattern: None, .. } => {
                        self.check_by_value(*mode, pat.span)?;
                        self.bind(*var, name.as_str(), mode.1 == Mutability::Mut)
                    }
                    PatKind::Wild => self.fresh("_"),
                    _ => return Err(self.unsupported(pat.span, "this parameter pattern")),
                },
                None => self.fresh("_"),
            };
            params.push(name);
        }

        let BodyTy::Fn(sig) = self.thir.body_type else {
            return Err(self.unsupported(self.tcx.def_span(def_id), "this kind of body"));
        };
        let dest = if sig.output().is_unit() { Dest::Discard } else { Dest::Return };
        let mut out = Vec::new();
        self.stmt(body.expr, &dest, &mut out)?;

        Ok(LoweredFn {
            function: js::Function {
                name: self.fns[&def_id].clone(),
                params,
                body: out,
                export: self.tcx.visibility(def_id).is_public(),
                span: self.js_span(self.tcx.def_span(def_id)),
                name_span: self.tcx.def_ident_span(def_id).map_or(js::Span::NONE, |s| self.js_span(s)),
            },
            runtime: std::mem::take(&mut self.runtime),
        })
    }

    // ── Statement mode ──────────────────────────────────────────────────

    /// Emit statements that compute `e` and deliver its value to `dest`.
    fn stmt(&mut self, e: ExprId, dest: &Dest, out: &mut Vec<Stmt>) -> R<()> {
        let expr = &self.thir[e];
        // A unit value carries no information, and a never value never
        // arrives. Either way, there is nothing to deliver.
        let dest = if expr.ty.is_unit() || expr.ty.is_never() { &Dest::Discard } else { dest };
        let span = self.js_span(expr.span);

        match expr.kind {
            ExprKind::Scope { value, hir_id, region_scope } => {
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
            ExprKind::If { cond, then, else_opt, .. } => {
                let cond = self.expr(cond, out)?;
                let mut then_out = Vec::new();
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
            ExprKind::Match { scrutinee, ref arms, .. } => self.lower_match(scrutinee, arms, dest, out),
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
            ExprKind::Assign { lhs, rhs } => {
                let name = self.place(lhs)?;
                if self.is_simple(rhs) {
                    let value = self.expr(rhs, out)?;
                    out.push(StmtKind::Assign(name, value).at(span));
                    Ok(())
                } else {
                    self.stmt(rhs, &Dest::Assign(name), out)
                }
            }
            ExprKind::AssignOp { op, lhs, rhs } => {
                // For primitives, Rust evaluates the right side first.
                let rhs_js = self.expr(rhs, out)?;
                let name = self.place(lhs)?;
                let ty = self.thir[lhs].ty;
                let target = Expr::var(&name).or_at(self.js_span(self.thir[lhs].span));
                let value = self.binary(assign_op(op), target, rhs_js, ty, expr.span)?.or_at(span);
                out.push(StmtKind::Assign(name, value).at(span));
                Ok(())
            }
            _ => {
                let value = self.expr(e, out)?;
                match dest {
                    Dest::Return => out.push(StmtKind::Return(Some(value)).at(span)),
                    Dest::Assign(name) => out.push(StmtKind::Assign(name.clone(), value).at(span)),
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
                thir::StmtKind::Let { pattern, initializer, else_block, span, .. } => {
                    if else_block.is_some() {
                        return Err(self.unsupported(*span, "`let ... else`"));
                    }
                    self.lower_let(pattern, *initializer, *span, out)?;
                }
            }
        }
        Ok(())
    }

    fn lower_let(
        &mut self,
        pat: &Pat<'tcx>,
        init: Option<ExprId>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let span = self.js_span(span);
        match &pat.kind {
            PatKind::Binding { name, var, mode, subpattern: None, ty, .. } => {
                self.check_by_value(*mode, pat.span)?;
                self.check_value_ty(*ty, pat.span)?;
                let mutable = mode.1 == Mutability::Mut;
                match init {
                    Some(init) if self.is_simple(init) => {
                        let value = self.expr(init, out)?;
                        let name = self.bind(*var, name.as_str(), mutable);
                        let kind = if mutable { StmtKind::Let(name, Some(value)) } else { StmtKind::Const(name, value) };
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
            _ => Err(self.unsupported(pat.span, "this `let` pattern")),
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
        self.loops.push(Loop { scope, label_base, label: None, dest: dest.clone() });

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
        out.push(StmtKind::While { label, cond, body: body_out }.at(span));
        Ok(())
    }

    /// Recognize the `while` desugaring; returns `(cond, body)`.
    fn as_while(&self, body: ExprId, scope: region::Scope) -> Option<(ExprId, ExprId)> {
        let ExprKind::Block { block } = self.thir[self.strip(body)].kind else { return None };
        let block = &self.thir[block];
        let (true, Some(tail)) = (block.stmts.is_empty(), block.expr) else { return None };
        let ExprKind::If { cond, then, else_opt: Some(els), .. } = self.thir[self.strip(tail)].kind
        else {
            return None;
        };
        let ExprKind::Block { block: els } = self.thir[self.strip(els)].kind else { return None };
        let els = &self.thir[els];
        let ([stmt], None) = (&*els.stmts, els.expr) else { return None };
        let thir::StmtKind::Expr { expr, .. } = self.thir[*stmt].kind else { return None };
        let ExprKind::Break { label, value: None } = self.thir[self.strip(expr)].kind else {
            return None;
        };
        (label == scope && self.is_simple(cond)).then_some((cond, then))
    }

    fn lower_match(
        &mut self,
        scrutinee: ExprId,
        arms: &[ArmId],
        dest: &Dest,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        // Evaluate the scrutinee once. An immutable variable can be tested
        // directly; anything else goes into a `const`.
        let (subject, stable) = match self.thir[self.strip(scrutinee)].kind {
            ExprKind::VarRef { id } => {
                let var = &self.vars[&id];
                (var.name.clone(), !var.mutable)
            }
            _ => {
                let value = self.expr(scrutinee, out)?;
                let name = self.fresh("match");
                let span = self.js_span(self.thir[scrutinee].span);
                out.push(StmtKind::Const(name.clone(), value).at(span));
                (name, true)
            }
        };

        let mut chain: Vec<(Option<Expr>, Vec<Stmt>, js::Span)> = Vec::new();
        for (i, &arm_id) in arms.iter().enumerate() {
            let arm = &self.thir[arm_id];
            let arm_span = self.js_span(arm.span);
            let pat_span = self.js_span(arm.pattern.span);
            let mut bindings = Vec::new();
            let mut test = self
                .pattern_test(&arm.pattern, &subject, &mut bindings)?
                .map(|t| t.or_at(pat_span));

            let mut body = Vec::new();
            for (var, name, mutable) in bindings {
                if stable && !mutable {
                    // `x => ..` just names the subject: reuse it.
                    self.vars.insert(var, Var { name: subject.clone(), mutable: false });
                } else {
                    if arm.guard.is_some() {
                        return Err(self.unsupported(arm.pattern.span, "this binding in a guarded arm"));
                    }
                    let name = self.bind(var, &name, mutable);
                    let value = Expr::var(&subject).or_at(pat_span);
                    let kind = if mutable { StmtKind::Let(name, Some(value)) } else { StmtKind::Const(name, value) };
                    body.push(kind.at(pat_span));
                }
            }
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
            rest = Some(match test {
                Some(t) => vec![StmtKind::If(t, body, rest).at(span)],
                None => body,
            });
        }
        out.extend(rest.unwrap_or_default());
        Ok(())
    }

    /// A JS boolean test for "`subject` matches `pat`" (`None`: always matches).
    fn pattern_test(
        &mut self,
        pat: &Pat<'tcx>,
        subject: &str,
        bindings: &mut Vec<(LocalVarId, String, bool)>,
    ) -> R<Option<Expr>> {
        match &pat.kind {
            PatKind::Wild => Ok(None),
            PatKind::Binding { name, var, mode, subpattern: None, .. } => {
                self.check_by_value(*mode, pat.span)?;
                bindings.push((*var, name.to_string(), mode.1 == Mutability::Mut));
                Ok(None)
            }
            PatKind::Constant { value } => {
                let value = self.const_value(*value, pat.span)?;
                Ok(Some(Expr::bin(Op::Eq, Expr::var(subject), value)))
            }
            PatKind::Variant { adt_def, variant_index, subpatterns, .. } if subpatterns.is_empty() => {
                let name = adt_def.variant(*variant_index).name.to_string();
                Ok(Some(Expr::bin(Op::Eq, Expr::var(subject), Expr::str(name))))
            }
            PatKind::Leaf { subpatterns } if subpatterns.is_empty() => Ok(None),
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
            ExprKind::VarRef { id } => Ok(Expr::var(&self.vars[&id].name)),
            ExprKind::Tuple { ref fields } if fields.is_empty() => Ok(Expr::undefined()),
            ExprKind::Adt(ref adt) => {
                let variant = adt.adt_def.variant(adt.variant_index);
                if !adt.adt_def.is_enum() || !is_fieldless_enum(adt.adt_def) {
                    return Err(self.unsupported(span, "structs and enums with fields"));
                }
                Ok(Expr::str(variant.name.to_string()))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let [l, r] = self.operands(&[lhs, rhs], out)?.try_into().ok().unwrap();
                self.binary(op, l, r, self.thir[lhs].ty, span)
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
            ExprKind::Cast { source } => {
                let v = self.expr(source, out)?;
                self.cast(v, self.thir[source].ty, ty, span)
            }
            ExprKind::Call { fun, ref args, .. } => {
                let callee = self.callee(fun)?;
                let args = self.operands(args, out)?;
                let callee = Expr::var(&callee).or_at(self.js_span(self.thir[fun].span));
                Ok(Expr::call(callee, args))
            }
            ExprKind::If { cond, then, else_opt: Some(els), .. }
                if self.is_simple(then) && self.is_simple(els) =>
            {
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
            if last_complex.is_some_and(|k| i < k) && !v.is_constant() {
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
            | ExprKind::Unary { arg: source, .. } => self.is_simple(source),
            ExprKind::Literal { .. }
            | ExprKind::NonHirLiteral { .. }
            | ExprKind::VarRef { .. }
            | ExprKind::ZstLiteral { .. }
            | ExprKind::Adt(_) => true,
            ExprKind::Tuple { ref fields } => fields.is_empty(),
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::LogicalOp { lhs, rhs, .. } => {
                self.is_simple(lhs) && self.is_simple(rhs)
            }
            ExprKind::Call { fun, ref args, .. } => {
                self.is_simple(fun) && args.iter().all(|&a| self.is_simple(a))
            }
            ExprKind::If { cond, then, else_opt: Some(els), .. } => {
                self.is_simple(cond) && self.is_simple(then) && self.is_simple(els)
            }
            ExprKind::Block { block } => {
                let block = &self.thir[block];
                !block.targeted_by_break
                    && block.stmts.is_empty()
                    && block.expr.is_none_or(|t| self.is_simple(t))
            }
            _ => false,
        }
    }

    // ── Operators ───────────────────────────────────────────────────────

    fn binary(&mut self, op: BinOp, l: Expr, r: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
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
                BinOp::BitAnd => Ok(Expr::unary(UnaryOp::Not, Expr::unary(UnaryOp::Not, Expr::bin(Op::BitAnd, l, r)))),
                BinOp::BitOr => Ok(Expr::unary(UnaryOp::Not, Expr::unary(UnaryOp::Not, Expr::bin(Op::BitOr, l, r)))),
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
                let safe = r.as_int().is_some_and(|d| d != 0 && !(num.signed() && d == -1));
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
                if op == BinOp::Rem && safe { quotient } else { num.wrap(quotient) }
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
        let source = self.num(from, span)?;
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
            LitKind::Int(n, _) => {
                self.num(ty, span)?;
                let n = n.get() as i128;
                Ok(Expr::int(if neg { -n } else { n }))
            }
            LitKind::Float(sym, _) if Num::of(ty) == Some(Num::F64) => {
                let x: f64 = sym.as_str().replace('_', "").parse().expect("rustc validated the literal");
                Ok(Expr::num(if neg { -x } else { x }))
            }
            _ => Err(self.unsupported(span, "this literal")),
        }
    }

    fn const_value(&self, value: ty::Value<'tcx>, span: Span) -> R<Expr> {
        if let Some(b) = value.try_to_bool() {
            return Ok(Expr::bool(b));
        }
        let (Some(num), Some(leaf)) = (Num::of(value.ty), value.try_to_leaf()) else {
            return Err(self.unsupported(span, "this constant pattern"));
        };
        Ok(num_literal(leaf.to_bits_unchecked(), num))
    }

    fn callee(&self, fun: ExprId) -> R<String> {
        let fun = &self.thir[self.strip(fun)];
        if let (ExprKind::ZstLiteral { .. }, ty::FnDef(def_id, _)) = (&fun.kind, fun.ty.kind()) {
            if let Some(name) = self.fns.get(def_id) {
                return Ok(name.clone());
            }
            let path = self.tcx.def_path_str(*def_id);
            return Err(self.unsupported(fun.span, &format!("calling `{path}`")));
        }
        Err(self.unsupported(fun.span, "calling this"))
    }

    /// The JS variable an assignment writes to.
    fn place(&self, e: ExprId) -> R<String> {
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } => Ok(self.vars[&id].name.clone()),
            _ => Err(self.unsupported(self.thir[e].span, "assigning to this place")),
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
        js::Span { lo: (span.lo() - self.file_start).0, hi: (span.hi() - self.file_start).0 }
    }

    /// Skip THIR's wrapper nodes that don't change meaning.
    fn strip(&self, mut e: ExprId) -> ExprId {
        loop {
            match self.thir[e].kind {
                ExprKind::Scope { value: inner, .. }
                | ExprKind::Use { source: inner }
                | ExprKind::NeverToAny { source: inner }
                | ExprKind::ValueTypeAscription { source: inner, .. }
                | ExprKind::PlaceTypeAscription { source: inner, .. } => e = inner,
                _ => return e,
            }
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
        let name = self.fresh(name);
        self.vars.insert(var, Var { name: name.clone(), mutable });
        name
    }

    fn num(&self, ty: Ty<'tcx>, span: Span) -> R<Num> {
        Num::of(ty).ok_or_else(|| self.unsupported(span, &format!("values of type `{ty}`")))
    }

    fn check_value_ty(&self, ty: Ty<'tcx>, span: Span) -> R<()> {
        let ok = ty.is_bool()
            || ty.is_unit()
            || Num::of(ty).is_some()
            || matches!(ty.kind(), ty::Adt(adt, _) if is_fieldless_enum(*adt));
        if ok { Ok(()) } else { Err(self.unsupported(span, &format!("values of type `{ty}`"))) }
    }

    fn check_by_value(&self, mode: BindingMode, span: Span) -> R<()> {
        if mode.0 == ByRef::No { Ok(()) } else { Err(self.unsupported(span, "`ref` bindings")) }
    }

    fn unsupported(&self, span: Span, what: &str) -> ErrorGuaranteed {
        self.tcx.dcx().span_err(span, format!("rsjs does not support {what} yet"))
    }
}

fn is_fieldless_enum(adt: ty::AdtDef<'_>) -> bool {
    adt.is_enum() && adt.variants().iter().all(|v| v.fields.is_empty())
}

/// Turn raw constant bits into a JS number literal.
fn num_literal(bits: u128, num: Num) -> Expr {
    if num == Num::F64 {
        return Expr::num(f64::from_bits(bits as u64));
    }
    let unused = 128 - num.bits();
    let n = if num.signed() { ((bits << unused) as i128) >> unused } else { bits as i128 };
    Expr::int(n)
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
    (1..).map(|k| format!("{base}${k}")).find(|name| taken.insert(name.clone())).unwrap()
}

/// Rust names that mean something else in JS get a `$` suffix.
fn js_ident(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "arguments", "await", "break", "case", "catch", "class", "const", "continue", "debugger",
        "default", "delete", "do", "else", "enum", "eval", "export", "extends", "false", "finally",
        "for", "function", "if", "implements", "import", "in", "instanceof", "interface", "let",
        "new", "null", "package", "private", "protected", "public", "return", "static", "super",
        "switch", "this", "throw", "true", "try", "typeof", "var", "void", "while", "with",
        "yield", "undefined", "NaN", "Infinity", "Math", "Error",
    ];
    if RESERVED.contains(&name) { format!("{name}$") } else { name.to_string() }
}
