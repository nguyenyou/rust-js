//! Read-only library-operation recognition. No function lowering state or JS emission.

mod methods;

use super::combinators::{Comb, HeapOp, IterComb, StepOp};
use super::format_spec::Radix;
use super::maps::{MapOp, Part};
use super::numbers::NumOp;
use super::representation::Num;
use super::text::TextOp;
use rustc_hir::def::DefKind;
use rustc_hir::{self as hir, LangItem, intravisit};
use rustc_middle::mir::{BinOp, UnOp};
use rustc_middle::traits::ImplSource;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::{DefId, LocalDefId};
use rustc_span::hygiene::{ExpnKind, MacroKind};
use rustc_span::{Symbol, sym};

/// Only immutable analysis inputs: recognition cannot record dependencies,
/// allocate names, register helpers, or lower an expression.
pub(super) struct Recognition<'a, 'tcx> {
    pub tcx: TyCtxt<'tcx>,
    pub typing_env: ty::TypingEnv<'tcx>,
    pub trait_impls: &'a [DefId],
}

/// The std functions whose JS meaning rust-js knows (ADRs 0023, 0025).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Std {
    /// The argument itself: `Box::new(x)`, `Rc::new(x)`,
    /// `s.to_owned()`, `String::from(s)`, `v.iter()`, and `Deref` of
    /// `String`, `Rc`, `Vec`, `Ref`, `RefMut` and JS objects.
    Same,
    /// `u64::from(x)` of a number that isn't a BigInt: `BigInt(x)` (ADR 0086).
    ToBig,
    /// `u8::try_from(x)` (`into: false`) or `x.try_into()` between
    /// integers: `Ok` of it in range, or a `TryFromIntError`.
    TryFromInt {
        into: bool,
    },
    /// An iterator's `cloned()` and `copied()`: its items, each cloned if
    /// that could be told apart from sharing it (ADR 0052).
    Cloned,
    /// `v[i]` of a `Vec`, `Index::index` or `IndexMut::index_mut`: `$index(v, i)`.
    Index,
    /// `Cell::new(x)` and `RefCell::new(x)`: `{ value: x }`.
    CellNew,
    CellGet,
    CellSet,
    /// `RefCell::borrow`, `borrow_mut`: the cell's `value`.
    Borrow,
    ToString,
    /// `String + &str`.
    Concat,
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
    /// `Argument::new_lower_hex` and the like: `{:x}` (ADR 0058).
    FmtRadix(Radix),
    /// `{:e}` (false) and `{:E}`: exponent notation.
    FmtExp(bool),
    /// `Argument::from_usize`: a width or precision from an argument, `{:>w$}`.
    FmtUsize,
    /// A `HashMap` or `HashSet` method (ADR 0059).
    Map(MapOp),
    /// A `char` or `str` method, `parse`, or slicing by a range (ADR 0063).
    Text(TextOp),
    Number(NumOp),
    /// A `BinaryHeap`'s own methods (ADR 0068).
    Heap(HeapOp),
    /// `serde_json::to_string(&v)` (false) and `to_string_pretty` (ADR 0077).
    ToJson(bool),
    /// `serde_json::from_str::<T>(s)` (ADR 0078).
    FromJson,
    /// `it.next()`, `peekable()`, `peek()` and the like (ADR 0071).
    Step(StepOp),
    /// `VecDeque::remove(i)`: an `Option`, where `Vec`'s panics.
    DequeRemove,
    /// `vec![x; n]`.
    FromElem,
    /// An `Option`, `Result` or `Vec` method (ADR 0062).
    Comb(Comb),
    /// An iterator adapter or consumer (ADR 0062).
    IterComb(IterComb),
    /// `Option` (ADR 0030): `o != null`, `o == null`.
    IsSome,
    IsNone,
    /// `unwrap()` and `expect(msg)`: `$unwrap(o)`, `$unwrap(o, msg)`.
    Unwrap,
    /// `unwrap_or(d)`: `o ?? d`.
    UnwrapOr,
    /// `a += b` of numbers where `b` is a reference: `a = a + b`.
    AssignOperator(BinOp),
    /// `copied()` and `cloned()` of an `Option<&T>`: a clone of what's in it.
    OptionCloned,
    /// `map(f)`: `o != null ? f(o) : undefined`, with a closure's body in place.
    OptionMap,
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
    /// `v.get(i)`: `v[i]`, which is `undefined` past the end.
    SliceGet,
    /// `char::from_digit(n, radix)` and `char::from_u32(n)`: `None` when there's no such `char`.
    FromDigit,
    FromU32,
    /// `Result` (ADR 0035): `r.TAG === "Ok"` (true) or `"Err"` (false).
    IsOk(bool),
    /// `r.ok()`: the value, or `undefined`.
    ResultOk,
    /// `r.unwrap()`, `r.expect(msg)`: `$unwrapOk(r)`.
    UnwrapOk,
    /// `r.unwrap_err()`, `r.expect_err(msg)`: `$unwrapErr(r)`.
    UnwrapErr,
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
                | Std::Cloned
                | Std::IterComb(_)
        )
    }
}

/// serde_json's own types, by name.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Json {
    Value,
    Number,
    Map,
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    pub(super) fn classify(&self, def_id: DefId, args: ty::GenericArgsRef<'tcx>) -> Option<Std> {
        let tcx = self.tcx;
        let diagnostic = |name: &str| tcx.is_diagnostic_item(Symbol::intern(name), def_id);
        let self_ty = args.types().next();
        if diagnostic("box_new") {
            return Some(Std::Same);
        }
        if diagnostic("box_assume_init_into_vec_unsafe") {
            return Some(Std::VecMacro);
        }
        // `char::from_digit`, `std::char::from_digit` and `from_u32`.
        let char_fn = |name: &str| {
            tcx.crate_name(def_id.krate) == sym::core
                && tcx.item_name(def_id).as_str() == name
                && tcx.def_path_str(def_id).contains("char")
        };
        if char_fn("from_digit") {
            return Some(Std::FromDigit);
        }
        if char_fn("from_u32") {
            return Some(Std::FromU32);
        }
        if tcx.crate_name(def_id.krate).as_str() == "serde_json" {
            match tcx.item_name(def_id).as_str() {
                "to_string" => return Some(Std::ToJson(false)),
                "to_string_pretty" => return Some(Std::ToJson(true)),
                "from_str" => return Some(Std::FromJson),
                _ => {}
            }
        }
        if diagnostic("vec_from_elem") {
            return Some(Std::FromElem);
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
        if krate == sym::core
            && let Some(imp) = tcx.inherent_impl_of_assoc(def_id)
            && Num::of(tcx.type_of(imp).instantiate_identity()) == Some(Num::F64)
        {
            match name.as_str() {
                "max" => return Some(Std::MaxOf(true)),
                "min" => return Some(Std::MaxOf(false)),
                _ => {}
            }
        }
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
                || self.is_std_adt(ty, sym::Rc)
                || self.is_vec_like(ty)
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
                // `total += x` with a `&u32` `x`: the same assignment as with a `u32`.
                let assigning = [
                    (LangItem::AddAssign, BinOp::Add),
                    (LangItem::SubAssign, BinOp::Sub),
                    (LangItem::MulAssign, BinOp::Mul),
                    (LangItem::DivAssign, BinOp::Div),
                    (LangItem::RemAssign, BinOp::Rem),
                ];
                if let Some(&(_, op)) = assigning.iter().find(|(item, _)| tcx.is_lang_item(trait_, *item)) {
                    return Some(Std::AssignOperator(op));
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
            // `m[k]` of a map: its value, or a panic, as `get(k).expect(..)`.
            if tcx.is_lang_item(trait_, LangItem::Index) && self.is_map(ty) && !self.is_set(ty) {
                return Some(Std::Map(MapOp::Index));
            }
            // `&v[a..b]` of a slice, an array or a `Vec` (ADR 0063).
            if tcx.is_lang_item(trait_, LangItem::Index)
                && let Some(range) = args.types().nth(1)
                && ["Range", "RangeFrom", "RangeTo", "RangeFull"].iter().any(|name| {
                    matches!(range.kind(), ty::Adt(adt, _) if tcx.item_name(adt.did()).as_str() == *name
                        && tcx.crate_name(adt.did().krate) == sym::core)
                })
                && (ty.peel_refs().is_slice() || ty.peel_refs().is_array() || self.is_vec_like(ty.peel_refs()))
            {
                return Some(Std::Text(TextOp::Slice));
            }
            // `v[i]` of a `Vec` is a slice's, checked the same way.
            if (tcx.is_lang_item(trait_, LangItem::Index) || tcx.is_lang_item(trait_, LangItem::IndexMut))
                && self.is_vec_like(ty.peel_refs())
                && args.types().nth(1).is_some_and(|i| i.is_usize())
            {
                return Some(Std::Index);
            }
            if tcx.is_lang_item(trait_, LangItem::Add) {
                return self.is_lang_adt(ty, LangItem::String).then_some(Std::Concat);
            }
            // `s += t` is `s.push_str(t)`.
            if tcx.is_lang_item(trait_, LangItem::AddAssign) && self.is_lang_adt(ty.peel_refs(), LangItem::String) {
                return Some(Std::PushStr);
            }
            // A map's or a set's `into_iter()`: its entries, as an array (ADR 0059).
            // A `for` over one takes the `Map` itself.
            if tcx.is_diagnostic_item(sym::IntoIterator, trait_) && self.is_map(ty) {
                return Some(Std::Map(MapOp::Iter(Part::Entries)));
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
                    // One of the crate's own is its impl's `next` (ADR 0055).
                    "next" if !self.is_user_iterator(ty) => Std::Step(StepOp::Next),
                    "peekable" => Std::Step(StepOp::Peekable),
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
                    name if let Some(comb) = methods::iterator(name) => Std::IterComb(comb),
                    "copied" | "cloned" => Std::Cloned,
                    "collect" if collects_string() => Std::CollectString,
                    "collect" if args.types().nth(1).is_some_and(|b| self.is_map(b)) => {
                        let set = args.types().nth(1).is_some_and(|b| self.is_set(b));
                        Std::Map(MapOp::From { set })
                    }
                    "collect" => Std::Collect,
                    _ => return None,
                });
            }
            if tcx.is_diagnostic_item(sym::IntoIterator, trait_)
                && tcx.item_name(def_id).as_str() == "into_iter"
                && (ty.peel_refs().is_array() || ty.peel_refs().is_slice() || self.is_vec_like(ty.peel_refs()))
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
            // `v.extend(items)` (ADR 0062).
            if is_extend(tcx, trait_) && self.is_vec_like(ty.peel_refs()) {
                return Some(Std::Comb(Comb::Extend));
            }
            // `VecDeque::from(v)` is a copy of `v`, which may be a clone that
            // was never made (ADR 0052); `BinaryHeap::from(v)` puts one in heap order.
            if tcx.is_diagnostic_item(sym::From, trait_) && self.is_std_adt(ty, Symbol::intern("VecDeque")) {
                return Some(Std::ToVec);
            }
            if tcx.is_diagnostic_item(sym::From, trait_) && self.is_std_adt(ty, Symbol::intern("BinaryHeap")) {
                return Some(Std::Heap(HeapOp::From));
            }
            // `HashMap::from([(k, v)])`: `new Map([[k, v]])`.
            if tcx.is_diagnostic_item(sym::From, trait_) && self.is_map(ty) {
                let set = self.is_set(ty);
                return Some(Std::Map(MapOp::From { set }));
            }
            // std's own conversions that change nothing in JS (ADR 0063): to a
            // `String` from a `&str` or a `char`, and between numbers, which
            // only widen.
            let (from_ty, to_ty) = if tcx.is_diagnostic_item(sym::Into, trait_) {
                (Some(ty), args.types().nth(1))
            } else if tcx.is_diagnostic_item(sym::From, trait_) {
                (args.types().nth(1), Some(ty))
            } else {
                (None, None)
            };
            if let (Some(from_ty), Some(to_ty)) = (from_ty, to_ty) {
                if self.is_lang_adt(to_ty, LangItem::String) && self.is_string_like(from_ty) {
                    return Some(Std::Same);
                }
                // Into a BigInt from a number (ADR 0086), else the same.
                if let (Some(from), Some(to)) = (Num::of(from_ty.peel_refs()), Num::of(to_ty)) {
                    return Some(if to.big() && !from.big() { Std::ToBig } else { Std::Same });
                }
            }
            // Between integers, which may not fit.
            let into = tcx.is_diagnostic_item(sym::TryInto, trait_);
            let (from_ty, to_ty) = if into {
                (Some(ty), args.types().nth(1))
            } else if tcx.is_diagnostic_item(sym::TryFrom, trait_) {
                (args.types().nth(1), Some(ty))
            } else {
                (None, None)
            };
            if let (Some(from), Some(to)) = (from_ty.and_then(|t| Num::of(t.peel_refs())), to_ty.and_then(Num::of))
                && from != Num::F64
                && to != Num::F64
            {
                return Some(Std::TryFromInt { into });
            }
            let from_str = tcx.is_diagnostic_item(sym::From, trait_) && self.is_lang_adt(ty, LangItem::String);
            let to_owned = tcx.is_diagnostic_item(Symbol::intern("ToOwned"), trait_) && ty.is_str();
            return (from_str || to_owned).then_some(Std::Same);
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
        let map = adt("HashMap") || adt("BTreeMap") || self.is_json_map(owner);
        let set = adt("HashSet") || adt("BTreeSet");
        let entry = adt("HashMapEntry") || adt("BTreeEntry");
        let name = tcx.item_name(def_id);
        let (deque, heap) = (adt("VecDeque"), adt("BinaryHeap"));
        // Theirs first: `push` and `pop` keep a heap's order, and a deque's
        // `remove` is an `Option` (ADR 0068).
        let peekable = self.is_peekable(owner);
        let chars = matches!(owner.kind(), ty::Adt(adt, _) if tcx.crate_name(adt.did().krate) == sym::core
            && tcx.item_name(adt.did()).as_str() == "Chars");
        let own = match name.as_str() {
            "peek" if peekable => Some(Std::Step(StepOp::Peek)),
            "next_if" if peekable => Some(Std::Step(StepOp::NextIf)),
            "next_if_eq" if peekable => Some(Std::Step(StepOp::NextIfEq)),
            "as_str" if chars => Some(Std::Step(StepOp::AsStr)),
            "push" if heap => Some(Std::Heap(HeapOp::Push)),
            "pop" if heap => Some(Std::Heap(HeapOp::Pop)),
            "peek" if heap => Some(Std::First),
            "into_sorted_vec" if heap => Some(Std::Heap(HeapOp::IntoSorted)),
            "into_vec" if heap => Some(Std::Same),
            "remove" if deque => Some(Std::DequeRemove),
            "push_back" if deque => Some(Std::Push),
            "pop_back" if deque => Some(Std::Method("pop")),
            "push_front" if deque => Some(Std::Method("unshift")),
            "pop_front" if deque => Some(Std::Method("shift")),
            "front" if deque => Some(Std::First),
            "back" if deque => Some(Std::SliceLast),
            "make_contiguous" if deque => Some(Std::Same),
            "drain" if adt("Vec") || deque => Some(Std::Text(TextOp::Drain)),
            "iter" | "iter_mut" if deque || heap => Some(Std::Same),
            "new" | "with_capacity" if deque || heap => Some(Std::VecNew),
            "len" if deque || heap => Some(Std::Len),
            "is_empty" if deque || heap => Some(Std::IsEmpty),
            "clear" if deque || heap => Some(Std::Clear),
            "retain" if deque => Some(Std::Retain),
            _ => None,
        };
        if own.is_some() {
            return own;
        }
        if let Some(op) = methods::text(name.as_str(), owner.is_char(), owner.is_str()) {
            return Some(Std::Text(op));
        }
        if let Some(num) = Num::of(owner)
            && let Some(op) = methods::number(name.as_str(), num)
        {
            return Some(Std::Number(op));
        }
        if let Some(comb) = methods::combinator(
            name.as_str(),
            option,
            result,
            adt("Vec") || deque,
            adt("Vec") || deque || owner.is_slice(),
        )
        .or_else(|| methods::boolean(name.as_str(), owner.is_bool()))
        {
            return Some(Std::Comb(comb));
        }
        Some(match tcx.item_name(def_id).as_str() {
            "new" | "with_capacity" if map || set => Std::Map(MapOp::New { set }),
            "insert" if map => Std::Map(MapOp::Insert),
            "insert" if set => Std::Map(MapOp::Add),
            "get" | "get_mut" if map => Std::Map(MapOp::Get),
            "contains_key" if map => Std::Map(MapOp::Has),
            "contains" if set => Std::Map(MapOp::Has),
            "remove" if map => Std::Map(MapOp::Remove),
            "remove" if set => Std::Map(MapOp::Delete),
            "len" if map || set => Std::Map(MapOp::Len),
            "is_empty" if map || set => Std::Map(MapOp::IsEmpty),
            "iter" | "iter_mut" if map || set => Std::Map(MapOp::Iter(Part::Entries)),
            "keys" if map => Std::Map(MapOp::Iter(Part::Keys)),
            "values" | "values_mut" if map => Std::Map(MapOp::Iter(Part::Values)),
            "entry" if map => Std::Map(MapOp::Entry),
            "or_insert" if entry => Std::Map(MapOp::OrInsert),
            "or_insert_with" if entry => Std::Map(MapOp::OrInsertWith),
            "or_default" if entry => Std::Map(MapOp::OrDefault),
            "from_str" | "from_str_nonconst" if arguments => Std::FmtStr,
            "new" if arguments => Std::FmtNew,
            "new_display" if argument => Std::FmtDisplay,
            "new_debug" if argument => Std::FmtDebug,
            "new_lower_hex" if argument => Std::FmtRadix(Radix::LowerHex),
            "new_upper_hex" if argument => Std::FmtRadix(Radix::UpperHex),
            "new_binary" if argument => Std::FmtRadix(Radix::Binary),
            "new_octal" if argument => Std::FmtRadix(Radix::Octal),
            "new_lower_exp" if argument => Std::FmtExp(false),
            "new_upper_exp" if argument => Std::FmtExp(true),
            "from_usize" if argument => Std::FmtUsize,
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
            // A closure as the pattern (ADR 0063).
            "split" | "contains" if owner.is_str() && self_ty.is_some_and(|p| matches!(p.kind(), ty::Closure(..))) => {
                Std::Text(if name.as_str() == "split" {
                    TextOp::SplitBy
                } else {
                    TextOp::ContainsBy
                })
            }
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
            "copied" | "cloned" if option => Std::OptionCloned,
            "is_none" if option => Std::IsNone,
            "unwrap_or" if option => Std::UnwrapOr,
            "map" if option => Std::OptionMap,
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
            "get" if owner.is_slice() && args.types().nth(1).is_some_and(|i| i.is_usize()) => Std::SliceGet,
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
            "unwrap_err" | "expect_err" if result => Std::UnwrapErr,
            "unwrap_or" if result => Std::ResultOr,
            _ => return None,
        })
    }

    pub(super) fn is_string_like(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        ty.is_str() || ty.is_char() || self.is_lang_adt(ty, LangItem::String)
    }

    pub(super) fn is_std_adt(&self, ty: Ty<'tcx>, name: Symbol) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.is_diagnostic_item(name, adt.did()))
    }

    pub(super) fn is_lang_adt(&self, ty: Ty<'tcx>, item: LangItem) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.is_lang_item(adt.did(), item))
    }

    pub(super) fn is_vec_like(&self, ty: Ty<'tcx>) -> bool {
        self.is_std_adt(ty, sym::Vec)
            || ["VecDeque", "BinaryHeap"]
                .into_iter()
                .any(|name| self.is_std_adt(ty, Symbol::intern(name)))
    }

    pub(super) fn is_js_object(&self, ty: Ty<'tcx>) -> bool {
        let ty::Adt(adt, args) = ty.kind() else { return false };
        if !adt.is_struct() {
            return false;
        }
        // `PhantomData<JsObject>`, then only more markers, for a generic one
        // like `Promise<T>`.
        let mut fields = adt.non_enum_variant().fields.iter().map(|f| f.ty(self.tcx, args));
        let first = fields.next();
        first.is_some_and(|field| {
            matches!(field.kind(), ty::Adt(marker, marked) if marker.is_phantom_data()
            && marked.types().next().is_some_and(|t| matches!(t.kind(), ty::Foreign(_))))
        }) && fields.all(|field| matches!(field.kind(), ty::Adt(marker, _) if marker.is_phantom_data()))
    }

    pub(super) fn is_map(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        ["HashMap", "HashSet", "BTreeMap", "BTreeSet"]
            .into_iter()
            .any(|name| self.is_std_adt(ty, Symbol::intern(name)))
            || self.is_json_map(ty)
    }

    pub(super) fn is_set(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        self.is_std_adt(ty, Symbol::intern("HashSet")) || self.is_std_adt(ty, Symbol::intern("BTreeSet"))
    }

    pub(super) fn is_peekable(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.peel_refs().kind(), ty::Adt(adt, _) if self.tcx.crate_name(adt.did().krate) == rustc_span::sym::core
            && self.tcx.item_name(adt.did()).as_str() == "Peekable")
    }

    pub(super) fn is_user_iterator(&self, ty: ty::Ty<'tcx>) -> bool {
        let iterator = self.tcx.get_diagnostic_item(sym::Iterator).expect("std has `Iterator`");
        matches!(ty.peel_refs().kind(), ty::Adt(..)) && self.has_user_impl(iterator, ty.peel_refs())
    }

    pub(super) fn args_of(&self, trait_id: DefId, ty: Ty<'tcx>) -> ty::GenericArgsRef<'tcx> {
        let ty = self.tcx.erase_and_anonymize_regions(ty);
        self.tcx.mk_args_from_iter(std::iter::repeat_n(
            ty::GenericArg::from(ty),
            self.tcx.generics_of(trait_id).count(),
        ))
    }

    pub(super) fn has_user_impl(&self, trait_id: DefId, ty: Ty<'tcx>) -> bool {
        self.is_user_impl(ty::TraitRef::new_from_args(
            self.tcx,
            trait_id,
            self.args_of(trait_id, ty),
        ))
    }

    pub(super) fn is_user_impl(&self, tr: ty::TraitRef<'tcx>) -> bool {
        let tr = self.tcx.erase_and_anonymize_regions(tr);
        matches!(self.tcx.codegen_select_candidate(self.typing_env.as_query_input(tr)),
            Ok(ImplSource::UserDefined(imp)) if self.trait_impls.contains(&imp.impl_def_id))
    }

    pub(super) fn json_type(&self, ty: Ty<'tcx>) -> Option<Json> {
        let ty::Adt(adt, _) = ty.peel_refs().kind() else {
            return None;
        };
        if self.tcx.crate_name(adt.did().krate).as_str() != "serde_json" {
            return None;
        }
        Some(match self.tcx.item_name(adt.did()).as_str() {
            "Value" => Json::Value,
            "Number" => Json::Number,
            "Map" => Json::Map,
            _ => return None,
        })
    }

    pub(super) fn is_json_map(&self, ty: Ty<'tcx>) -> bool {
        self.json_type(ty) == Some(Json::Map)
    }
}

/// Recognized serde_json calls carry type facts, never lowered operands.
pub(super) enum JsonCall<'tcx> {
    ToValue(Ty<'tcx>),
    FromValue(Ty<'tcx>),
    Index,
    Equal {
        other: Ty<'tcx>,
        value_first: bool,
        negate: bool,
    },
    Convert {
        to: Ty<'tcx>,
        from: Ty<'tcx>,
        json: Json,
    },
    Default,
    Method(JsonMethod),
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    pub(super) fn json_call(&self, def_id: DefId, generic_args: ty::GenericArgsRef<'tcx>) -> Option<JsonCall<'tcx>> {
        let tcx = self.tcx;
        let name = tcx.item_name(def_id);
        if tcx.crate_name(def_id.krate).as_str() == "serde_json" && tcx.trait_of_assoc(def_id).is_none() {
            match name.as_str() {
                "to_value" => {
                    return Some(JsonCall::ToValue(
                        generic_args.types().next().expect("`to_value::<T>`").peel_refs(),
                    ));
                }
                // `serde_json::from_value::<T>(v)`: `T` read from the `Value`.
                "from_value" => {
                    return Some(JsonCall::FromValue(
                        generic_args.types().next().expect("`from_value::<T>`"),
                    ));
                }
                _ => {}
            }
        }
        if let Some(trait_id) = tcx.trait_of_assoc(def_id) {
            let tr = ty::TraitRef::from_assoc(tcx, trait_id, generic_args);
            let (this, other) = (tr.self_ty(), tr.args.types().nth(1));
            let this_value = self.json_type(this) == Some(Json::Value);
            let other_value = other.is_some_and(|o| self.json_type(o) == Some(Json::Value));
            if tcx.is_lang_item(trait_id, LangItem::Index) && this_value {
                return Some(JsonCall::Index);
            }
            if tcx.is_lang_item(trait_id, LangItem::PartialEq)
                && let Some(other) = other
                && this_value != other_value
            {
                return Some(JsonCall::Equal {
                    other: if this_value { other } else { this },
                    value_first: this_value,
                    negate: name.as_str() == "ne",
                });
            }
            let converting = match other {
                Some(other) if tcx.is_diagnostic_item(sym::From, trait_id) => Some((this, other)),
                Some(other) if tcx.is_diagnostic_item(sym::Into, trait_id) => Some((other, this)),
                _ => None,
            };
            if let Some((to, from)) = converting
                && let Some(json) = self.json_type(to)
            {
                return Some(JsonCall::Convert { to, from, json });
            }
            if tcx.is_diagnostic_item(Symbol::intern("Default"), trait_id) && this_value {
                return Some(JsonCall::Default);
            }
            return None;
        }
        let imp = tcx.inherent_impl_of_assoc(def_id)?;
        let owner = tcx.type_of(imp).instantiate_identity();
        match self.json_type(owner)? {
            json @ (Json::Value | Json::Number) => Some(JsonCall::Method(JsonMethod::recognize(json, name))),
            Json::Map => None,
        }
    }
}

/// Inherent Value/Number operations, selected before operand emission.
pub(super) enum JsonMethod {
    NumberFromF64,
    NumberAsF64,
    /// `as_u64()` and `as_i64()`: `"u64"` or `"i64"`, a BigInt (ADR 0086).
    NumberAsInt(&'static str),
    NumberKind(&'static str),
    NumberIsI64,
    IsNull,
    IsTag(&'static str),
    IsNumber(&'static str),
    AsTag(&'static str),
    AsF64,
    AsInt(&'static str),
    Get,
    Unsupported {
        owner: &'static str,
        name: Symbol,
    },
}

impl JsonMethod {
    fn recognize(json: Json, name: Symbol) -> Self {
        if json == Json::Number {
            return match name.as_str() {
                "from_f64" => Self::NumberFromF64,
                "as_f64" => Self::NumberAsF64,
                "as_u64" => Self::NumberAsInt("u64"),
                "as_i64" => Self::NumberAsInt("i64"),
                "is_f64" => Self::NumberKind("f"),
                "is_u64" => Self::NumberKind("u"),
                "is_i64" => Self::NumberIsI64,
                _ => Self::Unsupported { owner: "Number", name },
            };
        }
        match name.as_str() {
            "is_null" => Self::IsNull,
            "is_boolean" => Self::IsTag("Bool"),
            "is_number" => Self::IsTag("Number"),
            "is_string" => Self::IsTag("String"),
            "is_array" => Self::IsTag("Array"),
            "is_object" => Self::IsTag("Object"),
            "is_f64" => Self::IsNumber("f64"),
            "is_u64" => Self::IsNumber("u64"),
            "is_i64" => Self::IsNumber("i64"),
            "as_bool" => Self::AsTag("Bool"),
            "as_str" => Self::AsTag("String"),
            "as_array" | "as_array_mut" => Self::AsTag("Array"),
            "as_object" | "as_object_mut" => Self::AsTag("Object"),
            "as_number" => Self::AsTag("Number"),
            "as_f64" => Self::AsF64,
            "as_u64" => Self::AsInt("u64"),
            "as_i64" => Self::AsInt("i64"),
            "get" | "get_mut" => Self::Get,
            _ => Self::Unsupported { owner: "Value", name },
        }
    }
}

/// The established Value representation for one source type. Container elements
/// are recognized recursively when lowering their conversion functions.
pub(super) enum JsonConversion<'tcx> {
    Tag(&'static str),
    Null,
    Float,
    Integer,
    Same,
    Vector(Ty<'tcx>),
    Option(Ty<'tcx>),
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    /// `T`, for an `Option<T>`.
    pub(super) fn option_of(&self, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
        match ty.kind() {
            ty::Adt(adt, args) if self.tcx.is_lang_item(adt.did(), LangItem::Option) => args.types().next(),
            _ => None,
        }
    }

    pub(super) fn json_conversion(&self, from: Ty<'tcx>) -> Option<JsonConversion<'tcx>> {
        let from = from.peel_refs();
        if self.is_string_like(from) && !from.is_char() {
            return Some(JsonConversion::Tag("String"));
        }
        if from.is_bool() {
            return Some(JsonConversion::Tag("Bool"));
        }
        if from.is_unit() {
            return Some(JsonConversion::Null);
        }
        match Num::of(from) {
            Some(Num::F64) => return Some(JsonConversion::Float),
            Some(_) => return Some(JsonConversion::Integer),
            None => {}
        }
        match self.json_type(from) {
            Some(Json::Map) => return Some(JsonConversion::Tag("Object")),
            Some(Json::Number) => return Some(JsonConversion::Tag("Number")),
            Some(Json::Value) => return Some(JsonConversion::Same),
            None => {}
        }
        if let ty::Adt(_, args) = from.kind()
            && self.is_std_adt(from, sym::Vec)
        {
            return Some(JsonConversion::Vector(args.type_at(0)));
        }
        self.option_of(from).map(JsonConversion::Option)
    }

    /// The scalar comparison category used by serde_json's PartialEq impls.
    pub(super) fn json_comparison(&self, other: Ty<'tcx>) -> Option<&'static str> {
        let other = other.peel_refs();
        if self.is_string_like(other) {
            Some("String")
        } else if other.is_bool() {
            Some("Bool")
        } else {
            match Num::of(other)? {
                Num::F64 => Some("f64"),
                n if n.signed() => Some("i64"),
                _ => Some("u64"),
            }
        }
    }
}

pub(super) enum WriteCall {
    Text,
    Display,
    Debug,
    StructFields,
    TupleFields,
    Struct,
    Tuple,
    Function,
    Trait,
    Unsupported,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum SkipPredicate {
    None,
    Some,
    Empty,
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    /// `&mut Formatter<'_>`.
    fn is_formatter(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Ref(_, inner, rustc_ast::Mutability::Mut)
            if self.is_std_adt(*inner, Symbol::intern("Formatter")))
    }
    pub(super) fn display_trait(&self) -> DefId {
        self.tcx
            .get_diagnostic_item(Symbol::intern("Display"))
            .expect("std has `Display`")
    }
    pub(super) fn is_fmt_result(&self, ty: Ty<'tcx>) -> bool {
        let fmt = self.tcx.associated_item_def_ids(self.display_trait())[0];
        let result = self.tcx.fn_sig(fmt).skip_binder().skip_binder().output();
        self.tcx.erase_and_anonymize_regions(ty) == self.tcx.erase_and_anonymize_regions(result)
    }
    pub(super) fn formatter_param(&self, def_id: DefId) -> Option<usize> {
        let sig = self.tcx.fn_sig(def_id).skip_binder().skip_binder();
        if !self.is_fmt_result(sig.output()) {
            return None;
        }
        sig.inputs().iter().position(|&t| self.is_formatter(t))
    }

    pub(super) fn write_call(&self, def_id: DefId, known_function: bool) -> WriteCall {
        let tcx = self.tcx;
        let trait_id = tcx.trait_of_assoc(def_id);
        let is_trait = |name: &str| trait_id.is_some_and(|t| tcx.is_diagnostic_item(Symbol::intern(name), t));
        let owner = tcx
            .inherent_impl_of_assoc(def_id)
            .map(|imp| tcx.type_of(imp).instantiate_identity());
        let on_formatter = owner.is_some_and(|t| self.is_std_adt(t, Symbol::intern("Formatter")));
        match tcx.item_name(def_id).as_str() {
            "write_fmt" | "write_str" | "write_char" if on_formatter || is_trait("FmtWrite") => WriteCall::Text,
            "fmt" if is_trait("Display") => WriteCall::Display,
            "fmt" if is_trait("Debug") => WriteCall::Debug,
            "debug_struct_fields_finish" if on_formatter => WriteCall::StructFields,
            "debug_tuple_fields_finish" if on_formatter => WriteCall::TupleFields,
            name if on_formatter && name.starts_with("debug_struct_field") && name.ends_with("_finish") => {
                WriteCall::Struct
            }
            name if on_formatter && name.starts_with("debug_tuple_field") && name.ends_with("_finish") => {
                WriteCall::Tuple
            }
            _ if known_function && trait_id.is_none() => WriteCall::Function,
            _ if trait_id.is_some_and(|t| t.is_local()) => WriteCall::Trait,
            _ => WriteCall::Unsupported,
        }
    }

    pub(super) fn skip_predicate(&self, function: DefId, ty: Ty<'tcx>) -> Option<SkipPredicate> {
        let standard = matches!(self.tcx.crate_name(function.krate).as_str(), "core" | "alloc");
        match self.tcx.item_name(function).as_str() {
            "is_none" if standard && self.option_of(ty).is_some() => Some(SkipPredicate::None),
            "is_some" if standard && self.option_of(ty).is_some() => Some(SkipPredicate::Some),
            "is_empty" if standard && (self.is_vec_like(ty.peel_refs()) || self.is_string_like(ty)) => {
                Some(SkipPredicate::Empty)
            }
            _ => None,
        }
    }

    pub(super) fn skips_none(&self, function: DefId) -> bool {
        self.tcx.crate_name(function.krate).as_str() == "core" && self.tcx.item_name(function).as_str() == "is_none"
    }
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    pub(super) fn is_parse_error(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.crate_name(adt.did().krate) == rustc_span::sym::core
            && ["ParseIntError", "ParseFloatError", "ParseBoolError", "ParseCharError", "TryFromIntError"]
                .contains(&self.tcx.item_name(adt.did()).as_str()))
    }

    pub(super) fn is_json_error(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.crate_name(adt.did().krate).as_str() == "serde_json"
            && self.tcx.item_name(adt.did()).as_str() == "Error")
    }

    pub(super) fn is_reverse(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.crate_name(adt.did().krate) == rustc_span::sym::core
            && self.tcx.item_name(adt.did()).as_str() == "Reverse")
    }

    pub(super) fn is_array_iter(&self, ty: Ty<'tcx>) -> bool {
        let ty = self.reveal(ty);
        let ty::Adt(adt, _) = ty.kind() else { return false };
        let path = self.tcx.def_path_str(adt.did());
        let krate = self.tcx.crate_name(adt.did().krate);
        (krate == sym::core || krate == sym::alloc)
            && (path.contains("::iter::")
                || [
                    "std::slice::Iter",
                    "std::vec::IntoIter",
                    "std::str::Chars",
                    "std::str::SplitWhitespace",
                    "std::str::Lines",
                    "std::array::IntoIter",
                    "std::char::ToUppercase",
                    "std::char::ToLowercase",
                    "std::collections::vec_deque::Iter",
                    "std::collections::vec_deque::IntoIter",
                    "std::collections::binary_heap::Iter",
                    "std::collections::binary_heap::IntoIter",
                ]
                .contains(&path.as_str())
                || self.is_str_split(ty))
            // A map's `iter()`, `keys()` and `values()` are arrays too (ADR 0059).
            || ((krate == sym::alloc || krate == sym::std)
                && ["btree_map::Iter", "btree_map::IterMut", "btree_map::Keys", "btree_map::Values", "btree_map::ValuesMut", "btree_map::IntoIter", "btree_set::Iter", "btree_set::IntoIter"]
                    .iter()
                    .any(|name| path == format!("std::collections::{name}")))
            || (krate == sym::std
                && ["hash_map::Iter", "hash_map::IterMut", "hash_map::Keys", "hash_map::Values", "hash_map::ValuesMut", "hash_map::IntoIter", "hash_set::Iter", "hash_set::IntoIter"]
                    .iter()
                    .any(|name| path == format!("std::collections::{name}")))
    }

    pub(super) fn is_str_split(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.crate_name(adt.did().krate) == sym::core
            && self.tcx.item_name(adt.did()).as_str() == "Split"
            && self.tcx.def_path_str(adt.did()).contains("str::"))
    }

    pub(super) fn reveal(&self, ty: Ty<'tcx>) -> Ty<'tcx> {
        if !rustc_middle::ty::TypeVisitableExt::has_opaque_types(&ty) {
            return ty;
        }
        self.tcx
            .try_normalize_erasing_regions(self.typing_env, ty)
            .unwrap_or(ty)
    }

    pub(super) fn is_std_wrapper(&self, ty: Ty<'tcx>) -> bool {
        ty.is_box()
            || self.is_lang_adt(ty, LangItem::String)
            || ["Rc", "Cell", "RefCell", "RefCellRef", "RefCellRefMut"]
                .into_iter()
                .any(|name| self.is_std_adt(ty, Symbol::intern(name)))
            || self.is_vec_like(ty)
    }

    pub(super) fn is_std(&self, id: DefId) -> bool {
        [sym::core, sym::alloc, sym::std].contains(&self.tcx.crate_name(id.krate))
    }

    pub(super) fn is_derived_impl(&self, trait_id: DefId, ty: Ty<'tcx>) -> bool {
        let tr = ty::TraitRef::new_from_args(self.tcx, trait_id, self.args_of(trait_id, ty));
        let tr = self.tcx.erase_and_anonymize_regions(tr);
        matches!(self.tcx.codegen_select_candidate(self.typing_env.as_query_input(tr)),
            Ok(ImplSource::UserDefined(imp)) if self.trait_impls.contains(&imp.impl_def_id)
                && self.tcx.is_automatically_derived(imp.impl_def_id))
    }
}

pub(super) fn serde_trait(tcx: TyCtxt<'_>, trait_id: DefId) -> Option<bool> {
    if !matches!(tcx.crate_name(trait_id.krate).as_str(), "serde" | "serde_core") {
        return None;
    }
    match tcx.item_name(trait_id).as_str() {
        "Serialize" => Some(true),
        "Deserialize" | "DeserializeOwned" => Some(false),
        _ => None,
    }
}

pub(super) fn from_serde_derive(tcx: TyCtxt<'_>, def_id: LocalDefId) -> bool {
    let mut item = Some(def_id);
    while let Some(id) = item {
        let mut ctxt = tcx.def_span(id).ctxt();
        while !ctxt.is_root() {
            let expansion = ctxt.outer_expn_data();
            // By the macro, not its name: `serde::Deserialize` and an alias are
            // named as they're written.
            if let ExpnKind::Macro(MacroKind::Derive, _) = expansion.kind
                && expansion
                    .macro_def_id
                    .is_some_and(|id| tcx.crate_name(id.krate).as_str() == "serde_derive")
            {
                return true;
            }
            ctxt = expansion.call_site.ctxt();
        }
        item = tcx.opt_local_parent(id);
    }
    false
}

pub(super) fn serde_impl(tcx: TyCtxt<'_>, id: DefId) -> Option<bool> {
    if !matches!(tcx.def_kind(id), DefKind::Impl { of_trait: true }) {
        return None;
    }
    let tr = tcx.impl_trait_ref(id).instantiate_identity();
    let own = matches!(tr.self_ty().kind(), ty::Adt(adt, _)
        if adt.did().as_local().is_some_and(|local| !from_serde_derive(tcx, local)));
    serde_trait(tcx, tr.def_id).filter(|_| own)
}

pub(super) fn is_extend(tcx: rustc_middle::ty::TyCtxt<'_>, trait_id: rustc_span::def_id::DefId) -> bool {
    tcx.crate_name(trait_id.krate) == rustc_span::sym::core && tcx.item_name(trait_id) == Symbol::intern("Extend")
}

/// How an explicitly fallible JavaScript binding delivers its result.
pub(super) enum Catching {
    Direct,
    Result,
    PromiseResult,
}

pub(super) struct UnsupportedString {
    pub name: Symbol,
    pub indexing: bool,
    pub suggest_is_empty: bool,
}

#[derive(Clone, Copy)]
pub(super) enum OrderingCall {
    Compare,
    Lt,
    Le,
    Gt,
    Ge,
    Max,
    Min,
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    pub(super) fn catching(&self, id: DefId) -> Catching {
        let output = self.tcx.fn_sig(id).skip_binder().skip_binder().output();
        if self.is_std_adt(output, sym::Result) {
            Catching::Result
        } else if matches!(output.kind(), ty::Adt(adt, args) if self.is_js_object(output)
            && self.tcx.item_name(adt.did()).as_str() == "Promise"
            && args.types().next().is_some_and(|t| self.is_std_adt(t, sym::Result)))
        {
            Catching::PromiseResult
        } else {
            Catching::Direct
        }
    }

    pub(super) fn unsupported_string(&self, id: DefId, receiver: Option<Ty<'tcx>>) -> Option<UnsupportedString> {
        if !receiver.is_some_and(|ty| self.is_string_like(ty)) {
            return None;
        }
        let indexing = self
            .tcx
            .trait_of_assoc(id)
            .is_some_and(|t| self.tcx.is_lang_item(t, LangItem::Index));
        let name = self.tcx.item_name(id);
        (indexing
            || [
                "len",
                "find",
                "rfind",
                "char_indices",
                "match_indices",
                "rmatch_indices",
            ]
            .contains(&name.as_str()))
        .then_some(UnsupportedString {
            name,
            indexing,
            suggest_is_empty: name.as_str() == "len",
        })
    }

    pub(super) fn ordering_call(&self, id: DefId, tr: ty::TraitRef<'tcx>) -> Option<(OrderingCall, bool)> {
        let partial = self.tcx.is_lang_item(tr.def_id, LangItem::PartialOrd);
        if !partial && !self.tcx.is_diagnostic_item(sym::Ord, tr.def_id) {
            return None;
        }
        let name = self.tcx.item_name(id);
        if Num::of(tr.self_ty().peel_refs()).is_some() && name.as_str() != "partial_cmp" {
            return None;
        }
        let call = match name.as_str() {
            "lt" => OrderingCall::Lt,
            "le" => OrderingCall::Le,
            "gt" => OrderingCall::Gt,
            "ge" => OrderingCall::Ge,
            "cmp" | "partial_cmp" => OrderingCall::Compare,
            "max" => OrderingCall::Max,
            "min" => OrderingCall::Min,
            _ => return None,
        };
        Some((call, partial))
    }
}

pub(super) fn operational(tcx: TyCtxt<'_>, id: DefId) -> bool {
    id.is_local()
        || tcx.is_lang_item(id, LangItem::Copy)
        || tcx.is_lang_item(id, LangItem::Clone)
        || tcx.is_lang_item(id, LangItem::PartialEq)
        || tcx.is_lang_item(id, LangItem::PartialOrd)
        || tcx.is_diagnostic_item(sym::Ord, id)
        || tcx.is_diagnostic_item(Symbol::intern("Display"), id)
        || tcx.is_diagnostic_item(Symbol::intern("Debug"), id)
        || tcx.is_diagnostic_item(Symbol::intern("Default"), id)
        // Its evidence is the writer or reader itself (ADR 0081).
        || serde_trait(tcx, id).is_some()
}

pub(super) fn implementable(tcx: TyCtxt<'_>, id: DefId) -> bool {
    operational(tcx, id)
        || tcx.is_diagnostic_item(sym::From, id)
        || tcx.is_diagnostic_item(sym::TryFrom, id)
        || tcx.is_diagnostic_item(sym::Eq, id)
        || tcx.is_diagnostic_item(sym::Iterator, id)
        || is_operator(tcx, id)
}

pub(super) fn is_operator(tcx: TyCtxt<'_>, id: DefId) -> bool {
    [
        LangItem::Add,
        LangItem::Sub,
        LangItem::Mul,
        LangItem::Div,
        LangItem::Rem,
        LangItem::Neg,
        LangItem::Not,
        LangItem::AddAssign,
        LangItem::SubAssign,
        LangItem::MulAssign,
        LangItem::DivAssign,
        LangItem::RemAssign,
        LangItem::Index,
    ]
    .into_iter()
    .any(|item| tcx.is_lang_item(id, item))
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    pub(super) fn bounded_by(&self, ty: ty::Ty<'tcx>, name: Symbol) -> bool {
        let ty = ty.peel_refs();
        let Some(trait_id) = self.tcx.get_diagnostic_item(name) else {
            return false;
        };
        matches!(ty.kind(), ty::Param(_)) && {
            let tr = ty::TraitRef::new(self.tcx, trait_id, [ty]);
            matches!(
                self.tcx.codegen_select_candidate(self.typing_env.as_query_input(tr)),
                Ok(rustc_middle::traits::ImplSource::Param(_))
            )
        }
    }

    pub(super) fn is_generic_iter(&self, ty: ty::Ty<'tcx>) -> bool {
        self.bounded_by(ty, sym::Iterator)
    }

    pub(super) fn is_lazy_iter(&self, ty: ty::Ty<'tcx>) -> bool {
        let ty = self.reveal(ty.peel_refs());
        self.is_user_iterator(ty)
            || self.is_generic_iter(ty)
            || matches!(ty.kind(), ty::Adt(_, args) if self.is_array_iter(ty) && args.types().any(|t| self.is_lazy_iter(t)))
    }
}

pub(super) enum TraitCall {
    Clone,
    Default,
    Equality { negate: bool },
    Ordering,
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    pub(super) fn trait_call(&self, method: DefId, trait_id: DefId) -> Option<TraitCall> {
        if self.tcx.is_lang_item(method, LangItem::CloneFn) {
            Some(TraitCall::Clone)
        } else if self.tcx.is_diagnostic_item(Symbol::intern("Default"), trait_id) {
            Some(TraitCall::Default)
        } else if self.tcx.is_lang_item(trait_id, LangItem::PartialEq) {
            Some(TraitCall::Equality {
                negate: self.tcx.item_name(method).as_str() == "ne",
            })
        } else if self.tcx.is_diagnostic_item(sym::Ord, trait_id)
            || self.tcx.is_lang_item(trait_id, LangItem::PartialOrd)
        {
            Some(TraitCall::Ordering)
        } else {
            None
        }
    }

    /// Resolve blanket Into/TryInto through From/TryFrom. The caller decides
    /// whether the resulting implementation is available for emission.
    pub(super) fn resolve_into(
        &self,
        def_id: DefId,
        args: ty::GenericArgsRef<'tcx>,
    ) -> Option<(DefId, ty::GenericArgsRef<'tcx>, DefId)> {
        let into = self.tcx.trait_of_assoc(def_id)?;
        let from = if self.tcx.is_diagnostic_item(sym::Into, into) {
            self.tcx.get_diagnostic_item(sym::From)?
        } else if self.tcx.is_diagnostic_item(sym::TryInto, into) {
            self.tcx.get_diagnostic_item(sym::TryFrom)?
        } else {
            return None;
        };
        let method = self
            .tcx
            .associated_items(from)
            .in_definition_order()
            .find(|item| item.is_fn())?
            .def_id;
        let args = self.tcx.mk_args(&[args[1], args[0]]);
        let instance = ty::Instance::try_resolve(self.tcx, self.typing_env, method, args).ok()??;
        Some((method, args, instance.def_id()))
    }
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    pub(super) fn conversion(&self, adt: DefId, convert: Symbol, serialize: bool) -> Option<Ty<'tcx>> {
        struct Finder<'tcx> {
            tcx: TyCtxt<'tcx>,
            types: &'tcx ty::TypeckResults<'tcx>,
            convert: DefId,
            found: Option<Ty<'tcx>>,
        }
        impl<'tcx> intravisit::Visitor<'tcx> for Finder<'tcx> {
            fn visit_expr(&mut self, expr: &'tcx hir::Expr<'tcx>) {
                if let hir::ExprKind::Path(ref path) = expr.kind
                    && let hir::def::Res::Def(_, id) = self.types.qpath_res(path, expr.hir_id)
                    && self.tcx.trait_of_assoc(id) == Some(self.convert)
                {
                    self.found = Some(self.types.node_args(expr.hir_id).type_at(1));
                }
                intravisit::walk_expr(self, expr);
            }
        }
        let convert = self.tcx.get_diagnostic_item(convert)?;
        for owner in self.tcx.hir_body_owners() {
            let mut parent = self.tcx.opt_parent(owner.to_def_id());
            while let Some(id) = parent
                && serde_impl(self.tcx, id).is_none()
            {
                parent = self.tcx.opt_parent(id);
            }
            let Some(imp) = parent else { continue };
            let self_ty = self.tcx.type_of(imp).instantiate_identity();
            if serde_impl(self.tcx, imp) != Some(serialize)
                || !matches!(self_ty.kind(), ty::Adt(a, _) if a.did() == adt)
            {
                continue;
            }
            let body = self.tcx.hir_body_owned_by(owner);
            let mut finder = Finder {
                tcx: self.tcx,
                types: self.tcx.typeck_body(body.id()),
                convert,
                found: None,
            };
            intravisit::Visitor::visit_body(&mut finder, body);
            if finder.found.is_some() {
                return finder.found;
            }
        }
        None
    }
}

impl<'a, 'tcx> Recognition<'a, 'tcx> {
    pub(super) fn is_mutable_map_get(&self, id: DefId, args: ty::GenericArgsRef<'tcx>) -> bool {
        self.classify(id, args) == Some(Std::Map(MapOp::Get)) && self.tcx.item_name(id).as_str() == "get_mut"
    }
}

pub(super) fn ordering_value(tcx: TyCtxt<'_>, enum_def: DefId, variant: Symbol) -> Option<i128> {
    if !tcx.is_lang_item(enum_def, LangItem::OrderingEnum) {
        return None;
    }
    Some(match variant.as_str() {
        "Less" => -1,
        "Equal" => 0,
        _ => 1,
    })
}
