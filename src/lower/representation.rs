//! Rust value representations, copying, and supported-type validation.

use super::bindings::field_key;
use super::{FnCx, R, Shape};
use crate::js;
use crate::js::{Expr, Op, Prop};
use rustc_ast::Mutability;
use rustc_hir as hir;
use rustc_hir::def::CtorKind;
use rustc_hir::{BindingMode, ByRef, LangItem};
use rustc_middle::mir::interpret::GlobalId;
use rustc_middle::ty;
use rustc_middle::ty::Ty;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol, sym};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// `str`, `String`, `char`, or a reference to one: all JS strings.
    pub(super) fn is_string_like(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        ty.is_str() || ty.is_char() || self.is_lang_adt(ty, LangItem::String)
    }

    /// An iterator that's a JS array (ADR 0036): a slice's or a `Vec`'s, a
    /// `split` or `chars` of a string, and the adapters on them.
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

    /// `str::split`'s iterator, which is a JS array of strings (ADR 0034).
    pub(super) fn is_str_split(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.crate_name(adt.did().krate) == sym::core
            && self.tcx.item_name(adt.did()).as_str() == "Split"
            && self.tcx.def_path_str(adt.did()).contains("str::"))
    }

    /// The type an `impl Trait` stands for, which rustc knows after type
    /// checking (ADR 0061); any other type is itself.
    pub(super) fn reveal(&self, ty: Ty<'tcx>) -> Ty<'tcx> {
        if !rustc_middle::ty::TypeVisitableExt::has_opaque_types(&ty) {
            return ty;
        }
        self.tcx
            .try_normalize_erasing_regions(self.typing_env, ty)
            .unwrap_or(ty)
    }

    /// `T`, for an `Option<T>`.
    pub(super) fn option_of(&self, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
        match ty.kind() {
            ty::Adt(adt, args) if self.tcx.is_lang_item(adt.did(), LangItem::Option) => args.types().next(),
            _ => None,
        }
    }

    /// What an `Option<T>`'s `T` is in JS: through references, `Box` and `Rc`,
    /// which are the value itself (ADR 0023).
    fn payload(&self, mut ty: Ty<'tcx>) -> Ty<'tcx> {
        loop {
            ty = match ty.kind() {
                ty::Ref(_, inner, _) => *inner,
                ty::Adt(_, args) if ty.is_box() || self.is_std_adt(ty, Symbol::intern("Rc")) => args.type_at(0),
                _ => return ty,
            };
        }
    }

    /// An `Option<T>` whose `T` might look like `None` only because it's a
    /// type parameter: `Some` of it is boxed when it does (ADR 0051).
    pub(super) fn boxed_payload(&self, ty: Ty<'tcx>) -> bool {
        matches!(self.payload(ty).kind(), ty::Param(_))
    }

    /// Can a `T` be `undefined` or `null` in JS? Then `Option<T>` can't be
    /// `T` itself: `Some(())` and `None` would be the same value.
    pub(super) fn can_be_nullish(&self, ty: Ty<'tcx>) -> bool {
        let ty = self.payload(ty);
        ty.is_unit()
            || matches!(ty.kind(), ty::Param(_) | ty::Alias(..))
            || self.option_of(ty).is_some()
            || matches!(ty.kind(), ty::Adt(adt, _) if adt.is_struct() && adt.non_enum_variant().fields.is_empty())
    }

    pub(super) fn is_std_adt(&self, ty: Ty<'tcx>, name: Symbol) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.is_diagnostic_item(name, adt.did()))
    }

    pub(super) fn is_lang_adt(&self, ty: Ty<'tcx>, item: LangItem) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.is_lang_item(adt.did(), item))
    }

    /// std types that aren't plain structs in JS: `String` is a JS string,
    /// `Box<T>` and `Rc<T>` are just `T`, `Cell<T>` and `RefCell<T>` are
    /// `{ value }`, a `RefCell`'s guards are what they guard, and `Vec<T>`
    /// is an array.
    pub(super) fn is_std_wrapper(&self, ty: Ty<'tcx>) -> bool {
        ty.is_box()
            || self.is_lang_adt(ty, LangItem::String)
            || ["Rc", "Cell", "RefCell", "RefCellRef", "RefCellRefMut", "Vec"]
                .into_iter()
                .any(|name| self.is_std_adt(ty, Symbol::intern(name)))
    }

    /// Is a `ty` value a JS object? Then a reference to it, even `&mut`, can
    /// be the object itself: changes through it change the one object (ADR 0025).
    pub(super) fn is_object(&self, ty: Ty<'tcx>) -> bool {
        matches!(self.shape(ty), Shape::Object(_) | Shape::Array(_))
            || self.is_js_object(ty)
            // A slice or an array is a JS array: `&mut` to one, as `sort` takes, is it.
            || ty.is_slice()
            || ty.is_array()
            || ["Vec", "Cell", "RefCell"].into_iter().any(|name| self.is_std_adt(ty, Symbol::intern(name)))
            || self.is_map(ty)
            // An enum with fields: those variants are objects (ADR 0033). A
            // fieldless one's string can't be changed through a `&mut` anyway,
            // since `*r = ..` of a whole value isn't supported. `Option` is
            // its value itself (ADR 0030), not an object.
            || matches!(ty.kind(), ty::Adt(adt, _) if adt.is_enum()
                && !self.tcx.is_lang_item(adt.did(), LangItem::Option)
                && adt.variants().iter().any(|v| !v.fields.is_empty()))
    }

    /// A struct that stands for a JS object, like `web::Element` (ADR 0024):
    /// its only field is `PhantomData` of an extern type. Rust never builds
    /// one; it only holds references to them, which are the JS objects.
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

    /// An enum variant's fields as JS properties (ADR 0033): `_0`, `_1` for a
    /// tuple variant, as in ReScript, and their names for a struct variant.
    pub(super) fn variant_fields(
        &self,
        variant: &ty::VariantDef,
        args: ty::GenericArgsRef<'tcx>,
    ) -> Vec<(String, Ty<'tcx>)> {
        variant
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| (variant_field(self.tcx, variant, i), f.ty(self.tcx, args)))
            .collect()
    }

    /// How a struct or tuple type looks in JS.
    pub(super) fn shape(&self, ty: Ty<'tcx>) -> Shape<'tcx> {
        if self.is_std_wrapper(ty) || self.is_js_object(ty) {
            return Shape::Other;
        }
        match ty.kind() {
            ty::Tuple(tys) if !tys.is_empty() => Shape::Array(tys.to_vec()),
            ty::Adt(adt, args) if adt.is_struct() => {
                let variant = adt.non_enum_variant();
                let fields = variant
                    .fields
                    .iter()
                    .map(|f| (field_key(self.tcx, f), f.ty(self.tcx, args)));
                match variant.ctor_kind() {
                    None => Shape::Object(fields.collect()),
                    Some(CtorKind::Fn) => Shape::Array(fields.map(|(_, ty)| ty).collect()),
                    Some(CtorKind::Const) => Shape::Other,
                }
            }
            _ => Shape::Other,
        }
    }

    /// Field `i` of a `ty` value: `base.x`, or `base[0]` for tuples.
    pub(super) fn project(&self, base: Expr, ty: Ty<'tcx>, i: usize) -> Expr {
        match (self.shape(ty), &base.kind) {
            // A part of `[a, b]` (a `match (a, b)` subject) is just `a`.
            (Shape::Array(_), js::ExprKind::Array(items)) if !base.has_effects() => items[i].clone(),
            (Shape::Array(_), _) => Expr::index(base, Expr::int(i as i128)),
            (Shape::Object(fields), _) => Expr::member(base, fields[i].0.clone()),
            (Shape::Other, _) => unreachable!("fields of a type without fields"),
        }
    }

    pub(super) fn is_copy(&self, ty: Ty<'tcx>) -> bool {
        self.tcx.type_is_copy_modulo_regions(self.typing_env, ty)
    }

    /// Rust copies a `Copy` value when it's read, and JS objects are shared
    /// references. The two only disagree if one of the copies is later
    /// changed in place, which needs a type in `mutated`. So only those
    /// types are copied, and everything else stays shared.
    pub(super) fn copy_if_needed(&self, place: Expr, ty: Ty<'tcx>) -> Expr {
        if self.contains_mutated(ty) && self.is_copy(ty) {
            self.copy(place, ty)
        } else {
            place
        }
    }

    pub(super) fn contains_mutated(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Param(_))
            || self.mutated_itself(ty)
            || match self.shape(ty) {
                Shape::Object(fields) => fields.iter().any(|&(_, t)| self.contains_mutated(t)),
                Shape::Array(tys) => tys.iter().any(|&t| self.contains_mutated(t)),
                // `Some(x)` is `x` (ADR 0030), and a variant's fields are
                // its object's (ADR 0033).
                Shape::Other => match ty.kind() {
                    _ if let Some(inner) = self.option_of(ty) => self.contains_mutated(inner),
                    ty::Adt(adt, args) if self.is_copy_enum(ty) => adt.variants().iter().any(|v| {
                        self.variant_fields(v, args)
                            .iter()
                            .any(|&(_, t)| self.contains_mutated(t))
                    }),
                    _ => false,
                },
            }
    }

    /// An enum rust-js writes as ADR 0033 says, that's `Copy`: one of the
    /// crate's own, or `Result`.
    fn is_copy_enum(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if adt.is_enum()
            && (adt.did().is_local() || self.is_std_adt(ty, sym::Result)))
            && self.is_copy(ty)
    }

    /// Is `ty` itself changed in place somewhere, not just a part of it?
    pub(super) fn mutated_itself(&self, ty: Ty<'tcx>) -> bool {
        self.krate.mutated.iter().any(|&mutated| self.instance_of(ty, mutated))
    }

    /// Is `ty` a `Vec` type something may change (ADR 0052)?
    pub(super) fn vec_changed(&self, ty: Ty<'tcx>) -> bool {
        self.krate
            .changed_vecs
            .iter()
            .any(|&changed| self.instance_of(ty, changed))
    }

    /// Is `ty` one of the types `general` stands for? A type mutated in a
    /// generic function has its parameters: `Holder<T>` stands for every
    /// `Holder<..>`, but `Pair<u32>` only for itself, so a `Pair<bool>`
    /// needn't be copied because a `Pair<u32>` is changed. Lifetimes don't
    /// matter.
    fn instance_of(&self, ty: Ty<'tcx>, general: Ty<'tcx>) -> bool {
        match (ty.kind(), general.kind()) {
            (_, ty::Param(_)) => true,
            (ty::Adt(adt, args), ty::Adt(general_adt, general_args)) => {
                adt.did() == general_adt.did()
                    && args.iter().zip(general_args.iter()).all(|(arg, general)| {
                        match (arg.as_type(), general.as_type()) {
                            (Some(arg), Some(general)) => self.instance_of(arg, general),
                            _ => true,
                        }
                    })
            }
            (ty::Ref(_, inner, _), ty::Ref(_, general, _))
            | (ty::Slice(inner), ty::Slice(general))
            | (ty::Array(inner, _), ty::Array(general, _)) => self.instance_of(*inner, *general),
            (ty::Tuple(parts), ty::Tuple(general)) => {
                parts.len() == general.len() && parts.iter().zip(general.iter()).all(|(p, g)| self.instance_of(p, g))
            }
            _ => self.tcx.erase_and_anonymize_regions(ty) == self.tcx.erase_and_anonymize_regions(general),
        }
    }

    /// A fresh `ty` value equal to the one at `place`: `{ ...p }`, `[t[0], t[1]]`.
    /// A field that also contains mutated types is copied in turn.
    pub(super) fn copy(&self, place: Expr, ty: Ty<'tcx>) -> Expr {
        if matches!(ty.kind(), ty::Param(_))
            && let Some((_, dictionary)) = self
                .evidence
                .iter()
                .find(|(tr, _)| tr.self_ty() == ty && self.tcx.is_lang_item(tr.def_id, LangItem::Copy))
        {
            return Expr::call(Expr::member(dictionary.clone(), "copy"), vec![place]);
        }
        // A copy reads its source once per part: a value that isn't a place,
        // like `$unwrap(v[0])`, is taken once, `((value) => ..)(source)`.
        let many = match self.shape(ty) {
            Shape::Object(fields) => fields.iter().any(|&(_, t)| self.contains_mutated(t)),
            Shape::Array(tys) => tys.len() > 1,
            Shape::Other => false,
        };
        if many && !place.reads_same() {
            let body = self.copy(Expr::var("value"), ty);
            return Expr::call(
                Expr::arrow(
                    vec!["value".into()],
                    vec![js::StmtKind::Return(Some(body)).at(js::Span::NONE)],
                ),
                vec![place],
            );
        }
        match self.shape(ty) {
            Shape::Object(fields) => {
                let mut props = vec![Prop::Spread(place.clone())];
                for (name, t) in fields {
                    if self.contains_mutated(t) {
                        let field = self.copy(Expr::member(place.clone(), name.clone()), t);
                        props.push(Prop::Field(name, field));
                    }
                }
                Expr::object(props)
            }
            Shape::Array(tys) => Expr::array(
                tys.into_iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let item = Expr::index(place.clone(), Expr::int(i as i128));
                        if self.contains_mutated(t) {
                            self.copy(item, t)
                        } else {
                            item
                        }
                    })
                    .collect(),
            ),
            Shape::Other => {
                // Read more than once: a value that isn't a place is taken once.
                let once = |place: Expr, copy: &dyn Fn(Expr) -> Expr| {
                    if place.reads_same() {
                        copy(place)
                    } else {
                        let body = copy(Expr::var("value"));
                        Expr::call(
                            Expr::arrow(
                                vec!["value".into()],
                                vec![js::StmtKind::Return(Some(body)).at(js::Span::NONE)],
                            ),
                            vec![place],
                        )
                    }
                };
                if let Some(inner) = self.option_of(ty) {
                    if !self.contains_mutated(inner) {
                        return place;
                    }
                    return once(place, &|o| {
                        let none = Expr::bin(Op::LooseEq, o.clone(), Expr::null());
                        Expr::cond(none, o.clone(), self.copy(o, inner))
                    });
                }
                // An array that's changed in place: `a.slice()`, or a copy of
                // each item that is too.
                if let ty::Array(item, _) = ty.kind() {
                    if !self.contains_mutated(*item) {
                        return Expr::call(Expr::member(place, "slice"), Vec::new());
                    }
                    let body = self.copy(Expr::var("item"), *item);
                    let copy = Expr::arrow(
                        vec!["item".into()],
                        vec![js::StmtKind::Return(Some(body)).at(js::Span::NONE)],
                    );
                    return Expr::call(Expr::member(place, "map"), vec![copy]);
                }
                let ty::Adt(adt, args) = ty.kind() else { return place };
                if !self.is_copy_enum(ty) {
                    return place;
                }
                // `{ TAG: "Line", _0: .. }`: a variant with a part that changes
                // gets a copy, and every other value is itself.
                once(place, &|e| {
                    let mut value = e.clone();
                    // Changed in place itself, through a `&mut`: every variant with fields.
                    let itself = self.mutated_itself(ty);
                    for variant in adt.variants().iter().rev() {
                        let fields = self.variant_fields(variant, args);
                        if fields.is_empty() || !(itself || fields.iter().any(|&(_, t)| self.contains_mutated(t))) {
                            continue;
                        }
                        let mut props = vec![Prop::Spread(e.clone())];
                        for (name, t) in fields {
                            if self.contains_mutated(t) {
                                props.push(Prop::Field(name.clone(), self.copy(Expr::member(e.clone(), name), t)));
                            }
                        }
                        let tag = Expr::bin(
                            Op::Eq,
                            Expr::member(e.clone(), "TAG"),
                            Expr::str(super::bindings::variant_name(self.tcx, variant)),
                        );
                        value = Expr::cond(tag, Expr::object(props), value);
                    }
                    value
                })
            }
        }
    }

    pub(super) fn num(&self, ty: Ty<'tcx>, span: Span) -> R<Num> {
        Num::of(ty).ok_or_else(|| self.unsupported(span, &format!("values of type `{ty}`")))
    }

    pub(super) fn check_value_ty(&self, ty: Ty<'tcx>, span: Span) -> R<()> {
        match self.unsupported_part(ty) {
            None => Ok(()),
            Some(part) => Err(self.unsupported(span, &format!("values of type `{part}`"))),
        }
    }

    /// The first type inside `ty` (or `ty` itself) that rust-js can't represent.
    pub(super) fn unsupported_part(&self, ty: Ty<'tcx>) -> Option<Ty<'tcx>> {
        self.unsupported_in(ty, &mut Vec::new())
    }

    /// `unsupported_part`, for a type inside the ones in `seen`. A type
    /// inside itself (`Tree` in `Node(Box<Tree>, ..)`) is being checked
    /// already, further out.
    pub(super) fn unsupported_in(&self, ty: Ty<'tcx>, seen: &mut Vec<Ty<'tcx>>) -> Option<Ty<'tcx>> {
        if ty.is_bool() || ty.is_unit() || ty.is_str() || ty.is_char() || Num::of(ty).is_some() || self.is_str_split(ty)
        {
            return None;
        }
        if self.is_array_iter(ty) {
            return None;
        }
        match ty.kind() {
            ty::Param(_) => return None,
            ty::Dynamic(predicates, ..)
                if predicates
                    .principal_def_id()
                    .is_some_and(|id| id.is_local() && self.readonly_dyn(id)) =>
            {
                return None;
            }
            // A JS value from an `extern` block, and closures: JS functions.
            ty::Foreign(_) | ty::Closure(..) | ty::CoroutineClosure(..) | ty::FnDef(..) | ty::FnPtr(..) => return None,
            // Futures are JS promises (ADR 0029): an `async` block, what an
            // `async fn` returns, and `dyn Future`.
            ty::Coroutine(..) => return None,
            ty::Alias(ty::Opaque, alias)
                if matches!(
                    self.tcx.opaque_ty_origin(alias.def_id),
                    hir::OpaqueTyOrigin::AsyncFn { .. }
                ) =>
            {
                return None;
            }
            // `impl Iterator<Item = u32>` is the type it hides (ADR 0061).
            ty::Alias(ty::Opaque, _) if self.reveal(ty) != ty => return self.unsupported_in(self.reveal(ty), seen),
            ty::Dynamic(traits, ..)
                if traits
                    .principal_def_id()
                    .is_some_and(|t| self.tcx.is_lang_item(t, LangItem::Future)) =>
            {
                return None;
            }
            // `&dyn Any` is any JS value, as the web crate's `object`
            // parameters take: a struct, say, which is a JS object already.
            ty::Dynamic(traits, ..)
                if traits
                    .principal_def_id()
                    .is_some_and(|t| self.tcx.is_diagnostic_item(Symbol::intern("Any"), t)) =>
            {
                return None;
            }
            ty::Adt(..) if self.is_js_object(ty) => return None,
            // `dyn Debug` is the string it shows (ADR 0060).
            ty::Dynamic(..) if self.is_dyn_debug(ty) => return None,
            // A `HashMap` or `HashSet` (ADR 0059): keys JS compares by value.
            ty::Adt(_, args) if self.is_map(ty) => {
                let key = args.type_at(0);
                if !self.is_key(key) {
                    return Some(key);
                }
                // A map's value; after it, and after a set's key, the hasher.
                let set = self.is_set(ty);
                return args
                    .types()
                    .skip(1)
                    .take(usize::from(!set))
                    .find_map(|t| self.unsupported_in(t, seen));
            }
            ty::Dynamic(traits, ..)
                if traits
                    .principal_def_id()
                    .is_some_and(|t| self.tcx.fn_trait_kind_from_def_id(t).is_some()) =>
            {
                return None;
            }
            ty::Ref(_, inner, Mutability::Not) => return self.unsupported_in(*inner, seen),
            // `&mut` to a JS object is the object; to anything else, it would
            // need a place to point at.
            ty::Ref(_, inner, Mutability::Mut) if self.is_object(*inner) => return self.unsupported_in(*inner, seen),
            ty::Array(elem, _) | ty::Slice(elem) => return self.unsupported_in(*elem, seen),
            ty::Adt(_, _) if self.is_lang_adt(ty, LangItem::String) => return None,
            // An `Option` is its value or `undefined` (ADR 0030), so the value
            // itself mustn't be able to look like `None`.
            ty::Adt(..) if let Some(inner) = self.option_of(ty) => {
                return if self.can_be_nullish(inner) && !self.boxed_payload(inner) {
                    Some(ty)
                } else {
                    self.unsupported_in(inner, seen)
                };
            }
            // `format_args!`'s pieces are strings by the time JS sees them.
            ty::Adt(_, _)
                if self.is_lang_adt(ty, LangItem::FormatArguments)
                    || self.is_lang_adt(ty, LangItem::FormatArgument) =>
            {
                return None;
            }
            // A guard held in a variable is the object it guards; a guarded
            // number would be a copy, not a place.
            ty::Adt(_, args)
                if ["RefCellRef", "RefCellRefMut"]
                    .into_iter()
                    .any(|name| self.is_std_adt(ty, Symbol::intern(name)))
                    && !args.types().next().is_some_and(|inner| self.is_object(inner)) =>
            {
                return Some(ty);
            }
            ty::Adt(_, args) if self.is_std_wrapper(ty) => {
                return args.types().next().and_then(|t| self.unsupported_in(t, seen));
            }
            // A thread-local is its value (ADR 0037).
            ty::Adt(_, args) if self.is_std_adt(ty, Symbol::intern("LocalKey")) => {
                return args.types().next().and_then(|t| self.unsupported_in(t, seen));
            }
            _ => {}
        }
        if seen.contains(&ty) {
            return None;
        }
        seen.push(ty);
        let found = match (ty.kind(), self.shape(ty)) {
            // An enum with fields (ADR 0033): every variant's fields.
            (ty::Adt(adt, args), _) if adt.is_enum() => {
                let fields: Vec<Ty<'tcx>> = adt.all_fields().map(|f| f.ty(self.tcx, args)).collect();
                fields.into_iter().find_map(|t| self.unsupported_in(t, seen))
            }
            (_, Shape::Object(fields)) => fields.iter().find_map(|&(_, t)| self.unsupported_in(t, seen)),
            (_, Shape::Array(tys)) => tys.iter().find_map(|&t| self.unsupported_in(t, seen)),
            (ty::Adt(adt, _), Shape::Other) if adt.is_struct() => None, // a unit struct
            _ => Some(ty),
        };
        seen.pop();
        found
    }

    /// A binding by value, or by `ref`: a reference is the value itself
    /// (ADR 0023), which rustc keeps from changing while it's borrowed. A `ref
    /// mut` works where `&mut` does, to an object (ADR 0025).
    pub(super) fn check_by_value(&self, mode: BindingMode, ty: Ty<'tcx>, span: Span) -> R<()> {
        match mode.0 {
            ByRef::No | ByRef::Yes(_, Mutability::Not) => Ok(()),
            ByRef::Yes(_, Mutability::Mut) if self.is_object(ty.peel_refs()) => Ok(()),
            ByRef::Yes(_, Mutability::Mut) => Err(self.unsupported(span, "`ref mut` bindings to this type")),
        }
    }
}

/// Number representations. Every one of them is a plain JS number; the
/// difference is how results are wrapped back into range.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Num {
    I8,
    I16,
    I32,
    U8,
    U16,
    U32,
    F64,
}

impl Num {
    pub(super) fn of(ty: Ty<'_>) -> Option<Num> {
        Some(match ty.kind() {
            ty::Int(ty::IntTy::I8) => Num::I8,
            ty::Int(ty::IntTy::I16) => Num::I16,
            // `isize` and `usize` are 32 bits, as on wasm32 (ADR 0025).
            ty::Int(ty::IntTy::I32 | ty::IntTy::Isize) => Num::I32,
            ty::Uint(ty::UintTy::U8) => Num::U8,
            ty::Uint(ty::UintTy::U16) => Num::U16,
            ty::Uint(ty::UintTy::U32 | ty::UintTy::Usize) => Num::U32,
            ty::Float(ty::FloatTy::F64) => Num::F64,
            _ => return None,
        })
    }

    pub(super) fn bits(self) -> u32 {
        match self {
            Num::I8 | Num::U8 => 8,
            Num::I16 | Num::U16 => 16,
            Num::I32 | Num::U32 => 32,
            Num::F64 => 64,
        }
    }

    pub(super) fn signed(self) -> bool {
        matches!(self, Num::I8 | Num::I16 | Num::I32)
    }

    /// The inclusive value range, for integers.
    pub(super) fn range(self) -> (i128, i128) {
        let bits = self.bits();
        if self.signed() {
            (-(1 << (bits - 1)), (1 << (bits - 1)) - 1)
        } else {
            (0, (1 << bits) - 1)
        }
    }

    /// Wrap an exact JS result back into this type's range, like Rust's
    /// wrapping arithmetic: `x | 0` for i32, `x >>> 0` for u32, and so on.
    pub(super) fn wrap(self, e: Expr) -> Expr {
        // A constant is wrapped here, not in the JS: `Code::NotFound as u32`
        // is `404`, not `(404 + 0 | 0) >>> 0`.
        if self != Num::F64
            && let Some(n) = const_int(&e)
        {
            let size = 1i128 << self.bits();
            let wrapped = n.rem_euclid(size);
            return Expr::int(if self.signed() && wrapped >= size / 2 {
                wrapped - size
            } else {
                wrapped
            });
        }
        match self {
            Num::I32 => Expr::bin(Op::BitOr, e, Expr::num(0)),
            Num::U32 => Expr::bin(Op::UShr, e, Expr::num(0)),
            Num::I8 | Num::I16 => {
                let shift = 32 - self.bits();
                Expr::bin(Op::Shr, Expr::bin(Op::Shl, e, Expr::num(shift)), Expr::num(shift))
            }
            Num::U8 | Num::U16 => Expr::bin(Op::BitAnd, e, Expr::int(self.range().1)),
            Num::F64 => e,
        }
    }
}

/// An integer the JS computes from constants alone: `404 + 0`.
fn const_int(e: &Expr) -> Option<i128> {
    Some(match &e.kind {
        js::ExprKind::Num(n) if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 => *n as i128,
        js::ExprKind::Unary(js::UnaryOp::Neg, x) => -const_int(x)?,
        js::ExprKind::Binary(op, a, b) => {
            let (a, b) = (const_int(a)?, const_int(b)?);
            match op {
                Op::Add => a.checked_add(b)?,
                Op::Sub => a.checked_sub(b)?,
                Op::Mul => a.checked_mul(b)?,
                _ => return None,
            }
        }
        _ => return None,
    })
}

pub(super) fn is_fieldless_enum(adt: ty::AdtDef<'_>) -> bool {
    adt.is_enum() && adt.variants().iter().all(|v| v.fields.is_empty())
}

/// Turn raw constant bits into a JS number literal.
/// What rustc computed for a `const`, as a value tree (ADR 0031).
pub(super) fn eval_const<'tcx>(
    tcx: TyCtxt<'tcx>,
    typing_env: ty::TypingEnv<'tcx>,
    def_id: DefId,
    args: ty::GenericArgsRef<'tcx>,
    span: Span,
) -> Option<ty::Value<'tcx>> {
    let instance = ty::Instance::try_resolve(tcx, typing_env, def_id, args).ok()??;
    let valtree = tcx
        .const_eval_global_id_for_typeck(
            typing_env,
            GlobalId {
                instance,
                promoted: None,
            },
            span,
        )
        .ok()?
        .ok()?;
    let ty = tcx.type_of(def_id).instantiate(tcx, args);
    Some(ty::Value {
        ty: tcx.normalize_erasing_regions(typing_env, ty),
        valtree,
    })
}

/// A constant value as a JS literal, in the shapes of ADRs 0011, 0013, 0020
/// and 0030: numbers, strings, `{ x: 0, y: 0 }`, `[a, b]`, `"High"`,
/// `undefined` for `None`.
pub(super) fn const_js<'tcx>(tcx: TyCtxt<'tcx>, value: ty::Value<'tcx>) -> Option<Expr> {
    let ty = value.ty;
    if ty.is_bool() {
        return value.try_to_bool().map(Expr::bool);
    }
    if let Some(num) = Num::of(ty) {
        return Some(num_literal(value.try_to_leaf()?.to_bits_unchecked(), num));
    }
    if let Some(c) = char_value(value) {
        return Some(Expr::str(c.to_string()));
    }
    // An enum's value tree starts with its variant's index, then its fields.
    let children = || -> Option<Vec<ty::Value<'tcx>>> {
        match &**value.valtree {
            ty::ValTreeKind::Branch(items) => items.iter().map(|c| c.try_to_value()).collect(),
            ty::ValTreeKind::Leaf(_) => None,
        }
    };
    let all = |values: &[ty::Value<'tcx>]| values.iter().map(|&v| const_js(tcx, v)).collect::<Option<Vec<_>>>();
    match ty.kind() {
        ty::Ref(_, inner, _) if inner.is_str() => {
            Some(Expr::str(std::str::from_utf8(value.try_to_raw_bytes(tcx)?).ok()?))
        }
        ty::Ref(_, inner, _) => const_js(
            tcx,
            ty::Value {
                ty: *inner,
                valtree: value.valtree,
            },
        ),
        ty::Tuple(items) if items.is_empty() => Some(Expr::undefined()),
        ty::Tuple(_) | ty::Array(..) | ty::Slice(_) => Some(Expr::array(all(&children()?)?)),
        ty::Adt(adt, _) if adt.is_enum() => {
            let items = children()?;
            let (index, fields) = items.split_first()?;
            let variant = adt.variant(index.try_to_leaf()?.to_u32().into());
            if tcx.is_lang_item(adt.did(), LangItem::Option) {
                return match fields.first() {
                    Some(&inner) => const_js(tcx, inner),
                    None => Some(Expr::undefined()),
                };
            }
            if let Some(n) = ordering_value(tcx, adt.did(), variant.name) {
                return Some(Expr::int(n));
            }
            if fields.is_empty() {
                return Some(Expr::str(super::bindings::variant_name(tcx, variant)));
            }
            let values = all(fields)?;
            let props = values
                .into_iter()
                .enumerate()
                .map(|(i, v)| Prop::Field(variant_field(tcx, variant, i), v));
            Some(Expr::object(
                std::iter::once(Prop::Field("TAG".into(), Expr::str(variant.name.to_string())))
                    .chain(props)
                    .collect(),
            ))
        }
        ty::Adt(adt, _) if adt.is_struct() => {
            let variant = adt.non_enum_variant();
            let values = all(&children()?)?;
            match variant.ctor_kind() {
                Some(CtorKind::Const) => Some(Expr::undefined()),
                Some(CtorKind::Fn) => Some(Expr::array(values)),
                None => Some(Expr::object(
                    variant
                        .fields
                        .iter()
                        .zip(values)
                        .map(|(f, v)| Prop::Field(field_key(tcx, f), v))
                        .collect(),
                )),
            }
        }
        _ => None,
    }
}

/// The JS property for field `i` of an enum variant (ADR 0033): `_0` in a
/// tuple variant, as in ReScript, and its name in a struct variant.
pub(super) fn variant_field(tcx: TyCtxt<'_>, variant: &ty::VariantDef, i: usize) -> String {
    match variant.ctor_kind() {
        Some(CtorKind::Fn) => format!("_{i}"),
        _ => field_key(tcx, variant.fields.iter().nth(i).expect("a field of this variant")),
    }
}

/// An `Ordering` is -1, 0 or 1 (ADR 0036), its discriminant, which a JS
/// comparator returns as it is.
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

/// A `char` constant (ADR 0034).
pub(super) fn char_value(value: ty::Value<'_>) -> Option<char> {
    if !value.ty.is_char() {
        return None;
    }
    char::from_u32(value.try_to_leaf()?.to_u32())
}

pub(super) fn num_literal(bits: u128, num: Num) -> Expr {
    if num == Num::F64 {
        return Expr::num(f64::from_bits(bits as u64));
    }
    let unused = 128 - num.bits();
    let n = if num.signed() {
        ((bits << unused) as i128) >> unused
    } else {
        bits as i128
    };
    Expr::int(n)
}
