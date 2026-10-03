//! A `&mut` to a value JS can't change in place, given to a call or given
//! back by one: a box the caller copies back, the place itself, or the
//! items a std call holds (ADRs 0072, 0099).

use super::bindings::{self};
use super::{FnCx, R, camel_case};
use crate::js::{Expr, Prop, Stmt, StmtKind};
use rustc_ast::Mutability;
use rustc_middle::thir::{ExprId, ExprKind, LocalVarId};
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;
use rustc_span::def_id::DefId;
use std::collections::{HashMap, HashSet};

/// How an argument is given to a function that takes boxes (`call_with_boxes`).
/// What the `&mut`s to values JS can't change in place, in the body being
/// lowered, are: boxes, aliases of places, a map's entry, or a std call's
/// items. Only this module reads or writes them.
#[derive(Default)]
pub(super) struct MutRefs {
    /// A `&mut` to a map's value that's a primitive, `if let Some(n) =
    /// m.get_mut(&k)`: a copy of it, and the map and key a write puts it back
    /// in (ADR 0059). While it lives, nothing else can change that entry.
    slots: HashMap<LocalVarId, (Expr, Expr)>,
    /// Parameters that are a `&mut` to a value JS can't change in place, a
    /// `String` or a number: a `{ value }` box the caller copies back (ADR 0072).
    boxes: HashSet<LocalVarId>,
    /// Variables bound once to a `&mut` of a value that isn't an object:
    /// each names the place it borrowed, so `*y = 5` writes it (ADR 0099).
    aliases: HashSet<LocalVarId>,
    /// The `let`s a temporary a `&mut` is to has as its home, `&mut Some(3)`
    /// matched: places its `ref mut` bindings write (ADR 0099).
    temporaries: HashSet<String>,
    /// The std calls a pattern matches whose `&mut`s to values JS can't
    /// change in place are the items, `m.get_mut(&k)`'s: bound, each is
    /// the item, not a cell (ADR 0099).
    item_calls: HashSet<ExprId>,
    /// Their bindings: `*v` reads the item, but `v` has no place to give
    /// as a `&mut`, which would write the binding's copy (ADR 0099).
    items: HashSet<LocalVarId>,
}

pub(super) enum ArgForm {
    /// As any argument is.
    Value,
    /// Its place, in a box, taken back out after the call.
    Boxed(ExprId),
    /// What's in a box, the variable's, given to a parameter that isn't one.
    Unboxed(LocalVarId),
}

/// What a call with boxes calls (`boxed_callee`).
pub(super) enum Callee<'tcx> {
    Fn(DefId, ty::GenericArgsRef<'tcx>),
    /// A trait's method, in this dictionary of its impl's (ADR 0049).
    Dictionary(DefId, ty::GenericArgsRef<'tcx>, Expr),
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// `var` is a `{ value }` box of a `String` or a number, as a `&mut`
    /// parameter is (ADR 0072).
    pub(super) fn bind_boxed(&mut self, var: LocalVarId) {
        self.locals.mut_refs.boxes.insert(var);
    }

    pub(super) fn is_boxed(&self, var: LocalVarId) -> bool {
        self.locals.mut_refs.boxes.contains(&var)
    }

    /// `var` names the place a `&mut` it's bound to borrowed (ADR 0099).
    pub(super) fn bind_alias(&mut self, var: LocalVarId) {
        self.locals.mut_refs.aliases.insert(var);
    }

    pub(super) fn is_alias(&self, var: LocalVarId) -> bool {
        self.locals.mut_refs.aliases.contains(&var)
    }

    /// `var` is a copy of a map's entry, `map` and `key` where a write puts
    /// it back (ADR 0059).
    pub(super) fn bind_slot(&mut self, var: LocalVarId, slot: (Expr, Expr)) {
        self.locals.mut_refs.slots.insert(var, slot);
    }

    pub(super) fn slot(&self, var: LocalVarId) -> Option<&(Expr, Expr)> {
        self.locals.mut_refs.slots.get(&var)
    }

    /// `name`, a `let`, is the home of a temporary a `&mut` is to (ADR 0099).
    pub(super) fn home_temporary(&mut self, name: String) {
        self.locals.mut_refs.temporaries.insert(name);
    }

    pub(super) fn is_temporary_home(&self, name: &str) -> bool {
        self.locals.mut_refs.temporaries.contains(name)
    }

    /// `call`, a std call a pattern matches, has its items as its `&mut`s.
    pub(super) fn mark_item_call(&mut self, call: ExprId) {
        self.locals.mut_refs.item_calls.insert(call);
    }

    pub(super) fn is_item_call(&self, call: ExprId) -> bool {
        self.locals.mut_refs.item_calls.contains(&call)
    }

    /// `var` is bound to one of those items.
    pub(super) fn bind_item(&mut self, var: LocalVarId) {
        self.locals.mut_refs.items.insert(var);
    }

    pub(super) fn is_item(&self, var: LocalVarId) -> bool {
        self.locals.mut_refs.items.contains(&var)
    }

    /// What a `&mut` argument points at, to read and write: `p` of `&mut p`,
    /// or a box's `value` (ADR 0074). A place with an item in it isn't one:
    /// its index would be evaluated at each use.
    pub(super) fn mut_place(&self, arg: ExprId, span: Span) -> R<Expr> {
        if let Some(place) = self.mut_borrowed(arg) {
            if self.element(place).is_none() {
                return self.assignee(place);
            }
        } else if let ExprKind::VarRef { id } = self.thir[self.strip(arg)].kind
            && self.is_boxed(id)
            && let Some((boxed, _)) = self.place(arg)
        {
            return Ok(Expr::member(boxed, "value"));
        }
        Err(self.unsupported(span, "this `&mut` argument, which isn't to a variable or a field"))
    }

    /// `p` of `&mut p`, a reborrow's `&mut *&mut v[0]` too: `v[0]`.
    pub(super) fn mut_borrowed(&self, arg: ExprId) -> Option<ExprId> {
        let ExprKind::Borrow {
            borrow_kind: rustc_middle::mir::BorrowKind::Mut { .. },
            arg: mut place,
        } = self.thir[self.strip(arg)].kind
        else {
            return None;
        };
        while let ExprKind::Deref { arg: inner } = self.thir[self.strip(place)].kind
            && let ExprKind::Borrow {
                borrow_kind: rustc_middle::mir::BorrowKind::Mut { .. },
                arg: reborrowed,
            } = self.thir[self.strip(inner)].kind
        {
            place = reborrowed;
        }
        Some(place)
    }

    /// What `a`, a `&` of a `&mut` to a value JS can't change in place, points
    /// at, as a comparison reads it (ADR 0099): the place of `&mut x` or of a
    /// `&mut` in a variable, a temporary of `&mut 1`, a cell's `value`.
    pub(super) fn pointee_value(&mut self, a: ExprId, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let ExprKind::Borrow {
            borrow_kind: rustc_middle::mir::BorrowKind::Shared,
            arg: e,
        } = self.thir[self.strip(a)].kind
        else {
            return Err(self.unsupported(span, "comparing this `&mut`"));
        };
        match self.thir[self.strip(e)].kind {
            ExprKind::Borrow {
                borrow_kind: rustc_middle::mir::BorrowKind::Mut { .. },
                arg,
            } => match self.place(arg) {
                Some((place, _)) => Ok(place),
                None if self.is_temporary(arg) => self.expr(arg, out),
                None => self.referent(arg, out),
            },
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } if !self.is_boxed(id) => self
                .place(e)
                .map(|(place, _)| place)
                .ok_or_else(|| self.unsupported(span, "comparing this `&mut`")),
            _ if self.is_cell_value(e) => Ok(Expr::member(self.expr(e, out)?, "value")),
            _ => Err(self.unsupported(span, "comparing this `&mut`")),
        }
    }

    /// A `&mut` to a value JS can't change in place that a std call's result,
    /// or the items of the iterator it is, holds, that neither its arguments
    /// nor its type's parameters did: one the call made, `get_mut`'s or
    /// `iter_mut`'s, which is the item itself (ADR 0099).
    pub(super) fn makes_items(
        &self,
        output: Ty<'tcx>,
        generic_args: ty::GenericArgsRef<'tcx>,
        args: &[ExprId],
    ) -> Option<Ty<'tcx>> {
        let cells = |ty: Ty<'tcx>| ty.walk().filter_map(|part| part.as_type()).filter(|&t| self.is_cell(t));
        let given: Vec<_> = args
            .iter()
            .map(|&a| self.thir[a].ty)
            .chain(generic_args.types())
            .flat_map(cells)
            .collect();
        let mut made = cells(output).chain(self.iterator_item(output).into_iter().flat_map(cells));
        made.find(|t| !given.contains(t))
    }

    /// Is `e` a std call whose `&mut`s are the items (`makes_items`)? A
    /// pattern matching it may take them apart, each binding the item.
    pub(super) fn item_subject(&mut self, e: ExprId) -> bool {
        let ExprKind::Call { fun, ref args, .. } = self.thir[self.strip(e)].kind else {
            return false;
        };
        let ty::FnDef(def_id, generic_args) = *self.thir[self.strip(fun)].ty.kind() else {
            return false;
        };
        let (def_id, generic_args) = self.callee(def_id, generic_args);
        if self.is_rust_fn(def_id) || self.makes_items(self.thir[e].ty, generic_args, args).is_none() {
            return false;
        }
        self.mark_item_call(fun);
        true
    }

    /// A `&mut` to one of `def_id`'s type parameters that's an object in a
    /// call of it, `generic_args`, anywhere but a parameter or its result: in
    /// a field of the crate's own type, an `Option` or a `Vec`, or a closure's
    /// parameters. Generic code has a cell there (ADR 0099), and the caller
    /// the object; a parameter is given a box, and the result's taken out.
    pub(super) fn nested_mut_object(&self, def_id: DefId, generic_args: ty::GenericArgsRef<'tcx>) -> Option<Ty<'tcx>> {
        let sig = self
            .tcx
            .fn_sig(def_id)
            .instantiate_identity()
            .skip_normalization()
            .skip_binder();
        let param_env = self.tcx.param_env(def_id);
        let mut todo: Vec<Ty<'tcx>> = sig
            .inputs_and_output
            .iter()
            .filter(|ty| !matches!(*ty.kind(), ty::Ref(_, pointee, Mutability::Mut) if self.is_generic_boxed(pointee, param_env)))
            .collect();
        for (clause, _) in self.tcx.predicates_of(def_id).instantiate_identity(self.tcx) {
            let clause = clause.skip_normalization();
            if let Some(bound) = clause.as_trait_clause() {
                todo.extend(bound.skip_binder().trait_ref.args.types());
            }
            if let Some(projection) = clause.as_projection_clause() {
                let projection = projection.skip_binder();
                todo.extend(projection.projection_term.args.types());
                todo.extend(projection.term.as_type());
            }
        }
        let mut seen = HashSet::new();
        while let Some(ty) = todo.pop() {
            for part in ty.walk().filter_map(|part| part.as_type()) {
                if !seen.insert(part) {
                    continue;
                }
                if let ty::Ref(_, pointee, Mutability::Mut) = *part.kind()
                    && self.is_generic_boxed(pointee, param_env)
                    && !self.is_cell_pointee(self.instantiated(pointee, generic_args))
                {
                    return Some(Ty::new_mut_ref(
                        self.tcx,
                        self.tcx.lifetimes.re_erased,
                        self.instantiated(pointee, generic_args),
                    ));
                }
                if let ty::Adt(adt, args) = *part.kind()
                    && !self.is_std(adt.did())
                {
                    todo.extend(adt.all_fields().map(|field| self.field_ty(field, args)));
                }
            }
        }
        None
    }

    /// Can what `fn_id` returns hold the borrow its parameter `i` is given: does
    /// its return type name a lifetime that parameter's type does (ADR 0099)?
    pub(super) fn result_borrows(&self, fn_id: DefId, i: usize) -> bool {
        let sig = self
            .tcx
            .fn_sig(fn_id)
            .instantiate_identity()
            .skip_normalization()
            .skip_binder();
        let Some(&input) = sig.inputs().get(i) else {
            return false;
        };
        let regions = |ty: Ty<'tcx>| ty.walk().filter_map(|part| part.as_region()).collect::<Vec<_>>();
        let returned = regions(sig.output());
        regions(input).iter().any(|region| returned.contains(region))
    }

    /// How `arg` is given as parameter `i` of `fn_id`: in a box, if that's a
    /// box (`param_is_box`), and its place isn't one already.
    pub(super) fn arg_form(&self, fn_id: DefId, i: usize, arg: ExprId) -> ArgForm {
        let Some(place) = self.mut_borrowed(arg) else {
            return ArgForm::Value;
        };
        let param_box = self.param_is_box(fn_id, i);
        // A `&mut` to a number given where a `T` goes is a box too, as a `&mut`
        // to one is anywhere (ADR 0074).
        let generic = self
            .tcx
            .fn_sig(fn_id)
            .instantiate_identity()
            .skip_normalization()
            .skip_binder()
            .inputs()
            .get(i)
            .is_some_and(|input| matches!(input.kind(), ty::Param(_)));
        // `&mut *out` of a box: the box itself, or, to a parameter that's
        // the value, as an object impl's `&mut self` is, what's in it.
        if let ExprKind::Deref { arg: inner } = self.thir[self.strip(place)].kind
            && let ExprKind::VarRef { id } = self.thir[self.strip(inner)].kind
            && self.is_boxed(id)
        {
            return if param_box || generic {
                ArgForm::Value
            } else {
                ArgForm::Unboxed(id)
            };
        }
        if param_box || (generic && self.is_boxable(self.thir[place].ty)) {
            ArgForm::Boxed(place)
        } else {
            ArgForm::Value
        }
    }

    /// What `f(args)` calls, when an argument must be boxed or taken out of
    /// its box: the function, the impl's method a trait's resolves to, or
    /// the trait's method in its dictionary, for a `Self` that isn't known or
    /// a default (ADR 0099).
    pub(super) fn boxed_callee(
        &mut self,
        def_id: DefId,
        generic_args: ty::GenericArgsRef<'tcx>,
        args: &[ExprId],
        span: Span,
    ) -> R<Option<Callee<'tcx>>> {
        let (fn_id, fn_args, dictionary) = match self.tcx.trait_of_assoc(def_id) {
            None if self.is_rust_fn(def_id) => (def_id, generic_args, None),
            None => return Ok(None),
            Some(trait_id) => match self.impl_method(def_id, generic_args)? {
                Some((method, method_args)) => (method, method_args, None),
                None if self.is_rust_trait(trait_id) => {
                    let generic_args = self.in_impl_terms(generic_args);
                    let tr = ty::TraitRef::from_assoc(self.tcx, trait_id, generic_args);
                    if matches!(tr.self_ty().kind(), ty::Dynamic(..)) {
                        return Ok(None);
                    }
                    (def_id, generic_args, Some(tr))
                }
                None => return Ok(None),
            },
        };
        let forms: Vec<ArgForm> = args
            .iter()
            .enumerate()
            .map(|(i, &a)| self.arg_form(fn_id, i, a))
            .collect();
        if forms.iter().all(|form| matches!(form, ArgForm::Value)) {
            return Ok(None);
        }
        Ok(Some(match dictionary {
            None => Callee::Fn(fn_id, fn_args),
            Some(tr) => Callee::Dictionary(fn_id, fn_args, self.dictionary(tr, span)?),
        }))
    }

    /// `f(&mut p)` with `p` a `String` or a number: `p` goes in a box named as
    /// `f`'s parameter, and back out after the call. That's exact: while `f`
    /// has the `&mut`, nothing else can read or write `p`.
    pub(super) fn call_with_boxes(
        &mut self,
        callee: Callee<'tcx>,
        args: &[ExprId],
        discarded: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let (def_id, generic_args) = match callee {
            Callee::Fn(def_id, generic_args) | Callee::Dictionary(def_id, generic_args, _) => (def_id, generic_args),
        };
        let inputs = self
            .tcx
            .fn_sig(def_id)
            .instantiate_identity()
            .skip_normalization()
            .skip_binder()
            .inputs()
            .to_vec();
        let names: Vec<String> = self
            .tcx
            .fn_arg_idents(def_id)
            .iter()
            .map(|ident| ident.map_or("value".to_string(), |i| i.name.to_string()))
            .collect();
        let js_span = self.js_span(span);
        let (mut values, mut backs) = (Vec::new(), Vec::new());
        for (i, &arg) in args.iter().enumerate() {
            match self.arg_form(def_id, i, arg) {
                // What it returns can hold the borrow, `pick(&mut a, &mut b)`: a
                // handle, since a box would be copied back before it's used.
                // `bump(&mut 5)`: a box of it, and nothing to take back.
                ArgForm::Boxed(place) if self.is_temporary(place) => {
                    let value = self.expr(place, out)?;
                    values.push(Expr::object(vec![Prop::Field("value".into(), value)]));
                }
                // `grow(&mut *shape)` of a `dyn`: a box of it, and nothing to take
                // back, since nothing replaces an unsized value through a `&mut`.
                ArgForm::Boxed(place) if !self.thir[place].ty.is_sized(self.tcx, self.typing_env) => {
                    let value = self.expr(place, out)?;
                    values.push(Expr::object(vec![Prop::Field("value".into(), value)]));
                }
                ArgForm::Boxed(place) if self.result_borrows(def_id, i) => {
                    let handle = Expr::handle(self.fixed_place(place, span, out)?);
                    values.push(handle);
                }
                ArgForm::Boxed(place) => {
                    let current = self.expr(place, out)?;
                    let target = match self.element(place) {
                        Some(_) => self.element_target(place, out)?,
                        None => self.assignee(place)?,
                    };
                    let name = self.fresh(&camel_case(names.get(i).map_or("value", String::as_str)));
                    let boxed = Expr::object(vec![Prop::Field("value".into(), current)]);
                    out.push(StmtKind::Const(name.clone(), boxed).at(js_span));
                    backs.push((target, name.clone()));
                    values.push(Expr::var(&name));
                }
                ArgForm::Unboxed(id) => values.push(Expr::member(self.locals.vars[&id].place.clone(), "value")),
                ArgForm::Value => {
                    let mut value = self.expr(arg, out)?;
                    // An iterator of the crate's own, given where a generic one
                    // goes, is a JS iterator (ADR 0061).
                    if let Some(&input) = inputs.get(i) {
                        value = self.iterator_arg((def_id, input), (arg, value), self.thir[arg].ty, span, out)?;
                    }
                    let value = if value.reads_same() {
                        value
                    } else {
                        self.spill("arg", value, out)
                    };
                    values.push(value);
                }
            }
        }
        let call = match callee {
            Callee::Fn(..) => {
                values.extend(self.evidence_args(def_id, generic_args, span)?);
                Expr::call(self.fn_ref(def_id), values)
            }
            Callee::Dictionary(_, _, dictionary) => {
                Expr::call(Expr::member(dictionary, bindings::fn_name(self.tcx, def_id)), values)
            }
        };
        let output = self
            .tcx
            .fn_sig(def_id)
            .instantiate(self.tcx, generic_args)
            .skip_normalization()
            .skip_binder()
            .output();
        let result = if discarded || output.is_unit() {
            out.push(StmtKind::Expr(call).at(js_span));
            Expr::undefined()
        } else {
            self.spill("result", call, out)
        };
        for (target, name) in backs {
            out.push(StmtKind::Assign(target, Expr::member(Expr::var(&name), "value")).at(js_span));
        }
        Ok(result)
    }

    /// What a call of the crate's gives back, as its caller has it: a generic
    /// `&mut T` it returns is a cell (ADR 0099), and of a `T` that's an object
    /// here, the caller's own `&mut` to one is the object, what's in it. One
    /// inside what it takes or returns, a `Vec<&mut T>`, isn't taken apart yet.
    pub(super) fn generic_result(&self, fun: ExprId, value: Expr, span: Span) -> R<Expr> {
        let ty::FnDef(def_id, generic_args) = *self.thir[self.strip(fun)].ty.kind() else {
            return Ok(value);
        };
        let (def_id, generic_args) = self.callee(def_id, generic_args);
        if !self.is_rust_fn(def_id) {
            return Ok(value);
        }
        if let Some(here) = self.nested_mut_object(def_id, generic_args) {
            let what = format!("a `{here}` inside a generic function's parameters or result");
            return Err(self.unsupported(span, &what));
        }
        let declared = self
            .tcx
            .fn_sig(def_id)
            .instantiate_identity()
            .skip_normalization()
            .skip_binder()
            .output();
        Ok(match *declared.kind() {
            ty::Ref(_, pointee, Mutability::Mut)
                if self.is_generic_boxed(pointee, self.tcx.param_env(def_id))
                    && !self.is_cell_pointee(self.instantiated(pointee, generic_args)) =>
            {
                Expr::member(value, "value")
            }
            _ => value,
        })
    }
}
