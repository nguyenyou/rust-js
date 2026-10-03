//! What `Debug` takes, crate-wide: whether anything shows a value pretty,
//! `{:#?}`, and which derives are shown (ADRs 0060, 0137).

use crate::lower::format_args::{Piece, decode_template};
use crate::lower::{Body, strip};
use rustc_hir::def::DefKind;
use rustc_middle::thir::{ExprId, ExprKind, LocalVarId, Thir};
use rustc_middle::ty;
use rustc_middle::ty::TyCtxt;
use rustc_span::Symbol;
use rustc_span::def_id::DefId;
use std::collections::HashMap;

/// Each `format_args!` in `thir`: its placeholders, and the argument at
/// each, `Argument::new_display(&x)` and the like, where they can be told:
/// `None` for one whose arguments can't.
fn format_calls(tcx: TyCtxt<'_>, thir: &Thir<'_>) -> Vec<(Vec<Piece>, Option<Vec<ExprId>>)> {
    let through = |mut e: ExprId| loop {
        e = strip(thir, e);
        match thir[e].kind {
            ExprKind::Borrow { arg, .. } | ExprKind::Deref { arg } => e = arg,
            _ => break e,
        }
    };
    // The `let`s a `format_args!` keeps its arguments' array in.
    let lets: HashMap<LocalVarId, ExprId> = thir
        .stmts
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            rustc_middle::thir::StmtKind::Let {
                pattern,
                initializer: Some(init),
                ..
            } => match pattern.kind {
                rustc_middle::thir::PatKind::Binding { var, .. } => Some((var, *init)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    thir.exprs
        .iter()
        .filter_map(|expr| {
            let ExprKind::Call { fun, ref args, .. } = expr.kind else {
                return None;
            };
            let &ty::FnDef(id, _) = thir[fun].ty.kind() else {
                return None;
            };
            if tcx.item_name(id).as_str() != "new"
                || !tcx.def_path_str(id).starts_with("std::fmt::Arguments")
                || args.len() != 2
            {
                return None;
            }
            let ExprKind::Literal { lit, .. } = thir[through(args[0])].kind else {
                return None;
            };
            let rustc_ast::LitKind::ByteStr(ref bytes, _) = lit.node else {
                return None;
            };
            let pieces = decode_template(bytes.as_byte_str())?;
            let array = match thir[through(args[1])].kind {
                ExprKind::VarRef { id } => lets.get(&id).map(|&init| through(init)),
                _ => Some(through(args[1])),
            };
            let fields = match array.map(|a| &thir[a].kind) {
                Some(ExprKind::Array { fields }) => Some(fields.iter().map(|&field| through(field)).collect()),
                _ => None,
            };
            Some((pieces, fields))
        })
        .collect()
}

/// Does any body show a value with `{:#?}`, a `Debug` argument's alternate
/// placeholder, or call `Formatter::alternate` (ADR 0137)? Another
/// alternate placeholder, `{:#}` or `{:#x}`, is no reason; one whose
/// argument can't be told is taken to be.
pub(super) fn uses_pretty_debug(tcx: TyCtxt<'_>, all_bodies: &[&Body<'_>]) -> bool {
    all_bodies.iter().any(|body| {
        let thir = &body.thir;
        let asks = thir.exprs.iter().any(|expr| {
            matches!(expr.kind, ExprKind::Call { fun, .. }
                if matches!(*thir[fun].ty.kind(), ty::FnDef(id, _)
                    if tcx.item_name(id).as_str() == "alternate" && tcx.def_path_str(id).starts_with("std::fmt::Formatter")))
        });
        asks || format_calls(tcx, thir).into_iter().any(|(pieces, fields)| {
            let alternates: Vec<usize> = pieces
                .iter()
                .filter_map(|piece| match piece {
                    Piece::Argument(index, spec) if spec.alternate => Some(*index),
                    _ => None,
                })
                .collect();
            if alternates.is_empty() {
                return false;
            }
            let Some(fields) = fields else { return true };
            alternates.iter().any(|&index| {
                let Some(&field) = fields.get(index) else { return true };
                match thir[field].kind {
                    ExprKind::Call { fun, .. } => match thir[fun].ty.kind() {
                        &ty::FnDef(made_by, _) => tcx.item_name(made_by).as_str().starts_with("new_debug"),
                        _ => true,
                    },
                    _ => true,
                }
            })
        })
    })
}

/// Does any body give a placeholder's options, a width, a precision, a sign
/// or zeros, to a value that isn't a number, a `bool`, a `char`, a string
/// or `()`: one of the crate's own, a generic `T`, a `dyn` (ADR 0058)? Then
/// its writers take them, and its dictionaries apply them. One whose
/// argument can't be told is taken to be.
pub(super) fn uses_format_options(tcx: TyCtxt<'_>, all_bodies: &[&Body<'_>]) -> bool {
    let primitive = |ty: ty::Ty<'_>| {
        ty.is_numeric()
            || ty.is_bool()
            || ty.is_char()
            || ty.is_str()
            || ty.is_unit()
            || matches!(ty.kind(), ty::Adt(adt, _) if tcx.is_lang_item(adt.did(), rustc_hir::LangItem::String))
    };
    all_bodies.iter().any(|body| {
        let thir = &body.thir;
        format_calls(tcx, thir).into_iter().any(|(pieces, fields)| {
            let given: Vec<usize> = pieces
                .iter()
                .filter_map(|piece| match piece {
                    Piece::Argument(index, spec)
                        if spec.width.is_some()
                            || spec.precision.is_some()
                            || spec.width_from.is_some()
                            || spec.precision_from.is_some()
                            || spec.plus
                            || spec.zero =>
                    {
                        Some(*index)
                    }
                    _ => None,
                })
                .collect();
            if given.is_empty() {
                return false;
            }
            let Some(fields) = fields else { return true };
            given.iter().any(|&index| {
                // `Argument::new_display(&x)`: what `x` is.
                let Some(&field) = fields.get(index) else { return true };
                let ExprKind::Call { ref args, .. } = thir[field].kind else {
                    return true;
                };
                args.first().is_none_or(|&arg| !primitive(thir[arg].ty.peel_refs()))
            })
        })
    })
}

/// A `#[derive(Debug)]` impl: lowered, since it's how `{:?}` shows its type.
pub(super) fn derived_debug(tcx: TyCtxt<'_>, id: DefId) -> bool {
    tcx.is_automatically_derived(id)
        && matches!(tcx.def_kind(id), DefKind::Impl { of_trait: true })
        && tcx.is_diagnostic_item(
            Symbol::intern("Debug"),
            tcx.impl_trait_ref(id)
                .instantiate_identity()
                .skip_normalization()
                .def_id,
        )
}
