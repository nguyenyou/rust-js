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
use rustc_middle::mir::BinOp;
use rustc_middle::thir::visit::{self, Visitor};
use rustc_middle::thir::{
    AdtExprBase, BlockId, Expr as ThirExpr, ExprId, ExprKind, LocalVarId, Pat, PatKind, StmtKind as ThirStmt, Thir,
};
use rustc_middle::ty::adjustment::PointerCoercion;
use rustc_middle::ty::{self, Ty};
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
    /// What this body does that isn't supported yet.
    problems: Vec<(Span, String)>,
}

impl Facts {
    pub(super) fn has_owners(&self) -> bool {
        !self.owners.is_empty()
    }
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
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
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
            // Never dropped, or dropped by hand.
            ty::Adt(..) if self.is_lang_adt(ty, LangItem::ManuallyDrop) || std("MaybeUninit") => Drops::Nothing,
            ty::Adt(adt, args) => {
                let own = self.tcx.adt_destructor(adt.did());
                let parts = |walk: &mut Walk<'tcx>| {
                    let mut fields = adt.all_fields().map(|f| f.ty(self.tcx, args));
                    all(self, &mut fields, walk)
                };
                match own {
                    Some(d) if d.did.is_local() => match parts(walk) {
                        Drops::Unsupported(t, what) => Drops::Unsupported(t, what),
                        _ => Drops::Runs,
                    },
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
                let own = usize::from(self.tcx.adt_destructor(adt.did()).is_some_and(|d| d.did.is_local()));
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
            && adt.did().is_local()
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
            ty::Array(item, _) | ty::Slice(item) => self.drop_items(value, *item, span, made, out)?,
            ty::Tuple(items) => {
                for (i, item) in items.iter().enumerate() {
                    let part = self.project(value.clone(), ty, i);
                    self.drop_in(part, item, span, made, out)?;
                }
            }
            ty::Adt(adt, args) => {
                if let Some(d) = self.tcx.adt_destructor(adt.did())
                    && d.did.is_local()
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
        let facts = self.drop_facts()?;
        let flag = facts.moved.contains(&var).then(|| {
            let base = match &value.kind {
                js::ExprKind::Var(name) => name.clone(),
                _ => "value".into(),
            };
            let flag = self.fresh(&format!("{base}$live"));
            out.push(StmtKind::Let(flag.clone(), Some(Expr::bool(true))).at(self.js_span(span)));
            self.drop_state.flags.insert(var, flag.clone());
            flag
        });
        self.drop_state.registered.insert(var);
        self.drop_state.owned.push(Owned { value, ty, flag });
        Ok(())
    }

    /// A value its scope drops that no variable names, as a `_` parameter.
    pub(super) fn own_value(&mut self, value: Expr, ty: Ty<'tcx>) {
        self.drop_state.owned.push(Owned { value, ty, flag: None });
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
            self.drop_value(o.value, o.ty, span, &mut drop)?;
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
                self.tcx.adt_destructor(adt.did()).is_some_and(|d| d.did.is_local())
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
        let ExprKind::VarRef { id } = self.thir[e].kind else {
            return Ok(());
        };
        if !self.drop_facts()?.moves.contains(&e) {
            return Ok(());
        }
        let key = std::ptr::from_ref(self.thir) as usize;
        self.drop_state.lowered_moves.insert((key, e));
        if let Some(flag) = self.drop_state.flags.get(&id) {
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
            .filter(|e| facts.moves.contains(e))
            .collect();
        for &e in &moves {
            self.drop_state.deferred.insert((key, e), None);
        }
        Ok(moves)
    }

    /// Once `values`, the operands, are evaluated: what has effects goes in
    /// a `const`, in order, then the moves' flags are cleared.
    pub(super) fn end_moves(&mut self, moves: &[ExprId], values: &mut [Expr], out: &mut Vec<Stmt>) {
        let key = std::ptr::from_ref(self.thir) as usize;
        let clears: Vec<Stmt> = moves
            .iter()
            .filter_map(|&e| self.drop_state.deferred.remove(&(key, e)).flatten())
            .collect();
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
            _ => None,
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
        for &e in &facts.moves {
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
                    .is_some_and(|d| d.did.is_local())
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
                Some(pat) => self.taken(pat),
                // A statement of its own, `x;`, moves it, and drops it.
                None => Taken::Whole,
            },
            Some(ExprKind::Borrow { arg, .. } | ExprKind::RawBorrow { arg, .. }) if *arg == child => Taken::Nothing,
            Some(ExprKind::Index { lhs, .. } | ExprKind::AssignOp { lhs, .. }) if *lhs == child => Taken::Nothing,
            // Written over: its old value is dropped where it's lowered.
            Some(ExprKind::Assign { lhs, .. }) if *lhs == child => Taken::Nothing,
            Some(ExprKind::Field { lhs, .. }) if *lhs == child => {
                let field = parent.expect("a field");
                match self.cx.has_drops(self.thir[field].ty) && self.projection_moved() {
                    true => Taken::Part,
                    false => Taken::Nothing,
                }
            }
            Some(ExprKind::Match { scrutinee, arms, .. }) if *scrutinee == child => {
                let taken: Vec<Taken> = arms.iter().map(|&a| self.taken(&self.thir[a].pattern)).collect();
                if taken.contains(&Taken::Part) {
                    Taken::Part
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
            self.problem(
                span,
                "a value with a destructor made before what may panic or leave early",
            );
            return;
        }
        let temporary = match parent.map(|p| &self.thir[p].kind) {
            None => self
                .lets
                .get(&child)
                .is_some_and(|pat| !matches!(pat.kind, PatKind::Wild | PatKind::Binding { subpattern: None, .. })),
            Some(
                ExprKind::Borrow { arg, .. }
                | ExprKind::RawBorrow { arg, .. }
                | ExprKind::Let { expr: arg, .. }
                | ExprKind::Match { scrutinee: arg, .. }
                | ExprKind::Field { lhs: arg, .. }
                | ExprKind::Index { lhs: arg, .. }
                | ExprKind::Deref { arg },
            ) => *arg == child,
            Some(ExprKind::Adt(adt)) => matches!(adt.base, AdtExprBase::Base(ref fru) if fru.base == child),
            _ => false,
        };
        if temporary {
            self.problem(span, "a temporary with a destructor");
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
                let source = self.thir[source].ty;
                let pointee = source.builtin_deref(true).unwrap_or(source);
                let pointee = match pointee.kind() {
                    ty::Adt(_, args) if pointee.is_box() => args.type_at(0),
                    _ => pointee,
                };
                self.cx.drops(pointee) != Drops::Nothing
            } =>
            {
                self.problem(expr.span, "a `dyn` of a value with a destructor");
            }
            _ => {}
        }
        // A generic function of the crate's own, given a value with a
        // destructor for a type parameter, would need to be given its drop.
        // A trait's `Self` is the impl's, which a call resolves to.
        if let ty::FnDef(def_id, args) = *expr.ty.kind()
            && matches!(expr.kind, ExprKind::ZstLiteral { .. })
            && (self.cx.krate.fns.contains_key(&def_id)
                || self.cx.tcx.trait_of_assoc(def_id).is_some_and(|t| t.is_local()))
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
