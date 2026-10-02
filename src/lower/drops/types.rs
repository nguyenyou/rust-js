//! What dropping a type runs (ADR 0098): nothing JS can see, a user `Drop`,
//! its own or a part's, or what rust-js can't run yet, and why. A type is
//! walked once.

use rustc_hir::LangItem;
use rustc_middle::thir::LocalVarId;
use rustc_middle::ty::{self, Ty};
use rustc_span::Symbol;
use rustc_span::def_id::DefId;

use super::super::FnCx;

/// What dropping a type runs.
#[derive(Clone, Copy, PartialEq)]
pub(in crate::lower) enum Drops<'tcx> {
    /// Nothing JS can see: no user `Drop` anywhere in it.
    Nothing,
    /// A user `Drop`, its own or a part's.
    Runs,
    /// One where rust-js can't run it yet, and what to say.
    Unsupported(Ty<'tcx>, &'static str),
}

/// The types a `drops` walk is inside, outermost first, and the outermost
/// of them that a type inside it was found inside of.
struct Walk<'tcx> {
    seen: Vec<Ty<'tcx>>,
    reached: usize,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// Is `drop`, a type's `Drop::drop`, one rust-js runs: the crate's own, or
    /// a library's it exports (ADR 0100)?
    pub(in crate::lower) fn runs_drop(&self, drop: DefId) -> bool {
        drop.is_local() || self.krate.foreign.item(drop).is_some()
    }

    /// What dropping a `ty` runs.
    /// `traits::may_have_destructors`, of this crate.
    pub(in crate::lower) fn may_have_destructors(&self) -> bool {
        super::super::traits::may_have_destructors(self.tcx, self.krate.foreign)
    }

    pub(in crate::lower) fn drops(&self, ty: Ty<'tcx>) -> Drops<'tcx> {
        self.drops_in(
            ty,
            &mut Walk {
                seen: Vec::new(),
                reached: usize::MAX,
            },
        )
    }

    pub(in crate::lower) fn has_drops(&self, ty: Ty<'tcx>) -> bool {
        self.drops(ty) == Drops::Runs
    }

    /// What a closure takes by value that has a destructor: each variable,
    /// and its type, in the order its drop drops them. None if it takes
    /// part of one, or is another crate's.
    pub(in crate::lower) fn held(&self, closure: DefId) -> Option<Vec<(LocalVarId, Ty<'tcx>)>> {
        let local = closure.as_local()?;
        let mut held = Vec::new();
        for captured in self.tcx.closure_captures(local) {
            let ty = captured.place.ty();
            if captured.is_by_ref() || self.drops(ty) == Drops::Nothing {
                continue;
            }
            if !captured.place.projections.is_empty() {
                return None;
            }
            held.push((LocalVarId(captured.get_root_variable()), ty));
        }
        Some(held)
    }

    fn takes_whole(&self, closure: DefId) -> bool {
        self.held(closure).is_some()
    }

    /// Each type is walked once and cached, so a type whose parts double at
    /// each level, `S2<S2<T>>` in `S3<T>`, isn't walked once for each path to
    /// it. What's found while taking a type further out as running nothing,
    /// being inside itself, is only as sure as that type's walk, so it's
    /// cached only once that one's done.
    fn drops_in(&self, ty: Ty<'tcx>, walk: &mut Walk<'tcx>) -> Drops<'tcx> {
        let ty = self.reveal(ty);
        // What drops nothing at all, a number or a `&T`, and what's being
        // walked further out: a type inside itself runs no more than it does.
        if !ty.needs_drop(self.tcx, self.typing_env) {
            return Drops::Nothing;
        }
        if let Some(at) = walk.seen.iter().position(|&t| t == ty) {
            walk.reached = walk.reached.min(at);
            return Drops::Nothing;
        }
        if let Some(&known) = self.drop_state.cache.borrow().get(&ty) {
            return known;
        }
        let depth = walk.seen.len();
        let outer = std::mem::replace(&mut walk.reached, usize::MAX);
        walk.seen.push(ty);
        let std = |name: &str| self.is_std_adt(ty, Symbol::intern(name));
        let all = |cx: &Self, tys: &mut dyn Iterator<Item = Ty<'tcx>>, walk: &mut Walk<'tcx>| {
            let mut found = Drops::Nothing;
            for t in tys {
                match cx.drops_in(t, walk) {
                    Drops::Nothing => {}
                    Drops::Runs => found = Drops::Runs,
                    unsupported => return unsupported,
                }
            }
            found
        };
        let found = match ty.kind() {
            ty::Tuple(items) => all(self, &mut items.iter(), walk),
            ty::Array(item, _) | ty::Slice(item) => self.drops_in(*item, walk),
            // What it holds, which it took whole: its drop drops that. Part of
            // one, `t.0`, would need its own flags.
            ty::Closure(def_id, args) => match all(self, &mut args.as_closure().upvar_tys().iter(), walk) {
                Drops::Runs if self.takes_whole(*def_id) => Drops::Runs,
                Drops::Runs => Drops::Unsupported(ty, "a closure that holds part of a value with a destructor"),
                found => found,
            },
            ty::Adt(_, args) if ty.is_box() || self.is_vec_like(ty) => self.drops_in(args.type_at(0), walk),
            // A type parameter a caller gives a drop function for.
            ty::Param(param) if self.drop_state.param_drops.contains_key(&param.index) => Drops::Runs,
            ty::Param(param) if let Some(&(t, what)) = self.drop_state.unsupported_params.get(&param.index) => {
                Drops::Unsupported(t, what)
            }
            // An associated type only a caller knows, `<S as Source>::Item` (ADR
            // 0106): no drop function is given for one, so it has nothing to
            // drop only where nothing does, the crate's own types and a
            // library's.
            ty::Alias(
                _,
                ty::AliasTy {
                    kind: ty::Projection { .. },
                    ..
                },
            ) if self.is_unknown(ty) => match self.may_have_destructors() {
                true => Drops::Unsupported(ty, "a value of an associated type, where a type may have a destructor"),
                false => Drops::Nothing,
            },
            // Never dropped, or dropped by hand.
            ty::Adt(..) if self.is_lang_adt(ty, LangItem::ManuallyDrop) || std("MaybeUninit") => Drops::Nothing,
            ty::Adt(adt, args) => {
                let own = self.tcx.adt_destructor(adt.did());
                let parts = |walk: &mut Walk<'tcx>| {
                    let mut fields = adt.all_fields().map(|f| f.ty(self.tcx, args).skip_normalization());
                    all(self, &mut fields, walk)
                };
                match own {
                    Some(d) if self.runs_drop(d.did) => match parts(walk) {
                        Drops::Unsupported(t, what) => Drops::Unsupported(t, what),
                        _ => Drops::Runs,
                    },
                    // Another crate's, that rust-js didn't compile, or that doesn't
                    // export it: what it runs is out of sight (ADR 0100).
                    Some(_) if !self.recognition().in_sysroot(adt.did()) => {
                        Drops::Unsupported(ty, "a destructor of another crate's that its manifest doesn't export")
                    }
                    // A std type that drops what it holds its own way: an
                    // `Rc` when its last clone goes, a map its entries. A `Cell`
                    // drops the old value when it's set.
                    _ if own.is_some() || std("Cell") || std("RefCell") => match all(self, &mut args.types(), walk) {
                        Drops::Nothing => Drops::Nothing,
                        _ => Drops::Unsupported(ty, "a std type holding a value with a destructor"),
                    },
                    _ => parts(walk),
                }
            }
            _ => Drops::Nothing,
        };
        walk.seen.pop();
        if walk.reached >= depth {
            self.drop_state.cache.borrow_mut().insert(ty, found);
        }
        walk.reached = walk.reached.min(outer);
        found
    }
}

/// What's unsupported, and of what type, unless it's a closure's, which
/// is only where it's written.
pub(super) fn describe(ty: Ty<'_>, what: &str) -> String {
    match ty.kind() {
        ty::Closure(..) => what.to_string(),
        _ => format!("{what}, `{ty}`,"),
    }
}
