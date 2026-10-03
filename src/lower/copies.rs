//! When a value is copied. Rust copies a `Copy` value where it's read, and
//! JS shares an object: a copy is made only where one of them may change in
//! place, which is found once for each type (ADRs 0020, 0052, 0100).

use super::{FnCx, Shape};
use crate::js;
use crate::js::{Expr, Op, Prop};
use rustc_ast::Mutability;
use rustc_hir::LangItem;
use rustc_middle::ty;
use rustc_middle::ty::{Ty, TyCtxt};
use rustc_span::{Symbol, sym};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// A fresh `ty` value equal to the one at `place`: `{ ...p }`, `[t[0], t[1]]`.
    /// A field that also contains mutated types is copied in turn.
    pub(super) fn copy(&self, place: Expr, ty: Ty<'tcx>) -> Expr {
        if self.is_unknown(ty)
            && let Some((_, dictionary)) = self
                .given
                .evidence
                .iter()
                .find(|(tr, _)| tr.self_ty() == ty && self.tcx.is_lang_item(tr.def_id, LangItem::Copy))
        {
            return Expr::call(Expr::member(dictionary.clone(), "copy"), vec![place]);
        }
        // A copy reads its source once per part: a value that isn't a place,
        // like `$unwrap(v[0])`, is taken once, `((value) => ..)(source)`.
        let many = match self.shape(ty) {
            Shape::Object(fields) => fields.iter().any(|&(_, t)| self.contains_mutated(t)),
            Shape::Array(tys) => tys.len() > 1,
            Shape::Other => false,
        };
        if many && !place.reads_same() {
            let body = self.copy(Expr::var("value"), ty);
            return Expr::call(
                Expr::arrow(
                    vec!["value".into()],
                    vec![js::StmtKind::Return(Some(body)).at(js::Span::NONE)],
                ),
                vec![place],
            );
        }
        match self.shape(ty) {
            Shape::Object(fields) => {
                let mut props = vec![Prop::Spread(place.clone())];
                for (name, t) in fields {
                    if self.contains_mutated(t) {
                        let field = self.copy(Expr::member(place.clone(), name.clone()), t);
                        props.push(Prop::Field(name, field));
                    }
                }
                Expr::object(props)
            }
            Shape::Array(tys) => Expr::array(
                tys.into_iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let item = Expr::index(place.clone(), Expr::int(i as i128));
                        if self.contains_mutated(t) {
                            self.copy(item, t)
                        } else {
                            item
                        }
                    })
                    .collect(),
            ),
            Shape::Other => {
                // Read more than once: a value that isn't a place is taken once.
                let once = |place: Expr, copy: &dyn Fn(Expr) -> Expr| {
                    if place.reads_same() {
                        copy(place)
                    } else {
                        let body = copy(Expr::var("value"));
                        Expr::call(
                            Expr::arrow(
                                vec!["value".into()],
                                vec![js::StmtKind::Return(Some(body)).at(js::Span::NONE)],
                            ),
                            vec![place],
                        )
                    }
                };
                if let Some(inner) = self.option_of(ty) {
                    if !self.contains_mutated(inner) {
                        return place;
                    }
                    return once(place, &|o| {
                        let none = Expr::bin(Op::LooseEq, o.clone(), Expr::null());
                        Expr::cond(none, o.clone(), self.copy(o, inner))
                    });
                }
                // An array that's changed in place: `a.slice()`, or a copy of
                // each item that is too.
                if let ty::Array(item, _) = ty.kind() {
                    if !self.contains_mutated(*item) {
                        return Expr::call(Expr::member(place, "slice"), Vec::new());
                    }
                    let body = self.copy(Expr::var("item"), *item);
                    let copy = Expr::arrow(
                        vec!["item".into()],
                        vec![js::StmtKind::Return(Some(body)).at(js::Span::NONE)],
                    );
                    return Expr::call(Expr::member(place, "map"), vec![copy]);
                }
                let ty::Adt(adt, args) = ty.kind() else { return place };
                if !self.is_copy_enum(ty) {
                    return place;
                }
                // `{ TAG: "Line", _0: .. }`: a variant with a part that changes
                // gets a copy, and every other value is itself.
                once(place, &|e| {
                    let mut value = e.clone();
                    // Changed in place itself, through a `&mut`: every variant with fields.
                    let itself = self.mutated_itself(ty);
                    for variant in adt.variants().iter().rev() {
                        let fields = self.variant_fields(variant, args);
                        if fields.is_empty() || !(itself || fields.iter().any(|&(_, t)| self.contains_mutated(t))) {
                            continue;
                        }
                        let mut props = vec![Prop::Spread(e.clone())];
                        for (name, t) in fields {
                            if self.contains_mutated(t) {
                                props.push(Prop::Field(name.clone(), self.copy(Expr::member(e.clone(), name), t)));
                            }
                        }
                        let tag = Expr::bin(
                            Op::Eq,
                            Expr::member(e.clone(), "TAG"),
                            Expr::str(super::bindings::variant_name(self.tcx, variant)),
                        );
                        value = Expr::cond(tag, Expr::object(props), value);
                    }
                    value
                })
            }
        }
    }

    /// Rust copies a `Copy` value when it's read, and JS objects are shared
    /// references. The two only disagree if one of the copies is later
    /// changed in place, which needs a type in `mutated`. So only those
    /// types are copied, and everything else stays shared.
    pub(super) fn copy_if_needed(&self, place: Expr, ty: Ty<'tcx>) -> Expr {
        if self.contains_mutated(ty) && self.is_copy(ty) {
            self.copy(place, ty)
        } else {
            place
        }
    }

    pub(super) fn is_copy(&self, ty: Ty<'tcx>) -> bool {
        self.tcx.type_is_copy_modulo_regions(self.typing_env, ty)
    }

    /// An enum rust-js writes as ADR 0033 says, that's `Copy`: one of the
    /// crate's own, or a library's (ADR 0100), or `Result`.
    pub(super) fn is_copy_enum(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if adt.is_enum()
            && (adt.did().is_local() || self.krate.foreign.in_library(adt.did()) || self.is_std_adt(ty, sym::Result)))
            && self.is_copy(ty)
    }

    pub(super) fn contains_mutated(&self, ty: Ty<'tcx>) -> bool {
        // Each type once: a type met along two paths isn't walked twice. It
        // ends at a `Box`, which is `Other`, so no answer depends on another.
        if let Some(&mutated) = self.walks.mutated.borrow().get(&ty) {
            return mutated;
        }
        let mutated = self.contains_mutated_uncached(ty);
        self.walks.mutated.borrow_mut().insert(ty, mutated);
        mutated
    }

    pub(super) fn contains_mutated_uncached(&self, ty: Ty<'tcx>) -> bool {
        self.is_unknown(ty)
            || self.mutated_itself(ty)
            || match self.shape(ty) {
                Shape::Object(fields) => fields.iter().any(|&(_, t)| self.contains_mutated(t)),
                Shape::Array(tys) => tys.iter().any(|&t| self.contains_mutated(t)),
                // `Some(x)` is `x` (ADR 0030), and a variant's fields are
                // its object's (ADR 0033).
                Shape::Other => match ty.kind() {
                    _ if let Some(inner) = self.option_of(ty) => self.contains_mutated(inner),
                    ty::Adt(adt, args) if self.is_copy_enum(ty) => adt.variants().iter().any(|v| {
                        self.variant_fields(v, args)
                            .iter()
                            .any(|&(_, t)| self.contains_mutated(t))
                    }),
                    _ => false,
                },
            }
    }

    /// Is `ty` itself changed in place somewhere, not just a part of it?
    pub(super) fn mutated_itself(&self, ty: Ty<'tcx>) -> bool {
        self.crosses_crates(ty) || self.krate.mutated.iter().any(|&mutated| self.instance_of(ty, mutated))
    }

    /// Might another crate change `ty` in place (ADR 0100)? Any crate using a
    /// library's type might, and so might a library's consumers its own type
    /// they can reach. This crate can't see their code, so it assumes they do,
    /// of a type that's a JS object: a fieldless enum's string can't change.
    /// A tuple or an array is any crate's, so in a library, or a crate using
    /// one, any may cross.
    pub(super) fn crosses_crates(&self, ty: Ty<'tcx>) -> bool {
        let shared = match ty.kind() {
            ty::Adt(adt, _) => {
                self.krate.foreign.in_library(adt.did())
                    || (self.krate.library && super::library::reachable(self.tcx, adt.did()))
            }
            ty::Tuple(_) | ty::Array(..) => self.krate.library || self.krate.foreign.any(),
            _ => false,
        };
        shared && self.is_object(ty)
    }

    /// Is `ty` a `Vec` type something may change (ADR 0052)? One another crate
    /// can hold might be changed there (ADR 0100): in a library, or a crate
    /// using one, any may be.
    pub(super) fn vec_changed(&self, ty: Ty<'tcx>) -> bool {
        (self.krate.library || self.krate.foreign.any())
            || self
                .krate
                .changed_vecs
                .iter()
                .any(|&changed| self.instance_of(ty, changed))
    }

    /// Is `ty` one of the types `general` stands for? A type mutated in a
    /// generic function has its parameters: `Holder<T>` stands for every
    /// `Holder<..>`, but `Pair<u32>` only for itself, so a `Pair<bool>`
    /// needn't be copied because a `Pair<u32>` is changed. Lifetimes don't
    /// matter.
    pub(super) fn instance_of(&self, ty: Ty<'tcx>, general: Ty<'tcx>) -> bool {
        match (ty.kind(), general.kind()) {
            _ if self.is_unknown(general) => true,
            (ty::Adt(adt, args), ty::Adt(general_adt, general_args)) => {
                adt.did() == general_adt.did()
                    && args.iter().zip(general_args.iter()).all(|(arg, general)| {
                        match (arg.as_type(), general.as_type()) {
                            (Some(arg), Some(general)) => self.instance_of(arg, general),
                            _ => true,
                        }
                    })
            }
            (ty::Ref(_, inner, _), ty::Ref(_, general, _))
            | (ty::Slice(inner), ty::Slice(general))
            | (ty::Array(inner, _), ty::Array(general, _)) => self.instance_of(*inner, *general),
            (ty::Tuple(parts), ty::Tuple(general)) => {
                parts.len() == general.len() && parts.iter().zip(general.iter()).all(|(p, g)| self.instance_of(p, g))
            }
            _ => self.tcx.erase_and_anonymize_regions(ty) == self.tcx.erase_and_anonymize_regions(general),
        }
    }

    /// Whether reconstructing a value can replace its Clone implementation.
    /// References and Rc share their referent; owned fields must all qualify.
    pub(super) fn structural_clone(&self, ty: Ty<'tcx>) -> bool {
        self.structural_clone_in(ty, &mut Vec::new())
    }

    pub(super) fn structural_clone_in(&self, ty: Ty<'tcx>, seen: &mut Vec<Ty<'tcx>>) -> bool {
        if seen.contains(&ty) {
            return true;
        }
        seen.push(ty);
        let structural = match ty.kind() {
            ty::Ref(..) => true,
            ty::Param(_) | ty::Alias(..) | ty::Dynamic(..) => false,
            ty::Tuple(parts) => parts.iter().all(|t| self.structural_clone_in(t, seen)),
            ty::Array(item, _) | ty::Slice(item) => self.structural_clone_in(*item, seen),
            ty::Adt(..)
                if self.is_std_adt(ty, Symbol::intern("Rc")) || self.is_string_like(ty) || self.is_js_object(ty) =>
            {
                true
            }
            ty::Adt(..) if self.has_user_impl(self.clone_trait(), ty) => false,
            ty::Adt(_, args) if self.is_std_wrapper(ty) || self.is_map(ty) => {
                args.types().all(|t| self.structural_clone_in(t, seen))
            }
            ty::Adt(adt, args) => adt
                .all_fields()
                .all(|f| self.structural_clone_in(f.ty(self.tcx, args).skip_normalization(), seen)),
            _ => true,
        };
        seen.pop();
        structural
    }
}

/// Can one JS value be every use of a constant of `ty`, which Rust makes
/// afresh at each? A function, a closure, a shared reference, a `dyn`
/// behind one, a number and a string can't change in place; an array, a
/// tuple, an `Option` and the crate's own struct or enum of those can, and
/// is copied where it's read if something changes one, as any value is.
pub(super) fn shareable<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> bool {
    fn walk<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>, seen: &mut Vec<Ty<'tcx>>) -> bool {
        if seen.contains(&ty) {
            return true;
        }
        seen.push(ty);
        let frozen = match ty.kind() {
            ty::Bool | ty::Char | ty::Int(_) | ty::Uint(_) | ty::Float(_) | ty::Str => true,
            ty::FnPtr(..) | ty::FnDef(..) | ty::Closure(..) | ty::Dynamic(..) => true,
            ty::Ref(_, inner, Mutability::Not) => walk(tcx, *inner, seen),
            ty::Array(item, _) | ty::Slice(item) => walk(tcx, *item, seen),
            ty::Tuple(items) => items.iter().all(|item| walk(tcx, item, seen)),
            ty::Adt(adt, args) if tcx.is_lang_item(adt.did(), LangItem::Option) => walk(tcx, args.type_at(0), seen),
            ty::Adt(adt, args) if adt.did().is_local() && !adt.is_union() => adt
                .all_fields()
                .all(|field| walk(tcx, field.ty(tcx, args).skip_normalization(), seen)),
            _ => false,
        };
        seen.pop();
        frozen
    }
    walk(tcx, ty, &mut Vec::new())
}
