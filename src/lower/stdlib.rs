//! Recognize supported standard-library operations and translate their behavior.

use super::representation::Num;
use super::{FnCx, R, Shape, is_fieldless_enum};
use crate::js;
use crate::js::{Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_middle::mir::{BinOp, UnOp};
use rustc_middle::thir::{ExprId, ExprKind};
use rustc_middle::ty;
use rustc_middle::ty::Ty;
use rustc_span::def_id::DefId;
use rustc_span::{ErrorGuaranteed, Span, Symbol, sym};

/// The std functions whose JS meaning rust-js knows (ADRs 0023, 0025).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Std {
    /// The argument itself: `Box::new(x)`, `Rc::new(x)`, `rc.clone()`,
    /// `s.to_owned()`, `String::from(s)`, `v.iter()`, and `Deref` of
    /// `String`, `Rc`, `Vec`, `Ref`, `RefMut` and JS objects.
    Same,
    /// `Cell::new(x)` and `RefCell::new(x)`: `{ value: x }`.
    CellNew,
    CellGet,
    CellSet,
    /// `RefCell::borrow`, `borrow_mut`: the cell's `value`.
    Borrow,
    ToString,
    /// `String + &str`.
    Concat,
    /// `==` (true) or `!=` (false) on strings and fieldless enums.
    Eq(bool),
    StringNew,
    Trim,
    /// `is_empty` on a string or a `Vec`: `x.length === 0`.
    IsEmpty,
    VecNew,
    /// `vec![a, b]`.
    VecMacro,
    Push,
    Len,
    Clear,
    Retain,
    /// `==` (true) or `!=` (false) on structs, tuples, arrays and `Vec`s.
    StructEq(bool),
    /// `panic!("..")`, `assert!(..)`: `throw new Error(..)`.
    Panic,
    /// `panic!("{}", x)`: the same, with a formatted message.
    PanicFmt,
    /// What `assert_eq!` and `assert_ne!` call when they fail.
    AssertFailed,
    /// `format_args!("..")` with no placeholders: the string.
    FmtStr,
    /// `format_args!("{} {:?}", ..)`: a template and its arguments.
    FmtNew,
    /// An argument for `{}`.
    FmtDisplay,
    /// An argument for `{:?}`.
    FmtDebug,
    /// `Option` (ADR 0030): `o != null`, `o == null`.
    IsSome,
    IsNone,
    /// `unwrap()` and `expect(msg)`: `$unwrap(o)`, `$unwrap(o, msg)`.
    Unwrap,
    /// `unwrap_or(d)`: `o ?? d`.
    UnwrapOr,
    /// `==` (true) or `!=` (false) on options of strings, numbers and the
    /// like: `==`, so that `null` and `undefined` are both `None`.
    LooseEq(bool),
    /// A string method that is a JS one (ADR 0034): `s.starts_with(p)` is
    /// `s.startsWith(p)`. Also `join` on a slice of strings.
    Method(&'static str),
    /// `strip_prefix` and `strip_suffix`: an option (ADR 0030).
    StripPrefix,
    StripSuffix,
    /// `split_once` and `rsplit_once`: an option of the two sides.
    SplitOnce,
    RsplitOnce,
    /// `s.push_str(t)` and `s.push(c)`: `s = s + t`.
    PushStr,
    /// `.last()` of a `split`: `.at(-1)`.
    Last,
    /// A slice's `first()` and `last()`: `v[0]` and `v.at(-1)`.
    First,
    SliceLast,
    /// `Result` (ADR 0035): `r.TAG === "Ok"` (true) or `"Err"` (false).
    IsOk(bool),
    /// `r.ok()`: the value, or `undefined`.
    ResultOk,
    /// `r.unwrap()`, `r.expect(msg)`: `$unwrapOk(r)`.
    UnwrapOk,
    /// `r.unwrap_or(d)`.
    ResultOr,
    /// An iterator's adapter or consumer that is the array's method (ADR 0036):
    /// `map`, `filter`, `any` (`some`), `all` (`every`), `find`, `for_each`.
    ArrayMethod(&'static str),
    Enumerate,
    Rev,
    Skip,
    Take,
    Fold,
    Sum,
    CollectString,
    /// `collect::<Vec<_>>()`: a new array, unless it's one already.
    Collect,
    Position,
    /// `max()` (true) or `min()` (false) of an iterator: an option.
    Extreme(bool),
    Chars,
    ToVec,
    Sort,
    SortBy,
    SortByKey,
    /// `a.cmp(&b)`: -1, 0 or 1 (ADR 0036).
    Cmp,
    /// `a.max(b)` (true) or `a.min(b)` (false) of two numbers.
    MaxOf(bool),
    /// An operator on references to numbers, `x % 10` with `x: &i32`,
    /// which rustc writes as a call of the operator's trait.
    Operator(BinOp),
    /// `-x` or `!b` of a reference to a number or a `bool`, likewise.
    UnaryOperator(UnOp),
    /// A thread-local's `with(f)`: `f(key)`; and `with_borrow(f)`,
    /// `with_borrow_mut(f)` of a `RefCell` one: `f(key.value)`.
    LocalWith,
    LocalBorrow,
    /// `Ordering::then`, `then_with`, `reverse`.
    Then,
    ThenWith,
    Reverse,
}

impl Std {
    /// Does it take an iterator, and so a range as an array?
    pub(super) fn takes_iterator(self) -> bool {
        matches!(
            self,
            Std::ArrayMethod(_)
                | Std::Enumerate
                | Std::Rev
                | Std::Skip
                | Std::Take
                | Std::Fold
                | Std::Sum
                | Std::CollectString
                | Std::Collect
                | Std::Position
                | Std::Extreme(_)
                | Std::Last
        )
    }
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// Which std function `fun` is, if rust-js knows what it means in JS.
    pub(super) fn std_fn(&self, fun: ExprId) -> Option<Std> {
        let tcx = self.tcx;
        let &ty::FnDef(def_id, args) = self.thir[self.strip(fun)].ty.kind() else {
            return None;
        };
        let diagnostic = |name: &str| tcx.is_diagnostic_item(Symbol::intern(name), def_id);
        let self_ty = args.types().next();
        if diagnostic("box_new") {
            return Some(Std::Same);
        }
        if diagnostic("box_assume_init_into_vec_unsafe") {
            return Some(Std::VecMacro);
        }
        if diagnostic("to_string_method") {
            return Some(Std::ToString);
        }
        if diagnostic("option_unwrap") || diagnostic("option_expect") {
            return Some(Std::Unwrap);
        }
        // `format!(..)` is `must_use(format(format_args!(..)))`, and the
        // arguments are a string already (ADR 0034).
        let krate = tcx.crate_name(def_id.krate);
        let name = tcx.item_name(def_id);
        if (krate == sym::alloc && name.as_str() == "format" && tcx.def_path_str(def_id).ends_with("fmt::format"))
            || (krate == sym::core && name.as_str() == "must_use")
        {
            return Some(Std::Same);
        }
        if tcx.is_lang_item(def_id, LangItem::Panic) {
            return Some(Std::Panic);
        }
        if tcx.is_lang_item(def_id, LangItem::PanicFmt) {
            return Some(Std::PanicFmt);
        }
        if tcx.crate_name(def_id.krate) == sym::core && tcx.item_name(def_id).as_str() == "assert_failed" {
            return Some(Std::AssertFailed);
        }
        if diagnostic("deref_method") || diagnostic("deref_mut_method") {
            // A reference to what's inside is the same JS value (ADR 0024 for JS objects).
            let ty = self_ty?;
            let same = self.is_string_like(ty)
                || self.is_js_object(ty)
                || [sym::Rc, sym::Vec].into_iter().any(|name| self.is_std_adt(ty, name))
                || ["RefCellRef", "RefCellRefMut"]
                    .into_iter()
                    .any(|name| self.is_std_adt(ty, Symbol::intern(name)));
            return same.then_some(Std::Same);
        }
        if let Some(trait_) = tcx.trait_of_assoc(def_id) {
            let ty = self_ty?;
            if Num::of(ty.peel_refs()).is_some() || ty.peel_refs().is_bool() {
                let operators = [(LangItem::Neg, UnOp::Neg), (LangItem::Not, UnOp::Not)];
                if let Some(&(_, op)) = operators.iter().find(|(item, _)| tcx.is_lang_item(trait_, *item)) {
                    return Some(Std::UnaryOperator(op));
                }
            }
            if Num::of(ty.peel_refs()).is_some() {
                let operators = [
                    (LangItem::Add, BinOp::Add),
                    (LangItem::Sub, BinOp::Sub),
                    (LangItem::Mul, BinOp::Mul),
                    (LangItem::Div, BinOp::Div),
                    (LangItem::Rem, BinOp::Rem),
                ];
                if let Some(&(_, op)) = operators.iter().find(|(item, _)| tcx.is_lang_item(trait_, *item)) {
                    return Some(Std::Operator(op));
                }
                if tcx.is_lang_item(trait_, LangItem::PartialOrd) {
                    return Some(Std::Operator(match tcx.item_name(def_id).as_str() {
                        "lt" => BinOp::Lt,
                        "le" => BinOp::Le,
                        "gt" => BinOp::Gt,
                        "ge" => BinOp::Ge,
                        _ => return None,
                    }));
                }
            }
            if tcx.is_lang_item(trait_, LangItem::Add) {
                return self.is_lang_adt(ty, LangItem::String).then_some(Std::Concat);
            }
            if tcx.is_lang_item(trait_, LangItem::PartialEq) {
                let simple = self.is_string_like(ty)
                    || Num::of(ty.peel_refs()).is_some()
                    || ty.peel_refs().is_bool()
                    || matches!(ty.peel_refs().kind(), ty::Adt(adt, _) if is_fieldless_enum(*adt));
                let eq = match tcx.item_name(def_id).as_str() {
                    "eq" => true,
                    "ne" => false,
                    _ => return None,
                };
                if simple {
                    return Some(Std::Eq(eq));
                }
                if let Some(inner) = self.option_of(ty) {
                    let simple = self.is_string_like(inner)
                        || inner.is_bool()
                        || Num::of(inner).is_some()
                        || matches!(inner.kind(), ty::Adt(adt, _) if is_fieldless_enum(*adt));
                    return if simple {
                        Some(Std::LooseEq(eq))
                    } else {
                        self.is_structural_eq(trait_, inner).then_some(Std::StructEq(eq))
                    };
                }
                return self.is_structural_eq(trait_, ty).then_some(Std::StructEq(eq));
            }
            // An iterator is a JS array (ADR 0036), and a `split` one of strings
            // (ADR 0034). Its adapters are the array's methods.
            if tcx.is_diagnostic_item(sym::Iterator, trait_) {
                let collects_string = || {
                    args.types()
                        .nth(1)
                        .is_some_and(|b| self.is_lang_adt(b, LangItem::String))
                };
                return Some(match tcx.item_name(def_id).as_str() {
                    "map" => Std::ArrayMethod("map"),
                    "filter" => Std::ArrayMethod("filter"),
                    "any" => Std::ArrayMethod("some"),
                    "all" => Std::ArrayMethod("every"),
                    "find" => Std::ArrayMethod("find"),
                    "for_each" => Std::ArrayMethod("forEach"),
                    "enumerate" => Std::Enumerate,
                    "rev" => Std::Rev,
                    "skip" => Std::Skip,
                    "take" => Std::Take,
                    "fold" => Std::Fold,
                    "sum" => Std::Sum,
                    "position" => Std::Position,
                    "max" => Std::Extreme(true),
                    "min" => Std::Extreme(false),
                    "last" => Std::Last,
                    "count" => Std::Len,
                    "copied" | "cloned" => Std::Same,
                    "collect" if collects_string() => Std::CollectString,
                    "collect" => Std::Collect,
                    _ => return None,
                });
            }
            if tcx.is_diagnostic_item(sym::IntoIterator, trait_)
                && tcx.item_name(def_id).as_str() == "into_iter"
                && (ty.peel_refs().is_array() || ty.peel_refs().is_slice() || self.is_std_adt(ty.peel_refs(), sym::Vec))
            {
                return Some(Std::Same);
            }
            // `cmp`, `max` and `min` of what JS's `<` orders the same way.
            if tcx.is_diagnostic_item(sym::Ord, trait_) {
                let peeled = ty.peel_refs();
                let comparable = Num::of(peeled).is_some() || peeled.is_bool() || self.is_string_like(peeled);
                return match tcx.item_name(def_id).as_str() {
                    "cmp" if comparable => Some(Std::Cmp),
                    "max" if Num::of(peeled).is_some() => Some(Std::MaxOf(true)),
                    "min" if Num::of(peeled).is_some() => Some(Std::MaxOf(false)),
                    _ => None,
                };
            }
            let from_str = tcx.is_diagnostic_item(sym::From, trait_) && self.is_lang_adt(ty, LangItem::String);
            let to_owned = tcx.is_diagnostic_item(Symbol::intern("ToOwned"), trait_) && ty.is_str();
            // A clone of what's never changed in place can be the value itself:
            // nothing can tell them apart.
            let rc_clone = tcx.is_lang_item(def_id, LangItem::CloneFn)
                && (self.is_std_adt(ty, sym::Rc) || self.is_string_like(ty) || !self.contains_mutated(ty.peel_refs()));
            return (from_str || to_owned || rc_clone).then_some(Std::Same);
        }
        let owner = tcx.type_of(tcx.inherent_impl_of_assoc(def_id)?).instantiate_identity();
        let adt = |name: &str| self.is_std_adt(owner, Symbol::intern(name));
        let string = self.is_lang_adt(owner, LangItem::String);
        let option = self.is_lang_adt(owner, LangItem::Option);
        let result = self.is_std_adt(owner, sym::Result);
        let ordering = self.is_lang_adt(owner, LangItem::OrderingEnum);
        let local_key = adt("LocalKey");
        let arguments = self.is_lang_adt(owner, LangItem::FormatArguments);
        let argument = self.is_lang_adt(owner, LangItem::FormatArgument);
        Some(match tcx.item_name(def_id).as_str() {
            "from_str" | "from_str_nonconst" if arguments => Std::FmtStr,
            "new" if arguments => Std::FmtNew,
            "new_display" if argument => Std::FmtDisplay,
            "new_debug" if argument => Std::FmtDebug,
            "new" if adt("Rc") => Std::Same,
            "new" if adt("Cell") || adt("RefCell") => Std::CellNew,
            "get" if adt("Cell") => Std::CellGet,
            "set" if adt("Cell") => Std::CellSet,
            "borrow" | "borrow_mut" if adt("RefCell") => Std::Borrow,
            "new" if adt("Vec") => Std::VecNew,
            "push" if adt("Vec") => Std::Push,
            // JS's `pop()` gives `undefined` when empty: `None` (ADR 0030).
            "pop" if adt("Vec") => Std::Method("pop"),
            "len" if adt("Vec") || owner.is_slice() => Std::Len,
            "clear" if adt("Vec") => Std::Clear,
            "retain" if adt("Vec") => Std::Retain,
            "iter" | "iter_mut" if owner.is_slice() => Std::Same,
            "new" if string => Std::StringNew,
            "as_str" if string => Std::Same,
            "trim" if owner.is_str() => Std::Trim,
            // Methods taking a pattern: only a string or a `char` one.
            "starts_with" | "ends_with" | "contains" | "replace" | "split" | "strip_prefix" | "strip_suffix"
            | "split_once" | "rsplit_once"
                if owner.is_str() && !self_ty.is_some_and(|p| self.is_string_like(p)) =>
            {
                return None;
            }
            "starts_with" if owner.is_str() => Std::Method("startsWith"),
            "ends_with" if owner.is_str() => Std::Method("endsWith"),
            "contains" if owner.is_str() => Std::Method("includes"),
            "replace" if owner.is_str() => Std::Method("replaceAll"),
            "split" if owner.is_str() => Std::Method("split"),
            "strip_prefix" if owner.is_str() => Std::StripPrefix,
            "strip_suffix" if owner.is_str() => Std::StripSuffix,
            "split_once" if owner.is_str() => Std::SplitOnce,
            "rsplit_once" if owner.is_str() => Std::RsplitOnce,
            "to_uppercase" if owner.is_str() => Std::Method("toUpperCase"),
            "to_lowercase" if owner.is_str() => Std::Method("toLowerCase"),
            "trim_start" if owner.is_str() => Std::Method("trimStart"),
            "trim_end" if owner.is_str() => Std::Method("trimEnd"),
            "repeat" if owner.is_str() => Std::Method("repeat"),
            "join" if owner.is_slice() => Std::Method("join"),
            "push_str" | "push" if string => Std::PushStr,
            "is_empty" if adt("Vec") || owner.is_slice() || owner.is_str() || string => Std::IsEmpty,
            "is_some" if option => Std::IsSome,
            "is_none" if option => Std::IsNone,
            "unwrap_or" if option => Std::UnwrapOr,
            // A thread-local (ADR 0037) is its `Cell` or `RefCell`: `{ value }`.
            "with" if local_key => Std::LocalWith,
            "get" if local_key => Std::CellGet,
            "set" if local_key => Std::CellSet,
            "with_borrow" | "with_borrow_mut" if local_key => Std::LocalBorrow,
            "then" if ordering => Std::Then,
            "then_with" if ordering => Std::ThenWith,
            "reverse" if ordering => Std::Reverse,
            "chars" if owner.is_str() => Std::Chars,
            "to_vec" if owner.is_slice() => Std::ToVec,
            "sort" | "sort_unstable" if owner.is_slice() => Std::Sort,
            "sort_by" | "sort_unstable_by" if owner.is_slice() => Std::SortBy,
            "sort_by_key" | "sort_unstable_by_key" if owner.is_slice() => Std::SortByKey,
            "reverse" if owner.is_slice() => Std::Method("reverse"),
            // `v[0]` and `v.at(-1)` are `undefined` when `v` is empty: `None`.
            "first" if owner.is_slice() => Std::First,
            "last" if owner.is_slice() => Std::SliceLast,
            // `includes` compares strings and numbers by value, as `==` does,
            // but objects by identity: only for those.
            "contains"
                if owner.is_slice()
                    && self_ty.is_some_and(|t| self.is_string_like(t) || Num::of(t).is_some() || t.is_bool()) =>
            {
                Std::Method("includes")
            }
            "is_ok" if result => Std::IsOk(true),
            "is_err" if result => Std::IsOk(false),
            "ok" if result => Std::ResultOk,
            "unwrap" | "expect" if result => Std::UnwrapOk,
            "unwrap_or" if result => Std::ResultOr,
            _ => return None,
        })
    }

    /// Does `==` on `ty` compare field by field or element by element? True
    /// for tuples, arrays, slices and `Vec`s, and structs with a derived `PartialEq`.
    pub(super) fn is_structural_eq(&self, partial_eq: DefId, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        if ty.is_array() || ty.is_slice() || matches!(ty.kind(), ty::Tuple(_)) || self.is_std_adt(ty, sym::Vec) {
            return true;
        }
        let mut derived = false;
        self.tcx
            .for_each_relevant_impl(partial_eq, ty, |imp| derived |= self.tcx.is_automatically_derived(imp));
        derived
            && (matches!(self.shape(ty), Shape::Object(_) | Shape::Array(_))
                || matches!(ty.kind(), ty::Adt(adt, _) if adt.is_enum()))
    }

    /// A `format_args!` template, decoded (its encoding is documented in
    /// core's `fmt::Arguments`): literal pieces prefixed by their length, and
    /// a byte with the top two bits set for each placeholder. `items` holds
    /// the arguments, already made into strings.
    pub(super) fn format(&self, template: &[u8], items: Expr, span: Span) -> R<Expr> {
        let bad = |what: &str| self.unsupported(span, what);
        let byte = |i: usize| template.get(i).copied().ok_or_else(|| bad("this format string"));
        let u16_at = |i: usize| Ok::<usize, ErrorGuaranteed>(u16::from_le_bytes([byte(i)?, byte(i + 1)?]) as usize);
        let piece = |from: usize, len: usize| {
            let bytes = template
                .get(from..from + len)
                .ok_or_else(|| bad("this format string"))?;
            Ok::<Expr, ErrorGuaranteed>(Expr::str(String::from_utf8_lossy(bytes)))
        };
        let (mut parts, mut i, mut next) = (Vec::new(), 0, 0);
        loop {
            let b = byte(i)?;
            i += 1;
            match b {
                0 => break,
                1..=0x7f => {
                    parts.push(piece(i, b as usize)?);
                    i += b as usize;
                }
                0x80 => {
                    let len = u16_at(i)?;
                    parts.push(piece(i + 2, len)?);
                    i += 2 + len;
                }
                _ if b & 0xc0 == 0xc0 => {
                    // Flags, width or precision (`{:>8}`, `{:.2}`, `{:#?}`).
                    if b & 0b111 != 0 {
                        return Err(bad("formatting options like width and precision"));
                    }
                    let index = if b & 0b1000 != 0 {
                        let k = u16_at(i)?;
                        i += 2;
                        k
                    } else {
                        next
                    };
                    next = index + 1;
                    // The values are usually written out, `[a, String(b)]`, with no
                    // effects beyond the ones `format_args!` put in `const`s.
                    parts.push(match &items.kind {
                        js::ExprKind::Array(values) => values[index].clone(),
                        _ => Expr::index(items.clone(), Expr::int(index as i128)),
                    });
                }
                _ => return Err(bad("this format string")),
            }
        }
        Ok(parts
            .into_iter()
            .reduce(|a, b| Expr::bin(Op::Add, a, b))
            .unwrap_or_else(|| Expr::str("")))
    }

    /// An iterator's method (ADR 0036). The iterator is a JS array: a range
    /// becomes one, `$range(a, b)`, and the rest already are.
    pub(super) fn iterator_call(
        &mut self,
        known: Std,
        args: &[ExprId],
        generic_args: ty::GenericArgsRef<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let receiver_ty = self.thir[args[0]].ty;
        let items = match self.thir[self.strip(args[0])].kind {
            ExprKind::Adt(ref range) if self.is_lang_adt(receiver_ty, LangItem::Range) => {
                let bound = |i: usize| range.fields.iter().find(|f| f.name.as_usize() == i).map(|f| f.expr);
                let (Some(start), Some(end)) = (bound(0), bound(1)) else {
                    unreachable!("a range has a start and an end")
                };
                self.runtime.insert(Helper::Range);
                let bounds = self.operands(&[start, end], out)?;
                Expr::call(Expr::var("$range"), bounds)
            }
            _ if self.is_lang_adt(receiver_ty, LangItem::Range) => {
                return Err(self.unsupported(span, "a range in a variable, as an iterator"));
            }
            _ => self.expr(args[0], out)?,
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
                method(items, "reduce", vec![f, Expr::int(0)])
            }
            Std::CollectString => method(items, "join", vec![Expr::str("")]),
            // A new `Vec`: an adapter's result is a new array already, and the
            // array an iterator started from is copied, so changing one of
            // them doesn't change the other.
            Std::Collect => {
                let fresh = match &items.kind {
                    js::ExprKind::Call(callee, _) => match &callee.kind {
                        js::ExprKind::Member(_, name) => {
                            ["map", "filter", "slice", "toReversed", "split", "from"].contains(&name.as_str())
                        }
                        js::ExprKind::Var(name) => name == "$range",
                        _ => false,
                    },
                    _ => false,
                };
                if fresh { items } else { method(items, "slice", vec![]) }
            }
            Std::Position => {
                self.runtime.insert(Helper::Position);
                Expr::call(Expr::var("$position"), vec![items, next()])
            }
            Std::Extreme(max) => {
                self.runtime.insert(if max { Helper::Max } else { Helper::Min });
                Expr::call(Expr::var(if max { "$max" } else { "$min" }), vec![items])
            }
            Std::Last => method(items, "at", vec![Expr::int(-1)]),
            // Sorting, in place (ADR 0036). JS's `sort()` compares as strings:
            // right for strings and `bool`s, and numbers need `a - b`.
            Std::Sort => {
                let elem = match receiver_ty.peel_refs().kind() {
                    ty::Slice(t) | ty::Array(t, _) => *t,
                    _ => return Err(self.unsupported(span, "sorting this")),
                };
                if Num::of(elem).is_some() {
                    let js_span = self.js_span(span);
                    let f = Expr::arrow(
                        vec!["a".into(), "b".into()],
                        vec![StmtKind::Return(Some(Expr::bin(Op::Sub, a, b))).at(js_span)],
                    );
                    method(items, "sort", vec![f])
                } else if self.is_string_like(elem) || elem.is_bool() {
                    method(items, "sort", vec![])
                } else {
                    return Err(self.unsupported(span, &format!("sorting `{elem}`s")));
                }
            }
            Std::SortByKey => {
                self.runtime.insert(Helper::Cmp);
                let key = next();
                let key = if matches!(key.kind, js::ExprKind::Var(_)) {
                    key
                } else {
                    self.spill("key", key, out)
                };
                let js_span = self.js_span(span);
                let compare = Expr::call(
                    Expr::var("$cmp"),
                    vec![Expr::call(key.clone(), vec![a]), Expr::call(key, vec![b])],
                );
                let f = Expr::arrow(
                    vec!["a".into(), "b".into()],
                    vec![StmtKind::Return(Some(compare)).at(js_span)],
                );
                method(items, "sort", vec![f])
            }
            _ => unreachable!("not an iterator's method"),
        })
    }
}
