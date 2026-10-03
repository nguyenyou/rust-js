//! The types something changes in place, which a copy is made of where
//! it's read (ADR 0052).

use crate::lower::{Body, strip};
use rustc_hir::LangItem;
use rustc_middle::mir::BorrowKind;
use rustc_middle::thir::ExprKind;
use rustc_middle::ty;
use rustc_middle::ty::{Ty, TyCtxt};
use rustc_span::DesugaringKind;
use std::collections::HashSet;

/// The types whose JS objects get changed in place somewhere in the crate:
/// `a.b.c = ..` changes the object `a.b`, so it's `a.b`'s type. Only these
/// ever need copying (ADR 0020).
pub(super) fn mutated_types<'tcx>(tcx: TyCtxt<'tcx>, all_bodies: &[&Body<'tcx>]) -> HashSet<Ty<'tcx>> {
    let mut mutated = HashSet::new();
    for body in all_bodies {
        for expr in body.thir.exprs.iter() {
            // An enum something takes `&mut` of, or matches with a `ref mut`
            // binding, may have a variant's field changed through it.
            let enum_ty = |ty: Ty<'tcx>| matches!(ty.peel_refs().kind(), ty::Adt(adt, _) if adt.is_enum());
            // A range's bounds are changed by std's methods, as `next()` (ADR 0129),
            // though not by a `for`, which steps through its own.
            let range_ty = |ty: Ty<'tcx>| {
                !expr.span.is_desugaring(DesugaringKind::ForLoop)
                    && matches!(ty.peel_refs().kind(), ty::Adt(adt, _) if [LangItem::Range, LangItem::RangeInclusiveStruct, LangItem::RangeFrom]
                    .into_iter()
                    .any(|item| tcx.is_lang_item(adt.did(), item)))
            };
            match expr.kind {
                ExprKind::Borrow {
                    borrow_kind: BorrowKind::Mut { .. },
                    arg,
                } if enum_ty(body.thir[arg].ty) || range_ty(body.thir[arg].ty) => {
                    mutated.insert(body.thir[arg].ty.peel_refs());
                }
                ExprKind::Match {
                    scrutinee, ref arms, ..
                } if enum_ty(body.thir[scrutinee].ty)
                    && arms.iter().any(|&arm| binds_ref_mut(&body.thir[arm].pattern)) =>
                {
                    mutated.insert(body.thir[scrutinee].ty.peel_refs());
                }
                ExprKind::Let {
                    expr: scrutinee,
                    ref pat,
                } if enum_ty(body.thir[scrutinee].ty) && binds_ref_mut(pat) => {
                    mutated.insert(body.thir[scrutinee].ty.peel_refs());
                }
                _ => {}
            }
            // `a[i] = ..` changes the array `a` the same way.
            if let ExprKind::Assign { lhs, .. } | ExprKind::AssignOp { lhs, .. } = expr.kind
                && let ExprKind::Field { lhs: object, .. } | ExprKind::Index { lhs: object, .. } =
                    body.thir[strip(&body.thir, lhs)].kind
            {
                mutated.insert(body.thir[object].ty);
            }
        }
    }
    mutated
}

/// Does `pat` bind a variable by `ref mut`, or through a `&mut` subject?
pub(super) fn binds_ref_mut(pat: &rustc_middle::thir::Pat<'_>) -> bool {
    let mut found = false;
    pat.walk_always(|p| {
        if let rustc_middle::thir::PatKind::Binding { mode, .. } = p.kind
            && matches!(mode.0, rustc_hir::ByRef::Yes(_, rustc_ast::Mutability::Mut))
        {
            found = true;
        }
    });
    found
}
