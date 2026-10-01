//! Ranges (ADR 0129): `a..b` is `{ start: a, end: b }`, as any struct is,
//! and so is `a..=b`; `a..` is `{ start: a }`. Iterated, a range is its
//! items: an array, or for `a..`, which never ends, a JS iterator.

use super::display::join;
use super::representation::Num;
use super::{FnCx, R};
use crate::js::{self, Expr, Op, Prop, Stmt, StmtKind, UnaryOp};
use crate::runtime::Helper;
use rustc_middle::thir::ExprId;
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;

/// Which of std's ranges a type is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum RangeKind {
    /// `a..b`
    Exclusive,
    /// `a..=b`
    Inclusive,
    /// `a..`
    From,
    /// `..b`
    To,
    /// `..=b`
    ToInclusive,
    /// `..`
    Full,
}

impl RangeKind {
    /// Its bounds, by the names of its fields.
    pub(super) fn bounds(self) -> &'static [&'static str] {
        match self {
            RangeKind::Exclusive | RangeKind::Inclusive => &["start", "end"],
            RangeKind::From => &["start"],
            RangeKind::To | RangeKind::ToInclusive => &["end"],
            RangeKind::Full => &[],
        }
    }

    /// What `{:?}` shows between its bounds.
    fn dots(self) -> &'static str {
        match self {
            RangeKind::Inclusive | RangeKind::ToInclusive => "..=",
            _ => "..",
        }
    }
}

/// A range method rust-js knows.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum RangeOp {
    /// `RangeInclusive::new(a, b)`, which `a..=b` is: `{ start: a, end: b }`.
    New,
    /// `r.contains(&x)`: `r.start <= x && x < r.end`.
    Contains,
    /// A `RangeInclusive`'s `start()` and `end()`: its field.
    Bound(&'static str),
    /// A `RangeInclusive`'s `into_inner()`: `[r.start, r.end]`.
    IntoInner,
    /// `is_empty()`: `!(r.start < r.end)`.
    IsEmpty,
    /// `len()`: `Math.max(0, r.end - r.start)`.
    Len,
    /// A `Range`'s `next()` and `next_back()`, and an `a..`'s `next()`,
    /// which move its `start` or its `end`, as Rust's do.
    Next,
    NextBack,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    pub(super) fn range_kind(&self, ty: Ty<'tcx>) -> Option<RangeKind> {
        self.recognition().range_kind(ty)
    }

    /// What a range's bounds are: the `u32` of a `Range<u32>`.
    pub(super) fn range_index(&self, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
        match ty.kind() {
            ty::Adt(_, args) => args.types().next(),
            _ => None,
        }
    }

    /// The bounds of `{ start: a, end: b }` itself, if each reads the same
    /// wherever it's read.
    pub(super) fn range_literal_parts(&self, range: &Expr, kind: RangeKind) -> Option<Vec<Expr>> {
        let js::ExprKind::Object(props) = &range.kind else {
            return None;
        };
        if props.len() != kind.bounds().len() {
            return None;
        }
        kind.bounds()
            .iter()
            .map(|name| {
                props.iter().find_map(|prop| match prop {
                    Prop::Field(field, value) if field == name && value.reads_same() => Some(value.clone()),
                    _ => None,
                })
            })
            .collect()
    }

    /// A range's bounds, each to be read once or more: from `{ start: a,
    /// end: b }` itself, or from the range, a `const` first if reading it
    /// again would differ.
    pub(super) fn range_parts(&mut self, range: Expr, kind: RangeKind, out: &mut Vec<Stmt>) -> Vec<Expr> {
        if let Some(parts) = self.range_literal_parts(&range, kind) {
            return parts;
        }
        let range = if range.reads_same() {
            range
        } else {
            self.spill("range", range, out)
        };
        kind.bounds()
            .iter()
            .map(|name| Expr::member(range.clone(), *name))
            .collect()
    }

    /// A range's items: `$range(r.start, r.end)`, `$range(a, b + 1)` of
    /// `a..=b`, and `$rangeFrom(a)` of `a..`, a JS iterator that never ends.
    pub(super) fn range_items(&mut self, range: Expr, ty: Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let kind = self.range_kind(ty).expect("a range");
        let index = self.range_index(ty);
        if index.is_some_and(|index| index.is_char()) && matches!(kind, RangeKind::Exclusive | RangeKind::Inclusive) {
            let mut list = self.range_parts(range, kind, out);
            if kind == RangeKind::Inclusive {
                list.push(Expr::bool(true));
            }
            self.runtime.insert(Helper::CharRange);
            return Ok(Expr::call(Expr::var("$charRange"), list));
        }
        let Some(num) = index.and_then(Num::of) else {
            return Err(self.unsupported(span, &format!("iterating over `{ty}`")));
        };
        let parts = self.range_parts(range, kind, out);
        Ok(match (kind, parts.as_slice()) {
            (RangeKind::Exclusive | RangeKind::Inclusive, [start, end]) => {
                // `..=6` is `..7`.
                let end = match end.as_int() {
                    Some(n) if kind == RangeKind::Inclusive && !num.big() => Expr::int(n + 1),
                    _ if kind == RangeKind::Inclusive => Expr::bin(Op::Add, end.clone(), num.literal(1)),
                    _ => end.clone(),
                };
                let (helper, name) = if num.big() {
                    (Helper::BigRange, "$bigRange")
                } else {
                    (Helper::Range, "$range")
                };
                self.runtime.insert(helper);
                Expr::call(Expr::var(name), vec![start.clone(), end])
            }
            (RangeKind::From, [start]) => {
                self.runtime.insert(Helper::RangeFrom);
                Expr::call(Expr::var("$rangeFrom"), vec![start.clone()])
            }
            _ => return Err(self.unsupported(span, &format!("iterating over `{ty}`"))),
        })
    }

    /// `{:?}` of a range: its bounds', around its dots, `1..4` or `..=4`.
    pub(super) fn range_debug(&mut self, range: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let kind = self.range_kind(ty).expect("a range");
        let index = self.range_index(ty);
        let shown = |this: &mut Self, parts: Vec<Expr>| -> R<Expr> {
            let mut parts = parts.into_iter();
            let mut pieces = Vec::new();
            if matches!(kind, RangeKind::Exclusive | RangeKind::Inclusive | RangeKind::From) {
                let start = parts.next().expect("a start");
                pieces.push(this.debug_string(start, index.expect("a bound's type"), span)?);
            }
            pieces.push(Expr::str(kind.dots()));
            if let Some(end) = parts.next() {
                pieces.push(this.debug_string(end, index.expect("a bound's type"), span)?);
            }
            Ok(join(pieces))
        };
        if let Some(parts) = self.range_literal_parts(&range, kind) {
            return shown(self, parts);
        }
        let parts = kind
            .bounds()
            .iter()
            .map(|name| Expr::member(Expr::var("range"), *name))
            .collect();
        let body = shown(self, parts)?;
        let f = Expr::arrow(
            vec!["range".into()],
            vec![StmtKind::Return(Some(body)).at(js::Span::NONE)],
        );
        Ok(self.applied(f, range))
    }

    pub(super) fn range_call(&mut self, op: RangeOp, args: &[ExprId], span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        if op == RangeOp::New {
            let values = self.operands(args, out)?;
            let props = ["start", "end"]
                .into_iter()
                .zip(values)
                .map(|(name, value)| Prop::Field(name.into(), value))
                .collect();
            return Ok(Expr::object(props));
        }
        let range_ty = self.thir[args[0]].ty.peel_refs();
        let kind = self.range_kind(range_ty).expect("a range's method");
        let index = self.range_index(range_ty);
        if op == RangeOp::Contains {
            let item_ty = self.thir[args[1]].ty.peel_refs();
            if index.is_none_or(|index| index.peel_refs() != item_ty || !self.is_primitive_ord(index)) {
                return Err(self.unsupported(span, &format!("`contains` of a `{range_ty}`")));
            }
            let [range, item]: [Expr; 2] = self.operands(args, out)?.try_into().ok().unwrap();
            let parts = self.range_parts(range, kind, out);
            let item = if item.reads_same() {
                item
            } else {
                self.spill("item", item, out)
            };
            // As Rust's: the start's test, then the end's.
            let tests: Vec<Expr> = match (kind, parts.as_slice()) {
                (RangeKind::Exclusive, [start, end]) => vec![
                    Expr::bin(Op::Le, start.clone(), item.clone()),
                    Expr::bin(Op::Lt, item, end.clone()),
                ],
                (RangeKind::Inclusive, [start, end]) => vec![
                    Expr::bin(Op::Le, start.clone(), item.clone()),
                    Expr::bin(Op::Le, item, end.clone()),
                ],
                (RangeKind::From, [start]) => vec![Expr::bin(Op::Le, start.clone(), item)],
                (RangeKind::To, [end]) => vec![Expr::bin(Op::Lt, item, end.clone())],
                (RangeKind::ToInclusive, [end]) => vec![Expr::bin(Op::Le, item, end.clone())],
                _ => vec![Expr::bool(true)],
            };
            return Ok(tests
                .into_iter()
                .reduce(|all, test| Expr::bin(Op::And, all, test))
                .expect("a test"));
        }
        if matches!(op, RangeOp::Next | RangeOp::NextBack) {
            if index.and_then(Num::of).is_none() {
                return Err(self.unsupported(span, &format!("stepping through a `{range_ty}`")));
            }
            let range = self.expr(args[0], out)?;
            let (helper, name) = match (op, kind) {
                (RangeOp::Next, RangeKind::From) => (Helper::RangeFromNext, "$rangeFromNext"),
                (RangeOp::Next, _) => (Helper::RangeNext, "$rangeNext"),
                _ => (Helper::RangeNextBack, "$rangeNextBack"),
            };
            self.runtime.insert(helper);
            return Ok(Expr::call(Expr::var(name), vec![range]));
        }
        let range = self.expr(args[0], out)?;
        let parts = self.range_parts(range, kind, out);
        Ok(match (op, parts.as_slice()) {
            (RangeOp::Bound(name), [start, end]) => {
                if name == "start" {
                    start.clone()
                } else {
                    end.clone()
                }
            }
            (RangeOp::IntoInner, [start, end]) => Expr::array(vec![start.clone(), end.clone()]),
            (RangeOp::IsEmpty, [start, end]) => {
                let op = if kind == RangeKind::Inclusive { Op::Le } else { Op::Lt };
                Expr::unary(UnaryOp::Not, Expr::bin(op, start.clone(), end.clone()))
            }
            (RangeOp::Len, [start, end]) => {
                let mut count = Expr::bin(Op::Sub, end.clone(), start.clone());
                if kind == RangeKind::Inclusive {
                    count = Expr::bin(Op::Add, count, Expr::int(1));
                }
                Expr::call(Expr::member(Expr::var("Math"), "max"), vec![Expr::int(0), count])
            }
            _ => return Err(self.unsupported(span, &format!("this method of a `{range_ty}`"))),
        })
    }
}
