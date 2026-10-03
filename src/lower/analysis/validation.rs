//! What a crate does that rust-js refuses, found before any of it is lowered.

use crate::lower::recognition::{StdItem, from_serde_derive, is_std_def};
use crate::lower::traits;
use crate::lower::{Body, strip};
use rustc_hir::def::DefKind;
use rustc_hir::find_attr;
use rustc_middle::middle::codegen_fn_attrs::CodegenFnAttrFlags;
use rustc_middle::mir::BorrowKind;
use rustc_middle::thir::{ExprId, ExprKind, Thir};
use rustc_middle::ty;
use rustc_middle::ty::TyCtxt;
use rustc_span::Symbol;
use rustc_span::def_id::{DefId, LocalDefId};
use std::collections::HashSet;

/// Report each item rust-js can't compile yet. False if there was one.
pub(super) fn reject_unsupported(
    tcx: TyCtxt<'_>,
    foreign: &crate::lower::library::Foreign<'_, '_>,
    markers: &[(LocalDefId, Symbol)],
) -> bool {
    let mut valid = true;
    // What the crate exports by a symbol of its own, `#[no_mangle]` or
    // `#[export_name]`: an `extern` declaration of one is a JS binding
    // naming a global no JS has, and the function is the crate's, by its path.
    let exported: HashSet<Symbol> = tcx
        .hir_crate_items(())
        .definitions()
        .filter(|&id| matches!(tcx.def_kind(id), DefKind::Fn | DefKind::AssocFn) && !tcx.is_foreign_item(id))
        .filter_map(|id| {
            let attrs = tcx.codegen_fn_attrs(id);
            match attrs.flags.contains(CodegenFnAttrFlags::NO_MANGLE) {
                true => Some(attrs.symbol_name.unwrap_or_else(|| tcx.item_name(id.to_def_id()))),
                false => attrs.symbol_name,
            }
        })
        .collect();
    for def_id in tcx.hir_crate_items(()).definitions() {
        let what = match tcx.def_kind(def_id) {
            DefKind::Fn
                if tcx.is_foreign_item(def_id)
                    && exported.contains(
                        &tcx.codegen_fn_attrs(def_id)
                            .symbol_name
                            .unwrap_or_else(|| tcx.item_name(def_id.to_def_id())),
                    ) =>
            {
                "an `extern` declaration of this crate's own `#[no_mangle]` function"
            }
            // `#[eii] static HELLO: u64;`, which the linker makes another
            // item: rust-js has none to link it to, and JS would read a name
            // nothing defines (ADR 0109).
            _ if find_attr!(tcx, def_id, EiiImpls(..) | EiiDeclaration(..) | RustcEiiForeignItem) => {
                "externally implementable items"
            }
            _ if markers.iter().any(|&(marker, _)| marker == def_id) => continue,
            _ if from_serde_derive(tcx, def_id) => continue,
            // std's storage for a thread-local: JS needs none.
            _ if in_thread_local(tcx, def_id).is_some() => continue,
            // Methods, trait impls' included (ADRs 0047, 0049). Derives like
            // `#[derive(Clone)]` write impls that are never called.
            DefKind::AssocFn => continue,
            // A `type const`, of `min_generic_const_args`, has no body to type-check.
            DefKind::AssocConst { is_type_const: true } => "type constants",
            // A type's own `const`, as `Vec2::ZERO`: its value where it's used,
            // as rustc computed it (ADR 0031). A trait's too, and in generic code
            // its impl's dictionary's (ADR 0106). Not one with parameters of its own.
            DefKind::AssocConst { .. } if tcx.generics_of(def_id).own_params.is_empty() => continue,
            DefKind::AssocConst { .. } => "generic constants",
            // An `Iterator`'s `Item` (ADR 0055), an operator's `Output` (ADR
            // 0064) and a `TryFrom`'s `Error`: rustc works out what they are.
            DefKind::AssocTy
                if tcx.trait_impl_of_assoc(def_id.to_def_id()).is_some_and(|imp| {
                    let tr = tcx
                        .impl_trait_ref(imp)
                        .instantiate_identity()
                        .skip_normalization()
                        .def_id;
                    is_std_def(tcx, tr, StdItem::Iterator)
                        || is_std_def(tcx, tr, StdItem::TryFrom)
                        || traits::is_operator(tcx, tr)
                }) =>
            {
                continue;
            }
            // A trait's own, a type only a caller knows in generic code, as a
            // type parameter is (ADR 0106); a generic one too, of lifetimes,
            // which JS hasn't, or of no bound a dictionary has (ADR 0146).
            DefKind::AssocTy if traits::gat_supported(tcx, def_id.to_def_id()) => continue,
            DefKind::AssocTy => "generic associated types with bounds",
            DefKind::Impl { of_trait: true }
                if !tcx.is_automatically_derived(def_id.to_def_id())
                    && !traits::implementable(
                        tcx,
                        foreign,
                        tcx.impl_trait_ref(def_id)
                            .instantiate_identity()
                            .skip_normalization()
                            .def_id,
                    ) =>
            {
                "user implementations of this standard or external trait"
            }
            DefKind::Static { .. } if tcx.is_thread_local_static(def_id.to_def_id()) => "`#[thread_local]` statics",
            _ => continue,
        };
        tcx.dcx()
            .span_err(tcx.def_span(def_id), format!("rust-js does not support {what} yet"));
        valid = false;
    }
    valid
}

/// Report each reference rust-js can't make to a static yet (ADR 0096): a
/// `&mut` to a `static mut` or a part of one, and a raw address of any. A
/// shared one is the value it points to, as any is. False if there was one.
pub(super) fn reject_static_references(tcx: TyCtxt<'_>, all_bodies: &[&Body<'_>]) -> bool {
    let mut valid = true;
    for body in all_bodies {
        let thir = &body.thir;
        for expr in thir.exprs.iter() {
            let (arg, raw) = match expr.kind {
                ExprKind::Borrow {
                    borrow_kind: BorrowKind::Mut { .. },
                    arg,
                } => (arg, false),
                ExprKind::RawBorrow { arg, .. } => (arg, true),
                _ => continue,
            };
            let what = match static_of(thir, arg) {
                Some(d) if tcx.is_foreign_item(d) => continue,
                Some(_) if raw => "raw addresses of statics",
                Some(d) if tcx.is_mutable_static(d) => "`&mut` references to a `static mut`",
                _ => continue,
            };
            tcx.dcx()
                .span_err(expr.span, format!("rust-js does not support {what} yet"));
            valid = false;
        }
    }
    valid
}

/// The static that place `e` is, or is a part of.
pub(super) fn static_of(thir: &Thir<'_>, e: ExprId) -> Option<DefId> {
    match thir[strip(thir, e)].kind {
        ExprKind::Field { lhs, .. } | ExprKind::Index { lhs, .. } => static_of(thir, lhs),
        ExprKind::Deref { arg } => match thir[strip(thir, arg)].kind {
            ExprKind::StaticRef { def_id, .. } => Some(def_id),
            _ => None,
        },
        _ => None,
    }
}

/// `thread_local!` (ADR 0037) is a `const NAME: LocalKey<T>` whose block holds
/// `fn __rust_std_internal_init_fn() -> T { init }`, then std's storage for
/// it. In JS, it's a variable of its module, made from `init`.
pub(in crate::lower) fn is_thread_local(tcx: TyCtxt<'_>, d: LocalDefId) -> bool {
    matches!(tcx.def_kind(d), DefKind::Const { .. })
        && matches!(tcx.type_of(d).instantiate_identity().skip_normalization().kind(), ty::Adt(adt, _) if is_std_def(tcx, adt.did(), StdItem::LocalKey))
}

/// The thread-local whose block `d` is in, if any.
pub(super) fn in_thread_local(tcx: TyCtxt<'_>, d: LocalDefId) -> Option<LocalDefId> {
    let mut parent = tcx.opt_local_parent(d);
    while let Some(p) = parent {
        if is_thread_local(tcx, p) {
            return Some(p);
        }
        parent = tcx.opt_local_parent(p);
    }
    None
}
