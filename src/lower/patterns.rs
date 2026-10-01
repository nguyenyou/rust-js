//! Bindings, destructuring and match/let-chain evaluation regions.

use super::{
    Binding, Dest, Evaluation, FnCx, Num, R, Shape, Var, bindings, camel_case, const_js, drops, fresh_in,
    ordering_value, std_impls, variant_field, without_refs,
};
use crate::js::{self, Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_ast::{LitKind, Mutability};
use rustc_hir::{BindingMode, ByRef, LangItem, RangeEnd};
use rustc_middle::mir::BorrowKind;
use rustc_middle::thir::{self, ArmId, ExprId, ExprKind, LogicalOp, Pat, PatKind, PatRangeBoundary};
use rustc_middle::ty;
use rustc_span::{DesugaringKind, Span};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// A tuple or struct pattern of plain variables and `_`s, as JS
    /// destructuring: `[count, setCount]`, `{ initial, label }`. Binds the
    /// variables, and says whether one is `mut`. `None`, binding nothing, if a
    /// part is anything else, or needs a copy of its own (ADR 0020).
    pub(super) fn js_pattern(&mut self, pat: &Pat<'tcx>) -> Option<(js::Pattern, bool)> {
        let PatKind::Leaf { subpatterns } = &pat.kind else {
            return None;
        };
        // `(i, &x)`: a reference is the value (ADR 0023), so that part is `x`.
        let parts: Vec<_> = subpatterns
            .iter()
            .map(|field| match without_refs(&field.pattern).kind {
                // A cell, a `&mut` to a number, is taken apart as a cell (ADR 0099).
                _ if self.is_cell(field.pattern.ty) => None,
                PatKind::Wild => Some((field.field.as_usize(), None)),
                PatKind::Binding {
                    name,
                    var,
                    mode: BindingMode(ByRef::No, mutability),
                    subpattern: None,
                    ty,
                    ..
                } if self.unsupported_part(ty).is_none() && !(self.contains_mutated(ty) && self.is_copy(ty)) => {
                    Some((field.field.as_usize(), Some((name, var, mutability == Mutability::Mut))))
                }
                _ => None,
            })
            .collect::<Option<_>>()?;
        let mutable = parts.iter().any(|(_, part)| part.is_some_and(|(_, _, m)| m));
        let pattern = match self.shape(pat.ty) {
            Shape::Array(tys) => {
                let mut items = vec![None; tys.len()];
                for (i, part) in parts {
                    items[i] = part.map(|(name, var, m)| self.bind(var, name.as_str(), m));
                }
                // `[a, b, , ]` is `[a, b]`.
                while items.last().is_some_and(Option::is_none) {
                    items.pop();
                }
                js::Pattern::Array(items)
            }
            Shape::Object(fields) => js::Pattern::Object(
                parts
                    .into_iter()
                    .filter_map(|(i, part)| {
                        part.map(|(name, var, m)| (fields[i].0.clone(), self.bind(var, name.as_str(), m)))
                    })
                    .collect(),
            ),
            Shape::Other => return None,
        };
        Some((pattern, mutable))
    }

    pub(super) fn lower_let(
        &mut self,
        pat: &Pat<'tcx>,
        init: Option<ExprId>,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        // `let mut it = v.iter();` that `it.next()` steps through: `$iter(v)`,
        // which knows where it is (ADR 0071).
        if let PatKind::Binding {
            name,
            var,
            mode: BindingMode(ByRef::No, mutability),
            subpattern: None,
            ty,
            ..
        } = pat.kind
            && let Some(init) = init
            && self.stepped.contains(&var)
            && self.is_array_iter(ty)
            && !self.is_peekable(ty)
            && !self.has_drops(ty)
        {
            let items = self.iter_value(init, out)?;
            let items = self.iter_source(items, ty, span)?;
            self.runtime.insert(Helper::Iter);
            let value = Expr::call(Expr::var("$iter"), vec![items]);
            let name = self.bind(var, name.as_str(), mutability == Mutability::Mut);
            self.locals.iterators.insert(var);
            let kind = if mutability == Mutability::Mut {
                StmtKind::Let(name, Some(value))
            } else {
                StmtKind::Const(name, value)
            };
            out.push(kind.at(self.js_span(span)));
            return Ok(());
        }
        // `let f = { let c = ..; move |n| .. };`: the block's statements
        // first, then `const f = ..` of its value. Every local has a JS name
        // of its own, so none of them can clash where they now are.
        if let Some(init) = init
            && let ExprKind::Block { block } = self.thir[self.strip(init)].kind
            && let thir::Block {
                targeted_by_break: false,
                expr: Some(value),
                safety_mode: thir::BlockSafety::Safe,
                span: block_span,
                ..
            } = self.thir[block]
            && !block_span.from_expansion()
            && !self.block_owns(block).unwrap_or(true)
        {
            self.block_stmts(block, out)?;
            return self.lower_let(pat, Some(value), span, out);
        }
        // `async fn f(x)` moves `x` into its body with `let x = x;` (ADR 0029).
        // In JS the body is the function's, so they're one variable.
        // `format_args!` holds its values in `super let args = (&a, &b);`, then
        // `super let args = [Argument::new_display(args.0), ..];` (ADR 0034).
        // Both are only read from, so they name their parts where they are:
        // `format!("{} ms", t)` is `t + " ms"`, with no arrays in between.
        if self.in_format_args(span)
            && let PatKind::Binding {
                var,
                mode: BindingMode(ByRef::No, Mutability::Not),
                subpattern: None,
                ..
            } = pat.kind
            && let Some(init) = init
        {
            let parts = match self.thir[self.strip(init)].kind {
                ExprKind::Tuple { ref fields } => self.tuple_parts(fields, "arg", true, out)?,
                _ => self.expr(init, out)?,
            };
            self.locals.vars.insert(
                var,
                Var {
                    place: parts,
                    mutable: false,
                    depth: self.loops.len(),
                },
            );
            return Ok(());
        }
        // `let a = f()?;` on an option: the value is `a` itself, so it's kept
        // under that name: `const a = f(); if (a == null) { return undefined; }`.
        if let PatKind::Binding {
            name,
            var,
            mode: BindingMode(ByRef::No, Mutability::Not),
            subpattern: None,
            ..
        } = pat.kind
            && let Some(init) = init
            && let Some(tried) = self.body_query().as_question(init)
            && self.option_of(self.thir[tried].ty).is_some()
            && !self.has_drops(self.thir[init].ty)
        {
            let value = self.question(init, tried, Some(name.as_str()), out)?;
            self.locals.vars.insert(
                var,
                Var {
                    place: value,
                    mutable: false,
                    depth: self.loops.len(),
                },
            );
            return Ok(());
        }
        if span.is_desugaring(DesugaringKind::Async)
            && let PatKind::Binding {
                var,
                mode: BindingMode(ByRef::No, mutability),
                subpattern: None,
                ..
            } = pat.kind
            && let Some(init) = init
            && let ExprKind::UpvarRef { var_hir_id, .. } = self.thir[self.strip(init)].kind
            && let Some(outer) = self.locals.vars.get(&var_hir_id)
        {
            let alias = Var {
                place: outer.place.clone(),
                mutable: mutability == Mutability::Mut,
                depth: outer.depth,
            };
            self.locals.vars.insert(var, alias);
            return Ok(());
        }
        // `let y = &mut x;`: `y` names `x`, so `*y = 5` is `x = 5` (ADR 0099).
        // While `y` lives, Rust lets nothing else use `x`.
        if let PatKind::Binding {
            name,
            var,
            mode: BindingMode(ByRef::No, Mutability::Not),
            subpattern: None,
            ty,
            ..
        } = pat.kind
            && let ty::Ref(_, inner, Mutability::Mut) = *ty.kind()
            && !self.is_object(inner)
            && let Some(init) = init
            && let moved = match self.thir[self.strip(init)].kind {
                ExprKind::VarRef { id } => self.locals.aliases.contains(&id).then_some(id),
                _ => None,
            }
            && let Some(borrowed) = self.mut_borrowed(init).or(moved.map(|_| init))
        {
            let place = match moved {
                Some(id) => self.locals.vars[&id].place.clone(),
                // `let x = &mut 1;`: `let x = 1;`, which `x` names.
                None if self.is_temporary(borrowed) => {
                    let value = self.expr(borrowed, out)?;
                    let home = self.fresh(&camel_case(name.as_str()));
                    out.push(StmtKind::Let(home.clone(), Some(value)).at(self.js_span(span)));
                    Expr::var(&home)
                }
                None => self.fixed_place(borrowed, pat.span, out)?,
            };
            self.locals.aliases.insert(var);
            self.locals.vars.insert(
                var,
                Var {
                    place,
                    mutable: true,
                    depth: self.loops.len(),
                },
            );
            return Ok(());
        }
        let span = self.js_span(span);
        match &pat.kind {
            PatKind::Binding {
                name,
                var,
                mode,
                subpattern: None,
                ty,
                ..
            } => {
                self.check_by_value(*mode, *ty, pat.span)?;
                self.check_value_ty(*ty, pat.span)?;
                let mutable = mode.1 == Mutability::Mut;
                // `let ref r = f();` owns what `f` made, as `let r = f();` does;
                // `let ref r = x;` borrows `x` (ADR 0098).
                let owned = match mode.0 {
                    ByRef::No => Some(*ty),
                    ByRef::Yes(..) => init
                        .filter(|&i| !drops::is_place(&self.thir[self.strip(i)].kind))
                        .map(|i| self.thir[i].ty),
                }
                .filter(|&t| self.has_drops(t));
                let owns = owned.is_some();
                if owns && init.is_none() {
                    return Err(self.unsupported(pat.span, "a `let` of a value with a destructor, without its value"));
                }
                let name = match init {
                    // Only control flow needs `let x;` and then assignments in
                    // its branches. Anything else (a closure, say) computes its
                    // statements first and then has a value.
                    Some(init) if self.is_simple(init) || !self.is_control_flow(init) => {
                        let value = self.expr(init, out)?;
                        let name = self.bind(*var, name.as_str(), mutable);
                        let kind = if mutable {
                            StmtKind::Let(name.clone(), Some(value))
                        } else {
                            StmtKind::Const(name.clone(), value)
                        };
                        out.push(kind.at(span));
                        name
                    }
                    Some(init) => {
                        let name = self.bind(*var, name.as_str(), mutable);
                        out.push(StmtKind::Let(name.clone(), None).at(span));
                        self.stmt(init, &Dest::Assign(name.clone()), out)?;
                        name
                    }
                    None => {
                        let name = self.bind(*var, name.as_str(), mutable);
                        out.push(StmtKind::Let(name.clone(), None).at(span));
                        name
                    }
                };
                // `let mut r = &mut x;`: a cell, whose `value` is `x` (ADR 0099).
                if mode.0 == ByRef::No && self.is_cell(*ty) {
                    self.locals.boxes.insert(*var);
                }
                if let Some(owned) = owned {
                    self.own(*var, Expr::var(&name), owned, pat.span, out)?;
                }
                Ok(())
            }
            // `let _ = f();` drops what `f` made at once; `let _ = x;` doesn't move `x`.
            PatKind::Wild => match init {
                Some(init)
                    if self.has_drops(self.thir[init].ty)
                        && !matches!(
                            self.thir[self.strip(init)].kind,
                            ExprKind::VarRef { .. }
                                | ExprKind::Field { .. }
                                | ExprKind::Index { .. }
                                | ExprKind::Deref { .. }
                                | ExprKind::UpvarRef { .. }
                                | ExprKind::StaticRef { .. }
                        ) =>
                {
                    let ty = self.thir[init].ty;
                    let value = self.expr(init, out)?;
                    let value = self.droppable(value, ty, out);
                    self.drop_value(value, ty, pat.span, out)
                }
                Some(init) => self.stmt(init, &Dest::Discard, out),
                None => Ok(()),
            },
            // `let (q, r) = divmod(a, b);`
            _ => {
                let Some(init) = init else {
                    return Err(self.unsupported(pat.span, "this `let` pattern without a value"));
                };
                // `let (count, set_count) = use_state(0);` is
                // `const [count, setCount] = useState(0);`. A place is taken
                // apart where it is, below.
                if self.place(init).is_none() && !self.is_control_flow(init) {
                    let value = self.expr(init, out)?;
                    if let Some((pattern, mutable)) = self.js_pattern(pat) {
                        out.push(
                            StmtKind::Destructure {
                                pattern,
                                value,
                                mutable,
                            }
                            .at(span),
                        );
                        return Ok(());
                    }
                    let subject = self.spill("tmp", value, out);
                    return self.destructure(pat, subject, true, false, out);
                }
                let items = self.item_subject(init);
                let (subject, stable) = self.subject(init, "tmp", out)?;
                // What it binds by value is moved out of `init` (ADR 0098).
                self.clear_parts(init, pat, out);
                self.destructure(pat, subject, stable, items, out)
            }
        }
    }

    /// Bind the variables of an irrefutable pattern to the parts of `subject`.
    pub(super) fn destructure(
        &mut self,
        pat: &Pat<'tcx>,
        subject: Expr,
        stable: bool,
        items: bool,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let mut bindings = Vec::new();
        if self.pattern_test(pat, &subject, &mut bindings)?.is_some() {
            return Err(self.unsupported(pat.span, "this refutable pattern"));
        }
        self.bind_all(bindings, stable, items, self.js_span(pat.span), out)
    }

    /// Where a `match` or `let` finds the value it takes apart, and whether
    /// that stays unchanged while the pattern's variables live.
    ///
    /// A place is used where it is, and may be stable (see `stable_place`).
    /// A tuple of stable places (`match (a, b)`) is used without building
    /// the array. Anything else is computed once into a `const` named `base`,
    /// which is stable: no Rust variable can move or change it.
    pub(super) fn subject(&mut self, e: ExprId, base: &str, out: &mut Vec<Stmt>) -> R<(Expr, bool)> {
        // A `&mut` in a variable, which names its place (ADR 0099): a handle on
        // it, as every `&mut` to a value JS can't change in place is matched.
        if self.is_cell(self.thir[e].ty) && !self.is_cell_value(e) && self.place(e).is_some() {
            return Ok((self.read(e, out)?, false));
        }
        if let Some(place) = self.stable_place(e) {
            return Ok((place, true));
        }
        // `&mut x`: the place, which its `ref mut` bindings name, as a `&x`
        // one is; of a temporary, `&mut Some(3)`, a `let` of it (ADR 0099).
        // Of a value JS can't change in place, the `&mut` is a handle on the
        // place, which a `&mut` pattern takes apart as the place itself and a
        // binding of the `&mut` binds.
        if let ExprKind::Borrow {
            borrow_kind: BorrowKind::Mut { .. },
            arg,
        } = self.thir[self.strip(e)].kind
        {
            let cell = |place: Expr, this: &Self| {
                if this.makes_cell(this.thir[arg].ty) {
                    Expr::handle(place)
                } else {
                    place
                }
            };
            if let Some((place, _)) = self.place(arg) {
                return Ok((cell(place, self), false));
            }
            if self.is_temporary(arg) {
                let value = self.expr(arg, out)?;
                let name = self.fresh(base);
                out.push(StmtKind::Let(name.clone(), Some(value)).at(self.js_span(self.thir[e].span)));
                self.locals.temporaries.insert(name.clone());
                return Ok((cell(Expr::var(&name), self), false));
            }
        }
        // `&x`: a reference is the value (ADR 0023), and a borrowed `x` stays put.
        if let ExprKind::Borrow {
            borrow_kind: BorrowKind::Shared,
            arg,
        } = self.thir[self.strip(e)].kind
            && let Some(place) = self.stable_place(arg)
        {
            return Ok((place, true));
        }
        if let Some((place, _)) = self.place(e) {
            return Ok((place, false));
        }
        // `match (a, b)` tests `a` and `b` where they are. A part that isn't a
        // place that stays put goes in a `const` of its own, in order.
        if let ExprKind::Tuple { ref fields } = self.thir[self.strip(e)].kind
            && !fields.is_empty()
        {
            return Ok((self.tuple_parts(fields, base, false, out)?, true));
        }
        let value = self.expr(e, out)?;
        let name = self.fresh(base);
        out.push(StmtKind::Const(name.clone(), value).at(self.js_span(self.thir[e].span)));
        Ok((Expr::var(&name), true))
    }

    /// `[a, b]` for a tuple `(a, b)` that's only taken apart: each part a
    /// place that stays put, a constant, or else a `const` of its own, in order.
    /// With `used_once`, a part without effects is written in place too.
    pub(super) fn tuple_parts(
        &mut self,
        fields: &[ExprId],
        base: &str,
        used_once: bool,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let mut parts = Vec::new();
        for &f in fields {
            // `format_args!`'s parts are references: `&a` is `a`. Not `&*o` of a
            // box, which is its `value` (ADR 0074).
            let referent = match self.thir[self.strip(f)].kind {
                ExprKind::Borrow { arg, .. } => arg,
                _ => f,
            };
            let part = match self.stable_place(referent) {
                Some(place) => place,
                None => {
                    let value = self.expr(f, out)?;
                    if value.is_constant() || (used_once && !value.has_effects()) {
                        value
                    } else {
                        self.spill(base, value, out)
                    }
                }
            };
            parts.push(part);
        }
        Ok(Expr::array(parts))
    }

    /// Is `span` rustc's lowering of a `format_args!` (so `format!`, `panic!`, ..)?
    pub(super) fn in_format_args(&self, span: Span) -> bool {
        matches!(span.desugaring_kind(), Some(DesugaringKind::FormatLiteral { .. }))
    }

    /// Give a pattern's variables their JS meaning. Immutable ones bound into
    /// a stable subject just name the place they matched, as ReScript does:
    /// `P { x, y } => x + y` becomes `p.x + p.y`. The rest get a variable
    /// holding their own value. Of `items`, a std call's (`item_subject`),
    /// a `&mut` is the item, not a cell (ADR 0099).
    pub(super) fn bind_all(
        &mut self,
        bindings: Vec<Binding<'tcx>>,
        stable: bool,
        items: bool,
        span: js::Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        // `x @ B { b, .. }`: `x`, or where it's moved, is the same JS object
        // as the value, and a change to it would change what `b` reads in
        // place, so each binding has its own. `n @ 1..=9` binds nothing else.
        let stable = stable && !(bindings.len() > 1 && bindings.iter().any(|b| b.whole));
        for b in bindings {
            // `Some(r)` of an `Option<&mut i32>`: `r` is a cell (ADR 0099).
            if !b.by_ref_mut && self.is_cell(b.ty) {
                if items {
                    self.locals.items.insert(b.var);
                } else {
                    self.locals.boxes.insert(b.var);
                }
            }
            // A place that's computed, like `$someValue(o)`, goes in a `const`.
            // A `ref mut` one always does: `*r = x` writes the place it names.
            // One that owns what it binds, by value, drops it as its scope
            // ends (ADR 0098): a `const` of its own, named as in Rust.
            let owns = self.is_owner(b.var)?;
            if (stable || b.by_ref_mut) && !b.mutable && !b.place.has_effects() && !owns {
                // `ref mut` of a `let` variable writes it, as a `&mut` in a
                // variable does (ADR 0099): `if let Some(n) = p { *n += 1 }`.
                // Not an object's: its variable may be a `&mut` itself.
                if b.by_ref_mut
                    && matches!(*b.ty.kind(), ty::Ref(_, inner, _) if !self.is_object(inner))
                    && self.is_let(&b.place)
                {
                    self.locals.aliases.insert(b.var);
                }
                self.locals.vars.insert(
                    b.var,
                    Var {
                        place: b.place,
                        mutable: false,
                        depth: self.loops.len(),
                    },
                );
                continue;
            }
            let value = self.copy_if_needed(b.place, b.ty).or_at(span);
            let name = self.bind(b.var, &b.name, b.mutable);
            let kind = if b.mutable {
                StmtKind::Let(name.clone(), Some(value))
            } else {
                StmtKind::Const(name.clone(), value)
            };
            out.push(kind.at(span));
            if owns {
                self.own_at(b.var, Expr::var(&name), b.ty, span, out)?;
            }
        }
        Ok(())
    }

    pub(super) fn lower_match(&mut self, scrutinee: ExprId, arms: &[ArmId], dest: &Dest, out: &mut Vec<Stmt>) -> R<()> {
        // A `fmt::Result` is nothing in JS (ADR 0054): there's no `Err` to match.
        if self.is_fmt_result(self.thir[scrutinee].ty) {
            return Err(self.unsupported(self.thir[scrutinee].span, "matching a `fmt::Result`"));
        }
        // Evaluate the scrutinee once, unless it's a place that can be
        // tested where it is.
        let items = self.item_subject(scrutinee);
        let (subject, stable) = self.subject(scrutinee, "match", out)?;

        // Each arm: its test, a guard's statements and test when it needs
        // statements of its own, and its body.
        type Arm = (Option<Expr>, Option<(Vec<Stmt>, Expr)>, Vec<Stmt>, js::Span);
        let mut chain: Vec<Arm> = Vec::new();
        for (i, &arm_id) in arms.iter().enumerate() {
            let arm = &self.thir[arm_id];
            let arm_span = self.js_span(arm.span);
            let pat_span = self.js_span(arm.pattern.span);
            let mut bindings = Vec::new();
            let mut test = self
                .pattern_test(&arm.pattern, &subject, &mut bindings)?
                .map(|t| t.or_at(pat_span));

            // A guard is tested before the arm's body, where a binding that
            // isn't the place it names gets its `const`. So the guard reads
            // each binding from its place, which nothing has changed yet: it
            // runs right after the pattern's test.
            let mut guarded = None;
            if let Some(guard) = arm.guard {
                for b in &bindings {
                    let place = Var {
                        place: b.place.clone(),
                        mutable: false,
                        depth: self.loops.len(),
                    };
                    self.locals.vars.insert(b.var, place);
                }
                let mut before = Vec::new();
                let guard = self.expr(guard, &mut before);
                for b in &bindings {
                    self.locals.vars.remove(&b.var);
                }
                let guard = guard?;
                if before.is_empty() {
                    test = Some(match test {
                        Some(t) => Expr::bin(Op::And, t, guard),
                        None => guard,
                    });
                } else {
                    guarded = Some((before, guard));
                }
            }
            // The arm owns what its pattern moves out of the scrutinee, and
            // drops it as it ends (ADR 0098).
            let mut body = Vec::new();
            let mark = self.owned_mark();
            self.clear_parts(scrutinee, &arm.pattern, &mut body);
            self.bind_all(bindings, stable, items, pat_span, &mut body)?;
            // Rust checked the match is exhaustive, so if we reach the last
            // unguarded arm, it matches. No need to test it.
            if i == arms.len() - 1 && arm.guard.is_none() {
                test = None;
            }
            let mut arm_body = Vec::new();
            self.stmt(arm.body, dest, &mut arm_body)?;
            self.close_scope(mark, arm_body, arm.span, &mut body)?;
            let done = test.is_none() && guarded.is_none();
            chain.push((test, guarded, body, arm_span));
            if done {
                break; // Later arms are unreachable.
            }
        }

        // Fold into `if (..) {..} else if (..) {..} else {..}`. A guard with
        // statements runs them after its arm's test, and one that fails goes
        // on to the later arms, so the chain is a labeled block that a
        // matched arm leaves.
        let mut label = None;
        let mut rest: Option<Vec<Stmt>> = None;
        for (test, guarded, mut body, span) in chain.into_iter().rev() {
            if let Some((mut before, guard)) = guarded {
                let leaves = matches!(
                    body.last().map(|s| &s.kind),
                    Some(StmtKind::Return(_) | StmtKind::Throw(_) | StmtKind::Break(_) | StmtKind::Continue(_))
                );
                if !leaves {
                    let label = label.get_or_insert_with(|| fresh_in(&mut self.labels, "arms")).clone();
                    body.push(StmtKind::Break(Some(label)).at(span));
                }
                before.push(StmtKind::If(guard, body, None).at(span));
                let mut arm = match test {
                    Some(t) => vec![StmtKind::If(t, before, None).at(span)],
                    None => before,
                };
                arm.extend(rest.unwrap_or_default());
                rest = Some(arm);
                continue;
            }
            rest = match test {
                // An arm that does nothing, `Dot => {}`, before others:
                // `if (s !== "Dot") { .. }`, not `if (s === "Dot") {} else ..`.
                Some(t) if body.is_empty() && rest.is_some() => Some(vec![
                    StmtKind::If(std_impls::negate(t), rest.unwrap_or_default(), None).at(span),
                ]),
                Some(t) => Some(vec![StmtKind::If(t, body, rest).at(span)]),
                // A last arm that does nothing, `None => {}`: no `else {}`.
                None if body.is_empty() => None,
                None => Some(body),
            };
        }
        match label {
            Some(label) => {
                let span = self.js_span(self.thir[scrutinee].span);
                out.push(StmtKind::Labeled(label, rest.unwrap_or_default()).at(span));
            }
            None => out.extend(rest.unwrap_or_default()),
        }
        Ok(())
    }

    /// An `if`'s condition as the parts joined by `&&`, when there are
    /// several and one is a `let`: a let chain (ADR 0048).
    pub(super) fn let_chain(&self, cond: ExprId) -> Option<Vec<ExprId>> {
        fn parts(cx: &FnCx<'_, '_>, e: ExprId, found: &mut Vec<ExprId>) {
            let e = cx.strip(e);
            match cx.thir[e].kind {
                ExprKind::LogicalOp {
                    op: LogicalOp::And,
                    lhs,
                    rhs,
                } => {
                    parts(cx, lhs, found);
                    parts(cx, rhs, found);
                }
                _ => found.push(e),
            }
        }
        let mut found = Vec::new();
        parts(self, cond, &mut found);
        let has_let = found.iter().any(|&p| matches!(self.thir[p].kind, ExprKind::Let { .. }));
        (found.len() > 1 && has_let).then_some(found)
    }

    /// `if let Some(h) = half(n) && h > 2 && let Some(q) = f(h) { .. } else { .. }`.
    /// Each part runs only if the ones before it held, and may read what an
    /// earlier `let` bound. Parts that need no statements of their own join
    /// one test: `const h = half(n); if (h != null && h > 2) { .. }`. One that
    /// does, like a `let` of a call, opens an `if` inside. With more than one,
    /// the `else` follows them all in a labeled block, which the `then` leaves.
    pub(super) fn lower_let_chain(
        &mut self,
        parts: Vec<ExprId>,
        then: ExprId,
        else_opt: Option<ExprId>,
        dest: &Dest,
        span: js::Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        // Each level: what runs before its test, its test, and what its body
        // starts with (a `let`'s bindings).
        let mut levels: Vec<(Vec<Stmt>, Vec<Expr>, Vec<Stmt>)> = vec![(Vec::new(), Vec::new(), Vec::new())];
        for part in parts {
            let (mut before, mut bindings) = (Vec::new(), Vec::new());
            let test = match self.thir[part].kind {
                ExprKind::Let { expr, ref pat } => self.if_let(expr, pat, &mut bindings, &mut before)?,
                _ => self.expr(part, &mut before)?,
            };
            let level = levels.last_mut().expect("a level");
            if level.1.is_empty() || (before.is_empty() && level.2.is_empty()) {
                level.0.extend(before);
                level.1.push(test);
                level.2.extend(bindings);
            } else {
                levels.push((before, vec![test], bindings));
            }
        }
        let mut then_out = Vec::new();
        self.stmt(then, dest, &mut then_out)?;
        let mut else_out = match else_opt {
            Some(els) => {
                let mut else_out = Vec::new();
                self.stmt(els, dest, &mut else_out)?;
                Some(else_out)
            }
            None => None,
        };
        let label = (levels.len() > 1 && else_out.is_some()).then(|| fresh_in(&mut self.labels, "chain"));
        let leaves = matches!(
            then_out.last().map(|s| &s.kind),
            Some(StmtKind::Return(_) | StmtKind::Throw(_) | StmtKind::Break(_) | StmtKind::Continue(_))
        );
        if let Some(label) = &label
            && !leaves
        {
            then_out.push(StmtKind::Break(Some(label.clone())).at(span));
        }
        // Built from the innermost level out; only a lone level has the `else`.
        let single = levels.len() == 1;
        let mut body = then_out;
        for (before, tests, bindings) in levels.into_iter().rev() {
            let test = tests
                .into_iter()
                .reduce(|a, b| Expr::bin(Op::And, a, b))
                .unwrap_or_else(|| Expr::bool(true));
            let mut inner = bindings;
            inner.extend(body);
            let els = if single { else_out.take() } else { None };
            body = before;
            body.push(StmtKind::If(test, inner, els).at(span));
        }
        match (label, else_out) {
            (Some(label), Some(else_out)) => {
                body.extend(else_out);
                out.push(StmtKind::Labeled(label, body).at(span));
            }
            _ => out.extend(body),
        }
        Ok(())
    }

    /// `if let pat = scrutinee`: the test, with the pattern's variables
    /// bound at the start of the `then` branch. `if let Some(el) = find()`
    /// keeps the value in a `const` named like the variable, which is then
    /// just that `const`: `const el = find(); if (el != null) { .. }`.
    pub(super) fn if_let(
        &mut self,
        scrutinee: ExprId,
        pat: &Pat<'tcx>,
        then_out: &mut Vec<Stmt>,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        if let Some(test) = self.slot_binding(scrutinee, pat, out)? {
            return Ok(test);
        }
        let base = match &pat.kind {
            PatKind::Variant { subpatterns, .. } if subpatterns.len() == 1 => match &subpatterns[0].pattern.kind {
                PatKind::Binding { name, .. } => name.to_string(),
                _ => "value".to_string(),
            },
            _ => "value".to_string(),
        };
        let items = self.item_subject(scrutinee);
        let (subject, stable) = self.subject(scrutinee, &base, out)?;
        let mut bindings = Vec::new();
        let test = self.pattern_test(pat, &subject, &mut bindings)?;
        self.bind_all(bindings, stable, items, self.js_span(pat.span), then_out)?;
        Ok(test.unwrap_or_else(|| Expr::bool(true)))
    }

    /// `if let Some(n) = m.get_mut(&k)` of a map whose values are primitives:
    /// `let n = m.get(k)`, which a write through `n` puts back (`PreparedPlace::Slot`).
    pub(super) fn slot_binding(&mut self, scrutinee: ExprId, pat: &Pat<'tcx>, out: &mut Vec<Stmt>) -> R<Option<Expr>> {
        let ExprKind::Call { fun, ref args, .. } = self.thir[self.strip(scrutinee)].kind else {
            return Ok(None);
        };
        let PatKind::Variant { subpatterns, .. } = &pat.kind else {
            return Ok(None);
        };
        let [field] = subpatterns.as_slice() else {
            return Ok(None);
        };
        let PatKind::Binding {
            name,
            var,
            mode: BindingMode(ByRef::No, Mutability::Not),
            subpattern: None,
            ty,
            ..
        } = &field.pattern.kind
        else {
            return Ok(None);
        };
        let &ty::FnDef(get, generic_args) = self.thir[self.strip(fun)].ty.kind() else {
            return Ok(None);
        };
        let slot = self.recognition().is_mutable_map_get(get, generic_args)
            && matches!(ty.kind(), ty::Ref(_, value, Mutability::Mut) if self.is_primitive_key(*value));
        if !slot {
            return Ok(None);
        }
        let args = args.clone();
        let [map, key]: [Expr; 2] = self.operands(&args, out)?.try_into().ok().expect("a map and a key");
        let map = if map.reads_same() {
            map
        } else {
            self.spill("map", map, out)
        };
        let key = if key.reads_same() {
            key
        } else {
            self.spill("key", key, out)
        };
        let name = self.bind(*var, name.as_str(), true);
        let there = Expr::call(Expr::member(map.clone(), "get"), vec![key.clone()]);
        out.push(StmtKind::Let(name.clone(), Some(there)).at(self.js_span(pat.span)));
        self.locals.slots.insert(*var, (map, key));
        Ok(Some(Expr::bin(Op::LooseNe, Expr::var(&name), Expr::null())))
    }

    /// The shape `as_matches` takes: `pat => true, _ => false`.
    pub(super) fn is_matches(&self, arms: &[ArmId]) -> bool {
        let is_bool = |arm: ArmId, want: bool| matches!(self.thir[self.strip(self.thir[arm].body)].kind, ExprKind::Literal { lit, .. } if lit.node == LitKind::Bool(want));
        matches!(arms, &[first, rest] if is_bool(first, true) && is_bool(rest, false)
            && matches!(self.thir[rest].pattern.kind, PatKind::Wild) && self.thir[rest].guard.is_none())
    }

    /// `matches!(x, pat)`, or `match x { pat if guard => true, _ => false }`:
    /// just the test, `x.TAG === "Circle"`, when the pattern binds nothing
    /// the guard can't read where it is.
    pub(super) fn as_matches(&mut self, scrutinee: ExprId, arms: &[ArmId], out: &mut Vec<Stmt>) -> R<Option<Expr>> {
        let is_bool = |arm: ArmId, want: bool| matches!(self.thir[self.strip(self.thir[arm].body)].kind, ExprKind::Literal { lit, .. } if lit.node == LitKind::Bool(want));
        let &[first, rest] = arms else { return Ok(None) };
        if !is_bool(first, true)
            || !is_bool(rest, false)
            || !matches!(self.thir[rest].pattern.kind, PatKind::Wild)
            || self.thir[rest].guard.is_some()
        {
            return Ok(None);
        }
        let items = self.item_subject(scrutinee);
        let (subject, stable) = self.subject(scrutinee, "match", out)?;
        let mut bindings = Vec::new();
        let test = self.pattern_test(&self.thir[first].pattern, &subject, &mut bindings)?;
        if !bindings.is_empty() && (!stable || bindings.iter().any(|b| b.mutable)) {
            return Err(self.unsupported(self.thir[first].pattern.span, "this binding in `matches!`"));
        }
        let span = self.js_span(self.thir[first].span);
        self.bind_all(bindings, stable, items, span, out)?;
        let guard = match self.thir[first].guard {
            Some(guard) if self.is_simple(guard) => Some(self.evaluated(guard)?),
            Some(guard) => return Err(self.unsupported(self.thir[guard].span, "this guard")),
            None => None,
        };
        let test = match (test, guard) {
            (Some(t), Some(g)) if g.statements.is_empty() => Expr::bin(Op::And, t, g.value),
            (Some(t), Some(g)) => self.conditional(
                t,
                g,
                Evaluation {
                    statements: Vec::new(),
                    value: Expr::bool(false),
                },
                span,
                out,
            ),
            (None, Some(g)) => {
                out.extend(g.statements);
                g.value
            }
            (Some(t), None) => t,
            (None, None) => Expr::bool(true),
        };
        Ok(Some(test))
    }

    /// A JS boolean test for "`subject` matches `pat`" (`None`: always matches).
    pub(super) fn pattern_test(
        &mut self,
        pat: &Pat<'tcx>,
        subject: &Expr,
        bindings: &mut Vec<Binding<'tcx>>,
    ) -> R<Option<Expr>> {
        match &pat.kind {
            PatKind::Wild => Ok(None),
            PatKind::Binding {
                name,
                var,
                mode,
                subpattern,
                ty,
                ..
            } => {
                // A `ref mut` binding names the place it matched (`bind_all`),
                // so even a number's can be written through.
                let by_ref_mut = matches!(mode.0, ByRef::Yes(_, Mutability::Mut));
                if !by_ref_mut {
                    self.check_by_value(*mode, *ty, pat.span)?;
                }
                bindings.push(Binding {
                    var: *var,
                    name: name.to_string(),
                    mutable: mode.1 == Mutability::Mut,
                    by_ref_mut,
                    whole: subpattern.is_some(),
                    place: subject.clone(),
                    ty: *ty,
                });
                // `x @ 1..=9`: bound, and tested by what's after the `@`.
                match subpattern {
                    Some(inner) => self.pattern_test(inner, subject, bindings),
                    None => Ok(None),
                }
            }
            PatKind::Constant { value } => {
                let value = self.const_value(*value, pat.span)?;
                Ok(Some(Expr::bin(Op::Eq, subject.clone(), value)))
            }
            // `1..=9`, `i32::MIN..0`, `'a'..='z'`: between its bounds, as `<`
            // compares numbers, and `char`s by their UTF-16 units (ADR 0034).
            PatKind::Range(range) => {
                let bound = |boundary: &PatRangeBoundary<'tcx>| match *boundary {
                    PatRangeBoundary::Finite(valtree) => {
                        let value = ty::Value { ty: range.ty, valtree };
                        const_js(self.tcx, value)
                            .map(Some)
                            .ok_or_else(|| self.unsupported(pat.span, "this range"))
                    }
                    PatRangeBoundary::NegInfinity | PatRangeBoundary::PosInfinity => Ok(None),
                };
                let (mut lo, mut hi) = (bound(&range.lo)?, bound(&range.hi)?);
                let below = if range.end == RangeEnd::Included {
                    Op::Le
                } else {
                    Op::Lt
                };
                // A bound at the type's own end always holds: `n >= 0` of a `u32`.
                if let Some(num) = Num::of(range.ty).filter(|&n| !n.float()) {
                    let (min, max) = num.range();
                    lo = lo.filter(|lo| lo.as_int() != Some(min));
                    hi = hi.filter(|hi| !(range.end == RangeEnd::Included && hi.as_int() == Some(max)));
                }
                let tests: Vec<Expr> = lo
                    .map(|lo| Expr::bin(Op::Ge, subject.clone(), lo))
                    .into_iter()
                    .chain(hi.map(|hi| Expr::bin(below, subject.clone(), hi)))
                    .collect();
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::And, a, b)))
            }
            // `Some(p)`: not `null` or `undefined`, and the value itself matches `p`.
            // A constant needs no `!= null`: `o === 0` already says it. So does
            // one through a reference, like every string literal: `o === "a"`.
            PatKind::Variant {
                adt_def,
                variant_index,
                subpatterns,
                ..
            } if self.tcx.is_lang_item(adt_def.did(), LangItem::Option) => {
                // `None`, or `Some(..)`, whose `..` names no field but is `Some`.
                let some = self
                    .tcx
                    .is_lang_item(adt_def.variant(*variant_index).def_id, LangItem::OptionSome);
                let Some(field) = subpatterns.first() else {
                    let op = if some { Op::LooseNe } else { Op::LooseEq };
                    return Ok(Some(Expr::bin(op, subject.clone(), Expr::null())));
                };
                // A generic `T`'s value may be boxed (ADR 0051): the pattern is on what's inside.
                let value = match self.option_of(pat.ty) {
                    Some(inner) if self.boxed_payload(inner) => self.some_value(subject.clone()),
                    _ => subject.clone(),
                };
                let inner = self.pattern_test(&field.pattern, &value, bindings)?;
                let present = Expr::bin(Op::LooseNe, subject.clone(), Expr::null());
                let mut value = &field.pattern;
                while let PatKind::Deref { subpattern, .. } = &value.kind {
                    value = subpattern;
                }
                Ok(Some(match inner {
                    // `undefined >= 1` is false too.
                    Some(test) if matches!(value.kind, PatKind::Constant { .. } | PatKind::Range(_)) => test,
                    Some(test) => Expr::bin(Op::And, present, test),
                    None => present,
                }))
            }
            // A variant (ADR 0013, 0033): its name, or its `TAG`, then its fields.
            // An enum with one variant needs no test.
            PatKind::Variant {
                adt_def,
                variant_index,
                subpatterns,
                ..
            } => {
                let variant = adt_def.variant(*variant_index);
                if let Some(n) = ordering_value(self.tcx, adt_def.did(), variant.name) {
                    return Ok(Some(Expr::bin(Op::Eq, subject.clone(), Expr::int(n))));
                }
                let name = Expr::str(bindings::variant_name(self.tcx, variant));
                let mut tests = Vec::new();
                if adt_def.variants().len() > 1 {
                    tests.push(match variant.fields.is_empty() {
                        true => Expr::bin(Op::Eq, subject.clone(), name),
                        false => Expr::bin(Op::Eq, Expr::member(subject.clone(), "TAG"), name),
                    });
                }
                for field in subpatterns {
                    let part = Expr::member(
                        subject.clone(),
                        variant_field(self.tcx, variant, field.field.as_usize()),
                    );
                    tests.extend(self.pattern_test(&field.pattern, &part, bindings)?);
                }
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::And, a, b)))
            }
            // Matching through a reference: the reference is the value (ADR 0023).
            // `&mut 3` of a cell, a box or a handle: what it points at (ADR 0099).
            PatKind::Deref { subpattern, .. } if self.is_cell(pat.ty) => {
                let pointee = match &subject.kind {
                    js::ExprKind::Handle(place) => (**place).clone(),
                    _ => Expr::member(subject.clone(), "value"),
                };
                self.pattern_test(subpattern, &pointee, bindings)
            }
            PatKind::Deref { subpattern, .. } => self.pattern_test(subpattern, subject, bindings),
            // A struct or tuple: every field must match.
            PatKind::Leaf { subpatterns } => {
                let mut tests = Vec::new();
                for field in subpatterns {
                    let part = self.project(subject.clone(), pat.ty, field.field.as_usize());
                    tests.extend(self.pattern_test(&field.pattern, &part, bindings)?);
                }
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::And, a, b)))
            }
            PatKind::Or { pats } => {
                let before = bindings.len();
                let mut tests = Vec::new();
                for p in pats {
                    match self.pattern_test(p, subject, bindings)? {
                        Some(t) => tests.push(t),
                        None => return Ok(None),
                    }
                }
                if bindings.len() != before {
                    return Err(self.unsupported(pat.span, "bindings inside `|` patterns"));
                }
                Ok(tests.into_iter().reduce(|a, b| Expr::bin(Op::Or, a, b)))
            }
            _ => Err(self.unsupported(pat.span, "this pattern")),
        }
    }
}
