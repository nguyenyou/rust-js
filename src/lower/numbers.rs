//! Arithmetic, casts, and methods of integers and `f64` (ADR 0064).
//! A small integer or `f64` is a JS number; `i64`/`u64` use BigInt (ADR 0086).
//! Many number methods map to `Math`: `x.sqrt()` is `Math.sqrt(x)`. Where
//! Rust's answer differs from JS's (`round` of a half, `pow` past 2^53),
//! a helper gives Rust's.

use super::discriminants;
use super::representation::{Num, is_fieldless_enum};
use super::{FnCx, R};
use crate::js::{self, Expr, Op, Prop, Stmt, UnaryOp};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_middle::mir::{AssignOp, BinOp, UnOp};
use rustc_middle::thir::ExprId;
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum NumOp {
    /// The same in JS: `Math.floor(x)`, `Math.atan2(y, x)`.
    Math(&'static str),
    Abs,
    UnsignedAbs,
    Pow,
    CheckedPow,
    Powi,
    Powf,
    Round,
    /// `a.total_cmp(&b)`: IEEE 754's total order, as Rust has it.
    TotalCmp,
    IsNan,
    IsFinite,
    IsInfinite,
    Checked(BinOp),
    Saturating(BinOp),
    Wrapping(BinOp),
    RemEuclid,
    DivEuclid,
    Signum,
    LeadingZeros,
    TrailingZeros,
    CountOnes,
    IsPowerOfTwo,
    AbsDiff,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    pub(super) fn number_call(
        &mut self,
        op: NumOp,
        args: &[ExprId],
        ty: Ty<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let num = Num::of(ty).expect("a number's method");
        if num.big() {
            return self.big_number_call(op, args, ty, num, span, out);
        }
        let (lo, hi) = num.range();
        let mut values = self.operands(args, out)?.into_iter();
        let mut arg = || values.next().expect("rustc checked the arguments");
        let math = |name: &str, list: Vec<Expr>| Expr::call(Expr::member(Expr::var("Math"), name), list);
        let number = |name: &str, list: Vec<Expr>| Expr::call(Expr::member(Expr::var("Number"), name), list);
        // A narrow signed integer's bits, for the questions about them: -1i8 is 0xff.
        let bits = |x: Expr| match num {
            Num::I8 => Expr::bin(Op::BitAnd, x, Expr::int(0xff)),
            Num::I16 => Expr::bin(Op::BitAnd, x, Expr::int(0xffff)),
            _ => x,
        };
        let helper = |this: &mut Self, helper: Helper, name: &str, list: Vec<Expr>| {
            this.runtime.insert(helper);
            Expr::call(Expr::var(name), list)
        };
        Ok(match op {
            NumOp::Math(name) => {
                let list = values.collect();
                math(name, list)
            }
            NumOp::Abs => num.wrap(math("abs", vec![arg()])),
            // An `i32::MIN`'s is 2^31, which a `u32` holds.
            NumOp::UnsignedAbs => math("abs", vec![arg()]),
            NumOp::CheckedPow => helper(
                self,
                Helper::CheckedPow,
                "$checkedPow",
                vec![arg(), arg(), Expr::int(lo), Expr::int(hi)],
            ),
            // Exact below 2^53, and `$pow` multiplies as `Math.imul` does, so
            // what's past it wraps as Rust's does.
            NumOp::Pow => {
                let power = helper(self, Helper::Pow, "$pow", vec![arg(), arg()]);
                if num == Num::I32 { power } else { num.wrap(power) }
            }
            NumOp::Powi => helper(self, Helper::Powi, "$powi", vec![arg(), arg()]),
            NumOp::Powf => Expr::bin(Op::Pow, arg(), arg()),
            NumOp::Round => helper(self, Helper::Round, "$round", vec![arg()]),
            NumOp::TotalCmp => helper(self, Helper::TotalCmp, "$totalCmp", vec![arg(), arg()]),
            NumOp::IsNan => number("isNaN", vec![arg()]),
            NumOp::IsFinite => number("isFinite", vec![arg()]),
            NumOp::IsInfinite => Expr::bin(Op::Eq, math("abs", vec![arg()]), Expr::var("Infinity")),
            // The exact result, if it's in range. A product past 2^53 is
            // rounded, but it's far out of range either way.
            NumOp::Checked(BinOp::Div) => {
                let min = Expr::int(lo);
                helper(self, Helper::CheckedDiv, "$checkedDiv", vec![arg(), arg(), min])
            }
            NumOp::Checked(op) => {
                let exact = Expr::bin(js_op(op), arg(), arg());
                helper(
                    self,
                    Helper::Checked,
                    "$checked",
                    vec![exact, Expr::int(lo), Expr::int(hi)],
                )
            }
            NumOp::Saturating(op) => {
                let exact = Expr::bin(js_op(op), arg(), arg());
                match (num.signed(), op) {
                    (false, BinOp::Sub) => math("max", vec![exact, Expr::int(0)]),
                    (false, _) => math("min", vec![exact, Expr::int(hi)]),
                    // `| 0`, exact in range, makes the -0 of `0 * -5` the integer 0.
                    (true, BinOp::Mul) => Expr::bin(
                        Op::BitOr,
                        math("min", vec![math("max", vec![exact, Expr::int(lo)]), Expr::int(hi)]),
                        Expr::int(0),
                    ),
                    _ => math("min", vec![math("max", vec![exact, Expr::int(lo)]), Expr::int(hi)]),
                }
            }
            NumOp::Wrapping(op) => {
                let (a, b) = (arg(), arg());
                self.binary(op, a, b, None, ty, span)?
            }
            NumOp::RemEuclid if !num.signed() => {
                let (a, b) = (arg(), arg());
                self.binary(BinOp::Rem, a, b, None, ty, span)?
            }
            NumOp::DivEuclid if !num.signed() => {
                let (a, b) = (arg(), arg());
                self.binary(BinOp::Div, a, b, None, ty, span)?
            }
            NumOp::RemEuclid => {
                self.runtime.insert(Helper::Rem);
                helper(self, Helper::RemEuclid, "$remEuclid", vec![arg(), arg(), Expr::int(lo)])
            }
            NumOp::DivEuclid => {
                self.runtime.insert(Helper::Div);
                helper(self, Helper::DivEuclid, "$divEuclid", vec![arg(), arg(), Expr::int(lo)])
            }
            NumOp::Signum => math("sign", vec![arg()]),
            NumOp::LeadingZeros => {
                let zeros = math("clz32", vec![bits(arg())]);
                match num.bits() {
                    32 => zeros,
                    n => Expr::bin(Op::Sub, zeros, Expr::int((32 - n).into())),
                }
            }
            NumOp::TrailingZeros => {
                let x = bits(arg());
                helper(
                    self,
                    Helper::TrailingZeros,
                    "$trailingZeros",
                    vec![x, Expr::int(num.bits().into())],
                )
            }
            NumOp::CountOnes => {
                let x = bits(arg());
                helper(self, Helper::CountOnes, "$countOnes", vec![x])
            }
            NumOp::IsPowerOfTwo => {
                let x = arg();
                let x = if x.reads_same() { x } else { self.spill("n", x, out) };
                let lower = Expr::bin(Op::BitAnd, x.clone(), Expr::bin(Op::Sub, x.clone(), Expr::int(1)));
                Expr::bin(
                    Op::And,
                    Expr::bin(Op::Ne, x, Expr::int(0)),
                    Expr::bin(Op::Eq, lower, Expr::int(0)),
                )
            }
            NumOp::AbsDiff => math("abs", vec![Expr::bin(Op::Sub, arg(), arg())]),
        })
    }
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// An `i64`'s or a `u64`'s methods (ADR 0086): a BigInt's, where JS's
    /// `Math` takes only numbers. What they count, `count_ones()` and the
    /// like, is a `u32`, a number.
    fn big_number_call(
        &mut self,
        op: NumOp,
        args: &[ExprId],
        ty: Ty<'tcx>,
        num: Num,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let (lo, hi) = num.range();
        let (lo, hi) = (Expr::bigint(lo), Expr::bigint(hi));
        let mut values = self.operands(args, out)?.into_iter();
        let mut arg = || values.next().expect("rustc checked the arguments");
        let helper = |this: &mut Self, helper: Helper, name: &str, list: Vec<Expr>| {
            this.runtime.insert(helper);
            Expr::call(Expr::var(name), list)
        };
        Ok(match op {
            NumOp::Abs => num.wrap(helper(self, Helper::BigAbs, "$bigAbs", vec![arg()])),
            // An `i64::MIN`'s is 2^63, which a `u64` holds.
            NumOp::UnsignedAbs => helper(self, Helper::BigAbs, "$bigAbs", vec![arg()]),
            NumOp::Pow => num.wrap(helper(self, Helper::BigPow, "$bigPow", vec![arg(), arg()])),
            NumOp::CheckedPow => helper(self, Helper::CheckedPow, "$checkedPow", vec![arg(), arg(), lo, hi]),
            NumOp::Checked(BinOp::Div) => {
                let min = if num.signed() { lo } else { Expr::undefined() };
                helper(self, Helper::BigCheckedDiv, "$bigCheckedDiv", vec![arg(), arg(), min])
            }
            NumOp::Checked(op) => {
                let exact = Expr::bin(js_op(op), arg(), arg());
                helper(self, Helper::BigChecked, "$bigChecked", vec![exact, lo, hi])
            }
            NumOp::Saturating(op) => {
                let exact = Expr::bin(js_op(op), arg(), arg());
                helper(self, Helper::BigClamp, "$bigClamp", vec![exact, lo, hi])
            }
            NumOp::Wrapping(op) => {
                let (a, b) = (arg(), arg());
                self.binary(op, a, b, None, ty, span)?
            }
            NumOp::RemEuclid if !num.signed() => {
                let (a, b) = (arg(), arg());
                self.binary(BinOp::Rem, a, b, None, ty, span)?
            }
            NumOp::DivEuclid if !num.signed() => {
                let (a, b) = (arg(), arg());
                self.binary(BinOp::Div, a, b, None, ty, span)?
            }
            NumOp::RemEuclid => {
                self.runtime.insert(Helper::BigRem);
                helper(self, Helper::BigRemEuclid, "$bigRemEuclid", vec![arg(), arg(), lo])
            }
            NumOp::DivEuclid => {
                self.runtime.insert(Helper::BigDiv);
                helper(self, Helper::BigDivEuclid, "$bigDivEuclid", vec![arg(), arg(), lo])
            }
            NumOp::Signum => helper(self, Helper::BigSignum, "$bigSignum", vec![arg()]),
            NumOp::LeadingZeros => helper(self, Helper::BigBits, "$bigLeadingZeros", vec![arg()]),
            NumOp::TrailingZeros => helper(self, Helper::BigBits, "$bigTrailingZeros", vec![arg()]),
            NumOp::CountOnes => helper(self, Helper::BigBits, "$bigCountOnes", vec![arg()]),
            NumOp::IsPowerOfTwo => {
                let x = arg();
                let x = if x.reads_same() { x } else { self.spill("n", x, out) };
                let lower = Expr::bin(Op::BitAnd, x.clone(), Expr::bin(Op::Sub, x.clone(), Expr::bigint(1)));
                Expr::bin(
                    Op::And,
                    Expr::bin(Op::Ne, x, Expr::bigint(0)),
                    Expr::bin(Op::Eq, lower, Expr::bigint(0)),
                )
            }
            NumOp::AbsDiff => helper(self, Helper::BigAbsDiff, "$bigAbsDiff", vec![arg(), arg()]),
            _ => return Err(self.unsupported(span, "this method of a 64-bit integer")),
        })
    }
}

fn js_op(op: BinOp) -> Op {
    match op {
        BinOp::Add => Op::Add,
        BinOp::Sub => Op::Sub,
        _ => Op::Mul,
    }
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    // ── Operators ───────────────────────────────────────────────────────

    /// `known` is `r`'s value, when rustc knows it and the JS doesn't show
    /// it: a named `const` (ADR 0031).
    pub(super) fn binary(
        &mut self,
        op: BinOp,
        l: Expr,
        r: Expr,
        known: Option<i128>,
        ty: Ty<'tcx>,
        span: Span,
    ) -> R<Expr> {
        let comparison = match op {
            BinOp::Eq => Some(Op::Eq),
            BinOp::Ne => Some(Op::Ne),
            BinOp::Lt => Some(Op::Lt),
            BinOp::Le => Some(Op::Le),
            BinOp::Gt => Some(Op::Gt),
            BinOp::Ge => Some(Op::Ge),
            _ => None,
        };
        if let Some(js_op) = comparison {
            return Ok(Expr::bin(js_op, l, r));
        }

        if ty.is_bool() {
            // `&`, `|`, `^` on bools evaluate both sides and give a bool.
            return match op {
                BinOp::BitXor => Ok(Expr::bin(Op::Ne, l, r)),
                BinOp::BitAnd => Ok(Expr::unary(
                    UnaryOp::Not,
                    Expr::unary(UnaryOp::Not, Expr::bin(Op::BitAnd, l, r)),
                )),
                BinOp::BitOr => Ok(Expr::unary(
                    UnaryOp::Not,
                    Expr::unary(UnaryOp::Not, Expr::bin(Op::BitOr, l, r)),
                )),
                _ => Err(self.unsupported(span, "this operator on `bool`")),
            };
        }

        let num = self.num(ty, span)?;
        if num == Num::F64 {
            let js_op = match op {
                BinOp::Add => Op::Add,
                BinOp::Sub => Op::Sub,
                BinOp::Mul => Op::Mul,
                BinOp::Div => Op::Div,
                BinOp::Rem => Op::Rem,
                _ => return Err(self.unsupported(span, "this operator on `f64`")),
            };
            return Ok(Expr::bin(js_op, l, r));
        }
        if num.big() {
            return self.big_binary(op, l, r, known, num, span);
        }

        // Integers: compute exactly in JS, then wrap back into range.
        Ok(match op {
            BinOp::Add => num.wrap(Expr::bin(Op::Add, l, r)),
            BinOp::Sub => num.wrap(Expr::bin(Op::Sub, l, r)),
            // A 32-bit product can exceed 2^53 and lose bits; `Math.imul` can't.
            BinOp::Mul if num.bits() == 32 => {
                let product = Expr::call(Expr::member(Expr::var("Math"), "imul"), vec![l, r]);
                if num.signed() { product } else { num.wrap(product) }
            }
            BinOp::Mul => num.wrap(Expr::bin(Op::Mul, l, r)),
            BinOp::Div | BinOp::Rem => {
                let (js_op, helper, name) = match op {
                    BinOp::Div => (Op::Div, Helper::Div, "$div"),
                    _ => (Op::Rem, Helper::Rem, "$rem"),
                };
                // A literal divisor that can't panic stays inline: `a / 3 | 0`.
                let safe = known
                    .or_else(|| r.as_int())
                    .is_some_and(|d| d != 0 && !(num.signed() && d == -1));
                let quotient = if safe {
                    Expr::bin(js_op, l, r)
                } else {
                    self.runtime.insert(helper);
                    let mut args = vec![l, r];
                    if num.signed() {
                        args.push(Expr::int(num.range().0));
                    }
                    Expr::call(Expr::var(name), args)
                };
                // The remainder of in-range integers is already in range.
                if op == BinOp::Rem && safe {
                    quotient
                } else {
                    num.wrap(quotient)
                }
            }
            BinOp::BitAnd => self.bitwise(Op::BitAnd, l, r, num),
            BinOp::BitOr => self.bitwise(Op::BitOr, l, r, num),
            BinOp::BitXor => self.bitwise(Op::BitXor, l, r, num),
            // Rust (without overflow checks) masks the shift amount to the
            // type's width. JS masks to 32, which is only right for 32 bits.
            BinOp::Shl if num.bits() == 32 => num.wrap(Expr::bin(Op::Shl, l, r)),
            BinOp::Shl => num.wrap(Expr::bin(Op::Shl, l, mask_shift(r, num))),
            BinOp::Shr => {
                let js_op = if num.signed() { Op::Shr } else { Op::UShr };
                let r = if num.bits() == 32 { r } else { mask_shift(r, num) };
                Expr::bin(js_op, l, r)
            }
            _ => return Err(self.unsupported(span, "this operator")),
        })
    }

    /// An `i64`'s or a `u64`'s operator (ADR 0086): exact on BigInts, then
    /// wrapped. A quotient needs no wrap, but can panic as Rust's does.
    pub(super) fn big_binary(
        &mut self,
        op: BinOp,
        l: Expr,
        r: Expr,
        known: Option<i128>,
        num: Num,
        span: Span,
    ) -> R<Expr> {
        Ok(match op {
            BinOp::Add => num.wrap(Expr::bin(Op::Add, unwrapped(l), unwrapped(r))),
            BinOp::Sub => num.wrap(Expr::bin(Op::Sub, unwrapped(l), unwrapped(r))),
            BinOp::Mul => num.wrap(Expr::bin(Op::Mul, unwrapped(l), unwrapped(r))),
            BinOp::Div | BinOp::Rem => {
                let (js_op, helper, name) = match op {
                    BinOp::Div => (Op::Div, Helper::BigDiv, "$bigDiv"),
                    _ => (Op::Rem, Helper::BigRem, "$bigRem"),
                };
                let safe = known
                    .or_else(|| r.as_bigint())
                    .is_some_and(|d| d != 0 && !(num.signed() && d == -1));
                if safe {
                    Expr::bin(js_op, l, r)
                } else {
                    self.runtime.insert(helper);
                    let mut args = vec![l, r];
                    if num.signed() {
                        args.push(Expr::bigint(num.range().0));
                    }
                    Expr::call(Expr::var(name), args)
                }
            }
            // Of two in range, in range.
            BinOp::BitAnd => Expr::bin(Op::BitAnd, l, r),
            BinOp::BitOr => Expr::bin(Op::BitOr, l, r),
            BinOp::BitXor => Expr::bin(Op::BitXor, l, r),
            // The amount masked to 63, as release Rust masks it, and a BigInt,
            // whatever its own type.
            BinOp::Shl => num.wrap(Expr::bin(Op::Shl, unwrapped(l), big_shift(r))),
            BinOp::Shr => Expr::bin(Op::Shr, l, big_shift(r)),
            _ => return Err(self.unsupported(span, "this operator")),
        })
    }

    /// A shift of a narrower integer by an `i64` or a `u64`: the amount a
    /// number, `Number(n & 63n)`, as JS won't shift a number by a BigInt.
    /// 63 keeps every width's own mask, which the shift then applies.
    pub(super) fn shift_amount(&self, op: BinOp, r: Expr, lhs: ExprId, rhs: ExprId) -> Expr {
        shift_amount_of(op, r, self.thir[lhs].ty, self.thir[rhs].ty)
    }

    pub(super) fn bitwise(&self, op: Op, l: Expr, r: Expr, num: Num) -> Expr {
        // JS bitwise ops return signed 32-bit results; only u32 needs fixing.
        let e = Expr::bin(op, l, r);
        if num == Num::U32 { num.wrap(e) } else { e }
    }

    pub(super) fn unary(&mut self, op: UnOp, a: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        match op {
            UnOp::Not if ty.is_bool() => Ok(Expr::unary(UnaryOp::Not, a)),
            UnOp::Not => {
                let num = self.num(ty, span)?;
                if num == Num::F64 {
                    return Err(self.unsupported(span, "`!` on `f64`"));
                }
                let e = Expr::unary(UnaryOp::BitNot, a);
                Ok(if num.signed() { e } else { num.wrap(e) })
            }
            UnOp::Neg => {
                let num = self.num(ty, span)?;
                // `-x` of a literal is just a negative literal, in range:
                // `-129i8` is 127, as `-i8::MIN` is itself.
                if let Some(n) = a.as_int().or_else(|| a.as_bigint()) {
                    return Ok(num.wrap(num.literal(-n)));
                }
                Ok(num.wrap(Expr::unary(UnaryOp::Neg, a)))
            }
            UnOp::PtrMetadata => Err(self.unsupported(span, "pointer metadata")),
        }
    }

    pub(super) fn cast(&mut self, v: Expr, from: Ty<'tcx>, to: Ty<'tcx>, span: Span) -> R<Expr> {
        let target = self.num(to, span)?;
        if from.is_bool() && target != Num::F64 {
            return Ok(Expr::cond(v, target.literal(1), target.literal(0)));
        }
        // A `char` is its code point (ADR 0063), and a `u8` as a `char` its
        // character.
        if from.is_char() {
            let code = Expr::call(Expr::member(v, "codePointAt"), vec![Expr::int(0)]);
            let code = if target.big() { to_bigint(code) } else { code };
            let (lo, hi) = target.range();
            return Ok(if target == Num::F64 || (lo <= 0 && hi >= 0x10ffff) {
                code
            } else {
                target.wrap(code)
            });
        }
        // An `Ordering` is -1, 0 or 1 already (ADR 0036).
        let (v, source) = if self.is_lang_adt(from, LangItem::OrderingEnum) {
            (v, Num::I8)
        } else if let ty::Adt(adt, _) = from.kind()
            && is_fieldless_enum(*adt)
        {
            // A fieldless enum is its variant's name (ADR 0013): its
            // discriminant, `["Red", "Green"].indexOf(color)` when they count
            // up from 0, and looked up by name otherwise.
            let discriminants = discriminants(self.tcx, *adt);
            let counting = discriminants.iter().enumerate().all(|(i, &(_, d))| d == i as i128);
            // Each one fits the target type, so there's nothing to wrap.
            let (lo, hi) = target.range();
            let fits = target == Num::F64 || discriminants.iter().all(|&(_, d)| lo <= d && d <= hi);
            let repr = rustc_middle::ty::util::IntTypeExt::to_ty(&adt.repr().discr_type(), self.tcx);
            let repr = Num::of(repr).unwrap_or(Num::I32);
            // The discriminants as what they're read as: the target, if each
            // fits it, else the enum's own type, cast to it after. A 64-bit
            // one is a BigInt from the start, exact past 2^53.
            let read = if fits { target } else { repr };
            let value = if counting {
                let names = Expr::array(discriminants.into_iter().map(|(n, _)| Expr::str(n)).collect());
                let index = Expr::call(Expr::member(names, "indexOf"), vec![v]);
                if read.big() { to_bigint(index) } else { index }
            } else {
                let table = Expr::object(
                    discriminants
                        .into_iter()
                        .map(|(n, d)| Prop::Field(n, if read.big() { Expr::bigint(d) } else { Expr::int(d) }))
                        .collect(),
                );
                Expr::index(table, v)
            };
            if fits {
                return Ok(value);
            }
            (value, repr)
        } else {
            (v, self.num(from, span)?)
        };
        match (source, target) {
            (Num::F64, Num::F64) => Ok(v),
            // `as` from float to int saturates (ADR 0086): `NaN` is 0, and the
            // rest is truncated into range.
            (Num::F64, _) => {
                let (lo, hi) = target.range();
                let (helper, name) = if target.big() {
                    (Helper::F64ToBig, "$f64ToBig")
                } else {
                    (Helper::F64ToInt, "$f64ToInt")
                };
                self.runtime.insert(helper);
                Ok(Expr::call(
                    Expr::var(name),
                    vec![v, target.literal(lo), target.literal(hi)],
                ))
            }
            // An `i64` or `u64` is its nearest `f64`, as `as` rounds it.
            (source, Num::F64) if source.big() => Ok(Expr::call(Expr::var("Number"), vec![v])),
            // Every other integer fits exactly in an f64.
            (_, Num::F64) => Ok(v),
            _ => {
                let (lo, hi) = source.range();
                let (tlo, thi) = target.range();
                let fits = tlo <= lo && hi <= thi;
                Ok(match (source.big(), target.big()) {
                    // Into a BigInt, then into range.
                    (false, true) if fits => to_bigint(v),
                    (false, true) => target.wrap(to_bigint(v)),
                    // A constant, or what a mask keeps in range: `x & 1023n`.
                    (true, false) if let Some(n) = v.as_bigint() => target.wrap(Expr::int(n)),
                    (true, false) if masked(&v).is_some_and(|mask| (0..=thi).contains(&mask)) => {
                        Expr::call(Expr::var("Number"), vec![v])
                    }
                    // Into range as a BigInt, then a number.
                    (true, false) => {
                        let method = if target.signed() { "asIntN" } else { "asUintN" };
                        let wrapped = Expr::call(
                            Expr::member(Expr::var("BigInt"), method),
                            vec![Expr::int(target.bits().into()), v],
                        );
                        Expr::call(Expr::var("Number"), vec![wrapped])
                    }
                    _ if fits => v,
                    _ => target.wrap(v),
                })
            }
        }
    }
}

pub(super) fn mask_shift(r: Expr, num: Num) -> Expr {
    Expr::bin(Op::BitAnd, r, Expr::num(num.bits() - 1))
}

/// `shift_amount`, by the two types: a trait's, `Shl<i64>` of a `u32`, where
/// there are no expressions, as in a dictionary (ADR 0108).
pub(super) fn shift_amount_of<'tcx>(op: BinOp, r: Expr, lhs: Ty<'tcx>, rhs: Ty<'tcx>) -> Expr {
    let big = |ty: Ty<'tcx>| Num::of(ty.peel_refs()).is_some_and(Num::big);
    if !matches!(op, BinOp::Shl | BinOp::Shr) || big(lhs) || !big(rhs) {
        return r;
    }
    match r.as_bigint() {
        Some(n) => Expr::int(n & 63),
        None => Expr::call(Expr::var("Number"), vec![Expr::bin(Op::BitAnd, r, Expr::bigint(63))]),
    }
}

/// `BigInt(x)`, or of a literal, the BigInt literal.
pub(super) fn to_bigint(e: Expr) -> Expr {
    match e.as_int() {
        Some(n) => Expr::bigint(n),
        None => Expr::call(Expr::var("BigInt"), vec![e]),
    }
}

/// A 64-bit shift's amount: masked to 63, as a BigInt, `BigInt(n) & 63n`.
pub(super) fn big_shift(r: Expr) -> Expr {
    match r.as_int().or_else(|| r.as_bigint()) {
        Some(n) => Expr::bigint(n & 63),
        None => Expr::bin(Op::BitAnd, to_bigint(r), Expr::bigint(63)),
    }
}

/// `x + y` of `BigInt.asUintN(64, x + y)`: what's added, subtracted,
/// multiplied or shifted left needn't be wrapped itself, as the result is,
/// modulo the same 2^64, so `a + b + c` is wrapped once.
pub(super) fn unwrapped(e: Expr) -> Expr {
    if let js::ExprKind::Call(callee, args) = &e.kind
        && let js::ExprKind::Member(object, name) = &callee.kind
        && matches!(&object.kind, js::ExprKind::Var(v) if v == "BigInt")
        && (name == "asUintN" || name == "asIntN")
        && let [bits, inner] = args.as_slice()
        && bits.as_int() == Some(64)
    {
        return inner.clone();
    }
    e
}

/// The mask of `x & 1023n`, which keeps it from 0 to 1023.
pub(super) fn masked(e: &Expr) -> Option<i128> {
    match &e.kind {
        js::ExprKind::Binary(Op::BitAnd, a, b) => b.as_bigint().or_else(|| a.as_bigint()),
        _ => None,
    }
}

pub(super) fn assign_op(op: AssignOp) -> BinOp {
    match op {
        AssignOp::AddAssign => BinOp::Add,
        AssignOp::SubAssign => BinOp::Sub,
        AssignOp::MulAssign => BinOp::Mul,
        AssignOp::DivAssign => BinOp::Div,
        AssignOp::RemAssign => BinOp::Rem,
        AssignOp::BitXorAssign => BinOp::BitXor,
        AssignOp::BitAndAssign => BinOp::BitAnd,
        AssignOp::BitOrAssign => BinOp::BitOr,
        AssignOp::ShlAssign => BinOp::Shl,
        AssignOp::ShrAssign => BinOp::Shr,
    }
}
