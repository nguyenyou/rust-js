//! `PartialOrd` and `Ord` (ADR 0057): a comparison is an `Ordering`, -1, 0
//! or 1 (ADR 0036), and `partial_cmp`'s `None` is `undefined`. JS's own `<`
//! for what it orders as Rust does, `$cmp`, a hand-written `cmp`, or the
//! parts compared in turn: `$cmp(a.x, b.x) || $cmp(a.y, b.y)`, since `Equal`
//! is the one that's falsy.

use super::bindings::variant_name;
use super::representation::Num;
use super::{FnCx, R, Shape};
use crate::js::{self, Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_middle::traits::ImplSource;
use rustc_middle::ty::{self, Ty};
use rustc_span::def_id::DefId;
use rustc_span::{DUMMY_SP, Span, Symbol};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    pub(super) fn ord_trait(&self) -> DefId {
        self.tcx
            .get_diagnostic_item(rustc_span::sym::Ord)
            .expect("std has `Ord`")
    }

    pub(super) fn partial_ord_trait(&self) -> DefId {
        self.tcx.require_lang_item(LangItem::PartialOrd, DUMMY_SP)
    }

    /// `std::cmp::Reverse`, which has no diagnostic item of its own.
    pub(super) fn is_reverse(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.crate_name(adt.did().krate) == rustc_span::sym::core
            && self.tcx.item_name(adt.did()).as_str() == "Reverse")
    }

    /// Does JS's `<` order `ty` as Rust does? Numbers, strings, `char`s,
    /// `bool`s (`false < true`), and `Ordering`s, which are numbers.
    pub(super) fn is_primitive_ord(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        Num::of(ty).is_some() || self.is_string_like(ty) || ty.is_bool() || self.is_lang_adt(ty, LangItem::OrderingEnum)
    }

    /// Is `ty` `Ord`, so that its `partial_cmp` is never `None`?
    fn is_total(&self, ty: Ty<'tcx>) -> bool {
        let tr = ty::TraitRef::new(self.tcx, self.ord_trait(), [self.tcx.erase_and_anonymize_regions(ty)]);
        matches!(
            self.tcx.codegen_select_candidate(self.typing_env.as_query_input(tr)),
            Ok(ImplSource::UserDefined(_) | ImplSource::Param(_) | ImplSource::Builtin(..))
        )
    }

    /// `a.cmp(&b)`, or `a.partial_cmp(&b)` if `partial`, of `ty` values: an
    /// `Ordering`, and for `partial_cmp`, `undefined` when there's none.
    pub(super) fn cmp_value(
        &mut self,
        a: Expr,
        b: Expr,
        ty: Ty<'tcx>,
        partial: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let ty = ty.peel_refs();
        // `f64`'s `NaN` isn't ordered at all.
        if partial && Num::of(ty) == Some(Num::F64) {
            self.runtime.insert(Helper::PartialCmp);
            return Ok(Expr::call(Expr::var("$partialCmp"), vec![a, b]));
        }
        if self.is_primitive_ord(ty) {
            self.runtime.insert(Helper::Cmp);
            return Ok(Expr::call(Expr::var("$cmp"), vec![a, b]));
        }
        let (ord, partial_ord) = (self.ord_trait(), self.partial_ord_trait());
        if let ty::Param(_) = ty.kind() {
            let total = ty::TraitRef::new(self.tcx, ord, [ty]);
            if let Some(dictionary) = self.evidence_for(total) {
                return Ok(Expr::call(Expr::member(dictionary, "cmp"), vec![a, b]));
            }
            let tr = ty::TraitRef::new_from_args(self.tcx, partial_ord, self.args_of(partial_ord, ty));
            let dictionary = self
                .evidence_for(tr)
                .ok_or_else(|| self.unsupported(span, &format!("implementation evidence for `{tr}`")))?;
            return Ok(Expr::call(Expr::member(dictionary, "partial_cmp"), vec![a, b]));
        }
        // A hand-written one: `cmp`, or `partial_cmp` if that's what's asked
        // for, or all there is.
        let own_ord = self.has_user_impl(ord, ty);
        let own_partial = self.has_user_impl(partial_ord, ty);
        if own_ord && (!partial || !own_partial) {
            let cmp = self.method(ord, "cmp");
            return self.impl_call(cmp, self.args_of(ord, ty), vec![a, b], span);
        }
        if own_partial {
            let cmp = self.method(partial_ord, "partial_cmp");
            return self.impl_call(cmp, self.args_of(partial_ord, ty), vec![a, b], span);
        }
        // Derived. Each is read more than once below.
        let a = if a.reads_same() { a } else { self.spill("left", a, out) };
        let b = if b.reads_same() { b } else { self.spill("right", b, out) };
        let std = |name: &str| self.is_std_adt(ty, Symbol::intern(name));
        match ty.kind() {
            _ if let Some(inner) = self.option_of(ty) => {
                // `None` is less than any `Some`.
                let (x, y) = if self.boxed_payload(inner) {
                    (self.some_value(a.clone()), self.some_value(b.clone()))
                } else {
                    (a.clone(), b.clone())
                };
                let some = self.cmp_value(x, y, inner, partial, span, out)?;
                let none = |x: &Expr| Expr::bin(Op::LooseEq, x.clone(), Expr::null());
                Ok(Expr::cond(
                    none(&a),
                    Expr::cond(none(&b), Expr::int(0), Expr::int(-1)),
                    Expr::cond(none(&b), Expr::int(1), some),
                ))
            }
            ty::Adt(_, args) if ty.is_box() || std("Rc") => self.cmp_value(a, b, args.type_at(0), partial, span, out),
            // `Reverse(x)`: `x`s the other way round, as its impl has it.
            ty::Adt(_, args) if self.is_reverse(ty) => {
                let inside = |x: Expr| Expr::index(x, Expr::int(0));
                self.cmp_value(inside(b), inside(a), args.type_at(0), partial, span, out)
            }
            ty::Adt(_, args) if self.is_vec_like(ty) => self.cmp_items(a, b, args.type_at(0), partial, span),
            ty::Array(item, _) | ty::Slice(item) => self.cmp_items(a, b, *item, partial, span),
            // A fieldless enum: by the order its variants are declared in.
            ty::Adt(adt, _) if adt.is_enum() && adt.variants().iter().all(|v| v.fields.is_empty()) => {
                let names = adt
                    .variants()
                    .iter()
                    .map(|v| Expr::str(variant_name(self.tcx, v)))
                    .collect();
                self.runtime.extend([Helper::CmpIn, Helper::Cmp]);
                Ok(Expr::call(Expr::var("$cmpIn"), vec![Expr::array(names), a, b]))
            }
            ty::Adt(adt, _) if adt.is_enum() => Err(self.unsupported(span, &format!("comparing `{ty}`s"))),
            // Another crate's struct orders as its impl says, which may not be
            // field by field.
            ty::Adt(adt, _) if !adt.did().is_local() => Err(self.unsupported(span, &format!("comparing `{ty}`s"))),
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
                            let at = |x: &Expr| Expr::index(x.clone(), Expr::int(i as i128));
                            (at(&a), at(&b), t)
                        })
                        .collect(),
                    Shape::Other => return Err(self.unsupported(span, &format!("comparing `{ty}`s"))),
                };
                // Each part in turn, until one isn't `Equal`. An unordered
                // one (`undefined`) is falsy too, so where one can be, the
                // parts go through `$thenCmp`, which stops at it.
                let total = !partial || parts.iter().all(|&(_, _, t)| self.is_total(t));
                let mut orders = Vec::new();
                for (x, y, t) in parts {
                    orders.push(self.cmp_value(x, y, t, partial, span, out)?);
                }
                if orders.is_empty() {
                    return Ok(Expr::int(0));
                }
                if total {
                    return Ok(orders
                        .into_iter()
                        .reduce(|all, next| Expr::bin(Op::Or, all, next))
                        .expect("some parts"));
                }
                self.runtime.insert(Helper::ThenCmp);
                Ok(Expr::call(Expr::var("$thenCmp"), orders))
            }
        }
    }

    /// Two sequences, item by item, then by length: `$cmpItems(a, b, $cmp)`.
    fn cmp_items(&mut self, a: Expr, b: Expr, item: Ty<'tcx>, partial: bool, span: Span) -> R<Expr> {
        let compare = self.cmp_fn(item, partial, span)?;
        self.runtime.extend([Helper::CmpItems, Helper::Cmp]);
        Ok(Expr::call(Expr::var("$cmpItems"), vec![a, b, compare]))
    }

    /// `(a, b) => <their Ordering>`, or the function itself: `$cmp`,
    /// `wordOrd_cmp`, or a dictionary's, `TOrd.cmp`, which needs no `this`.
    pub(super) fn cmp_fn(&mut self, ty: Ty<'tcx>, partial: bool, span: Span) -> R<Expr> {
        let mut body = Vec::new();
        let order = self.cmp_value(Expr::var("a"), Expr::var("b"), ty, partial, span, &mut body)?;
        if body.is_empty()
            && let js::ExprKind::Call(callee, args) = &order.kind
            && callee.reads_same()
            && matches!(args.as_slice(), [x, y] if is_var(x, "a") && is_var(y, "b"))
        {
            return Ok((**callee).clone());
        }
        body.push(StmtKind::Return(Some(order)).at(js::Span::NONE));
        Ok(Expr::arrow(vec!["a".into(), "b".into()], body))
    }

    fn method(&self, trait_id: DefId, name: &str) -> DefId {
        self.tcx
            .associated_item_def_ids(trait_id)
            .iter()
            .copied()
            .find(|&id| self.tcx.item_name(id).as_str() == name)
            .expect("the trait has the method")
    }

    /// `a < b` and the rest, `a.cmp(&b)`, `a.partial_cmp(&b)`, `a.max(b)`
    /// and `a.min(b)`. `None` for numbers, which std's operators already are.
    pub(super) fn ordering_call(
        &mut self,
        id: DefId,
        tr: ty::TraitRef<'tcx>,
        values: Vec<Expr>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Option<Expr>> {
        let ty = tr.self_ty();
        let partial = tr.def_id == self.partial_ord_trait();
        if !partial && tr.def_id != self.ord_trait() {
            return Ok(None);
        }
        let name = self.tcx.item_name(id);
        let operator = match name.as_str() {
            "lt" => Some(Op::Lt),
            "le" => Some(Op::Le),
            "gt" => Some(Op::Gt),
            "ge" => Some(Op::Ge),
            _ => None,
        };
        let numbers = Num::of(ty.peel_refs()).is_some() && name.as_str() != "partial_cmp";
        if numbers || (operator.is_none() && !matches!(name.as_str(), "cmp" | "partial_cmp" | "max" | "min")) {
            return Ok(None);
        }
        let [a, b]: [Expr; 2] = values
            .try_into()
            .map_err(|_| self.unsupported(span, "this comparison"))?;
        // JS's own `<` orders strings and `bool`s as Rust does.
        if let Some(op) = operator
            && self.is_primitive_ord(ty)
        {
            return Ok(Some(Expr::bin(op, a, b)));
        }
        // `max` and `min` return one of them, so each is read twice.
        let (a, b) = if matches!(name.as_str(), "max" | "min") {
            let a = if a.reads_same() { a } else { self.spill("left", a, out) };
            let b = if b.reads_same() { b } else { self.spill("right", b, out) };
            (a, b)
        } else {
            (a, b)
        };
        // `partial_cmp`'s `undefined` makes every one of these false, as `None` does.
        let order = self.cmp_value(a.clone(), b.clone(), ty, partial, span, out)?;
        let zero = || Expr::int(0);
        Ok(Some(match name.as_str() {
            _ if let Some(op) = operator => Expr::bin(op, order, zero()),
            // The second when they're equal, as Rust's `max` does.
            "max" => Expr::cond(Expr::bin(Op::Gt, order, zero()), a, b),
            "min" => Expr::cond(Expr::bin(Op::Gt, order, zero()), b, a),
            _ => order,
        }))
    }
}

fn is_var(e: &Expr, name: &str) -> bool {
    matches!(&e.kind, js::ExprKind::Var(n) if n == name)
}
