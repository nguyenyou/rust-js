//! A `Vec`'s or a slice's method (ADRs 0025, 0036).

use super::calls::Call;
use super::recognition::Std;
use super::{FnCx, R};
use crate::js;
use crate::js::{Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// A `Vec`'s or a slice's method (ADRs 0025, 0036): `None` if `known` is another.
    pub(super) fn vec_call(
        &mut self,
        known: Std,
        call: Call<'_, 'tcx>,
        values: &mut std::vec::IntoIter<Expr>,
        boxed: bool,
        out: &mut Vec<Stmt>,
    ) -> R<Option<Expr>> {
        let Call { args, span, .. } = call;
        let mut arg = || values.next().expect("rustc checked the arguments");
        let js_span = self.js_span(span);
        Ok(Some(match known {
            Std::Method("pop") if boxed => {
                self.runtime.insert(Helper::Pop);
                Expr::call(Expr::var("$pop"), vec![arg()])
            }
            Std::First if boxed => {
                let items = arg();
                self.some_at(items, Expr::int(0))
            }
            Std::SliceLast if boxed => {
                let mut items = arg();
                if items.has_effects() {
                    items = self.spill("items", items, out);
                }
                let last = Expr::bin(Op::Sub, Expr::member(items.clone(), "length"), Expr::int(1));
                self.some_at(items, last)
            }
            Std::SliceGet if boxed => {
                let items = arg();
                self.some_at(items, arg())
            }
            Std::First => Expr::index(arg(), Expr::int(0)),
            Std::SliceGet => {
                let items = arg();
                Expr::index(items, arg())
            }
            Std::SliceLast => Expr::call(Expr::member(arg(), "at"), vec![Expr::int(-1)]),
            // A copy, unless it's an array just written: `vec![3, 4].into()`.
            Std::ToVec => match arg() {
                items if matches!(items.kind, js::ExprKind::Array(_)) => items,
                items => Expr::call(Expr::member(items, "slice"), vec![]),
            },
            Std::SortBy => {
                let (v, compare) = (arg(), arg());
                Expr::call(Expr::member(v, "sort"), vec![compare])
            }
            Std::IsEmpty => Expr::bin(Op::Eq, Expr::member(arg(), "length"), Expr::num(0)),
            Std::VecNew => Expr::array(vec![]),
            Std::Append => {
                self.runtime.insert(Helper::Append);
                Expr::call(Expr::var("$append"), vec![arg(), arg()])
            }
            Std::Push => {
                let (v, x) = (arg(), arg());
                Expr::call(Expr::member(v, "push"), vec![x])
            }
            // `count()` of a JS iterator (ADR 0055) takes all of it.
            Std::Len if self.is_lazy_value(args[0]) => {
                let items = self.iter_source(arg(), self.thir[args[0]].ty, span, out)?;
                Expr::member(Expr::call(Expr::member(items, "toArray"), vec![]), "length")
            }
            // A range's, which is an object, is its items' (ADR 0129).
            Std::Len if self.range_kind(self.thir[args[0]].ty.peel_refs()).is_some() => {
                let items = self.iter_source(arg(), self.thir[args[0]].ty.peel_refs(), span, out)?;
                Expr::member(items, "length")
            }
            Std::Len => Expr::member(arg(), "length"),
            Std::Index => {
                self.runtime.insert(Helper::Index);
                Expr::call(Expr::var("$index"), vec![arg(), arg()])
            }
            Std::Clear => {
                out.push(StmtKind::Assign(Expr::member(arg(), "length"), Expr::num(0)).at(js_span));
                Expr::undefined()
            }
            Std::Retain => {
                self.runtime.insert(Helper::Retain);
                let (v, keep) = (arg(), arg());
                Expr::call(Expr::var("$retain"), vec![v, keep])
            }
            _ => return Ok(None),
        }))
    }
}
