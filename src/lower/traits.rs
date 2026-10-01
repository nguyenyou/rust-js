//! Trait evidence stays separate from payloads: arguments for generics,
//! lazy dictionaries for impls, and `{ value, impl }` for trait objects.

use super::bindings;
use super::drops::Drops;
use super::recognition::TraitCall;
use super::representation::{const_js, eval_const};
use super::{FnCx, R, lower_first};
use crate::js::{self, Expr, Op, Prop, StmtKind};
use crate::runtime::Helper;
use rustc_hir::def::DefKind;
use rustc_hir::{LangItem, Mutability};
use rustc_middle::traits::ImplSource;
use rustc_middle::ty::{self, Ty, TyCtxt, TypeVisitableExt};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, sym};
use std::collections::HashMap;

/// A trait whose bounds take dictionaries (ADR 0049): the crate's own, and
/// the std ones rust-js has dictionaries for (ADR 0052).
pub(super) use super::recognition::operational;

/// A trait the crate may implement. `From` and `TryFrom` have no
/// dictionaries: their impls are only called where the types are known
/// (ADR 0052). `Eq` has no
/// methods: a `T: Eq` bound is its `PartialEq` (ADR 0053). An `Iterator` is a
/// JS iterator, and has no dictionaries either (ADR 0055). An operator's
/// impl is called where `a + b` is, with the types known (ADR 0064).
pub(super) use super::recognition::implementable;

/// `Add`, `Neg`, `AddAssign` and the like: what `a + b`, `-a` and
/// `a += b` call on a type of the crate's own.
pub(super) use super::recognition::is_operator;

pub(super) fn validate(tcx: TyCtxt<'_>, foreign: &super::library::Foreign<'_, '_>) -> bool {
    let mut valid = true;
    for id in tcx.hir_crate_items(()).definitions() {
        if bindings::is_binding(tcx, id.to_def_id()) {
            continue;
        }
        let kind = tcx.def_kind(id);
        if !matches!(
            kind,
            DefKind::Trait | DefKind::Fn | DefKind::AssocFn | DefKind::Impl { .. }
        ) {
            continue;
        }
        // A derived impl's methods are never lowered: `#[derive(Hash)]`'s
        // generic `hash<H>` is no reason to reject the crate.
        let derived = |id: rustc_span::def_id::LocalDefId| tcx.is_automatically_derived(id.to_def_id());
        if derived(id)
            || (kind == DefKind::AssocFn && tcx.opt_local_parent(id).is_some_and(derived))
            || super::analysis::from_serde_derive(tcx, id)
        {
            continue;
        }
        let params = &tcx.generics_of(id).own_params;
        // A function's and an impl's are given their values (ADR 0107). Not
        // a trait's, or a trait method's own, which its dictionary would be
        // given too.
        let reason = if params
            .iter()
            .any(|p| matches!(p.kind, ty::GenericParamDefKind::Const { .. }))
            && (kind == DefKind::Trait
                || (kind == DefKind::AssocFn && tcx.inherent_impl_of_assoc(id.to_def_id()).is_none()))
        {
            Some("const generics of traits and their methods")
        } else if kind == DefKind::AssocFn
            && tcx.inherent_impl_of_assoc(id.to_def_id()).is_none()
            && params
                .iter()
                .any(|p| !matches!(p.kind, ty::GenericParamDefKind::Lifetime))
            && may_have_destructors(tcx, foreign)
        {
            // Called through a dictionary, it's given no drop function for its
            // own type parameters (ADR 0106).
            Some("generic trait methods, where a type may have a destructor")
        } else {
            None
        };
        if let Some(reason) = reason {
            tcx.dcx()
                .span_err(tcx.def_span(id), format!("rust-js does not support {reason} yet"));
            valid = false;
        }
        if kind == DefKind::Trait {
            let mut names = std::collections::HashSet::new();
            let identity = ty::GenericArgs::identity_for_item(tcx, id);
            for (name, tr, span) in supertraits(tcx, id.to_def_id(), identity) {
                if operational(tcx, foreign, tr.def_id) && (name == "__proto__" || !names.insert(name)) {
                    tcx.dcx().span_err(span, "rust-js: supertrait dictionary names collide");
                    valid = false;
                }
            }
            for item in tcx.associated_items(id).in_definition_order() {
                // The type rustc makes of an `async fn`'s future has no name,
                // and no place in a dictionary; nor has an associated type.
                if item.is_impl_trait_in_trait() || tcx.def_kind(item.def_id) == DefKind::AssocTy {
                    continue;
                }
                let name = bindings::fn_name(tcx, item.def_id);
                if name == "__proto__" || !names.insert(name) {
                    tcx.dcx().span_err(
                        tcx.def_span(item.def_id),
                        "rust-js: trait dictionary names collide or use reserved `__proto__`",
                    );
                    valid = false;
                }
            }
            for (name, _) in item_bounds(tcx, id.to_def_id(), identity) {
                if !names.insert(name) {
                    tcx.dcx()
                        .span_err(tcx.def_span(id), "rust-js: trait dictionary names collide");
                    valid = false;
                }
            }
        }
    }
    valid
}

/// `id`'s const parameters, its parent's first, as rustc numbers them.
fn const_params(tcx: TyCtxt<'_>, id: DefId) -> Vec<&ty::GenericParamDef> {
    let generics = tcx.generics_of(id);
    (0..generics.count())
        .map(|index| generics.param_at(index, tcx))
        .filter(|param| matches!(param.kind, ty::GenericParamDefKind::Const { .. }))
        .collect()
}

/// Signature order, including parent impl bounds. Never depend on body usage.
pub(super) fn bounds<'tcx>(
    tcx: TyCtxt<'tcx>,
    foreign: &super::library::Foreign<'_, 'tcx>,
    id: DefId,
) -> Vec<ty::TraitRef<'tcx>> {
    let mut result = Vec::new();
    if let Some(trait_id) = tcx.trait_of_assoc(id)
        && operational(tcx, foreign, trait_id)
    {
        result.push(ty::TraitRef::identity(tcx, trait_id));
    }
    for (clause, _) in tcx.predicates_of(id).instantiate_identity(tcx) {
        let clause = clause.skip_normalization();
        if let Some(tr) = bound_of(tcx, foreign, clause, id)
            && !result.contains(&tr)
        {
            result.push(tr);
        }
    }
    result
}

/// Might any value have a destructor: has the crate a `Drop` impl of its own,
/// or a library, whose types might? Where it hasn't, what a caller gives
/// generic code without a drop function has nothing to drop (ADR 0106).
pub(super) fn may_have_destructors(tcx: TyCtxt<'_>, foreign: &super::library::Foreign<'_, '_>) -> bool {
    let drop_trait = tcx.lang_items().drop_trait();
    drop_trait.is_some_and(|id| tcx.all_local_trait_impls(()).contains_key(&id)) || foreign.any()
}

/// A clause of a function's, as its evidence is for it, if it's given one.
fn bound_of<'tcx>(
    tcx: TyCtxt<'tcx>,
    foreign: &super::library::Foreign<'_, 'tcx>,
    clause: ty::Clause<'tcx>,
    id: DefId,
) -> Option<ty::TraitRef<'tcx>> {
    // A higher-ranked bound, `for<'a> T: Foo<'a>`, is one dictionary:
    // lifetimes aren't in the JS, so its own are erased, not left bound.
    let ty::ClauseKind::Trait(predicate) = tcx.instantiate_bound_regions_with_erased(clause.kind()) else {
        return None;
    };
    let mut tr = predicate.trait_ref;
    // `Eq` promises more than `PartialEq`, but it's `PartialEq`'s `eq`
    // that's called.
    if tcx.is_diagnostic_item(sym::Eq, tr.def_id) {
        let partial_eq = tcx.require_lang_item(LangItem::PartialEq, tcx.def_span(id));
        tr = ty::TraitRef::new(tcx, partial_eq, [tr.self_ty(), tr.self_ty()]);
    }
    operational(tcx, foreign, tr.def_id).then_some(tr)
}

/// The bounds of a function's own type parameters, which end its `bounds`:
/// a trait's generic method's, `T: Display` of `describe<T: Display>`, which
/// a caller through a dictionary gives where it calls, after its arguments,
/// in the trait's order, where an impl's are given when its dictionary is
/// made (ADR 0106).
pub(super) fn own_bounds<'tcx>(
    tcx: TyCtxt<'tcx>,
    foreign: &super::library::Foreign<'_, 'tcx>,
    id: DefId,
) -> Vec<ty::TraitRef<'tcx>> {
    let own: Vec<_> = tcx
        .predicates_of(id)
        .predicates
        .iter()
        .filter_map(|&(clause, _)| bound_of(tcx, foreign, clause, id))
        .collect();
    bounds(tcx, foreign, id)
        .into_iter()
        .filter(|tr| own.contains(tr))
        .collect()
}

/// The type, then the trait, then the trait's arguments other than their
/// defaults: `circleShape`, `metersFromF64` for `impl From<f64> for
/// Meters`, and `versionPartialEq`, whose `Rhs` is `Self` (ADR 0052).
pub(super) fn impl_name(tcx: TyCtxt<'_>, id: DefId) -> String {
    let tr = tcx.impl_trait_ref(id).instantiate_identity().skip_normalization();
    format!(
        "{}{}",
        lower_first(&js_word(&type_word(tcx, tr.self_ty()))),
        trait_word(tcx, tr)
    )
}

/// A trait's supertraits, as its dictionary has them: each one's key, its
/// trait's name, `PartialEq`, or with its arguments as the trait declares
/// them where it has two of one trait, `LabelU32` and `LabelString` of
/// `trait Both: Label<u32> + Label<String>`, so that a generic impl's
/// dictionary and its caller agree; and the supertrait with
/// `args`, its own lifetimes erased: a `for<'a> B<&'a ()>` is one
/// dictionary, and rustc's trait selection takes no bound ones (ADR 0106).
pub(super) fn supertraits<'tcx>(
    tcx: TyCtxt<'tcx>,
    trait_id: DefId,
    args: ty::GenericArgsRef<'tcx>,
) -> Vec<(String, ty::TraitRef<'tcx>, Span)> {
    let predicates = tcx.explicit_super_predicates_of(trait_id);
    let found: Vec<_> = predicates
        .iter_identity_copied()
        .map(|item| item.skip_normalization())
        .zip(
            predicates
                .iter_instantiated_copied(tcx, args)
                .map(|item| item.skip_normalization()),
        )
        .filter_map(|((declared, span), (instantiated, _))| {
            let ty::ClauseKind::Trait(declared) = declared.kind().skip_binder() else {
                return None;
            };
            let ty::ClauseKind::Trait(instantiated) = tcx.instantiate_bound_regions_with_erased(instantiated.kind())
            else {
                return None;
            };
            Some((declared.trait_ref, instantiated.trait_ref, span))
        })
        .collect();
    let twice = |id: DefId| found.iter().filter(|(declared, _, _)| declared.def_id == id).count() > 1;
    found
        .iter()
        .map(|&(declared, instantiated, span)| {
            let name = match twice(declared.def_id) {
                true => trait_word(tcx, declared),
                false => tcx.item_name(declared.def_id).to_string(),
            };
            (name, instantiated, span)
        })
        .collect()
}

/// The bounds a trait declares on its associated types, as its dictionary
/// has them: each one's key, the type's name and the bound's as the trait
/// declares it, `LabelDisplay` of `type Label: Display`, and the bound with
/// `args`, the trait's. Generic code finds `<L as Labeled>::Label: Display`
/// in `L`'s `Labeled`, as rustc proves it from the trait (ADR 0106).
pub(super) fn item_bounds<'tcx>(
    tcx: TyCtxt<'tcx>,
    trait_id: DefId,
    args: ty::GenericArgsRef<'tcx>,
) -> Vec<(String, ty::TraitRef<'tcx>)> {
    let mut found = Vec::new();
    for item in tcx.associated_items(trait_id).in_definition_order() {
        // A generic associated type's are its own parameters' too: those are
        // refused (`validate`).
        if tcx.def_kind(item.def_id) != DefKind::AssocTy
            || item.is_impl_trait_in_trait()
            || !tcx.generics_of(item.def_id).own_params.is_empty()
        {
            continue;
        }
        let bounds = tcx.explicit_item_bounds(item.def_id);
        for ((declared, _), (instantiated, _)) in
            bounds.iter_identity_copied().map(|item| item.skip_normalization()).zip(
                bounds
                    .iter_instantiated_copied(tcx, args)
                    .map(|item| item.skip_normalization()),
            )
        {
            let ty::ClauseKind::Trait(declared) = declared.kind().skip_binder() else {
                continue;
            };
            let ty::ClauseKind::Trait(instantiated) = tcx.instantiate_bound_regions_with_erased(instantiated.kind())
            else {
                continue;
            };
            let name = format!("{}{}", tcx.item_name(item.def_id), trait_word(tcx, declared.trait_ref));
            found.push((name, instantiated.trait_ref));
        }
    }
    found
}

/// A type as a word of an evidence name: a type parameter's, `T`, or an
/// associated type's of one, `SItem` of `<S as Source>::Item`.
fn evidence_word<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> String {
    match ty.kind() {
        ty::Alias(
            _,
            alias @ ty::AliasTy {
                kind: ty::Projection { def_id },
                ..
            },
        ) => {
            format!("{}{}", evidence_word(tcx, alias.self_ty()), tcx.item_name(*def_id))
        }
        // `name: impl Into<String>`'s, which rustc names as it's written: its
        // trait's word alone, `IntoString`.
        ty::Param(p) if p.name.as_str().starts_with("impl ") => String::new(),
        _ => ty.to_string(),
    }
}

/// A type as a word of a JS name: an ADT's own name, `Meters` of
/// `Meters<T>`.
fn type_word<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> String {
    match ty.kind() {
        ty::Adt(adt, _) => tcx.item_name(adt.did()).to_string(),
        _ => ty.to_string(),
    }
}

/// The trait and its arguments other than their defaults, as a word of a JS
/// name: `ConvertF64` of `Convert<f64>`, `PartialEq` of `PartialEq<Self>`.
fn trait_word<'tcx>(tcx: TyCtxt<'tcx>, tr: ty::TraitRef<'tcx>) -> String {
    let mut name = tcx.item_name(tr.def_id).to_string();
    let generics = tcx.generics_of(tr.def_id);
    for (param, arg) in generics.own_params.iter().zip(tr.args).skip(1) {
        let Some(arg) = arg.as_type() else { continue };
        if param
            .default_value(tcx)
            .map(|d| d.instantiate(tcx, tr.args).skip_normalization())
            == Some(arg.into())
        {
            continue;
        }
        let arg = js_word(&type_word(tcx, arg.peel_refs()));
        let mut chars = arg.chars();
        name.extend(chars.next().map(|c| c.to_ascii_uppercase()));
        name.extend(chars);
    }
    name
}

/// A type's name as part of a JS name: what isn't a letter or a digit is
/// `_`, so `Vec<T>` is `Vec_T_`.
fn js_word(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    pub(super) fn evidence_params(&mut self, id: DefId) -> Vec<js::Pattern> {
        // Each const parameter's value first, `N`, in the order they're
        // declared, the impl's before the method's (ADR 0107).
        let mut params: Vec<js::Pattern> = Vec::new();
        for param in const_params(self.tcx, id) {
            let name = self.fresh(param.name.as_str());
            self.const_params.push((param.index, Expr::var(&name)));
            params.push(name.into());
        }
        params.extend(bounds(self.tcx, self.krate.foreign, id).into_iter().map(|tr| {
            // `writeT` and `readT`, as a generic codec's (ADR 0081).
            let name = match super::serde::serde_trait(self.tcx, tr.def_id) {
                Some(true) => format!("write{}", tr.self_ty()),
                Some(false) => format!("read{}", tr.self_ty()),
                // `XConvertF64` and `XConvertString`, of two impls of one trait.
                None => format!("{}{}", evidence_word(self.tcx, tr.self_ty()), trait_word(self.tcx, tr)),
            };
            let name = self.fresh(&js_word(&name));
            self.evidence.push((tr, Expr::var(&name)));
            js::Pattern::from(name)
        }));
        // Then a drop function for each type parameter a caller gives a value
        // with a destructor, `dropT` (ADR 0098).
        for &index in self.krate.drop_params.get(&id).into_iter().flatten() {
            let param = self.tcx.generics_of(id).param_at(index as usize, self.tcx);
            let name = self.fresh(&format!("drop{}", param.name));
            self.give_drop_param(index, name.clone());
            params.push(name.into());
        }
        params
    }

    /// A const argument's value (ADR 0107): `3`, or the caller's own `N`.
    pub(super) fn const_arg(&self, c: ty::Const<'tcx>, span: Span) -> R<Expr> {
        let c = self
            .tcx
            .normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(c));
        let value = match c.kind() {
            ty::ConstKind::Param(p) => self
                .const_params
                .iter()
                .find(|&&(index, _)| index == p.index)
                .map(|(_, value)| value.clone()),
            _ => c.try_to_value().and_then(|value| const_js(self.tcx, value)),
        };
        value.ok_or_else(|| self.unsupported(span, "this const argument"))
    }

    fn super_evidence(&self, from: ty::TraitRef<'tcx>, to: ty::TraitRef<'tcx>, value: Expr) -> Option<Expr> {
        if self.tcx.erase_and_anonymize_regions(from) == self.tcx.erase_and_anonymize_regions(to) {
            return Some(value);
        }
        // A std trait's dictionary, like `Copy`'s, has no supertraits in it.
        if !self.is_rust_trait(from.def_id) {
            return None;
        }
        for (name, tr, _) in supertraits(self.tcx, from.def_id, from.args) {
            let parent = Expr::call(Expr::member(value.clone(), name), Vec::new());
            if let Some(found) = self.super_evidence(tr, to, parent) {
                return Some(found);
            }
        }
        None
    }

    /// The dictionary for `tr` among those this function was given, or
    /// a supertrait's of one.
    pub(super) fn evidence_for(&self, tr: ty::TraitRef<'tcx>) -> Option<Expr> {
        self.evidence
            .iter()
            .find_map(|(bound, value)| self.super_evidence(*bound, tr, value.clone()))
            .or_else(|| self.item_evidence(tr))
    }

    /// `<L as Labeled>::Label: Display`, which the trait declares, from `L`'s
    /// `Labeled`: its `LabelDisplay`, or a supertrait's of it (ADR 0106).
    fn item_evidence(&self, tr: ty::TraitRef<'tcx>) -> Option<Expr> {
        let ty::Alias(
            _,
            alias @ ty::AliasTy {
                kind: ty::Projection { .. },
                ..
            },
        ) = *tr.self_ty().kind()
        else {
            return None;
        };
        let owner = alias.trait_ref(self.tcx);
        let dictionary = self.evidence_for(owner)?;
        item_bounds(self.tcx, owner.def_id, owner.args)
            .into_iter()
            .find_map(|(name, bound)| {
                let found = Expr::call(Expr::member(dictionary.clone(), name), Vec::new());
                self.super_evidence(bound, tr, found)
            })
    }

    pub(super) fn dictionary(&mut self, tr: ty::TraitRef<'tcx>, span: Span) -> R<Expr> {
        // `<Words as Source>::Item: Debug` of a bound instantiated: `String: Debug`.
        let tr = self
            .tcx
            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(tr))
            .unwrap_or(tr);
        // serde's: the function that writes or reads the type (ADR 0081).
        match super::serde::serde_trait(self.tcx, tr.def_id) {
            Some(true) => return self.json_writer(tr.self_ty(), span),
            Some(false) => return self.json_reader(tr.self_ty(), span),
            None => {}
        }
        if let Some(found) = self.evidence_for(tr) {
            return Ok(found);
        }
        // Derived and std impls of `Default` and `Clone` (ADR 0052).
        let ty = tr.self_ty();
        let default = self.tcx.is_diagnostic_item(Symbol::intern("Default"), tr.def_id);
        let clone = self.tcx.is_lang_item(tr.def_id, LangItem::Clone);
        let eq = self.tcx.is_lang_item(tr.def_id, LangItem::PartialEq);
        let display = tr.def_id == self.display_trait();
        let debug = tr.def_id == self.debug_trait();
        let ord = tr.def_id == self.ord_trait();
        let partial_ord = tr.def_id == self.partial_ord_trait();
        if (default || clone || eq || display || debug || ord || partial_ord) && !self.has_user_impl(tr.def_id, ty) {
            if debug {
                let mut body = Vec::new();
                let shown = self.debug_string(Expr::var("value"), ty, span)?;
                body.push(StmtKind::Return(Some(shown)).at(js::Span::NONE));
                let fmt = Expr::arrow(vec!["value".into()], body);
                return Ok(Expr::object(vec![Prop::Field("fmt".into(), fmt)]));
            }
            if ord || partial_ord {
                let (name, compare) = if ord {
                    ("cmp", self.cmp_fn(ty, false, span)?)
                } else {
                    ("partial_cmp", self.cmp_fn(ty, true, span)?)
                };
                return Ok(Expr::object(vec![Prop::Field(name.into(), compare)]));
            }
            if display {
                let fmt = self.display_fn(ty, span)?;
                return Ok(Expr::object(vec![Prop::Field("fmt".into(), fmt)]));
            }
            if eq {
                let eq = self.eq_fn(ty, span)?;
                return Ok(Expr::object(vec![Prop::Field("eq".into(), eq)]));
            }
            if default {
                let value = self.default_value(ty, span)?;
                return Ok(Expr::object(vec![Prop::Field(
                    "default".into(),
                    Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(value)).at(js::Span::NONE)]),
                )]));
            }
            if clone {
                let clone = self.clone_fn("value", ty, span)?;
                return Ok(Expr::object(vec![Prop::Field("clone".into(), clone)]));
            }
        }
        // A number's `+` or `-`, as `a + b` of one is (ADR 0108), of a
        // number on each side: `impl Add<Meters> for f64` is the crate's.
        let primitive =
            |t: Ty<'tcx>| super::representation::Num::of(t.peel_refs()).is_some() || t.peel_refs().is_bool();
        if let Some(op) = super::recognition::value_operator(self.tcx, tr.def_id)
            && tr.args.types().all(primitive)
        {
            let ty = ty.peel_refs();
            let (params, value) = match op {
                Ok(op) => {
                    // A shift's amount of its own type, `Shl<u64>` of a `u32`.
                    let b = super::numbers::shift_amount_of(op, Expr::var("b"), ty, tr.args.type_at(1));
                    (
                        vec!["a".into(), "b".into()],
                        self.binary(op, Expr::var("a"), b, None, ty, span)?,
                    )
                }
                Err(op) => (vec!["value".into()], self.unary(op, Expr::var("value"), ty, span)?),
            };
            // Its method, `add`, after its `Output`.
            let method = self
                .tcx
                .associated_items(tr.def_id)
                .in_definition_order()
                .find(|item| item.is_fn())
                .expect("an operator trait has a method");
            let name = bindings::fn_name(self.tcx, method.def_id);
            return Ok(Expr::object(vec![Prop::Field(
                name,
                Expr::arrow(params, vec![StmtKind::Return(Some(value)).at(js::Span::NONE)]),
            )]));
        }
        // `x.into()` of a `T: Into<U>` (ADR 0108): std's conversion, the
        // crate's `From`, or of a `T` to itself, the value.
        if self.tcx.is_diagnostic_item(sym::Into, tr.def_id) {
            let into = self.tcx.associated_item_def_ids(tr.def_id)[0];
            let target = tr.args.type_at(1);
            let from = self.tcx.get_diagnostic_item(sym::From).expect("std has `From`");
            let from = self.tcx.associated_item_def_ids(from)[0];
            let from_args = self.tcx.mk_args(&[target.into(), ty.into()]);
            let value = if let Some(known) = self.recognition().classify(into, tr.args)
                && let Some(f) = self.std_fn_value(known, Ty::new_fn_def(self.tcx, into, tr.args), span)?
            {
                f
            } else if let Some(instance) = self.resolve_instance(from, from_args)?
                && self.is_rust_fn(instance.def_id())
                && self.tcx.trait_of_assoc(instance.def_id()).is_none()
            {
                let callee = self.fn_ref(instance.def_id());
                let mut values = vec![Expr::var("value")];
                values.extend(self.evidence_args(instance.def_id(), instance.args, span)?);
                match values.len() {
                    1 => callee,
                    _ => Expr::arrow(
                        vec!["value".into()],
                        vec![StmtKind::Return(Some(Expr::call(callee, values))).at(js::Span::NONE)],
                    ),
                }
            } else if self.tcx.erase_and_anonymize_regions(ty) == self.tcx.erase_and_anonymize_regions(target) {
                Expr::arrow(
                    vec!["value".into()],
                    vec![StmtKind::Return(Some(Expr::var("value"))).at(js::Span::NONE)],
                )
            } else {
                return Err(self.unsupported(span, &format!("implementation evidence for `{tr}`")));
            };
            return Ok(Expr::object(vec![Prop::Field("into".into(), value)]));
        }
        if self.tcx.is_lang_item(tr.def_id, LangItem::Copy) {
            let ty = tr.self_ty();
            if self.is_unknown(ty) {
                return Err(self.unsupported(span, "Copy without representation evidence"));
            }
            let copy = self.copy(Expr::var("value"), ty);
            return Ok(Expr::object(vec![Prop::Field(
                "copy".into(),
                Expr::arrow(
                    vec!["value".into()],
                    vec![StmtKind::Return(Some(copy)).at(js::Span::NONE)],
                ),
            )]));
        }
        let selected = self.tcx.codegen_select_candidate(self.typing_env.as_query_input(tr));
        // The crate's own impl's accessor, or one a library exports (ADR 0100).
        if let Ok(ImplSource::UserDefined(imp)) = selected
            && (self.krate.trait_impls.contains(&imp.impl_def_id) || self.krate.foreign.item(imp.impl_def_id).is_some())
        {
            let callee = self.fn_ref(imp.impl_def_id);
            let args = self.evidence_args(imp.impl_def_id, imp.args, span)?;
            return Ok(Expr::call(callee, args));
        }
        Err(self.unsupported(span, &format!("implementation evidence for `{tr}`")))
    }

    pub(super) fn evidence_args(&mut self, id: DefId, args: ty::GenericArgsRef<'tcx>, span: Span) -> R<Vec<Expr>> {
        let mut values = const_params(self.tcx, id)
            .into_iter()
            .map(|param| self.const_arg(args.const_at(param.index as usize), span))
            .collect::<R<Vec<_>>>()?;
        for bound in bounds(self.tcx, self.krate.foreign, id) {
            let bound = ty::EarlyBinder::bind(self.tcx, bound)
                .instantiate(self.tcx, args)
                .skip_normalization();
            values.push(self.dictionary(bound, span)?);
        }
        // Each drop function it takes: a type's with nothing to drop is none,
        // left out at the end (ADR 0098).
        let mut drops = Vec::new();
        let given = match self.krate.foreign.item(id) {
            Some(item) => item.drops.as_slice(),
            None => self.krate.drop_params.get(&id).map_or(&[][..], Vec::as_slice),
        };
        for &index in given {
            drops.push(self.drop_function(args.type_at(index as usize), span)?);
        }
        while matches!(drops.last(), Some(None)) {
            drops.pop();
        }
        values.extend(drops.into_iter().map(|drop| drop.unwrap_or_else(Expr::undefined)));
        Ok(values)
    }

    /// Select user code before std intrinsics, so custom implementations win.
    /// A trait method call, or `None` if it isn't one rust-js dispatches.
    /// `out` gets what must run first, like a receiver computed once.
    pub(super) fn trait_call(
        &mut self,
        id: DefId,
        generic_args: ty::GenericArgsRef<'tcx>,
        values: Vec<Expr>,
        span: Span,
        out: &mut Vec<js::Stmt>,
    ) -> R<Option<Expr>> {
        let Some(trait_id) = self.tcx.trait_of_assoc(id) else {
            return Ok(None);
        };
        if self.tcx.fn_trait_kind_from_def_id(trait_id).is_some() {
            return Ok(None);
        }
        // In a copied default, `Self` is the impl's type: a call on it
        // resolves to the impl's method, called directly.
        let generic_args = match self.self_args {
            Some(args) => ty::EarlyBinder::bind(self.tcx, generic_args)
                .instantiate(self.tcx, args)
                .skip_normalization(),
            None => generic_args,
        };
        let tr = ty::TraitRef::from_assoc(self.tcx, trait_id, generic_args);
        if matches!(tr.self_ty().kind(), ty::Dynamic(..)) && operational(self.tcx, self.krate.foreign, trait_id) {
            let mut values = values;
            let receiver = values.remove(0);
            // The pair is read twice, so one with effects goes in a `const`
            // first. It's still first: `operands` put anything before it that
            // needed statements in `const`s of its own.
            let pair = if receiver.has_effects() {
                self.spill("receiver", receiver, out)
            } else {
                receiver
            };
            let principal = self.dyn_trait_ref(tr.self_ty(), tr.self_ty()).unwrap();
            let dictionary = self
                .super_evidence(principal, tr, Expr::member(pair.clone(), "impl"))
                .ok_or_else(|| self.unsupported(span, "this trait object supertrait"))?;
            // A `&mut self` method is given the pair, whose `value` a box's is: a
            // number's impl writes the place it reads (ADR 0099).
            let receiver = self
                .tcx
                .fn_sig(id)
                .instantiate_identity()
                .skip_normalization()
                .skip_binder()
                .inputs()[0];
            let this = match receiver.kind() {
                ty::Ref(_, _, Mutability::Mut) => pair,
                _ => Expr::member(pair, "value"),
            };
            values.insert(0, this);
            values.extend(self.own_evidence(id, generic_args, span)?);
            return Ok(Some(Expr::call(
                Expr::member(dictionary, bindings::fn_name(self.tcx, id)),
                values,
            )));
        }
        if let Some(instance) = self.resolve_instance(id, generic_args)?
            && self.is_rust_fn(instance.def_id())
            && self.tcx.trait_of_assoc(instance.def_id()).is_none()
        {
            let mut values = values;
            values.extend(self.evidence_args(instance.def_id(), instance.args, span)?);
            return Ok(Some(Expr::call(self.fn_ref(instance.def_id()), values)));
        }
        // What rust-js writes itself, in place: `c.clone()` of a struct is a
        // copy of it, not a dictionary's `clone` (ADR 0052).
        let known = self.recognition().trait_call(id, trait_id);
        if matches!(known, Some(TraitCall::Clone)) {
            let mut values = values;
            return Ok(Some(self.clone_value(values.remove(0), tr.self_ty(), span, out)?));
        }
        if matches!(known, Some(TraitCall::Default)) {
            return Ok(Some(self.default_value(tr.self_ty(), span)?));
        }
        // `a != b` is `!(a == b)`, as Rust requires them to agree (ADR 0053).
        if let Some(TraitCall::Equality { negate }) = known {
            let [a, b]: [Expr; 2] = values.try_into().map_err(|_| self.unsupported(span, "this `==`"))?;
            // A hand-written `PartialEq<Rhs>` is its own `eq`, whatever `Rhs` is.
            let eq = if self.is_user_impl(tr) {
                let eq = self.tcx.associated_item_def_ids(trait_id)[0];
                self.impl_call(eq, tr.args, vec![a, b], span)?
            } else {
                self.eq_value(a, b, tr.self_ty(), span, out)?
            };
            return Ok(Some(if negate { super::std_impls::negate(eq) } else { eq }));
        }
        // `a < b`, `a.cmp(&b)`, `a.max(b)` (ADR 0057). Of numbers, they're
        // std's operators and `Math.max`, as before.
        let ordering = matches!(known, Some(TraitCall::Ordering));
        if let Some(call) = self.ordering_call(id, tr, values.clone(), span, out)? {
            return Ok(Some(call));
        }
        if ordering && super::representation::Num::of(tr.self_ty().peel_refs()).is_some() {
            return Ok(None);
        }
        // Where the types are known, `"paren".into()` and `a.add(b)` are std's,
        // written in place, as ever: a dictionary is for generic code (ADR 0108).
        if (super::recognition::value_operator(self.tcx, trait_id).is_some()
            || self.tcx.is_diagnostic_item(sym::Into, trait_id))
            && !tr.args.has_non_region_param()
        {
            return Ok(None);
        }
        // A std trait's dictionary has only its required methods.
        if operational(self.tcx, self.krate.foreign, trait_id)
            && !self.is_rust_trait(trait_id)
            && self.tcx.defaultness(id).has_value()
        {
            let what = format!("calling `{}`", self.tcx.def_path_str(id));
            return Err(self.unsupported(span, &what));
        }
        if operational(self.tcx, self.krate.foreign, trait_id) {
            let dictionary = self.dictionary(tr, span)?;
            let mut values = values;
            values.extend(self.own_evidence(id, generic_args, span)?);
            return Ok(Some(Expr::call(
                Expr::member(dictionary, bindings::fn_name(self.tcx, id)),
                values,
            )));
        }
        Ok(None)
    }

    /// What a trait's generic method is given where it's called through a
    /// dictionary, after its arguments: its own bounds' evidence, for this call
    /// (ADR 0106). None for one that isn't generic.
    fn own_evidence(&mut self, id: DefId, generic_args: ty::GenericArgsRef<'tcx>, span: Span) -> R<Vec<Expr>> {
        own_bounds(self.tcx, self.krate.foreign, id)
            .into_iter()
            .map(|bound| {
                self.dictionary(
                    ty::EarlyBinder::bind(self.tcx, bound)
                        .instantiate(self.tcx, generic_args)
                        .skip_normalization(),
                    span,
                )
            })
            .collect()
    }

    /// `evidence_args` of an impl's generic method, for its dictionary's entry:
    /// its own bounds' evidence is the entry's caller's, `names`, given for the
    /// trait's, `declared`, each passed on as the impl's bound it is, which may
    /// be in another order; the impl's own are this dictionary's (ADR 0106).
    fn method_evidence(
        &mut self,
        method: DefId,
        args: ty::GenericArgsRef<'tcx>,
        declared: &[ty::TraitRef<'tcx>],
        names: &[String],
        span: Span,
    ) -> R<Vec<Expr>> {
        let own = own_bounds(self.tcx, self.krate.foreign, method);
        let mut values = Vec::new();
        for bound in bounds(self.tcx, self.krate.foreign, method) {
            let here = ty::EarlyBinder::bind(self.tcx, bound)
                .instantiate(self.tcx, args)
                .skip_normalization();
            let here = self.tcx.erase_and_anonymize_regions(here);
            if own.contains(&bound) {
                let at = declared
                    .iter()
                    .position(|&d| self.tcx.erase_and_anonymize_regions(d) == here)
                    .ok_or_else(|| {
                        self.unsupported(span, "a generic method whose bound isn't one its trait declares")
                    })?;
                values.push(Expr::var(&names[at]));
            } else {
                values.push(self.dictionary(here, span)?);
            }
        }
        // Its drops are the impl's type parameters' (ADR 0098): its own, a
        // caller through a dictionary gives none of, are refused (`validate`).
        let mut drops = Vec::new();
        for &index in self.krate.drop_params.get(&method).map_or(&[][..], Vec::as_slice) {
            drops.push(self.drop_function(args.type_at(index as usize), span)?);
        }
        while matches!(drops.last(), Some(None)) {
            drops.pop();
        }
        values.extend(drops.into_iter().map(|drop| drop.unwrap_or_else(Expr::undefined)));
        Ok(values)
    }

    /// A trait rust-js compiled: the crate's own, or a library's (ADR 0100).
    /// Any other is std's, whose dictionaries rust-js makes as it knows them.
    pub(super) fn is_rust_trait(&self, id: DefId) -> bool {
        id.is_local() || self.krate.foreign.in_library(id)
    }

    pub(super) fn dynamic_trait(&self, ty: Ty<'tcx>) -> Option<DefId> {
        let inner = self.pointee(ty);
        match inner.kind() {
            ty::Dynamic(predicates, ..) => predicates.principal_def_id().filter(|&id| self.is_rust_trait(id)),
            _ => None,
        }
    }

    fn dyn_trait_ref(&self, ty: Ty<'tcx>, self_ty: Ty<'tcx>) -> Option<ty::TraitRef<'tcx>> {
        match self.pointee(ty).kind() {
            // `dyn for<'a> AsStr<'a, 'a>`: its lifetimes erased, as a
            // dictionary is the same for any.
            ty::Dynamic(predicates, ..) => predicates.principal().map(|p| {
                self.tcx
                    .instantiate_bound_regions_with_erased(p.with_self_ty(self.tcx, self_ty))
            }),
            _ => None,
        }
    }

    fn pointee(&self, ty: Ty<'tcx>) -> Ty<'tcx> {
        match ty.kind() {
            ty::Ref(_, inner, _) => *inner,
            ty::Adt(_, args) if self.is_std_wrapper(ty) => args.type_at(0),
            _ => ty,
        }
    }

    /// `x as &dyn Trait`: `{ value, impl }`, or for a trait object, the same
    /// value with its supertrait's dictionary. `out` gets a value computed once.
    pub(super) fn unsize_trait(
        &mut self,
        source: Ty<'tcx>,
        target: Ty<'tcx>,
        value: Expr,
        span: Span,
        out: &mut Vec<js::Stmt>,
    ) -> R<Expr> {
        // A pointer of the crate's own, as `#[derive(CoercePointee)]` makes
        // one, would hold a `dyn`'s value and impl where it holds the value.
        if let ty::Adt(adt, _) = target.kind()
            && (adt.did().is_local() || self.krate.foreign.in_library(adt.did()))
        {
            return Err(self.unsupported(span, &format!("unsizing a `{target}`")));
        }
        // A `&dyn Debug` is the string it shows (ADR 0060).
        if self.is_dyn_debug(target) && !self.is_dyn_debug(source) {
            return self.debug_string(value, self.pointee(source), span);
        }
        // `&Fat<Bar>` to `&Fat<dyn ToBar>`: the struct's last field would
        // be a `dyn`'s value and impl, or a `dyn Debug`'s string, which it
        // isn't. One ending in a slice, or a `dyn FnMut`, the JS function,
        // is the same value either way.
        let pointee = self.pointee(target);
        let tail = self.tcx.struct_tail_for_codegen(pointee, self.typing_env);
        if pointee.is_adt()
            && matches!(tail.kind(), ty::Dynamic(traits, ..)
                if traits.principal_def_id().is_some_and(|id| self.is_rust_trait(id)) || self.is_dyn_debug(tail))
        {
            return Err(self.unsupported(span, &format!("a `{pointee}`, whose last field is a `dyn`")));
        }
        if self.dynamic_trait(target).is_none() {
            return Ok(value);
        }
        self.check_value_ty(target, span)?;
        if self.dynamic_trait(source).is_some() {
            let self_ty = self.pointee(source);
            let from = self.dyn_trait_ref(source, self_ty).unwrap();
            let to = self.dyn_trait_ref(target, self_ty).unwrap();
            // The same trait (a `Box<dyn T>` to a `Box<dyn T>`): the same pair.
            if self.tcx.erase_and_anonymize_regions(from) == self.tcx.erase_and_anonymize_regions(to) {
                return Ok(value);
            }
            // Read twice, so one with effects goes in a `const` first.
            let pair = if value.has_effects() {
                self.spill("receiver", value, out)
            } else {
                value
            };
            let dictionary = self
                .super_evidence(from, to, Expr::member(pair.clone(), "impl"))
                .ok_or_else(|| self.unsupported(span, "this trait upcast"))?;
            // A `&mut dyn Sub` as a `&mut dyn Super`: a pair on the first's
            // `value`, which its `&mut self` methods write (ADR 0099).
            if matches!(target.kind(), ty::Ref(_, _, Mutability::Mut)) {
                return Ok(Expr::pair(Expr::member(pair, "value"), dictionary));
            }
            return Ok(Expr::object(vec![
                Prop::Field("value".into(), Expr::member(pair, "value")),
                Prop::Field("impl".into(), dictionary),
            ]));
        }
        let tr = self.dyn_trait_ref(target, self.pointee(source)).unwrap();
        let dictionary = self.dictionary(tr, span)?;
        // `&mut n as &mut dyn Trait` of a number: the pair reads and writes
        // the place the cell does (ADR 0099). A temporary's box is the pair's.
        if self.is_cell(source) {
            return Ok(match value.kind {
                js::ExprKind::Handle(place) => Expr::pair(*place, dictionary),
                js::ExprKind::Object(mut props) if matches!(props.as_slice(), [Prop::Field(name, _)] if name == "value") =>
                {
                    props.push(Prop::Field("impl".into(), dictionary));
                    Expr::object(props)
                }
                _ => {
                    let cell = if value.reads_same() {
                        value
                    } else {
                        self.spill("cell", value, out)
                    };
                    Expr::pair(Expr::member(cell, "value"), dictionary)
                }
            });
        }
        Ok(Expr::object(vec![
            Prop::Field("value".into(), value),
            Prop::Field("impl".into(), dictionary),
        ]))
    }

    pub(super) fn lower_dictionary(&mut self, id: DefId, cache: &str) -> R<js::Function> {
        let span = self.tcx.def_span(id);
        // An impl of a generic trait, `Convert<f64>`, or of a std one,
        // `PartialEq<Rhs>`, is for its arguments: a dictionary of its own.
        let tr = self.tcx.impl_trait_ref(id).instantiate_identity().skip_normalization();
        let params = self.evidence_params(id);
        let mut props = Vec::new();
        for (name, supertrait, _) in supertraits(self.tcx, tr.def_id, tr.args) {
            if operational(self.tcx, self.krate.foreign, supertrait.def_id) {
                let dictionary = self.dictionary(supertrait, span)?;
                props.push(Prop::Field(
                    name,
                    Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(dictionary)).at(js::Span::NONE)]),
                ));
            }
        }
        // And each bound its trait declares on an associated type, the impl's:
        // `LabelDisplay` of `type Label = u32` is `u32`'s `Display`.
        for (name, bound) in item_bounds(self.tcx, tr.def_id, tr.args) {
            if operational(self.tcx, self.krate.foreign, bound.def_id) {
                let dictionary = self.dictionary(bound, span)?;
                props.push(Prop::Field(
                    name,
                    Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(dictionary)).at(js::Span::NONE)]),
                ));
            }
        }
        for item in self.tcx.associated_items(tr.def_id).in_definition_order() {
            // A constant, the impl's or the trait's default, as rustc computed
            // it: read on each use where it's of a type changed in place, so
            // each is a value of its own, as ADR 0031's are.
            if matches!(self.tcx.def_kind(item.def_id), DefKind::AssocConst { .. }) {
                // Only one generic code reads, here or where a library's consumers
                // may (ADR 0100).
                if !self.krate.library && !self.krate.generic_consts.contains(&item.def_id) {
                    continue;
                }
                let value = eval_const(self.tcx, self.typing_env, item.def_id, tr.args, span)
                    .and_then(|value| const_js(self.tcx, value))
                    .ok_or_else(|| self.unsupported(span, "a generic impl's constant of its parameters"))?;
                let ty = self
                    .tcx
                    .type_of(item.def_id)
                    .instantiate(self.tcx, tr.args)
                    .skip_normalization();
                let ty = self
                    .tcx
                    .normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(ty));
                let name = bindings::fn_name(self.tcx, item.def_id);
                props.push(match self.contains_mutated(ty) {
                    true => Prop::Getter(
                        name,
                        Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(value)).at(js::Span::NONE)]),
                    ),
                    false => Prop::Field(name, value),
                });
                continue;
            }
            if self.tcx.def_kind(item.def_id) != DefKind::AssocFn {
                continue;
            }
            // A std trait's provided methods, like `Clone::clone_from`,
            // aren't in its dictionary: nothing calls them through it.
            if !self.is_rust_trait(tr.def_id) && self.tcx.defaultness(item.def_id).has_value() {
                continue;
            }
            // The method's own parameters: lifetimes, `fn bar<'b>`, are erased,
            // as they aren't in the JS, but rustc resolves with them; a type,
            // `describe<T>`, is its caller's, the trait method's own (ADR 0106).
            let args = tr.args.extend_to(self.tcx, item.def_id, |param, _| match param.kind {
                ty::GenericParamDefKind::Lifetime => self.tcx.lifetimes.re_erased.into(),
                _ => self.tcx.mk_param_from_def(param),
            });
            let instance = self
                .resolve_instance(item.def_id, args)?
                .ok_or_else(|| self.unsupported(span, "this trait implementation"))?;
            let method = instance.def_id();
            // A library's trait's default, whose body is the library's (ADR 0100).
            if self.krate.foreign.in_library(method) {
                let what = format!(
                    "implementing another crate's trait without its default `{}`: write the method in the impl",
                    self.tcx.def_path_str(method)
                );
                return Err(self.unsupported(span, &what));
            }
            if !self.krate.fns.contains_key(&method) {
                return Err(self.unsupported(span, &format!("trait method `{}`", self.tcx.def_path_str(method))));
            }
            if self.tcx.trait_of_assoc(method).is_some() {
                let value = self.default_method(method, instance.args)?;
                props.push(Prop::Field(bindings::fn_name(self.tcx, item.def_id), value));
                continue;
            }
            let callee = self.fn_ref(method);
            // Without a `Formatter`, which isn't a JS parameter (ADR 0054).
            let count = self
                .tcx
                .fn_sig(method)
                .instantiate_identity()
                .skip_normalization()
                .skip_binder()
                .inputs()
                .len()
                - usize::from(self.formatter_param(method).is_some());
            let mut params: Vec<String> = (0..count).map(|i| format!("arg{i}")).collect();
            let mut values: Vec<Expr> = params.iter().map(|name| Expr::var(name)).collect();
            // A generic method's own evidence is its caller's, after the arguments.
            let declared: Vec<_> = own_bounds(self.tcx, self.krate.foreign, item.def_id)
                .into_iter()
                .map(|bound| {
                    ty::EarlyBinder::bind(self.tcx, bound)
                        .instantiate(self.tcx, args)
                        .skip_normalization()
                })
                .collect();
            let names: Vec<String> = declared
                .iter()
                .map(|&d| {
                    let word = format!("{}{}", evidence_word(self.tcx, d.self_ty()), trait_word(self.tcx, d));
                    self.fresh(&js_word(&word))
                })
                .collect();
            // A `&mut self` its caller through the dictionary gives in a box,
            // as a generic `&mut Self` is (ADR 0099), to a method that takes the
            // object itself: what's in it.
            for (i, value) in values.iter_mut().enumerate() {
                if self.param_is_box(item.def_id, i) && !self.param_is_box(method, i) {
                    *value = Expr::member(std::mem::replace(value, Expr::undefined()), "value");
                }
            }
            // Everything the method takes that its caller through the dictionary
            // doesn't give: its dictionaries, then its drops (ADR 0098), which
            // are this impl's own.
            let evidence = match declared.is_empty() {
                true => self.evidence_args(method, instance.args, span)?,
                false => self.method_evidence(method, instance.args, &declared, &names, span)?,
            };
            params.extend(names);
            values.extend(evidence);
            // One that passes on just what it's given, in order, is the method.
            let passed = values.len() == params.len()
                && values
                    .iter()
                    .zip(&params)
                    .all(|(value, param)| matches!(&value.kind, js::ExprKind::Var(v) if v == param));
            let value = if passed {
                callee
            } else {
                Expr::arrow(
                    params.into_iter().map(Into::into).collect(),
                    vec![StmtKind::Return(Some(Expr::call(callee, values))).at(js::Span::NONE)],
                )
            };
            props.push(Prop::Field(bindings::fn_name(self.tcx, item.def_id), value));
        }
        let object = Expr::object(props);
        let undefined = Expr::bin(Op::Eq, Expr::var(cache), Expr::undefined());
        let mut body = Vec::new();
        if params.is_empty() {
            body.push(
                StmtKind::If(
                    undefined,
                    vec![StmtKind::Assign(Expr::var(cache), object).at(js::Span::NONE)],
                    None,
                )
                .at(js::Span::NONE),
            );
            body.push(StmtKind::Return(Some(Expr::var(cache))).at(js::Span::NONE));
        } else {
            // Keyed by a const parameter's value first, a number, which only a
            // `Map` holds (ADR 0107).
            let map = if self.const_params.is_empty() { "WeakMap" } else { "Map" };
            body.push(
                StmtKind::If(
                    undefined,
                    vec![StmtKind::Assign(Expr::var(cache), Expr::new_(Expr::var(map), Vec::new())).at(js::Span::NONE)],
                    None,
                )
                .at(js::Span::NONE),
            );
            self.runtime.insert(Helper::TraitImpl);
            // One dictionary for each set of what it's given: its const
            // parameters' values, its dictionaries, and its drops, which may be
            // none.
            let keys = Expr::array(
                self.const_params
                    .iter()
                    .map(|(_, value)| value.clone())
                    .chain(self.evidence.iter().map(|(_, value)| value.clone()))
                    .chain(self.given_drops().iter().map(|name| Expr::var(name)))
                    .collect(),
            );
            let make = Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(object)).at(js::Span::NONE)]);
            body.push(
                StmtKind::Return(Some(Expr::call(
                    Expr::var("$traitImpl"),
                    vec![Expr::var(cache), keys, make],
                )))
                .at(js::Span::NONE),
            );
        }
        Ok(js::Function {
            name: self.krate.fns[&id].name.clone(),
            params,
            body,
            export: self.tcx.visibility(tr.def_id).is_public(),
            is_async: false,
            span: self.js_span(span),
            name_span: js::Span::NONE,
        })
    }

    /// Copy the default body into this implementation. Its Rust bindings still
    /// refer to the trait definition; only its evidence is specialized here.
    fn default_method(&mut self, id: DefId, args: ty::GenericArgsRef<'tcx>) -> R<Expr> {
        let span = self.tcx.def_span(id);
        let mut specialized = Vec::new();
        // A generic default's own evidence is its caller's, after its
        // arguments (ADR 0106); the rest is the impl's, made here.
        let own = own_bounds(self.tcx, self.krate.foreign, id);
        let mut own_params = Vec::new();
        for bound in bounds(self.tcx, self.krate.foreign, id) {
            if own.contains(&bound) {
                let word = format!(
                    "{}{}",
                    evidence_word(self.tcx, bound.self_ty()),
                    trait_word(self.tcx, bound)
                );
                let name = self.fresh(&js_word(&word));
                specialized.push((bound, Expr::var(&name)));
                own_params.push(name);
                continue;
            }
            let concrete = ty::EarlyBinder::bind(self.tcx, bound)
                .instantiate(self.tcx, args)
                .skip_normalization();
            specialized.push((bound, self.dictionary(concrete, span)?));
        }
        // Its trait's type parameters, `Self` among them, drop as the impl's
        // arguments for them do, with the impl's drops (ADR 0098): each is
        // made here, and the body is given it by name.
        let mut made = Vec::new();
        let mut drops = HashMap::new();
        // One rust-js can't make is an error only if the body drops one.
        let mut unsupported = HashMap::new();
        let generics = self.tcx.generics_of(id);
        for (index, arg) in args.iter().enumerate() {
            let Some(ty) = arg.as_type() else {
                continue;
            };
            match self.drops(ty) {
                Drops::Nothing => continue,
                Drops::Unsupported(t, what) => {
                    unsupported.insert(index as u32, (t, what));
                    continue;
                }
                Drops::Runs => {}
            }
            let Some(drop) = self.drop_function(ty, span)? else {
                continue;
            };
            let name = match &drop.kind {
                js::ExprKind::Var(name) => name.clone(),
                _ => {
                    let name = self.fresh(&format!("drop{}", generics.param_at(index, self.tcx).name));
                    made.push((index as u32, StmtKind::Const(name.clone(), drop).at(js::Span::NONE)));
                    name
                }
            };
            drops.insert(index as u32, name);
        }
        let body = self.krate.bodies[&id];
        let nested = super::Nested::Default {
            evidence: specialized,
            self_args: args,
            typing_env: ty::TypingEnv::post_analysis(self.tcx, id),
            drops,
            unsupported,
        };
        let enclosing = self.enter_body(body, id, nested)?;
        let mut rest = Vec::new();
        let (mut params, is_async) = self.lower_signature(id, &body.thir.params.raw, body.expr, &mut rest)?;
        params.extend(own_params.into_iter().map(Into::into));
        // Only the drops it uses: most defaults drop nothing of their `Self`.
        let used = self.used_drops();
        self.leave_body(enclosing)?;
        let mut out: Vec<_> = made
            .into_iter()
            .filter(|(index, _)| used.contains(index))
            .map(|(_, stmt)| stmt)
            .collect();
        out.extend(rest);
        Ok(if is_async {
            Expr::async_arrow(params, out)
        } else {
            Expr::arrow(params, out)
        })
    }

    /// Can a `dyn` of the trait `id` be a pair (ADR 0049): its items are
    /// methods, and neither it nor a supertrait has type parameters. A `&mut
    /// self` method is given the pair, whose `value` a box's is (ADR 0099).
    pub(super) fn dyn_supported(&self, id: DefId) -> bool {
        self.tcx
            .associated_items(id)
            .in_definition_order()
            .all(|item| match self.tcx.def_kind(item.def_id) {
                DefKind::AssocFn => true,
                // A `dyn Source<Item = u32>` says what it is (ADR 0106).
                DefKind::AssocTy => self.tcx.generics_of(item.def_id).own_params.is_empty(),
                _ => false,
            })
            && self
                .tcx
                .explicit_super_predicates_of(id)
                .iter_identity_copied()
                .map(|item| item.skip_normalization())
                .all(|(clause, _)| match clause.kind().skip_binder() {
                    ty::ClauseKind::Trait(p) if operational(self.tcx, self.krate.foreign, p.trait_ref.def_id) => {
                        self.dyn_supported(p.trait_ref.def_id)
                    }
                    _ => true,
                })
    }
}
