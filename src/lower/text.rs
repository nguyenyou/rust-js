//! More of `char`'s and `str`'s methods, `parse`, and slicing by a range
//! (ADR 0063). A `char` is a one-character string (ADR 0034), so its
//! questions are regular expressions of the Unicode properties Rust uses:
//! `c.is_whitespace()` is `/^\p{White_Space}$/u.test(c)`.

use super::ranges::RangeKind;
use super::representation::Num;
use super::{FnCx, R};
use crate::js::{Expr, Op, Prop, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_middle::thir::{ExprId, ExprKind};
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum TextOp {
    /// A `char` question, as a regular expression it matches.
    Is(&'static str),
    IsAscii,
    ToAsciiUpper,
    ToAsciiLower,
    ToDigit,
    IsDigit,
    SplitWhitespace,
    /// `c.to_uppercase()`: its `char`s, as JS's own mapping gives them (`ß` is `SS`).
    CharCase(bool),
    Lines,
    /// `s.as_bytes()`: its UTF-8 bytes, a copy, as nothing writes through it (ADR 0126).
    Bytes,
    /// `s.split(|c| ..)` and `s.contains(|c| ..)`: a closure as the pattern.
    SplitBy,
    ContainsBy,
    Parse,
    /// `&v[a..b]` of a slice, an array or a `Vec`.
    Slice,
    /// `&s[a..b]` of a string: by its UTF-8 bytes (ADR 0138).
    StrSlice,
    /// `s.len()`: its UTF-8 bytes.
    ByteLen,
    /// `s.find(p)`, or `s.rfind(p)` if `true`: where, in UTF-8 bytes.
    Find(bool),
    /// `s.char_indices()`: each `char`, with where it starts in UTF-8 bytes.
    CharIndices,
    /// `v.drain(a..b)`: those items, taken out of `v`.
    Drain,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    pub(super) fn text_call(
        &mut self,
        op: TextOp,
        args: &[ExprId],
        generic_args: ty::GenericArgsRef<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        if matches!(op, TextOp::Slice | TextOp::StrSlice | TextOp::Drain) {
            return self.slice_range(op, args, span, out);
        }
        let mut values = self.operands(args, out)?.into_iter();
        let mut arg = || values.next().expect("rustc checked the arguments");
        let method = |object: Expr, name: &str, list: Vec<Expr>| Expr::call(Expr::member(object, name), list);
        Ok(match op {
            TextOp::Is(regex) => method(Expr::regex(regex), "test", vec![arg()]),
            TextOp::IsAscii => {
                let code = method(arg(), "charCodeAt", vec![Expr::int(0)]);
                Expr::bin(Op::Lt, code, Expr::int(128))
            }
            TextOp::ToAsciiUpper | TextOp::ToAsciiLower => {
                let (letters, case) = match op {
                    TextOp::ToAsciiUpper => ("/[a-z]/", "toUpperCase"),
                    _ => ("/[A-Z]/", "toLowerCase"),
                };
                let letter = Expr::arrow(
                    vec!["letter".into()],
                    vec![
                        StmtKind::Return(Some(method(Expr::var("letter"), case, Vec::new()))).at(crate::js::Span::NONE),
                    ],
                );
                method(arg(), "replace", vec![Expr::regex(letters), letter])
            }
            TextOp::ToDigit | TextOp::IsDigit => {
                self.runtime.insert(Helper::ToDigit);
                let digit = Expr::call(Expr::var("$toDigit"), vec![arg(), arg()]);
                if op == TextOp::IsDigit {
                    Expr::bin(Op::Ne, digit, Expr::undefined())
                } else {
                    digit
                }
            }
            // Rust's whitespace is Unicode's `White_Space`: JS's `\s` less U+FEFF.
            TextOp::SplitWhitespace => {
                let words = method(arg(), "split", vec![Expr::regex("/\\p{White_Space}+/u")]);
                let word = Expr::arrow(
                    vec!["word".into()],
                    vec![
                        StmtKind::Return(Some(Expr::bin(Op::Ne, Expr::var("word"), Expr::str(""))))
                            .at(crate::js::Span::NONE),
                    ],
                );
                method(words, "filter", vec![word])
            }
            TextOp::CharCase(upper) => {
                let mapped = method(arg(), if upper { "toUpperCase" } else { "toLowerCase" }, Vec::new());
                Expr::call(Expr::member(Expr::var("Array"), "from"), vec![mapped])
            }
            TextOp::SplitBy => {
                self.runtime.insert(Helper::SplitBy);
                Expr::call(Expr::var("$splitBy"), vec![arg(), arg()])
            }
            TextOp::ContainsBy => {
                let chars = Expr::call(Expr::member(Expr::var("Array"), "from"), vec![arg()]);
                method(chars, "some", vec![arg()])
            }
            TextOp::Lines => {
                self.runtime.insert(Helper::Lines);
                Expr::call(Expr::var("$lines"), vec![arg()])
            }
            // `Array.from(new TextEncoder().encode(s))`: an array of `u8`s, as a
            // slice of them is.
            TextOp::Bytes => {
                let encoder = Expr::new_(Expr::var("TextEncoder"), Vec::new());
                let encoded = Expr::call(Expr::member(encoder, "encode"), vec![arg()]);
                Expr::call(Expr::member(Expr::var("Array"), "from"), vec![encoded])
            }
            TextOp::Parse => {
                let target = generic_args
                    .types()
                    .next()
                    .ok_or_else(|| self.unsupported(span, "this `parse`"))?;
                self.parse_as(arg(), target, span)?
            }
            TextOp::ByteLen => {
                self.runtime.insert(Helper::ByteLen);
                Expr::call(Expr::var("$byteLen"), vec![arg()])
            }
            TextOp::Find(last) => {
                let (helper, name) = if last {
                    (Helper::Rfind, "$rfind")
                } else {
                    (Helper::Find, "$find")
                };
                self.runtime.insert(helper);
                Expr::call(Expr::var(name), vec![arg(), arg()])
            }
            TextOp::CharIndices => {
                self.runtime.insert(Helper::CharIndices);
                Expr::call(Expr::var("$charIndices"), vec![arg()])
            }
            TextOp::Slice | TextOp::StrSlice | TextOp::Drain => unreachable!("handled above"),
        })
    }

    /// `s.parse::<T>()`: a `Result`, whose `Err` is the error's message,
    /// which is what its `to_string()` gives.
    fn parse_as(&mut self, text: Expr, target: Ty<'tcx>, span: Span) -> R<Expr> {
        if let Some(num) = Num::of(target) {
            if num == Num::F64 {
                self.runtime.insert(Helper::ParseF64);
                return Ok(Expr::call(Expr::var("$parseF64"), vec![text]));
            }
            // The digits' nearest `f32`, which through an `f64` alone can be
            // missed (ADR 0122).
            if num == Num::F32 {
                self.runtime.insert(Helper::ParseF32);
                return Ok(Expr::call(Expr::var("$parseF32"), vec![text]));
            }
            let (lo, hi) = num.range();
            // A 64-bit one is read as a BigInt, exactly (ADR 0086).
            let (helper, name) = if num.big() {
                (Helper::ParseBig, "$parseBig")
            } else {
                (Helper::ParseInt, "$parseInt")
            };
            self.runtime.insert(helper);
            return Ok(Expr::call(
                Expr::var(name),
                vec![text, num.literal(lo), num.literal(hi)],
            ));
        }
        if target.is_bool() {
            self.runtime.insert(Helper::ParseBool);
            return Ok(Expr::call(Expr::var("$parseBool"), vec![text]));
        }
        if target.is_char() {
            self.runtime.insert(Helper::ParseChar);
            return Ok(Expr::call(Expr::var("$parseChar"), vec![text]));
        }
        if self.is_lang_adt(target, LangItem::String) {
            let ok = Expr::object(vec![
                Prop::Field("TAG".into(), Expr::str("Ok")),
                Prop::Field("_0".into(), text),
            ]);
            return Ok(ok);
        }
        Err(self.unsupported(span, &format!("`parse` to a `{target}`")))
    }

    /// `&v[a..b]`, `&v[a..]`, `&v[..b]`, `&v[..]`: a copy, which a shared
    /// slice can be, since nothing changes `v` while it's borrowed. Out of
    /// bounds, it panics, as Rust does.
    fn slice_range(&mut self, op: TextOp, args: &[ExprId], span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let range = self.strip(args[1]);
        let range_ty = self.thir[range].ty;
        let (helper, name) = match op {
            TextOp::Drain => (Helper::Drain, "$drain"),
            TextOp::StrSlice => (Helper::StrSlice, "$strSlice"),
            _ => (Helper::SliceRange, "$slice"),
        };
        let kind = self.range_kind(range_ty);
        let fields: Vec<(usize, ExprId)> = match self.thir[range].kind {
            ExprKind::Adt(ref adt) if kind != Some(RangeKind::ToInclusive) => {
                adt.fields.iter().map(|f| (f.name.as_usize(), f.expr)).collect()
            }
            // A range kept as a value, or `a..=b` or `..=b` (ADR 0129): its bounds.
            _ => {
                let Some(kind) = kind else {
                    return Err(self.unsupported(span, "slicing by this range"));
                };
                let [items, range]: [Expr; 2] = self.operands(&[args[0], args[1]], out)?.try_into().ok().unwrap();
                let parts = self.range_parts(range, kind, out);
                // `..=1` is `..2`.
                let past = |end: &Expr| match end.as_int() {
                    Some(n) => Expr::int(n + 1),
                    None => Expr::bin(Op::Add, end.clone(), Expr::int(1)),
                };
                let (start, end) = match (kind, parts.as_slice()) {
                    (RangeKind::Exclusive, [start, end]) => (start.clone(), Some(end.clone())),
                    (RangeKind::Inclusive, [start, end]) => (start.clone(), Some(past(end))),
                    (RangeKind::From, [start]) => (start.clone(), None),
                    (RangeKind::To, [end]) => (Expr::int(0), Some(end.clone())),
                    (RangeKind::ToInclusive, [end]) => (Expr::int(0), Some(past(end))),
                    _ => (Expr::int(0), None),
                };
                self.runtime.insert(helper);
                let mut list = vec![items, start];
                list.extend(end);
                return Ok(Expr::call(Expr::var(name), list));
            }
        };
        let bound = |i: usize| fields.iter().find(|&&(n, _)| n == i).map(|&(_, e)| e);
        let (start, end) = if self.is_lang_adt(range_ty, LangItem::Range) {
            (bound(0), bound(1))
        } else if self.is_lang_adt(range_ty, LangItem::RangeFrom) {
            (bound(0), None)
        } else if self.is_lang_adt(range_ty, LangItem::RangeTo) {
            (None, bound(0))
        } else if self.is_lang_adt(range_ty, LangItem::RangeFull) {
            (None, None)
        } else {
            return Err(self.unsupported(span, "slicing by this range"));
        };
        // `&s[..]` of a string: all of it, which is never out of bounds, nor
        // inside a character.
        if op == TextOp::StrSlice && start.is_none() && end.is_none() {
            return Ok(self.operands(&[args[0]], out)?.remove(0));
        }
        let mut list = vec![args[0]];
        list.extend(start);
        list.extend(end);
        let mut values = self.operands(&list, out)?.into_iter();
        let items = values.next().expect("the slice");
        let start = match start {
            Some(_) => values.next().expect("a start"),
            None => Expr::int(0),
        };
        let end = end.map(|_| values.next().expect("an end"));
        self.runtime.insert(helper);
        let mut list = vec![items, start];
        list.extend(end);
        Ok(Expr::call(Expr::var(name), list))
    }
}
