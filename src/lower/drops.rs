//! Destructors (ADR 0098): which types have one to run, the JS that runs
//! it, and where in a body a value that has one is moved.
//!
//! A scope that owns such a value is a `try`, and its drops the `finally`.
//! What this can't do yet is an error, found before any JS is written: a
//! value with a destructor that nothing drops would be a wrong answer.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use rustc_hir::{BindingMode, ByRef, LangItem};
use rustc_middle::middle::region;
use rustc_middle::mir::BinOp;
use rustc_middle::thir::visit::{self, Visitor};
use rustc_middle::thir::{
    AdtExprBase, BlockId, Expr as ThirExpr, ExprId, ExprKind, LocalVarId, Pat, PatKind, StmtKind as ThirStmt, Thir,
};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{self, Ty};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol};

use super::bindings::variant_name;
use super::representation::variant_field;
use super::{FnCx, R, lower_first};
use crate::js::{self, Expr, Op, Stmt, StmtKind};

/// What dropping a type runs.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Drops<'tcx> {
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

/// The drop functions one drop makes, which come before its code: a call
/// to one may be in a branch, as an `Option`'s, that another isn't in.
#[derive(Default)]
struct Made<'tcx> {
    functions: Vec<(Ty<'tcx>, String)>,
    defs: Vec<Stmt>,
}

/// A variable that owns a value with a destructor, dropped when its scope
/// ends: its JS value, and the flag that says it's still owned, if it moves.
struct Owned<'tcx> {
    value: Expr,
    ty: Ty<'tcx>,
    flag: Option<String>,
    /// Each part moved somewhere, and its flag.
    parts: Vec<(Path, String)>,
}

/// What a body does with its values that have destructors.
#[derive(Default)]
pub(super) struct Facts {
    /// The variables that own one, bound by value.
    owners: HashMap<LocalVarId, Span>,
    /// Those moved somewhere, which get a flag.
    moved: HashSet<LocalVarId>,
    /// Each use that moves one.
    moves: HashSet<ExprId>,
    /// Each value with a destructor that lives in a temporary: one used in
    /// place, borrowed or taken apart, and one made before an operand after
    /// it that can leave early, which a call then moves.
    temps: HashMap<ExprId, TempKind>,
    /// The parts of each owner that are moved somewhere, which get flags of
    /// their own, and each field that moves one, with its owner and part.
    parts: HashMap<LocalVarId, Vec<Path>>,
    part_moves: HashMap<ExprId, (LocalVarId, Path)>,
    /// What this body does that isn't supported yet.
    problems: Vec<(Span, String)>,
}

impl Facts {
    pub(super) fn has_owners(&self) -> bool {
        !self.owners.is_empty()
    }
}

/// A part of a value, field by field: each step the variant it's in, by
/// index, for an enum's, and the field.
pub(super) type Path = Vec<(Option<u32>, usize)>;

/// Why a value with a destructor is a temporary.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum TempKind {
    /// It's used in place, and dropped where rustc's scope tree ends it.
    Place,
    /// It's an operand, which the call or aggregate moves once every
    /// operand is evaluated; one after it that leaves early leaves it owned.
    Operand,
}

/// A temporary that ends with the statement being lowered.
pub(super) struct Temp<'tcx> {
    name: String,
    ty: Ty<'tcx>,
    flag: Option<String>,
    /// The operand it is, which must have been moved by the statement's end.
    operand: Option<ExprId>,
}

/// The statement being lowered, which a statement inside it saves and puts
/// back: the scopes its temporaries end in, its own and, for a `let`, the
/// rest of the block's, and the temporaries that end with it.
#[derive(Default)]
pub(super) struct Statement<'tcx> {
    scopes: Option<(region::Scope, Option<region::Scope>)>,
    temps: Vec<Temp<'tcx>>,
}

/// A function's drops while a copied default body has its own.
pub(super) struct SwappedDrops<'tcx> {
    params: HashMap<u32, String>,
    used: HashSet<u32>,
    unsupported: HashMap<u32, (Ty<'tcx>, &'static str)>,
    cache: HashMap<Ty<'tcx>, Drops<'tcx>>,
    sizes: HashMap<Ty<'tcx>, (usize, bool)>,
}

/// A function's drops, as its bodies are lowered.
#[derive(Default)]
pub(super) struct DropState<'tcx> {
    cache: RefCell<HashMap<Ty<'tcx>, Drops<'tcx>>>,
    sizes: RefCell<HashMap<Ty<'tcx>, (usize, bool)>>,
    /// Each body's facts, by the address of its THIR: a closure's is its own.
    facts: HashMap<usize, Rc<Facts>>,
    /// The owners in scope, innermost last.
    owned: Vec<Owned<'tcx>>,
    /// Every owner given a scope, and each move lowered, which a body's
    /// facts must all be once it's lowered.
    registered: HashSet<LocalVarId>,
    lowered_moves: HashSet<(usize, ExprId)>,
    flags: HashMap<LocalVarId, String>,
    /// Moves that are a call's operands, and the statements that clear
    /// their flags, which wait until every operand is evaluated.
    deferred: HashMap<(usize, ExprId), Option<Stmt>>,
    statement: Statement<'tcx>,
    /// An operand temporary's flag, and whether its move was lowered.
    temp_flags: HashMap<(usize, ExprId), String>,
    temps_moved: HashSet<(usize, ExprId)>,
    /// The function being lowered's drop functions, by the index of the
    /// type parameter each drops.
    param_drops: HashMap<u32, String>,
    /// The type parameters whose drops the body has used.
    used_drops: HashSet<u32>,
    /// A copied default's type parameters whose drops rust-js can't make,
    /// and why: an error only where the body drops one.
    unsupported_params: HashMap<u32, (Ty<'tcx>, &'static str)>,
    part_flags: HashMap<(LocalVarId, Path), String>,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// Is `drop`, a type's `Drop::drop`, one rust-js runs: the crate's own, or
    /// a library's it exports (ADR 0100)?
    pub(super) fn runs_drop(&self, drop: DefId) -> bool {
        drop.is_local() || self.krate.foreign.item(drop).is_some()
    }

    /// What dropping a `ty` runs.
    pub(super) fn drops(&self, ty: Ty<'tcx>) -> Drops<'tcx> {
        self.drops_in(
            ty,
            &mut Walk {
                seen: Vec::new(),
                reached: usize::MAX,
            },
        )
    }

    pub(super) fn has_drops(&self, ty: Ty<'tcx>) -> bool {
        self.drops(ty) == Drops::Runs
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
            ty::Closure(_, args) => match all(self, &mut args.as_closure().upvar_tys().iter(), walk) {
                Drops::Nothing => Drops::Nothing,
                _ => Drops::Unsupported(ty, "a closure that holds a value with a destructor"),
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
            ty::Alias(ty::Projection, _) if self.is_unknown(ty) => {
                let drop_trait = self.tcx.lang_items().drop_trait();
                let own = drop_trait.is_some_and(|id| self.tcx.all_local_trait_impls(()).contains_key(&id));
                match own || self.krate.foreign.any() {
                    true => Drops::Unsupported(ty, "a value of an associated type, where a type may have a destructor"),
                    false => Drops::Nothing,
                }
            }
            // Never dropped, or dropped by hand.
            ty::Adt(..) if self.is_lang_adt(ty, LangItem::ManuallyDrop) || std("MaybeUninit") => Drops::Nothing,
            ty::Adt(adt, args) => {
                let own = self.tcx.adt_destructor(adt.did());
                let parts = |walk: &mut Walk<'tcx>| {
                    let mut fields = adt.all_fields().map(|f| f.ty(self.tcx, args));
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

    /// Drop `value`, a `ty`: its own `drop`, then each part's, in Rust's
    /// order. `value` is read more than once, so it must read the same.
    pub(super) fn drop_value(&mut self, value: Expr, ty: Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<()> {
        let mut made = Made::default();
        let mut code = Vec::new();
        self.drop_in(value, ty, span, &mut made, &mut code)?;
        out.extend(made.defs);
        out.extend(code);
        Ok(())
    }

    /// How many drops writing a `ty`'s in place takes, and whether it's
    /// inside itself, found once for each type.
    fn drop_size(&self, ty: Ty<'tcx>, stack: &mut Vec<Ty<'tcx>>) -> (usize, bool) {
        if !self.has_drops(ty) {
            return (0, false);
        }
        // One inside a type that's inside it: both are inside themselves.
        if stack.contains(&ty) {
            return (0, true);
        }
        if let Some(&known) = self.drop_state.sizes.borrow().get(&ty) {
            return known;
        }
        stack.push(ty);
        let sum = |cx: &Self, tys: &mut dyn Iterator<Item = Ty<'tcx>>, stack: &mut Vec<Ty<'tcx>>| {
            tys.map(|t| cx.drop_size(t, stack))
                .fold((0, false), |(n, r), (m, q)| (n + m, r || q))
        };
        let (size, recursive) = match ty.kind() {
            ty::Adt(_, args) if ty.is_box() => self.drop_size(args.type_at(0), stack),
            ty::Adt(_, args) if self.is_vec_like(ty) => self.drop_size(args.type_at(0), stack),
            ty::Array(item, _) | ty::Slice(item) => self.drop_size(*item, stack),
            ty::Tuple(items) => sum(self, &mut items.iter(), stack),
            ty::Adt(adt, args) => {
                let own = usize::from(
                    self.tcx
                        .adt_destructor(adt.did())
                        .is_some_and(|d| self.runs_drop(d.did)),
                );
                let (parts, recursive) = sum(self, &mut adt.all_fields().map(|f| f.ty(self.tcx, args)), stack);
                (own + parts, recursive)
            }
            _ => (1, false),
        };
        stack.pop();
        self.drop_state.sizes.borrow_mut().insert(ty, (size, recursive));
        (size, recursive)
    }

    fn drop_in(&mut self, value: Expr, ty: Ty<'tcx>, span: Span, made: &mut Made<'tcx>, out: &mut Vec<Stmt>) -> R<()> {
        let ty = self.reveal(ty);
        match self.drops(ty) {
            Drops::Nothing => return Ok(()),
            Drops::Unsupported(t, what) => return Err(self.unsupported(span, &describe(t, what))),
            Drops::Runs => {}
        }
        // A type inside itself, as a list in a `Box` of itself, would need a
        // function of its own to drop.
        // One whose drop is a function already, in this drop: that.
        if let Some((_, name)) = made.functions.iter().find(|(t, _)| *t == ty) {
            let js_span = self.js_span(span);
            out.push(StmtKind::Expr(Expr::call(Expr::var(name), vec![value])).at(js_span));
            return Ok(());
        }
        // A type of the crate's own whose drop is long, or inside itself, as
        // a list is, gets a function of its own, which calls itself for the
        // ones inside: its drop is written once, not once for each path to it.
        if let ty::Adt(adt, _) = ty.kind()
            && (adt.did().is_local() || self.krate.foreign.in_library(adt.did()))
            && let (size, recursive) = self.drop_size(ty, &mut Vec::new())
            && (recursive || size > 8)
        {
            let type_name = self.tcx.item_name(adt.did()).to_string();
            let name = self.fresh(&format!("drop{type_name}"));
            let param = self.fresh(&lower_first(&type_name));
            made.functions.push((ty, name.clone()));
            let mut body = Vec::new();
            self.drop_parts(Expr::var(&param), ty, span, made, &mut body)?;
            let js_span = self.js_span(span);
            made.defs
                .push(StmtKind::Const(name.clone(), Expr::arrow(vec![param.into()], body)).at(js_span));
            out.push(StmtKind::Expr(Expr::call(Expr::var(&name), vec![value])).at(js_span));
            return Ok(());
        }
        self.drop_parts(value, ty, span, made, out)
    }

    /// A `ty`'s drop, written in place: its own `drop`, then its parts'.
    fn drop_parts(
        &mut self,
        value: Expr,
        ty: Ty<'tcx>,
        span: Span,
        made: &mut Made<'tcx>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let js_span = self.js_span(span);
        match ty.kind() {
            ty::Adt(_, args) if ty.is_box() => self.drop_in(value, args.type_at(0), span, made, out)?,
            ty::Adt(_, args) if self.is_vec_like(ty) => self.drop_items(value, args.type_at(0), span, made, out)?,
            // `dropT?.(value)`: the caller's drop, if its `T` has one.
            ty::Param(param) => {
                self.drop_state.used_drops.insert(param.index);
                let drop = Expr::var(&self.drop_state.param_drops[&param.index]);
                let js_span = self.js_span(span);
                out.push(
                    StmtKind::Expr(Expr {
                        kind: js::ExprKind::OptionalCall(Box::new(drop), vec![value]),
                        span: js_span,
                    })
                    .at(js_span),
                );
            }
            ty::Array(item, _) | ty::Slice(item) => self.drop_items(value, *item, span, made, out)?,
            ty::Tuple(items) => {
                for (i, item) in items.iter().enumerate() {
                    let part = self.project(value.clone(), ty, i);
                    self.drop_in(part, item, span, made, out)?;
                }
            }
            ty::Adt(adt, args) => {
                if let Some(d) = self.tcx.adt_destructor(adt.did())
                    && self.runs_drop(d.did)
                {
                    // `drop(&mut self)`: a `&mut` to what isn't an object is a
                    // box of it (ADR 0072).
                    let this = match self.is_boxable(ty) {
                        true => Expr::object(vec![js::Prop::Field("value".into(), value.clone())]),
                        false => value.clone(),
                    };
                    let drop = self
                        .tcx
                        .lang_items()
                        .drop_trait()
                        .map(|t| self.tcx.associated_item_def_ids(t)[0])
                        .expect("`Drop` has `drop`");
                    let call = self
                        .trait_call(drop, self.tcx.mk_args(&[ty.into()]), vec![this], span, out)?
                        .ok_or_else(|| self.unsupported(span, &format!("calling `{ty}`'s `drop`")))?;
                    out.push(StmtKind::Expr(call).at(js_span));
                }
                if let Some(inner) = self.option_of(ty) {
                    if self.can_be_nullish(inner) || self.boxed_payload(inner) {
                        return Err(self.unsupported(span, &format!("dropping `{ty}`")));
                    }
                    let mut some = Vec::new();
                    self.drop_in(value.clone(), inner, span, made, &mut some)?;
                    out.push(StmtKind::If(Expr::bin(Op::LooseNe, value, Expr::null()), some, None).at(js_span));
                } else if adt.is_struct() {
                    for (i, field) in adt.non_enum_variant().fields.iter().enumerate() {
                        let part = self.project(value.clone(), ty, i);
                        self.drop_in(part, field.ty(self.tcx, args), span, made, out)?;
                    }
                } else if adt.is_enum() {
                    for variant in adt.variants() {
                        let mut fields = Vec::new();
                        for (i, field) in variant.fields.iter().enumerate() {
                            let part = Expr::member(value.clone(), variant_field(self.tcx, variant, i));
                            self.drop_in(part, field.ty(self.tcx, args), span, made, &mut fields)?;
                        }
                        if fields.is_empty() {
                            continue;
                        }
                        let tag = Expr::str(variant_name(self.tcx, variant));
                        let test = match adt.variants().len() {
                            1 => Expr::bool(true),
                            _ => Expr::bin(Op::Eq, Expr::member(value.clone(), "TAG"), tag),
                        };
                        out.push(StmtKind::If(test, fields, None).at(js_span));
                    }
                } else {
                    return Err(self.unsupported(span, &format!("dropping `{ty}`")));
                }
            }
            _ => return Err(self.unsupported(span, &format!("dropping `{ty}`"))),
        }
        Ok(())
    }

    /// Drop `value`, a `ty` some of whose parts may have moved: a part with
    /// a flag only if it's still owned, the rest as `drop_value` would. Rust
    /// forbids moving out of a type with a `Drop` of its own (E0509), so
    /// what's around a moved part has no `drop` to call.
    fn drop_owned(
        &mut self,
        value: Expr,
        ty: Ty<'tcx>,
        parts: &[(Path, String)],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        if parts.is_empty() {
            return self.drop_value(value, ty, span, out);
        }
        let js_span = self.js_span(span);
        // This part itself moved, on some path: all of it only if it's owned.
        if let Some((_, flag)) = parts.iter().find(|(p, _)| p.is_empty()) {
            let rest: Vec<(Path, String)> = parts.iter().filter(|(p, _)| !p.is_empty()).cloned().collect();
            let mut owned = Vec::new();
            self.drop_owned(value, ty, &rest, span, &mut owned)?;
            if !owned.is_empty() {
                out.push(StmtKind::If(Expr::var(flag), owned, None).at(js_span));
            }
            return Ok(());
        }
        let under = |step: (Option<u32>, usize)| -> Vec<(Path, String)> {
            parts
                .iter()
                .filter(|(p, _)| p[0] == step)
                .map(|(p, f)| (p[1..].to_vec(), f.clone()))
                .collect()
        };
        match ty.kind() {
            ty::Tuple(items) => {
                for (i, item) in items.iter().enumerate() {
                    let part = self.project(value.clone(), ty, i);
                    self.drop_owned(part, item, &under((None, i)), span, out)?;
                }
            }
            ty::Adt(adt, args) if adt.is_struct() => {
                for (i, field) in adt.non_enum_variant().fields.iter().enumerate() {
                    let part = self.project(value.clone(), ty, i);
                    self.drop_owned(part, field.ty(self.tcx, args), &under((None, i)), span, out)?;
                }
            }
            // `Some(x)` is `x` itself (ADR 0030).
            ty::Adt(adt, _) if let Some(inner) = self.option_of(ty) => {
                let some = adt.variants().iter().position(|v| !v.fields.is_empty()).unwrap_or(1) as u32;
                let mut body = Vec::new();
                self.drop_owned(value.clone(), inner, &under((Some(some), 0)), span, &mut body)?;
                if !body.is_empty() {
                    out.push(StmtKind::If(Expr::bin(Op::LooseNe, value, Expr::null()), body, None).at(js_span));
                }
            }
            ty::Adt(adt, args) if adt.is_enum() => {
                for (index, variant) in adt.variants().iter().enumerate() {
                    let mut fields = Vec::new();
                    for (i, field) in variant.fields.iter().enumerate() {
                        let part = Expr::member(value.clone(), variant_field(self.tcx, variant, i));
                        self.drop_owned(
                            part,
                            field.ty(self.tcx, args),
                            &under((Some(index as u32), i)),
                            span,
                            &mut fields,
                        )?;
                    }
                    if fields.is_empty() {
                        continue;
                    }
                    let test = match adt.variants().len() {
                        1 => Expr::bool(true),
                        _ => Expr::bin(
                            Op::Eq,
                            Expr::member(value.clone(), "TAG"),
                            Expr::str(variant_name(self.tcx, variant)),
                        ),
                    };
                    out.push(StmtKind::If(test, fields, None).at(js_span));
                }
            }
            _ => return Err(self.unsupported(span, &format!("dropping what's left of a `{ty}`"))),
        }
        Ok(())
    }

    /// Each item of an array, in order: `for (const item of v) { .. }`.
    fn drop_items(
        &mut self,
        items: Expr,
        item: Ty<'tcx>,
        span: Span,
        made: &mut Made<'tcx>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let name = self.fresh("item");
        let mut body = Vec::new();
        self.drop_in(Expr::var(&name), item, span, made, &mut body)?;
        let js_span = self.js_span(span);
        out.push(
            StmtKind::ForOf {
                label: None,
                pattern: js::Pattern::Name(name),
                mutable: false,
                iterable: items,
                body,
            }
            .at(js_span),
        );
        Ok(())
    }

    /// This body's facts, found the first time they're asked for; what it
    /// does that isn't supported is an error then.
    pub(super) fn drop_facts(&mut self) -> R<Rc<Facts>> {
        let key = std::ptr::from_ref(self.thir) as usize;
        if let Some(facts) = self.drop_state.facts.get(&key) {
            return Ok(facts.clone());
        }
        let facts = Rc::new(find_facts(self));
        self.drop_state.facts.insert(key, facts.clone());
        let mut failed = None;
        for (span, what) in &facts.problems {
            failed = Some(self.unsupported(*span, what));
        }
        match failed {
            Some(guar) => Err(guar),
            None => Ok(facts),
        }
    }

    /// `var`, just bound to `value` in `out`, owns a value with a
    /// destructor: its scope drops it. One that's moved gets a flag.
    pub(super) fn own(&mut self, var: LocalVarId, value: Expr, ty: Ty<'tcx>, span: Span, out: &mut Vec<Stmt>) -> R<()> {
        let js_span = self.js_span(span);
        self.own_at(var, value, ty, js_span, out)
    }

    /// `own`, with the flags' statements at `js_span`. A part that's moved
    /// somewhere, `pair.a`, gets a flag of its own, `pair$a$live`.
    pub(super) fn own_at(
        &mut self,
        var: LocalVarId,
        value: Expr,
        ty: Ty<'tcx>,
        js_span: js::Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let facts = self.drop_facts()?;
        let base = match &value.kind {
            js::ExprKind::Var(name) => name.clone(),
            _ => "value".into(),
        };
        let flag = facts.moved.contains(&var).then(|| {
            let flag = self.fresh(&format!("{base}$live"));
            out.push(StmtKind::Let(flag.clone(), Some(Expr::bool(true))).at(js_span));
            self.drop_state.flags.insert(var, flag.clone());
            flag
        });
        let mut parts: Vec<(Path, String)> = Vec::new();
        for path in facts.parts.get(&var).into_iter().flatten() {
            if parts.iter().any(|(p, _)| p == path) {
                continue;
            }
            let named = self.path_name(ty, path);
            let flag = self.fresh(&format!("{base}${named}$live"));
            out.push(StmtKind::Let(flag.clone(), Some(Expr::bool(true))).at(js_span));
            self.drop_state.part_flags.insert((var, path.clone()), flag.clone());
            parts.push((path.clone(), flag));
        }
        self.drop_state.registered.insert(var);
        self.drop_state.owned.push(Owned { value, ty, flag, parts });
        Ok(())
    }

    /// A part's path as a name, its fields' and variants': `a`, `0$1`, `Some$0`.
    fn path_name(&self, ty: Ty<'tcx>, path: &Path) -> String {
        let mut names = Vec::new();
        let mut at = ty;
        for &(variant, field) in path {
            match at.kind() {
                ty::Adt(adt, args) => {
                    let def = match variant {
                        Some(v) => {
                            let def = adt.variants().iter().nth(v as usize).expect("the variant");
                            names.push(def.name.to_string());
                            def
                        }
                        None => adt.non_enum_variant(),
                    };
                    let f = def.fields.iter().nth(field).expect("the field");
                    names.push(f.name.to_string());
                    at = f.ty(self.tcx, args);
                }
                ty::Tuple(items) => {
                    names.push(field.to_string());
                    at = items[field];
                }
                _ => names.push(field.to_string()),
            }
        }
        names.join("$")
    }

    /// The parts a pattern moves out of what it's matched against, by value:
    /// those it binds that have a destructor. None if it moves one a way this
    /// doesn't follow yet, as through a `Box` or into `x @ ..`.
    pub(super) fn pattern_paths(&self, pat: &Pat<'tcx>) -> Option<Vec<Path>> {
        let mut found = Vec::new();
        self.paths_in(pat, &mut Path::new(), &mut found).then_some(found)
    }

    fn paths_in(&self, pat: &Pat<'tcx>, at: &mut Path, found: &mut Vec<Path>) -> bool {
        let moves = |p: &Pat<'tcx>| {
            let mut any = false;
            p.walk_always(|p| {
                if let PatKind::Binding {
                    mode: BindingMode(ByRef::No, _),
                    ty,
                    ..
                } = p.kind
                {
                    any |= self.has_drops(ty);
                }
            });
            any
        };
        match &pat.kind {
            PatKind::Wild => true,
            PatKind::Binding {
                mode: BindingMode(ByRef::No, _),
                subpattern: None,
                ty,
                ..
            } => {
                if self.has_drops(*ty) {
                    found.push(at.clone());
                }
                true
            }
            PatKind::Binding {
                mode: BindingMode(ByRef::Yes(..), _),
                subpattern: None,
                ..
            } => true,
            PatKind::Leaf { subpatterns } => subpatterns.iter().all(|f| {
                at.push((None, f.field.as_usize()));
                let ok = self.paths_in(&f.pattern, at, found);
                at.pop();
                ok
            }),
            PatKind::Variant {
                variant_index,
                subpatterns,
                ..
            } => subpatterns.iter().all(|f| {
                at.push((Some(variant_index.as_u32()), f.field.as_usize()));
                let ok = self.paths_in(&f.pattern, at, found);
                at.pop();
                ok
            }),
            _ => !moves(pat),
        }
    }

    /// What `pat`, matched against `scrutinee`, a variable whose parts have
    /// flags, moves out of it: those parts' flags are cleared.
    pub(super) fn clear_parts(&mut self, scrutinee: ExprId, pat: &Pat<'tcx>, out: &mut Vec<Stmt>) {
        let ExprKind::VarRef { id } = self.thir[self.strip(scrutinee)].kind else {
            return;
        };
        let js_span = self.js_span(pat.span);
        for path in self.pattern_paths(pat).unwrap_or_default() {
            if let Some(flag) = self.drop_state.part_flags.get(&(id, path)) {
                out.push(StmtKind::Assign(Expr::var(flag), Expr::bool(false)).at(js_span));
            }
        }
    }

    /// Whether `var` owns a value with a destructor.
    pub(super) fn is_owner(&mut self, var: LocalVarId) -> R<bool> {
        Ok(self.drop_facts()?.owners.contains_key(&var))
    }

    /// Whether `e` is a temporary, and why.
    pub(super) fn temp_kind(&mut self, e: ExprId) -> R<Option<TempKind>> {
        Ok(self.drop_facts()?.temps.get(&e).copied())
    }

    /// `e`, a value with a destructor that's a temporary: in a `const` of its
    /// own, dropped where rustc's scope tree ends it. One that ends with the
    /// statement is dropped by its `finally`; one a `let` extends is owned
    /// by the rest of the block, as a variable is.
    pub(super) fn temporary(&mut self, e: ExprId, kind: TempKind, value: Expr, out: &mut Vec<Stmt>) -> R<Expr> {
        let span = self.thir[e].span;
        let ty = self.thir[e].ty;
        let js_span = self.js_span(span);
        let base = match ty.peel_refs().kind() {
            ty::Adt(adt, _) => lower_first(self.tcx.item_name(adt.did()).as_str()),
            _ => "temporary".to_string(),
        };
        let name = self.fresh(&base);
        out.push(StmtKind::Const(name.clone(), value).at(js_span));
        let Some((statement, rest)) = self.drop_state.statement.scopes else {
            return Err(self.unsupported(span, "a temporary with a destructor here"));
        };
        let key = std::ptr::from_ref(self.thir) as usize;
        let flag = match kind {
            TempKind::Operand => {
                let flag = self.fresh(&format!("{name}$live"));
                out.push(StmtKind::Let(flag.clone(), Some(Expr::bool(true))).at(js_span));
                self.drop_state.temp_flags.insert((key, e), flag.clone());
                Some(flag)
            }
            TempKind::Place => {
                let tree = self.tcx.region_scope_tree(self.body_owner);
                match tree.temporary_scope(self.thir[e].temp_scope_id).temp_lifetime {
                    // Never dropped, as a promoted constant isn't.
                    None => return Ok(Expr::var(&name)),
                    Some(scope) if scope == statement => None,
                    // `let r = &f();`: it lives as long as `r` does.
                    Some(scope) if Some(scope) == rest => {
                        self.own_value(Expr::var(&name), ty);
                        return Ok(Expr::var(&name));
                    }
                    Some(_) => return Err(self.unsupported(span, "a temporary with a destructor here")),
                }
            }
        };
        let operand = (kind == TempKind::Operand).then_some(e);
        self.drop_state.statement.temps.push(Temp {
            name: name.clone(),
            ty,
            flag,
            operand,
        });
        Ok(Expr::var(&name))
    }

    /// Start lowering a statement whose temporaries end in `scopes`: its own,
    /// and for a `let`, the rest of the block's. What it replaces, a
    /// statement it's inside, `end_statement` puts back.
    pub(super) fn begin_statement(&mut self, scopes: (region::Scope, Option<region::Scope>)) -> Statement<'tcx> {
        std::mem::replace(
            &mut self.drop_state.statement,
            Statement {
                scopes: Some(scopes),
                temps: Vec::new(),
            },
        )
    }

    /// Finish the statement `begin_statement` started: `lowered`, its JS,
    /// goes in `out`, each of its temporaries dropped by a `finally` after
    /// the `const` that holds it, last first.
    pub(super) fn end_statement(
        &mut self,
        outer: Statement<'tcx>,
        lowered: Vec<Stmt>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let statement = std::mem::replace(&mut self.drop_state.statement, outer);
        let key = std::ptr::from_ref(self.thir) as usize;
        // An operand temporary must have been moved where it's an operand.
        for t in &statement.temps {
            if let Some(e) = t.operand
                && !self.drop_state.temps_moved.contains(&(key, e))
            {
                return Err(self.unsupported(
                    self.thir[e].span,
                    "a value with a destructor made before what may panic or leave early, here",
                ));
            }
        }
        if statement.temps.is_empty() {
            out.extend(lowered);
            return Ok(());
        }
        // Each is declared in the statement's own JS, not in a branch of it.
        let mut placed = Vec::new();
        for t in statement.temps {
            let at = lowered
                .iter()
                .position(|s| matches!(&s.kind, StmtKind::Const(n, _) if *n == t.name))
                .ok_or_else(|| self.unsupported(span, "a temporary with a destructor in a branch"))?;
            placed.push((at, t));
        }
        placed.sort_by_key(|(at, _)| *at);
        self.wrap_temps(lowered, placed, span, out)
    }

    fn wrap_temps(
        &mut self,
        mut lowered: Vec<Stmt>,
        mut placed: Vec<(usize, Temp<'tcx>)>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        if placed.is_empty() {
            out.extend(lowered);
            return Ok(());
        }
        let (at, temp) = placed.remove(0);
        // Through its declaration, and its flag's.
        let mut end = at + 1;
        if let Some(flag) = &temp.flag
            && matches!(lowered.get(end).map(|s| &s.kind), Some(StmtKind::Let(n, _)) if n == flag)
        {
            end += 1;
        }
        let rest = lowered.split_off(end);
        out.extend(lowered);
        let placed = placed.into_iter().map(|(i, t)| (i - end, t)).collect();
        let mut inner = Vec::new();
        self.wrap_temps(rest, placed, span, &mut inner)?;
        // What the rest declares is used after it: declared before the `try`.
        let mut body = Vec::new();
        for s in inner {
            let js_span = s.span;
            match s.kind {
                StmtKind::Const(name, value) | StmtKind::Let(name, Some(value)) => {
                    out.push(StmtKind::Let(name.clone(), None).at(js_span));
                    body.push(StmtKind::Assign(Expr::var(&name), value).at(js_span));
                }
                StmtKind::Let(name, None) => out.push(StmtKind::Let(name, None).at(js_span)),
                StmtKind::Destructure { .. } => {
                    return Err(self.unsupported(
                        span,
                        "taking a value apart in a statement with a temporary that has a destructor",
                    ));
                }
                kind => body.push(kind.at(js_span)),
            }
        }
        let mut drop = Vec::new();
        self.drop_value(Expr::var(&temp.name), temp.ty, span, &mut drop)?;
        let js_span = self.js_span(span);
        let finally = match temp.flag {
            Some(flag) => vec![StmtKind::If(Expr::var(&flag), drop, None).at(js_span)],
            None => drop,
        };
        if body.is_empty() {
            out.extend(finally);
        } else {
            out.push(StmtKind::Try(body, finally).at(js_span));
        }
        Ok(())
    }

    /// The function being lowered is given a drop for its type parameter
    /// `index`, named `name` (ADR 0098).
    pub(super) fn give_drop_param(&mut self, index: u32, name: String) {
        self.drop_state.param_drops.insert(index, name);
    }

    /// A copied default body's drops, in place of this function's, for its
    /// trait's type parameters, `Self` among them (ADR 0049): each is what the
    /// impl's argument for it drops, with the impl's own drops. What was
    /// there, and what was found with it, is given back by `restore_drops`.
    pub(super) fn swap_drops(
        &mut self,
        drops: HashMap<u32, String>,
        unsupported: HashMap<u32, (Ty<'tcx>, &'static str)>,
    ) -> SwappedDrops<'tcx> {
        let cache = std::mem::take(&mut *self.drop_state.cache.borrow_mut());
        let sizes = std::mem::take(&mut *self.drop_state.sizes.borrow_mut());
        SwappedDrops {
            params: std::mem::replace(&mut self.drop_state.param_drops, drops),
            used: std::mem::take(&mut self.drop_state.used_drops),
            unsupported: std::mem::replace(&mut self.drop_state.unsupported_params, unsupported),
            cache,
            sizes,
        }
    }

    /// The type parameters whose drops the body being lowered has used.
    pub(super) fn used_drops(&self) -> HashSet<u32> {
        self.drop_state.used_drops.clone()
    }

    pub(super) fn restore_drops(&mut self, swapped: SwappedDrops<'tcx>) {
        self.drop_state.param_drops = swapped.params;
        self.drop_state.used_drops = swapped.used;
        self.drop_state.unsupported_params = swapped.unsupported;
        *self.drop_state.cache.borrow_mut() = swapped.cache;
        *self.drop_state.sizes.borrow_mut() = swapped.sizes;
    }

    /// The drops this function is given, as it takes them: by their type
    /// parameters' order.
    pub(super) fn given_drops(&self) -> Vec<String> {
        let mut given: Vec<(&u32, &String)> = self.drop_state.param_drops.iter().collect();
        given.sort();
        given.into_iter().map(|(_, name)| name.clone()).collect()
    }

    /// The function that drops a `ty`, which a generic function is given for
    /// its type parameter: its `drop` itself, when that's all its drop is,
    /// `noisyDrop_drop`, or an arrow; a type parameter's is the one this
    /// function was given. None for a type with nothing to drop.
    pub(super) fn drop_function(&mut self, ty: Ty<'tcx>, span: Span) -> R<Option<Expr>> {
        if let ty::Param(param) = ty.kind() {
            let drop = self
                .drop_state
                .param_drops
                .get(&param.index)
                .map(|name| Expr::var(name));
            if drop.is_some() {
                self.drop_state.used_drops.insert(param.index);
            }
            return Ok(drop);
        }
        match self.drops(ty) {
            Drops::Nothing => return Ok(None),
            Drops::Unsupported(t, what) => return Err(self.unsupported(span, &describe(t, what))),
            Drops::Runs => {}
        }
        let base = match ty.kind() {
            ty::Adt(adt, _) => lower_first(self.tcx.item_name(adt.did()).as_str()),
            _ => "value".to_string(),
        };
        let param = self.fresh(&base);
        let mut body = Vec::new();
        self.drop_value(Expr::var(&param), ty, span, &mut body)?;
        // `(noisy) => noisyDrop_drop(noisy)` is `noisyDrop_drop`.
        if let [
            Stmt {
                kind: StmtKind::Expr(call),
                ..
            },
        ] = body.as_slice()
            && let js::ExprKind::Call(callee, args) = &call.kind
            && let [arg] = args.as_slice()
            && matches!(&arg.kind, js::ExprKind::Var(name) if *name == param)
            && matches!(&callee.kind, js::ExprKind::Var(_) | js::ExprKind::Symbol(_))
        {
            return Ok(Some((**callee).clone()));
        }
        Ok(Some(Expr::arrow(vec![param.into()], body)))
    }

    /// A value its scope drops that no variable names, as a `_` parameter.
    pub(super) fn own_value(&mut self, value: Expr, ty: Ty<'tcx>) {
        self.drop_state.owned.push(Owned {
            value,
            ty,
            flag: None,
            parts: Vec::new(),
        });
    }

    /// How many owners are in scope: where a new scope's start.
    pub(super) fn owned_mark(&self) -> usize {
        self.drop_state.owned.len()
    }

    /// End the scope that began at `mark`: `body`, then the drops of what
    /// it owns, last first, in a `finally`.
    pub(super) fn close_scope(&mut self, mark: usize, body: Vec<Stmt>, span: Span, out: &mut Vec<Stmt>) -> R<()> {
        let owned: Vec<Owned<'tcx>> = self.drop_state.owned.drain(mark..).collect();
        if owned.is_empty() {
            out.extend(body);
            return Ok(());
        }
        let js_span = self.js_span(span);
        let mut finally = Vec::new();
        for o in owned.into_iter().rev() {
            let mut drop = Vec::new();
            self.drop_owned(o.value, o.ty, &o.parts, span, &mut drop)?;
            match o.flag {
                Some(flag) => finally.push(StmtKind::If(Expr::var(&flag), drop, None).at(js_span)),
                None => finally.extend(drop),
            }
        }
        // Nothing between the declaration and the drops: nothing to leave by.
        if body.is_empty() {
            out.extend(finally);
        } else {
            out.push(StmtKind::Try(body, finally).at(js_span));
        }
        Ok(())
    }

    /// Whether dropping a `ty` reads it once, so a value that isn't a
    /// variable can be dropped where it's made: `noisyDrop_drop(["x"])`.
    pub(super) fn drops_once(&self, ty: Ty<'tcx>) -> bool {
        match ty.kind() {
            ty::Adt(_, args) if ty.is_box() => self.drops_once(args.type_at(0)),
            ty::Adt(adt, args) => {
                self.tcx
                    .adt_destructor(adt.did())
                    .is_some_and(|d| self.runs_drop(d.did))
                    && adt.is_struct()
                    && adt.all_fields().all(|f| !self.has_drops(f.ty(self.tcx, args)))
            }
            _ => false,
        }
    }

    /// `value`, which `drop_value` may read more than once, in a `const`
    /// first unless it reads the same each time, or is read once.
    pub(super) fn droppable(&mut self, value: Expr, ty: Ty<'tcx>, out: &mut Vec<Stmt>) -> Expr {
        if value.reads_same() || self.drops_once(ty) {
            value
        } else {
            self.spill("value", value, out)
        }
    }

    /// Whether computing `e` can't panic, or leave early any other way: a
    /// literal, a variable, and what's built of them. A `let` of one needs
    /// no `try` of its own after one before it: nothing can leave between.
    pub(super) fn cannot_leave(&self, e: ExprId) -> bool {
        match &self.thir[self.strip(e)].kind {
            ExprKind::Literal { .. }
            | ExprKind::NonHirLiteral { .. }
            | ExprKind::ZstLiteral { .. }
            | ExprKind::NamedConst { .. }
            | ExprKind::VarRef { .. }
            | ExprKind::UpvarRef { .. } => true,
            ExprKind::Adt(adt) => {
                matches!(adt.base, AdtExprBase::None) && adt.fields.iter().all(|f| self.cannot_leave(f.expr))
            }
            ExprKind::Tuple { fields } | ExprKind::Array { fields } => fields.iter().all(|&f| self.cannot_leave(f)),
            ExprKind::Borrow { arg, .. }
            | ExprKind::Field { lhs: arg, .. }
            | ExprKind::Deref { arg }
            | ExprKind::Unary { arg, .. }
            | ExprKind::Cast { source: arg } => self.cannot_leave(*arg),
            // Arithmetic wraps (ADR 0011), so only division can panic.
            ExprKind::Binary { op, lhs, rhs } => {
                !matches!(op, BinOp::Div | BinOp::Rem) && self.cannot_leave(*lhs) && self.cannot_leave(*rhs)
            }
            ExprKind::LogicalOp { lhs, rhs, .. } => self.cannot_leave(*lhs) && self.cannot_leave(*rhs),
            // `Box::new(x)` only puts `x` in a box.
            ExprKind::Call { fun, args, .. } => {
                matches!(*self.thir[*fun].ty.kind(), ty::FnDef(id, _) if self.tcx.is_diagnostic_item(Symbol::intern("box_new"), id))
                    && args.iter().all(|&a| self.cannot_leave(a))
            }
            _ => false,
        }
    }

    /// `e` moves a variable that owns a value with a destructor: it's not
    /// owned from here, so its flag is cleared first.
    pub(super) fn moved(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<()> {
        let e = self.strip(e);
        let facts = self.drop_facts()?;
        let key = std::ptr::from_ref(self.thir) as usize;
        // A part moved out, `pair.a`: its flag.
        let flag = if let Some((var, path)) = facts.part_moves.get(&e) {
            self.drop_state.part_flags.get(&(*var, path.clone())).cloned()
        } else if let ExprKind::VarRef { id } = self.thir[e].kind
            && facts.moves.contains(&e)
        {
            self.drop_state.flags.get(&id).cloned()
        } else {
            return Ok(());
        };
        self.drop_state.lowered_moves.insert((key, e));
        if let Some(flag) = &flag {
            let js_span = self.js_span(self.thir[e].span);
            let clear = StmtKind::Assign(Expr::var(flag), Expr::bool(false)).at(js_span);
            match self.drop_state.deferred.get_mut(&(key, e)) {
                Some(slot) => *slot = Some(clear),
                None => out.push(clear),
            }
        }
        Ok(())
    }

    /// Before `list`, a call's operands, is evaluated: the variables it
    /// moves. Rust moves them as the call's made, after all of them are, so
    /// a later one that panics leaves them owned, and dropped.
    pub(super) fn defer_moves(&mut self, list: &[ExprId]) -> R<Vec<ExprId>> {
        let facts = self.drop_facts()?;
        let key = std::ptr::from_ref(self.thir) as usize;
        let moves: Vec<ExprId> = list
            .iter()
            .map(|&e| self.strip(e))
            .filter(|e| {
                facts.moves.contains(e)
                    || facts.part_moves.contains_key(e)
                    || facts.temps.get(e) == Some(&TempKind::Operand)
            })
            .collect();
        for &e in &moves {
            if facts.moves.contains(&e) || facts.part_moves.contains_key(&e) {
                self.drop_state.deferred.insert((key, e), None);
            }
        }
        Ok(moves)
    }

    /// Once `values`, the operands, are evaluated: what has effects goes in
    /// a `const`, in order, then the moves' flags are cleared.
    pub(super) fn end_moves(&mut self, moves: &[ExprId], values: &mut [Expr], out: &mut Vec<Stmt>) {
        let key = std::ptr::from_ref(self.thir) as usize;
        let mut clears: Vec<Stmt> = Vec::new();
        for &e in moves {
            if let Some(clear) = self.drop_state.deferred.remove(&(key, e)).flatten() {
                clears.push(clear);
            } else if let Some(flag) = self.drop_state.temp_flags.get(&(key, e)) {
                // A temporary operand, moved now.
                let js_span = self.js_span(self.thir[e].span);
                clears.push(StmtKind::Assign(Expr::var(flag), Expr::bool(false)).at(js_span));
                self.drop_state.temps_moved.insert((key, e));
            }
        }
        if clears.is_empty() {
            return;
        }
        for value in values.iter_mut() {
            if value.has_effects() {
                let original = std::mem::replace(value, Expr::undefined());
                *value = self.spill("arg", original, out);
            }
        }
        out.extend(clears);
    }

    /// `lhs = rhs` of a value with a destructor: the new value, then the
    /// old one's drop, if it's still owned, then the write, as Rust does it.
    pub(super) fn assign_dropping(&mut self, lhs: ExprId, rhs: ExprId, span: js::Span, out: &mut Vec<Stmt>) -> R<()> {
        let ty = self.thir[lhs].ty;
        let rust_span = self.thir[lhs].span;
        let value = self.expr(rhs, out)?;
        let value = if value.has_effects() {
            self.spill("next", value, out)
        } else {
            value
        };
        let Some((target, _)) = self.place(lhs) else {
            return Err(self.unsupported(rust_span, "assigning a value with a destructor here"));
        };
        let flag = match self.thir[self.strip(lhs)].kind {
            ExprKind::VarRef { id } => self.drop_state.flags.get(&id).cloned(),
            // A part that may have moved, `pair.a = ..` after `consume(pair.a)`.
            _ => self.body_query().place_path(lhs).and_then(|(var, fields)| {
                let path: Path = fields.into_iter().map(|f| (None, f)).collect();
                self.drop_state.part_flags.get(&(var, path)).cloned()
            }),
        };
        let mut drop = Vec::new();
        self.drop_value(target.clone(), ty, rust_span, &mut drop)?;
        match &flag {
            Some(flag) => out.push(StmtKind::If(Expr::var(flag), drop, None).at(span)),
            None => out.extend(drop),
        }
        out.push(StmtKind::Assign(target, value).at(span));
        if let Some(flag) = flag {
            out.push(StmtKind::Assign(Expr::var(&flag), Expr::bool(true)).at(span));
        }
        Ok(())
    }

    /// Whether `block` binds a variable that owns a value with a destructor.
    pub(super) fn block_owns(&mut self, block: BlockId) -> R<bool> {
        let facts = self.drop_facts()?;
        let thir = self.thir;
        Ok(thir[block].stmts.iter().any(|&s| match &thir[s].kind {
            ThirStmt::Let { pattern, .. } => binds_any(pattern, &facts.owners),
            ThirStmt::Expr { .. } => false,
        }))
    }

    /// Once a body is lowered: every owner it has must have had a scope, and
    /// every move its flag cleared. One that didn't went a way this doesn't
    /// know, so it's an error, not JS that forgets a drop.
    pub(super) fn check_drops(&mut self) -> R<()> {
        let facts = self.drop_facts()?;
        let key = std::ptr::from_ref(self.thir) as usize;
        let mut failed = None;
        for (var, span) in &facts.owners {
            if !self.drop_state.registered.contains(var) {
                failed = Some(self.unsupported(*span, "binding a value with a destructor here"));
            }
        }
        for &e in facts.moves.iter().chain(facts.part_moves.keys()) {
            if !self.drop_state.lowered_moves.contains(&(key, e)) {
                failed = Some(self.unsupported(self.thir[e].span, "moving a value with a destructor here"));
            }
        }
        match failed {
            Some(guar) => Err(guar),
            None => Ok(()),
        }
    }
}

/// What's unsupported, and of what type, unless it's a closure's, which
/// is only where it's written.
fn describe(ty: Ty<'_>, what: &str) -> String {
    match ty.kind() {
        ty::Closure(..) => what.to_string(),
        _ => format!("{what}, `{ty}`,"),
    }
}

fn binds_any(pat: &Pat<'_>, owners: &HashMap<LocalVarId, Span>) -> bool {
    let mut found = false;
    pat.walk_always(|p| {
        if let PatKind::Binding { var, .. } = p.kind {
            found |= owners.contains_key(&var);
        }
    });
    found
}

/// What a pattern takes of a value it's matched against.
#[derive(PartialEq)]
enum Taken {
    /// Nothing that has a destructor: it borrows, copies or ignores it.
    Nothing,
    /// All of it, into one variable.
    Whole,
    /// A part, which a partial move leaves the rest of.
    Part,
}

/// Find a body's owners, and what moves them, before it's lowered.
fn find_facts<'a, 'tcx>(cx: &FnCx<'a, 'tcx>) -> Facts {
    let thir = cx.thir;
    let mut finder = Finder {
        cx,
        thir,
        ids: thir
            .exprs
            .iter_enumerated()
            .map(|(id, e)| (std::ptr::from_ref(e) as usize, id))
            .collect(),
        stack: Vec::new(),
        lets: HashMap::new(),
        facts: Facts::default(),
    };
    for param in &thir.params {
        if let Some(pat) = &param.pat {
            finder.visit_pat(pat);
        }
    }
    // THIR is built from the leaves up, so the body's own expression is
    // the last, and every other is reached from it.
    if let Some(root) = thir.exprs.last_index() {
        finder.visit_expr(&thir[root]);
    }
    finder.facts
}

struct Finder<'c, 'a, 'tcx> {
    cx: &'c FnCx<'a, 'tcx>,
    thir: &'a Thir<'tcx>,
    ids: HashMap<usize, ExprId>,
    /// The expressions being walked, outermost first.
    stack: Vec<ExprId>,
    /// Each `let` statement's value, and its pattern.
    lets: HashMap<ExprId, &'a Pat<'tcx>>,
    facts: Facts,
}

impl<'c, 'a, 'tcx> Finder<'c, 'a, 'tcx> {
    fn id(&self, e: &ThirExpr<'tcx>) -> ExprId {
        self.ids[&(std::ptr::from_ref(e) as usize)]
    }

    /// What `e`, the top of the walk, is used for: the expression it's in,
    /// past what only passes it on, and which of its parts `e` is.
    fn context(&self) -> (Option<ExprId>, ExprId) {
        let mut child = *self.stack.last().expect("an expression being walked");
        for &parent in self.stack.iter().rev().skip(1) {
            // A `let`'s value is its statement's, not the block's around it:
            // its pattern says what it takes.
            if self.lets.contains_key(&child) {
                return (None, child);
            }
            match self.thir[parent].kind {
                ExprKind::Scope { .. }
                | ExprKind::Use { .. }
                | ExprKind::ValueTypeAscription { .. }
                | ExprKind::PlaceTypeAscription { .. } => child = parent,
                _ => return (Some(parent), child),
            }
        }
        (None, child)
    }

    /// Whether a value made here has a destructor to run: `None` of an
    /// `Option` that could hold one doesn't, nor does a variant of an enum
    /// without a `Drop` of its own whose fields have none.
    fn holds_drops(&self, expr: &ThirExpr<'tcx>) -> bool {
        if !self.cx.has_drops(expr.ty) {
            return false;
        }
        match &expr.kind {
            ExprKind::Adt(adt) => {
                self.cx
                    .tcx
                    .adt_destructor(adt.adt_def.did())
                    .is_some_and(|d| self.cx.runs_drop(d.did))
                    || adt.fields.iter().any(|f| self.cx.has_drops(self.thir[f.expr].ty))
                    || !matches!(adt.base, AdtExprBase::None)
            }
            _ => true,
        }
    }

    fn problem(&mut self, span: Span, what: &str) {
        self.facts.problems.push((span, what.to_string()));
    }

    fn taken(&self, pat: &Pat<'tcx>) -> Taken {
        match &pat.kind {
            PatKind::Wild => Taken::Nothing,
            PatKind::Binding {
                mode: BindingMode(ByRef::No, _),
                subpattern: None,
                ty,
                ..
            } if self.cx.has_drops(*ty) => Taken::Whole,
            _ => {
                let mut part = false;
                pat.walk_always(|p| {
                    if let PatKind::Binding {
                        mode: BindingMode(ByRef::No, _),
                        ty,
                        ..
                    } = p.kind
                    {
                        part |= self.cx.has_drops(ty);
                    }
                });
                if part { Taken::Part } else { Taken::Nothing }
            }
        }
    }

    /// A use of `var`, an owner, at the top of the walk.
    fn owner_used(&mut self, e: ExprId, var: LocalVarId) {
        let span = self.thir[e].span;
        let (parent, child) = self.context();
        let taken = match parent.map(|p| &self.thir[p].kind) {
            None => match self.lets.get(&child) {
                // `let (c, d) = b;` moves the parts it binds.
                Some(pat) if self.taken(pat) == Taken::Part => {
                    match self.cx.pattern_paths(pat) {
                        Some(paths) => self.facts.parts.entry(var).or_default().extend(paths),
                        None => self.problem(span, "moving part of a value with a destructor"),
                    }
                    return;
                }
                Some(pat) => self.taken(pat),
                // A statement of its own, `x;`, moves it, and drops it.
                None => Taken::Whole,
            },
            Some(ExprKind::Borrow { arg, .. } | ExprKind::RawBorrow { arg, .. }) if *arg == child => Taken::Nothing,
            Some(ExprKind::Index { lhs, .. } | ExprKind::AssignOp { lhs, .. }) if *lhs == child => Taken::Nothing,
            // Written over: its old value is dropped where it's lowered.
            Some(ExprKind::Assign { lhs, .. }) if *lhs == child => Taken::Nothing,
            Some(ExprKind::Field { lhs, .. }) if *lhs == child => {
                // A field moved out, `consume(pair.a)`: a part of its own.
                match self.moved_projection() {
                    Ok(Some((field, path))) => {
                        self.facts.parts.entry(var).or_default().push(path.clone());
                        self.facts.part_moves.insert(field, (var, path));
                    }
                    Ok(None) => {}
                    Err(()) => self.problem(span, "moving part of a value with a destructor"),
                }
                return;
            }
            Some(ExprKind::Match { scrutinee, arms, .. }) if *scrutinee == child => {
                let taken: Vec<Taken> = arms.iter().map(|&a| self.taken(&self.thir[a].pattern)).collect();
                if taken.contains(&Taken::Part) {
                    // Each arm moves the parts its pattern binds.
                    let mut paths = Vec::new();
                    for (&arm, taken) in arms.iter().zip(&taken) {
                        match (taken, self.cx.pattern_paths(&self.thir[arm].pattern)) {
                            (Taken::Whole, _) | (_, None) => {
                                self.problem(span, "moving part of a value with a destructor");
                                return;
                            }
                            (_, Some(found)) => paths.extend(found),
                        }
                    }
                    self.facts.parts.entry(var).or_default().extend(paths);
                    return;
                } else if taken.contains(&Taken::Whole) {
                    Taken::Whole
                } else {
                    Taken::Nothing
                }
            }
            Some(ExprKind::Let { expr, pat }) if *expr == child => self.taken(pat),
            Some(ExprKind::Closure(closure)) if closure.upvars.contains(&child) => {
                self.problem(span, "a closure that captures a value with a destructor");
                return;
            }
            Some(ExprKind::Adt(adt)) if matches!(adt.base, AdtExprBase::Base(ref fru) if fru.base == child) => {
                self.problem(span, "a struct update from a value with a destructor");
                return;
            }
            _ => Taken::Whole,
        };
        match taken {
            Taken::Nothing => {}
            Taken::Whole => {
                self.facts.moved.insert(var);
                self.facts.moves.insert(e);
            }
            Taken::Part => self.problem(span, "moving part of a value with a destructor"),
        }
    }

    /// The field of a projection chain from the variable at the top of the
    /// walk that's used as a value, moving it, and its path: none if what's
    /// used has nothing to drop, and an error through what isn't a struct or
    /// a tuple, as a `Box`.
    fn moved_projection(&self) -> Result<Option<(ExprId, Path)>, ()> {
        if !self.projection_moved() {
            return Ok(None);
        }
        let mut child = *self.stack.last().expect("the variable");
        let mut field = child;
        let mut path = Path::new();
        for &parent in self.stack.iter().rev().skip(1) {
            match &self.thir[parent].kind {
                ExprKind::Scope { .. } | ExprKind::PlaceTypeAscription { .. } => {}
                ExprKind::Field { lhs, name, .. } if *lhs == child => {
                    let ty = self.thir[*lhs].ty;
                    if !matches!(ty.kind(), ty::Tuple(_)) && !matches!(ty.kind(), ty::Adt(adt, _) if adt.is_struct()) {
                        return Err(());
                    }
                    path.push((None, name.as_usize()));
                    field = parent;
                }
                _ => break,
            }
            child = parent;
        }
        Ok(self.cx.has_drops(self.thir[field].ty).then_some((field, path)))
    }

    /// Whether the field at the top of the walk, of a projection chain, is
    /// used as a value, which moves it, rather than borrowed or written.
    fn projection_moved(&self) -> bool {
        let mut child = *self.stack.last().expect("the variable");
        for &parent in self.stack.iter().rev().skip(1) {
            match &self.thir[parent].kind {
                ExprKind::Scope { .. } | ExprKind::PlaceTypeAscription { .. } => {}
                ExprKind::Field { lhs, .. } if *lhs == child => {}
                ExprKind::Borrow { arg, .. } | ExprKind::RawBorrow { arg, .. } if *arg == child => return false,
                ExprKind::Assign { lhs, .. } | ExprKind::AssignOp { lhs, .. } | ExprKind::Index { lhs, .. }
                    if *lhs == child =>
                {
                    return false;
                }
                _ => return self.cx.has_drops(self.thir[child].ty),
            }
            child = parent;
        }
        self.cx.has_drops(self.thir[child].ty)
    }

    /// A value with a destructor, made here, at the top of the walk: one
    /// borrowed, or taken apart, is a temporary, dropped at the end of its
    /// statement, which isn't supported yet. So is one made before an
    /// operand after it that can leave early, which drops it as it leaves.
    fn value_made(&mut self, e: ExprId) {
        let (parent, child) = self.context();
        let span = self.thir[e].span;
        let siblings: Vec<ExprId> = match parent.map(|p| &self.thir[p].kind) {
            Some(ExprKind::Call { args, .. }) => args.to_vec(),
            Some(ExprKind::Tuple { fields } | ExprKind::Array { fields }) => fields.to_vec(),
            Some(ExprKind::Adt(adt)) => adt.fields.iter().map(|f| f.expr).collect(),
            _ => Vec::new(),
        };
        if let Some(at) = siblings.iter().position(|&s| s == child)
            && siblings[at + 1..].iter().any(|&s| !self.cx.cannot_leave(s))
        {
            self.facts.temps.insert(e, TempKind::Operand);
            return;
        }
        let taken = |finder: &Self, pats: &mut dyn Iterator<Item = &Pat<'tcx>>| {
            pats.map(|p| finder.taken(p)).any(|t| t != Taken::Nothing)
        };
        let used_in_place =
            match parent.map(|p| &self.thir[p].kind) {
                // `let (a, _) = (x, y);` takes a temporary apart, and drops the rest.
                None => {
                    if self.lets.get(&child).is_some_and(|pat| {
                        !matches!(pat.kind, PatKind::Wild | PatKind::Binding { subpattern: None, .. })
                    }) {
                        self.problem(span, "taking apart a temporary with a destructor");
                    }
                    return;
                }
                Some(ExprKind::Let { expr, pat }) if *expr == child => {
                    if taken(self, &mut std::iter::once(&**pat)) {
                        self.problem(span, "moving part of a temporary with a destructor");
                        return;
                    }
                    true
                }
                Some(ExprKind::Match { scrutinee, arms, .. }) if *scrutinee == child => {
                    if taken(self, &mut arms.iter().map(|&a| &*self.thir[a].pattern)) {
                        self.problem(span, "moving part of a temporary with a destructor");
                        return;
                    }
                    true
                }
                Some(
                    ExprKind::Borrow { arg, .. }
                    | ExprKind::RawBorrow { arg, .. }
                    | ExprKind::Field { lhs: arg, .. }
                    | ExprKind::Index { lhs: arg, .. }
                    | ExprKind::Deref { arg },
                ) => *arg == child,
                Some(ExprKind::Adt(adt)) if matches!(adt.base, AdtExprBase::Base(ref fru) if fru.base == child) => {
                    self.problem(span, "a struct update from a value with a destructor");
                    return;
                }
                _ => false,
            };
        if used_in_place {
            self.facts.temps.insert(e, TempKind::Place);
        }
    }
}

/// A place: a variable, or a part of one, or what a reference points to.
pub(super) fn is_place(kind: &ExprKind<'_>) -> bool {
    matches!(
        kind,
        ExprKind::VarRef { .. }
            | ExprKind::UpvarRef { .. }
            | ExprKind::Field { .. }
            | ExprKind::Index { .. }
            | ExprKind::Deref { .. }
            | ExprKind::StaticRef { .. }
            | ExprKind::Scope { .. }
            | ExprKind::Use { .. }
            | ExprKind::ValueTypeAscription { .. }
            | ExprKind::PlaceTypeAscription { .. }
            | ExprKind::NeverToAny { .. }
    )
}

impl<'c, 'a, 'tcx> Visitor<'a, 'tcx> for Finder<'c, 'a, 'tcx> {
    fn thir(&self) -> &'a Thir<'tcx> {
        self.thir
    }

    fn visit_stmt(&mut self, stmt: &'a rustc_middle::thir::Stmt<'tcx>) {
        if let ThirStmt::Let {
            initializer: Some(init),
            pattern,
            ..
        } = &stmt.kind
        {
            self.lets.insert(*init, pattern);
        }
        visit::walk_stmt(self, stmt);
    }

    fn visit_pat(&mut self, pat: &'a Pat<'tcx>) {
        if let PatKind::Binding {
            var,
            mode: BindingMode(ByRef::No, _),
            ty,
            ..
        } = pat.kind
        {
            match self.cx.drops(ty) {
                Drops::Nothing => {}
                Drops::Runs => {
                    self.facts.owners.insert(var, pat.span);
                }
                Drops::Unsupported(t, what) => self.problem(pat.span, &describe(t, what)),
            }
        }
        visit::walk_pat(self, pat);
    }

    fn visit_expr(&mut self, expr: &'a ThirExpr<'tcx>) {
        let id = self.id(expr);
        self.stack.push(id);
        match expr.kind {
            ExprKind::VarRef { id: var } if self.facts.owners.contains_key(&var) => self.owner_used(id, var),
            _ if !is_place(&expr.kind) && self.holds_drops(expr) => self.value_made(id),
            ExprKind::PointerCoercion {
                cast: PointerCoercion::Unsize,
                source,
                ..
            } if {
                // To a `dyn`: an array unsized to a slice is the same array.
                // What's unsized, and what to: `&D` or `Box<D>` to a `dyn`, and
                // `Rc<D>` to `Rc<dyn Send>` too, whose destructor then runs
                // through the `dyn`. Found by rustc's `issue-25515.rs`.
                let target = expr.ty;
                let from = self.thir[source].ty;
                let (to, from) = match (target.builtin_deref(true), from.builtin_deref(true)) {
                    (Some(to), Some(from)) => (to, from),
                    _ => match (target.kind(), from.kind()) {
                        (ty::Adt(_, to_args), ty::Adt(_, from_args)) => to_args
                            .types()
                            .zip(from_args.types())
                            .find(|(to, _)| matches!(to.kind(), ty::Dynamic(..)))
                            .unwrap_or((target, from)),
                        _ => (target, from),
                    },
                };
                matches!(to.kind(), ty::Dynamic(..)) && self.cx.drops(from) != Drops::Nothing
            } =>
            {
                self.problem(expr.span, "a `dyn` of a value with a destructor");
            }
            _ => {}
        }
        // A generic trait method of the crate's own, given a value with a
        // destructor for a type parameter, would need to be given its drop,
        // as a generic function is. A trait's `Self` is the impl's, which a
        // call resolves to.
        if let ty::FnDef(def_id, args) = *expr.ty.kind()
            && matches!(expr.kind, ExprKind::ZstLiteral { .. })
            && self
                .cx
                .tcx
                .trait_of_assoc(def_id)
                .is_some_and(|t| self.cx.is_rust_trait(t))
            && args
                .types()
                .skip(usize::from(self.cx.tcx.trait_of_assoc(def_id).is_some()))
                .any(|t| self.cx.drops(t) != Drops::Nothing)
        {
            self.problem(expr.span, "a generic function given a value with a destructor");
        }
        visit::walk_expr(self, expr);
        self.stack.pop();
    }
}
