//! A placeholder's options (ADR 0058): `{:>8}`, `{:08.3}`, `{:#x}`. They
//! apply where Rust applies them: numbers, strings, `char`s and `bool`s pad,
//! and a `fmt` that writes with `write!` ignores them, as it does in Rust.

use super::display::Pretty;
use super::recognition::Std;
use super::representation::Num;
use super::{FnCx, R};
use crate::js::{Expr, Op, Prop};
use crate::runtime::Helper;
use rustc_middle::ty::Ty;
use rustc_span::Span;

/// A placeholder's options, with its width and precision, which a `{:?}`
/// gives each part of what it shows (ADR 0058).
#[derive(Clone)]
pub(super) struct Options {
    pub(super) spec: Spec,
    pub(super) width: Option<Expr>,
    pub(super) precision: Option<Expr>,
}

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
        let (value, ty) = self.through_refs(value, ty);
        let num = Num::of(ty);
        if spec.debug_hex {
            return Err(self.unsupported(span, "`{:x?}`"));
        }
        // Rust gives a placeholder's options to the `fmt` that shows the value,
        // which gives a `{:?}`'s to each part of what it shows, and hands its
        // `Formatter` on to another `fmt`. A number, a `bool` and a string are
        // padded here. Anything else is shown given them: each part of it
        // that's one of those applies them here, and a `fmt` of the crate's
        // own, a generic `T`'s or a `dyn`'s is given them as an object.
        let shown = self.shown_type(ty);
        let leaf = num.is_some() || shown.is_bool() || shown.is_unit() || self.is_string_like(shown);
        let options = width.is_some() || precision.is_some() || spec.plus;
        if options && !leaf && matches!(kind, Std::FmtDisplay | Std::FmtDebug) {
            let given = Pretty::Given(options_object(spec, &width, &precision), spec.alternate);
            let options = Options { spec, width, precision };
            return self.with_options(Some(options), |cx| match kind {
                Std::FmtDisplay => cx.display_string_with(value, ty, span, &given),
                _ => cx.debug_string_with(value, ty, span, &given),
            });
        }
        let text = match kind {
            Std::FmtRadix(radix) => {
                let Some(num) = num.filter(|&n| !n.float()) else {
                    return Err(self.unsupported(span, &format!("`{{:x}}` and the like of a `{ty}`")));
                };
                // A negative number's bits, as Rust shows them: `-1i32` is `ffffffff`.
                let bits = match num {
                    Num::I8 => Expr::bin(Op::BitAnd, value, Expr::int(0xff)),
                    Num::I16 => Expr::bin(Op::BitAnd, value, Expr::int(0xffff)),
                    Num::I32 => Expr::bin(Op::UShr, value, Expr::int(0)),
                    Num::I64 => Num::U64.wrap(value),
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
            // `{:e}`: the shortest digits, as JS's `toExponential()` has them,
            // written as Rust writes them, `1.2345e3`. With a precision, Rust
            // rounds a tie to even and JS away from zero: not yet.
            Std::FmtExp(upper) => {
                if num.is_none() {
                    return Err(self.unsupported(span, &format!("`{{:e}}` of a `{ty}`")));
                }
                if precision.is_some() {
                    return Err(self.unsupported(span, "`{:.2e}` and the like"));
                }
                self.runtime.insert(Helper::LowerExp);
                let text = Expr::call(Expr::var("$lowerExp"), vec![value]);
                if upper {
                    Expr::call(Expr::member(text, "toUpperCase"), Vec::new())
                } else {
                    text
                }
            }
            // `{:.2}` of an `f64`: exact, and rounded as Rust rounds.
            _ if let Some(digits) = precision.clone()
                && num.is_some_and(Num::float) =>
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
            // `{:#}` of a `Value`: its pretty JSON (ADR 0083).
            Std::FmtDisplay if spec.alternate && self.json_type(ty).is_some() => self
                .json_value_display(value.clone(), ty, true)
                .map_or_else(|| self.display_string(value, ty, span), Ok)?,
            // `{:#}` of the crate's own `Display`, which may ask (ADR 0137).
            Std::FmtDisplay if spec.alternate => self.display_string_with(value, ty, span, &Pretty::Always)?,
            Std::FmtDisplay => self.display_string(value, ty, span)?,
            // By the type (ADR 0060): `1.0`, `Some(1)`, `Point { x: 1.0 }`;
            // `{:#?}` on lines of their own, indented (ADR 0137).
            _ if spec.alternate => self.debug_string_with(value, ty, span, &Pretty::Always)?,
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
        // A `bool`'s `Debug` is its `Display`, and `()`'s pads `"()"`.
        let pads = num.is_some()
            || ty.is_bool()
            || (kind == Std::FmtDisplay && self.is_string_like(ty))
            || (kind == Std::FmtDebug && ty.is_unit());
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

/// A placeholder's options as the object a `fmt` is given (ADR 0058):
/// `{ width: 6, align: ">" }`, with only those it has.
fn options_object(spec: Spec, width: &Option<Expr>, precision: &Option<Expr>) -> Expr {
    let mut fields = Vec::new();
    let mut field = |name: &str, value: Expr| fields.push(Prop::Field(name.into(), value));
    if spec.alternate {
        field("alternate", Expr::bool(true));
    }
    if let Some(width) = width {
        field("width", width.clone());
    }
    if let Some(precision) = precision {
        field("precision", precision.clone());
    }
    if spec.fill != ' ' {
        field("fill", Expr::str(spec.fill.to_string()));
    }
    // As `f.align()` gives it, a `fmt::Alignment`'s variant.
    if let Some(align) = spec.align {
        let name = match align {
            '<' => "Left",
            '>' => "Right",
            _ => "Center",
        };
        field("align", Expr::str(name));
    }
    if spec.plus {
        field("plus", Expr::bool(true));
    }
    if spec.zero {
        field("zero", Expr::bool(true));
    }
    Expr::object(fields)
}
