//! More of `char`'s and `str`'s methods, `parse`, and slicing by a range
//! (ADR 0063). A `char` is a one-character string (ADR 0034), so its
//! questions are regular expressions of the Unicode properties Rust uses:
//! `c.is_whitespace()` is `/^\p{White_Space}$/u.test(c)`.

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
    Lines,
    /// `s.split(|c| ..)` and `s.contains(|c| ..)`: a closure as the pattern.
    SplitBy,
    ContainsBy,
    Parse,
    /// `&v[a..b]` of a slice, an array or a `Vec`.
    Slice,
}

/// Which `TextOp` a method of a `char` or a `str` is.
pub(super) fn classify(name: &str, char: bool, str: bool) -> Option<TextOp> {
    Some(match name {
        "is_whitespace" if char => TextOp::Is("/^\\p{White_Space}$/u"),
        "is_alphabetic" if char => TextOp::Is("/^\\p{Alphabetic}$/u"),
        "is_numeric" if char => TextOp::Is("/^\\p{N}$/u"),
        "is_alphanumeric" if char => TextOp::Is("/^[\\p{Alphabetic}\\p{N}]$/u"),
        "is_uppercase" if char => TextOp::Is("/^\\p{Uppercase}$/u"),
        "is_lowercase" if char => TextOp::Is("/^\\p{Lowercase}$/u"),
        "is_control" if char => TextOp::Is("/^\\p{Cc}$/u"),
        "is_ascii_digit" if char => TextOp::Is("/^[0-9]$/"),
        "is_ascii_hexdigit" if char => TextOp::Is("/^[0-9A-Fa-f]$/"),
        "is_ascii_alphabetic" if char => TextOp::Is("/^[A-Za-z]$/"),
        "is_ascii_alphanumeric" if char => TextOp::Is("/^[A-Za-z0-9]$/"),
        "is_ascii_uppercase" if char => TextOp::Is("/^[A-Z]$/"),
        "is_ascii_lowercase" if char => TextOp::Is("/^[a-z]$/"),
        "is_ascii_whitespace" if char => TextOp::Is("/^[ \\t\\n\\f\\r]$/"),
        "is_ascii_punctuation" if char => TextOp::Is("/^[!-\\/:-@[-`{-~]$/"),
        "is_ascii_graphic" if char => TextOp::Is("/^[!-~]$/"),
        "is_ascii_control" if char => TextOp::Is("/^[\\0-\\x1f\\x7f]$/"),
        "is_ascii" if char => TextOp::IsAscii,
        "to_ascii_uppercase" if char => TextOp::ToAsciiUpper,
        "to_ascii_lowercase" if char => TextOp::ToAsciiLower,
        "to_digit" if char => TextOp::ToDigit,
        "is_digit" if char => TextOp::IsDigit,
        "split_whitespace" if str => TextOp::SplitWhitespace,
        "lines" if str => TextOp::Lines,
        "parse" if str => TextOp::Parse,
        _ => return None,
    })
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
        if op == TextOp::Slice {
            return self.slice_range(args, span, out);
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
            TextOp::Parse => {
                let target = generic_args
                    .types()
                    .next()
                    .ok_or_else(|| self.unsupported(span, "this `parse`"))?;
                self.parse_as(arg(), target, span)?
            }
            TextOp::Slice => unreachable!("handled above"),
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
            let (lo, hi) = num.range();
            self.runtime.insert(Helper::ParseInt);
            return Ok(Expr::call(
                Expr::var("$parseInt"),
                vec![text, Expr::int(lo), Expr::int(hi)],
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
    fn slice_range(&mut self, args: &[ExprId], span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let range = self.strip(args[1]);
        let range_ty = self.thir[range].ty;
        let fields: Vec<(usize, ExprId)> = match self.thir[range].kind {
            ExprKind::Adt(ref adt) => adt.fields.iter().map(|f| (f.name.as_usize(), f.expr)).collect(),
            _ => return Err(self.unsupported(span, "slicing by a range in a variable")),
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
        self.runtime.insert(Helper::SliceRange);
        let mut list = vec![items, start];
        list.extend(end);
        Ok(Expr::call(Expr::var("$slice"), list))
    }
}
