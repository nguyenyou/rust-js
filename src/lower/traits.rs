//! Trait evidence stays separate from payloads: arguments for generics,
//! lazy dictionaries for impls, and `{ value, impl }` for trait objects.

use super::bindings;
use super::{FnCx, R, lower_first};
use crate::js::{self, Expr, Op, Prop, StmtKind};
use crate::runtime::Helper;
use rustc_hir::def::DefKind;
use rustc_hir::{LangItem, Mutability};
use rustc_middle::traits::ImplSource;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, sym};

/// A trait whose bounds take dictionaries (ADR 0049): the crate's own, and
/// the std ones rust-js has dictionaries for (ADR 0052).
pub(super) fn operational(tcx: TyCtxt<'_>, id: DefId) -> bool {
    id.is_local()
        || tcx.is_lang_item(id, LangItem::Copy)
        || tcx.is_lang_item(id, LangItem::Clone)
        || tcx.is_lang_item(id, LangItem::PartialEq)
        || tcx.is_lang_item(id, LangItem::PartialOrd)
        || tcx.is_diagnostic_item(sym::Ord, id)
        || tcx.is_diagnostic_item(Symbol::intern("Display"), id)
        || tcx.is_diagnostic_item(Symbol::intern("Default"), id)
}

/// A trait the crate may implement. `From` has no dictionaries: its impls
/// are only called where the types are known (ADR 0052). `Eq` has no
/// methods: a `T: Eq` bound is its `PartialEq` (ADR 0053). An `Iterator` is a
/// JS iterator, and has no dictionaries either (ADR 0055).
pub(super) fn implementable(tcx: TyCtxt<'_>, id: DefId) -> bool {
    operational(tcx, id)
        || tcx.is_diagnostic_item(sym::From, id)
        || tcx.is_diagnostic_item(sym::Eq, id)
        || tcx.is_diagnostic_item(sym::Iterator, id)
}

pub(super) fn validate(tcx: TyCtxt<'_>) -> bool {
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
        if derived(id) || (kind == DefKind::AssocFn && tcx.opt_local_parent(id).is_some_and(derived)) {
            continue;
        }
        let params = &tcx.generics_of(id).own_params;
        let reason = if params
            .iter()
            .any(|p| matches!(p.kind, ty::GenericParamDefKind::Const { .. }))
        {
            Some("const generics")
        } else if kind == DefKind::Trait
            && params
                .iter()
                .any(|p| p.index != 0 && !matches!(p.kind, ty::GenericParamDefKind::Lifetime))
        {
            Some("generic trait parameters")
        } else if kind == DefKind::AssocFn
            && tcx.inherent_impl_of_assoc(id.to_def_id()).is_none()
            && params
                .iter()
                .any(|p| !matches!(p.kind, ty::GenericParamDefKind::Lifetime))
        {
            Some("generic trait methods")
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
            for (predicate, span) in tcx.explicit_super_predicates_of(id).iter_identity_copied() {
                if let ty::ClauseKind::Trait(p) = predicate.kind().skip_binder()
                    && operational(tcx, p.trait_ref.def_id)
                {
                    let name = tcx.item_name(p.trait_ref.def_id).to_string();
                    if name == "__proto__" || !names.insert(name) {
                        tcx.dcx().span_err(span, "rust-js: supertrait dictionary names collide");
                        valid = false;
                    }
                }
            }
            for item in tcx.associated_items(id).in_definition_order() {
                let name = bindings::fn_name(tcx, item.def_id);
                if name == "__proto__" || !names.insert(name) {
                    tcx.dcx().span_err(
                        tcx.def_span(item.def_id),
                        "rust-js: trait dictionary names collide or use reserved `__proto__`",
                    );
                    valid = false;
                }
            }
        }
    }
    valid
}

/// Signature order, including parent impl bounds. Never depend on body usage.
pub(super) fn bounds<'tcx>(tcx: TyCtxt<'tcx>, id: DefId) -> Vec<ty::TraitRef<'tcx>> {
    let mut result = Vec::new();
    if let Some(trait_id) = tcx.trait_of_assoc(id)
        && operational(tcx, trait_id)
    {
        result.push(ty::TraitRef::identity(tcx, trait_id));
    }
    for (clause, _) in tcx.predicates_of(id).instantiate_identity(tcx) {
        let ty::ClauseKind::Trait(predicate) = clause.kind().skip_binder() else {
            continue;
        };
        let mut tr = predicate.trait_ref;
        // `Eq` promises more than `PartialEq`, but it's `PartialEq`'s `eq`
        // that's called.
        if tcx.is_diagnostic_item(sym::Eq, tr.def_id) {
            let partial_eq = tcx.require_lang_item(LangItem::PartialEq, tcx.def_span(id));
            tr = ty::TraitRef::new(tcx, partial_eq, [tr.self_ty(), tr.self_ty()]);
        }
        if operational(tcx, tr.def_id) && !result.contains(&tr) {
            result.push(tr);
        }
    }
    result
}

/// The type, then the trait, then the trait's arguments other than their
/// defaults: `circleShape`, `metersFromF64` for `impl From<f64> for
/// Meters`, and `versionPartialEq`, whose `Rhs` is `Self` (ADR 0052).
pub(super) fn impl_name(tcx: TyCtxt<'_>, id: DefId) -> String {
    let tr = tcx.impl_trait_ref(id).instantiate_identity();
    let word = |ty: Ty<'_>| match ty.kind() {
        ty::Adt(adt, _) => tcx.item_name(adt.did()).to_string(),
        _ => ty.to_string(),
    };
    let mut name = format!(
        "{}{}",
        lower_first(&js_word(&word(tr.self_ty()))),
        tcx.item_name(tr.def_id)
    );
    let generics = tcx.generics_of(tr.def_id);
    for (param, arg) in generics.own_params.iter().zip(tr.args).skip(1) {
        let Some(arg) = arg.as_type() else { continue };
        if param.default_value(tcx).map(|d| d.instantiate(tcx, tr.args)) == Some(arg.into()) {
            continue;
        }
        let arg = js_word(&word(arg.peel_refs()));
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
        bounds(self.tcx, id)
            .into_iter()
            .map(|tr| {
                let name = self.fresh(&js_word(&format!("{}{}", tr.self_ty(), self.tcx.item_name(tr.def_id))));
                self.evidence.push((tr, Expr::var(&name)));
                name.into()
            })
            .collect()
    }

    fn super_evidence(&self, from: ty::TraitRef<'tcx>, to: ty::TraitRef<'tcx>, value: Expr) -> Option<Expr> {
        if self.tcx.erase_and_anonymize_regions(from) == self.tcx.erase_and_anonymize_regions(to) {
            return Some(value);
        }
        // A std trait's dictionary, like `Copy`'s, has no supertraits in it.
        if !from.def_id.is_local() {
            return None;
        }
        for (clause, _) in self
            .tcx
            .explicit_super_predicates_of(from.def_id)
            .iter_instantiated_copied(self.tcx, from.args)
        {
            if let ty::ClauseKind::Trait(predicate) = clause.kind().skip_binder() {
                let tr = predicate.trait_ref;
                let parent = Expr::call(
                    Expr::member(value.clone(), self.tcx.item_name(tr.def_id).to_string()),
                    Vec::new(),
                );
                if let Some(found) = self.super_evidence(tr, to, parent) {
                    return Some(found);
                }
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
    }

    pub(super) fn dictionary(&mut self, tr: ty::TraitRef<'tcx>, span: Span) -> R<Expr> {
        if let Some(found) = self.evidence_for(tr) {
            return Ok(found);
        }
        // Derived and std impls of `Default` and `Clone` (ADR 0052).
        let ty = tr.self_ty();
        let default = self.tcx.is_diagnostic_item(Symbol::intern("Default"), tr.def_id);
        let clone = self.tcx.is_lang_item(tr.def_id, LangItem::Clone);
        let eq = self.tcx.is_lang_item(tr.def_id, LangItem::PartialEq);
        let display = tr.def_id == self.display_trait();
        let ord = tr.def_id == self.ord_trait();
        let partial_ord = tr.def_id == self.partial_ord_trait();
        if (default || clone || eq || display || ord || partial_ord) && !self.has_user_impl(tr.def_id, ty) {
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
        if self.tcx.is_lang_item(tr.def_id, LangItem::Copy) {
            let ty = tr.self_ty();
            if matches!(ty.kind(), ty::Param(..)) {
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
        if let Ok(ImplSource::UserDefined(imp)) = selected
            && self.krate.trait_impls.contains(&imp.impl_def_id)
        {
            let callee = self.fn_ref(imp.impl_def_id);
            let args = self.evidence_args(imp.impl_def_id, imp.args, span)?;
            return Ok(Expr::call(callee, args));
        }
        Err(self.unsupported(span, &format!("implementation evidence for `{tr}`")))
    }

    pub(super) fn evidence_args(&mut self, id: DefId, args: ty::GenericArgsRef<'tcx>, span: Span) -> R<Vec<Expr>> {
        bounds(self.tcx, id)
            .into_iter()
            .map(|bound| {
                let bound = ty::EarlyBinder::bind(bound).instantiate(self.tcx, args);
                self.dictionary(bound, span)
            })
            .collect()
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
            Some(args) => ty::EarlyBinder::bind(generic_args).instantiate(self.tcx, args),
            None => generic_args,
        };
        let tr = ty::TraitRef::from_assoc(self.tcx, trait_id, generic_args);
        if matches!(tr.self_ty().kind(), ty::Dynamic(..)) && operational(self.tcx, trait_id) {
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
            values.insert(0, Expr::member(pair, "value"));
            return Ok(Some(Expr::call(
                Expr::member(dictionary, bindings::fn_name(self.tcx, id)),
                values,
            )));
        }
        if let Some(instance) = ty::Instance::try_resolve(self.tcx, self.typing_env, id, generic_args)?
            && self.krate.fns.contains_key(&instance.def_id())
            && self.tcx.trait_of_assoc(instance.def_id()).is_none()
        {
            let mut values = values;
            values.extend(self.evidence_args(instance.def_id(), instance.args, span)?);
            return Ok(Some(Expr::call(self.fn_ref(instance.def_id()), values)));
        }
        // What rust-js writes itself, in place: `c.clone()` of a struct is a
        // copy of it, not a dictionary's `clone` (ADR 0052).
        if self.tcx.is_lang_item(id, LangItem::CloneFn) {
            let mut values = values;
            return Ok(Some(self.clone_value(values.remove(0), tr.self_ty(), span, out)?));
        }
        if self.tcx.is_diagnostic_item(Symbol::intern("Default"), trait_id) {
            return Ok(Some(self.default_value(tr.self_ty(), span)?));
        }
        // `a != b` is `!(a == b)`, as Rust requires them to agree (ADR 0053).
        if self.tcx.is_lang_item(trait_id, LangItem::PartialEq) {
            let [a, b]: [Expr; 2] = values.try_into().map_err(|_| self.unsupported(span, "this `==`"))?;
            // A hand-written `PartialEq<Rhs>` is its own `eq`, whatever `Rhs` is.
            let eq = if self.is_user_impl(tr) {
                let eq = self.tcx.associated_item_def_ids(trait_id)[0];
                self.impl_call(eq, tr.args, vec![a, b], span)?
            } else {
                self.eq_value(a, b, tr.self_ty(), span, out)?
            };
            return Ok(Some(match self.tcx.item_name(id).as_str() {
                "ne" => super::std_impls::negate(eq),
                _ => eq,
            }));
        }
        // `a < b`, `a.cmp(&b)`, `a.max(b)` (ADR 0057). Of numbers, they're
        // std's operators and `Math.max`, as before.
        let ordering = tr.def_id == self.ord_trait() || tr.def_id == self.partial_ord_trait();
        if let Some(call) = self.ordering_call(id, tr, values.clone(), span, out)? {
            return Ok(Some(call));
        }
        if ordering && super::representation::Num::of(tr.self_ty().peel_refs()).is_some() {
            return Ok(None);
        }
        // A std trait's dictionary has only its required methods.
        if operational(self.tcx, trait_id) && !trait_id.is_local() && self.tcx.defaultness(id).has_value() {
            let what = format!("calling `{}`", self.tcx.def_path_str(id));
            return Err(self.unsupported(span, &what));
        }
        if operational(self.tcx, trait_id) {
            let dictionary = self.dictionary(tr, span)?;
            return Ok(Some(Expr::call(
                Expr::member(dictionary, bindings::fn_name(self.tcx, id)),
                values,
            )));
        }
        Ok(None)
    }

    pub(super) fn dynamic_trait(&self, ty: Ty<'tcx>) -> Option<DefId> {
        let inner = self.pointee(ty);
        match inner.kind() {
            ty::Dynamic(predicates, ..) => predicates.principal_def_id().filter(|id| id.is_local()),
            _ => None,
        }
    }

    fn dyn_trait_ref(&self, ty: Ty<'tcx>, self_ty: Ty<'tcx>) -> Option<ty::TraitRef<'tcx>> {
        match self.pointee(ty).kind() {
            ty::Dynamic(predicates, ..) => predicates
                .principal()
                .map(|p| p.with_self_ty(self.tcx, self_ty).skip_binder()),
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
            return Ok(Expr::object(vec![
                Prop::Field("value".into(), Expr::member(pair, "value")),
                Prop::Field("impl".into(), dictionary),
            ]));
        }
        let tr = self.dyn_trait_ref(target, self.pointee(source)).unwrap();
        Ok(Expr::object(vec![
            Prop::Field("value".into(), value),
            Prop::Field("impl".into(), self.dictionary(tr, span)?),
        ]))
    }

    pub(super) fn lower_dictionary(&mut self, id: DefId, cache: &str) -> R<js::Function> {
        let span = self.tcx.def_span(id);
        // The crate's own generic traits are errors (`validate`); a std one's
        // impl, like `PartialEq<Rhs>`'s, is for its arguments.
        let tr = self.tcx.impl_trait_ref(id).instantiate_identity();
        let params = self.evidence_params(id);
        let mut props = Vec::new();
        for (clause, _) in self
            .tcx
            .explicit_super_predicates_of(tr.def_id)
            .iter_instantiated_copied(self.tcx, tr.args)
        {
            if let ty::ClauseKind::Trait(predicate) = clause.kind().skip_binder()
                && operational(self.tcx, predicate.trait_ref.def_id)
            {
                let dictionary = self.dictionary(predicate.trait_ref, span)?;
                props.push(Prop::Field(
                    self.tcx.item_name(predicate.trait_ref.def_id).to_string(),
                    Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(dictionary)).at(js::Span::NONE)]),
                ));
            }
        }
        for item in self.tcx.associated_items(tr.def_id).in_definition_order() {
            if self.tcx.def_kind(item.def_id) != DefKind::AssocFn {
                continue;
            }
            // A std trait's provided methods, like `Clone::clone_from`,
            // aren't in its dictionary: nothing calls them through it.
            if !tr.def_id.is_local() && self.tcx.defaultness(item.def_id).has_value() {
                continue;
            }
            if self
                .tcx
                .generics_of(item.def_id)
                .own_params
                .iter()
                .any(|p| !matches!(p.kind, ty::GenericParamDefKind::Lifetime))
            {
                return Err(self.unsupported(self.tcx.def_span(item.def_id), "generic trait methods"));
            }
            let instance = ty::Instance::try_resolve(self.tcx, self.typing_env, item.def_id, tr.args)?
                .ok_or_else(|| self.unsupported(span, "this trait implementation"))?;
            let method = instance.def_id();
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
                .skip_binder()
                .inputs()
                .len()
                - usize::from(self.formatter_param(method).is_some());
            let args: Vec<String> = (0..count).map(|i| format!("arg{i}")).collect();
            let mut values: Vec<Expr> = args.iter().map(|name| Expr::var(name)).collect();
            // A default body has a Self dictionary. Build its thunk without
            // recursively forcing the dictionary currently being assembled.
            let mut evidence = Vec::new();
            for bound in bounds(self.tcx, method) {
                let bound = ty::EarlyBinder::bind(bound).instantiate(self.tcx, instance.args);
                evidence.push(self.dictionary(bound, span)?);
            }
            let value = if evidence.is_empty() {
                callee
            } else {
                values.extend(evidence);
                Expr::arrow(
                    args.into_iter().map(Into::into).collect(),
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
            body.push(
                StmtKind::If(
                    undefined,
                    vec![
                        StmtKind::Assign(Expr::var(cache), Expr::new_(Expr::var("WeakMap"), Vec::new()))
                            .at(js::Span::NONE),
                    ],
                    None,
                )
                .at(js::Span::NONE),
            );
            self.runtime.insert(Helper::TraitImpl);
            let keys = Expr::array(self.evidence.iter().map(|(_, value)| value.clone()).collect());
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
        for bound in bounds(self.tcx, id) {
            let concrete = ty::EarlyBinder::bind(bound).instantiate(self.tcx, args);
            specialized.push((bound, self.dictionary(concrete, span)?));
        }
        let body = self.krate.bodies[&id];
        let evidence = std::mem::replace(&mut self.evidence, specialized);
        let self_args = self.self_args.replace(args);
        let thir = std::mem::replace(&mut self.thir, &body.thir);
        let typing_env = std::mem::replace(&mut self.typing_env, ty::TypingEnv::post_analysis(self.tcx, id));
        let vars = std::mem::take(&mut self.vars);
        let names = self.names.clone();
        let mut out = Vec::new();
        let (params, is_async) = self.lower_signature(id, &body.thir.params.raw, body.expr, &mut out)?;
        self.evidence = evidence;
        self.self_args = self_args;
        self.thir = thir;
        self.typing_env = typing_env;
        self.vars = vars;
        self.names = names;
        Ok(if is_async {
            Expr::async_arrow(params, out)
        } else {
            Expr::arrow(params, out)
        })
    }

    pub(super) fn readonly_dyn(&self, id: DefId) -> bool {
        !self
            .tcx
            .generics_of(id)
            .own_params
            .iter()
            .any(|p| p.index != 0 && !matches!(p.kind, ty::GenericParamDefKind::Lifetime))
            && self.tcx.associated_items(id).in_definition_order().all(|item| {
                if self.tcx.def_kind(item.def_id) != DefKind::AssocFn {
                    return false;
                }
                let sig = self.tcx.fn_sig(item.def_id).instantiate_identity().skip_binder();
                !matches!(
                    sig.inputs().first().map(|t| t.kind()),
                    Some(ty::Ref(_, _, Mutability::Mut))
                )
            })
            && self
                .tcx
                .explicit_super_predicates_of(id)
                .iter_identity_copied()
                .all(|(clause, _)| match clause.kind().skip_binder() {
                    ty::ClauseKind::Trait(p) if operational(self.tcx, p.trait_ref.def_id) => {
                        self.readonly_dyn(p.trait_ref.def_id)
                    }
                    _ => true,
                })
    }
}
