//! Iterators: a chain of std's adapters on an array or a JS iterator, and
//! what ends it (ADRs 0036, 0055, 0128), lazy where Rust's order can be seen
//! (ADR 0139), and what a chain that owns its items drops (ADR 0098).

use super::calls::Call;
use super::combinators::IterComb;
use super::combinators::{IterSource, StepOp};
use super::recognition::{StdItem, is_std_def, std_item, trait_method};
use super::representation::Num;
use super::{FnCx, R, Std};
use crate::js;
use crate::js::{Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_middle::thir::{ExprId, ExprKind, LocalVarId};
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;
use rustc_span::def_id::DefId;
use std::collections::HashSet;

/// Whether `known` takes an iterator and iterates it, through
/// `iterator_call`: a stage or a consumer.
fn iterates(known: Std) -> bool {
    is_adapter(known)
        || matches!(
            known,
            Std::Collect
                | Std::CollectString
                | Std::CollectFallible
                | Std::Sum
                | Std::Fold
                | Std::Last
                | Std::Position
                | Std::Extreme(_)
                | Std::ArrayMethod(_)
                | Std::IterComb(_)
                | Std::Len
                | Std::Step(StepOp::Next)
        )
}

/// What a function's iterator chains are, beyond their types, each stage by
/// its body's THIR and its expression.
#[derive(Default)]
pub(super) struct Chains {
    /// The stages that are JS iterators, though their types aren't lazy
    /// (ADR 0139), and the receivers that start one.
    pub(super) lazy: HashSet<(usize, ExprId)>,
    pub(super) starts: HashSet<(usize, ExprId)>,
    /// The stages of chains that `collect` drains of owned items (ADR 0098),
    /// which std calls' check of what they take lets through.
    pub(super) drains: HashSet<(usize, ExprId)>,
    /// The variables that hold a lazy chain, which every use iterates.
    pub(super) locals: HashSet<LocalVarId>,
}

/// A stage of a chain, as `mark_lazy_chain` weighs it.
struct Stage {
    at: ExprId,
    /// Whether its own closure, or what it discards, does what can be seen.
    own: bool,
    impure: bool,
    tells: bool,
    zips: bool,
    receiver: ExprId,
    other: Option<ExprId>,
}

/// Whether `known` is an adapter, which makes an iterator of an iterator,
/// rather than a consumer, which ends one.
fn is_adapter(known: Std) -> bool {
    matches!(
        known,
        Std::ArrayMethod("map" | "filter")
            | Std::Enumerate
            | Std::Rev
            | Std::Skip
            | Std::Take
            | Std::Cloned
            | Std::Fuse
            | Std::IterComb(
                IterComb::FilterMap
                    | IterComb::Scan
                    | IterComb::FlatMap
                    | IterComb::Flatten
                    | IterComb::Zip
                    | IterComb::Chain
                    | IterComb::TakeWhile
                    | IterComb::SkipWhile
                    | IterComb::StepBy
                    | IterComb::Inspect
            )
    )
}

/// Whether `known` stops before its iterator ends, so a stage before it
/// that does what can be seen runs fewer times in Rust than over an array,
/// or, as `rev` does, runs it from the other end.
fn sensitive(known: Std) -> bool {
    matches!(
        known,
        Std::Take
            | Std::Rev
            | Std::Position
            // It stops at the first `Err` or `None`.
            | Std::CollectFallible
            | Std::ArrayMethod("find" | "some" | "every")
            | Std::IterComb(IterComb::TakeWhile | IterComb::Zip | IterComb::FindMap | IterComb::Nth)
    )
}

/// The locals stepped through, each a `$iter` that knows where it is (ADR
/// 0071): those `next()` is called on, or that are lent as a `&mut dyn
/// Iterator`, in the bodies being lowered, and those bound as one.
#[derive(Default)]
pub(super) struct Stepping {
    stepped: HashSet<LocalVarId>,
    bound: HashSet<LocalVarId>,
}

impl Stepping {
    pub(super) fn of(stepped: &HashSet<LocalVarId>) -> Stepping {
        Stepping {
            stepped: stepped.clone(),
            bound: HashSet::new(),
        }
    }
}

/// What a body inside another takes from the enclosing one's `Stepping`
/// while it's lowered, given back as it ends.
pub(super) struct EnclosingStepping {
    stepped: HashSet<LocalVarId>,
    bound: Option<HashSet<LocalVarId>>,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// Start lowering a body inside the one being lowered, whose own stepped
    /// locals are `own`: a closure's, as well as the enclosing body's, which
    /// it sees; a copied default's, an item of its own, instead of them.
    pub(super) fn enter_body_stepping(&mut self, own: &HashSet<LocalVarId>, item: bool) -> EnclosingStepping {
        let stepped = self.stepping.stepped.clone();
        let bound = match item {
            true => {
                self.stepping.stepped = own.clone();
                Some(std::mem::take(&mut self.stepping.bound))
            }
            false => {
                self.stepping.stepped.extend(own);
                None
            }
        };
        EnclosingStepping { stepped, bound }
    }

    pub(super) fn leave_body_stepping(&mut self, enclosing: EnclosingStepping) {
        self.stepping.stepped = enclosing.stepped;
        if let Some(bound) = enclosing.bound {
            self.stepping.bound = bound;
        }
    }

    /// Is `var` stepped through, so it's bound as a `$iter`?
    pub(super) fn steps_through(&self, var: LocalVarId) -> bool {
        self.stepping.stepped.contains(&var)
    }

    /// `var` is bound as a `$iter`.
    pub(super) fn bound_as_iter(&mut self, var: LocalVarId) {
        self.stepping.bound.insert(var);
    }

    /// `Iterator.from(value)`: a JS iterator of an array or of one, which
    /// steps either (ADR 0061).
    pub(super) fn js_iterator(&self, value: Expr) -> Expr {
        Expr::call(Expr::member(Expr::var("Iterator"), "from"), vec![value])
    }

    /// `$lent(it)`: a generic iterator lent as a `&mut`, which steps `it` and
    /// can't close it, as its lender goes on stepping it (ADR 0071).
    pub(super) fn lent_iterator(&mut self, it: Expr) -> Expr {
        self.runtime.insert(Helper::Lent);
        Expr::call(Expr::var("$lent"), vec![it])
    }

    /// `$iter(items)`: a JS iterator of an array that knows where it is
    /// (ADR 0071).
    pub(super) fn stepped_items(&mut self, items: Expr) -> Expr {
        self.runtime.insert(Helper::Iter);
        Expr::call(Expr::var("$iter"), vec![items])
    }

    /// What `var`, a local that `it.next()` steps through (ADR 0071), is
    /// bound to, of `init` of type `ty`: `$iter(v)` of an array's iterator,
    /// `Iterator.from(it)` of a generic one. None of one that isn't stepped
    /// through, or that knows where it is already, a JS iterator's.
    pub(super) fn stepped_value(
        &mut self,
        var: LocalVarId,
        (init, ty): (ExprId, Ty<'tcx>),
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Option<Expr>> {
        if !self.steps_through(var) {
            return Ok(None);
        }
        let value = if self.is_generic_iter(ty) {
            let value = self.expr(init, out)?;
            self.js_iterator(value)
        } else if self.is_array_iter(ty) && !self.is_lazy_value(init) && !self.is_peekable(ty) && !self.has_drops(ty) {
            let items = self.iter_value(init, out)?;
            let items = self.iter_source(items, ty, span, out)?;
            self.stepped_items(items)
        } else {
            return Ok(None);
        };
        self.bound_as_iter(var);
        Ok(Some(value))
    }

    /// An iterator that's a JS iterator, not an array (ADR 0055): one of the
    /// crate's own, or std's adapters on one.
    fn is_lazy_iter(&self, ty: ty::Ty<'tcx>) -> bool {
        self.recognition().is_lazy_iter(ty)
    }

    /// Is a `ty` given for `callee`'s type parameter `input` one whose JS
    /// value isn't an iterator, where one goes: one of the crate's own, or a
    /// range, an object (ADR 0129), where `input` is an `Iterator` or an
    /// `IntoIterator`. A range is itself anywhere else.
    pub(super) fn given_as_iterator(&self, callee: DefId, input: ty::Ty<'tcx>, ty: ty::Ty<'tcx>) -> bool {
        let ty = self.reveal(ty);
        if self.is_user_iterator(ty) {
            return true;
        }
        if self.range_kind(ty.peel_refs()).is_none() {
            return false;
        }
        let env = ty::TypingEnv::post_analysis(self.tcx, callee);
        [StdItem::Iterator, StdItem::IntoIterator].into_iter().any(|item| {
            let tr = ty::TraitRef::new(self.tcx, std_item(self.tcx, item), [input]);
            matches!(
                self.tcx.codegen_select_candidate(env.as_query_input(tr)),
                Ok(rustc_middle::traits::ImplSource::Param(_))
            )
        })
    }

    /// An argument given where `callee` takes a generic iterator, `input`,
    /// by value or lent as `&mut`: an iterator of the crate's own, or a
    /// range, as a JS iterator, a wrapper of what it steps (ADRs 0061, 0071).
    /// Anything else is `value` itself.
    pub(super) fn iterator_arg(
        &mut self,
        (callee, input): (DefId, ty::Ty<'tcx>),
        (arg, value): (ExprId, Expr),
        given: ty::Ty<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let (input, given, lent) = match *input.kind() {
            ty::Ref(_, inner, rustc_ast::Mutability::Mut) => (inner, given.peel_refs(), true),
            _ => (input, given, false),
        };
        if !matches!(input.kind(), ty::Param(_)) {
            return Ok(value);
        }
        if self.given_as_iterator(callee, input, given) {
            return self.iter_source(value, given, span, out);
        }
        // Lent, it must know where it is: a local stepped through does, and a
        // lazy one, a JS iterator, but an array kept anywhere else doesn't
        // (ADR 0071).
        let lent_place = self.boxed_or_lent(arg);
        if lent
            && !self.is_stepping(arg)
            && !self.is_lazy_value(lent_place)
            && (self.is_array_iter(given) || self.is_generic_iter(given))
        {
            return Err(self.unsupported(span, "lending an iterator that isn't a local"));
        }
        Ok(value)
    }

    /// An iterator of the crate's own as a JS one, `$iterator(it,
    /// countdownIterator_next)`, and a range as its items (ADR 0129).
    /// Anything else is `value` itself.
    pub(super) fn iter_source(&mut self, value: Expr, ty: ty::Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let ty = self.reveal(ty);
        if self.range_kind(ty).is_some() {
            return self.range_items(value, ty, span, out);
        }
        // A `&mut` one is stepped through as it's taken from.
        if let ty::Ref(_, inner, _) = ty.kind()
            && self.range_kind(*inner).is_some()
        {
            return Err(self.unsupported(span, &format!("iterating over a `{ty}`, which steps it")));
        }
        // A generic one is an array or a JS iterator: `Iterator.from` takes
        // either (ADR 0061).
        if self.is_generic_iter(ty) {
            return Ok(self.js_iterator(value));
        }
        if !self.is_user_iterator(ty) {
            return Ok(value);
        }
        let iterator = std_item(self.tcx, StdItem::Iterator);
        let next = trait_method(self.tcx, iterator, "next");
        let args = self.args_of(iterator, ty.peel_refs());
        // A generic `next` boxes a `Some` that looks like `None` (ADR 0051).
        let boxed = self.resolve_instance(next, args)?.is_some_and(|instance| {
            let id = instance.def_id();
            let output = self
                .tcx
                .fn_sig(id)
                .instantiate_identity()
                .skip_normalization()
                .skip_binder()
                .output();
            let output = self
                .tcx
                .try_normalize_erasing_regions(
                    ty::TypingEnv::post_analysis(self.tcx, id),
                    ty::Unnormalized::new_wip(output),
                )
                .unwrap_or(output);
            self.option_of(output).is_some_and(|item| self.boxed_payload(item))
        });
        let call = self.impl_call(next, args, vec![Expr::var("iterator")], span)?;
        // `(iterator) => f(iterator)` is `f`.
        let next = match &call.kind {
            js::ExprKind::Call(callee, list) if matches!(list.as_slice(), [only] if matches!(&only.kind, js::ExprKind::Var(n) if n == "iterator")) => {
                (**callee).clone()
            }
            _ => Expr::arrow(
                vec!["iterator".into()],
                vec![StmtKind::Return(Some(call)).at(js::Span::NONE)],
            ),
        };
        self.runtime.insert(Helper::Iterator);
        let mut list = vec![value, next];
        if boxed {
            self.runtime.insert(Helper::SomeValue);
            list.push(Expr::bool(true));
        }
        Ok(Expr::call(Expr::var("$iterator"), list))
    }

    /// A value given where a `dyn Iterator` goes, `Box::new(it)` or `&mut it`:
    /// a JS iterator, with JS's lazy helpers. One already is itself: a lazy
    /// chain, another `dyn`, one of the crate's own as `$iterator` makes it.
    /// Anything else, an array or a stepped local's `$iter`, is
    /// `Iterator.from` of it, which shares a `$iter`'s place. Its chain runs
    /// item by item, as what it's given to takes them (ADR 0139).
    pub(super) fn dyn_iterator(&mut self, source: ExprId, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let inner = self.boxed_or_lent(source);
        self.mark_lazy_chain(inner, true);
        let value = self.expr(source, out)?;
        let ty = self.reveal(self.thir[inner].ty);
        if self.is_user_iterator(ty) || self.is_generic_iter(ty) {
            return self.iter_source(value, ty, span, out);
        }
        let value = self.iter_source(value, ty, span, out)?;
        if self.recognition().is_dyn_iter(ty) || self.is_lazy_value(inner) {
            return Ok(value);
        }
        Ok(self.js_iterator(value))
    }

    /// What `Box::new(it)` boxes, or `&mut it` lends, reborrowed or not: `it`.
    fn boxed_or_lent(&self, e: ExprId) -> ExprId {
        let e = self.strip(e);
        match self.thir[e].kind {
            ExprKind::Borrow { arg, .. } | ExprKind::Deref { arg } => self.boxed_or_lent(arg),
            ExprKind::Call { fun, ref args, .. }
                if let &ty::FnDef(id, _) = self.thir[fun].ty.kind()
                    && is_std_def(self.tcx, id, StdItem::BoxNew) =>
            {
                self.boxed_or_lent(args[0])
            }
            _ => e,
        }
    }

    /// What an iterator of type `iterator` yields: its `Item`.
    pub(super) fn iterator_item(&self, iterator: ty::Ty<'tcx>) -> Option<ty::Ty<'tcx>> {
        let trait_id = std_item(self.tcx, StdItem::Iterator);
        let item = trait_method(self.tcx, trait_id, "Item");
        let projection = ty::Ty::new_projection(self.tcx, ty::IsRigid::No, item, [iterator]);
        self.tcx
            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(projection))
            .ok()
    }

    /// A chain's stage, by its body and its expression: what a consumer
    /// that takes it by `&mut`, as `find` does, borrows.
    pub(super) fn chain_key(&self, e: ExprId) -> (usize, ExprId) {
        (std::ptr::from_ref(self.thir) as usize, self.chain_stage(e))
    }

    fn chain_stage(&self, e: ExprId) -> ExprId {
        match self.thir[self.strip(e)].kind {
            ExprKind::Borrow { arg, .. } => self.strip(arg),
            _ => self.strip(e),
        }
    }

    /// Whether a closure or function given to a stage may do what can be
    /// seen, or panic.
    fn impure(&self, f: Option<ExprId>) -> bool {
        f.is_some_and(|f| {
            let ty = self.thir[f].ty.peel_refs();
            let callable = ty.is_fn() || matches!(ty.kind(), ty::Closure(..) | ty::Param(_));
            callable && !super::effects::is_pure_fn(self.tcx, self.krate.closures, ty)
        })
    }

    /// Whether `e`'s value is a JS iterator: a lazy type's, or a stage a
    /// chain's consumer made lazy.
    pub(super) fn is_lazy_value(&self, e: ExprId) -> bool {
        self.is_lazy_iter(self.thir[e].ty)
            || self.chains.lazy.contains(&self.chain_key(e))
            || matches!(self.thir[self.chain_stage(e)].kind, ExprKind::VarRef { id } if self.chains.locals.contains(&id))
    }

    /// A chain kept in `var`, made by `init`: lazy if a stage does what can be
    /// seen, when every use of `var` iterates it (ADR 0139).
    pub(super) fn keep_chain(&mut self, var: LocalVarId, init: ExprId) {
        if !self.iterated_only(var) {
            return;
        }
        self.mark_lazy_chain(init, true);
        if self.chains.lazy.contains(&self.chain_key(init)) {
            self.chains.locals.insert(var);
        }
    }

    /// Whether `e` is a stage of a chain `collect()` drains of owned items
    /// (ADR 0098).
    pub(super) fn is_drained(&self, e: ExprId) -> bool {
        self.chains.drains.contains(&self.chain_key(e))
    }

    /// Whether every use of `var` iterates it: a loop over it, or a stage or
    /// a consumer of it.
    fn iterated_only(&self, var: LocalVarId) -> bool {
        let named = |e: ExprId| matches!(self.thir[self.chain_stage(e)].kind, ExprKind::VarRef { id } if id == var);
        let uses = self
            .thir
            .exprs
            .iter()
            .filter(
                |e| matches!(e.kind, ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } if id == var),
            )
            .count();
        let iterating = self
            .thir
            .exprs
            .iter()
            .filter(|e| match e.kind {
                ExprKind::Call { fun, ref args, .. } => {
                    args.first().is_some_and(|&a| named(a))
                        && (self.std_fn(fun).is_some_and(iterates)
                            || matches!(*self.thir[fun].ty.kind(), ty::FnDef(id, _) if self.tcx.is_lang_item(id, LangItem::IntoIterIntoIter)))
                }
                _ => false,
            })
            .count();
        uses > 0 && uses == iterating
    }

    /// Rust runs each item of a chain through every stage before the next,
    /// and a JS array runs every item through each stage before the next:
    /// the same, unless a stage that does what can be seen is followed by
    /// one that can tell, one that does too, or stops early. Then the chain
    /// from that stage on is a JS iterator, whose helpers are Rust's order
    /// (ADR 0139). `seen`: whether what ends the chain at `receiver`, a
    /// consumer or a loop, can tell.
    pub(super) fn mark_lazy_chain(&mut self, receiver: ExprId, seen: bool) {
        let stages = self.chain_stages(receiver);
        let Some(first) = stages.iter().rposition(|stage| stage.impure) else {
            return;
        };
        // `zip` takes from its other side only when this one gives an item:
        // that side's stages run as often as Rust's only lazily.
        let zips = stages[first].zips;
        if !seen && !zips && !stages[..first].iter().any(|stage| stage.tells) {
            return;
        }
        for stage in &stages[..=first] {
            self.chains.lazy.insert(self.chain_key(stage.at));
            // What's chained or zipped on runs in its turn too.
            if let Some(other) = stage.other {
                self.mark_lazy_chain(other, true);
            }
        }
        // A `chain` or a `zip` that does what can be seen only by its other
        // side takes this side as it is.
        if stages[first].own {
            let start = self.chain_key(stages[first].receiver);
            self.chains.starts.insert(start);
        }
    }

    /// A chain's stages, the last first: each, whether it does what can be
    /// seen, whether it can tell, and its receiver; and for `chain` and `zip`,
    /// the other side, which makes it do what can be seen if it does.
    fn chain_stages(&self, receiver: ExprId) -> Vec<Stage> {
        let mut stages = Vec::new();
        let mut at = self.chain_stage(receiver);
        while let ExprKind::Call { fun, ref args, .. } = self.thir[at].kind
            && let Some(known) = self.std_fn(fun)
            && is_adapter(known)
            && let Some(&inner) = args.first()
        {
            let other = matches!(known, Std::IterComb(IterComb::Chain | IterComb::Zip))
                .then(|| args.get(1).copied())
                .flatten();
            let other_impure = other.is_some_and(|o| self.chain_stages(o).iter().any(|stage| stage.impure));
            let own =
                (other.is_none() && self.impure(args.get(1).copied())) || self.discards_owned(known, inner).is_some();
            let impure = own || other_impure;
            stages.push(Stage {
                at,
                own,
                impure,
                tells: impure || sensitive(known),
                zips: other_impure && known == Std::IterComb(IterComb::Zip),
                receiver: inner,
                other: other.filter(|_| other_impure),
            });
            at = self.chain_stage(inner);
        }
        stages
    }

    /// `collect()` of a chain that owns its items (ADR 0098): `map`, `filter`
    /// and `skip_while` over the `into_iter()` of a `Vec` or an array, which
    /// it drains. A stage drops what it discards, and `collect` keeps the
    /// rest. Its stages are let through std calls' check of what they take:
    /// a chain nothing drains would never drop its items.
    pub(super) fn mark_owned_drain(&mut self, receiver: ExprId) {
        let mut stages = Vec::new();
        let mut at = self.chain_stage(receiver);
        loop {
            let ExprKind::Call { fun, ref args, .. } = self.thir[at].kind else {
                return;
            };
            let Some(&inner) = args.first() else { return };
            stages.push(self.chain_key(at));
            match self.std_fn(fun) {
                Some(Std::ArrayMethod("map" | "filter") | Std::IterComb(IterComb::SkipWhile)) => {
                    at = self.chain_stage(inner);
                }
                Some(Std::Same) => {
                    let source = self.reveal(self.thir[inner].ty);
                    if !(source.is_array() || self.is_vec_like(source)) {
                        return;
                    }
                    break;
                }
                _ => return,
            }
        }
        self.chains.drains.extend(stages);
    }

    /// Whether `known`, at `receiver`, a stage of a drained chain, discards
    /// items it owns, which it then drops: `filter`'s and `skip_while`'s.
    fn discards_owned(&self, known: Std, receiver: ExprId) -> Option<ty::Ty<'tcx>> {
        if !matches!(known, Std::ArrayMethod("filter") | Std::IterComb(IterComb::SkipWhile))
            || !self.chains.drains.contains(&self.chain_key(receiver))
        {
            return None;
        }
        self.iterator_item(self.reveal(self.thir[receiver].ty))
            .filter(|&item| self.has_drops(item))
    }

    /// `test`, a stage's predicate, as one that drops the `item` it
    /// discards: one `filter` doesn't keep, or one `skip_while` skips.
    fn dropping_discarded(
        &mut self,
        test: Expr,
        item: ty::Ty<'tcx>,
        discard_when: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let test = if matches!(test.kind, js::ExprKind::Var(_)) {
            test
        } else {
            self.spill(if discard_when { "skip" } else { "keep" }, test, out)
        };
        let js_span = self.js_span(span);
        let mut drop = Vec::new();
        self.drop_value(Expr::var("item"), item, span, &mut drop)?;
        let asked = Expr::call(test, vec![Expr::var("item")]);
        let answer = |b: bool| StmtKind::Return(Some(Expr::bool(b))).at(js_span);
        let body = if discard_when {
            drop.push(answer(true));
            vec![StmtKind::If(asked, drop, None).at(js_span), answer(false)]
        } else {
            let mut rest = vec![StmtKind::If(asked, vec![answer(true)], None).at(js_span)];
            rest.extend(drop);
            rest.push(answer(false));
            rest
        };
        Ok(Expr::arrow(vec!["item".into()], body))
    }

    /// An iterator's method (ADR 0036). The iterator is a JS array: a range
    /// becomes one, `$range(a, b)`, and the rest already are.
    pub(super) fn iterator_call(
        &mut self,
        known: Std,
        args: &[ExprId],
        generic_args: ty::GenericArgsRef<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        // A search, `(0..n).all(f)`, takes its iterator by `&mut`: a range
        // it's called on is the one borrowed.
        let receiver = match self.thir[self.strip(args[0])].kind {
            ExprKind::Borrow {
                borrow_kind: rustc_middle::mir::BorrowKind::Mut { .. },
                arg,
            } if (self.is_lang_adt(self.reveal(self.thir[arg].ty), LangItem::Range)
                && matches!(self.thir[self.strip(arg)].kind, ExprKind::Adt(_)))
                || self.inclusive_range(arg).is_some() =>
            {
                arg
            }
            _ => args[0],
        };
        let receiver_ty = self.reveal(self.thir[receiver].ty);
        // A consumer, which ends a chain, says which of its stages run lazily.
        if !is_adapter(known) {
            let closure = args.get(1).copied();
            self.mark_lazy_chain(receiver, sensitive(known) || self.impure(closure));
            // `find_map(f)` is a `map` that stops at the first `Some`.
            if known == Std::IterComb(IterComb::FindMap) && self.impure(closure) {
                let key = self.chain_key(receiver);
                self.chains.starts.insert(key);
            }
        }
        let items = match self.thir[self.strip(receiver)].kind {
            ExprKind::Adt(ref range)
                if self.is_lang_adt(receiver_ty, LangItem::Range)
                    && self.range_index(receiver_ty).and_then(Num::of).is_some() =>
            {
                let bound = |i: usize| range.fields.iter().find(|f| f.name.as_usize() == i).map(|f| f.expr);
                let (Some(start), Some(end)) = (bound(0), bound(1)) else {
                    unreachable!("a range has a start and an end")
                };
                let big = Num::of(self.thir[start].ty).is_some_and(Num::big);
                let (helper, name) = if big {
                    (Helper::BigRange, "$bigRange")
                } else {
                    (Helper::Range, "$range")
                };
                self.runtime.insert(helper);
                let bounds = self.operands(&[start, end], out)?;
                Expr::call(Expr::var(name), bounds)
            }
            // `a..=b`: `$range(a, b + 1)`, exact, and past the type's end.
            _ if let Some((start_id, end_id)) = self.inclusive_range(receiver)
                && Num::of(self.thir[start_id].ty).is_some() =>
            {
                let num = Num::of(self.thir[start_id].ty);
                let big = num.is_some_and(Num::big);
                let (helper, name) = if big {
                    (Helper::BigRange, "$bigRange")
                } else {
                    (Helper::Range, "$range")
                };
                self.runtime.insert(helper);
                let [start, end]: [Expr; 2] = self.operands(&[start_id, end_id], out)?.try_into().ok().unwrap();
                let one = if big { Expr::bigint(1) } else { Expr::int(1) };
                Expr::call(Expr::var(name), vec![start, Expr::bin(Op::Add, end, one)])
            }
            _ => {
                let value = self.iter_value(receiver, out)?;
                self.iter_source(value, receiver_ty, span, out)?
            }
        };
        // A JS iterator's helpers are lazy: `map`, `filter`, `take`, `drop`,
        // and those that stop early, like `find`. Anything else takes all of
        // it, as an array (ADR 0055).
        // A stage of a chain its consumer found must be lazy too (ADR 0139).
        let lazy = self.is_lazy_value(receiver);
        let starts = !lazy && self.chains.starts.contains(&self.chain_key(receiver));
        let (items, lazy) = if starts {
            (Expr::call(Expr::member(items, "values"), vec![]), true)
        } else {
            (items, lazy)
        };
        if lazy && known == Std::Rev {
            return Err(self.unsupported(
                span,
                "`rev` of a lazy iterator: one of the crate's own, or after a closure whose effects can be seen",
            ));
        }
        let discarded = self.discards_owned(known, receiver);
        // A lazy side of a `chain` or a `zip` makes it lazy (ADR 0128): its
        // helpers take either side as it is.
        let lazy = lazy
            || matches!(known, Std::IterComb(IterComb::Chain | IterComb::Zip))
                && args.get(1).is_some_and(|&o| self.is_lazy_value(o));
        if let Std::IterComb(comb) = known {
            let mut rest = self.operands(&args[1..], out)?;
            if let Some(item) = discarded {
                let test = std::mem::replace(&mut rest[0], Expr::undefined());
                rest[0] = self.dropping_discarded(test, item, true, span, out)?;
            }
            // What's chained or zipped on: one of the crate's own as a JS iterator.
            if matches!(comb, IterComb::Chain | IterComb::Zip) {
                let other = std::mem::replace(&mut rest[0], Expr::undefined());
                rest[0] = self.iter_source(other, self.thir[args[1]].ty, span, out)?;
            }
            return self.iter_comb(
                comb,
                items,
                rest.into_iter(),
                generic_args,
                receiver_ty,
                lazy,
                span,
                out,
            );
        }
        let items = match known {
            _ if !lazy => items,
            Std::ArrayMethod(_)
            | Std::Enumerate
            | Std::Fold
            | Std::Sum
            | Std::Skip
            | Std::Take
            | Std::Cloned
            | Std::Fuse
            | Std::Position
            // Steps it, and stops at the first `Err` or `None`.
            | Std::CollectFallible => items,
            _ => Expr::call(Expr::member(items, "toArray"), vec![]),
        };
        let mut rest = self.operands(&args[1..], out)?.into_iter();
        let mut next = || rest.next().expect("rustc checked the arguments");
        let method = |items: Expr, name: &str, list: Vec<Expr>| Expr::call(Expr::member(items, name), list);
        let (a, b) = (Expr::var("a"), Expr::var("b"));
        Ok(match known {
            Std::ArrayMethod(name) => match discarded {
                Some(item) => {
                    let test = next();
                    let keep = self.dropping_discarded(test, item, false, span, out)?;
                    method(items, name, vec![keep])
                }
                None => method(items, name, vec![next()]),
            },
            Std::Enumerate => {
                let pair = Expr::array(vec![Expr::var("i"), Expr::var("x")]);
                let js_span = self.js_span(span);
                method(
                    items,
                    "map",
                    vec![Expr::arrow(
                        vec!["x".into(), "i".into()],
                        vec![StmtKind::Return(Some(pair)).at(js_span)],
                    )],
                )
            }
            Std::Rev => method(items, "toReversed", vec![]),
            Std::Skip if lazy => method(items, "drop", vec![next()]),
            Std::Take if lazy => method(items, "take", vec![next()]),
            Std::Skip => method(items, "slice", vec![next()]),
            Std::Take => method(items, "slice", vec![Expr::int(0), next()]),
            Std::Fold => {
                let (init, f) = (next(), next());
                method(items, "reduce", vec![f, init])
            }
            Std::Sum => {
                let ty = generic_args.types().nth(1).expect("`sum` names what it sums to");
                let num = self.num(ty, span)?;
                let js_span = self.js_span(span);
                let add = num.wrap(Expr::bin(Op::Add, a, b));
                let f = Expr::arrow(
                    vec!["a".into(), "b".into()],
                    vec![StmtKind::Return(Some(add)).at(js_span)],
                );
                // Rust's floating Sum starts at -0.0, preserving the sign of
                // an empty sum and of a sequence containing only negative zero.
                let zero = if num.float() { Expr::num(-0.0) } else { num.literal(0) };
                method(items, "reduce", vec![f, zero])
            }
            // `Array.from(s).join("")` is `s`.
            Std::CollectString => match items.kind {
                js::ExprKind::Call(ref callee, ref args)
                    if matches!(&callee.kind, js::ExprKind::Member(object, name)
                        if name == "from" && matches!(&object.kind, js::ExprKind::Var(v) if v == "Array"))
                        && args.len() == 1 =>
                {
                    args[0].clone()
                }
                _ => method(items, "join", vec![Expr::str("")]),
            },
            // A new `Vec`: an adapter's result is a new array already, and the
            // array an iterator started from is copied, so changing one of
            // them doesn't change the other.
            Std::Collect => {
                let fresh = match &items.kind {
                    js::ExprKind::Array(_) => true,
                    js::ExprKind::Call(callee, _) => match &callee.kind {
                        js::ExprKind::Member(_, name) => [
                            "map",
                            "filter",
                            "slice",
                            "toReversed",
                            "split",
                            "from",
                            "toArray",
                            "flatMap",
                            "flat",
                            "concat",
                        ]
                        .contains(&name.as_str()),
                        js::ExprKind::Var(name) => [
                            "$range",
                            "$bigRange",
                            "$charRange",
                            "$zip",
                            "$takeWhile",
                            "$skipWhile",
                            "$windows",
                            "$chunks",
                            "$splitBy",
                            "$lines",
                            "$slice",
                            "$rest",
                            "$scan",
                            "$drain",
                            "$splitOff",
                        ]
                        .contains(&name.as_str()),
                        _ => false,
                    },
                    _ => false,
                };
                let items = if fresh { items } else { method(items, "slice", vec![]) };
                // Into a `BinaryHeap`: put in heap order, as `BinaryHeap::from` does.
                match generic_args.types().nth(1) {
                    Some(target) if self.is_std_type(target, StdItem::BinaryHeap) => {
                        let item = target.walk().nth(1).and_then(|a| a.as_type()).expect("a heap's item");
                        self.heap_of(item, span)?;
                        let compare = self.cmp_fn(item, false, span)?;
                        self.runtime.insert(Helper::HeapFrom);
                        Expr::call(Expr::var("$heapFrom"), vec![items, compare])
                    }
                    _ => items,
                }
            }
            // Into a `Result` or an `Option` of an array: the first `Err` or
            // `None`, which ends it, or `Ok` of all the values, or the values.
            Std::CollectFallible => {
                let target = generic_args.types().nth(1).expect("a collection's type");
                let ty::Adt(_, target_args) = target.kind() else {
                    unreachable!("a `Result` or an `Option`")
                };
                let collection = target_args.type_at(0);
                if !self.is_vec_like(collection) && !collection.boxed_ty().is_some_and(|t| t.is_slice()) {
                    return Err(self.unsupported(span, &format!("collecting into a `{target}`")));
                }
                if self.option_of(target).is_some() {
                    let mut list = vec![items];
                    let item = generic_args.types().next().and_then(|i| self.iterator_item(i));
                    if item.is_some_and(|item| self.option_of(item).is_some_and(|inner| self.boxed_payload(inner))) {
                        self.runtime.insert(Helper::SomeValue);
                        list.push(Expr::bool(true));
                    }
                    self.runtime.insert(Helper::CollectOptions);
                    Expr::call(Expr::var("$collectOptions"), list)
                } else {
                    self.runtime.insert(Helper::CollectResults);
                    Expr::call(Expr::var("$collectResults"), vec![items])
                }
            }
            Std::Position => {
                self.runtime.insert(Helper::Position);
                Expr::call(Expr::var("$position"), vec![items, next()])
            }
            Std::Extreme(max) => {
                let item = generic_args.types().next().and_then(|i| self.iterator_item(i));
                match item {
                    // Of what JS's `<` doesn't order: with its `cmp` (ADR 0057).
                    Some(item) if !self.is_primitive_ord(item) => {
                        let compare = self.cmp_fn(item, false, span)?;
                        self.runtime.insert(if max { Helper::MaxBy } else { Helper::MinBy });
                        let mut list = vec![items, compare];
                        if self.boxed_payload(item) {
                            self.runtime.insert(Helper::Some);
                            list.push(Expr::bool(true));
                        }
                        Expr::call(Expr::var(if max { "$maxBy" } else { "$minBy" }), list)
                    }
                    _ => {
                        self.runtime.insert(if max { Helper::Max } else { Helper::Min });
                        Expr::call(Expr::var(if max { "$max" } else { "$min" }), vec![items])
                    }
                }
            }
            Std::Last => match generic_args.types().next().and_then(|i| self.iterator_item(i)) {
                // An item that looks like `None` is boxed (ADR 0051).
                Some(item) if self.boxed_payload(item) => {
                    let items = if items.reads_same() {
                        items
                    } else {
                        self.spill("items", items, out)
                    };
                    let last = Expr::bin(Op::Sub, Expr::member(items.clone(), "length"), Expr::int(1));
                    self.some_at(items, last)
                }
                _ => method(items, "at", vec![Expr::int(-1)]),
            },
            Std::Cloned => {
                let item = generic_args.types().nth(1).expect("`cloned` names its item");
                if self.needs_clone(item) {
                    method(items, "map", vec![self.clone_fn("item", item, span)?])
                } else {
                    items
                }
            }
            Std::Fuse => items,
            // Sorting, in place (ADR 0036). JS's `sort()` compares as strings:
            // right for strings and `bool`s, and numbers need `a - b`.
            Std::Sort => {
                let elem = match receiver_ty.peel_refs().kind() {
                    ty::Slice(t) | ty::Array(t, _) => *t,
                    _ => return Err(self.unsupported(span, "sorting this")),
                };
                // A comparator's answer is a number, so a BigInt's is its `cmp`.
                if Num::of(elem).is_some_and(|n| !n.big()) {
                    let js_span = self.js_span(span);
                    let f = Expr::arrow(
                        vec!["a".into(), "b".into()],
                        vec![StmtKind::Return(Some(Expr::bin(Op::Sub, a, b))).at(js_span)],
                    );
                    method(items, "sort", vec![f])
                } else if self.is_string_like(elem) || elem.is_bool() {
                    method(items, "sort", vec![])
                } else {
                    // By its `cmp`: JS's `sort` is stable too (ADR 0057).
                    method(items, "sort", vec![self.cmp_fn(elem, false, span)?])
                }
            }
            Std::SortByKey => {
                let key = next();
                let key = if matches!(key.kind, js::ExprKind::Var(_)) {
                    key
                } else {
                    self.spill("key", key, out)
                };
                let js_span = self.js_span(span);
                // The keys' `cmp`, which is `$cmp` for what JS orders.
                let key_ty = generic_args.types().nth(1).expect("`sort_by_key` names its key");
                let mut body = Vec::new();
                let compare = self.cmp_value(
                    Expr::call(key.clone(), vec![a]),
                    Expr::call(key, vec![b]),
                    key_ty,
                    false,
                    span,
                    &mut body,
                )?;
                // A key read more than once, like a tuple's, is a `const` first.
                body.push(StmtKind::Return(Some(compare)).at(js_span));
                let f = Expr::arrow(vec!["a".into(), "b".into()], body);
                method(items, "sort", vec![f])
            }
            _ => unreachable!("not an iterator's method"),
        })
    }
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// One of std's iterator sources (ADR 0128): `None` if `known` is another.
    pub(super) fn iter_source_call(
        &mut self,
        known: Std,
        call: Call<'_, 'tcx>,
        values: &mut std::vec::IntoIter<Expr>,
    ) -> R<Option<Expr>> {
        let Call {
            generic_args,
            args,
            span,
            ..
        } = call;
        let mut arg = || values.next().expect("rustc checked the arguments");
        Ok(Some(match known {
            Std::IterSource(IterSource::Once) => Expr::array(vec![arg()]),
            Std::IterSource(IterSource::Empty) => Expr::array(vec![]),
            Std::IterSource(IterSource::Repeat) => {
                let value = arg();
                let item = generic_args.type_at(0);
                let mut list = vec![value];
                if self.needs_clone(item) {
                    list.push(self.clone_fn("value", item, span)?);
                }
                self.runtime.insert(Helper::Repeating);
                Expr::call(Expr::var("$repeating"), list)
            }
            Std::IterSource(IterSource::RepeatWith) => {
                self.runtime.insert(Helper::RepeatingWith);
                Expr::call(Expr::var("$repeatingWith"), vec![arg()])
            }
            // Their closures' `Option`s: a `Some` that looks like `None` is boxed (ADR 0051).
            Std::IterSource(source @ (IterSource::Successors | IterSource::FromFn)) => {
                let item = generic_args.type_at(0);
                let (helper, name) = match source {
                    IterSource::Successors => (Helper::Successors, "$successors"),
                    _ => (Helper::FromFn, "$fromFn"),
                };
                let boxed = self.boxed_payload(item);
                let mut list: Vec<Expr> = (0..args.len()).map(|_| arg()).collect();
                if boxed {
                    list.push(Expr::bool(true));
                }
                self.runtime.insert(helper);
                Expr::call(Expr::var(name), list)
            }
            _ => return Ok(None),
        }))
    }

    /// Does `e` name one that knows where it is: a `Peekable`, or a local
    /// `next()` steps through?
    pub(super) fn is_stepping(&self, e: ExprId) -> bool {
        // `it`, `&mut it`, or `&mut *it` of a `&mut` to one.
        let mut e = self.strip(e);
        while let ExprKind::Borrow { arg, .. } | ExprKind::Deref { arg } = self.thir[e].kind {
            e = self.strip(arg);
        }
        let ty = self.thir[e].ty;
        self.is_peekable(ty)
            || matches!(self.thir[e].kind, ExprKind::VarRef { id } if self.stepping.bound.contains(&id))
    }
}
