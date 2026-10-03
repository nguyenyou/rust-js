//! What `Debug` takes, crate-wide: whether anything shows a value pretty,
//! `{:#?}`, and which derives are shown (ADRs 0060, 0137).

use crate::lower::{Body, strip};
use rustc_hir::def::DefKind;
use rustc_middle::thir::{ExprId, ExprKind, LocalVarId};
use rustc_middle::ty;
use rustc_middle::ty::TyCtxt;
use rustc_span::Symbol;
use rustc_span::def_id::DefId;
use std::collections::HashMap;

/// Does any body show a value with `{:#?}`, a `Debug` argument's alternate
/// placeholder, or call `Formatter::alternate` (ADR 0137)? Another
/// alternate placeholder, `{:#}` or `{:#x}`, is no reason; one whose
/// argument can't be told is taken to be.
pub(super) fn uses_pretty_debug(tcx: TyCtxt<'_>, all_bodies: &[&Body<'_>]) -> bool {
    all_bodies.iter().any(|body| {
        let thir = &body.thir;
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
        thir.exprs.iter().any(|expr| {
            let ExprKind::Call { fun, ref args, .. } = expr.kind else {
                return false;
            };
            let &ty::FnDef(id, _) = thir[fun].ty.kind() else {
                return false;
            };
            let name = tcx.item_name(id);
            let path = tcx.def_path_str(id);
            if name.as_str() == "alternate" && path.starts_with("std::fmt::Formatter") {
                return true;
            }
            if name.as_str() != "new" || !path.starts_with("std::fmt::Arguments") || args.len() != 2 {
                return false;
            }
            let ExprKind::Literal { lit, .. } = thir[through(args[0])].kind else {
                return false;
            };
            let rustc_ast::LitKind::ByteStr(ref bytes, _) = lit.node else {
                return false;
            };
            let Some(pieces) = crate::lower::format_args::decode_template(bytes.as_byte_str()) else {
                return false;
            };
            let alternates: Vec<usize> = pieces
                .iter()
                .filter_map(|piece| match piece {
                    crate::lower::format_args::Piece::Argument(index, spec) if spec.alternate => Some(*index),
                    _ => None,
                })
                .collect();
            if alternates.is_empty() {
                return false;
            }
            let array = match thir[through(args[1])].kind {
                ExprKind::VarRef { id } => lets.get(&id).map(|&init| through(init)),
                _ => Some(through(args[1])),
            };
            let Some(ExprKind::Array { fields }) = array.map(|a| &thir[a].kind) else {
                return true;
            };
            alternates.iter().any(|&index| {
                let Some(&field) = fields.get(index) else { return true };
                match thir[through(field)].kind {
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
