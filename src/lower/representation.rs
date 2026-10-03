//! Rust value representations, copying, and supported-type validation.

use super::bindings::field_key;
use super::{FnCx, R, Shape};
use crate::js;
use crate::js::{Expr, Op, Prop};
use rustc_ast::Mutability;
use rustc_hir::LangItem;
use rustc_hir::def::{CtorKind, DefKind};
use rustc_middle::mir::ConstValue;
use rustc_middle::mir::interpret::{AllocId, ConstAllocation, GlobalAlloc, GlobalId, Pointer, Scalar, alloc_range};
use rustc_middle::ty;
use rustc_middle::ty::Ty;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::{DefId, LocalDefId};
use rustc_span::{Span, Symbol, sym};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
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

    /// A type only a caller knows: a type parameter, or an associated type of
    /// one, `<S as Source>::Item`, that isn't a type here (ADR 0106). Its
    /// dictionaries are the ones a function is given. One that is a type here,
    /// `<Count as Source>::Item` in an impl's signature, is that type.
    pub(super) fn is_unknown(&self, ty: Ty<'tcx>) -> bool {
        match ty.kind() {
            ty::Param(_) => true,
            ty::Alias(
                _,
                ty::AliasTy {
                    kind: ty::Projection { .. },
                    ..
                },
            ) => self
                .tcx
                .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(ty))
                .map_or(true, |known| {
                    matches!(
                        known.kind(),
                        ty::Param(_)
                            | ty::Alias(
                                _,
                                ty::AliasTy {
                                    kind: ty::Projection { .. },
                                    ..
                                }
                            )
                    )
                }),
            _ => false,
        }
    }

    /// An `Option<T>` whose `T` might look like `None` only because it's a
    /// type parameter: `Some` of it is boxed when it does (ADR 0051).
    pub(super) fn boxed_payload(&self, ty: Ty<'tcx>) -> bool {
        self.is_unknown(self.payload(ty))
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

    /// Is a `ty` value a JS object? Then a reference to it, even `&mut`, can
    /// be the object itself: changes through it change the one object (ADR 0025).
    /// A value that a `&mut` to must be a box to change (ADR 0072): one that
    /// isn't a JS object, as a `String`, a number or a fieldless enum is.
    pub(super) fn is_boxable(&self, ty: Ty<'tcx>) -> bool {
        !ty.is_ref()
            && !self.is_unknown(ty)
            && !self.is_object(ty)
            // A `&mut dyn FnMut()` is the function (ADR 0099).
            && !self.is_callable(ty)
            && self.unsupported_part(ty).is_none()
    }

    /// A type parameter a `&mut` is to, `&mut T` or `&mut Self`, where `param_env`
    /// says what it's bound by: a box, whatever it is (ADR 0099), since generic
    /// code is compiled once and `T` may be a number. Not one that's a JS
    /// function or a JS iterator already: a closure, whose `&mut` is the
    /// closure, or an iterator (ADR 0061).
    pub(super) fn is_generic_boxed(&self, ty: Ty<'tcx>, param_env: ty::ParamEnv<'tcx>) -> bool {
        self.is_unknown(ty)
            && !self.bound_by(ty, param_env, |id| {
                self.tcx.fn_trait_kind_from_def_id(id).is_some()
                    || self.tcx.is_diagnostic_item(sym::Iterator, id)
                    || self.tcx.is_diagnostic_item(sym::IntoIterator, id)
            })
    }

    /// Is `ty`, a type parameter, bound by a trait `which` says, in `param_env`?
    fn bound_by(&self, ty: Ty<'tcx>, param_env: ty::ParamEnv<'tcx>, which: impl Fn(DefId) -> bool) -> bool {
        param_env.caller_bounds().iter().any(|clause| {
            clause
                .as_trait_clause()
                .is_some_and(|tr| tr.self_ty().skip_binder() == ty && which(tr.def_id()))
        })
    }

    /// A JS function: a closure, a function, a `dyn Fn` and the like, or a
    /// type parameter bound by one of them, an `impl FnMut` too. A `&mut` to
    /// one is the function (ADR 0099): calling it changes what it captured,
    /// as calling it through the `&mut` does.
    pub(super) fn is_callable(&self, ty: Ty<'tcx>) -> bool {
        let fn_trait = |id: DefId| self.tcx.fn_trait_kind_from_def_id(id).is_some();
        match ty.kind() {
            ty::Closure(..) | ty::FnDef(..) | ty::FnPtr(..) => true,
            ty::Dynamic(predicates, ..) => predicates.principal_def_id().is_some_and(fn_trait),
            _ if self.is_unknown(ty) => self.bound_by(ty, self.typing_env.param_env, fn_trait),
            _ => false,
        }
    }

    /// What a `&mut` to is a cell: a value JS can't change in place, or a
    /// type parameter, which may be one (ADR 0099).
    pub(super) fn is_cell_pointee(&self, ty: Ty<'tcx>) -> bool {
        self.is_boxable(ty) || self.is_generic_boxed(ty, self.typing_env.param_env)
    }

    /// Is a `&mut` made here, to a place of type `pointee`, a cell? In a trait's
    /// default copied into an impl, `Self` is the impl's type: `&mut self` of
    /// an object `Self` is the object, though a `&mut Self` parameter is the
    /// dictionary's box (ADR 0099).
    pub(super) fn makes_cell(&self, pointee: Ty<'tcx>) -> bool {
        let pointee = match self.given.self_args {
            Some(args) => ty::EarlyBinder::bind(self.tcx, pointee)
                .instantiate(self.tcx, args)
                .skip_normalization(),
            None => pointee,
        };
        self.is_cell_pointee(pointee)
    }

    /// A `&mut` to a value JS can't change in place, as a value: a cell, a box
    /// or a handle, whose `value` is the place it points at (ADR 0099).
    pub(super) fn is_cell(&self, ty: Ty<'tcx>) -> bool {
        matches!(*ty.kind(), ty::Ref(_, inner, Mutability::Mut) if self.is_cell_pointee(inner))
    }

    /// A value of type `ty` seen through its references, as the code that
    /// shows, compares or orders one reads it: a `&` is the value (ADR 0023),
    /// a `&mut` to a value JS can't change in place a cell, whose `value` it
    /// is (ADR 0099), or the place of a `Handle` on one.
    pub(super) fn through_refs(&self, mut value: Expr, mut ty: Ty<'tcx>) -> (Expr, Ty<'tcx>) {
        while let ty::Ref(_, inner, mutability) = *ty.kind() {
            if mutability == Mutability::Mut && self.is_cell_pointee(inner) {
                value = match value.kind {
                    js::ExprKind::Handle(place) => *place,
                    _ => Expr::member(value, "value"),
                };
            }
            ty = inner;
        }
        (value, ty)
    }

    /// Is one of the references `ty` is behind a `&mut` cell, so that its JS
    /// value is an object, not what it points at (ADR 0099)?
    pub(super) fn has_cell_layer(&self, mut ty: Ty<'tcx>) -> bool {
        while let ty::Ref(_, inner, mutability) = *ty.kind() {
            if mutability == Mutability::Mut && self.is_cell_pointee(inner) {
                return true;
            }
            ty = inner;
        }
        false
    }

    /// Is `fn_id`'s parameter `i` a box: a `&mut` to a value JS can't change in
    /// place (ADR 0074), or to a type parameter (ADR 0099)?
    pub(super) fn param_is_box(&self, fn_id: DefId, i: usize) -> bool {
        let inputs = self
            .tcx
            .fn_sig(fn_id)
            .instantiate_identity()
            .skip_normalization()
            .skip_binder()
            .inputs();
        match inputs.get(i).map(|input| *input.kind()) {
            Some(ty::Ref(_, inner, Mutability::Mut)) => {
                self.is_boxable(inner) || self.is_generic_boxed(inner, self.tcx.param_env(fn_id))
            }
            _ => false,
        }
    }

    pub(super) fn is_object(&self, ty: Ty<'tcx>) -> bool {
        matches!(self.shape(ty), Shape::Object(_) | Shape::Array(_))
            || self.is_js_object(ty)
            // A slice or an array is a JS array: `&mut` to one, as `sort` takes, is it.
            || ty.is_slice()
            // A `dyn Iterator` is a JS iterator, which steps itself.
            || self.recognition().is_dyn_iter(ty)
            || ty.is_array()
            || ["Cell", "RefCell", "Atomic"].into_iter().any(|name| self.is_std_adt(ty, Symbol::intern(name)))
            || self.is_vec_like(ty)
            || self.is_map(ty)
            // A `dyn` of the crate's trait is its pair (ADR 0049): a `&mut` to
            // one is the pair, whose `value` its `&mut self` methods write.
            || matches!(ty.kind(), ty::Dynamic(..)) && self.dynamic_trait(ty).is_some()
            // An enum with fields: those variants are objects (ADR 0033). A
            // fieldless one's string can't be changed through a `&mut` anyway,
            // since `*r = ..` of a whole value isn't supported. `Option` is
            // its value itself (ADR 0030), not an object.
            || matches!(ty.kind(), ty::Adt(adt, _) if adt.is_enum()
                && !self.tcx.is_lang_item(adt.did(), LangItem::Option)
                && adt.variants().iter().any(|v| !v.fields.is_empty()))
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
            .map(|(i, f)| (variant_field(self.tcx, variant, i), self.field_ty(f, args)))
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
                    .map(|f| (field_key(self.tcx, f), self.field_ty(f, args)));
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

    pub(super) fn num(&self, ty: Ty<'tcx>, span: Span) -> R<Num> {
        Num::of(ty).ok_or_else(|| self.unsupported(span, &format!("values of type `{ty}`")))
    }
}

/// Number representations. Up to 32 bits, `f32` and `f64`, a plain JS number,
/// and `i64` and `u64`, a BigInt (ADR 0086); the difference is how results
/// are wrapped back into range, or for an `f32`, rounded to one (ADR 0122).
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Num {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

/// An `f32` as JS writes it: its shortest digits, `1.5`, where they're the
/// number exactly, and else rounded to it, `Math.fround(0.1)` (ADR 0122).
pub(super) fn f32_literal(v: f32) -> Expr {
    let shortest: f64 = format!("{v:e}").parse().expect("Rust writes a float it reads");
    if !v.is_finite() || shortest == f64::from(v) {
        return Expr::num(f64::from(v));
    }
    Expr::call(Expr::member(Expr::var("Math"), "fround"), vec![Expr::num(shortest)])
}

impl Num {
    pub(super) fn of(ty: Ty<'_>) -> Option<Num> {
        Some(match ty.kind() {
            ty::Int(ty::IntTy::I8) => Num::I8,
            ty::Int(ty::IntTy::I16) => Num::I16,
            // `isize` and `usize` are 32 bits, as on wasm32 (ADR 0025).
            ty::Int(ty::IntTy::I32 | ty::IntTy::Isize) => Num::I32,
            ty::Int(ty::IntTy::I64) => Num::I64,
            ty::Uint(ty::UintTy::U8) => Num::U8,
            ty::Uint(ty::UintTy::U16) => Num::U16,
            ty::Uint(ty::UintTy::U32 | ty::UintTy::Usize) => Num::U32,
            ty::Uint(ty::UintTy::U64) => Num::U64,
            ty::Float(ty::FloatTy::F32) => Num::F32,
            ty::Float(ty::FloatTy::F64) => Num::F64,
            _ => return None,
        })
    }

    pub(super) fn bits(self) -> u32 {
        match self {
            Num::I8 | Num::U8 => 8,
            Num::I16 | Num::U16 => 16,
            Num::I32 | Num::U32 | Num::F32 => 32,
            Num::I64 | Num::U64 | Num::F64 => 64,
        }
    }

    /// An `f32` or an `f64`: a JS number, not an integer (ADR 0122).
    pub(super) fn float(self) -> bool {
        matches!(self, Num::F32 | Num::F64)
    }

    pub(super) fn signed(self) -> bool {
        matches!(self, Num::I8 | Num::I16 | Num::I32 | Num::I64)
    }

    /// An `i64` or a `u64`: a JS BigInt, which mixes only with another.
    pub(super) fn big(self) -> bool {
        matches!(self, Num::I64 | Num::U64)
    }

    /// `n` as a literal of this type: `5`, or `5n`.
    pub(super) fn literal(self, n: i128) -> Expr {
        if self.big() { Expr::bigint(n) } else { Expr::int(n) }
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
        // An `f32` constant is rounded here too.
        if self == Num::F32
            && let js::ExprKind::Num(n) = e.kind
        {
            return f32_literal(n as f32);
        }
        if !self.float()
            && let Some(n) = const_int(&e)
        {
            let size = 1i128 << self.bits();
            let wrapped = n.rem_euclid(size);
            return self.literal(if self.signed() && wrapped >= size / 2 {
                wrapped - size
            } else {
                wrapped
            });
        }
        let as_n = |name: &str| Expr::call(Expr::member(Expr::var("BigInt"), name), vec![Expr::int(64), e.clone()]);
        match self {
            Num::I64 => as_n("asIntN"),
            Num::U64 => as_n("asUintN"),
            Num::I32 => Expr::bin(Op::BitOr, e, Expr::num(0)),
            Num::U32 => Expr::bin(Op::UShr, e, Expr::num(0)),
            Num::I8 | Num::I16 => {
                let shift = 32 - self.bits();
                Expr::bin(Op::Shr, Expr::bin(Op::Shl, e, Expr::num(shift)), Expr::num(shift))
            }
            Num::U8 | Num::U16 => Expr::bin(Op::BitAnd, e, Expr::int(self.range().1)),
            // The nearest `f32`: of an exact result, Rust's (ADR 0122).
            Num::F32 => Expr::call(Expr::member(Expr::var("Math"), "fround"), vec![e]),
            Num::F64 => e,
        }
    }
}

/// An integer the JS computes from constants alone: `404 + 0`.
fn const_int(e: &Expr) -> Option<i128> {
    Some(match &e.kind {
        js::ExprKind::Num(n) if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 => *n as i128,
        js::ExprKind::BigInt(n) => *n,
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
    let args = tcx
        .try_normalize_erasing_regions(typing_env, ty::Unnormalized::new_wip(args))
        .ok()?;
    let instance = ty::Instance::try_resolve(tcx, typing_env, def_id, args).ok()??;
    // The query itself, not `const_eval_global_id_for_typeck`, which
    // reports a constant too large for a value tree as rustc's own error
    // where rust-js reports it as unsupported.
    let cid = GlobalId {
        instance,
        promoted: None,
    };
    let inputs = tcx.erase_and_anonymize_regions(typing_env.with_post_analysis_normalized(tcx).as_query_input(cid));
    let valtree = tcx.at(span).eval_to_valtree(inputs).ok()?;
    let ty = tcx.type_of(def_id).instantiate(tcx, args).skip_normalization();
    Some(ty::Value {
        ty: tcx.normalize_erasing_regions(typing_env, ty::Unnormalized::new_wip(ty)),
        valtree,
    })
}

/// What rustc computed for a `static` (ADR 0096), as a `const`'s value tree.
/// rustc makes value trees only of constants, since a static is a place, so
/// this reads one out of the static's memory. None for a value no value tree
/// holds: a reference to another static, a pointer, a `dyn`.
pub(super) fn static_value<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> Option<ty::Value<'tcx>> {
    let memory = tcx.eval_static_initializer(def_id).ok()?;
    let ty = tcx.type_of(def_id).instantiate_identity().skip_normalization();
    let valtree = valtree_at(tcx, Pointer::from(unchanging(tcx, memory)), ty)?;
    Some(ty::Value { ty, valtree })
}

/// Memory rustc reads for a value tree only if it can't change, as a `static
/// mut`'s or an atomic's can: a copy of it, marked so.
fn unchanging<'tcx>(tcx: TyCtxt<'tcx>, memory: ConstAllocation<'tcx>) -> AllocId {
    let mut copy = memory.inner().clone();
    copy.mutability = Mutability::Not;
    tcx.reserve_and_set_memory_alloc(tcx.mk_const_alloc(copy))
}

/// Where a pointer in a static's memory points, readable. A `&` in one is
/// to memory of its own, a nested static; one to another static is `None`.
fn pointee(tcx: TyCtxt<'_>, pointer: Scalar) -> Option<Pointer> {
    let pointer = pointer.to_pointer(&tcx).discard_err()?.into_pointer_or_addr().ok()?;
    let (prov, offset) = pointer.prov_and_relative_offset();
    let memory = match tcx.try_get_global_alloc(prov.alloc_id())? {
        GlobalAlloc::Memory(memory) if memory.inner().mutability == Mutability::Not => return Some(pointer),
        GlobalAlloc::Memory(memory) => memory,
        GlobalAlloc::Static(d) if matches!(tcx.def_kind(d), DefKind::Static { nested: true, .. }) => {
            tcx.eval_static_initializer(d).ok()?
        }
        _ => return None,
    };
    Some(Pointer::new(unchanging(tcx, memory).into(), offset))
}

/// The value tree of the `ty` at `at`, as rustc makes one for a constant.
fn valtree_at<'tcx>(tcx: TyCtxt<'tcx>, at: Pointer, ty: Ty<'tcx>) -> Option<ty::ValTree<'tcx>> {
    let (prov, offset) = at.prov_and_relative_offset();
    valtree_of(
        tcx,
        ConstValue::Indirect {
            alloc_id: prov.alloc_id(),
            offset,
        },
        ty,
    )
}

fn valtree_of<'tcx>(tcx: TyCtxt<'tcx>, value: ConstValue, ty: Ty<'tcx>) -> Option<ty::ValTree<'tcx>> {
    let typing_env = ty::TypingEnv::fully_monomorphized();
    let word = tcx.data_layout.pointer_size();
    let read = |words: u64, size, provenance| {
        let ConstValue::Indirect { alloc_id, offset } = value else {
            return None;
        };
        let memory = tcx.global_alloc(alloc_id).unwrap_memory();
        memory
            .inner()
            .read_scalar(&tcx, alloc_range(offset + word * words, size), provenance)
            .ok()
    };
    match ty.kind() {
        ty::Bool | ty::Char | ty::Int(_) | ty::Uint(_) | ty::Float(_) => {
            let scalar = match value {
                ConstValue::Scalar(scalar) => scalar,
                _ => read(0, tcx.layout_of(typing_env.as_query_input(ty)).ok()?.size, false)?,
            };
            Some(ty::ValTree::from_scalar_int(tcx, scalar.try_to_scalar_int().ok()?))
        }
        // A reference's value tree is its referent's. A `&str` or a `&[T]`
        // is two words: where its items are, and how many.
        ty::Ref(_, inner, _) if matches!(inner.kind(), ty::Str | ty::Slice(_)) => {
            let (data, len) = match value {
                ConstValue::Slice { alloc_id, meta } => (Scalar::from_pointer(alloc_id.into(), &tcx), meta),
                _ => (
                    read(0, word, true)?,
                    read(1, word, false)?.to_target_usize(&tcx).discard_err()?,
                ),
            };
            let len_usize = usize::try_from(len).ok()?;
            match *inner.kind() {
                ty::Str if len == 0 => Some(ty::ValTree::from_raw_bytes(tcx, &[])),
                ty::Str => {
                    let (prov, offset) = pointee(tcx, data)?.prov_and_relative_offset();
                    let memory = tcx.global_alloc(prov.alloc_id()).unwrap_memory();
                    let start = usize::try_from(offset.bytes()).ok()?;
                    let bytes = memory
                        .inner()
                        .inspect_with_uninit_and_ptr_outside_interpreter(start..start + len_usize);
                    Some(ty::ValTree::from_raw_bytes(tcx, bytes))
                }
                ty::Slice(_) if len == 0 => Some(ty::ValTree::zst(tcx)),
                ty::Slice(item) => valtree_at(tcx, pointee(tcx, data)?, Ty::new_array(tcx, item, len)),
                _ => None,
            }
        }
        ty::Ref(_, inner, _) if inner.is_sized(tcx, typing_env) => {
            let pointer = match value {
                ConstValue::Scalar(scalar) => scalar,
                _ => read(0, word, true)?,
            };
            valtree_at(tcx, pointee(tcx, pointer)?, *inner)
        }
        ty::Adt(adt, _) if adt.is_union() => None,
        ty::Array(..) | ty::Tuple(_) | ty::Adt(..) => {
            let parts = tcx.try_destructure_mir_constant_for_user_output(value, ty)?;
            // An enum's starts with its variant's index, as rustc's own does.
            // (The destructured value has one for a struct too.)
            let variant = parts.variant.filter(|_| ty.is_enum()).map(|v| {
                ty::Const::new_value(tcx, ty::ValTree::from_scalar_int(tcx, v.as_u32().into()), tcx.types.u32)
            });
            let fields = parts
                .fields
                .iter()
                .map(|&(field, ty)| Some(ty::Const::new_value(tcx, valtree_of(tcx, field, ty)?, ty)))
                .collect::<Option<Vec<_>>>()?;
            Some(ty::ValTree::from_branches(tcx, variant.into_iter().chain(fields)))
        }
        _ => None,
    }
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
        // A std struct's fields are its own, not the JS value rust-js makes
        // of it: `iter::empty()` is no `[undefined]`. Only a `PhantomData`, and
        // `Reverse(x)`, `[x]`, are their fields.
        // A `Cell` or a `RefCell` is `{ value }`, what its `UnsafeCell` holds,
        // as one made at run time is.
        ty::Adt(adt, _)
            if ["Cell", "RefCell"]
                .iter()
                .any(|name| tcx.is_diagnostic_item(Symbol::intern(name), adt.did())) =>
        {
            let at = adt
                .non_enum_variant()
                .fields
                .iter()
                .position(|f| f.name == sym::value)?;
            let unsafe_cell = *children()?.get(at)?;
            let inner = match &*unsafe_cell.valtree {
                ty::ValTreeKind::Branch(items) => items.first()?.try_to_value()?,
                ty::ValTreeKind::Leaf(_) => return None,
            };
            Some(Expr::object(vec![Prop::Field("value".into(), const_js(tcx, inner)?)]))
        }
        // An atomic is `{ value }` too (ADR 0096). What it holds is stored
        // as an integer of its size, in a struct that aligns it.
        ty::Adt(adt, args) if tcx.is_diagnostic_item(Symbol::intern("Atomic"), adt.did()) => {
            let mut stored = value.valtree;
            while let ty::ValTreeKind::Branch(items) = &**stored {
                let [only] = &items[..] else {
                    return None;
                };
                stored = only.try_to_value()?.valtree;
            }
            let inner = const_js(
                tcx,
                ty::Value {
                    ty: args.type_at(0),
                    valtree: stored,
                },
            )?;
            Some(Expr::object(vec![Prop::Field("value".into(), inner)]))
        }
        ty::Adt(adt, _) if adt.is_struct() && !super::recognition::struct_is_its_fields(tcx, adt.did()) => None,
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
pub(super) use super::recognition::ordering_value;

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
    if num == Num::F32 {
        return f32_literal(f32::from_bits(bits as u32));
    }
    let unused = 128 - num.bits();
    let n = if num.signed() {
        ((bits << unused) as i128) >> unused
    } else {
        bits as i128
    };
    num.literal(n)
}
