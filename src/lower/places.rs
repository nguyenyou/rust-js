//! Read, borrow and write places. Prepared targets lower each operand once.

use super::numbers::assign_op;
use super::{Dest, FnCx, R, Std, is_union, js_name, maps};
use crate::js::{self, Expr, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_ast::Mutability;
use rustc_middle::thir::{ExprId, ExprKind};
use rustc_middle::ty;
use rustc_span::Span;

/// An assignment target whose setup has been sequenced after its RHS.
/// Reading and writing this target never lower Rust expressions again.
pub(super) enum PreparedPlace {
    Direct(Expr),
    Map(maps::MapPlace),
    Slot { local: Expr, map: Expr, key: Expr },
}

impl PreparedPlace {
    pub(super) fn read(&self) -> Expr {
        match self {
            Self::Direct(place) => place.clone(),
            Self::Map(place) => place.read(),
            Self::Slot { local, .. } => local.clone(),
        }
    }

    pub(super) fn write(self, value: Expr, span: js::Span, out: &mut Vec<Stmt>) {
        match self {
            Self::Direct(place) => out.push(StmtKind::Assign(place, value).at(span)),
            Self::Map(place) => place.write(value, span, out),
            Self::Slot { local, map, key } => {
                out.push(StmtKind::Assign(local.clone(), value).at(span));
                out.push(StmtKind::Expr(Expr::call(Expr::member(map, "set"), vec![key, local])).at(span));
            }
        }
    }
}

/// Does running this only read, so what a value reads is the same before
/// and after it? Variables, their fields and items do, and a bounds check
/// of them, `$at(v, i)`, which can only panic; a call may write anything.
pub(super) fn only_reads(e: &Expr) -> bool {
    match &e.kind {
        js::ExprKind::Member(object, _) => only_reads(object),
        js::ExprKind::Index(object, index) => only_reads(object) && only_reads(index),
        js::ExprKind::Call(f, args) if matches!(&f.kind, js::ExprKind::Var(v) if v == "$at" || v == "$index") => {
            args.iter().all(only_reads)
        }
        _ => !e.has_effects(),
    }
}
impl<'a, 'tcx> FnCx<'a, 'tcx> {
    pub(super) fn assign(&mut self, lhs: ExprId, rhs: ExprId, span: Span, out: &mut Vec<Stmt>) -> R<()> {
        // A plain variable can receive control flow directly, without a temporary.
        if self.slot_place(lhs).is_none()
            && self.map_slot(lhs).is_none()
            && let Some((target, _)) = self.place(lhs)
            && let js::ExprKind::Var(name) = &target.kind
            && !self.is_simple(rhs)
        {
            self.assignee(lhs)?;
            return self.stmt(rhs, &Dest::Assign(name.clone()), out);
        }
        let value = self.assignment_rhs(lhs, rhs, out)?;
        let (place, value) = self.prepare_assignment_target(lhs, false, value, span, out)?;
        place.write(value, self.js_span(span), out);
        Ok(())
    }

    pub(super) fn assign_op(
        &mut self,
        op: rustc_middle::mir::AssignOp,
        lhs: ExprId,
        rhs: ExprId,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let value = self.assignment_rhs(lhs, rhs, out)?;
        let value = self.shift_amount(assign_op(op), value, lhs, rhs);
        let (place, value) = self.prepare_assignment_target(lhs, true, value, span, out)?;
        let current = place.read().or_at(self.js_span(self.thir[lhs].span));
        let value = self.binary(
            assign_op(op),
            current,
            value,
            self.known_int(rhs),
            self.thir[lhs].ty,
            span,
        )?;
        place.write(value.or_at(self.js_span(span)), self.js_span(span), out);
        Ok(())
    }

    pub(super) fn slot_place(&self, lhs: ExprId) -> Option<PreparedPlace> {
        let ExprKind::Deref { arg } = self.thir[self.strip(lhs)].kind else {
            return None;
        };
        let ExprKind::VarRef { id } = self.thir[self.strip(arg)].kind else {
            return None;
        };
        let (map, key) = self.locals.slots.get(&id)?.clone();
        Some(PreparedPlace::Slot {
            local: self.locals.vars[&id].place.clone(),
            map,
            key,
        })
    }

    pub(super) fn assignment_rhs(&mut self, lhs: ExprId, rhs: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        if self.map_slot(lhs).is_some() {
            self.assignment_value(rhs, out)
        } else {
            self.expr(rhs, out)
        }
    }

    /// Prepare a target once, after its value. Direct JS targets keep compact
    /// output when safe; map checks and reference-slot write-back use the same
    /// read/write contract. Primitive compound assignment reads after the RHS.
    pub(super) fn prepare_assignment_target(
        &mut self,
        lhs: ExprId,
        read: bool,
        value: Expr,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<(PreparedPlace, Expr)> {
        if let Some(place) = self.slot_place(lhs) {
            return Ok((place, value));
        }
        if let Some(slot) = self.map_slot(lhs) {
            return Ok((
                PreparedPlace::Map(self.prepare_map_place(slot, read, span, out)?),
                value,
            ));
        }
        let mut before = Vec::new();
        let target = match self.place(lhs) {
            Some(_) => self.assignee(lhs)?,
            None => self.element_target(lhs, &mut before)?,
        };
        let target = if read {
            self.read_twice(target, &mut before)
        } else {
            target
        };
        let value = self.value_first(value, &before, &target, out);
        let value = if read && value.has_effects() && self.may_change(lhs) {
            self.spill("value", value, out)
        } else {
            value
        };
        out.extend(before);
        Ok((PreparedPlace::Direct(target), value))
    }

    /// Assignments evaluate the RHS before the target, including its checks.
    /// A nonconstant value is captured before target preparation emits code.
    pub(super) fn assignment_value(&mut self, rhs: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let value = self.expr(rhs, out)?;
        Ok(if value.is_constant() {
            value
        } else {
            self.spill("value", value, out)
        })
    }

    /// `e` as a place, a variable and some of its fields, without reading it.
    /// Also says whether that variable is mutable.
    /// Is `e`'s JS value a cell, a box or a handle (ADR 0099)? A `&mut` to a
    /// value JS can't change in place, as rust-js makes one: a variable
    /// holding one, a field or an item keeping one, what a reference to one
    /// points at, or what the crate's own function returns. Not a std call's,
    /// as `v[i]`'s `index_mut` is: that's the item itself.
    pub(super) fn is_cell_value(&self, e: ExprId) -> bool {
        if !self.is_cell(self.thir[e].ty) {
            return false;
        }
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => self.locals.boxes.contains(&id),
            ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::Deref { .. } => true,
            // `if c { a } else { b }` of cells.
            ExprKind::If {
                then,
                else_opt: Some(otherwise),
                ..
            } => self.is_cell_value(then) && self.is_cell_value(otherwise),
            ExprKind::Block { block } => self.thir[block].expr.is_some_and(|value| self.is_cell_value(value)),
            // `&mut *a` of a cell: the cell.
            ExprKind::Borrow { arg, .. } => {
                matches!(self.thir[self.strip(arg)].kind, ExprKind::Deref { arg: inner } if self.is_cell_value(inner))
            }
            // The crate's own function, or the impl's method a trait's resolves to.
            ExprKind::Call { fun, .. } => match *self.thir[self.strip(fun)].ty.kind() {
                ty::FnDef(def_id, _) if self.tcx.trait_of_assoc(def_id).is_none() => self.is_rust_fn(def_id),
                ty::FnDef(def_id, args) => self.impl_method(def_id, args).ok().flatten().is_some(),
                _ => false,
            },
            _ => false,
        }
    }

    pub(super) fn place(&self, e: ExprId) -> Option<(Expr, bool)> {
        // Inside a closure, a place it captured by value is its snapshot.
        if !self.captures.is_empty()
            && let Some(var) = self
                .body_query()
                .place_path(e)
                .and_then(|path| self.captures.get(&path))
        {
            return Some((var.place.clone(), var.mutable));
        }
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => {
                let var = &self.locals.vars[&id];
                Some((var.place.clone(), var.mutable))
            }
            // A union's field is no place: it has no representation yet.
            ExprKind::Field { lhs, name, .. } if !is_union(self.thir[lhs].ty) => {
                let (base, mutable) = self.place(lhs)?;
                Some((self.project(base, self.thir[lhs].ty, name.as_usize()), mutable))
            }
            // `*out` of a box (ADR 0072), or of a handle (ADR 0099): what's in it.
            ExprKind::Deref { arg }
                if let ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } =
                    self.thir[self.strip(arg)].kind
                    && self.locals.boxes.contains(&id) =>
            {
                let (cell, _) = self.place(arg)?;
                Some((Expr::member(cell, "value"), true))
            }
            // `*self.0` of a cell kept in a field, an item or behind a reference
            // (ADR 0099): what's in it. A variable of one that isn't a box
            // names its place, as `let r = &mut x;` does.
            ExprKind::Deref { arg }
                if self.is_cell_value(arg)
                    && !matches!(
                        self.thir[self.strip(arg)].kind,
                        ExprKind::VarRef { .. } | ExprKind::UpvarRef { .. } | ExprKind::Call { .. }
                    ) =>
            {
                let (cell, _) = self.place(arg)?;
                Some((Expr::member(cell, "value"), true))
            }
            // A reference is the value it points to, so `*r` is where `r` is.
            // (A static is reached through a pointer to it.)
            ExprKind::Deref { arg }
                if matches!(self.thir[arg].ty.kind(), ty::Ref(..) | ty::RawPtr(..)) || self.thir[arg].ty.is_box() =>
            {
                self.place(arg).or_else(|| self.ref_place(arg))
            }
            // A JS global (ADR 0021).
            ExprKind::StaticRef { def_id, .. } if self.tcx.is_foreign_item(def_id) => {
                Some((self.js_ref(&js_name(self.tcx, def_id)), false))
            }
            // A static of the crate's (ADR 0096): its module's `const`, or
            // the `{ value }` of a `static mut`.
            ExprKind::StaticRef { def_id, .. } if self.krate.fns.contains_key(&def_id) => {
                let item = self.fn_ref(def_id);
                Some(match self.tcx.is_mutable_static(def_id) {
                    true => (Expr::member(item, "value"), true),
                    false => (item, false),
                })
            }
            _ => None,
        }
    }

    /// `e` as a place whose value can't change while it's still in scope, so
    /// a pattern's variables can just name parts of it.
    ///
    /// Its variable must be immutable. That's not enough on its own: `let mut
    /// s = r;` moves `r`, and then `s.origin.x = 0` changes the object `r`
    /// still names. So the variable must also be `Copy` (read, never moved:
    /// the read copies it if needed) or hold nothing changed in place.
    pub(super) fn stable_place(&self, e: ExprId) -> Option<Expr> {
        let (place, mutable) = self.place(e)?;
        let mut root = self.strip(e);
        while let ExprKind::Field { lhs, .. } | ExprKind::Deref { arg: lhs } = self.thir[root].kind {
            root = self.strip(lhs);
        }
        let ty = self.thir[root].ty;
        let unchanging = self.is_copy(ty) || !self.contains_mutated(ty);
        (!mutable && unchanging).then_some(place)
    }

    /// Where a reference made by a call points: `c.borrow_mut()` points at
    /// the cell's `value`, and so does the guard's `deref_mut()` (ADR 0025).
    pub(super) fn ref_place(&self, e: ExprId) -> Option<(Expr, bool)> {
        match self.thir[self.strip(e)].kind {
            ExprKind::Borrow { arg, .. } => self.place(arg).or_else(|| self.ref_place(arg)),
            ExprKind::Call { fun, ref args, .. } => match self.std_fn(fun)? {
                Std::Same => self.ref_place(args[0]),
                Std::Borrow => {
                    let (cell, _) = self.ref_place(args[0])?;
                    Some((Expr::member(cell, "value"), true))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// What a reference made with `&` or `&mut` refers to, which is the JS
    /// value itself: `&v[i]` is the element, not a copy of it.
    pub(super) fn referent(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        match self.thir[self.strip(e)].kind {
            // `&*f()`: the reference `f` returned.
            ExprKind::Deref { arg } => self.expr(arg, out),
            ExprKind::Index { lhs, index } => {
                let values = self.indexed(lhs, index, out)?;
                Ok(self.checked_index(lhs, values))
            }
            _ => self.expr(e, out),
        }
    }

    /// Is `place` a variable declared with `let`, which can be assigned?
    pub(super) fn is_let(&self, place: &Expr) -> bool {
        let js::ExprKind::Var(name) = &place.kind else {
            return false;
        };
        self.locals
            .vars
            .values()
            .chain(self.captures.values())
            .any(|v| v.mutable && matches!(&v.place.kind, js::ExprKind::Var(n) if n == name))
    }

    /// What a `&mut` in a variable names (ADR 0099), fixed where it's
    /// borrowed, since the variables it's reached through may change
    /// meanwhile: an index is evaluated once, `let r = &mut v[i]` keeping
    /// the index of that moment, and so is an object reached through a
    /// reference in a variable that's assigned again, `&mut cur.count`.
    pub(super) fn fixed_place(&mut self, borrowed: ExprId, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
        let place = if self.in_element(borrowed) {
            self.element_target(borrowed, out)?
        } else {
            match self.place(borrowed) {
                Some((place, _)) => place,
                None => return Err(self.unsupported(span, "a `&mut` of this in a variable")),
            }
        };
        let rebound = self.through_rebound(borrowed, false);
        Ok(self.fixed(place, rebound, out))
    }

    /// Is `e` reached through a reference that can be assigned again while
    /// `e` is borrowed, as `cur[0]` is of a `let mut cur = &mut a`, and
    /// `h.list[0]` of a field `list: &mut Vec<i32>`, `refs[0][0]` of an
    /// element and `outer[0]` of a `&mut &mut Vec<i32>`? Then what's meant
    /// is the object it holds now, not the place holding it. With
    /// `followed`, `e` is itself such a reference, followed.
    pub(super) fn through_rebound(&self, e: ExprId, followed: bool) -> bool {
        // Rust freezes the rest of the path while it's borrowed, but not
        // where a reference is: only what it points to. Only an immutable
        // variable's, or a `&mut` in a variable's, which names its place,
        // stays put; keeping any other's object, a temporary's say, is
        // the same object anyway.
        let reassignable = |r: ExprId| {
            matches!(self.thir[r].ty.kind(), ty::Ref(..))
                && match self.thir[self.strip(r)].kind {
                    ExprKind::VarRef { id } => {
                        self.locals.vars.get(&id).is_some_and(|v| v.mutable) && !self.locals.aliases.contains(&id)
                    }
                    _ => true,
                }
        };
        if followed && reassignable(e) {
            return true;
        }
        let mut root = self.strip(e);
        loop {
            match self.thir[root].kind {
                ExprKind::Field { lhs, .. } | ExprKind::Index { lhs, .. } | ExprKind::Borrow { arg: lhs, .. } => {
                    root = self.strip(lhs)
                }
                // `cur[0]` of a `Vec` is `*index_mut(&mut *cur, 0)`.
                ExprKind::Deref { arg } => match self.element(root) {
                    Some((items, _)) => root = self.strip(items),
                    None if reassignable(arg) => return true,
                    None => root = self.strip(arg),
                },
                _ => return false,
            }
        }
    }

    /// A place whose parts are evaluated now: each index that isn't a
    /// constant, and what it's in, if that isn't a variable's path. If
    /// it's `rebound`, what it's in is kept, `const o = h.list;`: the
    /// object then, which is the one the place is in.
    pub(super) fn fixed(&mut self, place: Expr, rebound: bool, out: &mut Vec<Stmt>) -> Expr {
        let span = place.span;
        match place.kind {
            js::ExprKind::Var(_) => place,
            js::ExprKind::Member(object, key) => {
                let object = self.fixed_object(*object, rebound, out);
                Expr::member(object, &key).or_at(span)
            }
            js::ExprKind::Index(items, index) => {
                let items = self.fixed_object(*items, rebound, out);
                let index = if index.is_constant() {
                    *index
                } else {
                    self.spill("at", *index, out)
                };
                Expr::index(items, index).or_at(span)
            }
            _ => self.spill("item", place, out),
        }
    }

    pub(super) fn fixed_object(&mut self, object: Expr, rebound: bool, out: &mut Vec<Stmt>) -> Expr {
        match object.kind {
            // A `const` is the object already.
            js::ExprKind::Var(_) if rebound && self.is_let(&object) => self.spill("o", object, out),
            _ if rebound && !matches!(object.kind, js::ExprKind::Var(_)) => self.spill("o", object, out),
            js::ExprKind::Var(_) => object,
            _ => self.fixed(object, false, out),
        }
    }

    /// An array or slice and an index into it: the array itself, not a
    /// copy, since only the element is read or written.
    pub(super) fn indexed(&mut self, items: ExprId, index: ExprId, out: &mut Vec<Stmt>) -> R<Vec<Expr>> {
        match self.place(items) {
            Some((place, _)) => Ok(vec![place, self.expr(index, out)?]),
            // An element that's indexed in turn, `grid[i][j]`: the row itself,
            // not the copy reading it as a value makes, which `grid[i][j] = x`
            // would change instead.
            None if self.element(items).is_some() => {
                let mut row = self.referent(items, out)?;
                let index = self.evaluated(index)?;
                if !index.statements.is_empty() && !row.is_constant() {
                    row = self.spill("row", row, out);
                }
                out.extend(index.statements);
                Ok(vec![row, index.value])
            }
            None => self.operands(&[items, index], out),
        }
    }

    /// An element of an array, a slice or a `Vec`, as its collection and
    /// its index: `a[i]`, or `*IndexMut::index_mut(&mut v, i)`.
    pub(super) fn element(&self, e: ExprId) -> Option<(ExprId, ExprId)> {
        match self.thir[self.strip(e)].kind {
            ExprKind::Index { lhs, index } => Some((lhs, index)),
            ExprKind::Deref { arg } => match self.thir[self.strip(arg)].kind {
                ExprKind::Call { fun, ref args, .. } if self.std_fn(fun) == Some(Std::Index) => {
                    Some((args[0], args[1]))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// An element, or a field of one: what `element_target` writes. Or a
    /// field of what a call's reference points to, as `w.x = 1` through a
    /// `DerefMut` is `deref_mut(&mut w).x = 1`.
    pub(super) fn in_element(&self, e: ExprId) -> bool {
        self.element(e).is_some()
            || matches!(self.thir[self.strip(e)].kind, ExprKind::Field { lhs, .. } if self.in_element(lhs) || self.returned(lhs))
    }

    /// `*f(..)`: what a call's reference points to.
    pub(super) fn returned(&self, e: ExprId) -> bool {
        matches!(self.thir[self.strip(e)].kind, ExprKind::Deref { arg } if matches!(self.thir[self.strip(arg)].kind, ExprKind::Call { .. }))
    }

    /// `v[i]`, checked: `$index(v, i)`, or just `v[i]` for an array whose
    /// length is its type's and a constant index below it, which rustc checked.
    pub(super) fn checked_index(&mut self, lhs: ExprId, values: Vec<Expr>) -> Expr {
        if self.in_bounds(lhs, &values[1]) {
            let [items, index]: [Expr; 2] = values.try_into().ok().expect("the items and an index");
            return Expr::index(items, index);
        }
        self.runtime.insert(Helper::Index);
        Expr::call(Expr::var("$index"), values)
    }

    /// Is `index` a constant below the length of the array `lhs`'s type?
    pub(super) fn in_bounds(&self, lhs: ExprId, index: &Expr) -> bool {
        let ty::Array(_, len) = self.thir[lhs].ty.peel_refs().kind() else {
            return false;
        };
        let (Some(len), Some(i)) = (len.try_to_target_usize(self.tcx), index.as_int()) else {
            return false;
        };
        (0..i128::from(len)).contains(&i)
    }

    /// An element as the target of an assignment, `v[$at(v, i)]`, checked
    /// first since JS would make the array longer. A field of one is
    /// `$index(v, i).x`.
    pub(super) fn element_target(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        if let Some((items, index)) = self.element(e) {
            let array = items;
            let [items, index]: [Expr; 2] = self.indexed(items, index, out)?.try_into().ok().unwrap();
            if self.in_bounds(array, &index) {
                return Ok(Expr::index(items, index));
            }
            // `items` is read twice.
            let items = if items.reads_same() {
                items
            } else {
                self.spill("items", items, out)
            };
            self.runtime.insert(Helper::At);
            return Ok(Expr::index(
                items.clone(),
                Expr::call(Expr::var("$at"), vec![items, index]),
            ));
        }
        match self.thir[self.strip(e)].kind {
            ExprKind::Field { lhs, name, .. } => {
                let base = match (self.place(lhs), self.element(lhs)) {
                    (Some((place, _)), _) => place,
                    (None, Some(_)) => self.referent(lhs, out)?,
                    (None, None) if self.returned(lhs) => self.referent(lhs, out)?,
                    (None, None) => self.element_target(lhs, out)?,
                };
                Ok(self.project(base, self.thir[lhs].ty, name.as_usize()))
            }
            // `*pick(a, b) = v` of a cell a call returns: its `value` (ADR 0099).
            ExprKind::Deref { arg } if self.is_cell_value(arg) && self.place(e).is_none() => {
                let cell = self.expr(arg, out)?;
                let cell = if cell.reads_same() {
                    cell
                } else {
                    self.spill("cell", cell, out)
                };
                Ok(Expr::member(cell, "value"))
            }
            _ => self.assignee(e),
        }
    }

    /// An assignment's value, taken before its place runs as Rust takes it
    /// (ADR 0056), where JS would run the place first: before a place that
    /// could change what it reads, `v[{ x = 2; 0 }] = x`, and, if it has
    /// effects, before one that could panic first, `v[$at(v, i)] = f()`.
    pub(super) fn value_first(&mut self, value: Expr, place: &[Stmt], target: &Expr, out: &mut Vec<Stmt>) -> Expr {
        let writes = !only_reads(target)
            || place
                .iter()
                .any(|s| !matches!(&s.kind, StmtKind::Const(_, e) if only_reads(e)));
        let panics = target.has_effects() || !place.is_empty();
        if (writes && !value.is_constant()) || (panics && value.has_effects()) {
            self.spill("value", value, out)
        } else {
            value
        }
    }

    /// Could running code change the place `lhs`? Only what holds a
    /// `&mut` of its variable could, a call it's passed to or a closure
    /// that captured it, and anything could change a place of no variable.
    pub(super) fn may_change(&self, lhs: ExprId) -> bool {
        let Some(id) = self.body_query().root_var(lhs) else {
            return true;
        };
        self.body_facts.mutably_borrowed.contains(&id)
    }

    /// A target that `x += 1` reads and then writes: each part of it that
    /// wouldn't read the same twice, `v[f()]` or `$index(v, f()).x`, taken
    /// once. A bounds check of what reads the same, `$at(v, i)`, can be.
    pub(super) fn read_twice(&mut self, target: Expr, out: &mut Vec<Stmt>) -> Expr {
        let mut once = |this: &mut Self, e: Expr, name: &str| {
            let repeatable = e.reads_same()
                || matches!(&e.kind, js::ExprKind::Call(f, args)
                    if matches!(&f.kind, js::ExprKind::Var(v) if v == "$at" || v == "$index")
                        && args.iter().all(Expr::reads_same));
            if repeatable { e } else { this.spill(name, e, out) }
        };
        match target.kind {
            js::ExprKind::Index(items, index) => {
                let items = once(self, *items, "items");
                let index = once(self, *index, "index");
                Expr::index(items, index)
            }
            js::ExprKind::Member(object, name) => {
                let object = once(self, *object, "item");
                Expr::member(object, &name)
            }
            kind => Expr { kind, ..target },
        }
    }

    /// A local variable of this function, or a field of one: not reached
    /// through a reference, nor captured by a closure.
    pub(super) fn is_local_place(&self, e: ExprId) -> bool {
        match self.thir[self.strip(e)].kind {
            ExprKind::VarRef { .. } => true,
            ExprKind::Field { lhs, .. } => self.is_local_place(lhs),
            _ => false,
        }
    }

    /// The place an assignment writes to.
    pub(super) fn assignee(&self, e: ExprId) -> R<Expr> {
        // `*r = v` with a `&mut` variable `r` would only rebind the JS variable.
        // One that names a place, as a `ref mut` binding does, writes it.
        let names_place = |arg: ExprId| match self.thir[self.strip(arg)].kind {
            ExprKind::VarRef { id } => {
                self.locals.boxes.contains(&id)
                    || self.locals.aliases.contains(&id)
                    || self
                        .locals
                        .vars
                        .get(&id)
                        .is_some_and(|v| matches!(v.place.kind, js::ExprKind::Member(..) | js::ExprKind::Index(..)))
            }
            // A cell kept in a field, `*self.0 = v`: its `value` (ADR 0099).
            ExprKind::Field { .. } => self.is_cell_value(arg),
            _ => false,
        };
        if let ExprKind::Deref { arg } = self.thir[self.strip(e)].kind
            && matches!(self.thir[arg].ty.kind(), ty::Ref(..))
            && matches!(
                self.thir[self.strip(arg)].kind,
                ExprKind::VarRef { .. } | ExprKind::Field { .. }
            )
            && !names_place(arg)
        {
            return Err(self.unsupported(self.thir[e].span, "assigning a whole value through a `&mut`"));
        }
        self.place(e)
            .map(|(place, _)| place)
            .ok_or_else(|| self.unsupported(self.thir[e].span, "assigning to this place"))
    }

    /// Read a variable or field's value.
    pub(super) fn read(&mut self, e: ExprId, out: &mut Vec<Stmt>) -> R<Expr> {
        let ty = self.thir[e].ty;
        // A `&mut` in a variable is its place (ADR 0099). Read as a value,
        // `generic(y)`, it would be the place's value, not a `&mut`.
        if let ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } = self.thir[self.strip(e)].kind
            && self.locals.aliases.contains(&id)
            && matches!(ty.kind(), ty::Ref(_, _, Mutability::Mut))
        {
            // A `&mut` to a number kept, `S { r: y }`: a handle on its place.
            if self.is_cell(ty) {
                return Ok(Expr::handle(self.locals.vars[&id].place.clone()));
            }
            return Err(self.unsupported(self.thir[e].span, "a `&mut` in a variable used as a value"));
        }
        self.moved(e, out)?;
        if let Some((place, _)) = self.place(e) {
            return Ok(self.copy_if_needed(place, ty));
        }
        match self.thir[self.strip(e)].kind {
            // A union's field: a constant of one is rejected, and reading it
            // is too, rather than taken as a struct's.
            ExprKind::Field { lhs, .. } if is_union(self.thir[lhs].ty) => {
                Err(self.unsupported(self.thir[e].span, "unions"))
            }
            // A field of a temporary, like `f().x`: nothing else can see the rest.
            ExprKind::Field { lhs, name, .. } => {
                // A field of an element, `v[i].x`, or of what a reference points
                // at, `f().unwrap().x`: that itself, and a copy of just the
                // field if it needs one.
                if self.element(lhs).is_some() || matches!(self.thir[self.strip(lhs)].kind, ExprKind::Deref { .. }) {
                    let base = self.referent(lhs, out)?;
                    let field = self.project(base, self.thir[lhs].ty, name.as_usize());
                    return Ok(self.copy_if_needed(field, ty));
                }
                let base = self.expr(lhs, out)?;
                Ok(self.project(base, self.thir[lhs].ty, name.as_usize()))
            }
            // `*f()`, including `Deref::deref` on a `String` or `Rc`: a
            // reference is its value. Reading a `Copy` one copies it, as
            // reading a place does: `*v.first().unwrap()` isn't `v[0]` itself.
            ExprKind::Deref { arg } => {
                let value = self.expr(arg, out)?;
                // `*pick(a, b)` of a cell it returned: what's in it (ADR 0099).
                let value = if self.is_cell_value(arg) {
                    Expr::member(value, "value")
                } else {
                    value
                };
                Ok(self.copy_if_needed(value, ty))
            }
            _ => Err(self.unsupported(self.thir[e].span, "reading this")),
        }
    }
}
