//! What evaluating an expression, or calling a closure, can do that can be
//! seen: whether it can leave early, by a panic, a `return` or a `break`,
//! or change anything. A question of the THIR alone, with no emission state,
//! as `body_queries`' are: the destructors' analysis asks it (ADR 0098), and
//! so do iterator chains, of their stages' closures (ADR 0139).

use super::recognition::{PureStd, pure_std};
use std::collections::HashMap;

use rustc_hir::LangItem;
use rustc_hir::def::DefKind;
use rustc_middle::mir::BinOp;
use rustc_middle::thir::{AdtExprBase, ExprId, ExprKind, PatKind, StmtKind as ThirStmt, Thir};
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::LocalDefId;

use super::Body;

/// Whether `e`, of `thir`, can neither leave early, by a panic, a `return`
/// or a `break`, nor change anything: what it does can't be seen.
pub(super) fn cannot_leave_in<'tcx>(tcx: TyCtxt<'tcx>, thir: &Thir<'tcx>, e: ExprId) -> bool {
    let pure = |e: ExprId| cannot_leave_in(tcx, thir, e);
    match &thir[super::strip(thir, e)].kind {
        ExprKind::Literal { .. }
        | ExprKind::NonHirLiteral { .. }
        | ExprKind::ZstLiteral { .. }
        | ExprKind::NamedConst { .. }
        | ExprKind::VarRef { .. }
        | ExprKind::UpvarRef { .. } => true,
        ExprKind::Adt(adt) => matches!(adt.base, AdtExprBase::None) && adt.fields.iter().all(|f| pure(f.expr)),
        ExprKind::Tuple { fields } | ExprKind::Array { fields } => fields.iter().all(|&f| pure(f)),
        ExprKind::Borrow { arg, .. }
        | ExprKind::Field { lhs: arg, .. }
        | ExprKind::Deref { arg }
        | ExprKind::Unary { arg, .. }
        | ExprKind::Cast { source: arg } => pure(*arg),
        // Arithmetic wraps (ADR 0011), so only an integer's division can
        // panic, and not by a literal other than 0 or -1.
        ExprKind::Binary { op, lhs, rhs } => {
            let divides = matches!(op, BinOp::Div | BinOp::Rem)
                && !thir[*lhs].ty.is_floating_point()
                && !matches!(
                    thir[super::strip(thir, *rhs)].kind,
                    ExprKind::Literal { lit, neg: false } if matches!(lit.node, rustc_ast::LitKind::Int(n, _) if n.get() != 0)
                );
            !divides && pure(*lhs) && pure(*rhs)
        }
        ExprKind::LogicalOp { lhs, rhs, .. } => pure(*lhs) && pure(*rhs),
        ExprKind::If {
            cond, then, else_opt, ..
        } => pure(*cond) && pure(*then) && else_opt.is_none_or(pure),
        // `Box::new(x)` only puts `x` in a box. An operator on references to
        // numbers, `x * 2` of an `&i32`, is a call of its trait's method, which
        // is the operator's; and a comparison of numbers or strings.
        ExprKind::Call { fun, args, .. } => {
            let ty::FnDef(id, generic_args) = *thir[*fun].ty.kind() else {
                return false;
            };
            let operator = tcx.trait_of_assoc(id).and_then(|tr| {
                [
                    LangItem::Add,
                    LangItem::Sub,
                    LangItem::Mul,
                    LangItem::BitAnd,
                    LangItem::BitOr,
                    LangItem::BitXor,
                    LangItem::Shl,
                    LangItem::Shr,
                    LangItem::Neg,
                    LangItem::Not,
                    LangItem::Div,
                    LangItem::Rem,
                    LangItem::PartialEq,
                    LangItem::PartialOrd,
                ]
                .into_iter()
                .find(|&item| tcx.is_lang_item(tr, item))
            });
            let simple = |t: Ty<'tcx>| {
                let t = t.peel_refs();
                t.is_primitive()
                    || t.is_str()
                    || matches!(t.kind(), ty::Adt(adt, _) if tcx.is_lang_item(adt.did(), LangItem::String))
            };
            let divides = |rhs: ExprId| {
                !generic_args
                    .types()
                    .next()
                    .is_some_and(|t| t.peel_refs().is_floating_point())
                    && !matches!(
                        thir[super::strip(thir, rhs)].kind,
                        ExprKind::Literal { lit, neg: false } if matches!(lit.node, rustc_ast::LitKind::Int(n, _) if n.get() != 0)
                    )
            };
            let call_pure = match operator {
                Some(LangItem::Div | LangItem::Rem) => {
                    generic_args.types().all(simple) && args.get(1).is_some_and(|&rhs| !divides(rhs))
                }
                Some(_) => generic_args.types().all(simple),
                // std's own, where no code of the crate's runs: a `len()`, a
                // question of an `Option` or a `Result`, and a copy or a string
                // of what's simple.
                None => match pure_std(tcx, id) {
                    Some(PureStd::Question | PureStd::BoxNew) => true,
                    Some(PureStd::Copy) => generic_args.types().all(simple),
                    None => false,
                },
            };
            call_pure && args.iter().all(|&a| pure(a))
        }
        // A block of `let`s of such values, and one.
        ExprKind::Block { block } => {
            let block = &thir[*block];
            block.stmts.iter().all(|&s| match &thir[s].kind {
                ThirStmt::Let {
                    initializer: Some(init),
                    else_block: None,
                    pattern,
                    ..
                } => matches!(pattern.kind, PatKind::Wild | PatKind::Binding { subpattern: None, .. }) && pure(*init),
                _ => false,
            }) && block.expr.is_none_or(pure)
        }
        _ => false,
    }
}

/// Whether calling a `ty`, a closure or a function, does nothing that
/// can be seen and can't panic: a closure whose body can't (as
/// `cannot_leave` says), or a struct's or a variant's constructor.
pub(super) fn is_pure_fn<'tcx>(tcx: TyCtxt<'tcx>, closures: &HashMap<LocalDefId, &Body<'tcx>>, ty: Ty<'tcx>) -> bool {
    match *ty.peel_refs().kind() {
        ty::Closure(def_id, _) => def_id
            .as_local()
            .and_then(|id| closures.get(&id))
            .is_some_and(|body| cannot_leave_in(tcx, &body.thir, body.expr)),
        ty::FnDef(def_id, _) => matches!(tcx.def_kind(def_id), DefKind::Ctor(..)),
        _ => false,
    }
}
