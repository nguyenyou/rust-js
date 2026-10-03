//! Read-only questions about a captured THIR body. No emission state, names,
//! dependencies or JavaScript: these answers cannot change lowering as a side effect.

use rustc_hir::{HirId, LangItem};
use rustc_middle::middle::region;
use rustc_middle::mir::BorrowKind;
use rustc_middle::thir::{self, ExprId, ExprKind, LocalVarId, Pat, PatKind, Thir};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::sym;
use std::collections::{HashMap, HashSet};

/// The parts of a `for pat in head { body }` (ADR 0025).
pub(super) struct ForLoop<'a, 'tcx> {
    pub(super) head: ExprId,
    pub(super) pat: &'a Pat<'tcx>,
    pub(super) body: ExprId,
    /// The `loop` inside, which `break` and `continue` refer to.
    pub(super) scope: region::Scope,
    pub(super) hir_id: HirId,
}

pub(super) struct BodyQuery<'a, 'tcx> {
    pub(super) tcx: TyCtxt<'tcx>,
    pub(super) thir: &'a Thir<'tcx>,
}

impl<'a, 'tcx> BodyQuery<'a, 'tcx> {
    fn strip(&self, e: ExprId) -> ExprId {
        strip(self.thir, e)
    }

    /// The body of the `loop` a scope holds: its value, or, for one that
    /// never ends used as a value of another type, what `NeverToAny` holds.
    pub(super) fn scoped_loop(&self, value: ExprId) -> Option<ExprId> {
        match self.thir[value].kind {
            ExprKind::Loop { body } => Some(body),
            ExprKind::NeverToAny { source } => match self.thir[source].kind {
                ExprKind::Loop { body } => Some(body),
                _ => None,
            },
            _ => None,
        }
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
    pub(super) fn as_for(&self, e: ExprId) -> Option<ForLoop<'a, 'tcx>> {
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
    pub(super) fn as_await(&self, e: ExprId) -> Option<ExprId> {
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

    /// Recognize `?`'s desugaring (ADR 0035), and return what's tried:
    ///
    /// ```text
    /// match Try::branch(e) { Continue(v) => v, Break(r) => return FromResidual::from_residual(r) }
    /// ```
    pub(super) fn as_question(&self, e: ExprId) -> Option<ExprId> {
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

    /// The variable a place starts from.
    pub(super) fn root_var(&self, e: ExprId) -> Option<LocalVarId> {
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => Some(id),
            ExprKind::Field { lhs, .. } | ExprKind::Deref { arg: lhs } => self.root_var(lhs),
            _ => None,
        }
    }

    /// A place as a variable and a path of fields, like `p.x` as `(p, [0])`.
    pub(super) fn place_path(&self, e: ExprId) -> Option<(LocalVarId, Vec<usize>)> {
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
}

/// Context-independent facts collected once, before borrowck steals the body.
/// Type/representation facts depend on the current instantiation and stay elsewhere.
#[derive(Default)]
pub(super) struct BodyFacts {
    pub(super) uses: HashMap<LocalVarId, usize>,
    pub(super) mutably_borrowed: HashSet<LocalVarId>,
    pub(super) stepped: HashSet<LocalVarId>,
}

impl BodyFacts {
    pub(super) fn collect<'tcx>(tcx: TyCtxt<'tcx>, thir: &Thir<'tcx>) -> Self {
        let query = BodyQuery { tcx, thir };
        let mut facts = Self {
            stepped: stepped_locals(tcx, thir),
            ..Self::default()
        };
        for expr in thir.exprs.iter() {
            match expr.kind {
                ExprKind::VarRef { id } => {
                    *facts.uses.entry(id).or_default() += 1;
                }
                ExprKind::Borrow {
                    borrow_kind: BorrowKind::Mut { .. },
                    arg,
                } => {
                    if let Some(id) = query.root_var(arg) {
                        facts.mutably_borrowed.insert(id);
                    }
                }
                _ => {}
            }
        }
        facts
    }
}

/// Skip THIR's wrapper nodes that don't change meaning.
pub(super) fn strip(thir: &Thir<'_>, mut e: ExprId) -> ExprId {
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

/// What `&mut it`, reborrowed or not, lends: `it`.
fn lent(thir: &Thir<'_>, e: ExprId) -> ExprId {
    let e = strip(thir, e);
    match thir[e].kind {
        ExprKind::Borrow { arg, .. } | ExprKind::Deref { arg } => lent(thir, arg),
        _ => e,
    }
}

/// The locals a body calls `next()` on, directly or through `&mut`: the
/// ones that must know where they are (ADR 0071). So is one lent as a
/// `&mut dyn Iterator`, which what it's lent to steps through.
pub(super) fn stepped_locals<'tcx>(tcx: TyCtxt<'tcx>, thir: &Thir<'tcx>) -> HashSet<LocalVarId> {
    let mut stepped = HashSet::new();
    for expr in thir.exprs.iter() {
        if let ExprKind::PointerCoercion {
            cast: PointerCoercion::Unsize,
            source,
            ..
        } = expr.kind
            && expr.ty.is_ref()
            && super::recognition::is_dyn_iter(tcx, expr.ty)
            && let ExprKind::VarRef { id } = thir[lent(thir, source)].kind
        {
            stepped.insert(id);
        }
        let ExprKind::Call { fun, ref args, .. } = expr.kind else {
            continue;
        };
        let &ty::FnDef(def_id, _) = thir[fun].ty.kind() else {
            continue;
        };
        let steps = tcx
            .trait_of_assoc(def_id)
            .is_some_and(|t| tcx.is_diagnostic_item(sym::Iterator, t))
            && tcx.item_name(def_id) == sym::next;
        let Some(&receiver) = args.first() else {
            continue;
        };
        let receiver = match thir[strip(thir, receiver)].kind {
            ExprKind::Borrow { arg, .. } => strip(thir, arg),
            _ => strip(thir, receiver),
        };
        if steps && let ExprKind::VarRef { id } = thir[receiver].kind {
            stepped.insert(id);
        }
    }
    stepped
}

/// Does `thir` read a static? A static's value, made where its module loads,
/// may not be there yet where another's initializer is (ADR 0096).
pub(super) fn reads_statics(thir: &Thir<'_>) -> bool {
    thir.exprs
        .iter()
        .any(|e| matches!(e.kind, ExprKind::StaticRef { .. } | ExprKind::ThreadLocalRef(_)))
}
