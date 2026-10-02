//! Function setup, captures and nested-body entry/exit. Context changes live here.

use super::{Body, Dest, Enclosing, EnclosingKind, FnCx, ItemScope, LoweredFn, Nested, R, Var, lower_first, root_var};
use crate::js::{self, Expr, Stmt, StmtKind};
use rustc_ast::Mutability;
use rustc_hir::{BindingMode, ByRef, CoroutineDesugaring, CoroutineKind, CoroutineSource};
use rustc_middle::thir::{self, BodyTy, ExprId, ExprKind, Pat, PatKind};
use rustc_middle::ty;
use rustc_span::Span;
use rustc_span::def_id::DefId;

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    pub(super) fn lower_fn(&mut self, body: &Body<'tcx>) -> R<LoweredFn> {
        let def_id = body.def_id.to_def_id();
        let mut out = Vec::new();
        let thir = self.thir;
        let evidence = self.evidence_params(def_id);
        self.drop_facts()?;
        let (mut params, is_async) = self.lower_signature(def_id, &thir.params.raw, body.expr, &mut out)?;
        params.extend(evidence);
        self.check_drops()?;

        Ok(LoweredFn {
            function: js::Function {
                name: self.krate.fns[&def_id].name.clone(),
                params,
                body: out,
                export: self.tcx.visibility(def_id).is_public()
                    && (self.tcx.def_kind(def_id) != rustc_hir::def::DefKind::AssocFn
                        || self.tcx.inherent_impl_of_assoc(def_id).is_some()),
                is_async,
                span: self.js_span(self.tcx.def_span(def_id)),
                name_span: self
                    .tcx
                    .def_ident_span(def_id)
                    .map_or(js::Span::NONE, |s| self.js_span(s)),
            },
            runtime: std::mem::take(&mut self.runtime),
            jsx: self.jsx,
            dependencies: self.dependencies.take(),
        })
    }

    /// A function's parameters and body, in `out`, and whether it's `async`.
    /// One that writes to a `Formatter` returns the string (ADR 0054).
    pub(super) fn lower_signature(
        &mut self,
        def_id: DefId,
        params: &[thir::Param<'tcx>],
        body: ExprId,
        out: &mut Vec<Stmt>,
    ) -> R<(Vec<js::Pattern>, bool)> {
        let span = self.tcx.def_span(def_id);
        if let Some(formatter) = self.formatter_param(def_id) {
            return Ok((self.lower_writer(params, formatter, body, span, out)?, false));
        }
        // The parameters are the body's to drop (ADR 0098).
        let mark = self.owned_mark();
        let params = self.lower_params(params, span, out)?;
        let BodyTy::Fn(sig) = self.thir.body_type else {
            return Err(self.unsupported(span, "this kind of body"));
        };
        self.check_value_ty(sig.output(), span)?;
        let dest = if sig.output().is_unit() {
            Dest::Discard
        } else {
            Dest::Return
        };
        let mut lowered = Vec::new();
        let is_async = self.lower_body(body, &dest, &mut lowered)?;
        if is_async && self.owned_mark() > mark {
            return Err(self.unsupported(span, "an `async` function that owns a value with a destructor"));
        }
        self.close_scope(mark, lowered, span, out)?;
        Ok((params, is_async))
    }

    /// Name the parameters. One with a pattern (`(x, y): (i32, i32)`) is
    /// taken whole, then taken apart at the start of the body in `out`.
    pub(super) fn lower_params(
        &mut self,
        params: &[thir::Param<'tcx>],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Vec<js::Pattern>> {
        let mut names = Vec::new();
        for param in params {
            let span = param.ty_span.unwrap_or(span);
            // `out: &mut String`: a box, `out.value` (ADR 0072), and `x: &mut T`
            // whatever `T` is (ADR 0099).
            if let ty::Ref(_, inner, Mutability::Mut) = *param.ty.kind()
                && (self.is_boxable(inner) || self.is_generic_boxed(inner, self.typing_env.param_env))
                && let Some(Pat {
                    kind:
                        PatKind::Binding {
                            name,
                            var,
                            mode: BindingMode(ByRef::No, _),
                            subpattern: None,
                            ..
                        },
                    ..
                }) = param.pat.as_deref()
            {
                let name = self.bind(*var, name.as_str(), false);
                self.locals.boxes.insert(*var);
                names.push(js::Pattern::Name(name));
                continue;
            }
            self.check_value_ty(param.ty, span)?;
            // `|&x|`: a reference is the value (ADR 0023), so the parameter is `x`.
            let mut inner = param.pat.as_deref();
            while let Some(Pat {
                kind: PatKind::Deref { subpattern, .. },
                ..
            }) = inner
            {
                inner = Some(subpattern);
            }
            let binding = |p: &Pat<'tcx>| {
                matches!(
                    p.kind,
                    PatKind::Binding {
                        mode: BindingMode(ByRef::No, Mutability::Not),
                        subpattern: None,
                        ..
                    }
                )
            };
            let peeled = if inner.is_some_and(binding) {
                inner
            } else {
                param.pat.as_deref()
            };
            // `Props { initial, label }: Props` is `{ initial, label }`, as a
            // React component takes its props.
            // One with a destructor is owned by the function, less what its
            // pattern moves out: taken apart below (ADR 0131).
            if let Some(pat) = peeled
                && !self.has_drops(param.ty)
                && let Some((pattern, _)) = self.js_pattern(pat)
            {
                names.push(pattern);
                continue;
            }
            let name = match peeled {
                Some(pat) => match &pat.kind {
                    PatKind::Binding {
                        name,
                        var,
                        mode,
                        subpattern: None,
                        ..
                    } => {
                        self.check_by_value(*mode, pat.ty, pat.span)?;
                        // `async fn f((a, b): ..)` takes `__arg0`, and takes it
                        // apart in its body (ADR 0029): named as in a plain `fn`.
                        let generated = name
                            .as_str()
                            .strip_prefix("__arg")
                            .is_some_and(|n| n.parse::<u32>().is_ok());
                        // A method's `self` is named after its type, `counter`
                        // for a `Counter`, as a JS function of one would name it.
                        let receiver = match pat.ty.peel_refs().kind() {
                            ty::Adt(adt, _) if name.as_str() == "self" => {
                                Some(lower_first(self.tcx.item_name(adt.did()).as_str()))
                            }
                            _ => None,
                        };
                        let rust_name = match &receiver {
                            Some(receiver) => receiver.as_str(),
                            None if generated => "param",
                            None => name.as_str(),
                        };
                        let name = self.bind(*var, rust_name, mode.1 == Mutability::Mut);
                        // The function owns its parameter however it's bound:
                        // `ref n` too (ADR 0098).
                        if self.has_drops(param.ty) {
                            self.own(*var, Expr::var(&name), param.ty, pat.span, out)?;
                        }
                        name
                    }
                    PatKind::Wild => {
                        let name = self.fresh("_");
                        if self.has_drops(param.ty) {
                            self.own_value(Expr::var(&name), param.ty);
                        }
                        name
                    }
                    // `(x, y): (i32, i32)`: take the whole value, then take it apart.
                    // The function owns what the pattern leaves of it, `ref b`'s,
                    // dropped after what it binds, as Rust drops them (ADR 0131).
                    _ => {
                        let name = self.fresh("param");
                        if self.has_drops(param.ty) {
                            self.own_rest(Expr::var(&name), param.ty, pat)?;
                        }
                        self.destructure(pat, Expr::var(&name), true, false, out)?;
                        name
                    }
                },
                None => self.fresh("_"),
            };
            names.push(name.into());
        }
        Ok(names)
    }

    // ── Closures (ADR 0022) ─────────────────────────────────────────────

    /// A closure is an arrow function, lowered right where it's created.
    ///
    /// JS closures capture *variables*, which is what a Rust capture by
    /// reference means, and the borrow checker has made sure nothing else
    /// uses them meanwhile. A capture by value is a copy: for an immutable
    /// variable that's the same thing, so only mutable ones get a snapshot.
    pub(super) fn closure(&mut self, closure: &thir::ClosureExpr<'tcx>, out: &mut Vec<Stmt>) -> R<Expr> {
        let body: &'a Body<'tcx> = self.krate.closures[&closure.closure_id];
        let mut shadowed = Vec::new();
        for &upvar in closure.upvars.iter() {
            if !self.needs_snapshot(upvar) {
                continue;
            }
            // Since Rust 2021 a closure may capture part of a variable
            // (`p.x`), so the snapshot stands for that place.
            let span = self.thir[upvar].span;
            let Some(path) = self.body_query().place_path(upvar) else {
                return Err(self.unsupported(span, "capturing this place by value"));
            };
            let value = self.read(upvar, out)?;
            // Named after what it copies, from the Rust name: `n` gives `n$1`.
            let base = match self.place(upvar) {
                Some((
                    Expr {
                        kind: js::ExprKind::Member(_, field),
                        ..
                    },
                    _,
                )) => field,
                Some((
                    Expr {
                        kind: js::ExprKind::Var(name),
                        ..
                    },
                    _,
                )) => name,
                _ => "capture".to_string(),
            };
            let name = self.fresh(base.split('$').next().unwrap_or_default());
            out.push(StmtKind::Let(name.clone(), Some(value)).at(self.js_span(span)));
            let snapshot = Var {
                place: Expr::var(&name),
                mutable: true,
                depth: self.loops.len(),
            };
            shadowed.push((path.clone(), self.captures.insert(path, snapshot)));
        }

        // What it takes by value that has a destructor is moved into it, and
        // its drop drops that, where it's made (ADR 0098). Called once, by
        // a body that moves what it holds, the body owns it.
        let mut held = Vec::new();
        if let ty::UpvarArgs::Closure(args) = closure.args
            && let closure_ty = ty::Ty::new_closure(self.tcx, closure.closure_id.to_def_id(), args)
            && self.has_drops(closure_ty)
        {
            let captures = self.tcx.closure_captures(closure.closure_id);
            for (captured, &upvar) in captures.iter().zip(closure.upvars.iter()) {
                let ty = captured.place.ty();
                if captured.is_by_ref() || !self.has_drops(ty) {
                    continue;
                }
                self.moved(upvar, out)?;
                let Some((place, _)) = self.place(upvar) else {
                    return Err(self.unsupported(self.thir[upvar].span, "capturing this value with a destructor"));
                };
                held.push((thir::LocalVarId(captured.get_root_variable()), place, ty));
            }
            self.closure_made(
                closure.closure_id.to_def_id(),
                held.iter().map(|(_, place, ty)| (place.clone(), *ty)).collect(),
            );
            if args.as_closure().kind() != ty::ClosureKind::FnOnce {
                held.clear();
            }
        }

        // Lower the body as if it were a function of its own, then come back.
        // Its names are its own: once it's lowered, a sibling closure or later
        // code may use them again (`v.some((x) => ..)`, `v.every((x) => ..)`).
        // Like a JS arrow's, they may reuse an outer name, `(count) => count + 1`,
        // unless the closure uses what that name holds: a capture.
        let mut inner = self.module_names.clone();
        let mut known = true;
        for &upvar in closure.upvars.iter() {
            match self.place(upvar) {
                Some((place, _)) => inner.extend(root_var(&place).map(str::to_string)),
                None => known = false,
            }
        }
        let names = if known { inner } else { self.names.clone() };
        let enclosing = self.enter_body(body, body.def_id.to_def_id(), Nested::Closure { names })?;
        let mut stmts = Vec::new();
        // An `async` block takes no arguments, and runs as soon as it's
        // made: an async arrow, called right away (ADR 0029).
        let block = matches!(
            self.tcx.coroutine_kind(closure.closure_id),
            Some(CoroutineKind::Desugared(
                CoroutineDesugaring::Async,
                CoroutineSource::Block
            ))
        );
        let span = self.tcx.def_span(body.def_id);
        if self.drop_facts()?.has_owners() && block {
            return Err(self.unsupported(span, "an `async` block that owns a value with a destructor"));
        }
        let mark = self.owned_mark();
        // What it holds, dropped after its parameters, in the order it took
        // them: owners of its own, whose flags are its own.
        let flags = self.take_flags(&held.iter().map(|&(var, _, _)| var).collect::<Vec<_>>());
        for (var, place, ty) in held.into_iter().rev() {
            self.own(var, place, ty, span, &mut stmts)?;
        }
        // The first parameter is the closure itself, which JS doesn't need.
        let params = if block {
            Vec::new()
        } else {
            // JS ignores extra arguments, so `|_| ..` is `() => ..`, unless
            // the closure drops what it's given.
            let mut params = &body.thir.params.raw[1..];
            while let [rest @ .., last] = params
                && last.pat.as_deref().is_some_and(|p| matches!(p.kind, PatKind::Wild))
                && !self.has_drops(last.ty)
            {
                params = rest;
            }
            self.lower_params(params, self.tcx.def_span(body.def_id), &mut stmts)?
        };
        let BodyTy::Fn(sig) = body.thir.body_type else {
            unreachable!("a closure body is a function")
        };
        let dest = if sig.output().is_unit() {
            Dest::Discard
        } else {
            Dest::Return
        };
        let mut lowered = Vec::new();
        let is_async = if block {
            self.stmt(body.expr, &Dest::Return, &mut lowered)?;
            true
        } else {
            self.lower_body(body.expr, &dest, &mut lowered)?
        };
        if is_async && self.owned_mark() > mark {
            return Err(self.unsupported(span, "an `async` closure that owns a value with a destructor"));
        }
        self.close_scope(mark, lowered, span, &mut stmts)?;
        self.leave_body(enclosing)?;
        self.give_flags(flags);
        for (path, previous) in shadowed {
            match previous {
                Some(var) => self.captures.insert(path, var),
                None => self.captures.remove(&path),
            };
        }
        Ok(match (block, is_async) {
            (true, _) => Expr::call(Expr::async_arrow(params, stmts), Vec::new()),
            (false, true) => Expr::async_arrow(params, stmts),
            (false, false) => Expr::arrow(params, stmts),
        })
    }

    /// Lower a function's or a closure's body. For an `async fn` or an
    /// `async` closure, that's the body of the coroutine it returns: in JS,
    /// an `async` function's body. Says whether it was async (ADR 0029).
    pub(super) fn lower_body(&mut self, e: ExprId, dest: &Dest, out: &mut Vec<Stmt>) -> R<bool> {
        let coroutine = match self.thir[self.strip(e)].kind {
            ExprKind::Closure(ref closure)
                if matches!(
                    self.tcx.coroutine_kind(closure.closure_id),
                    Some(CoroutineKind::Desugared(
                        CoroutineDesugaring::Async,
                        CoroutineSource::Fn | CoroutineSource::Closure
                    ))
                ) =>
            {
                closure.closure_id
            }
            _ => {
                self.stmt(e, dest, out)?;
                return Ok(false);
            }
        };
        // Its captures are this function's parameters and variables, so no snapshots.
        let body: &'a Body<'tcx> = self.krate.closures[&coroutine];
        let enclosing = self.enter_body(body, body.def_id.to_def_id(), Nested::Coroutine)?;
        // A future dropped before it's done drops what it holds, which a JS
        // promise can't be (ADR 0098).
        if self.drop_facts()?.has_owners() {
            let span = self.tcx.def_span(body.def_id);
            return Err(self.unsupported(span, "`async` code that owns a value with a destructor"));
        }
        self.stmt(body.expr, &Dest::Return, out)?;
        self.leave_body(enclosing)?;
        Ok(true)
    }

    /// Start lowering `thir`, a body inside the one being lowered, as
    /// `nested` says it starts. Each body finds its own stepped iterators
    /// (ADR 0071) and is checked for what it drops (ADR 0098). A body that
    /// fails leaves the state as it is: its whole item fails with it.
    pub(super) fn enter_body(
        &mut self,
        body: &'a Body<'tcx>,
        owner: DefId,
        nested: Nested<'tcx>,
    ) -> R<Enclosing<'a, 'tcx>> {
        let own = body.facts.stepped.iter().copied();
        let thir = std::mem::replace(&mut self.thir, &body.thir);
        let body_facts = std::mem::replace(&mut self.body_facts, &body.facts);
        let body_owner = std::mem::replace(&mut self.body_owner, owner);
        let stepped = self.stepped.clone();
        let kind = match nested {
            Nested::Closure { names } => {
                self.stepped.extend(own);
                EnclosingKind::Closure {
                    loops: std::mem::take(&mut self.loops),
                    names: std::mem::replace(&mut self.names, names),
                }
            }
            Nested::Coroutine => {
                self.stepped.extend(own);
                EnclosingKind::Coroutine
            }
            Nested::Default {
                evidence,
                self_args,
                typing_env,
                drops,
                unsupported,
            } => {
                self.stepped = own.collect();
                EnclosingKind::Default(Box::new(ItemScope {
                    drops: self.swap_drops(drops, unsupported),
                    names: self.names.clone(),
                    locals: std::mem::take(&mut self.locals),
                    evidence: std::mem::replace(&mut self.evidence, evidence),
                    self_args: self.self_args.replace(self_args),
                    typing_env: std::mem::replace(&mut self.typing_env, typing_env),
                }))
            }
        };
        self.drop_facts()?;
        Ok(Enclosing {
            thir,
            body_facts,
            body_owner,
            stepped,
            kind,
        })
    }

    /// Finish the body `enter_body` started, and go back to the enclosing one.
    pub(super) fn leave_body(&mut self, enclosing: Enclosing<'a, 'tcx>) -> R<()> {
        self.check_drops()?;
        self.thir = enclosing.thir;
        self.body_facts = enclosing.body_facts;
        self.body_owner = enclosing.body_owner;
        self.stepped = enclosing.stepped;
        match enclosing.kind {
            EnclosingKind::Closure { loops, names } => {
                self.loops = loops;
                self.names = names;
            }
            EnclosingKind::Coroutine => {}
            EnclosingKind::Default(scope) => {
                let ItemScope {
                    names,
                    locals,
                    evidence,
                    self_args,
                    typing_env,
                    drops,
                } = *scope;
                self.names = names;
                self.locals = locals;
                self.evidence = evidence;
                self.self_args = self_args;
                self.typing_env = typing_env;
                self.restore_drops(drops);
            }
        }
        Ok(())
    }

    /// Does capturing `upvar` need a snapshot? Only a by-value capture of a
    /// mutable variable does, and not when the capture is the variable's
    /// only use, outside any loop the variable isn't also in.
    pub(super) fn needs_snapshot(&self, upvar: ExprId) -> bool {
        let u = self.strip(upvar);
        if matches!(self.thir[u].kind, ExprKind::Borrow { .. }) {
            return false;
        }
        let Some(var) = self.body_query().root_var(u).and_then(|id| self.locals.vars.get(&id)) else {
            return false;
        };
        if !var.mutable {
            return false;
        }
        let only_use = match self.thir[u].kind {
            ExprKind::VarRef { id } => self.body_facts.uses.get(&id) == Some(&1) && var.depth == self.loops.len(),
            _ => false,
        };
        !only_use
    }
}
