//! `Option`'s and `Result`'s combinators, more iterator adapters, and more
//! of `Vec`'s methods (ADR 0062). A closure whose body is one value is
//! written in place, as `Option::map`'s is: `o ?? f()` is `o ?? 0` for
//! `unwrap_or_else(|| 0)`.

use super::{FnCx, R, Std, representation::Num};
use crate::js::{self, Expr, Op, Prop, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_middle::thir::{AdtExprBase, ExprId, ExprKind};
use rustc_middle::ty::{self, Ty};
use rustc_span::{Span, Symbol};

/// An `Option`, `Result` or `Vec` method (ADR 0062).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Comb {
    UnwrapOrElse,
    UnwrapOrDefault,
    MapOr,
    MapOrElse,
    AndThen,
    Filter,
    OkOr,
    OkOrElse,
    Or,
    OrElse,
    IsSomeAnd,
    IsNoneOr,
    ResultMap,
    MapErr,
    ResultAndThen,
    ResultUnwrapOrElse,
    ResultUnwrapOrDefault,
    Err,
    IsOkAnd,
    IsErrAnd,
    Contains,
    BinarySearch,
    /// `b.then(|| x)` and `b.then_some(x)`: `b ? x : undefined`.
    Then,
    ThenSome,
    Extend,
    Insert,
    Remove,
    Swap,
    Truncate,
    Dedup,
    Windows,
    Chunks,
    Concat,
}

/// An iterator adapter or consumer (ADR 0062), over an array or a JS iterator.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum IterComb {
    FilterMap,
    FlatMap,
    Flatten,
    Zip,
    Chain,
    TakeWhile,
    SkipWhile,
    StepBy,
    MaxByKey(bool),
    MaxBy(bool),
    Product,
    Nth,
    FindMap,
    Partition,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// `vec![x; n]`: `new Array(n).fill(x)`, when copies of `x` can't be
    /// told apart (ADR 0052). Otherwise each item is its own, as Rust clones
    /// it: made again, `Array.from({ length: n }, () => new Array(m).fill(0))`,
    /// if that makes the same value and does nothing else, or cloned.
    pub(super) fn vec_of_copies(&mut self, args: &[ExprId], span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let item_ty = self.thir[args[0]].ty;
        let rebuilt = self.rebuilt(args[0]);
        let [item, n]: [Expr; 2] = self.operands(args, out)?.try_into().ok().expect("an item and a count");
        if !self.needs_clone(item_ty) {
            let array = Expr::new_(Expr::var("Array"), vec![n]);
            return Ok(Expr::call(Expr::member(array, "fill"), vec![item]));
        }
        let body = if rebuilt {
            vec![StmtKind::Return(Some(item)).at(js::Span::NONE)]
        } else {
            let item = if item.reads_same() {
                item
            } else {
                self.spill("item", item, out)
            };
            let mut body = Vec::new();
            let copy = self.clone_value(item, item_ty, span, &mut body)?;
            body.push(StmtKind::Return(Some(copy)).at(js::Span::NONE));
            body
        };
        let length = Expr::object(vec![Prop::Field("length".into(), n)]);
        let from = Expr::member(Expr::var("Array"), "from");
        Ok(Expr::call(from, vec![length, Expr::arrow(Vec::new(), body)]))
    }

    /// Does evaluating `e` again make a value that's the same as a clone of
    /// it, and do nothing else? `vec![0; m]`, `Vec::new()`, or a tuple,
    /// array or struct of such parts and of values that need no copy.
    fn rebuilt(&self, e: ExprId) -> bool {
        let e = self.strip(e);
        let part = |p: ExprId| self.rebuilt(p) || (!self.needs_clone(self.thir[p].ty) && self.pure(p));
        match self.thir[e].kind {
            ExprKind::Call { fun, ref args, .. } => match self.std_fn(fun) {
                Some(Std::FromElem) => part(args[0]) && self.pure(args[1]),
                Some(Std::VecNew | Std::StringNew) => true,
                _ => false,
            },
            ExprKind::Tuple { ref fields } | ExprKind::Array { ref fields } => fields.iter().all(|&f| part(f)),
            ExprKind::Adt(ref adt) => matches!(adt.base, AdtExprBase::None) && adt.fields.iter().all(|f| part(f.expr)),
            _ => false,
        }
    }

    /// A value read without doing anything: a literal, a variable, a constant.
    fn pure(&self, e: ExprId) -> bool {
        matches!(
            self.thir[self.strip(e)].kind,
            ExprKind::Literal { .. }
                | ExprKind::NonHirLiteral { .. }
                | ExprKind::ZstLiteral { .. }
                | ExprKind::NamedConst { .. }
                | ExprKind::VarRef { .. }
                | ExprKind::UpvarRef { .. }
        )
    }

    /// `f(args)`, with a closure that only returns written in place. One of
    /// statements gets a name first: `const f = (x) => { .. }; f(o)`.
    fn call_with(&mut self, f: Expr, args: Vec<Expr>, name: &str, out: &mut Vec<Stmt>) -> Expr {
        let args: Vec<Expr> = args
            .into_iter()
            .map(|a| if a.reads_same() { a } else { self.spill("value", a, out) })
            .collect();
        let applied = super::calls::apply(f.clone(), args.clone());
        match &applied.kind {
            js::ExprKind::Call(callee, _) if matches!(callee.kind, js::ExprKind::Arrow(..)) => {
                let f = self.spill(name, f, out);
                Expr::call(f, args)
            }
            _ => applied,
        }
    }

    fn ok(value: Expr) -> Expr {
        Expr::object(vec![
            Prop::Field("TAG".into(), Expr::str("Ok")),
            Prop::Field("_0".into(), value),
        ])
    }

    fn err(value: Expr) -> Expr {
        Expr::object(vec![
            Prop::Field("TAG".into(), Expr::str("Err")),
            Prop::Field("_0".into(), value),
        ])
    }

    /// One of `Comb`'s: `args[0]` the `Option`, `Result` or `Vec`.
    pub(super) fn comb_call(
        &mut self,
        comb: Comb,
        args: &[ExprId],
        generic_args: ty::GenericArgsRef<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let subject_ty = self.thir[args[0]].ty.peel_refs();
        // An `Option` of a generic `T` may be boxed (ADR 0051): not these yet.
        if let Some(inner) = self.option_of(subject_ty)
            && self.boxed_payload(inner)
        {
            return Err(self.unsupported(span, "this method of an `Option` of a generic type"));
        }
        let mut values = self.operands(args, out)?;
        if let Comb::Then | Comb::ThenSome = comb {
            let some = generic_args.type_at(0);
            if self.can_be_nullish(some) {
                let what = format!("`then` to a `{some}`, whose `Some` would be `None` in JS");
                return Err(self.unsupported(span, &what));
            }
            let (test, value) = (values.remove(0), values.remove(0));
            let value = if comb == Comb::Then {
                self.call_with(value, Vec::new(), "value", out)
            } else if value.has_effects() {
                // Rust works it out either way.
                self.spill("value", value, out)
            } else {
                value
            };
            return Ok(Expr::cond(test, value, Expr::undefined()));
        }
        // `map_err(|e| e.to_string())` of a parse error, whose message is
        // already a string: the same `Result`. Only one just made, so no
        // other variable is left sharing it.
        if matches!(comb, Comb::ResultMap | Comb::MapErr)
            && values[1].is_identity()
            && matches!(self.thir[self.strip(args[0])].kind, ExprKind::Call { .. })
        {
            return Ok(values.remove(0));
        }
        let subject = values.remove(0);
        // The subject is read more than once.
        let subject = if subject.reads_same() {
            subject
        } else {
            let base = if self.option_of(subject_ty).is_some() {
                "option"
            } else {
                "result"
            };
            self.spill(base, subject, out)
        };
        let mut rest = values.into_iter();
        let mut next = || rest.next().expect("rustc checked the arguments");
        let some = Expr::bin(Op::LooseNe, subject.clone(), Expr::null());
        let none = || Expr::bin(Op::LooseEq, subject.clone(), Expr::null());
        let tag = |t: &str| Expr::bin(Op::Eq, Expr::member(subject.clone(), "TAG"), Expr::str(t));
        let inside = || Expr::member(subject.clone(), "_0");
        // A value Rust computes either way, that JS would only compute when
        // it's needed: in a `const` first if it has effects.
        let eager = |this: &mut Self, value: Expr, out: &mut Vec<Stmt>| {
            if value.has_effects() {
                this.spill("fallback", value, out)
            } else {
                value
            }
        };
        let helper = |this: &mut Self, helper: Helper, name: &str, list: Vec<Expr>| {
            this.runtime.insert(helper);
            Expr::call(Expr::var(name), list)
        };
        Ok(match comb {
            Comb::Then | Comb::ThenSome => unreachable!("handled above"),
            Comb::UnwrapOrElse => {
                let f = next();
                let fallback = self.call_with(f, Vec::new(), "fallback", out);
                Expr::bin(Op::Coalesce, subject, fallback)
            }
            Comb::UnwrapOrDefault => {
                let inner = self.option_of(subject_ty).expect("an `Option`");
                let fallback = self.default_value(inner, span)?;
                Expr::bin(Op::Coalesce, subject, fallback)
            }
            Comb::MapOr => {
                let fallback = next();
                let fallback = eager(self, fallback, out);
                let f = next();
                let mapped = self.call_with(f, vec![subject.clone()], "map", out);
                Expr::cond(some, mapped, fallback)
            }
            Comb::MapOrElse => {
                let (g, f) = (next(), next());
                let mapped = self.call_with(f, vec![subject.clone()], "map", out);
                let fallback = self.call_with(g, Vec::new(), "fallback", out);
                Expr::cond(some, mapped, fallback)
            }
            Comb::AndThen => {
                let f = next();
                let then = self.call_with(f, vec![subject.clone()], "then", out);
                Expr::cond(some, then, Expr::undefined())
            }
            Comb::Filter => {
                let p = next();
                let keep = self.call_with(p, vec![subject.clone()], "keep", out);
                Expr::cond(Expr::bin(Op::And, some, keep), subject, Expr::undefined())
            }
            Comb::OkOr => {
                let e = next();
                let e = eager(self, e, out);
                Expr::cond(some, Self::ok(subject), Self::err(e))
            }
            Comb::OkOrElse => {
                let f = next();
                let e = self.call_with(f, Vec::new(), "error", out);
                Expr::cond(some, Self::ok(subject), Self::err(e))
            }
            Comb::Or => {
                let other = next();
                let other = eager(self, other, out);
                Expr::bin(Op::Coalesce, subject, other)
            }
            Comb::OrElse => {
                let f = next();
                let other = self.call_with(f, Vec::new(), "fallback", out);
                Expr::bin(Op::Coalesce, subject, other)
            }
            Comb::IsSomeAnd => {
                let p = next();
                let holds = self.call_with(p, vec![subject.clone()], "holds", out);
                Expr::bin(Op::And, some, holds)
            }
            Comb::IsNoneOr => {
                let p = next();
                let holds = self.call_with(p, vec![subject.clone()], "holds", out);
                Expr::bin(Op::Or, none(), holds)
            }
            Comb::ResultMap => {
                let f = next();
                let mapped = self.call_with(f, vec![inside()], "map", out);
                Expr::cond(tag("Ok"), Self::ok(mapped), subject)
            }
            Comb::MapErr => {
                let f = next();
                let mapped = self.call_with(f, vec![inside()], "map", out);
                Expr::cond(tag("Err"), Self::err(mapped), subject)
            }
            Comb::ResultAndThen => {
                let f = next();
                let then = self.call_with(f, vec![inside()], "then", out);
                Expr::cond(tag("Ok"), then, subject)
            }
            Comb::ResultUnwrapOrElse => {
                let f = next();
                let fallback = self.call_with(f, vec![inside()], "fallback", out);
                Expr::cond(tag("Ok"), inside(), fallback)
            }
            Comb::ResultUnwrapOrDefault => {
                let ok_ty = generic_args.type_at(0);
                let fallback = self.default_value(ok_ty, span)?;
                Expr::cond(tag("Ok"), inside(), fallback)
            }
            Comb::Err => Expr::cond(tag("Err"), inside(), Expr::undefined()),
            Comb::IsOkAnd | Comb::IsErrAnd => {
                let p = next();
                let holds = self.call_with(p, vec![inside()], "holds", out);
                let which = if comb == Comb::IsOkAnd { "Ok" } else { "Err" };
                Expr::bin(Op::And, tag(which), holds)
            }
            // `v.contains(&x)`: JS's `includes` for what `===` compares, and
            // `==` item by item for the rest (ADR 0053).
            Comb::BinarySearch => {
                let x = next();
                let item = self
                    .slice_item(subject_ty)
                    .ok_or_else(|| self.unsupported(span, "`binary_search` of this"))?;
                // What `<` orders as `Ord` does: integers, `char`s, strings.
                let ordered = (Num::of(item).is_some_and(|n| n != Num::F64))
                    || item.is_char()
                    || item.is_bool()
                    || self.is_string_like(item);
                if !ordered {
                    return Err(self.unsupported(span, &format!("`binary_search` of `{item}`s")));
                }
                helper(self, Helper::BinarySearch, "$binarySearch", vec![subject, x])
            }
            Comb::Contains => {
                let x = next();
                let item = self
                    .slice_item(subject_ty)
                    .ok_or_else(|| self.unsupported(span, "`contains` of this"))?;
                if self.eq_is_identity(item) {
                    Expr::call(Expr::member(subject, "includes"), vec![x])
                } else {
                    let x = if x.reads_same() { x } else { self.spill("item", x, out) };
                    let mut body = Vec::new();
                    let same = self.eq_value(Expr::var("each"), x, item, span, &mut body)?;
                    body.push(StmtKind::Return(Some(same)).at(js::Span::NONE));
                    let f = Expr::arrow(vec!["each".into()], body);
                    Expr::call(Expr::member(subject, "some"), vec![f])
                }
            }
            Comb::Extend => {
                let items = next();
                let spread = Expr::call(Expr::member(Expr::var("Array"), "from"), vec![items]);
                helper(self, Helper::Extend, "$extend", vec![subject, spread])
            }
            Comb::Insert => helper(self, Helper::InsertAt, "$insertAt", vec![subject, next(), next()]),
            Comb::Remove => helper(self, Helper::RemoveAt, "$removeAt", vec![subject, next()]),
            Comb::Swap => helper(self, Helper::Swap, "$swap", vec![subject, next(), next()]),
            Comb::Truncate => helper(self, Helper::Truncate, "$truncate", vec![subject, next()]),
            Comb::Dedup => {
                let item = self
                    .slice_item(subject_ty)
                    .ok_or_else(|| self.unsupported(span, "`dedup` of this"))?;
                if !self.eq_is_identity(item) {
                    return Err(self.unsupported(span, "`dedup` of what `===` doesn't compare"));
                }
                helper(self, Helper::Dedup, "$dedup", vec![subject])
            }
            Comb::Windows => helper(self, Helper::Windows, "$windows", vec![subject, next()]),
            Comb::Chunks => helper(self, Helper::Chunks, "$chunks", vec![subject, next()]),
            Comb::Concat => Expr::call(Expr::member(subject, "flat"), Vec::new()),
        })
    }

    /// What a slice, an array or a `Vec` holds.
    fn slice_item(&self, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
        let ty = ty.peel_refs();
        match ty.kind() {
            ty::Slice(item) | ty::Array(item, _) => Some(*item),
            ty::Adt(_, args) if self.is_std_adt(ty, rustc_span::sym::Vec) => Some(args.type_at(0)),
            _ => None,
        }
    }

    /// Is `==` on `ty` JS's `===`?
    fn eq_is_identity(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        self.is_string_like(ty)
            || Num::of(ty).is_some_and(|n| n != Num::F64)
            || ty.is_bool()
            || matches!(ty.kind(), ty::Adt(adt, _) if super::is_fieldless_enum(*adt))
    }

    /// One of `IterComb`'s, on `items`: an array, or a JS iterator if `lazy`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn iter_comb(
        &mut self,
        comb: IterComb,
        items: Expr,
        mut rest: std::vec::IntoIter<Expr>,
        generic_args: ty::GenericArgsRef<'tcx>,
        receiver_ty: Ty<'tcx>,
        lazy: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let mut next = || rest.next().expect("rustc checked the arguments");
        let method = |items: Expr, name: &str, list: Vec<Expr>| Expr::call(Expr::member(items, name), list);
        let present = || {
            Expr::arrow(
                vec!["item".into()],
                vec![
                    StmtKind::Return(Some(Expr::bin(Op::LooseNe, Expr::var("item"), Expr::null()))).at(js::Span::NONE),
                ],
            )
        };
        let eager_only = |this: &Self, what: &str| {
            if lazy {
                Err(this.unsupported(span, &format!("`{what}` of an iterator of the crate's own")))
            } else {
                Ok(())
            }
        };
        let item_ty = || self.iterator_item(receiver_ty);
        Ok(match comb {
            // `Some`s only: `.map(f).filter((item) => item != null)`.
            IterComb::FilterMap => method(method(items, "map", vec![next()]), "filter", vec![present()]),
            IterComb::FindMap => method(method(items, "map", vec![next()]), "find", vec![present()]),
            IterComb::FlatMap => {
                // A closure that returns an `Option` is a `filter_map`.
                let returned = generic_args.types().nth(1);
                if returned.is_some_and(|t| self.option_of(t).is_some()) {
                    method(method(items, "map", vec![next()]), "filter", vec![present()])
                } else {
                    method(items, "flatMap", vec![next()])
                }
            }
            IterComb::Flatten => match item_ty() {
                Some(item) if self.option_of(item).is_some() => method(items, "filter", vec![present()]),
                _ if lazy => {
                    let each = Expr::arrow(
                        vec!["item".into()],
                        vec![StmtKind::Return(Some(Expr::var("item"))).at(js::Span::NONE)],
                    );
                    method(items, "flatMap", vec![each])
                }
                _ => method(items, "flat", Vec::new()),
            },
            IterComb::Zip => {
                eager_only(self, "zip")?;
                self.runtime.insert(Helper::Zip);
                Expr::call(Expr::var("$zip"), vec![items, next()])
            }
            IterComb::Chain => {
                eager_only(self, "chain")?;
                method(items, "concat", vec![next()])
            }
            IterComb::TakeWhile => {
                eager_only(self, "take_while")?;
                self.runtime.insert(Helper::TakeWhile);
                Expr::call(Expr::var("$takeWhile"), vec![items, next()])
            }
            IterComb::SkipWhile => {
                eager_only(self, "skip_while")?;
                self.runtime.insert(Helper::SkipWhile);
                Expr::call(Expr::var("$skipWhile"), vec![items, next()])
            }
            IterComb::StepBy => {
                let n = next();
                let n = if n.reads_same() { n } else { self.spill("step", n, out) };
                let keep = Expr::bin(Op::Eq, Expr::bin(Op::Rem, Expr::var("i"), n), Expr::int(0));
                let f = Expr::arrow(
                    vec!["_".into(), "i".into()],
                    vec![StmtKind::Return(Some(keep)).at(js::Span::NONE)],
                );
                method(items, "filter", vec![f])
            }
            IterComb::MaxByKey(max) | IterComb::MaxBy(max) => {
                let items = if lazy {
                    method(items, "toArray", Vec::new())
                } else {
                    items
                };
                let f = next();
                let compare = match comb {
                    IterComb::MaxBy(_) => f,
                    _ => {
                        let key_ty = generic_args
                            .types()
                            .nth(1)
                            .ok_or_else(|| self.unsupported(span, "this key"))?;
                        let key = if matches!(f.kind, js::ExprKind::Var(_)) {
                            f
                        } else {
                            self.spill("key", f, out)
                        };
                        let mut body = Vec::new();
                        let order = self.cmp_value(
                            Expr::call(key.clone(), vec![Expr::var("a")]),
                            Expr::call(key, vec![Expr::var("b")]),
                            key_ty,
                            false,
                            span,
                            &mut body,
                        )?;
                        body.push(StmtKind::Return(Some(order)).at(js::Span::NONE));
                        Expr::arrow(vec!["a".into(), "b".into()], body)
                    }
                };
                self.runtime.insert(if max { Helper::MaxBy } else { Helper::MinBy });
                Expr::call(Expr::var(if max { "$maxBy" } else { "$minBy" }), vec![items, compare])
            }
            IterComb::Product => {
                let ty = generic_args
                    .types()
                    .nth(1)
                    .ok_or_else(|| self.unsupported(span, "this product"))?;
                let num = self.num(ty, span)?;
                let times = match num {
                    Num::F64 => Expr::bin(Op::Mul, Expr::var("a"), Expr::var("b")),
                    Num::I32 | Num::U32 => num.wrap(Expr::call(
                        Expr::member(Expr::var("Math"), "imul"),
                        vec![Expr::var("a"), Expr::var("b")],
                    )),
                    _ => num.wrap(Expr::bin(Op::Mul, Expr::var("a"), Expr::var("b"))),
                };
                let f = Expr::arrow(
                    vec!["a".into(), "b".into()],
                    vec![StmtKind::Return(Some(times)).at(js::Span::NONE)],
                );
                method(items, "reduce", vec![f, Expr::int(1)])
            }
            IterComb::Nth => {
                let n = next();
                if lazy {
                    let first = method(method(items, "drop", vec![n]), "take", vec![Expr::int(1)]);
                    Expr::index(method(first, "toArray", Vec::new()), Expr::int(0))
                } else {
                    Expr::index(items, n)
                }
            }
            IterComb::Partition => {
                let items = if lazy {
                    method(items, "toArray", Vec::new())
                } else {
                    items
                };
                self.runtime.insert(Helper::Partition);
                Expr::call(Expr::var("$partition"), vec![items, next()])
            }
        })
    }
}

/// Which `Comb` a method of an `Option`, `Result` or `Vec` is.
pub(super) fn classify(name: &str, option: bool, result: bool, vec: bool, slice: bool) -> Option<Comb> {
    Some(match name {
        "unwrap_or_else" if option => Comb::UnwrapOrElse,
        "unwrap_or_default" if option => Comb::UnwrapOrDefault,
        "map_or" if option => Comb::MapOr,
        "map_or_else" if option => Comb::MapOrElse,
        "and_then" if option => Comb::AndThen,
        "filter" if option => Comb::Filter,
        "ok_or" if option => Comb::OkOr,
        "ok_or_else" if option => Comb::OkOrElse,
        "or" if option => Comb::Or,
        "or_else" if option => Comb::OrElse,
        "is_some_and" if option => Comb::IsSomeAnd,
        "is_none_or" if option => Comb::IsNoneOr,
        "map" if result => Comb::ResultMap,
        "map_err" if result => Comb::MapErr,
        "and_then" if result => Comb::ResultAndThen,
        "unwrap_or_else" if result => Comb::ResultUnwrapOrElse,
        "unwrap_or_default" if result => Comb::ResultUnwrapOrDefault,
        "err" if result => Comb::Err,
        "is_ok_and" if result => Comb::IsOkAnd,
        "is_err_and" if result => Comb::IsErrAnd,
        "contains" if slice => Comb::Contains,
        "binary_search" if slice => Comb::BinarySearch,
        "insert" if vec => Comb::Insert,
        "remove" if vec => Comb::Remove,
        "swap" if slice => Comb::Swap,
        "truncate" if vec => Comb::Truncate,
        "dedup" if vec => Comb::Dedup,
        "windows" if slice => Comb::Windows,
        "chunks" if slice => Comb::Chunks,
        "concat" if slice => Comb::Concat,
        _ => return None,
    })
}

/// Which `Comb` a method of a `bool` is.
pub(super) fn classify_bool(name: &str, boolean: bool) -> Option<Comb> {
    match name {
        "then" if boolean => Some(Comb::Then),
        "then_some" if boolean => Some(Comb::ThenSome),
        _ => None,
    }
}

/// Which `IterComb` an `Iterator` method is.
pub(super) fn classify_iter(name: &str) -> Option<IterComb> {
    Some(match name {
        "filter_map" => IterComb::FilterMap,
        "flat_map" => IterComb::FlatMap,
        "flatten" => IterComb::Flatten,
        "zip" => IterComb::Zip,
        "chain" => IterComb::Chain,
        "take_while" => IterComb::TakeWhile,
        "skip_while" => IterComb::SkipWhile,
        "step_by" => IterComb::StepBy,
        "max_by_key" => IterComb::MaxByKey(true),
        "min_by_key" => IterComb::MaxByKey(false),
        "max_by" => IterComb::MaxBy(true),
        "min_by" => IterComb::MaxBy(false),
        "product" => IterComb::Product,
        "nth" => IterComb::Nth,
        "find_map" => IterComb::FindMap,
        "partition" => IterComb::Partition,
        _ => return None,
    })
}

/// `core::iter::Extend`, which has no diagnostic item.
pub(super) fn is_extend(tcx: rustc_middle::ty::TyCtxt<'_>, trait_id: rustc_span::def_id::DefId) -> bool {
    tcx.crate_name(trait_id.krate) == rustc_span::sym::core && tcx.item_name(trait_id) == Symbol::intern("Extend")
}
