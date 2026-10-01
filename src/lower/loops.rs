//! Loop lowering and labels: preserve control flow and iteration order.

use super::body_queries::ForLoop;
use super::{Dest, FnCx, Loop, R, Std, Var, fresh_in, is_enumerate_pair, std_impls, without_refs};
use crate::js::{self, Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_ast::Mutability;
use rustc_hir as hir;
use rustc_hir::{BindingMode, ByRef, HirId, LangItem};
use rustc_middle::middle::region;
use rustc_middle::thir::{self, ExprId, ExprKind, PatKind};
use rustc_middle::ty;
use rustc_span::{Span, Symbol, sym};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    pub(super) fn lower_loop(
        &mut self,
        scope: region::Scope,
        hir_id: HirId,
        body: ExprId,
        dest: &Dest,
        span: js::Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let label_base = match self.tcx.hir_expect_expr(hir_id).kind {
            hir::ExprKind::Loop(_, Some(label), ..) => label.ident.name.as_str().trim_start_matches('\'').to_string(),
            _ => "loop".to_string(),
        };
        self.loops.push(Loop {
            scope,
            label_base,
            label: None,
            dest: dest.clone(),
        });

        let mut body_out = Vec::new();
        // `while c { .. }` reaches us desugared as `loop { if c { .. } else { break } }`.
        // Put the `while` back.
        let cond = match self.as_while(body, scope) {
            Some((cond, then)) => {
                let mut before = Vec::new();
                let cond = self.expr(cond, &mut before)?;
                if before.is_empty() {
                    self.stmt(then, &Dest::Discard, &mut body_out)?;
                    cond
                } else {
                    // A condition whose JS has statements, as `a && f(&mut y)`'s
                    // does: they run each time round, before its test, so
                    // `while (true) { ..; if (!c) break; .. }`.
                    body_out.extend(before);
                    body_out.push(
                        StmtKind::If(std_impls::negate(cond), vec![StmtKind::Break(None).at(span)], None).at(span),
                    );
                    self.stmt(then, &Dest::Discard, &mut body_out)?;
                    Expr::bool(true)
                }
            }
            None => {
                self.stmt(body, &Dest::Discard, &mut body_out)?;
                Expr::bool(true)
            }
        };

        let label = self.loops.pop().unwrap().label;
        out.push(
            StmtKind::While {
                label,
                cond,
                body: body_out,
            }
            .at(span),
        );
        Ok(())
    }

    /// `for x in &v` is `for (const x of v)`; `for i in a..b` is
    /// `for (let i = a; i < b; i++)`.
    pub(super) fn lower_for(&mut self, f: ForLoop<'a, 'tcx>, span: js::Span, out: &mut Vec<Stmt>) -> R<()> {
        let label_base = match self.tcx.hir_expect_expr(f.hir_id).kind {
            hir::ExprKind::Loop(_, Some(label), ..) => label.ident.name.as_str().trim_start_matches('\'').to_string(),
            _ => "loop".to_string(),
        };
        if let Some((items, range)) = self.mut_items(&f) {
            return self.index_loop(f, label_base, items, range, span, out);
        }
        let head_ty = self.reveal(self.thir[f.head].ty);
        let head_span = self.thir[f.head].span;
        let inclusive = self.inclusive_range(f.head);
        let range = self.is_lang_adt(head_ty, LangItem::Range) || inclusive.is_some();

        // What to loop over: a range's bounds, or a sequence.
        let (iterable, start_end) = if range {
            let (start, end) = match inclusive {
                Some(bounds) => bounds,
                None => {
                    let ExprKind::Adt(ref adt) = self.thir[self.strip(f.head)].kind else {
                        return Err(self.unsupported(head_span, "this range"));
                    };
                    let bound = |i: usize| {
                        adt.fields
                            .iter()
                            .find(|field| field.name.as_usize() == i)
                            .map(|field| field.expr)
                    };
                    let (Some(start), Some(end)) = (bound(0), bound(1)) else {
                        unreachable!("a range has a start and an end")
                    };
                    (start, end)
                }
            };
            self.num(self.thir[start].ty, head_span)?;
            let [start_js, end_js] = self.operands(&[start, end], out)?.try_into().ok().unwrap();
            // Rust works out the end once; JS would test it again each time round.
            let end_js = if end_js.is_constant() || self.stable_place(end).is_some() {
                end_js
            } else {
                let name = self.fresh("end");
                out.push(StmtKind::Const(name.clone(), end_js).at(span));
                Expr::var(&name)
            };
            (None, Some((start_js, end_js)))
        } else {
            let peeled = head_ty.peel_refs();
            // An `Option`, or a `&Option`: a `&mut` one's items are places.
            let option = self
                .option_of(peeled)
                .filter(|_| !matches!(head_ty.kind(), ty::Ref(_, _, Mutability::Mut)));
            let sequence = option.is_some()
                || peeled.is_array()
                || peeled.is_slice()
                || self.is_vec_like(peeled)
                || self.is_std_adt(peeled, Symbol::intern("SliceIter"))
                || self.is_str_split(peeled)
                || self.is_array_iter(peeled)
                || self.is_lazy_iter(peeled)
                || self.is_map(peeled)
                // A generic one, an array or a JS iterator: `for .. of` takes either (ADR 0061).
                || self.bounded_by(peeled, sym::IntoIterator)
                || matches!(self.thir[self.strip(f.head)].kind, ExprKind::Call { fun, .. } if self.std_fn(fun) == Some(Std::Same));
            if !sequence {
                return Err(self.unsupported(head_span, &format!("iterating over `{head_ty}`")));
            }
            let head = self.iter_value(f.head, out)?;
            let head = match option {
                Some(item) => self.option_items(head, item, out),
                None => head,
            };
            let head = self.in_order_of(head, head_ty, head_span)?;
            (Some(self.iter_source(head, head_ty, head_span)?), None)
        };

        // The loop variable: the pattern's own name if it's a plain
        // immutable binding, a JS pattern for a tuple's or a struct's parts,
        // else a fresh one that the body takes apart.
        let mut body = Vec::new();
        let mut mutable = false;
        // `for &x in &v`: a reference is the value (ADR 0023).
        let pat = without_refs(f.pat);
        let name = match &pat.kind {
            PatKind::Binding {
                name,
                var,
                mode,
                subpattern: None,
                ty,
                ..
            } if mode.1 == Mutability::Not && mode.0 == ByRef::No && !self.contains_mutated(*ty) => {
                self.check_value_ty(*ty, f.pat.span)?;
                if self.is_cell(*ty) {
                    self.locals.boxes.insert(*var);
                }
                js::Pattern::Name(self.bind(*var, name.as_str(), false))
            }
            _ if let Some((pattern, is_mut)) = self.js_pattern(pat) => {
                mutable = is_mut;
                pattern
            }
            _ => {
                let name = self.fresh(if range { "i" } else { "item" });
                self.destructure(f.pat, Expr::var(&name), true, false, &mut body)?;
                js::Pattern::Name(name)
            }
        };
        // `for (i, x) in v.iter().enumerate()` of an array: its `entries()`.
        let iterable = iterable.map(|it| match it.kind {
            js::ExprKind::Call(ref callee, ref args)
                if matches!(args.as_slice(), [f] if is_enumerate_pair(f))
                    && let js::ExprKind::Member(ref items, ref method) = callee.kind
                    && method == "map"
                    && !self.is_lazy_iter(head_ty) =>
            {
                Expr::call(Expr::member((**items).clone(), "entries"), vec![])
            }
            _ => it,
        });

        self.loops.push(Loop {
            scope: f.scope,
            label_base,
            label: None,
            dest: Dest::Discard,
        });
        self.stmt(f.body, &Dest::Discard, &mut body)?;
        let label = self.loops.pop().unwrap().label;
        out.push(
            match (iterable, start_end) {
                (Some(iterable), _) => StmtKind::ForOf {
                    label,
                    pattern: name,
                    mutable,
                    iterable,
                    body,
                },
                (None, Some((start, end))) => {
                    let js::Pattern::Name(name) = name else {
                        unreachable!("a range's item is a number, bound by name")
                    };
                    // `1..=n` includes its end.
                    let op = if inclusive.is_some() { Op::Le } else { Op::Lt };
                    let test = Expr::bin(op, Expr::var(&name), end);
                    StmtKind::For {
                        label,
                        name,
                        start,
                        test,
                        body,
                    }
                }
                (None, None) => unreachable!("a range or a sequence"),
            }
            .at(span),
        );
        Ok(())
    }

    /// What `for x in &mut v` changes, if it's an index loop (ADR 0099):
    /// `v` of `&mut v` or `v.iter_mut()`, of a `Vec`, an array or a slice
    /// whose items aren't objects, bound to a plain `x`; and the range
    /// of `&mut v[a..b]`, if it's one.
    pub(super) fn mut_items(&self, f: &ForLoop<'a, 'tcx>) -> Option<(ExprId, Option<ExprId>)> {
        let PatKind::Binding {
            mode: BindingMode(ByRef::No, Mutability::Not),
            subpattern: None,
            ty,
            ..
        } = f.pat.kind
        else {
            return None;
        };
        let ty::Ref(_, item, Mutability::Mut) = *ty.kind() else {
            return None;
        };
        if self.is_object(item) {
            return None;
        }
        let mut e = self.strip(f.head);
        let mut range = None;
        loop {
            match self.thir[e].kind {
                ExprKind::Borrow { arg, .. }
                | ExprKind::Deref { arg }
                | ExprKind::PointerCoercion { source: arg, .. } => {
                    e = self.strip(arg);
                }
                ExprKind::Call { fun, ref args, .. } => {
                    let index_mut = matches!(*self.thir[fun].ty.kind(), ty::FnDef(d, _)
                        if self.tcx.trait_of_assoc(d).is_some_and(|t| self.tcx.is_lang_item(t, LangItem::IndexMut)));
                    let by_range = [
                        LangItem::Range,
                        LangItem::RangeFrom,
                        LangItem::RangeTo,
                        LangItem::RangeFull,
                    ]
                    .into_iter()
                    .any(|item| args.len() == 2 && self.is_lang_adt(self.thir[args[1]].ty, item));
                    if index_mut && by_range && range.is_none() {
                        range = Some(args[1]);
                    } else if self.std_fn(fun) != Some(Std::Same) {
                        return None;
                    }
                    e = self.strip(args[0]);
                }
                _ => break,
            }
        }
        let items = self.thir[e].ty.peel_refs();
        let sequence = items.is_array() || items.is_slice() || self.is_vec_like(items);
        // Its own items borrowed, not `&mut`s it holds: `for r in refs` of a
        // `Vec<&mut i32>` gives each cell (ADR 0099).
        let element = match items.kind() {
            ty::Array(element, _) | ty::Slice(element) => Some(*element),
            ty::Adt(_, args) => args.types().next(),
            _ => None,
        };
        (sequence && element == Some(item) && self.place(e).is_some()).then_some((e, range))
    }

    /// `for x in &mut v`: `for (let i = 0; i < v.length; i++)`, with `*x`
    /// naming `v[i]` (ADR 0099). Of `&mut v[a..b]`, from `a` to `b`, as
    /// Rust checks them.
    pub(super) fn index_loop(
        &mut self,
        f: ForLoop<'a, 'tcx>,
        label_base: String,
        items: ExprId,
        range: Option<ExprId>,
        span: js::Span,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let PatKind::Binding { var, .. } = f.pat.kind else {
            unreachable!("`mut_items` binds a name")
        };
        let (place, _) = self.place(items).expect("`mut_items` has a place");
        // The collection of the loop's start, whatever `cur` holds after.
        let place = if self.through_rebound(items, true) {
            self.spill("items", place, out)
        } else {
            self.fixed(place, false, out)
        };
        let length = Expr::member(place.clone(), "length");
        let (start, end) = match range.map(|r| self.strip(r)) {
            None => (Expr::int(0), length),
            Some(range) => {
                let range_ty = self.thir[range].ty;
                let ExprKind::Adt(ref adt) = self.thir[range].kind else {
                    return Err(self.unsupported(self.thir[range].span, "slicing by a range in a variable"));
                };
                let bound = |i: usize| {
                    adt.fields
                        .iter()
                        .find(|field| field.name.as_usize() == i)
                        .map(|field| field.expr)
                };
                let (start, end) = if self.is_lang_adt(range_ty, LangItem::Range) {
                    (bound(0), bound(1))
                } else if self.is_lang_adt(range_ty, LangItem::RangeFrom) {
                    (bound(0), None)
                } else if self.is_lang_adt(range_ty, LangItem::RangeTo) {
                    (None, bound(0))
                } else {
                    (None, None)
                };
                let mut bounds: Vec<ExprId> = start.into_iter().collect();
                bounds.extend(end);
                let mut values = self.operands(&bounds, out)?.into_iter();
                let start = match start {
                    Some(_) => values.next().expect("a start"),
                    None => Expr::int(0),
                };
                let end = end.map(|_| values.next().expect("an end"));
                if matches!(start.kind, js::ExprKind::Num(n) if n == 0.0) && end.is_none() {
                    (start, length)
                } else {
                    // Where it ends, checked as `&v[a..b]` is, once.
                    let start = if start.is_constant() {
                        start
                    } else {
                        self.spill("start", start, out)
                    };
                    let mut args = vec![place.clone(), start.clone()];
                    args.extend(end);
                    self.runtime.insert(Helper::SliceEnd);
                    let end = self.spill("end", Expr::call(Expr::var("$sliceEnd"), args), out);
                    (start, end)
                }
            }
        };
        let name = self.fresh("i");
        self.locals.aliases.insert(var);
        self.locals.vars.insert(
            var,
            Var {
                place: Expr::index(place, Expr::var(&name)),
                mutable: true,
                depth: self.loops.len(),
            },
        );
        self.loops.push(Loop {
            scope: f.scope,
            label_base,
            label: None,
            dest: Dest::Discard,
        });
        let mut body = Vec::new();
        self.stmt(f.body, &Dest::Discard, &mut body)?;
        let label = self.loops.pop().unwrap().label;
        let test = Expr::bin(Op::Lt, Expr::var(&name), end);
        out.push(
            StmtKind::For {
                label,
                name,
                start,
                test,
                body,
            }
            .at(span),
        );
        Ok(())
    }

    /// Recognize the `while` desugaring; returns `(cond, body)`.
    pub(super) fn as_while(&self, body: ExprId, scope: region::Scope) -> Option<(ExprId, ExprId)> {
        let ExprKind::Block { block } = self.thir[self.strip(body)].kind else {
            return None;
        };
        let block = &self.thir[block];
        let (true, Some(tail)) = (block.stmts.is_empty(), block.expr) else {
            return None;
        };
        let ExprKind::If {
            cond,
            then,
            else_opt: Some(els),
            ..
        } = self.thir[self.strip(tail)].kind
        else {
            return None;
        };
        let ExprKind::Block { block: els } = self.thir[self.strip(els)].kind else {
            return None;
        };
        let els = &self.thir[els];
        let ([stmt], None) = (&*els.stmts, els.expr) else {
            return None;
        };
        let thir::StmtKind::Expr { expr, .. } = self.thir[*stmt].kind else {
            return None;
        };
        let ExprKind::Break { label, value: None } = self.thir[self.strip(expr)].kind else {
            return None;
        };
        (label == scope && self.is_simple(cond)).then_some((cond, then))
    }

    pub(super) fn loop_index(&self, label: region::Scope, span: Span) -> R<usize> {
        self.loops
            .iter()
            .rposition(|l| l.scope == label)
            .ok_or_else(|| self.unsupported(span, "breaking out of a labeled block"))
    }

    /// JS needs a label only when jumping past the innermost loop.
    pub(super) fn jump_label(&mut self, i: usize) -> Option<String> {
        if i == self.loops.len() - 1 {
            return None;
        }
        if self.loops[i].label.is_none() {
            let base = self.loops[i].label_base.clone();
            let label = fresh_in(&mut self.labels, &base);
            self.loops[i].label = Some(label);
        }
        self.loops[i].label.clone()
    }

    /// `a..=b`: `RangeInclusive::new(a, b)`, with its bounds.
    pub(super) fn inclusive_range(&self, e: ExprId) -> Option<(ExprId, ExprId)> {
        match self.thir[self.strip(e)].kind {
            ExprKind::Call { fun, ref args, .. } if matches!(self.thir[self.strip(fun)].ty.kind(), &ty::FnDef(d, _) if self.tcx.is_lang_item(d, LangItem::RangeInclusiveNew)) => {
                Some((args[0], args[1]))
            }
            _ => None,
        }
    }
}
