//! Methods of integers and `f64` (ADR 0064). A number is a JS number
//! (ADR 0025), so most are `Math`'s: `x.sqrt()` is `Math.sqrt(x)`. Where
//! Rust's answer differs from JS's (`round` of a half, `pow` past 2^53),
//! a helper gives Rust's.

use super::representation::Num;
use super::{FnCx, R};
use crate::js::{Expr, Op, Stmt};
use crate::runtime::Helper;
use rustc_middle::mir::BinOp;
use rustc_middle::thir::ExprId;
use rustc_middle::ty::Ty;
use rustc_span::Span;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum NumOp {
    /// The same in JS: `Math.floor(x)`, `Math.atan2(y, x)`.
    Math(&'static str),
    Abs,
    Pow,
    Powi,
    Powf,
    Round,
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

/// Which `NumOp` a method of a number is.
pub(super) fn classify(name: &str, num: Num) -> Option<NumOp> {
    let float = num == Num::F64;
    let signed = num.signed();
    Some(match name {
        "floor" | "ceil" | "trunc" | "sqrt" | "cbrt" | "exp" | "log10" | "log2" | "sin" | "cos" | "tan" | "asin"
        | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "hypot" | "atan2"
            if float =>
        {
            NumOp::Math(match name {
                "floor" => "floor",
                "ceil" => "ceil",
                "trunc" => "trunc",
                "sqrt" => "sqrt",
                "cbrt" => "cbrt",
                "exp" => "exp",
                "log10" => "log10",
                "log2" => "log2",
                "sin" => "sin",
                "cos" => "cos",
                "tan" => "tan",
                "asin" => "asin",
                "acos" => "acos",
                "atan" => "atan",
                "sinh" => "sinh",
                "cosh" => "cosh",
                "tanh" => "tanh",
                "hypot" => "hypot",
                _ => "atan2",
            })
        }
        "ln" if float => NumOp::Math("log"),
        "abs" if float => NumOp::Math("abs"),
        "abs" if signed => NumOp::Abs,
        "pow" if !float => NumOp::Pow,
        "powi" if float => NumOp::Powi,
        "powf" if float => NumOp::Powf,
        "round" if float => NumOp::Round,
        "is_nan" if float => NumOp::IsNan,
        "is_finite" if float => NumOp::IsFinite,
        "is_infinite" if float => NumOp::IsInfinite,
        "checked_add" if !float => NumOp::Checked(BinOp::Add),
        "checked_sub" if !float => NumOp::Checked(BinOp::Sub),
        "checked_mul" if !float => NumOp::Checked(BinOp::Mul),
        "checked_div" if !float => NumOp::Checked(BinOp::Div),
        "saturating_add" if !float => NumOp::Saturating(BinOp::Add),
        "saturating_sub" if !float => NumOp::Saturating(BinOp::Sub),
        "saturating_mul" if !float => NumOp::Saturating(BinOp::Mul),
        "wrapping_add" if !float => NumOp::Wrapping(BinOp::Add),
        "wrapping_sub" if !float => NumOp::Wrapping(BinOp::Sub),
        "wrapping_mul" if !float => NumOp::Wrapping(BinOp::Mul),
        "rem_euclid" if !float => NumOp::RemEuclid,
        "div_euclid" if !float => NumOp::DivEuclid,
        "signum" if signed => NumOp::Signum,
        "leading_zeros" if !float => NumOp::LeadingZeros,
        "trailing_zeros" if !float => NumOp::TrailingZeros,
        "count_ones" if !float => NumOp::CountOnes,
        "is_power_of_two" if !float && !signed => NumOp::IsPowerOfTwo,
        "abs_diff" if !float => NumOp::AbsDiff,
        _ => return None,
    })
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
            // Exact below 2^53, and `$pow` multiplies as `Math.imul` does, so
            // what's past it wraps as Rust's does.
            NumOp::Pow => {
                let power = helper(self, Helper::Pow, "$pow", vec![arg(), arg()]);
                if num == Num::I32 { power } else { num.wrap(power) }
            }
            NumOp::Powi => helper(self, Helper::Powi, "$powi", vec![arg(), arg()]),
            NumOp::Powf => Expr::bin(Op::Pow, arg(), arg()),
            NumOp::Round => helper(self, Helper::Round, "$round", vec![arg()]),
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

fn js_op(op: BinOp) -> Op {
    match op {
        BinOp::Add => Op::Add,
        BinOp::Sub => Op::Sub,
        _ => Op::Mul,
    }
}
