//! `Clone`, `Default` and `PartialEq` where rust-js writes the
//! implementation: derived ones and std types' (ADRs 0052, 0053). A
//! hand-written one is called instead.

use super::bindings::variant_name;
use super::representation::Num;
use super::{FnCx, R, Shape, is_fieldless_enum, lower_first};
use crate::js::{self, Expr, Op, Prop, Stmt, StmtKind, UnaryOp};
use crate::runtime::Helper;
use rustc_hir as hir;
use rustc_hir::LangItem;
use rustc_hir::def::{DefKind, Res};
use rustc_middle::traits::ImplSource;
use rustc_middle::ty::{self, Ty};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, sym};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// The trait's arguments for `Self = ty`, with `ty` for any others too:
    /// `PartialEq`'s `Rhs` is `Self` unless it says otherwise.
    pub(super) fn args_of(&self, trait_id: DefId, ty: Ty<'tcx>) -> ty::GenericArgsRef<'tcx> {
        let ty = self.tcx.erase_and_anonymize_regions(ty);
        self.tcx.mk_args_from_iter(std::iter::repeat_n(
            ty::GenericArg::from(ty),
            self.tcx.generics_of(trait_id).count(),
        ))
    }

    /// Does `ty` use a hand-written impl of `trait_id` from this crate?
    pub(super) fn has_user_impl(&self, trait_id: DefId, ty: Ty<'tcx>) -> bool {
        self.is_user_impl(ty::TraitRef::new_from_args(
            self.tcx,
            trait_id,
            self.args_of(trait_id, ty),
        ))
    }

    /// Is the crate's impl of `trait_id` for `ty` a `#[derive]`d one?
    pub(super) fn is_derived_impl(&self, trait_id: DefId, ty: Ty<'tcx>) -> bool {
        let tr = ty::TraitRef::new_from_args(self.tcx, trait_id, self.args_of(trait_id, ty));
        let tr = self.tcx.erase_and_anonymize_regions(tr);
        matches!(self.tcx.codegen_select_candidate(self.typing_env.as_query_input(tr)),
            Ok(ImplSource::UserDefined(imp)) if self.krate.trait_impls.contains(&imp.impl_def_id)
                && self.tcx.is_automatically_derived(imp.impl_def_id))
    }

    /// Is `tr` a hand-written impl from this crate?
    pub(super) fn is_user_impl(&self, tr: ty::TraitRef<'tcx>) -> bool {
        let tr = self.tcx.erase_and_anonymize_regions(tr);
        matches!(self.tcx.codegen_select_candidate(self.typing_env.as_query_input(tr)),
            Ok(ImplSource::UserDefined(imp)) if self.krate.trait_impls.contains(&imp.impl_def_id))
    }

    /// `method` of the hand-written impl for the trait's `args`, called
    /// directly: `counterClone_clone(c)`.
    pub(super) fn impl_call(
        &mut self,
        method: DefId,
        args: ty::GenericArgsRef<'tcx>,
        mut values: Vec<Expr>,
        span: Span,
    ) -> R<Expr> {
        let args = self.tcx.erase_and_anonymize_regions(args);
        let instance = ty::Instance::try_resolve(self.tcx, self.typing_env, method, args)?
            .filter(|i| self.krate.fns.contains_key(&i.def_id()))
            .ok_or_else(|| self.unsupported(span, "this implementation"))?;
        values.extend(self.evidence_args(instance.def_id(), instance.args, span)?);
        Ok(Expr::call(self.fn_ref(instance.def_id()), values))
    }

    pub(super) fn clone_trait(&self) -> DefId {
        self.tcx.require_lang_item(LangItem::Clone, rustc_span::DUMMY_SP)
    }

    /// Can a clone of `ty` be told apart from the value itself? Only if one
    /// of them can change in place, like a `Vec`, a cell, or a type in
    /// `mutated` (ADR 0020), or if cloning runs code: a hand-written `clone`,
    /// or a `T`'s, which might be one.
    pub(super) fn needs_clone(&self, ty: Ty<'tcx>) -> bool {
        self.needs_clone_in(ty, &mut Vec::new())
    }

    fn needs_clone_in(&self, ty: Ty<'tcx>, seen: &mut Vec<Ty<'tcx>>) -> bool {
        if self.contains_mutated(ty) {
            return true;
        }
        // A recursive type: its other fields decide.
        if seen.contains(&ty) {
            return false;
        }
        seen.push(ty);
        let std = |name: &str| self.is_std_adt(ty, Symbol::intern(name));
        let needs = match ty.kind() {
            // A clone of a `&T` is the same reference.
            ty::Ref(..) => false,
            ty::Tuple(tys) => tys.iter().any(|t| self.needs_clone_in(t, seen)),
            // Arrays and cells are JS objects that change in place, and so
            // is a `Vec` something takes `&mut` of.
            ty::Array(..) => true,
            ty::Adt(_, args) if self.is_vec_like(ty) => {
                self.vec_changed(ty) || self.needs_clone_in(args.type_at(0), seen)
            }
            ty::Adt(..) if std("Cell") || std("RefCell") => true,
            // A map or a set changes in place (ADR 0059).
            ty::Adt(..) if self.is_map(ty) => true,
            ty::Adt(..) if std("Rc") || self.is_lang_adt(ty, LangItem::String) || self.is_js_object(ty) => false,
            ty::Adt(..) if self.has_user_impl(self.clone_trait(), ty) => true,
            ty::Adt(adt, args) if ty.is_box() || !self.is_std(adt.did()) || self.is_known_std(ty) => adt
                .all_fields()
                .any(|f| self.needs_clone_in(f.ty(self.tcx, args), seen)),
            // Another std type: `clone_value` says it can't.
            ty::Adt(..) => true,
            _ => false,
        };
        seen.pop();
        needs
    }

    fn is_std(&self, id: DefId) -> bool {
        [sym::core, sym::alloc, sym::std].contains(&self.tcx.crate_name(id.krate))
    }

    /// std enums whose fields are what JS has: `Option`, `Result`, `Ordering`.
    fn is_known_std(&self, ty: Ty<'tcx>) -> bool {
        self.is_lang_adt(ty, LangItem::Option)
            || self.is_std_adt(ty, sym::Result)
            || self.is_lang_adt(ty, LangItem::OrderingEnum)
            // Its `PartialEq` and `Clone` are its field's; its order isn't
            // (`cmp_value`).
            || self.is_reverse(ty)
    }

    /// `Clone::clone` of the `ty` at `place`: the place itself when nothing
    /// could tell a clone from it, and otherwise a copy of the parts that
    /// could, calling each hand-written `clone` on the way.
    pub(super) fn clone_value(&mut self, place: Expr, ty: Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        if !self.needs_clone(ty) {
            return Ok(place);
        }
        if let ty::Param(_) = ty.kind() {
            let tr = ty::TraitRef::new(self.tcx, self.clone_trait(), [ty]);
            if let Some(dictionary) = self.evidence_for(tr) {
                return Ok(Expr::call(Expr::member(dictionary, "clone"), vec![place]));
            }
            // A `T: Copy`: its dictionary copies (ADR 0049).
            return Ok(self.copy(place, ty));
        }
        if self.has_user_impl(self.clone_trait(), ty) {
            let method = self.tcx.require_lang_item(LangItem::CloneFn, span);
            let args = self.args_of(self.clone_trait(), ty);
            return self.impl_call(method, args, vec![place], span);
        }
        // A constant, like `"Dot"`, is a value no one else holds.
        if place.is_constant() {
            return Ok(place);
        }
        if self.is_copy(ty) {
            return Ok(self.copy(place, ty));
        }
        // A type inside itself, `Value` in `Obj(BTreeMap<String, Value>)`: its
        // clone is a function, which calls itself for the ones inside.
        if self.is_recursive(ty) {
            if let Some((_, name)) = self.cloning.iter().find(|(t, _)| *t == ty) {
                return Ok(Expr::call(Expr::var(name), vec![place]));
            }
            let type_name = match ty.kind() {
                ty::Adt(adt, _) => self.tcx.item_name(adt.did()).to_string(),
                _ => "Value".to_string(),
            };
            let name = self.fresh(&format!("clone{type_name}"));
            let param = self.fresh(&lower_first(&type_name));
            self.cloning.push((ty, name.clone()));
            let mut body = Vec::new();
            let value = self.clone_parts(Expr::var(&param), ty, span, &mut body);
            self.cloning.pop();
            body.push(StmtKind::Return(Some(value?)).at(js::Span::NONE));
            let f = Expr::arrow(vec![param.into()], body);
            out.push(StmtKind::Const(name.clone(), f).at(js::Span::NONE));
            return Ok(Expr::call(Expr::var(&name), vec![place]));
        }
        self.clone_parts(place, ty, span, out)
    }

    /// Is `ty`, one of the crate's own, inside itself, through its fields or
    /// what a std type holds? A std type that holds one is cloned in place.
    fn is_recursive(&self, ty: Ty<'tcx>) -> bool {
        let mut seen = Vec::new();
        matches!(ty.kind(), ty::Adt(adt, _) if adt.did().is_local()) && self.holds(ty, ty, &mut seen)
    }

    /// Does `outer` hold `target` anywhere inside it?
    fn holds(&self, outer: Ty<'tcx>, target: Ty<'tcx>, seen: &mut Vec<Ty<'tcx>>) -> bool {
        let parts: Vec<Ty<'tcx>> = match outer.kind() {
            ty::Adt(adt, args) if adt.did().is_local() => adt.all_fields().map(|f| f.ty(self.tcx, args)).collect(),
            ty::Adt(_, args) => args.types().collect(),
            ty::Tuple(tys) => tys.to_vec(),
            ty::Array(item, _) | ty::Slice(item) | ty::Ref(_, item, _) => vec![*item],
            _ => Vec::new(),
        };
        parts.into_iter().any(|part| {
            if part == target {
                return true;
            }
            if seen.contains(&part) {
                return false;
            }
            seen.push(part);
            self.holds(part, target, seen)
        })
    }

    /// `clone_value` of a `ty` that needs a copy, part by part.
    fn clone_parts(&mut self, place: Expr, ty: Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        // Read more than once below.
        let place = if !place.reads_same() {
            self.spill("value", place, out)
        } else {
            place
        };
        let std = |name: &str| self.is_std_adt(ty, Symbol::intern(name));
        match ty.kind() {
            ty::Array(item, _) => self.clone_items(place, *item, span),
            ty::Adt(_, args) if self.is_vec_like(ty) => self.clone_items(place, args.type_at(0), span),
            ty::Adt(_, args) if ty.is_box() => self.clone_value(place, args.type_at(0), span, out),
            ty::Adt(_, args) if std("Cell") || std("RefCell") => {
                let value = self.clone_value(Expr::member(place, "value"), args.type_at(0), span, out)?;
                Ok(Expr::object(vec![Prop::Field("value".into(), value)]))
            }
            // `new Map(m)`, cloning each value that needs it; keys never do.
            ty::Adt(_, args) if self.is_map(ty) => {
                let set = self.is_set(ty);
                let value = args.types().nth(1).filter(|&v| !set && self.needs_clone(v));
                let entries = match value {
                    Some(v) => {
                        let clone = self.clone_value(Expr::var("value"), v, span, out)?;
                        let pair = Expr::array(vec![Expr::var("key"), clone]);
                        let f = Expr::arrow(
                            vec![js::Pattern::Array(vec![Some("key".into()), Some("value".into())])],
                            vec![StmtKind::Return(Some(pair)).at(js::Span::NONE)],
                        );
                        let all = Expr::call(Expr::member(Expr::var("Array"), "from"), vec![place]);
                        Expr::call(Expr::member(all, "map"), vec![f])
                    }
                    None => place,
                };
                Ok(Expr::new_(Expr::var(if set { "Set" } else { "Map" }), vec![entries]))
            }
            ty::Adt(_, args) if self.is_lang_adt(ty, LangItem::Option) => {
                let inner = args.type_at(0);
                let some = if self.boxed_payload(inner) {
                    let value = self.some_value(place.clone());
                    let clone = self.clone_value(value, inner, span, out)?;
                    self.some(clone)
                } else {
                    self.clone_value(place.clone(), inner, span, out)?
                };
                let none = Expr::bin(Op::LooseEq, place.clone(), Expr::null());
                Ok(Expr::cond(none, place, some))
            }
            ty::Adt(adt, args) if adt.is_enum() && (!self.is_std(adt.did()) || self.is_known_std(ty)) => {
                // `{ TAG: "Line", _0: .. }` (ADR 0033): a variant with fields
                // that need it gets a copy, and every other value is itself.
                let itself = self.mutated_itself(ty);
                let mut value = place.clone();
                for variant in adt.variants().iter().rev() {
                    let fields = self.variant_fields(variant, args);
                    if fields.is_empty() || !(itself || fields.iter().any(|&(_, t)| self.needs_clone(t))) {
                        continue;
                    }
                    let copy = self.clone_fields(place.clone(), fields, span, out)?;
                    let test = Expr::bin(
                        Op::Eq,
                        Expr::member(place.clone(), "TAG"),
                        Expr::str(variant_name(self.tcx, variant)),
                    );
                    value = Expr::cond(test, copy, value);
                }
                Ok(value)
            }
            _ if !matches!(ty.kind(), ty::Adt(adt, _) if self.is_std(adt.did())) => match self.shape(ty) {
                Shape::Object(fields) => self.clone_fields(place, fields, span, out),
                Shape::Array(tys) => Ok(Expr::array(
                    tys.into_iter()
                        .enumerate()
                        .map(|(i, t)| self.clone_value(Expr::index(place.clone(), Expr::int(i as i128)), t, span, out))
                        .collect::<R<_>>()?,
                )),
                Shape::Other => Err(self.unsupported(span, &format!("cloning `{ty}`"))),
            },
            _ => Err(self.unsupported(span, &format!("cloning `{ty}`"))),
        }
    }

    /// `{ ...place, v: <clone of place.v> }`: a new object, with a clone of
    /// each field that needs one.
    fn clone_fields(
        &mut self,
        place: Expr,
        fields: Vec<(String, Ty<'tcx>)>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let mut props = vec![Prop::Spread(place.clone())];
        for (name, t) in fields {
            if self.needs_clone(t) {
                let field = self.clone_value(Expr::member(place.clone(), name.clone()), t, span, out)?;
                props.push(Prop::Field(name, field));
            }
        }
        Ok(Expr::object(props))
    }

    /// A clone of an array: `items.slice()`, or `items.map((item) => ..)`
    /// if its items need cloning too.
    fn clone_items(&mut self, items: Expr, item: Ty<'tcx>, span: Span) -> R<Expr> {
        if !self.needs_clone(item) {
            return Ok(Expr::call(Expr::member(items, "slice"), Vec::new()));
        }
        let clone = self.clone_fn("item", item, span)?;
        Ok(Expr::call(Expr::member(items, "map"), vec![clone]))
    }

    /// `(name) => <clone of name>`.
    pub(super) fn clone_fn(&mut self, name: &str, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let mut body = Vec::new();
        let value = self.clone_value(Expr::var(name), ty, span, &mut body)?;
        body.push(StmtKind::Return(Some(value)).at(js::Span::NONE));
        Ok(Expr::arrow(vec![name.into()], body))
    }

    /// A derived `Default` of an enum is its `#[default]` variant, which
    /// has no fields: the constructor the derived body names, `Mode::Off`.
    fn default_variant(&self, default: DefId, ty: Ty<'tcx>) -> Option<DefId> {
        let tr = ty::TraitRef::new(self.tcx, default, [self.tcx.erase_and_anonymize_regions(ty)]);
        let Ok(ImplSource::UserDefined(imp)) = self.tcx.codegen_select_candidate(self.typing_env.as_query_input(tr))
        else {
            return None;
        };
        let method = self.tcx.associated_item_def_ids(imp.impl_def_id)[0].as_local()?;
        let mut expr = self.tcx.hir_body_owned_by(method).value;
        while let hir::ExprKind::Block(hir::Block { expr: Some(inner), .. }, _) = expr.kind {
            expr = inner;
        }
        let hir::ExprKind::Path(ref path) = expr.kind else {
            return None;
        };
        match self.tcx.typeck(method).qpath_res(path, expr.hir_id) {
            Res::Def(DefKind::Ctor(..), ctor) => Some(ctor),
            _ => None,
        }
    }

    /// `Default::default()` of `ty`: `0`, `""`, `[]`, a struct of its fields'
    /// defaults, or a call of a hand-written `default`.
    pub(super) fn default_value(&mut self, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let default = self
            .tcx
            .get_diagnostic_item(Symbol::intern("Default"))
            .expect("std has `Default`");
        if let ty::Param(_) = ty.kind() {
            let tr = ty::TraitRef::new(self.tcx, default, [ty]);
            return match self.evidence_for(tr) {
                Some(dictionary) => Ok(Expr::call(Expr::member(dictionary, "default"), Vec::new())),
                None => Err(self.unsupported(span, &format!("implementation evidence for `{tr}`"))),
            };
        }
        if self.has_user_impl(default, ty) {
            let method = self.tcx.associated_item_def_ids(default)[0];
            return self.impl_call(method, self.args_of(default, ty), Vec::new(), span);
        }
        self.check_value_ty(ty, span)?;
        let std = |name: &str| self.is_std_adt(ty, Symbol::intern(name));
        Ok(match ty.kind() {
            _ if Num::of(ty).is_some() => Expr::int(0),
            ty::Bool => Expr::bool(false),
            ty::Char => Expr::str("\0"),
            _ if ty.is_unit() || self.option_of(ty).is_some() => Expr::undefined(),
            _ if self.is_lang_adt(ty, LangItem::String) => Expr::str(""),
            _ if self.is_vec_like(ty) => Expr::array(Vec::new()),
            _ if self.is_map(ty) => Expr::new_(Expr::var(if self.is_set(ty) { "Set" } else { "Map" }), Vec::new()),
            ty::Adt(_, args) if ty.is_box() || std("Rc") => self.default_value(args.type_at(0), span)?,
            ty::Adt(_, args) if std("Cell") || std("RefCell") => Expr::object(vec![Prop::Field(
                "value".into(),
                self.default_value(args.type_at(0), span)?,
            )]),
            ty::Adt(adt, _) if adt.is_enum() && !self.is_std(adt.did()) => {
                let variant = self
                    .default_variant(default, ty)
                    .ok_or_else(|| self.unsupported(span, &format!("`Default` of `{ty}`")))?;
                Expr::str(variant_name(self.tcx, adt.variant_with_ctor_id(variant)))
            }
            ty::Adt(adt, _) if self.is_std(adt.did()) => {
                return Err(self.unsupported(span, &format!("`Default` of `{ty}`")));
            }
            ty::Adt(adt, _) if adt.is_struct() && adt.non_enum_variant().fields.is_empty() => Expr::undefined(),
            _ => match self.shape(ty) {
                Shape::Object(fields) => Expr::object(
                    fields
                        .into_iter()
                        .map(|(name, t)| Ok(Prop::Field(name, self.default_value(t, span)?)))
                        .collect::<R<_>>()?,
                ),
                Shape::Array(tys) => {
                    Expr::array(tys.into_iter().map(|t| self.default_value(t, span)).collect::<R<_>>()?)
                }
                Shape::Other => return Err(self.unsupported(span, &format!("`Default` of `{ty}`"))),
            },
        })
    }

    pub(super) fn partial_eq_trait(&self) -> DefId {
        self.tcx.require_lang_item(LangItem::PartialEq, rustc_span::DUMMY_SP)
    }

    /// Is `==` on `ty` JS's `===`: strings, numbers, `bool`s, `()` and
    /// fieldless enums, all JS primitives?
    fn is_primitive_eq(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        self.is_string_like(ty)
            || Num::of(ty).is_some()
            || ty.is_bool()
            || ty.is_unit()
            || matches!(ty.kind(), ty::Adt(adt, _) if is_fieldless_enum(*adt))
    }

    /// Does `==` on `ty` run code of its own anywhere in it: a hand-written
    /// `eq`, or a `T`'s, which might be one? If not, it compares field by
    /// field, element by element, which `$eq` does.
    pub(super) fn custom_eq(&self, ty: Ty<'tcx>) -> bool {
        self.custom_eq_in(ty, &mut Vec::new())
    }

    fn custom_eq_in(&self, ty: Ty<'tcx>, seen: &mut Vec<Ty<'tcx>>) -> bool {
        let ty = ty.peel_refs();
        if seen.contains(&ty) || self.is_primitive_eq(ty) {
            return false;
        }
        seen.push(ty);
        let custom = match ty.kind() {
            ty::Param(_) => true,
            ty::Tuple(tys) => tys.iter().any(|t| self.custom_eq_in(t, seen)),
            ty::Array(item, _) | ty::Slice(item) => self.custom_eq_in(*item, seen),
            ty::Adt(..) if self.has_user_impl(self.partial_eq_trait(), ty) => true,
            // `Vec`, `Box`, `Rc` and cells compare what they hold.
            ty::Adt(_, args) if self.is_std_wrapper(ty) => args.types().any(|t| self.custom_eq_in(t, seen)),
            ty::Adt(adt, args) => adt.all_fields().any(|f| self.custom_eq_in(f.ty(self.tcx, args), seen)),
            _ => false,
        };
        seen.pop();
        custom
    }

    /// `a == b` for values of type `ty`: `a === b` for JS primitives, a call
    /// of a hand-written `eq`, `TPartialEq.eq(a, b)` in generic code, and
    /// `$eq(a, b)` for what compares field by field. A derived `==` of a
    /// type with a custom part compares its parts one by one.
    pub(super) fn eq_value(&mut self, a: Expr, b: Expr, ty: Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let ty = ty.peel_refs();
        if self.is_primitive_eq(ty) {
            return Ok(Expr::bin(Op::Eq, a, b));
        }
        if let ty::Param(_) = ty.kind() {
            let tr = ty::TraitRef::new_from_args(
                self.tcx,
                self.partial_eq_trait(),
                self.args_of(self.partial_eq_trait(), ty),
            );
            if let Some(dictionary) = self.evidence_for(tr) {
                return Ok(Expr::call(Expr::member(dictionary, "eq"), vec![a, b]));
            }
            // A `T: Ord` or `T: PartialOrd` only: equal is `Equal` (ADR 0057).
            let ord = ty::TraitRef::new(self.tcx, self.ord_trait(), [ty]);
            let partial_ord = ty::TraitRef::new_from_args(
                self.tcx,
                self.partial_ord_trait(),
                self.args_of(self.partial_ord_trait(), ty),
            );
            if self.evidence_for(ord).is_some() || self.evidence_for(partial_ord).is_some() {
                let order = self.cmp_value(a, b, ty, self.evidence_for(ord).is_none(), span, out)?;
                return Ok(Expr::bin(Op::Eq, order, Expr::int(0)));
            }
            return Err(self.unsupported(span, &format!("implementation evidence for `{tr}`")));
        }
        if self.has_user_impl(self.partial_eq_trait(), ty) {
            let eq = self.tcx.associated_item_def_ids(self.partial_eq_trait())[0];
            let args = self.args_of(self.partial_eq_trait(), ty);
            return self.impl_call(eq, args, vec![a, b], span);
        }
        // A constant, like a fieldless variant's `"Nothing"`, is a JS
        // primitive: only itself is equal to it. `None` is also `null`.
        if a.is_constant() || b.is_constant() {
            let none = [&a, &b].iter().any(|x| matches!(x.kind, js::ExprKind::Undefined));
            let op = if none { Op::LooseEq } else { Op::Eq };
            return Ok(Expr::bin(op, a, b));
        }
        if let Some(inner) = self.option_of(ty) {
            // `==`, so that `null` and `undefined` are both `None`.
            if self.is_primitive_eq(inner) {
                return Ok(Expr::bin(Op::LooseEq, a, b));
            }
        }
        let structural = matches!(ty.kind(), ty::Tuple(_) | ty::Array(..) | ty::Slice(_))
            || self.is_std_wrapper(ty)
            || matches!(ty.kind(), ty::Adt(adt, _) if !self.is_std(adt.did()) || self.is_known_std(ty));
        if !structural || self.is_js_object(ty) || self.is_lang_adt(ty, LangItem::String) {
            return Err(self.unsupported(span, &format!("`==` on `{ty}`")));
        }
        if !self.custom_eq(ty) {
            self.runtime.insert(Helper::Eq);
            return Ok(Expr::call(Expr::var("$eq"), vec![a, b]));
        }
        // Each is read more than once below.
        let a = if a.reads_same() { a } else { self.spill("left", a, out) };
        let b = if b.reads_same() { b } else { self.spill("right", b, out) };
        let std = |name: &str| self.is_std_adt(ty, Symbol::intern(name));
        match ty.kind() {
            ty::Array(item, _) | ty::Slice(item) => self.eq_items(a, b, *item, span),
            ty::Adt(_, args) if self.is_vec_like(ty) => self.eq_items(a, b, args.type_at(0), span),
            ty::Adt(_, args) if ty.is_box() || std("Rc") => self.eq_value(a, b, args.type_at(0), span, out),
            ty::Adt(_, args) if std("Cell") || std("RefCell") => self.eq_value(
                Expr::member(a, "value"),
                Expr::member(b, "value"),
                args.type_at(0),
                span,
                out,
            ),
            ty::Adt(_, args) if self.option_of(ty).is_some() => {
                let inner = args.type_at(0);
                let (x, y) = if self.boxed_payload(inner) {
                    (self.some_value(a.clone()), self.some_value(b.clone()))
                } else {
                    (a.clone(), b.clone())
                };
                let some = self.eq_value(x, y, inner, span, out)?;
                let none = Expr::bin(
                    Op::Or,
                    Expr::bin(Op::LooseEq, a.clone(), Expr::null()),
                    Expr::bin(Op::LooseEq, b.clone(), Expr::null()),
                );
                Ok(Expr::cond(none, Expr::bin(Op::LooseEq, a, b), some))
            }
            ty::Adt(adt, args) if adt.is_enum() => {
                // `{ TAG: "Line", _0: .. }` (ADR 0033): a variant with a custom
                // part compares its fields, and `$eq` the rest.
                self.runtime.insert(Helper::Eq);
                let mut value = Expr::call(Expr::var("$eq"), vec![a.clone(), b.clone()]);
                for variant in adt.variants().iter().rev() {
                    let fields = self.variant_fields(variant, args);
                    if !fields.iter().any(|&(_, t)| self.custom_eq(t)) {
                        continue;
                    }
                    let name = variant_name(self.tcx, variant);
                    let tag = |x: &Expr| Expr::bin(Op::Eq, Expr::member(x.clone(), "TAG"), Expr::str(name.clone()));
                    let mut same = tag(&b);
                    for (field, t) in fields {
                        let field = self.eq_value(
                            Expr::member(a.clone(), field.clone()),
                            Expr::member(b.clone(), field),
                            t,
                            span,
                            out,
                        )?;
                        same = Expr::bin(Op::And, same, field);
                    }
                    value = Expr::cond(tag(&a), same, value);
                }
                Ok(value)
            }
            _ => {
                let parts: Vec<(Expr, Expr, Ty<'tcx>)> = match self.shape(ty) {
                    Shape::Object(fields) => fields
                        .into_iter()
                        .map(|(name, t)| (Expr::member(a.clone(), name.clone()), Expr::member(b.clone(), name), t))
                        .collect(),
                    Shape::Array(tys) => tys
                        .into_iter()
                        .enumerate()
                        .map(|(i, t)| {
                            (
                                Expr::index(a.clone(), Expr::int(i as i128)),
                                Expr::index(b.clone(), Expr::int(i as i128)),
                                t,
                            )
                        })
                        .collect(),
                    Shape::Other => return Err(self.unsupported(span, &format!("`==` on `{ty}`"))),
                };
                let mut all: Option<Expr> = None;
                for (x, y, t) in parts {
                    let part = self.eq_value(x, y, t, span, out)?;
                    all = Some(match all {
                        Some(all) => Expr::bin(Op::And, all, part),
                        None => part,
                    });
                }
                Ok(all.unwrap_or_else(|| Expr::bool(true)))
            }
        }
    }

    /// `a.length === b.length && a.every((x, i) => <x == b[i]>)`.
    fn eq_items(&mut self, a: Expr, b: Expr, item: Ty<'tcx>, span: Span) -> R<Expr> {
        let mut body = Vec::new();
        let other = Expr::index(b.clone(), Expr::var("i"));
        let same = self.eq_value(Expr::var("x"), other, item, span, &mut body)?;
        body.push(StmtKind::Return(Some(same)).at(js::Span::NONE));
        let every = Expr::call(
            Expr::member(a.clone(), "every"),
            vec![Expr::arrow(vec!["x".into(), "i".into()], body)],
        );
        let lengths = Expr::bin(Op::Eq, Expr::member(a, "length"), Expr::member(b, "length"));
        Ok(Expr::bin(Op::And, lengths, every))
    }

    /// `(a, b) => <a == b>` for a dictionary's `eq`, or `$eq` itself.
    pub(super) fn eq_fn(&mut self, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let mut body = Vec::new();
        let same = self.eq_value(Expr::var("a"), Expr::var("b"), ty, span, &mut body)?;
        if body.is_empty()
            && let js::ExprKind::Call(callee, _) = &same.kind
            && let js::ExprKind::Var(name) = &callee.kind
            && name == "$eq"
        {
            return Ok(Expr::var("$eq"));
        }
        body.push(StmtKind::Return(Some(same)).at(js::Span::NONE));
        Ok(Expr::arrow(vec!["a".into(), "b".into()], body))
    }
}

/// `!(a == b)`, as JS writes it: `a !== b` for `a === b`.
pub(super) fn negate(eq: Expr) -> Expr {
    match eq.kind {
        js::ExprKind::Binary(Op::Eq, a, b) => Expr::bin(Op::Ne, *a, *b),
        js::ExprKind::Binary(Op::LooseEq, a, b) => Expr::bin(Op::LooseNe, *a, *b),
        js::ExprKind::Binary(Op::Ne, a, b) => Expr::bin(Op::Eq, *a, *b),
        js::ExprKind::Binary(Op::LooseNe, a, b) => Expr::bin(Op::LooseEq, *a, *b),
        _ => Expr::unary(UnaryOp::Not, eq),
    }
}
