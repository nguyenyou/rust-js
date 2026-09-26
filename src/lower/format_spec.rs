//! A placeholder's options (ADR 0058): `{:>8}`, `{:08.3}`, `{:#x}`. They
//! apply where Rust applies them: numbers, strings, `char`s and `bool`s pad,
//! and a `fmt` that writes with `write!` ignores them, as it does in Rust.

use super::representation::Num;
use super::stdlib::Std;
use super::{FnCx, R};
use crate::js::{Expr, Op};
use crate::runtime::Helper;
use rustc_middle::ty::Ty;
use rustc_span::Span;

/// A placeholder's options, as core's `FormattingOptions` encodes them.
#[derive(Clone, Copy, PartialEq, Default)]
pub(super) struct Spec {
    pub(super) fill: char,
    /// `<`, `>`, `^`, or none.
    pub(super) align: Option<char>,
    pub(super) plus: bool,
    pub(super) alternate: bool,
    pub(super) zero: bool,
    pub(super) debug_hex: bool,
    pub(super) width: Option<u16>,
    pub(super) precision: Option<u16>,
    /// `{:>w$}`, `{:.*}`: the width or precision is this argument's value.
    pub(super) width_from: Option<usize>,
    pub(super) precision_from: Option<usize>,
}

impl Spec {
    pub(super) fn plain() -> Spec {
        Spec {
            fill: ' ',
            ..Spec::default()
        }
    }

    /// From a placeholder's flags field (core's `fmt::FormattingOptions`).
    pub(super) fn from_flags(flags: u32) -> Spec {
        Spec {
            fill: char::from_u32(flags & 0x1f_ffff).unwrap_or(' '),
            align: match (flags >> 29) & 0b11 {
                0 => Some('<'),
                1 => Some('>'),
                2 => Some('^'),
                _ => None,
            },
            plus: flags & (1 << 21) != 0,
            alternate: flags & (1 << 23) != 0,
            zero: flags & (1 << 24) != 0,
            debug_hex: flags & (3 << 25) != 0,
            // A width or precision of 0 is only a flag: its field is left out.
            width: (flags & (1 << 27) != 0).then_some(0),
            precision: (flags & (1 << 28) != 0).then_some(0),
            width_from: None,
            precision_from: None,
        }
    }
}

/// `{:x}`, `{:X}`, `{:b}` and `{:o}`: a number's digits in another base.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Radix {
    LowerHex,
    UpperHex,
    Binary,
    Octal,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// One placeholder's string: `value` shown as `kind` says, with `spec`'s
    /// options, and its `width` and `precision`: numbers, or other arguments.
    pub(super) fn format_value(
        &mut self,
        value: Expr,
        (kind, ty): (Std, Ty<'tcx>),
        spec: Spec,
        (width, precision): (Option<Expr>, Option<Expr>),
        span: Span,
    ) -> R<Expr> {
        let ty = ty.peel_refs();
        let num = Num::of(ty);
        if spec.debug_hex {
            return Err(self.unsupported(span, "`{:x?}`"));
        }
        let text = match kind {
            Std::FmtRadix(radix) => {
                let Some(num) = num.filter(|&n| n != Num::F64) else {
                    return Err(self.unsupported(span, &format!("`{{:x}}` and the like of a `{ty}`")));
                };
                // A negative number's bits, as Rust shows them: `-1i32` is `ffffffff`.
                let bits = match num {
                    Num::I8 => Expr::bin(Op::BitAnd, value, Expr::int(0xff)),
                    Num::I16 => Expr::bin(Op::BitAnd, value, Expr::int(0xffff)),
                    Num::I32 => Expr::bin(Op::UShr, value, Expr::int(0)),
                    _ => value,
                };
                let base = match radix {
                    Radix::LowerHex | Radix::UpperHex => 16,
                    Radix::Binary => 2,
                    Radix::Octal => 8,
                };
                let mut digits = Expr::call(Expr::member(bits, "toString"), vec![Expr::int(base)]);
                if radix == Radix::UpperHex {
                    digits = Expr::call(Expr::member(digits, "toUpperCase"), Vec::new());
                }
                if spec.alternate {
                    let prefix = match radix {
                        Radix::LowerHex | Radix::UpperHex => "0x",
                        Radix::Binary => "0b",
                        Radix::Octal => "0o",
                    };
                    digits = Expr::bin(Op::Add, Expr::str(prefix), digits);
                }
                digits
            }
            // `{:.2}` of an `f64`: exact, and rounded as Rust rounds.
            _ if let Some(digits) = precision.clone()
                && num == Some(Num::F64) =>
            {
                self.runtime.insert(Helper::ToFixed);
                Expr::call(Expr::var("$toFixed"), vec![value, digits])
            }
            // `{:.3}` of a string: its first three `char`s.
            Std::FmtDisplay if precision.is_some() && self.is_string_like(ty) => {
                let chars = Expr::call(Expr::member(Expr::var("Array"), "from"), vec![value]);
                let kept = Expr::call(
                    Expr::member(chars, "slice"),
                    vec![Expr::int(0), precision.clone().unwrap_or_else(|| Expr::int(0))],
                );
                Expr::call(Expr::member(kept, "join"), vec![Expr::str("")])
            }
            _ if precision.is_some() && num.is_none() => {
                return Err(self.unsupported(span, &format!("a precision for a `{ty}`")));
            }
            Std::FmtDisplay => self.display_string(value, ty, span)?,
            // `{:#?}` breaks lines and indents: not yet.
            _ if spec.alternate => return Err(self.unsupported(span, "`{:#?}`")),
            // By the type (ADR 0060): `1.0`, `Some(1)`, `Point { x: 1.0 }`.
            _ => self.debug_string(value, ty, span)?,
        };
        let text = if spec.plus && num.is_some() {
            self.runtime.insert(Helper::Plus);
            Expr::call(Expr::var("$plus"), vec![text])
        } else {
            text
        };
        // Width: only what Rust pads. A `str`'s `{:?}` and a `fmt` of the
        // crate's own don't.
        let Some(width) = width else { return Ok(text) };
        let pads = num.is_some() || (kind == Std::FmtDisplay && (self.is_string_like(ty) || ty.is_bool()));
        if !pads {
            return Ok(text);
        }
        if spec.zero && num.is_some() {
            // After the sign and any `0x`.
            if !spec.plus && !spec.alternate && matches!(num, Some(Num::U8 | Num::U16 | Num::U32)) {
                return Ok(Expr::call(Expr::member(text, "padStart"), vec![width, Expr::str("0")]));
            }
            self.runtime.insert(Helper::ZeroPad);
            return Ok(Expr::call(Expr::var("$zeroPad"), vec![text, width]));
        }
        let align = spec.align.unwrap_or(if num.is_some() { '>' } else { '<' });
        let fill = spec.fill.to_string();
        // Numbers and `bool`s are ASCII, so JS's own padding counts right.
        if (num.is_some() || ty.is_bool()) && align != '^' && spec.fill.len_utf16() == 1 {
            let method = if align == '>' { "padStart" } else { "padEnd" };
            let mut args = vec![width];
            if spec.fill != ' ' {
                args.push(Expr::str(fill));
            }
            return Ok(Expr::call(Expr::member(text, method), args));
        }
        self.runtime.insert(Helper::Pad);
        let mut args = vec![text, width, Expr::str(align.to_string())];
        if spec.fill != ' ' {
            args.push(Expr::str(fill));
        }
        Ok(Expr::call(Expr::var("$pad"), args))
    }
}
