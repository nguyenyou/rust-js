//! `Clone` and `Default` where rust-js writes the implementation: derived
//! ones and std types' (ADR 0052). A hand-written one is called instead.

use super::bindings::variant_name;
use super::representation::Num;
use super::{FnCx, R, Shape};
use crate::js::{self, Expr, Op, Prop, Stmt, StmtKind};
use rustc_hir as hir;
use rustc_hir::LangItem;
use rustc_hir::def::{DefKind, Res};
use rustc_middle::traits::ImplSource;
use rustc_middle::ty::{self, Ty};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, sym};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// Does `ty` use a hand-written impl of `trait_id` from this crate?
    pub(super) fn has_user_impl(&self, trait_id: DefId, ty: Ty<'tcx>) -> bool {
        let tr = ty::TraitRef::new(self.tcx, trait_id, [self.tcx.erase_and_anonymize_regions(ty)]);
        matches!(self.tcx.codegen_select_candidate(self.typing_env.as_query_input(tr)),
            Ok(ImplSource::UserDefined(imp)) if self.krate.trait_impls.contains(&imp.impl_def_id))
    }

    /// `method` of `Self = ty`'s hand-written impl, called directly:
    /// `counterClone_clone(c)`.
    fn impl_call(&mut self, method: DefId, ty: Ty<'tcx>, mut values: Vec<Expr>, span: Span) -> R<Expr> {
        let args = self.tcx.mk_args(&[self.tcx.erase_and_anonymize_regions(ty).into()]);
        let instance = ty::Instance::try_resolve(self.tcx, self.typing_env, method, args)?
            .filter(|i| self.krate.fns.contains_key(&i.def_id()))
            .ok_or_else(|| self.unsupported(span, &format!("this implementation for `{ty}`")))?;
        values.extend(self.evidence_args(instance.def_id(), instance.args, span)?);
        Ok(Expr::call(self.fn_ref(instance.def_id()), values))
    }

    fn clone_trait(&self) -> DefId {
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
            ty::Adt(_, args) if std("Vec") => self.vec_changed(ty) || self.needs_clone_in(args.type_at(0), seen),
            ty::Adt(..) if std("Cell") || std("RefCell") => true,
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
            return self.impl_call(method, ty, vec![place], span);
        }
        // A constant, like `"Dot"`, is a value no one else holds.
        if place.is_constant() {
            return Ok(place);
        }
        if self.is_copy(ty) {
            return Ok(self.copy(place, ty));
        }
        // Read more than once below.
        let place = if place.has_effects() {
            self.spill("value", place, out)
        } else {
            place
        };
        let std = |name: &str| self.is_std_adt(ty, Symbol::intern(name));
        match ty.kind() {
            ty::Array(item, _) => self.clone_items(place, *item, span),
            ty::Adt(_, args) if std("Vec") => self.clone_items(place, args.type_at(0), span),
            ty::Adt(_, args) if ty.is_box() => self.clone_value(place, args.type_at(0), span, out),
            ty::Adt(_, args) if std("Cell") || std("RefCell") => {
                let value = self.clone_value(Expr::member(place, "value"), args.type_at(0), span, out)?;
                Ok(Expr::object(vec![Prop::Field("value".into(), value)]))
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
            return self.impl_call(method, ty, Vec::new(), span);
        }
        self.check_value_ty(ty, span)?;
        let std = |name: &str| self.is_std_adt(ty, Symbol::intern(name));
        Ok(match ty.kind() {
            _ if Num::of(ty).is_some() => Expr::int(0),
            ty::Bool => Expr::bool(false),
            ty::Char => Expr::str("\0"),
            _ if ty.is_unit() || self.option_of(ty).is_some() => Expr::undefined(),
            _ if self.is_lang_adt(ty, LangItem::String) => Expr::str(""),
            _ if std("Vec") => Expr::array(Vec::new()),
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
}
