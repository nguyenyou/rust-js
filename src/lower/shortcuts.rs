//! Shortcuts: the questions `recognition` answers, asked of the function
//! being lowered, `self.is_map(ty)` for `self.recognition().is_map(ty)`.
//! Each is answered there, and only shortened here.

use super::FnCx;
use super::ranges::RangeKind;
use super::recognition::Json;
use rustc_hir::LangItem;
use rustc_middle::ty::{self, Ty};
use rustc_span::Symbol;
use rustc_span::def_id::DefId;

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// `#[serde(from = "T")]`, `try_from` or `into` on `adt`: `T`, from the
    /// derive's call of `From::from`, `TryFrom::try_from` or `Into::into`
    /// (the trait `convert` names), whose types rustc worked out.
    pub(super) fn conversion(&self, adt: DefId, convert: Symbol, serialize: bool) -> Option<Ty<'tcx>> {
        self.recognition().conversion(adt, convert, serialize)
    }

    /// Is `ty` a `Peekable`, which is always a `$iter` (ADR 0071)?
    pub(super) fn is_peekable(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_peekable(ty)
    }

    /// `std::cmp::Reverse`, which has no diagnostic item of its own.
    pub(super) fn is_reverse(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_reverse(ty)
    }

    pub(super) fn range_kind(&self, ty: Ty<'tcx>) -> Option<RangeKind> {
        self.recognition().range_kind(ty)
    }

    pub(super) fn display_trait(&self) -> DefId {
        self.recognition().display_trait()
    }

    /// `fmt::Result`, as `Display::fmt` returns it.
    pub(super) fn is_fmt_result(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_fmt_result(ty)
    }

    /// Which parameter of `def_id` is the `Formatter` it writes to, if it
    /// takes one and returns a `fmt::Result`. In JS it returns the string
    /// instead, and takes no formatter.
    pub(super) fn formatter_param(&self, def_id: DefId) -> Option<usize> {
        self.recognition().formatter_param(def_id)
    }

    /// `ParseIntError`, `TryFromIntError` and the like, which rust-js holds as their message
    /// (ADR 0063): `e.to_string()` is the message itself.
    pub(super) fn is_parse_error(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_parse_error(ty)
    }

    /// `serde_json::Error`: `{ message, line, column }` (ADR 0077).
    pub(super) fn is_json_error(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_json_error(ty)
    }

    /// A `HashMap`, `HashSet`, `BTreeMap` or `BTreeSet`: a JS `Map` or `Set`.
    /// serde_json's `Map` is one too, a `BTreeMap` (ADR 0083).
    pub(super) fn is_map(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_map(ty)
    }

    /// A `HashSet` or `BTreeSet`: a JS `Set`.
    pub(super) fn is_set(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_set(ty)
    }

    /// The trait's arguments for `Self = ty`, with `ty` for any others too:
    /// `PartialEq`'s `Rhs` is `Self` unless it says otherwise.
    pub(super) fn args_of(&self, trait_id: DefId, ty: Ty<'tcx>) -> ty::GenericArgsRef<'tcx> {
        self.recognition().args_of(trait_id, ty)
    }

    /// Does `ty` use a hand-written impl of `trait_id` from this crate?
    pub(super) fn has_user_impl(&self, trait_id: DefId, ty: Ty<'tcx>) -> bool {
        self.recognition().has_user_impl(trait_id, ty)
    }

    /// Is the crate's impl of `trait_id` for `ty` a `#[derive]`d one?
    pub(super) fn is_derived_impl(&self, trait_id: DefId, ty: Ty<'tcx>) -> bool {
        self.recognition().is_derived_impl(trait_id, ty)
    }

    /// Is `tr` a hand-written impl from this crate?
    pub(super) fn is_user_impl(&self, tr: ty::TraitRef<'tcx>) -> bool {
        self.recognition().is_user_impl(tr)
    }

    pub(super) fn is_std(&self, id: DefId) -> bool {
        self.recognition().is_std(id)
    }

    /// `str`, `String`, `char`, or a reference to one: all JS strings.
    pub(super) fn is_string_like(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_string_like(ty)
    }

    /// An iterator that's a JS array (ADR 0036): a slice's or a `Vec`'s, a
    /// `split` or `chars` of a string, and the adapters on them.
    pub(super) fn is_array_iter(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_array_iter(ty)
    }

    /// `str::split`'s iterator, which is a JS array of strings (ADR 0034).
    pub(super) fn is_str_split(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_str_split(ty)
    }

    /// The type an `impl Trait` stands for, which rustc knows after type
    /// checking (ADR 0061); any other type is itself.
    pub(super) fn reveal(&self, ty: Ty<'tcx>) -> Ty<'tcx> {
        self.recognition().reveal(ty)
    }

    /// `T`, for an `Option<T>`.
    pub(super) fn option_of(&self, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
        self.recognition().option_of(ty)
    }

    pub(super) fn is_std_adt(&self, ty: Ty<'tcx>, name: Symbol) -> bool {
        self.recognition().is_std_adt(ty, name)
    }

    pub(super) fn is_lang_adt(&self, ty: Ty<'tcx>, item: LangItem) -> bool {
        self.recognition().is_lang_adt(ty, item)
    }

    /// std types that aren't plain structs in JS: `String` is a JS string,
    /// `Box<T>` and `Rc<T>` are just `T`, `Cell<T>` and `RefCell<T>` are
    /// `{ value }`, a `RefCell`'s guards are what they guard, and `Vec<T>`
    /// is an array.
    pub(super) fn is_std_wrapper(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_std_wrapper(ty)
    }

    /// A `Vec`, a `VecDeque` or a `BinaryHeap`: a JS array (ADR 0068). A
    /// heap's array is in the order Rust's own heap keeps it.
    pub(super) fn is_vec_like(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_vec_like(ty)
    }

    /// A struct that stands for a JS object, like `webapi::Element` (ADR 0024):
    /// its only field is `PhantomData` of an extern type. Rust never builds
    /// one; it only holds references to them, which are the JS objects.
    pub(super) fn is_js_object(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_js_object(ty)
    }

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

    /// `serde_json::Value`, `Number` or `Map`.
    pub(in crate::lower) fn json_type(&self, ty: Ty<'tcx>) -> Option<Json> {
        self.recognition().json_type(ty)
    }

    pub(in crate::lower) fn is_json_map(&self, ty: Ty<'tcx>) -> bool {
        self.recognition().is_json_map(ty)
    }
}
