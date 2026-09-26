//! Trait evidence stays separate from payloads: arguments for generics,
//! lazy dictionaries for impls, and `{ value, impl }` for trait objects.

use super::bindings;
use super::representation::Num;
use super::{Dest, FnCx, R, lower_first};
use crate::js::{self, Expr, Op, Prop, StmtKind};
use crate::runtime::Helper;
use rustc_hir::def::DefKind;
use rustc_hir::{LangItem, Mutability};
use rustc_middle::thir::BodyTy;
use rustc_middle::traits::ImplSource;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, sym};

pub(super) fn operational(tcx: TyCtxt<'_>, id: DefId) -> bool {
    id.is_local()
        || tcx.is_lang_item(id, LangItem::Copy)
        || tcx.is_diagnostic_item(Symbol::intern("Default"), id)
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
                        tcx.dcx()
                            .span_err(span, "rust-js: supertrait dictionary names collide");
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
    if let Some(trait_id) = tcx.trait_of_assoc(id) {
        if operational(tcx, trait_id) {
            result.push(ty::TraitRef::identity(tcx, trait_id));
        }
    }
    for (clause, _) in tcx.predicates_of(id).instantiate_identity(tcx) {
        if let ty::ClauseKind::Trait(predicate) = clause.kind().skip_binder()
            && operational(tcx, predicate.trait_ref.def_id)
            && !result.contains(&predicate.trait_ref)
        {
            result.push(predicate.trait_ref);
        }
    }
    result
}

pub(super) fn impl_name(tcx: TyCtxt<'_>, id: DefId) -> String {
    let tr = tcx.impl_trait_ref(id).instantiate_identity();
    let name = match tr.self_ty().kind() {
        ty::Adt(adt, _) => tcx.item_name(adt.did()).to_string(),
        _ => tr.self_ty().to_string(),
    };
    let name: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("{}{}", lower_first(&name), tcx.item_name(tr.def_id))
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    pub(super) fn evidence_params(&mut self, id: DefId) -> Vec<js::Pattern> {
        bounds(self.tcx, id)
            .into_iter()
            .map(|tr| {
                let base = format!("{}{}", tr.self_ty(), self.tcx.item_name(tr.def_id));
                let base: String = base
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c } else { '_' })
                    .collect();
                let name = self.fresh(&base);
                self.evidence.push((tr, Expr::var(&name)));
                name.into()
            })
            .collect()
    }

    fn super_evidence(&self, from: ty::TraitRef<'tcx>, to: ty::TraitRef<'tcx>, value: Expr) -> Option<Expr> {
        if self.tcx.erase_and_anonymize_regions(from) == self.tcx.erase_and_anonymize_regions(to) {
            return Some(value);
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

    pub(super) fn dictionary(&mut self, tr: ty::TraitRef<'tcx>, span: Span) -> R<Expr> {
        for (bound, value) in &self.evidence {
            if let Some(found) = self.super_evidence(*bound, tr, value.clone()) {
                return Ok(found);
            }
        }
        if self.tcx.is_diagnostic_item(Symbol::intern("Default"), tr.def_id) {
            let ty = tr.self_ty();
            let value = if Num::of(ty).is_some() {
                Some(Expr::int(0))
            } else if ty.is_bool() {
                Some(Expr::bool(false))
            } else if ty.is_char() {
                Some(Expr::str("\0"))
            } else if ty.is_unit() || self.option_of(ty).is_some() {
                Some(Expr::undefined())
            } else if self.is_lang_adt(ty, LangItem::String) {
                Some(Expr::str(""))
            } else if self.is_std_adt(ty, sym::Vec) {
                Some(Expr::array(Vec::new()))
            } else {
                None
            };
            if let Some(value) = value {
                self.check_value_ty(ty, span)?;
                return Ok(Expr::object(vec![Prop::Field(
                    "default".into(),
                    Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(value)).at(js::Span::NONE)]),
                )]));
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
        let selected = self
            .tcx
            .codegen_select_candidate(self.typing_env.as_query_input(tr));
        if let Ok(ImplSource::UserDefined(imp)) = selected
            && self.krate.trait_impls.contains(&imp.impl_def_id)
        {
            let callee = self.fn_ref(imp.impl_def_id);
            let args = self.evidence_args(imp.impl_def_id, imp.args, span)?;
            return Ok(Expr::call(callee, args));
        }
        Err(self.unsupported(span, &format!("implementation evidence for `{tr}`")))
    }

    pub(super) fn evidence_args(
        &mut self,
        id: DefId,
        args: ty::GenericArgsRef<'tcx>,
        span: Span,
    ) -> R<Vec<Expr>> {
        bounds(self.tcx, id)
            .into_iter()
            .map(|bound| {
                let bound = ty::EarlyBinder::bind(bound).instantiate(self.tcx, args);
                self.dictionary(bound, span)
            })
            .collect()
    }

    /// Select user code before std intrinsics, so custom implementations win.
    pub(super) fn trait_call(
        &mut self,
        id: DefId,
        generic_args: ty::GenericArgsRef<'tcx>,
        values: Vec<Expr>,
        span: Span,
    ) -> R<Option<Expr>> {
        let Some(trait_id) = self.tcx.trait_of_assoc(id) else {
            return Ok(None);
        };
        if self.tcx.fn_trait_kind_from_def_id(trait_id).is_some() {
            return Ok(None);
        }
        let tr = ty::TraitRef::from_assoc(self.tcx, trait_id, generic_args);
        if matches!(tr.self_ty().kind(), ty::Dynamic(..)) && operational(self.tcx, trait_id) {
            let mut values = values;
            let receiver = values.remove(0);
            // `operands` already preserves Rust evaluation order. Evaluate a
            // receiver expression only once even though the pair is read twice.
            let temporary = receiver.has_effects().then(|| self.fresh("shape"));
            let pair = temporary
                .as_ref()
                .map_or_else(|| receiver.clone(), |name| Expr::var(name));
            let principal = self.dyn_trait_ref(tr.self_ty(), tr.self_ty()).unwrap();
            let dictionary = self
                .super_evidence(principal, tr, Expr::member(pair.clone(), "impl"))
                .ok_or_else(|| self.unsupported(span, "this trait object supertrait"))?;
            values.insert(0, Expr::member(pair, "value"));
            let call = Expr::call(Expr::member(dictionary, bindings::fn_name(self.tcx, id)), values);
            return Ok(Some(match temporary {
                Some(name) => Expr::call(
                    Expr::arrow(
                        vec![name.into()],
                        vec![StmtKind::Return(Some(call)).at(self.js_span(span))],
                    ),
                    vec![receiver],
                ),
                None => call,
            }));
        }
        if let Some(instance) = ty::Instance::try_resolve(self.tcx, self.typing_env, id, generic_args)?
            && self.krate.fns.contains_key(&instance.def_id())
            && self.tcx.trait_of_assoc(instance.def_id()).is_none()
        {
            let mut values = values;
            values.extend(self.evidence_args(instance.def_id(), instance.args, span)?);
            return Ok(Some(Expr::call(self.fn_ref(instance.def_id()), values)));
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

    pub(super) fn unsize_trait(
        &mut self,
        source: Ty<'tcx>,
        target: Ty<'tcx>,
        value: Expr,
        span: Span,
    ) -> R<Expr> {
        if self.dynamic_trait(target).is_none() {
            return Ok(value);
        }
        self.check_value_ty(target, span)?;
        if self.dynamic_trait(source).is_some() {
            let self_ty = self.pointee(source);
            let from = self.dyn_trait_ref(source, self_ty).unwrap();
            let to = self.dyn_trait_ref(target, self_ty).unwrap();
            let pair = Expr::var("shape");
            let dictionary = self
                .super_evidence(from, to, Expr::member(pair.clone(), "impl"))
                .ok_or_else(|| self.unsupported(span, "this trait upcast"))?;
            let pair = Expr::object(vec![
                Prop::Field("value".into(), Expr::member(pair, "value")),
                Prop::Field("impl".into(), dictionary),
            ]);
            return Ok(Expr::call(
                Expr::arrow(
                    vec!["shape".into()],
                    vec![StmtKind::Return(Some(pair)).at(self.js_span(span))],
                ),
                vec![value],
            ));
        }
        let tr = self.dyn_trait_ref(target, self.pointee(source)).unwrap();
        Ok(Expr::object(vec![
            Prop::Field("value".into(), value),
            Prop::Field("impl".into(), self.dictionary(tr, span)?),
        ]))
    }

    pub(super) fn lower_dictionary(&mut self, id: DefId, cache: &str) -> R<js::Function> {
        let span = self.tcx.def_span(id);
        let tr = self.tcx.impl_trait_ref(id).instantiate_identity();
        if self
            .tcx
            .generics_of(tr.def_id)
            .own_params
            .iter()
            .any(|p| p.index != 0 && !matches!(p.kind, ty::GenericParamDefKind::Lifetime))
        {
            return Err(self.unsupported(span, "generic trait parameters"));
        }
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
                    Expr::arrow(
                        Vec::new(),
                        vec![StmtKind::Return(Some(dictionary)).at(js::Span::NONE)],
                    ),
                ));
            }
        }
        for item in self.tcx.associated_items(tr.def_id).in_definition_order() {
            if self.tcx.def_kind(item.def_id) != DefKind::AssocFn {
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
                return Err(
                    self.unsupported(span, &format!("trait method `{}`", self.tcx.def_path_str(method)))
                );
            }
            if self.tcx.trait_of_assoc(method).is_some() {
                let value = self.default_method(method, instance.args)?;
                props.push(Prop::Field(bindings::fn_name(self.tcx, item.def_id), value));
                continue;
            }
            let callee = self.fn_ref(method);
            let count = self
                .tcx
                .fn_sig(method)
                .instantiate_identity()
                .skip_binder()
                .inputs()
                .len();
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
            let make = Expr::arrow(
                Vec::new(),
                vec![StmtKind::Return(Some(object)).at(js::Span::NONE)],
            );
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
        let thir = std::mem::replace(&mut self.thir, &body.thir);
        let typing_env = std::mem::replace(&mut self.typing_env, ty::TypingEnv::post_analysis(self.tcx, id));
        let vars = std::mem::take(&mut self.vars);
        let names = self.names.clone();
        let mut out = Vec::new();
        let params = self.lower_params(&body.thir.params.raw, span, &mut out)?;
        let BodyTy::Fn(sig) = body.thir.body_type else {
            unreachable!("trait method body")
        };
        self.check_value_ty(sig.output(), span)?;
        let dest = if sig.output().is_unit() {
            Dest::Discard
        } else {
            Dest::Return
        };
        let is_async = self.lower_body(body.expr, &dest, &mut out)?;
        self.evidence = evidence;
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
