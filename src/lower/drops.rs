//! Destructors (ADR 0098): the scopes, flags and temporaries that drop a
//! value where rustc does, and the JS that drops it. What dropping a type
//! runs is `types`'s, and what a body owns and moves, found before it's
//! lowered, `facts`'.
//!
//! A scope that owns such a value is a `try`, and its drops the `finally`.
//! What this can't do yet is an error, found before any JS is written: a
//! value with a destructor that nothing drops would be a wrong answer.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use rustc_hir::{self as hir, BindingMode, ByRef, HirId, Node};
use rustc_middle::middle::region;
use rustc_middle::thir::{BlockId, ExprId, ExprKind, LocalVarId, Pat, PatKind, StmtKind as ThirStmt};
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;
use rustc_span::def_id::DefId;

use super::bindings::variant_name;
use super::representation::variant_field;
use super::{FnCx, R, lower_first};
use crate::js::{self, Expr, Op, Stmt, StmtKind};

mod facts;
mod types;

pub(super) use facts::is_place;
use facts::{Facts, TempKind, binds_any, find_facts};
pub(super) use types::Drops;
use types::describe;

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
    parts: Vec<(Path, Option<String>)>,
}

/// A part of a value, field by field: each step the variant it's in, by
/// index, for an enum's, and the field.
pub(super) type Path = Vec<(Option<u32>, usize)>;

/// A temporary that ends with the statement being lowered.
pub(super) struct Temp<'tcx> {
    name: String,
    ty: Ty<'tcx>,
    flag: Option<String>,
    /// The operand it is, which must have been moved by the statement's end.
    operand: Option<ExprId>,
    /// The parts a pattern moves out of it: on every path, or with a flag.
    parts: Vec<(Path, Option<String>)>,
}

/// The statement being lowered, which a statement inside it saves and puts
/// back: the scopes its temporaries end in, its own and, for a `let`, the
/// rest of the block's, and the temporaries that end with it.
#[derive(Default)]
pub(super) struct Statement<'tcx> {
    scopes: Option<(region::Scope, Option<region::Scope>)>,
    temps: Vec<Temp<'tcx>>,
}

/// A copied default's drops: the drop functions of its trait's type
/// parameters, by index, and those it can't make, with why (ADR 0098).
pub(super) type DefaultDrops<'tcx> = (HashMap<u32, String>, HashMap<u32, (Ty<'tcx>, &'static str)>);

/// What a body inside another, a closure's or a copied default's, takes
/// from the drops of the one it's in while it's lowered, given back as it
/// ends: its statement and scopes, and a default's, the item's drops.
pub(super) struct EnclosingDrops<'tcx> {
    scopes: BodyScopes<'tcx>,
    swapped: Option<SwappedDrops<'tcx>>,
}

/// The statement and the expressions being lowered, whose temporaries a
/// body inside them, a closure's, has none of: it has its own.
struct BodyScopes<'tcx> {
    statement: Statement<'tcx>,
    open: Vec<(region::Scope, Vec<Temp<'tcx>>)>,
}

/// A function's drops while a copied default body has its own.
struct SwappedDrops<'tcx> {
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
    /// Each closure made here that holds a value with a destructor: the body
    /// it's made in, and the variables it holds, which its drop drops.
    closures: HashMap<DefId, (DefId, Vec<(Expr, Ty<'tcx>)>)>,
    /// The flags of each temporary's parts that a pattern moves on some paths.
    temp_part_flags: HashMap<(usize, ExprId), Vec<(Path, String)>>,
    /// The scopes of the expressions being lowered, innermost last, and the
    /// temporaries that end with each: a condition's, a block's tail's.
    open: Vec<(region::Scope, Vec<Temp<'tcx>>)>,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// A closure, at `closure`, made here: the places its drop drops.
    pub(super) fn closure_made(&mut self, closure: DefId, places: Vec<(Expr, Ty<'tcx>)>) {
        self.drop_state.closures.insert(closure, (self.body_owner, places));
    }

    /// The variables of the body being lowered that a closure flags as its
    /// own as it's lowered, given back once it is.
    pub(super) fn take_flags(&mut self, vars: &[LocalVarId]) -> Vec<(LocalVarId, Option<String>)> {
        vars.iter().map(|&v| (v, self.drop_state.flags.remove(&v))).collect()
    }

    pub(super) fn give_flags(&mut self, flags: Vec<(LocalVarId, Option<String>)>) {
        for (var, flag) in flags {
            match flag {
                Some(flag) => self.drop_state.flags.insert(var, flag),
                None => self.drop_state.flags.remove(&var),
            };
        }
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
                let (parts, recursive) = sum(
                    self,
                    &mut adt.all_fields().map(|f| f.ty(self.tcx, args).skip_normalization()),
                    stack,
                );
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
            // What a closure holds: the variables it took, where it was made.
            // Only there, or in a closure made inside it, can JS see them.
            ty::Closure(def_id, _) => {
                let Some((made_in, places)) = self.drop_state.closures.get(def_id).cloned() else {
                    return Err(self.unsupported(span, "dropping a closure that holds a value with a destructor here"));
                };
                if !std::iter::successors(Some(self.body_owner), |&d| self.tcx.opt_parent(d)).any(|d| d == made_in) {
                    return Err(self.unsupported(span, "dropping a closure that holds a value with a destructor here"));
                }
                for (place, ty) in places {
                    self.drop_in(place, ty, span, made, out)?;
                }
            }
            // A `dyn`'s: its dictionary's `$drop`, if what it holds has one.
            ty::Dynamic(..) => {
                let drop = Expr::member(Expr::member(value.clone(), "impl"), "$drop");
                out.push(
                    StmtKind::Expr(Expr {
                        kind: js::ExprKind::OptionalCall(Box::new(drop), vec![Expr::member(value, "value")]),
                        span: js_span,
                    })
                    .at(js_span),
                );
            }
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
                        self.drop_in(part, field.ty(self.tcx, args).skip_normalization(), span, made, out)?;
                    }
                } else if adt.is_enum() {
                    for variant in adt.variants() {
                        let mut fields = Vec::new();
                        for (i, field) in variant.fields.iter().enumerate() {
                            let part = Expr::member(value.clone(), variant_field(self.tcx, variant, i));
                            self.drop_in(
                                part,
                                field.ty(self.tcx, args).skip_normalization(),
                                span,
                                made,
                                &mut fields,
                            )?;
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
        parts: &[(Path, Option<String>)],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        if parts.is_empty() {
            return self.drop_value(value, ty, span, out);
        }
        let js_span = self.js_span(span);
        // This part itself moved on every path: nothing of it is left.
        if parts.iter().any(|(p, flag)| p.is_empty() && flag.is_none()) {
            return Ok(());
        }
        // This part itself moved, on some path: all of it only if it's owned.
        if let Some((_, Some(flag))) = parts.iter().find(|(p, _)| p.is_empty()) {
            let rest: Vec<(Path, Option<String>)> = parts.iter().filter(|(p, _)| !p.is_empty()).cloned().collect();
            let mut owned = Vec::new();
            self.drop_owned(value, ty, &rest, span, &mut owned)?;
            if !owned.is_empty() {
                out.push(StmtKind::If(Expr::var(flag), owned, None).at(js_span));
            }
            return Ok(());
        }
        let under = |step: (Option<u32>, usize)| -> Vec<(Path, Option<String>)> {
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
                    self.drop_owned(
                        part,
                        field.ty(self.tcx, args).skip_normalization(),
                        &under((None, i)),
                        span,
                        out,
                    )?;
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
                            field.ty(self.tcx, args).skip_normalization(),
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
        let mut parts: Vec<(Path, Option<String>)> = Vec::new();
        for path in facts.parts.get(&var).into_iter().flatten() {
            if parts.iter().any(|(p, _)| p == path) {
                continue;
            }
            let named = self.path_name(ty, path);
            let flag = self.fresh(&format!("{base}${named}$live"));
            out.push(StmtKind::Let(flag.clone(), Some(Expr::bool(true))).at(js_span));
            self.drop_state.part_flags.insert((var, path.clone()), flag.clone());
            parts.push((path.clone(), Some(flag)));
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
                    at = f.ty(self.tcx, args).skip_normalization();
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
        let js_span = self.js_span(pat.span);
        // A temporary's, taken apart (ADR 0131).
        let key = std::ptr::from_ref(self.thir) as usize;
        if let Some(flags) = self.drop_state.temp_part_flags.get(&(key, self.strip(scrutinee))) {
            let flags = flags.clone();
            for path in self.pattern_paths(pat).unwrap_or_default() {
                if let Some((_, flag)) = flags.iter().find(|(p, _)| *p == path) {
                    out.push(StmtKind::Assign(Expr::var(flag), Expr::bool(false)).at(js_span));
                }
            }
            return;
        }
        let ExprKind::VarRef { id } = self.thir[self.strip(scrutinee)].kind else {
            return;
        };
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

    /// Whether `e` is a temporary that a pattern takes apart (ADR 0131).
    pub(super) fn takes_apart_temporary(&mut self, e: ExprId) -> R<bool> {
        Ok(self.drop_facts()?.temp_parts.contains_key(&self.strip(e)))
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
        let (statement, rest) = match self.drop_state.statement.scopes {
            Some((statement, rest)) => (Some(statement), rest),
            None => (None, None),
        };
        let key = std::ptr::from_ref(self.thir) as usize;
        // The expression whose scope it ends with, if not the statement's.
        let mut open = None;
        let flag = match kind {
            TempKind::Operand => {
                // Outside any statement, a function body's tail's: the scope
                // of what moves it, the innermost that isn't its own.
                if statement.is_none() {
                    let own = self.thir[e].temp_scope_id;
                    match self.drop_state.open.iter().rposition(|(s, _)| s.local_id != own) {
                        Some(at) => open = Some(at),
                        None => return Err(self.unsupported(span, "a temporary with a destructor here")),
                    }
                }
                let flag = self.fresh(&format!("{name}$live"));
                out.push(StmtKind::Let(flag.clone(), Some(Expr::bool(true))).at(js_span));
                self.drop_state.temp_flags.insert((key, e), flag.clone());
                Some(flag)
            }
            TempKind::Place => {
                let tree = self.tcx.region_scope_tree(self.body_owner.expect_local());
                match tree.temporary_scope(self.thir[e].temp_scope_id).temp_lifetime {
                    // Never dropped, as a promoted constant isn't.
                    None => return Ok(Expr::var(&name)),
                    Some(scope) if statement.is_some_and(|st| scope == st || self.ends_with_statement(scope, st)) => {
                        None
                    }
                    // `let r = &f();`: it lives as long as `r` does, less what a
                    // `let` moves out of it, `let (a, ref b) = (f(), g());` (ADR 0131).
                    Some(scope) if Some(scope) == rest => {
                        let parts = match self.drop_facts()?.temp_parts.get(&e) {
                            Some((_, true)) => return Err(self.unsupported(span, "a temporary with a destructor here")),
                            Some((paths, false)) => paths.iter().map(|p| (p.clone(), None)).collect(),
                            None => Vec::new(),
                        };
                        self.drop_state.owned.push(Owned {
                            value: Expr::var(&name),
                            ty,
                            flag: None,
                            parts,
                        });
                        return Ok(Expr::var(&name));
                    }
                    // A condition's, a block's tail's: its expression's.
                    Some(scope) => match self.drop_state.open.iter().rposition(|(s, _)| *s == scope) {
                        Some(at) => {
                            open = Some(at);
                            None
                        }
                        None => return Err(self.unsupported(span, "a temporary with a destructor here")),
                    },
                }
            }
        };
        let operand = (kind == TempKind::Operand).then_some(e);
        // What a pattern moves out of it (ADR 0131): a part moved on only
        // some paths gets a flag, `tuple$0$live`, as a variable's does.
        let mut parts: Vec<(Path, Option<String>)> = Vec::new();
        if let Some((paths, flagged)) = self.drop_facts()?.temp_parts.get(&e) {
            let mut flags = Vec::new();
            for path in paths {
                if parts.iter().any(|(p, _)| p == path) {
                    continue;
                }
                let flag = flagged.then(|| {
                    let flag = match self.path_name(ty, path) {
                        named if named.is_empty() => self.fresh(&format!("{name}$live")),
                        named => self.fresh(&format!("{name}${named}$live")),
                    };
                    out.push(StmtKind::Let(flag.clone(), Some(Expr::bool(true))).at(js_span));
                    flags.push((path.clone(), flag.clone()));
                    flag
                });
                parts.push((path.clone(), flag));
            }
            self.drop_state.temp_part_flags.insert((key, e), flags);
        }
        let temp = Temp {
            name: name.clone(),
            ty,
            flag,
            operand,
            parts,
        };
        match open {
            Some(at) => self.drop_state.open[at].1.push(temp),
            None => self.drop_state.statement.temps.push(temp),
        }
        Ok(Expr::var(&name))
    }

    /// Whether `scope` ends as `statement` does: an `if let`'s, whose
    /// scrutinee's temporaries end with the `if` in Rust 2024, where the
    /// `if` is the whole statement and has no `else` to run after them.
    fn ends_with_statement(&self, scope: region::Scope, statement: region::Scope) -> bool {
        if scope.data != region::ScopeData::IfThenRescope {
            return false;
        }
        // The scope is the `then` block's, inside the `if`.
        let owner = self.tcx.local_def_id_to_hir_id(self.body_owner.expect_local()).owner;
        let then = HirId {
            owner,
            local_id: scope.local_id,
        };
        let id = self.tcx.parent_hir_id(then);
        matches!(self.tcx.hir_node(id), Node::Expr(e) if matches!(e.kind, hir::ExprKind::If(_, _, None)))
            && self.tcx.parent_hir_id(id).local_id == statement.local_id
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
        self.place_temps(statement.temps, lowered, span, out)
    }

    /// Start lowering a body inside the one being lowered: its own facts, and
    /// none of the enclosing one's statement and scopes. A copied default's,
    /// `default`, is given its trait's type parameters' drops and those it
    /// can't make (ADR 0098).
    pub(super) fn enter_body_drops(&mut self, default: Option<DefaultDrops<'tcx>>) -> R<EnclosingDrops<'tcx>> {
        let swapped = default.map(|(drops, unsupported)| self.swap_drops(drops, unsupported));
        self.drop_facts()?;
        Ok(EnclosingDrops {
            scopes: self.take_scopes(),
            swapped,
        })
    }

    /// Finish the body `enter_body_drops` started: every owner it has had a
    /// scope, and every move its flag cleared, and the enclosing body's back.
    pub(super) fn leave_body_drops(&mut self, enclosing: EnclosingDrops<'tcx>) -> R<()> {
        self.check_drops()?;
        self.give_scopes(enclosing.scopes);
        if let Some(swapped) = enclosing.swapped {
            self.restore_drops(swapped);
        }
        Ok(())
    }

    /// The enclosing body's statement and expressions, while a body inside
    /// it is lowered.
    fn take_scopes(&mut self) -> BodyScopes<'tcx> {
        BodyScopes {
            statement: std::mem::take(&mut self.drop_state.statement),
            open: std::mem::take(&mut self.drop_state.open),
        }
    }

    fn give_scopes(&mut self, scopes: BodyScopes<'tcx>) {
        self.drop_state.statement = scopes.statement;
        self.drop_state.open = scopes.open;
    }

    /// Start lowering the expression whose scope is `scope`, which a
    /// temporary may end with, as a condition's does.
    pub(super) fn begin_scope(&mut self, scope: region::Scope) {
        self.drop_state.open.push((scope, Vec::new()));
    }

    /// Finish the expression `begin_scope` started, whose JS is `out` from
    /// `mark` on: each of the temporaries that end with it is dropped by a
    /// `finally` after it.
    pub(super) fn end_scope(&mut self, mark: usize, span: Span, out: &mut Vec<Stmt>) -> R<()> {
        let (_, temps) = self.drop_state.open.pop().expect("a scope begun");
        if temps.is_empty() {
            return Ok(());
        }
        let lowered = out.split_off(mark);
        self.place_temps(temps, lowered, span, out)
    }

    /// Whether the innermost scope begun has temporaries that end with it.
    pub(super) fn scope_has_temps(&self) -> bool {
        self.drop_state.open.last().is_some_and(|(_, temps)| !temps.is_empty())
    }

    /// `lowered`, in `out`, with `temps` dropped after it, last first.
    fn place_temps(&mut self, temps: Vec<Temp<'tcx>>, lowered: Vec<Stmt>, span: Span, out: &mut Vec<Stmt>) -> R<()> {
        let key = std::ptr::from_ref(self.thir) as usize;
        // An operand temporary must have been moved where it's an operand.
        for t in &temps {
            if let Some(e) = t.operand
                && !self.drop_state.temps_moved.contains(&(key, e))
            {
                return Err(self.unsupported(
                    self.thir[e].span,
                    "a value with a destructor made before what may panic or leave early, here",
                ));
            }
        }
        if temps.is_empty() {
            out.extend(lowered);
            return Ok(());
        }
        // Each is declared in the scope's own JS, not in a branch of it.
        let mut placed = Vec::new();
        for t in temps {
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
        // Through its declaration, and its flags'.
        let mut end = at + 1;
        let flags: Vec<&String> = temp
            .flag
            .iter()
            .chain(temp.parts.iter().filter_map(|(_, f)| f.as_ref()))
            .collect();
        while matches!(lowered.get(end).map(|s| &s.kind), Some(StmtKind::Let(n, _)) if flags.contains(&n)) {
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
        self.drop_owned(Expr::var(&temp.name), temp.ty, &temp.parts, span, &mut drop)?;
        let js_span = self.js_span(span);
        let finally = match temp.flag {
            Some(flag) => vec![StmtKind::If(Expr::var(&flag), drop, None).at(js_span)],
            None => drop,
        };
        // Nothing to leave by, as `x = temporary[0];`: the drops come after.
        let cannot_leave = body
            .iter()
            .all(|s| matches!(&s.kind, StmtKind::Assign(Expr { kind: js::ExprKind::Var(_), .. }, value) if value.reads_same()));
        if body.is_empty() || cannot_leave {
            out.extend(body);
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
    fn swap_drops(
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

    fn restore_drops(&mut self, swapped: SwappedDrops<'tcx>) {
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

    /// `value`, a parameter `pat` takes apart, owned by the scope, less
    /// what `pat` moves out of it, which is moved on every path (ADR 0131).
    pub(super) fn own_rest(&mut self, value: Expr, ty: Ty<'tcx>, pat: &Pat<'tcx>) -> R<()> {
        let paths = self
            .pattern_paths(pat)
            .ok_or_else(|| self.unsupported(pat.span, "moving part of a value with a destructor"))?;
        self.drop_state.owned.push(Owned {
            value,
            ty,
            flag: None,
            parts: paths.into_iter().map(|p| (p, None)).collect(),
        });
        Ok(())
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
            // Its drop drops what it holds, not the function.
            ty::Closure(..) => true,
            ty::Adt(_, args) if ty.is_box() => self.drops_once(args.type_at(0)),
            ty::Adt(adt, args) => {
                self.tcx
                    .adt_destructor(adt.did())
                    .is_some_and(|d| self.runs_drop(d.did))
                    && adt.is_struct()
                    && adt
                        .all_fields()
                        .all(|f| !self.has_drops(f.ty(self.tcx, args).skip_normalization()))
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
        super::effects::cannot_leave_in(self.tcx, self.thir, e)
    }

    /// `e` moves a variable that owns a value with a destructor: it's not
    /// owned from here, so its flag is cleared first.
    pub(super) fn moved(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<()> {
        let e = self.strip(e);
        let facts = self.drop_facts()?;
        // `*b` of a box: what it holds is moved out, which is all the box owns.
        let e = match self.thir[e].kind {
            ExprKind::Deref { arg } if facts.moves.contains(&self.strip(arg)) => self.strip(arg),
            _ => e,
        };
        let key = std::ptr::from_ref(self.thir) as usize;
        // A part moved out, `pair.a`: its flag.
        let flag = if let Some((var, path)) = facts.part_moves.get(&e) {
            self.drop_state.part_flags.get(&(*var, path.clone())).cloned()
        } else if let ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } = self.thir[e].kind
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

    /// Whether `base`, a struct update's, is an owner it moves parts of.
    pub(super) fn update_moves(&mut self, base: ExprId) -> R<bool> {
        let base = self.strip(base);
        Ok(self.drop_facts()?.updates.contains_key(&base))
    }

    /// A struct update, `..base`, once every field's value is made: the
    /// parts it moves out of `base`, an owner, are no longer its own.
    pub(super) fn update_moved(&mut self, base: ExprId, out: &mut Vec<Stmt>) -> R<()> {
        let base = self.strip(base);
        let facts = self.drop_facts()?;
        let Some((var, paths)) = facts.updates.get(&base) else {
            return Ok(());
        };
        let key = std::ptr::from_ref(self.thir) as usize;
        self.drop_state.lowered_moves.insert((key, base));
        let js_span = self.js_span(self.thir[base].span);
        for path in paths {
            if let Some(flag) = self.drop_state.part_flags.get(&(*var, path.clone())) {
                out.push(StmtKind::Assign(Expr::var(flag), Expr::bool(false)).at(js_span));
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
            // A box's value that may have moved, `*b = ..` after `consume(*b)`:
            // the box's own flag, as it was cleared by the move.
            ExprKind::Deref { arg } if self.thir[arg].ty.is_box() => match self.thir[self.strip(arg)].kind {
                ExprKind::VarRef { id } => self.drop_state.flags.get(&id).cloned(),
                _ => None,
            },
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
        for &e in facts
            .moves
            .iter()
            .chain(facts.part_moves.keys())
            .chain(facts.updates.keys())
        {
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
