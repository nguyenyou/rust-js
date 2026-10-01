//! Lower standard-library formatting and iterator behavior. Recognition is separate.

use super::format_spec::Spec;
use super::representation::Num;
use super::{FnCx, R, Std};
use crate::js;
use crate::js::{Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_ast::LitKind;
use rustc_hir::LangItem;
use rustc_middle::thir::{self, ExprId, ExprKind, PatKind};
use rustc_middle::ty::{self, Ty};
use rustc_span::{ErrorGuaranteed, Span, Symbol, sym};

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
    /// Which std function `fun` is, if rust-js knows what it means in JS.
    pub(super) fn std_fn(&self, fun: ExprId) -> Option<Std> {
        let &ty::FnDef(def_id, args) = self.thir[self.strip(fun)].ty.kind() else {
            return None;
        };
        self.recognition().classify(def_id, args)
    }

    /// A `format_args!` template, decoded (its encoding is documented in
    /// core's `fmt::Arguments`): literal pieces prefixed by their length, and
    /// a byte with the top two bits set for each placeholder, which names an
    /// argument by its place in the array of them.
    fn decode_template(&self, template: &[u8], span: Span) -> R<Vec<Piece>> {
        let bad = |what: &str| self.unsupported(span, what);
        let byte = |i: usize| template.get(i).copied().ok_or_else(|| bad("this format string"));
        let u16_at = |i: usize| Ok::<usize, ErrorGuaranteed>(u16::from_le_bytes([byte(i)?, byte(i + 1)?]) as usize);
        let piece = |from: usize, len: usize| {
            let bytes = template
                .get(from..from + len)
                .ok_or_else(|| bad("this format string"))?;
            Ok::<Piece, ErrorGuaranteed>(Piece::Text(String::from_utf8_lossy(bytes).into_owned()))
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
                _ => return Err(bad("this format string")),
            }
        }
        Ok(pieces)
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
        let mut values = self.operands(&f.values, out)?;
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

    /// An iterator's method (ADR 0036). The iterator is a JS array: a range
    /// becomes one, `$range(a, b)`, and the rest already are.
    /// Is `ty` an iterator of the crate's own (ADR 0055)? `&mut` of one is too.
    pub(super) fn is_user_iterator(&self, ty: ty::Ty<'tcx>) -> bool {
        self.recognition().is_user_iterator(ty)
    }

    /// A type parameter that's an `Iterator`: `I: Iterator<Item = u32>`, or
    /// `impl Iterator` as a parameter's type (ADR 0061).
    pub(super) fn is_generic_iter(&self, ty: ty::Ty<'tcx>) -> bool {
        self.recognition().is_generic_iter(ty)
    }

    /// A type parameter with a bound of the std trait `name`.
    pub(super) fn bounded_by(&self, ty: ty::Ty<'tcx>, name: Symbol) -> bool {
        self.recognition().bounded_by(ty, name)
    }

    /// An iterator that's a JS iterator, not an array (ADR 0055): one of the
    /// crate's own, or std's adapters on one.
    pub(super) fn is_lazy_iter(&self, ty: ty::Ty<'tcx>) -> bool {
        self.recognition().is_lazy_iter(ty)
    }

    /// An iterator of the crate's own as a JS one, `$iterator(it,
    /// countdownIterator_next)`. Anything else is `value` itself.
    pub(super) fn iter_source(&mut self, value: Expr, ty: ty::Ty<'tcx>, span: Span) -> R<Expr> {
        let ty = self.reveal(ty);
        // A generic one is an array or a JS iterator: `Iterator.from` takes
        // either (ADR 0061).
        if self.is_generic_iter(ty) {
            return Ok(Expr::call(Expr::member(Expr::var("Iterator"), "from"), vec![value]));
        }
        if !self.is_user_iterator(ty) {
            return Ok(value);
        }
        let iterator = self.tcx.get_diagnostic_item(sym::Iterator).expect("std has `Iterator`");
        let next = self
            .tcx
            .associated_item_def_ids(iterator)
            .iter()
            .copied()
            .find(|&id| self.tcx.item_name(id) == sym::next)
            .expect("`Iterator` has `next`");
        let args = self.args_of(iterator, ty.peel_refs());
        // A generic `next` boxes a `Some` that looks like `None` (ADR 0051).
        let boxed = self.resolve_instance(next, args)?.is_some_and(|instance| {
            let id = instance.def_id();
            let output = self
                .tcx
                .fn_sig(id)
                .instantiate_identity()
                .skip_normalization()
                .skip_binder()
                .output();
            let output = self
                .tcx
                .try_normalize_erasing_regions(
                    ty::TypingEnv::post_analysis(self.tcx, id),
                    ty::Unnormalized::new_wip(output),
                )
                .unwrap_or(output);
            self.option_of(output).is_some_and(|item| self.boxed_payload(item))
        });
        let call = self.impl_call(next, args, vec![Expr::var("iterator")], span)?;
        // `(iterator) => f(iterator)` is `f`.
        let next = match &call.kind {
            js::ExprKind::Call(callee, list) if matches!(list.as_slice(), [only] if matches!(&only.kind, js::ExprKind::Var(n) if n == "iterator")) => {
                (**callee).clone()
            }
            _ => Expr::arrow(
                vec!["iterator".into()],
                vec![StmtKind::Return(Some(call)).at(js::Span::NONE)],
            ),
        };
        self.runtime.insert(Helper::Iterator);
        let mut list = vec![value, next];
        if boxed {
            self.runtime.insert(Helper::SomeValue);
            list.push(Expr::bool(true));
        }
        Ok(Expr::call(Expr::var("$iterator"), list))
    }

    /// What an iterator of type `iterator` yields: its `Item`.
    pub(super) fn iterator_item(&self, iterator: ty::Ty<'tcx>) -> Option<ty::Ty<'tcx>> {
        let trait_id = self.tcx.get_diagnostic_item(sym::Iterator)?;
        let item = self
            .tcx
            .associated_item_def_ids(trait_id)
            .iter()
            .copied()
            .find(|&id| self.tcx.item_name(id) == sym::Item)?;
        let projection = ty::Ty::new_projection(self.tcx, ty::IsRigid::No, item, [iterator]);
        self.tcx
            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(projection))
            .ok()
    }

    pub(super) fn iterator_call(
        &mut self,
        known: Std,
        args: &[ExprId],
        generic_args: ty::GenericArgsRef<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        // A search, `(0..n).all(f)`, takes its iterator by `&mut`: a range
        // it's called on is the one borrowed.
        let receiver = match self.thir[self.strip(args[0])].kind {
            ExprKind::Borrow {
                borrow_kind: rustc_middle::mir::BorrowKind::Mut { .. },
                arg,
            } if (self.is_lang_adt(self.reveal(self.thir[arg].ty), LangItem::Range)
                && matches!(self.thir[self.strip(arg)].kind, ExprKind::Adt(_)))
                || self.inclusive_range(arg).is_some() =>
            {
                arg
            }
            _ => args[0],
        };
        let receiver_ty = self.reveal(self.thir[receiver].ty);
        let items = match self.thir[self.strip(receiver)].kind {
            ExprKind::Adt(ref range) if self.is_lang_adt(receiver_ty, LangItem::Range) => {
                let bound = |i: usize| range.fields.iter().find(|f| f.name.as_usize() == i).map(|f| f.expr);
                let (Some(start), Some(end)) = (bound(0), bound(1)) else {
                    unreachable!("a range has a start and an end")
                };
                let big = Num::of(self.thir[start].ty).is_some_and(Num::big);
                let (helper, name) = if big {
                    (Helper::BigRange, "$bigRange")
                } else {
                    (Helper::Range, "$range")
                };
                self.runtime.insert(helper);
                let bounds = self.operands(&[start, end], out)?;
                Expr::call(Expr::var(name), bounds)
            }
            _ if self.is_lang_adt(receiver_ty, LangItem::Range) => {
                return Err(self.unsupported(span, "a range in a variable, as an iterator"));
            }
            // `a..=b`: `$range(a, b + 1)`, exact, and past the type's end.
            _ if let Some((start_id, end_id)) = self.inclusive_range(receiver) => {
                let num = Num::of(self.thir[start_id].ty);
                let big = num.is_some_and(Num::big);
                let (helper, name) = if big {
                    (Helper::BigRange, "$bigRange")
                } else {
                    (Helper::Range, "$range")
                };
                self.runtime.insert(helper);
                let [start, end]: [Expr; 2] = self.operands(&[start_id, end_id], out)?.try_into().ok().unwrap();
                let one = if big { Expr::bigint(1) } else { Expr::int(1) };
                Expr::call(Expr::var(name), vec![start, Expr::bin(Op::Add, end, one)])
            }
            _ => {
                let value = self.iter_value(receiver, out)?;
                self.iter_source(value, receiver_ty, span)?
            }
        };
        // A JS iterator's helpers are lazy: `map`, `filter`, `take`, `drop`,
        // and those that stop early, like `find`. Anything else takes all of
        // it, as an array (ADR 0055).
        let lazy = self.is_lazy_iter(receiver_ty);
        if lazy && known == Std::Rev {
            return Err(self.unsupported(span, "`rev` of an iterator of the crate's own"));
        }
        if let Std::IterComb(comb) = known {
            let rest = self.operands(&args[1..], out)?.into_iter();
            return self.iter_comb(comb, items, rest, generic_args, receiver_ty, lazy, span, out);
        }
        let items = match known {
            _ if !lazy => items,
            Std::ArrayMethod(_)
            | Std::Enumerate
            | Std::Fold
            | Std::Sum
            | Std::Skip
            | Std::Take
            | Std::Cloned
            | Std::Fuse => items,
            _ => Expr::call(Expr::member(items, "toArray"), vec![]),
        };
        let mut rest = self.operands(&args[1..], out)?.into_iter();
        let mut next = || rest.next().expect("rustc checked the arguments");
        let method = |items: Expr, name: &str, list: Vec<Expr>| Expr::call(Expr::member(items, name), list);
        let (a, b) = (Expr::var("a"), Expr::var("b"));
        Ok(match known {
            Std::ArrayMethod(name) => method(items, name, vec![next()]),
            Std::Enumerate => {
                let pair = Expr::array(vec![Expr::var("i"), Expr::var("x")]);
                let js_span = self.js_span(span);
                method(
                    items,
                    "map",
                    vec![Expr::arrow(
                        vec!["x".into(), "i".into()],
                        vec![StmtKind::Return(Some(pair)).at(js_span)],
                    )],
                )
            }
            Std::Rev => method(items, "toReversed", vec![]),
            Std::Skip if lazy => method(items, "drop", vec![next()]),
            Std::Take if lazy => method(items, "take", vec![next()]),
            Std::Skip => method(items, "slice", vec![next()]),
            Std::Take => method(items, "slice", vec![Expr::int(0), next()]),
            Std::Fold => {
                let (init, f) = (next(), next());
                method(items, "reduce", vec![f, init])
            }
            Std::Sum => {
                let ty = generic_args.types().nth(1).expect("`sum` names what it sums to");
                let num = self.num(ty, span)?;
                let js_span = self.js_span(span);
                let add = num.wrap(Expr::bin(Op::Add, a, b));
                let f = Expr::arrow(
                    vec!["a".into(), "b".into()],
                    vec![StmtKind::Return(Some(add)).at(js_span)],
                );
                // Rust's floating Sum starts at -0.0, preserving the sign of
                // an empty sum and of a sequence containing only negative zero.
                let zero = if num.float() { Expr::num(-0.0) } else { num.literal(0) };
                method(items, "reduce", vec![f, zero])
            }
            // `Array.from(s).join("")` is `s`.
            Std::CollectString => match items.kind {
                js::ExprKind::Call(ref callee, ref args)
                    if matches!(&callee.kind, js::ExprKind::Member(object, name)
                        if name == "from" && matches!(&object.kind, js::ExprKind::Var(v) if v == "Array"))
                        && args.len() == 1 =>
                {
                    args[0].clone()
                }
                _ => method(items, "join", vec![Expr::str("")]),
            },
            // A new `Vec`: an adapter's result is a new array already, and the
            // array an iterator started from is copied, so changing one of
            // them doesn't change the other.
            Std::Collect => {
                let fresh = match &items.kind {
                    js::ExprKind::Call(callee, _) => match &callee.kind {
                        js::ExprKind::Member(_, name) => [
                            "map",
                            "filter",
                            "slice",
                            "toReversed",
                            "split",
                            "from",
                            "toArray",
                            "flatMap",
                            "flat",
                            "concat",
                        ]
                        .contains(&name.as_str()),
                        js::ExprKind::Var(name) => [
                            "$range",
                            "$bigRange",
                            "$zip",
                            "$takeWhile",
                            "$skipWhile",
                            "$windows",
                            "$chunks",
                            "$splitBy",
                            "$lines",
                            "$slice",
                            "$rest",
                            "$scan",
                            "$drain",
                            "$splitOff",
                        ]
                        .contains(&name.as_str()),
                        _ => false,
                    },
                    _ => false,
                };
                let items = if fresh { items } else { method(items, "slice", vec![]) };
                // Into a `BinaryHeap`: put in heap order, as `BinaryHeap::from` does.
                match generic_args.types().nth(1) {
                    Some(target) if self.is_std_adt(target, Symbol::intern("BinaryHeap")) => {
                        let item = target.walk().nth(1).and_then(|a| a.as_type()).expect("a heap's item");
                        self.heap_of(item, span)?;
                        let compare = self.cmp_fn(item, false, span)?;
                        self.runtime.insert(Helper::HeapFrom);
                        Expr::call(Expr::var("$heapFrom"), vec![items, compare])
                    }
                    _ => items,
                }
            }
            Std::Position => {
                self.runtime.insert(Helper::Position);
                Expr::call(Expr::var("$position"), vec![items, next()])
            }
            Std::Extreme(max) => {
                let item = generic_args.types().next().and_then(|i| self.iterator_item(i));
                match item {
                    // Of what JS's `<` doesn't order: with its `cmp` (ADR 0057).
                    Some(item) if !self.is_primitive_ord(item) => {
                        let compare = self.cmp_fn(item, false, span)?;
                        self.runtime.insert(if max { Helper::MaxBy } else { Helper::MinBy });
                        let mut list = vec![items, compare];
                        if self.boxed_payload(item) {
                            self.runtime.insert(Helper::Some);
                            list.push(Expr::bool(true));
                        }
                        Expr::call(Expr::var(if max { "$maxBy" } else { "$minBy" }), list)
                    }
                    _ => {
                        self.runtime.insert(if max { Helper::Max } else { Helper::Min });
                        Expr::call(Expr::var(if max { "$max" } else { "$min" }), vec![items])
                    }
                }
            }
            Std::Last => method(items, "at", vec![Expr::int(-1)]),
            Std::Cloned => {
                let item = generic_args.types().nth(1).expect("`cloned` names its item");
                if self.needs_clone(item) {
                    method(items, "map", vec![self.clone_fn("item", item, span)?])
                } else {
                    items
                }
            }
            Std::Fuse => items,
            // Sorting, in place (ADR 0036). JS's `sort()` compares as strings:
            // right for strings and `bool`s, and numbers need `a - b`.
            Std::Sort => {
                let elem = match receiver_ty.peel_refs().kind() {
                    ty::Slice(t) | ty::Array(t, _) => *t,
                    _ => return Err(self.unsupported(span, "sorting this")),
                };
                // A comparator's answer is a number, so a BigInt's is its `cmp`.
                if Num::of(elem).is_some_and(|n| !n.big()) {
                    let js_span = self.js_span(span);
                    let f = Expr::arrow(
                        vec!["a".into(), "b".into()],
                        vec![StmtKind::Return(Some(Expr::bin(Op::Sub, a, b))).at(js_span)],
                    );
                    method(items, "sort", vec![f])
                } else if self.is_string_like(elem) || elem.is_bool() {
                    method(items, "sort", vec![])
                } else {
                    // By its `cmp`: JS's `sort` is stable too (ADR 0057).
                    method(items, "sort", vec![self.cmp_fn(elem, false, span)?])
                }
            }
            Std::SortByKey => {
                let key = next();
                let key = if matches!(key.kind, js::ExprKind::Var(_)) {
                    key
                } else {
                    self.spill("key", key, out)
                };
                let js_span = self.js_span(span);
                // The keys' `cmp`, which is `$cmp` for what JS orders.
                let key_ty = generic_args.types().nth(1).expect("`sort_by_key` names its key");
                let mut body = Vec::new();
                let compare = self.cmp_value(
                    Expr::call(key.clone(), vec![a]),
                    Expr::call(key, vec![b]),
                    key_ty,
                    false,
                    span,
                    &mut body,
                )?;
                // A key read more than once, like a tuple's, is a `const` first.
                body.push(StmtKind::Return(Some(compare)).at(js_span));
                let f = Expr::arrow(vec!["a".into(), "b".into()], body);
                method(items, "sort", vec![f])
            }
            _ => unreachable!("not an iterator's method"),
        })
    }
}
