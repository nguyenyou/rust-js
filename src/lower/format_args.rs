//! `format_args!`, as `format!`, `println!` and `write!` hand it over: its
//! template decoded, and the values it shows written into a JS template
//! literal, in Rust's order (ADRs 0034, 0058, 0066).

use super::calls::Call;
use super::display::Pretty;
use super::format_spec::Spec;
use super::{FnCx, R, Std};
use crate::js;
use crate::js::StmtKind;
use crate::js::{Expr, Op, Stmt};
use crate::runtime::Helper;
use rustc_ast::LitKind;
use rustc_hir::LangItem;
use rustc_middle::thir::{self, ExprId, ExprKind, PatKind};
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;
use std::collections::HashSet;

/// A piece of a `format_args!` template.
pub(super) enum Piece {
    Text(String),
    /// A placeholder: which of the arguments goes there, and its options.
    Argument(usize, Spec),
}
/// A `format_args!`, taken apart (`as_format_args`).
pub(super) struct FormatArgs<'tcx> {
    template: Vec<u8>,
    /// What's formatted, in the order it's written.
    pub(super) values: Vec<ExprId>,
    /// Each placeholder's argument: which value, how, and its type.
    slots: Vec<(usize, Std, Ty<'tcx>)>,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// A `format_args!` template, decoded (`decode_template`).
    fn decode_template(&self, template: &[u8], span: Span) -> R<Vec<Piece>> {
        decode_template(template).ok_or_else(|| self.unsupported(span, "this format string"))
    }

    /// The string a template makes: its pieces, with `items` (the arguments,
    /// already strings) in place, joined by `+`.
    pub(super) fn format(&self, template: &[u8], items: Expr, span: Span) -> R<Expr> {
        let parts = self
            .decode_template(template, span)?
            .into_iter()
            .map(|piece| match piece {
                Piece::Argument(index, spec) if spec == Spec::plain() => Ok(match &items.kind {
                    js::ExprKind::Array(values) => values[index].clone(),
                    _ => Expr::index(items.clone(), Expr::int(index as i128)),
                }),
                Piece::Argument(..) => Err(self.unsupported(span, "formatting options here")),
                Piece::Text(text) => Ok(Expr::str(text)),
            })
            .collect::<R<Vec<_>>>()?;
        Ok(parts
            .into_iter()
            .reduce(|a, b| Expr::bin(Op::Add, a, b))
            .unwrap_or_else(|| Expr::str("")))
    }

    /// `format_args!("{} and {:?}", a, b)` as rustc writes it: a block of
    /// `super let args = (&a, &b);`, `super let args = [new_display(args.0),
    /// new_debug(args.1)];`, then `format_arguments::new(template, &args)`.
    /// Recognized whole, like `?`, so its arguments can be written in place.
    pub(super) fn as_format_args(&self, e: ExprId) -> Option<FormatArgs<'tcx>> {
        let thir = self.thir;
        let ExprKind::Block { block } = thir[self.strip(e)].kind else {
            return None;
        };
        let block = &thir[block];
        let ([values, arguments], Some(tail)) = (&*block.stmts, block.expr) else {
            return None;
        };
        let init = |stmt: thir::StmtId| match thir[stmt].kind {
            thir::StmtKind::Let {
                initializer: Some(init),
                ref pattern,
                ..
            } => match pattern.kind {
                PatKind::Binding { var, .. } => Some((var, self.strip(init))),
                _ => None,
            },
            _ => None,
        };
        let (tuple, values) = init(*values)?;
        let ExprKind::Tuple { ref fields } = thir[values].kind else {
            return None;
        };
        let values = fields
            .iter()
            .map(|&f| match thir[self.strip(f)].kind {
                ExprKind::Borrow { arg, .. } => Some(arg),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let (array, arguments) = init(*arguments)?;
        let ExprKind::Array { ref fields } = thir[arguments].kind else {
            return None;
        };
        // Each is `new_display(args.0)` or `new_debug(args.0)`.
        let slots = fields
            .iter()
            .map(|&f| {
                let ExprKind::Call { fun, ref args, .. } = thir[self.strip(f)].kind else {
                    return None;
                };
                let kind = self.std_fn(fun).filter(|k| {
                    matches!(
                        k,
                        Std::FmtDisplay | Std::FmtDebug | Std::FmtRadix(_) | Std::FmtExp(_) | Std::FmtUsize
                    )
                })?;
                let &ty::FnDef(_, generic_args) = thir[self.strip(fun)].ty.kind() else {
                    return None;
                };
                let mut arg = self.strip(*args.first()?);
                while let ExprKind::Borrow { arg: inner, .. } | ExprKind::Deref { arg: inner } = thir[arg].kind {
                    arg = self.strip(inner);
                }
                let ExprKind::Field { lhs, name, .. } = thir[arg].kind else {
                    return None;
                };
                matches!(thir[self.strip(lhs)].kind, ExprKind::VarRef { id } if id == tuple)
                    .then(|| {
                        let ty = match kind {
                            Std::FmtUsize => Some(self.tcx.types.usize),
                            _ => generic_args.types().next(),
                        };
                        ty.map(|ty| (name.as_usize(), kind, ty))
                    })
                    .flatten()
            })
            .collect::<Option<Vec<_>>>()?;
        // `unsafe { format_arguments::new(template, &args) }`.
        let mut tail = self.strip(tail);
        while let ExprKind::Block { block } = thir[tail].kind {
            tail = self.strip(thir[block].expr?);
        }
        let ExprKind::Call { fun, ref args, .. } = thir[tail].kind else {
            return None;
        };
        if self.std_fn(fun) != Some(Std::FmtNew) {
            return None;
        }
        // `&args`, made a slice.
        let mut list = self.strip(args[1]);
        while let ExprKind::Borrow { arg, .. }
        | ExprKind::Deref { arg }
        | ExprKind::PointerCoercion { source: arg, .. } = thir[list].kind
        {
            list = self.strip(arg);
        }
        if !matches!(thir[list].kind, ExprKind::VarRef { id } if id == array) {
            return None;
        }
        let ExprKind::Literal { lit, .. } = thir[self.strip_refs(args[0])].kind else {
            return None;
        };
        let LitKind::ByteStr(ref bytes, _) = lit.node else {
            return None;
        };
        Some(FormatArgs {
            template: bytes.as_byte_str().to_vec(),
            values,
            slots,
        })
    }

    /// A variable, a field of one, or a `const`: a place, not a value made.
    fn is_place_expr(&self, e: ExprId) -> bool {
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { .. } | ExprKind::UpvarRef { .. } | ExprKind::NamedConst { .. } => true,
            ExprKind::Literal { .. } | ExprKind::NonHirLiteral { .. } => true,
            ExprKind::Field { lhs, .. } | ExprKind::Deref { arg: lhs } | ExprKind::Borrow { arg: lhs, .. } => {
                self.is_place_expr(lhs)
            }
            _ => false,
        }
    }

    /// Which value each placeholder shows, in the template's order. `None`
    /// if it can't be read, which `format` then reports.
    fn shown(&self, f: &FormatArgs<'tcx>, span: Span) -> Option<Vec<usize>> {
        let pieces = self.decode_template(&f.template, span).ok()?;
        pieces
            .into_iter()
            .filter_map(|piece| match piece {
                Piece::Argument(slot, _) => Some(f.slots.get(slot).map(|&(value, _, _)| value)),
                Piece::Text(_) => None,
            })
            .collect()
    }

    /// Does the template show each value once, in the order they're written?
    /// Then each can be written in its place, and runs when Rust runs it.
    pub(super) fn in_order(&self, f: &FormatArgs<'tcx>, span: Span) -> bool {
        self.shown(f, span)
            .is_some_and(|shown| shown.into_iter().eq(0..f.values.len()))
    }

    /// The string `format_args!` makes, its arguments in their places:
    /// `"<" + g(2) + ">"`. Shown in another order (`{1} {0}`, or named ones
    /// after the rest), they can still be written in place if none has
    /// effects, since nothing then changes in between. Otherwise each goes
    /// in a `const` first, in the order Rust runs them, unless it's a place:
    /// borrowed until the end, a place can't be changed by the others. One
    /// read twice goes in a `const` too, unless it's a variable or a
    /// constant: shown twice, or shown by its parts, as `{:?}` of an
    /// `Option` is. Those before it that have effects go first, to keep
    /// Rust's order.
    pub(super) fn lower_format_args(&mut self, f: FormatArgs<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let in_order = self.in_order(&f, span);
        let shown = self.shown(&f, span).unwrap_or_default();
        // `{:#?}` of a `&dyn Debug` made here, as `dbg!` makes one: made pretty
        // (ADR 0137). One made elsewhere is the string it showed there.
        let mut pretty_dyn = HashSet::new();
        let mut plain_dyn = HashSet::new();
        for piece in self.decode_template(&f.template, span)? {
            if let Piece::Argument(slot, spec) = piece
                && let Some(&(value, kind, ty)) = f.slots.get(slot)
                && kind == Std::FmtDebug
                && self.is_dyn_debug(ty)
            {
                match spec.alternate {
                    true => pretty_dyn.insert(value),
                    false => plain_dyn.insert(value),
                };
            }
        }
        if !pretty_dyn.is_empty() {
            let made_here = pretty_dyn.iter().all(|&i| {
                matches!(
                    self.thir[self.strip(f.values[i])].kind,
                    ExprKind::PointerCoercion { .. }
                )
            });
            if !made_here || !plain_dyn.is_empty() {
                return Err(self.unsupported(span, "`{:#?}` of a `&dyn Debug` made elsewhere"));
            }
        }
        let pretty = if pretty_dyn.is_empty() {
            Pretty::Plain
        } else {
            Pretty::Always
        };
        let outer = std::mem::replace(&mut self.writing.dyn_debug, pretty);
        let values = self.operands(&f.values, out);
        self.writing.dyn_debug = outer;
        let mut values = values?;
        let effects = values.iter().any(Expr::has_effects);
        let named: Vec<bool> = (0..values.len())
            .map(|i| {
                let twice = shown.iter().filter(|&&v| v == i).count() > 1;
                let by_parts = f
                    .slots
                    .iter()
                    .any(|&(v, kind, ty)| v == i && kind == Std::FmtDebug && self.debug_reads_parts(ty));
                (twice || by_parts) && !values[i].reads_same()
            })
            .collect();
        let last_named = named.iter().rposition(|&n| n);
        for (i, value) in values.iter_mut().enumerate() {
            let settled = value.is_constant() || self.is_place_expr(f.values[i]);
            let spill = if in_order {
                named[i] || last_named.is_some_and(|last| i < last && value.has_effects() && !settled)
            } else if effects {
                !settled
            } else {
                named[i]
            };
            if spill {
                let v = std::mem::replace(value, Expr::undefined());
                *value = self.spill("arg", v, out);
            }
        }
        // Each placeholder with its own options: `{:>5}` and `{}` of one value differ.
        let mut parts = Vec::new();
        for piece in self.decode_template(&f.template, span)? {
            parts.push(match piece {
                Piece::Text(text) => Expr::str(text),
                Piece::Argument(slot, spec) => {
                    let slot_value = |slot: usize| f.slots.get(slot).map(|&(value, _, _)| values[value].clone());
                    let bad = || self.unsupported(span, "this format string");
                    let &(value, kind, ty) = f.slots.get(slot).ok_or_else(bad)?;
                    let width = match spec.width_from {
                        Some(from) => Some(slot_value(from).ok_or_else(bad)?),
                        None => spec.width.map(|w| Expr::int(w.into())),
                    };
                    let precision = match spec.precision_from {
                        Some(from) => Some(slot_value(from).ok_or_else(bad)?),
                        None => spec.precision.map(|p| Expr::int(p.into())),
                    };
                    self.format_value(values[value].clone(), (kind, ty), spec, (width, precision), span)?
                }
            });
        }
        Ok(super::display::join(parts))
    }
}

/// A `format_args!` template, decoded (its encoding is documented in core's
/// `fmt::Arguments`): literal pieces prefixed by their length, and a byte
/// with the top two bits set for each placeholder, which names an argument by
/// its place in the array of them. `None` if it isn't one.
pub(super) fn decode_template(template: &[u8]) -> Option<Vec<Piece>> {
    let byte = |i: usize| template.get(i).copied();
    let u16_at = |i: usize| Some(u16::from_le_bytes([byte(i)?, byte(i + 1)?]) as usize);
    let piece = |from: usize, len: usize| {
        let bytes = template.get(from..from + len)?;
        Some(Piece::Text(String::from_utf8_lossy(bytes).into_owned()))
    };
    let (mut pieces, mut i, mut next) = (Vec::new(), 0, 0);
    loop {
        let b = byte(i)?;
        i += 1;
        match b {
            0 => break,
            1..=0x7f => {
                pieces.push(piece(i, b as usize)?);
                i += b as usize;
            }
            0x80 => {
                let len = u16_at(i)?;
                pieces.push(piece(i + 2, len)?);
                i += 2 + len;
            }
            _ if b & 0xc0 == 0xc0 => {
                // Then, if its bits say so: flags, width, precision, and
                // which argument (ADR 0058).
                let mut spec = Spec::plain();
                if b & 0b1 != 0 {
                    let flags = u32::from_le_bytes([byte(i)?, byte(i + 1)?, byte(i + 2)?, byte(i + 3)?]);
                    spec = Spec::from_flags(flags);
                    i += 4;
                }
                // An indirect one is the index of the argument that holds it.
                if b & 0b10 != 0 {
                    let field = u16_at(i)?;
                    match b & 0b1_0000 != 0 {
                        true => spec.width_from = Some(field),
                        false => spec.width = Some(field as u16),
                    }
                    i += 2;
                }
                if b & 0b100 != 0 {
                    let field = u16_at(i)?;
                    match b & 0b10_0000 != 0 {
                        true => spec.precision_from = Some(field),
                        false => spec.precision = Some(field as u16),
                    }
                    i += 2;
                }
                let index = if b & 0b1000 != 0 {
                    let k = u16_at(i)?;
                    i += 2;
                    k
                } else {
                    next
                };
                next = index + 1;
                pieces.push(Piece::Argument(index, spec));
            }
            _ => return None,
        }
    }
    Some(pieces)
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// What `print!`, `panic!` and `format_args!` hand their values to (ADRs 0026, 0034): `None` if `known` is another.
    pub(super) fn print_call(
        &mut self,
        known: Std,
        call: Call<'_, 'tcx>,
        values: &mut std::vec::IntoIter<Expr>,
        out: &mut Vec<Stmt>,
    ) -> R<Option<Expr>> {
        let Call { generic_args, span, .. } = call;
        let mut arg = || values.next().expect("rustc checked the arguments");
        let js_span = self.js_span(span);
        Ok(Some(match known {
            Std::Panic | Std::PanicFmt => {
                out.push(StmtKind::Throw(Expr::new_(Expr::var("Error"), vec![arg()])).at(js_span));
                Expr::undefined()
            }
            // A `&str` or a `String` is the panic's message, as Rust's hook
            // shows it; another payload, `panic!(5)`, has none.
            Std::BeginPanic => {
                let payload = generic_args.types().next().expect("`begin_panic` has a type argument");
                let text = matches!(payload.kind(), ty::Ref(_, inner, _) if inner.is_str())
                    || self.is_lang_adt(payload, LangItem::String);
                if !text {
                    return Err(self.unsupported(span, "a panic whose payload isn't text"));
                }
                out.push(StmtKind::Throw(Expr::new_(Expr::var("Error"), vec![arg()])).at(js_span));
                Expr::undefined()
            }
            // A whole line is `console.log`'s, which ends it; text that may
            // not end one is written as it is (ADR 0087).
            Std::Print { error } => match without_newline(arg()) {
                Ok(line) => Expr::call(
                    Expr::member(Expr::var("console"), if error { "error" } else { "log" }),
                    vec![line],
                ),
                Err(text) => {
                    self.runtime.insert(Helper::Print);
                    Expr::call(Expr::var(if error { "$eprint" } else { "$print" }), vec![text])
                }
            },
            Std::FmtStr => arg(),
            Std::FmtDisplay => {
                let ty = generic_args.types().next().expect("`new_display` has a type argument");
                self.display_string(arg(), ty, span)?
            }
            Std::FmtDebug => {
                let ty = generic_args.types().next().expect("`new_debug` has a type argument");
                self.debug_string(arg(), ty, span)?
            }
            // Only in a `format_args!` it recognizes whole (ADR 0058).
            Std::FmtRadix(_) | Std::FmtExp(_) | Std::FmtUsize => {
                return Err(self.unsupported(span, "`{:x}` and the like here"));
            }
            _ => return Ok(None),
        }))
    }
}

/// `"a\n"` or `` `a ${x}\n` ``: the line, `"a"`, without the newline
/// `println!` ends it with. Anything else is given back.
pub(super) fn without_newline(text: Expr) -> Result<Expr, Expr> {
    match &text.kind {
        js::ExprKind::Str(s) if s.ends_with('\n') => Ok(Expr::str(&s[..s.len() - 1])),
        js::ExprKind::Template(texts, values) if texts.last().is_some_and(|last| last.ends_with('\n')) => {
            let mut texts = texts.clone();
            texts.last_mut().expect("a template has a last text").pop();
            Ok(Expr::template(texts, values.clone()))
        }
        _ => Err(text),
    }
}
