//! Calls to local functions, JavaScript bindings, closures and standard operations.

use super::bindings::{JsForm, is_binding, is_method, js_form};
use super::combinators::Comb;
use super::combinators::StepOp;
use super::drops::Drops;
use super::recognition::{Catching, Std, StreamOp};
use super::{FnCx, R};
use crate::js;
use crate::js::{Expr, Op, Prop, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_ast::{LitKind, Mutability};
use rustc_hir::{LangItem, find_attr};
use rustc_middle::thir::{ExprId, ExprKind};
use rustc_middle::ty::{self, Ty, TypeVisitableExt};
use rustc_span::def_id::DefId;
use rustc_span::{Span, sym};

use super::format_args::without_newline;

/// A call being lowered: what's called, which function that is, its
/// arguments, and whether its value is used.
#[derive(Clone, Copy)]
pub(super) struct Call<'c, 'tcx> {
    pub(super) fun: ExprId,
    pub(super) def_id: DefId,
    pub(super) generic_args: ty::GenericArgsRef<'tcx>,
    pub(super) args: &'c [ExprId],
    pub(super) discarded: bool,
    pub(super) span: Span,
}

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// A call to one of our functions (local or imported by name),
    /// to JS (ADR 0021), or to one of the std functions rust-js knows (ADR 0023).
    pub(super) fn call(
        &mut self,
        fun: ExprId,
        args: &[ExprId],
        discarded: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let fun_span = self.js_span(self.thir[fun].span);
        let f = &self.thir[self.strip(fun)];
        let (ExprKind::ZstLiteral { .. }, &ty::FnDef(def_id, generic_args)) = (&f.kind, f.ty.kind()) else {
            if matches!(f.ty.kind(), ty::FnDef(..) | ty::FnPtr(..)) {
                let mut operands = vec![fun];
                operands.extend_from_slice(args);
                let mut values = self.operands(&operands, out)?;
                let callee = values.remove(0);
                return Ok(Expr::call(callee, values));
            }
            return Err(self.unsupported(f.span, "calling this"));
        };
        // serde_json's `Value` and what makes one (ADR 0083).
        if let Some(value) = self.json_call(def_id, generic_args, args, span, out)? {
            return Ok(value);
        }
        // `x.into()` is the `From::from(x)` it calls, when that's the crate's
        // own (ADR 0052).
        let (def_id, generic_args) = self
            .resolve_into(def_id, generic_args)
            .unwrap_or((def_id, generic_args));
        let call = Call {
            fun,
            def_id,
            generic_args,
            args,
            discarded,
            span,
        };
        if let Some(value) = self.special_call(call, fun_span, out)? {
            return Ok(value);
        }
        let Some(known) = self.std_fn(fun) else {
            let path = self.tcx.def_path_str(def_id);
            // A library's, that its manifest doesn't list (ADR 0100).
            if let Some(why) = self.krate.foreign.unlisted(def_id) {
                return Err(self.tcx.dcx().span_err(self.thir[fun].span, why));
            }
            return Err(self.unsupported(self.thir[fun].span, &format!("calling `{path}`")));
        };
        self.std_call(known, call, out)
    }

    /// A call of what isn't one of std's functions: the crate's own, a
    /// binding's (ADR 0021), a closure's, a trait method's, and what rust-js
    /// writes its own way. `None` if it's std's.
    fn special_call(&mut self, call: Call<'_, 'tcx>, fun_span: js::Span, out: &mut Vec<Stmt>) -> R<Option<Expr>> {
        let Call {
            fun,
            def_id,
            generic_args,
            args,
            discarded,
            span,
        } = call;
        if let Some(callee) = self.boxed_callee(def_id, generic_args, args, span)? {
            return self.call_with_boxes(callee, args, discarded, span, out).map(Some);
        }
        // `x == &mut 1` or `p < q` of `&mut`s to values JS can't change in
        // place: of what they point at, whatever each `&mut` is (ADR 0099).
        if let Some(trait_id) = self.tcx.trait_of_assoc(def_id)
            && (self.tcx.is_lang_item(trait_id, LangItem::PartialEq)
                || self.tcx.is_lang_item(trait_id, LangItem::PartialOrd)
                || self.tcx.is_diagnostic_item(rustc_span::sym::Ord, trait_id))
            && generic_args.types().next().is_some_and(|t| self.is_cell(t))
        {
            let pointees = self
                .tcx
                .mk_args_from_iter(generic_args.iter().map(|arg| match arg.as_type() {
                    Some(t) if self.is_cell(t) => t.builtin_deref(true).unwrap_or(t).into(),
                    _ => arg,
                }));
            let values = args
                .iter()
                .map(|&a| self.pointee_value(a, span, out))
                .collect::<R<Vec<_>>>()?;
            if let Some(compared) = self.trait_call(def_id, pointees, values.clone(), span, out)? {
                return Ok(Some(compared));
            }
            // `p < q` of numbers: the operator, as of `&i32`s.
            if let Some(Std::Operator(op)) = self.std_fn(fun)
                && let [left, right] =
                    <[Expr; 2]>::try_from(values).map_err(|_| self.unsupported(span, "comparing `&mut`s"))?
            {
                let ty = pointees.types().next().expect("a comparison has a type");
                return self.binary(op, left, right, None, ty, span).map(Some);
            }
            return Err(self.unsupported(span, "comparing `&mut`s"));
        }
        // `Box<dyn Error>` from an error or a message, by std's `From`, or
        // the `Into` it gives (ADR 0141): `{ value, impl }`.
        if let Some((to, from)) = self.converted(def_id, generic_args)
            && let Some(dictionary) = self.dyn_error_from(to, from, span)?
        {
            let value = self.expr(args[0], out)?;
            return Ok(Some(Expr::object(vec![
                Prop::Field("value".into(), value),
                Prop::Field("impl".into(), dictionary),
            ])));
        }
        if let Some(written) = self.write_call(def_id, generic_args, args, span, out)? {
            return Ok(Some(written));
        }
        if let Some(written) = self.debug_builder(def_id, args, span, out)? {
            return Ok(Some(written));
        }
        if self.is_rust_fn(def_id) && self.tcx.trait_of_assoc(def_id).is_none() {
            let callee = self.fn_ref(def_id);
            let mut values = self.operands(args, out)?;
            // An iterator of the crate's own, given where a generic one goes,
            // is a JS iterator (ADR 0061).
            let inputs = self
                .tcx
                .fn_sig(def_id)
                .instantiate_identity()
                .skip_normalization()
                .skip_binder()
                .inputs()
                .to_vec();
            for ((value, &arg), input) in values.iter_mut().zip(args).zip(inputs) {
                if matches!(input.kind(), ty::Param(_)) && self.given_as_iterator(def_id, input, self.thir[arg].ty) {
                    let taken = std::mem::replace(value, Expr::undefined());
                    *value = self.iter_source(taken, self.thir[arg].ty, span, out)?;
                }
            }
            let mut args = values;
            args.extend(self.evidence_args(def_id, generic_args, span)?);
            return Ok(Some(Expr::call(callee.or_at(fun_span), args)));
        }
        if is_binding(self.tcx, def_id) {
            // An `#[eii]` function is declared in an `extern` block too, but
            // it's Rust's, linked to its implementation, not JS's.
            if find_attr!(self.tcx, def_id, RustcEiiForeignItem) {
                return Err(self.unsupported(span, "externally implementable items, `#[eii]`,"));
            }
            match js_form(self.tcx, def_id) {
                JsForm::Jsx(tag) => return self.jsx(&tag, args, span, out).map(Some),
                JsForm::Prop(name) => return self.jsx_prop(name.as_deref(), args, span, out).map(Some),
                JsForm::Object(keys) => return self.object_binding(&keys, args, span, out).map(Some),
                _ => {}
            }
            let mut values = self.operands(args, out)?;
            // `()` given to JS is what a tuple is, an array (ADR 0020):
            // `use_effect(f, ())` is `useEffect(f, [])`.
            for (value, &arg) in values.iter_mut().zip(args) {
                if matches!(self.thir[self.strip(arg)].kind, ExprKind::Tuple { ref fields } if fields.is_empty()) {
                    *value = Expr::array(Vec::new());
                }
            }
            let mut args = values;
            let this = is_method(self.tcx, def_id).then(|| args.remove(0));
            let value = match (js_form(self.tcx, def_id), this) {
                // A method or a property is on `this`: it can't be an import.
                (JsForm::Call(name), Some(this)) if !name.contains('#') => {
                    Expr::call(Expr::member(this, name).or_at(fun_span), args)
                }
                (JsForm::Call(name), None) => Expr::call(self.js_ref(&name).or_at(fun_span), args),
                (JsForm::New(name), None) => Expr::new_(self.js_ref(&name).or_at(fun_span), args),
                (JsForm::Get(name), Some(this)) if args.is_empty() && !name.contains('#') => Expr::member(this, name),
                (JsForm::Set(name), Some(this)) if args.len() == 1 && !name.contains('#') => {
                    let value = args.remove(0);
                    out.push(StmtKind::Assign(Expr::member(this, name), value).at(self.js_span(span)));
                    Expr::undefined()
                }
                (JsForm::This, Some(this)) if args.is_empty() => this,
                (JsForm::CallThis, Some(this)) => Expr::call(this, args),
                (JsForm::InstanceOf(class), Some(this)) if args.is_empty() => {
                    Expr::bin(Op::InstanceOf, this, self.js_ref(&class))
                }
                _ => {
                    let what = format!(
                        "the `#[link_name]` of `{}` with this signature",
                        self.tcx.def_path_str(def_id)
                    );
                    return Err(self.unsupported(self.thir[fun].span, &what));
                }
            };
            return Ok(Some(self.catching(def_id, value)));
        }
        // Calling a closure, `f(a, b)`, is `Fn::call(&f, (a, b))`: in JS, `f(a, b)`.
        if let Some(fn_trait) = self.tcx.trait_of_assoc(def_id)
            && (self.tcx.fn_trait_kind_from_def_id(fn_trait).is_some()
                || self.tcx.async_fn_trait_kind_from_def_id(fn_trait).is_some())
        {
            let [callee, ExprKind::Tuple { fields }] = [args[0], args[1]].map(|a| &self.thir[self.strip(a)].kind)
            else {
                return Err(self.unsupported(span, "this closure call"));
            };
            let callee = match *callee {
                // `&f` or `&mut f`: the closure itself.
                ExprKind::Borrow { arg, .. } => arg,
                _ => args[0],
            };
            let mut list = vec![callee];
            list.extend(fields.iter().copied());
            let mut values = self.operands(&list, out)?;
            let callee = values.remove(0);
            return Ok(Some(Expr::call(callee, values)));
        }
        if self
            .tcx
            .trait_of_assoc(def_id)
            .is_some_and(|id| super::traits::operational(self.tcx, self.krate.foreign, id))
            || (self.tcx.trait_of_assoc(def_id).is_some()
                && self
                    .resolve_instance(def_id, generic_args)?
                    .is_some_and(|i| self.is_rust_fn(i.def_id())))
        {
            let mut pending = Vec::new();
            let values = self.operands(args, &mut pending)?;
            if let Some(call) = self.trait_call(def_id, generic_args, values, span, &mut pending)? {
                out.extend(pending);
                return Ok(Some(call));
            }
        }
        // A `fmt::Result` is nothing in JS (ADR 0054), without `Result`'s methods.
        if args
            .first()
            .is_some_and(|&a| self.is_fmt_result(self.thir[a].ty.peel_refs()))
        {
            return Err(self.unsupported(span, "methods of a `fmt::Result`"));
        }
        // `f.alternate()`: whether this writer is pretty, `{:#?}` (ADR 0137).
        if self.tcx.item_name(def_id).as_str() == "alternate"
            && self.tcx.def_path_str(def_id).starts_with("std::fmt::Formatter")
        {
            return self
                .writer_alternate()
                .ok_or_else(|| self.unsupported(span, "`alternate()` of a `Formatter` here"))
                .map(Some);
        }
        // `cmp::max(a, b)` of what isn't a number: `Ord::max(a, b)`'s (ADR 0136).
        if self.std_fn(fun).is_none()
            && let Some(name) = match self.tcx.def_path_str(def_id).as_str() {
                "std::cmp::max" | "core::cmp::max" => Some("max"),
                "std::cmp::min" | "core::cmp::min" => Some("min"),
                _ => None,
            }
        {
            let ord = self.tcx.get_diagnostic_item(sym::Ord).expect("std has `Ord`");
            let method = self
                .tcx
                .associated_item_def_ids(ord)
                .iter()
                .copied()
                .find(|&id| self.tcx.item_name(id).as_str() == name)
                .expect("`Ord` has `max` and `min`");
            let values = self.operands(args, out)?;
            if let Some(call) = self.trait_call(method, generic_args, values, span, out)? {
                return Ok(Some(call));
            }
        }
        Ok(None)
    }

    /// A call of one of std's functions rust-js knows (ADR 0023), `known`.
    fn std_call(&mut self, known: Std, call: Call<'_, 'tcx>, out: &mut Vec<Stmt>) -> R<Expr> {
        let Call {
            fun,
            def_id,
            generic_args,
            args,
            discarded,
            span,
        } = call;
        // One that takes a value with a destructor, or changes a place that
        // holds one, must keep or give back what it takes: these do. Another
        // might drop it, which JS wouldn't (ADR 0098). A value whose drops
        // rust-js can't follow, a `vec::IntoIter` of them say, might hold one.
        // `collect()` of a chain that owns its items drains it (ADR 0098).
        if known == Std::Collect
            && let Some(&receiver) = args.first()
        {
            self.mark_owned_drain(receiver);
        }
        let drained = args.first().is_some_and(|&a| self.is_drained(a));
        let holds_drops = |ty: Ty<'tcx>| self.drops(ty) != Drops::Nothing;
        let takes_drops = args.iter().any(|&a| match *self.thir[a].ty.kind() {
            ty::Ref(_, inner, Mutability::Mut) => holds_drops(inner),
            ty::Ref(..) => false,
            _ => holds_drops(self.thir[a].ty),
        });
        if takes_drops
            && !drained
            && !matches!(
                known,
                Std::Drop
                    | Std::Forget
                    | Std::Swap
                    | Std::Replace
                    | Std::Push
                    | Std::Same
                    | Std::VecMacro
                    | Std::Unwrap
                    | Std::UnwrapOk
                    | Std::Method("pop")
                    | Std::Index
                    | Std::Len
                    | Std::IsEmpty
            )
        {
            let path = self.tcx.def_path_str(def_id);
            return Err(self.unsupported(span, &format!("`{path}` of a value with a destructor")));
        }
        // A std function that makes an `Option` of a generic `T` must box it
        // (ADR 0051); these do, and others aren't supported.
        // Normalized, so an iterator's `Self::Item` is the item's type.
        let output = self
            .tcx
            .fn_sig(def_id)
            .instantiate(self.tcx, generic_args)
            .skip_normalization()
            .skip_binder()
            .output();
        let output = self
            .tcx
            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(output))
            .unwrap_or(output);
        // A `&mut` it made itself, to a value JS can't change in place, is the
        // item, not a cell (ADR 0099): only a pattern takes it apart.
        if let Some(cell) = self.makes_items(output, generic_args, args)
            && !self.is_item_call(fun)
        {
            let path = self.tcx.def_path_str(def_id);
            return Err(self.unsupported(span, &format!("a `{cell}` from `{path}` used as a value")));
        }
        // One whose `Some` is boxed where it looks like `None` (ADR 0051): only
        // these make one.
        let boxed = self.option_of(output).is_some_and(|inner| self.boxed_payload(inner));
        if boxed
            && !matches!(
                known,
                Std::Same
                    | Std::OptionMap
                    | Std::Method("pop")
                    | Std::First
                    | Std::SliceLast
                    | Std::SliceGet
                    | Std::OptionCloned
                    | Std::Comb(Comb::Then | Comb::ThenSome)
                    // An `Option` of the same type, or the closure's own.
                    | Std::Comb(Comb::Filter | Comb::Or | Comb::OrElse | Comb::AndThen)
                    | Std::Last
                    // `as_ref()`: the same value, box and all.
                    | Std::Pointee
                    | Std::ResultOk
                    | Std::ArrayMethod("find")
                    | Std::Extreme(_)
                    | Std::Step(StepOp::Next | StepOp::Peek)
                    // The old value, as it's kept: boxed already.
                    | Std::OptionTake
                    | Std::OptionReplace
            )
        {
            return Err(self.unsupported(span, "this call, for an `Option` of what could look like `None`"));
        }
        if let Std::Map(op) = known {
            return self.map_call(op, args, generic_args, discarded, span, out);
        }
        if let Std::Range(op) = known {
            return self.range_call(op, args, span, out);
        }
        // A standard stream holds nothing JS needs, and a write to one is
        // `print!`'s (ADR 0132).
        if let Std::Stream(op) = known {
            let mut values = self.operands(args, out)?.into_iter();
            if op != StreamOp::Open
                && let Some(on) = values.next()
                && on.has_effects()
            {
                out.push(StmtKind::Expr(on).at(self.js_span(span)));
            }
            return match op {
                StreamOp::Write { error } => {
                    let text = values.next().expect("a write's text");
                    Ok(match without_newline(text) {
                        Ok(line) => Expr::call(
                            Expr::member(Expr::var("console"), if error { "error" } else { "log" }),
                            vec![line],
                        ),
                        Err(text) => {
                            self.runtime.insert(Helper::Print);
                            Expr::call(Expr::var(if error { "$eprint" } else { "$print" }), vec![text])
                        }
                    })
                }
                _ => Ok(Expr::undefined()),
            };
        }
        // rustc's name for the type, as a string; of a type parameter, which
        // each caller's type names, it's an error.
        if let Std::TypeName { of_val } = known {
            let of = generic_args.type_at(0);
            if of.has_param() {
                return Err(self.unsupported(span, "`type_name` of a type parameter"));
            }
            if of_val {
                let value = self.expr(args[0], out)?;
                if value.has_effects() {
                    out.push(StmtKind::Expr(value).at(self.js_span(span)));
                }
            }
            let of = self
                .tcx
                .normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(of));
            return Ok(Expr::str(rustc_const_eval::util::type_name(self.tcx, of)));
        }
        if let Std::Comb(comb) = known {
            return self.comb_call(comb, args, generic_args, span, out);
        }
        if let Std::Text(op) = known {
            return self.text_call(op, args, generic_args, span, out);
        }
        if let Std::Number(op) = known {
            let ty = self.thir[args[0]].ty.peel_refs();
            return self.number_call(op, args, ty, span, out);
        }
        if let Std::ToJson(pretty) = known {
            let ty = generic_args.type_at(0);
            let value = self.expr(args[0], out)?;
            return self.json_text(value, ty, pretty, span);
        }
        if let Std::FromJson = known {
            let ty = generic_args.types().next().expect("`from_str::<T>`");
            let text = self.expr(args[0], out)?;
            return self.json_value(text, ty, span);
        }
        if let Std::Step(op) = known {
            return self.step_call(op, fun, args, generic_args, span, out);
        }
        if let Std::Heap(op) = known {
            return self.heap_call(op, args, span, out);
        }
        if known == Std::DequeRemove {
            let [items, at]: [Expr; 2] = self.operands(args, out)?.try_into().ok().expect("a deque and an index");
            self.runtime.insert(Helper::RemoveOpt);
            return Ok(Expr::call(Expr::var("$removeOpt"), vec![items, at]));
        }
        if known == Std::FromElem {
            return self.vec_of_copies(args, span, out);
        }
        // `vec![a, b]` is `box_assume_init_into_vec_unsafe(write_box_via_move(<box>, [a, b]))`.
        if known == Std::VecMacro {
            let ExprKind::Call { args: ref inner, .. } = self.thir[self.strip(args[0])].kind else {
                return Err(self.unsupported(span, "this `vec!`"));
            };
            return self.expr(inner[1], out);
        }
        if known.takes_iterator() || matches!(known, Std::Sort | Std::SortByKey) {
            let value = self.iterator_call(known, args, generic_args, span, out)?;
            // `items.find(f)` can't tell a found `None` from none found.
            if boxed
                && let js::ExprKind::Call(callee, found) = &value.kind
                && let js::ExprKind::Member(items, name) = &callee.kind
                && name == "find"
            {
                let items = if items.has_effects() {
                    self.spill("items", (**items).clone(), out)
                } else {
                    (**items).clone()
                };
                let index = Expr::call(Expr::member(items.clone(), "findIndex"), found.clone());
                return Ok(self.some_at(items, index));
            }
            return Ok(value);
        }
        // `s.push_str(t)`: JS strings don't change, so `s` gets a new one.
        if known == Std::PushStr {
            let ExprKind::Borrow { arg: place, .. } = self.thir[self.strip(args[0])].kind else {
                return Err(self.unsupported(span, "`push_str` on this"));
            };
            let value = self.expr(args[1], out)?;
            let js_span = self.js_span(span);
            // A place is written where it is: `t` can't change the `s` it's
            // pushed to, which Rust has borrowed.
            if self.slot_place(place).is_none() && self.map_slot(place).is_none() && self.place(place).is_some() {
                let target = self.assignee(place)?;
                out.push(StmtKind::Assign(target.clone(), Expr::bin(Op::Add, target, value)).at(js_span));
                return Ok(Expr::undefined());
            }
            // Else where `+=` would write: a map's slot, or what a call's cell
            // points at, `pick(&mut a, &mut b).push_str(t)` (ADR 0099).
            let (target, value) = self.prepare_assignment_target(place, true, value, span, out)?;
            let appended = Expr::bin(Op::Add, target.read(), value);
            target.write(appended, js_span, out);
            return Ok(Expr::undefined());
        }
        if let Std::AssignOperator(op) = known {
            let ExprKind::Borrow { arg: place, .. } = self.thir[self.strip(args[0])].kind else {
                return Err(self.unsupported(span, "this assignment"));
            };
            if let Some(target) = self.slot_place(place) {
                let value = self.expr(args[1], out)?;
                let value = self.shift_amount(op, value, place, args[1]);
                let ty = self.thir[place].ty;
                let value = self.binary(op, target.read(), value, None, ty, span)?;
                target.write(value, self.js_span(span), out);
                return Ok(Expr::undefined());
            }
            // `*m.entry(k).or_insert(0) += n` with a `&u32` `n`: as with a `u32` (ADR 0059).
            if let Some(slot) = self.map_slot(place) {
                // A trait call evaluates its receiver before its argument.
                let target = self.prepare_map_place(slot, true, span, out)?;
                let value = self.expr(args[1], out)?;
                let value = self.shift_amount(op, value, place, args[1]);
                let ty = self.thir[place].ty;
                let value = self.binary(op, target.read(), value, None, ty, span)?;
                target.write(value, self.js_span(span), out);
                return Ok(Expr::undefined());
            }
            let value = self.expr(args[1], out)?;
            let value = self.shift_amount(op, value, place, args[1]);
            let target = self.assignee(place)?;
            let ty = self.thir[place].ty;
            let current = self.binary(op, target.clone(), value, None, ty, span)?;
            let js_span = self.js_span(span);
            out.push(StmtKind::Assign(target, current).at(js_span));
            return Ok(Expr::undefined());
        }
        if known == Std::FmtNew {
            // `format_arguments::new(template, &args)`, the template a byte string.
            let ExprKind::Literal { lit, .. } = self.thir[self.strip_refs(args[0])].kind else {
                return Err(self.unsupported(span, "this format string"));
            };
            let LitKind::ByteStr(ref bytes, _) = lit.node else {
                return Err(self.unsupported(span, "this format string"));
            };
            let items = self.expr(args[1], out)?;
            return self.format(bytes.as_byte_str(), items, span);
        }
        if known == Std::AssertFailed {
            // `assert_failed(kind, &left, &right, None or Some(message))`.
            let message = match self.thir[self.strip(args[3])].kind {
                ExprKind::Adt(ref option) => option.fields.first().map(|f| f.expr),
                _ => return Err(self.unsupported(span, "this assertion")),
            };
            let mut list = args[..3].to_vec();
            list.extend(message);
            let mut values = self.operands(&list, out)?;
            // The two values' `{:?}`, by their types (ADR 0060).
            for i in [1, 2] {
                let ty = self.thir[args[i]].ty;
                let value = std::mem::replace(&mut values[i], Expr::undefined());
                values[i] = self.debug_string(value, ty, span)?;
            }
            self.runtime.insert(Helper::AssertFailed);
            return Ok(Expr::call(Expr::var("$assertFailed"), values));
        }
        if matches!(
            known,
            Std::Swap | Std::Replace | Std::OptionTake | Std::OptionReplace | Std::MemTake
        ) {
            return self.swap_or_replace(known, args, discarded, span, out);
        }
        let mut values = self.operands(args, out)?.into_iter();
        if let Some(js) = self.vec_call(known, call, &mut values, boxed, out)? {
            return Ok(js);
        }
        if let Some(js) = self.option_call(known, call, &mut values, boxed, out)? {
            return Ok(js);
        }
        if let Some(js) = self.cell_call(known, call, &mut values, out)? {
            return Ok(js);
        }
        if let Some(js) = self.channel_call(known, call, &mut values, out)? {
            return Ok(js);
        }
        if let Some(js) = self.numeric_call(known, call, &mut values, out)? {
            return Ok(js);
        }
        if let Some(js) = self.iter_source_call(known, call, &mut values)? {
            return Ok(js);
        }
        if let Some(js) = self.string_call(known, call, &mut values)? {
            return Ok(js);
        }
        if let Some(js) = self.print_call(known, call, &mut values, out)? {
            return Ok(js);
        }
        let mut arg = || values.next().expect("rustc checked the arguments");
        Ok(match known {
            Std::Swap | Std::Replace | Std::OptionTake | Std::OptionReplace | Std::MemTake => {
                unreachable!("lowered from their places, above")
            }
            // An `Rc` is the JS reference itself: the garbage collector does
            // its counting, so a clone is the same object.
            Std::Same => arg(),
            Std::Pointee => self.through_refs(arg(), self.thir[args[0]].ty).0,
            Std::Last
            | Std::Cloned
            | Std::Fuse
            | Std::ArrayMethod(_)
            | Std::Enumerate
            | Std::Rev
            | Std::Skip
            | Std::Take
            | Std::Fold
            | Std::Sum
            | Std::CollectString
            | Std::Collect
            | Std::Position
            | Std::Extreme(_)
            | Std::Sort
            | Std::SortByKey => {
                unreachable!("handled above")
            }
            Std::PushStr | Std::AssignOperator(_) => unreachable!("handled above"),
            Std::VecMacro | Std::FmtNew | Std::AssertFailed => unreachable!("handled above"),
            Std::Map(_)
            | Std::Range(_)
            | Std::Stream(_)
            | Std::TypeName { .. }
            | Std::Comb(_)
            | Std::IterComb(_)
            | Std::Text(_)
            | Std::Number(_)
            | Std::FromElem
            | Std::Heap(_)
            | Std::DequeRemove
            | Std::Step(_)
            | Std::ToJson(_)
            | Std::FromJson => {
                unreachable!("handled above")
            }
            Std::Method(_)
            | Std::First
            | Std::SliceLast
            | Std::SliceGet
            | Std::ToVec
            | Std::SortBy
            | Std::IsEmpty
            | Std::VecNew
            | Std::Append
            | Std::Push
            | Std::Len
            | Std::Index
            | Std::Clear
            | Std::Retain => unreachable!("lowered by vec_call"),
            Std::Then
            | Std::ThenWith
            | Std::IsOk(_)
            | Std::UnwrapOk
            | Std::UnwrapErr
            | Std::ResultOk
            | Std::ResultOr
            | Std::IsSome
            | Std::IsNone
            | Std::Unwrap
            | Std::UnwrapOr
            | Std::OptionMap
            | Std::OptionIter
            | Std::OptionCloned => unreachable!("lowered by option_call"),
            Std::Channel(_) => unreachable!("lowered by channel_call"),
            Std::CellNew
            | Std::CellGet
            | Std::CellSet
            | Std::Borrow
            | Std::Lock
            | Std::Drop
            | Std::Forget
            | Std::AtomicLoad
            | Std::AtomicStore
            | Std::AtomicSwap
            | Std::AtomicFetch(_)
            | Std::AtomicFetchMax(_)
            | Std::AtomicCompareExchange
            | Std::LocalWith
            | Std::LocalBorrow => unreachable!("lowered by cell_call"),
            Std::ToBig
            | Std::TryFromInt { .. }
            | Std::FromDigit
            | Std::FromU32
            | Std::Cmp
            | Std::MaxOf(_)
            | Std::Operator(_)
            | Std::UnaryOperator(_)
            | Std::Reverse
            | Std::SizeOf
            | Std::AlignOf
            | Std::SizeOfVal => unreachable!("lowered by numeric_call"),
            Std::IterSource(_) => unreachable!("lowered by iter_source_call"),
            Std::Concat
            | Std::StripPrefix
            | Std::StripSuffix
            | Std::SplitOnce
            | Std::RsplitOnce
            | Std::Chars
            | Std::StringNew
            | Std::Trim
            | Std::AsciiCase { .. }
            | Std::AsciiEq
            | Std::ToString => unreachable!("lowered by string_call"),
            Std::Panic
            | Std::PanicFmt
            | Std::BeginPanic
            | Std::Print { .. }
            | Std::FmtStr
            | Std::FmtDisplay
            | Std::FmtDebug
            | Std::FmtRadix(_)
            | Std::FmtExp(_)
            | Std::FmtUsize => unreachable!("lowered by print_call"),
        })
    }

    /// What `From::from` or `Into::into` makes, and from what.
    fn converted(&self, def_id: DefId, args: ty::GenericArgsRef<'tcx>) -> Option<(Ty<'tcx>, Ty<'tcx>)> {
        let tr = self.tcx.trait_of_assoc(def_id)?;
        let mut types = args.types();
        let (first, second) = (types.next()?, types.next()?);
        if self.tcx.is_diagnostic_item(rustc_span::sym::From, tr) {
            Some((first, second))
        } else if self.tcx.is_diagnostic_item(rustc_span::sym::Into, tr) {
            Some((second, first))
        } else {
            None
        }
    }

    /// A JS call that says, in Rust, that it may throw (ADR 0035): one
    /// returning a `Result` runs in a `try`, `$try(() => f(x))`, and one
    /// returning a `Promise<Result<..>>` settles either way, `$settle(p)`.
    pub(super) fn catching(&mut self, def_id: DefId, value: Expr) -> Expr {
        match self.recognition().catching(def_id) {
            Catching::Result => {
                self.runtime.insert(Helper::Try);
                let span = value.span;
                let thunk = Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(value)).at(span)]);
                Expr::call(Expr::var("$try"), vec![thunk])
            }
            Catching::PromiseResult => {
                self.runtime.insert(Helper::Settle);
                Expr::call(Expr::var("$settle"), vec![value])
            }
            Catching::Direct => value,
        }
    }
}

/// `f(args)`, with a closure that only returns put in place, its parameters
/// the arguments: `((s) => s.value)(START)` is `START.value`. Only for
/// arguments that read the same however often they're read.
pub(super) fn apply(f: Expr, args: Vec<Expr>) -> Expr {
    if let js::ExprKind::Arrow(params, body) = &f.kind
        && let [
            js::Stmt {
                kind: StmtKind::Return(Some(value)),
                ..
            },
        ] = body.as_slice()
        && params.len() <= args.len()
        && args.iter().all(Expr::reads_same)
    {
        let names: Option<Vec<&str>> = params
            .iter()
            .map(|p| match p {
                js::Pattern::Name(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        let inlined = names.and_then(|names| {
            value.substitute(&|name: &str| names.iter().position(|n| *n == name).map(|i| args[i].clone()))
        });
        if let Some(inlined) = inlined {
            return inlined;
        }
    }
    Expr::call(f, args)
}
