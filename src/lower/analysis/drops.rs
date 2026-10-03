//! What dropping takes, crate-wide: the generic parameters given a drop,
//! and the derives that drop nothing (ADR 0098).

use crate::lower::recognition::serde_impl;
use crate::lower::{Body, FnInfo};
use rustc_hir::LangItem;
use rustc_hir::def::DefKind;
use rustc_middle::thir::ExprKind;
use rustc_middle::ty;
use rustc_middle::ty::{Ty, TyCtxt};
use rustc_span::def_id::DefId;
use rustc_span::{Symbol, sym};
use std::collections::{HashMap, HashSet};

/// The type parameters of the crate's own generic functions that a caller
/// gives a value with a destructor (ADR 0098), directly or through a
/// generic function of its own: those functions drop a `T` through a drop
/// function they're given. Only they are, so generic code nothing gives such
/// a value to is what it was.
pub(super) fn drop_params<'tcx>(
    tcx: TyCtxt<'tcx>,
    all_bodies: &[&Body<'tcx>],
    fns: &HashMap<DefId, FnInfo>,
    foreign: &crate::lower::library::Foreign<'_, 'tcx>,
    library: bool,
) -> HashMap<DefId, Vec<u32>> {
    let mut given: HashSet<(DefId, u32)> = HashSet::new();
    // A crate with no destructor of its own, and none of a library's, has
    // no value to drop: what's below would give drops nothing calls. A
    // library's consumers may have one.
    if !library && !crate::lower::traits::may_have_destructors(tcx, foreign) {
        return HashMap::new();
    }
    // A trait impl's methods are called through its dictionary, or resolved
    // where they're called, by callers the walk below can't see (ADRs 0098,
    // 0100), and what one drops needn't be in its own body: a helper it lends
    // a value to may, or a std method, or a default the trait wrote. So a
    // generic impl is given a drop for each type parameter that isn't `Copy`,
    // and so are its methods, unless it's a derive whose body drops none.
    let impls: HashSet<DefId> = fns
        .keys()
        .filter_map(|&id| {
            tcx.trait_impl_of_assoc(id)
                .or((tcx.def_kind(id) == DefKind::Impl { of_trait: true }).then_some(id))
        })
        .collect();
    for imp in impls {
        if drops_nothing_derived(tcx, imp) {
            continue;
        }
        let typing_env = ty::TypingEnv::non_body_analysis(tcx, imp);
        let owners: Vec<DefId> = std::iter::once(imp)
            .chain(tcx.associated_item_def_ids(imp).iter().copied())
            .collect();
        for param in &tcx.generics_of(imp).own_params {
            if let ty::GenericParamDefKind::Type { .. } = param.kind
                && !tcx.type_is_copy_modulo_regions(typing_env, Ty::new_param(tcx, param.index, param.name))
            {
                for &id in &owners {
                    given.insert((id, param.index));
                }
            }
        }
    }
    // A trait's default body is copied into each impl that keeps it (ADR 0049),
    // and dropped there as the impl's type drops: what it gives a generic
    // function of the crate's, as `discard(self)`, that function must be able
    // to drop. So its `Self`, and each type parameter of its trait that isn't
    // `Copy`, count as given a drop, here to be passed on.
    for body in all_bodies {
        let method = tcx.typeck_root_def_id(body.def_id.to_def_id());
        let Some(trait_id) = tcx.trait_of_assoc(method) else {
            continue;
        };
        let typing_env = ty::TypingEnv::non_body_analysis(tcx, method);
        for param in &tcx.generics_of(trait_id).own_params {
            if let ty::GenericParamDefKind::Type { .. } = param.kind
                && !tcx.type_is_copy_modulo_regions(typing_env, Ty::new_param(tcx, param.index, param.name))
            {
                given.insert((method, param.index));
            }
        }
    }
    // A library's consumers are callers it never sees (ADR 0100): a function
    // of it they can reach is given a drop for each type parameter they could
    // give a value with a destructor, one that isn't `Copy`.
    if library {
        for &id in fns.keys() {
            // A trait impl's, and its methods, are decided above.
            if tcx.trait_of_assoc(id).is_some()
                || tcx.trait_impl_of_assoc(id).is_some()
                || matches!(tcx.def_kind(id), DefKind::Impl { .. })
                || !crate::lower::library::reachable(tcx, id)
            {
                continue;
            }
            let typing_env = ty::TypingEnv::non_body_analysis(tcx, id);
            let generics = tcx.generics_of(id);
            for index in 0..generics.count() {
                let param = generics.param_at(index, tcx);
                if let ty::GenericParamDefKind::Type { .. } = param.kind
                    && !tcx.type_is_copy_modulo_regions(typing_env, Ty::new_param(tcx, param.index, param.name))
                {
                    given.insert((id, index as u32));
                }
            }
        }
    }
    // A caller's type parameter passed on as a callee's: `relay<U>` calling `consume::<U>`.
    let mut passed: Vec<((DefId, u32), (DefId, u32))> = Vec::new();
    for body in all_bodies {
        let caller = tcx.typeck_root_def_id(body.def_id.to_def_id());
        for expr in body.thir.exprs.iter() {
            let (ExprKind::ZstLiteral { .. }, &ty::FnDef(callee, args)) = (&expr.kind, expr.ty.kind()) else {
                continue;
            };
            if !fns.contains_key(&callee) || tcx.trait_of_assoc(callee).is_some() {
                continue;
            }
            for (index, arg) in args.iter().enumerate() {
                let Some(ty) = arg.as_type() else { continue };
                let index = index as u32;
                if holds_user_drop(tcx, foreign, ty, &mut Vec::new()) {
                    given.insert((callee, index));
                }
                for part in ty.walk() {
                    if let Some(part) = part.as_type()
                        && let ty::Param(param) = part.kind()
                    {
                        passed.push(((caller, param.index), (callee, index)));
                    }
                }
            }
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for &(from, to) in &passed {
            if given.contains(&from) && given.insert(to) {
                changed = true;
            }
        }
    }
    let mut params: HashMap<DefId, Vec<u32>> = HashMap::new();
    for (def, index) in given {
        params.entry(def).or_default().push(index);
    }
    for indices in params.values_mut() {
        indices.sort();
    }
    params
}

/// Is `imp` a derive whose body drops nothing of what it's given or makes:
/// `Clone`'s, `Debug`'s, `Default`'s and the comparisons', or serde's, whose
/// codecs rust-js writes (ADR 0077)?
pub(super) fn drops_nothing_derived(tcx: TyCtxt<'_>, imp: DefId) -> bool {
    if !tcx.is_automatically_derived(imp) {
        return false;
    }
    let tr = tcx
        .impl_trait_ref(imp)
        .instantiate_identity()
        .skip_normalization()
        .def_id;
    [
        LangItem::Clone,
        LangItem::Copy,
        LangItem::PartialEq,
        LangItem::PartialOrd,
    ]
    .into_iter()
    .any(|item| tcx.is_lang_item(tr, item))
        || [
            sym::Eq,
            sym::Ord,
            sym::Hash,
            Symbol::intern("Debug"),
            Symbol::intern("Default"),
        ]
        .into_iter()
        .any(|name| tcx.is_diagnostic_item(name, tr))
        || serde_impl(tcx, imp).is_some()
}

/// Whether dropping a `ty` could run a `Drop` of the crate's own, through
/// its fields, variants or what it holds.
pub(super) fn holds_user_drop<'tcx>(
    tcx: TyCtxt<'tcx>,
    foreign: &crate::lower::library::Foreign<'_, 'tcx>,
    ty: Ty<'tcx>,
    seen: &mut Vec<Ty<'tcx>>,
) -> bool {
    if seen.contains(&ty) {
        return false;
    }
    seen.push(ty);
    let found = match ty.kind() {
        ty::Adt(adt, args) => {
            tcx.adt_destructor(adt.did())
                .is_some_and(|d| d.did.is_local() || foreign.item(d.did).is_some())
                || adt
                    .all_fields()
                    .any(|f| holds_user_drop(tcx, foreign, f.ty(tcx, args).skip_normalization(), seen))
                || args.types().any(|t| holds_user_drop(tcx, foreign, t, seen))
        }
        ty::Tuple(items) => items.iter().any(|t| holds_user_drop(tcx, foreign, t, seen)),
        ty::Array(item, _) | ty::Slice(item) => holds_user_drop(tcx, foreign, *item, seen),
        ty::Closure(_, args) => args
            .as_closure()
            .upvar_tys()
            .iter()
            .any(|t| holds_user_drop(tcx, foreign, t, seen)),
        _ => false,
    };
    seen.pop();
    found
}
