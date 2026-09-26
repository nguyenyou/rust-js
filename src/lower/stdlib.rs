//! Recognize supported standard-library operations and translate their behavior.

use super::combinators::{self, Comb, IterComb};
use super::format_spec::{Radix, Spec};
use super::maps::{MapOp, Part};
use super::representation::Num;
use super::{FnCx, R};
use crate::js;
use crate::js::{Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_ast::LitKind;
use rustc_hir::LangItem;
use rustc_middle::mir::{BinOp, UnOp};
use rustc_middle::thir::{self, ExprId, ExprKind, PatKind};
use rustc_middle::ty::{self, Ty};
use rustc_span::{ErrorGuaranteed, Span, Symbol, sym};

/// The std functions whose JS meaning rust-js knows (ADRs 0023, 0025).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Std {
    /// The argument itself: `Box::new(x)`, `Rc::new(x)`,
    /// `s.to_owned()`, `String::from(s)`, `v.iter()`, and `Deref` of
    /// `String`, `Rc`, `Vec`, `Ref`, `RefMut` and JS objects.
    Same,
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
    /// `Argument::from_usize`: a width or precision from an argument, `{:>w$}`.
    FmtUsize,
    /// A `HashMap` or `HashSet` method (ADR 0059).
    Map(MapOp),
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
                | Std::Cloned
                | Std::IterComb(_)
        )
    }
}

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
            // `v[i]` of a `Vec` is a slice's, checked the same way.
            if (tcx.is_lang_item(trait_, LangItem::Index) || tcx.is_lang_item(trait_, LangItem::IndexMut))
                && self.is_std_adt(ty.peel_refs(), sym::Vec)
                && args.types().nth(1).is_some_and(|i| i.is_usize())
            {
                return Some(Std::Index);
            }
            if tcx.is_lang_item(trait_, LangItem::Add) {
                return self.is_lang_adt(ty, LangItem::String).then_some(Std::Concat);
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
                    name if let Some(comb) = combinators::classify_iter(name) => Std::IterComb(comb),
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
            // `v.extend(items)` (ADR 0062).
            if combinators::is_extend(tcx, trait_) && self.is_std_adt(ty.peel_refs(), sym::Vec) {
                return Some(Std::Comb(Comb::Extend));
            }
            // `HashMap::from([(k, v)])`: `new Map([[k, v]])`.
            if tcx.is_diagnostic_item(sym::From, trait_) && self.is_map(ty) {
                let set = self.is_set(ty);
                return Some(Std::Map(MapOp::From { set }));
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
        let (map, set) = (adt("HashMap") || adt("BTreeMap"), adt("HashSet") || adt("BTreeSet"));
        let entry = adt("HashMapEntry") || adt("BTreeEntry");
        let name = tcx.item_name(def_id);
        if let Some(comb) = combinators::classify(
            name.as_str(),
            option,
            result,
            adt("Vec"),
            adt("Vec") || owner.is_slice(),
        ) {
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
                let kind = self
                    .std_fn(fun)
                    .filter(|k| matches!(k, Std::FmtDisplay | Std::FmtDebug | Std::FmtRadix(_) | Std::FmtUsize))?;
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
    /// shown twice goes in a `const` too, unless it's a variable or a constant.
    pub(super) fn lower_format_args(&mut self, f: FormatArgs<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let in_order = self.in_order(&f, span);
        let shown = self.shown(&f, span).unwrap_or_default();
        let mut values = self.operands(&f.values, out)?;
        let effects = values.iter().any(Expr::has_effects);
        for (i, value) in values.iter_mut().enumerate() {
            let twice = shown.iter().filter(|&&v| v == i).count() > 1;
            let spill = if in_order {
                false
            } else if effects {
                !value.is_constant() && !self.is_place_expr(f.values[i])
            } else {
                twice && !value.reads_same()
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
        Ok(parts
            .into_iter()
            .reduce(|a, b| Expr::bin(Op::Add, a, b))
            .unwrap_or_else(|| Expr::str("")))
    }

    /// An iterator's method (ADR 0036). The iterator is a JS array: a range
    /// becomes one, `$range(a, b)`, and the rest already are.
    /// Is `ty` an iterator of the crate's own (ADR 0055)? `&mut` of one is too.
    pub(super) fn is_user_iterator(&self, ty: ty::Ty<'tcx>) -> bool {
        let iterator = self.tcx.get_diagnostic_item(sym::Iterator).expect("std has `Iterator`");
        matches!(ty.peel_refs().kind(), ty::Adt(..)) && self.has_user_impl(iterator, ty.peel_refs())
    }

    /// A type parameter that's an `Iterator`: `I: Iterator<Item = u32>`, or
    /// `impl Iterator` as a parameter's type (ADR 0061).
    pub(super) fn is_generic_iter(&self, ty: ty::Ty<'tcx>) -> bool {
        self.bounded_by(ty, sym::Iterator)
    }

    /// A type parameter with a bound of the std trait `name`.
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

    /// An iterator that's a JS iterator, not an array (ADR 0055): one of the
    /// crate's own, or std's adapters on one.
    pub(super) fn is_lazy_iter(&self, ty: ty::Ty<'tcx>) -> bool {
        let ty = self.reveal(ty.peel_refs());
        self.is_user_iterator(ty)
            || self.is_generic_iter(ty)
            || matches!(ty.kind(), ty::Adt(_, args) if self.is_array_iter(ty) && args.types().any(|t| self.is_lazy_iter(t)))
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
        let boxed = ty::Instance::try_resolve(self.tcx, self.typing_env, next, args)?.is_some_and(|instance| {
            let id = instance.def_id();
            let output = self.tcx.fn_sig(id).instantiate_identity().skip_binder().output();
            let output = self
                .tcx
                .try_normalize_erasing_regions(ty::TypingEnv::post_analysis(self.tcx, id), output)
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
        let projection = ty::Ty::new_projection(self.tcx, item, [iterator]);
        self.tcx.try_normalize_erasing_regions(self.typing_env, projection).ok()
    }

    pub(super) fn iterator_call(
        &mut self,
        known: Std,
        args: &[ExprId],
        generic_args: ty::GenericArgsRef<'tcx>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let receiver_ty = self.reveal(self.thir[args[0]].ty);
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
            // `a..=b`: `$range(a, b + 1)`.
            _ if let Some((start, end)) = self.inclusive_range(args[0]) => {
                self.runtime.insert(Helper::Range);
                let [start, end]: [Expr; 2] = self.operands(&[start, end], out)?.try_into().ok().unwrap();
                Expr::call(Expr::var("$range"), vec![start, Expr::bin(Op::Add, end, Expr::int(1))])
            }
            _ => {
                let value = self.expr(args[0], out)?;
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
            Std::ArrayMethod(_) | Std::Enumerate | Std::Fold | Std::Sum | Std::Skip | Std::Take | Std::Cloned => items,
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
                let zero = if num == Num::F64 { Expr::num(-0.0) } else { Expr::int(0) };
                method(items, "reduce", vec![f, zero])
            }
            Std::CollectString => method(items, "join", vec![Expr::str("")]),
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
                        js::ExprKind::Var(name) => {
                            ["$range", "$zip", "$takeWhile", "$skipWhile", "$windows", "$chunks"]
                                .contains(&name.as_str())
                        }
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
