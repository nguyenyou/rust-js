//! Calls to local functions, JavaScript bindings, closures and standard operations.

use super::bindings::{self, JsForm, is_binding, is_method, js_form, js_import};
use super::combinators::{IterSource, StepOp};
use super::drops::Drops;
use super::numbers::NumOp;
use super::recognition::{Catching, Std};
use super::representation::Num;
use super::text::TextOp;
use super::{FnCx, R, camel_case, global};
use crate::js;
use crate::js::{Expr, Op, Prop, Stmt, StmtKind, UnaryOp};
use crate::runtime::Helper;
use rustc_ast::{LitKind, Mutability};
use rustc_hir::{LangItem, find_attr};
use rustc_middle::thir::{ExprId, ExprKind, LocalVarId};
use rustc_middle::ty::{self, Ty, TypeVisitableExt};
use rustc_span::Span;
use rustc_span::def_id::DefId;
use std::collections::HashSet;

/// How an argument is given to a function that takes boxes (`call_with_boxes`).
enum ArgForm {
    /// As any argument is.
    Value,
    /// Its place, in a box, taken back out after the call.
    Boxed(ExprId),
    /// What's in a box, the variable's, given to a parameter that isn't one.
    Unboxed(LocalVarId),
}

/// What a call with boxes calls (`boxed_callee`).
enum Callee<'tcx> {
    Fn(DefId, ty::GenericArgsRef<'tcx>),
    /// A trait's method, in this dictionary of its impl's (ADR 0049).
    Dictionary(DefId, ty::GenericArgsRef<'tcx>, Expr),
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
        if let Some(callee) = self.boxed_callee(def_id, generic_args, args, span)? {
            return self.call_with_boxes(callee, args, discarded, span, out);
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
                return Ok(compared);
            }
            // `p < q` of numbers: the operator, as of `&i32`s.
            if let Some(Std::Operator(op)) = self.std_fn(fun)
                && let [left, right] =
                    <[Expr; 2]>::try_from(values).map_err(|_| self.unsupported(span, "comparing `&mut`s"))?
            {
                let ty = pointees.types().next().expect("a comparison has a type");
                return self.binary(op, left, right, None, ty, span);
            }
            return Err(self.unsupported(span, "comparing `&mut`s"));
        }
        if let Some(written) = self.write_call(def_id, generic_args, args, span, out)? {
            return Ok(written);
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
                if matches!(input.kind(), ty::Param(_)) && self.is_user_iterator(self.reveal(self.thir[arg].ty)) {
                    let taken = std::mem::replace(value, Expr::undefined());
                    *value = self.iter_source(taken, self.thir[arg].ty, span)?;
                }
            }
            let mut args = values;
            args.extend(self.evidence_args(def_id, generic_args, span)?);
            return Ok(Expr::call(callee.or_at(fun_span), args));
        }
        if is_binding(self.tcx, def_id) {
            // An `#[eii]` function is declared in an `extern` block too, but
            // it's Rust's, linked to its implementation, not JS's.
            if find_attr!(self.tcx, def_id, RustcEiiForeignItem) {
                return Err(self.unsupported(span, "externally implementable items, `#[eii]`,"));
            }
            match js_form(self.tcx, def_id) {
                JsForm::Jsx(tag) => return self.jsx(&tag, args, span, out),
                JsForm::Prop(name) => return self.jsx_prop(name.as_deref(), args, span, out),
                JsForm::Object(keys) => return self.object_binding(&keys, args, span, out),
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
            return Ok(self.catching(def_id, value));
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
            return Ok(Expr::call(callee, values));
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
                return Ok(call);
            }
        }
        // A `fmt::Result` is nothing in JS (ADR 0054), without `Result`'s methods.
        if args
            .first()
            .is_some_and(|&a| self.is_fmt_result(self.thir[a].ty.peel_refs()))
        {
            return Err(self.unsupported(span, "methods of a `fmt::Result`"));
        }
        let Some(known) = self.std_fn(fun) else {
            // Rust counts a string's UTF-8 bytes, and JS its UTF-16 units (ADR 0034).
            if let Some(string) = self
                .recognition()
                .unsupported_string(def_id, args.first().map(|&a| self.thir[a].ty))
            {
                let name = string.name;
                let indexing = string.indexing;
                let what = if indexing {
                    "indexing or slicing a string".to_string()
                } else {
                    format!("`{name}()` of a string")
                };
                let why = if string.suggest_is_empty {
                    "Rust counts its UTF-8 bytes, and JS its UTF-16 units; `is_empty()` works"
                } else {
                    "Rust counts its UTF-8 bytes, and JS its UTF-16 units"
                };
                return Err(self
                    .tcx
                    .dcx()
                    .span_err(span, format!("rust-js does not support {what}: {why}")));
            }
            let path = self.tcx.def_path_str(def_id);
            // A library's, that its manifest doesn't list (ADR 0100).
            if let Some(why) = self.krate.foreign.unlisted(def_id) {
                return Err(self.tcx.dcx().span_err(self.thir[fun].span, why));
            }
            return Err(self.unsupported(self.thir[fun].span, &format!("calling `{path}`")));
        };
        // One that takes a value with a destructor, or changes a place that
        // holds one, must keep or give back what it takes: these do. Another
        // might drop it, which JS wouldn't (ADR 0098). A value whose drops
        // rust-js can't follow, a `vec::IntoIter` of them say, might hold one.
        let holds_drops = |ty: Ty<'tcx>| self.drops(ty) != Drops::Nothing;
        let takes_drops = args.iter().any(|&a| match *self.thir[a].ty.kind() {
            ty::Ref(_, inner, Mutability::Mut) => holds_drops(inner),
            ty::Ref(..) => false,
            _ => holds_drops(self.thir[a].ty),
        });
        if takes_drops
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
            && !self.locals.item_calls.contains(&fun)
        {
            let path = self.tcx.def_path_str(def_id);
            return Err(self.unsupported(span, &format!("a `{cell}` from `{path}` used as a value")));
        }
        let boxed = self.option_of(output).is_some_and(|inner| self.boxed_payload(inner));
        // And one of a `()` or an `Option`, which would be `None` (ADR 0030).
        // `map` says so in its own words.
        if known != Std::OptionMap
            && self
                .option_of(output)
                .is_some_and(|inner| self.can_be_nullish(inner) && !self.boxed_payload(inner))
        {
            return Err(self.unsupported(span, &format!("values of type `{output}`")));
        }
        if boxed
            && !matches!(
                known,
                Std::Same
                    | Std::OptionMap
                    | Std::Method("pop")
                    | Std::First
                    | Std::SliceLast
                    | Std::ResultOk
                    | Std::ArrayMethod("find")
                    | Std::Extreme(_)
                    | Std::Step(StepOp::Next | StepOp::Peek)
            )
        {
            return Err(self.unsupported(span, "this call, for an `Option` of a generic type"));
        }
        if let Std::Map(op) = known {
            return self.map_call(op, args, generic_args, discarded, span, out);
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
        if matches!(known, Std::Swap | Std::Replace) {
            return self.swap_or_replace(known, args, discarded, span, out);
        }
        let mut values = self.operands(args, out)?.into_iter();
        let mut arg = || values.next().expect("rustc checked the arguments");
        let js_span = self.js_span(span);
        Ok(match known {
            Std::Swap | Std::Replace => unreachable!("lowered from their places, above"),
            // An `Rc` is the JS reference itself: the garbage collector does
            // its counting, so a clone is the same object.
            Std::Same => arg(),
            Std::Pointee => self.through_refs(arg(), self.thir[args[0]].ty).0,
            Std::ToBig => Expr::call(Expr::var("BigInt"), vec![arg()]),
            Std::TryFromInt { into } => {
                let target = if into {
                    generic_args.type_at(1)
                } else {
                    generic_args.type_at(0)
                };
                let num = self.num(target, span)?;
                let (lo, hi) = num.range();
                self.runtime.insert(Helper::TryFromInt);
                Expr::call(Expr::var("$tryFromInt"), vec![arg(), num.literal(lo), num.literal(hi)])
            }
            // A `Cell` or `RefCell` is `{ value }`, so everyone sharing it sees a change.
            Std::CellNew => Expr::object(vec![Prop::Field("value".into(), arg())]),
            Std::CellGet => self.copy_if_needed(Expr::member(arg(), "value"), generic_args.type_at(0)),
            Std::CellSet => {
                let (cell, value) = (arg(), arg());
                out.push(StmtKind::Assign(Expr::member(cell, "value"), value).at(js_span));
                Expr::undefined()
            }
            // A `Ref` or `RefMut` guard is what it guards: the object itself.
            Std::Borrow => Expr::member(arg(), "value"),
            // `mem::drop(x)` is `x`'s destructor, run now (ADR 0098).
            Std::Drop => {
                let ty = self.thir[args[0]].ty;
                let value = arg();
                let value = self.droppable(value, ty, out);
                self.drop_value(value, ty, span, out)?;
                Expr::undefined()
            }
            Std::Forget => {
                let value = arg();
                if value.has_effects() {
                    out.push(StmtKind::Expr(value).at(js_span));
                }
                Expr::undefined()
            }
            // An atomic's operation (ADR 0096) is the plain one on its `{ value }`:
            // JS runs a module on one thread, so every ordering holds. Each
            // ordering is evaluated, and not used.
            Std::AtomicLoad
            | Std::AtomicStore
            | Std::AtomicSwap
            | Std::AtomicFetch(_)
            | Std::AtomicFetchMax(_)
            | Std::AtomicCompareExchange => {
                let ty::Adt(_, atomic) = self.thir[args[0]].ty.peel_refs().kind() else {
                    return Err(self.unsupported(span, "this atomic"));
                };
                let item = atomic.type_at(0);
                let operands = match known {
                    Std::AtomicLoad => 0,
                    Std::AtomicCompareExchange => 2,
                    _ => 1,
                };
                let cell = arg();
                let cell = if operands > 0 && !cell.reads_same() {
                    self.spill("atomic", cell, out)
                } else {
                    cell
                };
                let given: Vec<Expr> = (0..operands)
                    .map(|_| arg())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|v| {
                        if v.reads_same() {
                            v
                        } else {
                            self.spill("operand", v, out)
                        }
                    })
                    .collect();
                for ordering in values.by_ref() {
                    if ordering.has_effects() {
                        out.push(StmtKind::Expr(ordering).at(js_span));
                    }
                }
                let slot = Expr::member(cell, "value");
                if known == Std::AtomicLoad {
                    return Ok(slot);
                }
                let [v, rest @ ..] = &given[..] else {
                    unreachable!("an atomic's operand");
                };
                if known == Std::AtomicStore {
                    out.push(StmtKind::Assign(slot, v.clone()).at(js_span));
                    return Ok(Expr::undefined());
                }
                let previous = self.spill("previous", slot.clone(), out);
                let next = match known {
                    Std::AtomicSwap => v.clone(),
                    Std::AtomicFetch(op) => self.binary(op, previous.clone(), v.clone(), None, item, span)?,
                    Std::AtomicFetchMax(max) => {
                        let op = if max { Op::Gt } else { Op::Lt };
                        Expr::cond(Expr::bin(op, previous.clone(), v.clone()), previous.clone(), v.clone())
                    }
                    _ => {
                        let done = self.spill("exchanged", Expr::bin(Op::Eq, previous.clone(), v.clone()), out);
                        out.push(
                            StmtKind::If(
                                done.clone(),
                                vec![StmtKind::Assign(slot, rest[0].clone()).at(js_span)],
                                None,
                            )
                            .at(js_span),
                        );
                        let result = |tag: &str| {
                            Expr::object(vec![
                                Prop::Field("TAG".into(), Expr::str(tag)),
                                Prop::Field("_0".into(), previous.clone()),
                            ])
                        };
                        return Ok(Expr::cond(done, result("Ok"), result("Err")));
                    }
                };
                out.push(StmtKind::Assign(slot, next).at(js_span));
                previous
            }
            Std::Concat => Expr::bin(Op::Add, arg(), arg()),
            Std::Method("pop") if boxed => {
                self.runtime.insert(Helper::Pop);
                Expr::call(Expr::var("$pop"), vec![arg()])
            }
            Std::Method(name) => {
                let this = arg();
                let rest: Vec<Expr> = (1..args.len()).map(|_| arg()).collect();
                // A pattern that may be empty, which Rust matches at each
                // char's boundary and JS between UTF-16 units (ADR 0063).
                let may_be_empty = matches!(name, "replaceAll" | "split")
                    && !generic_args.types().next().is_some_and(|p| p.is_char())
                    && !matches!(&rest[0].kind, js::ExprKind::Str(s) if !s.is_empty());
                if may_be_empty {
                    self.runtime.insert(Helper::EmptyPattern);
                    let helper = if name == "split" { "$split" } else { "$replace" };
                    return Ok(Expr::call(Expr::var(helper), [vec![this], rest].concat()));
                }
                Expr::call(Expr::member(this, name), rest)
            }
            Std::StripPrefix | Std::StripSuffix | Std::SplitOnce | Std::RsplitOnce => {
                let (helper, name) = match known {
                    Std::StripPrefix => (Helper::StripPrefix, "$stripPrefix"),
                    Std::StripSuffix => (Helper::StripSuffix, "$stripSuffix"),
                    Std::SplitOnce => (Helper::SplitOnce, "$splitOnce"),
                    _ => (Helper::RsplitOnce, "$rsplitOnce"),
                };
                self.runtime.insert(helper);
                Expr::call(Expr::var(name), vec![arg(), arg()])
            }
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
            Std::Chars => Expr::call(Expr::member(Expr::var("Array"), "from"), vec![arg()]),
            Std::First if boxed => {
                let items = arg();
                self.some_at(items, Expr::int(0))
            }
            Std::SliceLast if boxed => {
                let mut items = arg();
                if items.has_effects() {
                    items = self.spill("items", items, out);
                }
                let last = Expr::bin(Op::Sub, Expr::member(items.clone(), "length"), Expr::int(1));
                self.some_at(items, last)
            }
            Std::First => Expr::index(arg(), Expr::int(0)),
            Std::FromDigit => {
                self.runtime.insert(Helper::FromDigit);
                Expr::call(Expr::var("$fromDigit"), vec![arg(), arg()])
            }
            Std::FromU32 => {
                self.runtime.insert(Helper::FromU32);
                Expr::call(Expr::var("$fromU32"), vec![arg()])
            }
            Std::SliceGet => {
                let items = arg();
                Expr::index(items, arg())
            }
            Std::SliceLast => Expr::call(Expr::member(arg(), "at"), vec![Expr::int(-1)]),
            // A copy, unless it's an array just written: `vec![3, 4].into()`.
            Std::ToVec => match arg() {
                items if matches!(items.kind, js::ExprKind::Array(_)) => items,
                items => Expr::call(Expr::member(items, "slice"), vec![]),
            },
            Std::SortBy => {
                let (v, compare) = (arg(), arg());
                Expr::call(Expr::member(v, "sort"), vec![compare])
            }
            Std::Cmp => {
                self.runtime.insert(Helper::Cmp);
                Expr::call(Expr::var("$cmp"), vec![arg(), arg()])
            }
            Std::MaxOf(max) => {
                let num = Num::of(self.thir[args[0]].ty.peel_refs());
                let callee = if num.is_some_and(Num::float) {
                    self.runtime.insert(if max { Helper::F64Max } else { Helper::F64Min });
                    Expr::var(if max { "$f64Max" } else { "$f64Min" })
                } else if num.is_some_and(Num::big) {
                    // `Math.max` takes numbers only.
                    self.runtime.insert(Helper::BigMinMax);
                    Expr::var(if max { "$bigMax" } else { "$bigMin" })
                } else {
                    Expr::member(Expr::var("Math"), if max { "max" } else { "min" })
                };
                Expr::call(callee, vec![arg(), arg()])
            }
            // An `Ordering` is -1, 0 or 1: `Equal` is the one that's falsy.
            Std::Operator(op) => {
                let ty = generic_args
                    .types()
                    .next()
                    .expect("an operator's trait has a type")
                    .peel_refs();
                let (l, r) = (arg(), arg());
                // `a << &n` of an `i64` `n`: its type, the trait's `Rhs`.
                let r = match generic_args.types().nth(1) {
                    Some(rhs) => super::numbers::shift_amount_of(op, r, ty, rhs),
                    None => r,
                };
                self.binary(op, l, r, None, ty, span)?
            }
            Std::UnaryOperator(op) => {
                let ty = generic_args
                    .types()
                    .next()
                    .expect("an operator's trait has a type")
                    .peel_refs();
                let a = arg();
                self.unary(op, a, ty, span)?
            }
            Std::LocalWith => {
                let (key, f) = (arg(), arg());
                apply(f, vec![key])
            }
            Std::LocalBorrow => {
                let (key, f) = (arg(), arg());
                apply(f, vec![Expr::member(key, "value")])
            }
            Std::Then => Expr::bin(Op::Or, arg(), arg()),
            Std::ThenWith => {
                let (first, next) = (arg(), arg());
                // `then_with(|| a.cmp(b))` is `first || $cmp(a, b)`: the closure's body in place.
                let then = match next.kind {
                    js::ExprKind::Arrow(ref params, ref body) if params.is_empty() => match body.as_slice() {
                        [
                            js::Stmt {
                                kind: StmtKind::Return(Some(value)),
                                ..
                            },
                        ] => value.clone(),
                        _ => Expr::call(next.clone(), vec![]),
                    },
                    _ => Expr::call(next.clone(), vec![]),
                };
                Expr::bin(Op::Or, first, then)
            }
            Std::Reverse => Expr::unary(UnaryOp::Neg, arg()),
            Std::IsOk(ok) => Expr::bin(
                if ok { Op::Eq } else { Op::Ne },
                Expr::member(arg(), "TAG"),
                Expr::str("Ok"),
            ),
            Std::UnwrapOk => {
                let mut list: Vec<Expr> = (0..args.len()).map(|_| arg()).collect();
                // An `Ok(x)` just made, as a `to_value` that can't fail is: `x`.
                if let js::ExprKind::Object(props) = &list[0].kind
                    && let [Prop::Field(tag, name), Prop::Field(field, value)] = props.as_slice()
                    && (tag.as_str(), field.as_str()) == ("TAG", "_0")
                    && matches!(&name.kind, js::ExprKind::Str(s) if s == "Ok")
                    && list[1..].iter().all(|e| !e.has_effects())
                {
                    return Ok(value.clone());
                }
                // A parse error is its message (ADR 0063), which `$debug` would
                // show as a string: its own `Debug`, `ParseIntError { kind: .. }`.
                if let Some(error) = generic_args.types().nth(1)
                    && self.is_parse_error(error)
                {
                    let e = self.fresh("e");
                    let shown = self.debug_string(Expr::var(&e), error, span)?;
                    if list.len() == 1 {
                        list.push(Expr::undefined());
                    }
                    list.push(Expr::arrow(
                        vec![e.into()],
                        vec![StmtKind::Return(Some(shown)).at(js::Span::NONE)],
                    ));
                }
                self.runtime.insert(Helper::UnwrapOk);
                Expr::call(Expr::var("$unwrapOk"), list)
            }
            Std::UnwrapErr => {
                self.runtime.insert(Helper::UnwrapErr);
                let list = (0..args.len()).map(|_| arg()).collect();
                Expr::call(Expr::var("$unwrapErr"), list)
            }
            // `r.TAG === "Ok" ? r._0 : d`, with `r` computed once, and `d` too,
            // before the test, as Rust does.
            Std::ResultOk | Std::ResultOr => {
                let mut result = arg();
                if result.has_effects() {
                    result = self.spill("result", result, out);
                }
                let otherwise = match known {
                    Std::ResultOr => {
                        let d = arg();
                        if d.has_effects() {
                            self.spill("fallback", d, out)
                        } else {
                            d
                        }
                    }
                    _ => Expr::undefined(),
                };
                let ok = Expr::bin(Op::Eq, Expr::member(result.clone(), "TAG"), Expr::str("Ok"));
                let value = Expr::member(result, "_0");
                let value = if boxed { self.some(value) } else { value };
                Expr::cond(ok, value, otherwise)
            }
            Std::PushStr | Std::AssignOperator(_) => unreachable!("handled above"),
            Std::IsSome => Expr::bin(Op::LooseNe, arg(), Expr::null()),
            Std::IsNone => Expr::bin(Op::LooseEq, arg(), Expr::null()),
            Std::Unwrap => {
                self.runtime.insert(Helper::Unwrap);
                // `expect` has a message too.
                let list = (0..args.len()).map(|_| arg()).collect();
                let unwrapped = Expr::call(Expr::var("$unwrap"), list);
                if self.boxed_payload(generic_args.type_at(0)) {
                    self.some_value(unwrapped)
                } else {
                    unwrapped
                }
            }
            // `??` skips its right side when it isn't needed, and Rust
            // evaluates it either way: one with effects runs first, in order.
            Std::UnwrapOr => {
                let (mut option, mut default) = (arg(), arg());
                if default.has_effects() {
                    if option.has_effects() {
                        option = self.spill("option", option, out);
                    }
                    default = self.spill("fallback", default, out);
                }
                // Of a generic `T` (ADR 0051): `$someValue(o ?? $some(d))`.
                if self.boxed_payload(generic_args.type_at(0)) {
                    let default = self.some(default);
                    self.some_value(Expr::bin(Op::Coalesce, option, default))
                } else {
                    Expr::bin(Op::Coalesce, option, default)
                }
            }
            // `o.map(|x| value)` is `o != null ? value : undefined`, with the
            // option for `x`, read once: `const h = half(n); h != null ? h + 1 : undefined`.
            // A function, or a closure of statements, is called with it.
            Std::OptionMap => {
                let (option, f) = (arg(), arg());
                let mapped = generic_args.type_at(1);
                if self.can_be_nullish(mapped) && !self.boxed_payload(mapped) {
                    let what = format!("`map` to a `{mapped}`, whose `Some` would be `None` in JS");
                    return Err(self.unsupported(span, &what));
                }
                // `|_| 7` has no parameter left (ADR 0038): `Some(None)`.
                let param = match &f.kind {
                    js::ExprKind::Arrow(params, _) if params.len() <= 1 => Some(params.first().cloned()),
                    _ => None,
                };
                let body = match &f.kind {
                    js::ExprKind::Arrow(_, body) => match body.as_slice() {
                        [
                            js::Stmt {
                                kind: StmtKind::Return(Some(value)),
                                ..
                            },
                        ] => Some(value.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let base = match &param {
                    Some(Some(js::Pattern::Name(name))) => name.clone(),
                    _ => "option".to_string(),
                };
                let option = match option.kind {
                    js::ExprKind::Var(_) => option,
                    _ => self.spill(&base, option, out),
                };
                // Of a generic `T`, the closure gets what's inside (ADR 0051).
                let present = Expr::bin(Op::LooseNe, option.clone(), Expr::null());
                let option = if self.boxed_payload(generic_args.type_at(0)) {
                    self.some_value(option)
                } else {
                    option
                };
                let value = param.zip(body).and_then(|(param, body)| {
                    let with = |name: &str| match &param {
                        None => None,
                        Some(js::Pattern::Name(p)) => (p == name).then(|| option.clone()),
                        Some(js::Pattern::Array(items)) => items
                            .iter()
                            .position(|item| item.as_deref() == Some(name))
                            .map(|i| Expr::index(option.clone(), Expr::int(i as i128))),
                        Some(js::Pattern::Object(fields)) => fields
                            .iter()
                            .find(|(_, var)| var == name)
                            .map(|(field, _)| Expr::member(option.clone(), field.clone())),
                    };
                    body.substitute(&with)
                });
                let value = match value {
                    Some(value) => value,
                    // Not `((h) => { .. })(h)`: the closure gets a name first.
                    None if matches!(f.kind, js::ExprKind::Arrow(..)) => {
                        let f = self.spill("map", f, out);
                        Expr::call(f, vec![option.clone()])
                    }
                    None => Expr::call(f, vec![option.clone()]),
                };
                let value = if self.boxed_payload(mapped) {
                    self.some(value)
                } else {
                    value
                };
                Expr::cond(present, value, Expr::undefined())
            }
            Std::StringNew => Expr::str(""),
            Std::Trim => Expr::call(Expr::member(arg(), "trim"), vec![]),
            Std::IsEmpty => Expr::bin(Op::Eq, Expr::member(arg(), "length"), Expr::num(0)),
            Std::VecNew => Expr::array(vec![]),
            Std::OptionIter => {
                let item = self
                    .option_of(self.thir[args[0]].ty.peel_refs())
                    .expect("an `Option` has a `T`");
                let option = arg();
                self.option_items(option, item, out)
            }
            Std::IterSource(IterSource::Once) => Expr::array(vec![arg()]),
            Std::IterSource(IterSource::Empty) => Expr::array(vec![]),
            Std::IterSource(IterSource::Repeat) => {
                let value = arg();
                let item = generic_args.type_at(0);
                let mut list = vec![value];
                if self.needs_clone(item) {
                    list.push(self.clone_fn("value", item, span)?);
                }
                self.runtime.insert(Helper::Repeating);
                Expr::call(Expr::var("$repeating"), list)
            }
            Std::IterSource(IterSource::RepeatWith) => {
                self.runtime.insert(Helper::RepeatingWith);
                Expr::call(Expr::var("$repeatingWith"), vec![arg()])
            }
            // Their closures' `Option`s: a generic `Some` is boxed (ADR 0051).
            Std::IterSource(source @ (IterSource::Successors | IterSource::FromFn)) => {
                let item = generic_args.type_at(0);
                let (helper, name, what) = match source {
                    IterSource::Successors => (Helper::Successors, "$successors", "successors"),
                    _ => (Helper::FromFn, "$fromFn", "from_fn"),
                };
                let boxed = self.boxed_payload(item);
                if self.can_be_nullish(item) && !boxed {
                    let what = format!("`{what}` of a `{item}`, whose `Some` would be `None` in JS");
                    return Err(self.unsupported(span, &what));
                }
                let mut list: Vec<Expr> = (0..args.len()).map(|_| arg()).collect();
                if boxed {
                    list.push(Expr::bool(true));
                }
                self.runtime.insert(helper);
                Expr::call(Expr::var(name), list)
            }
            Std::VecMacro | Std::FmtNew | Std::AssertFailed => unreachable!("handled above"),
            Std::Panic | Std::PanicFmt => {
                out.push(StmtKind::Throw(Expr::new_(Expr::var("Error"), vec![arg()])).at(js_span));
                Expr::undefined()
            }
            // The size rustc works out for the wasm32 target, which rust-js
            // checks programs for, as a `const` of it has (ADR 0090). A
            // generic function is one JS function for every type, so a type
            // parameter's has no one answer.
            Std::SizeOf | Std::AlignOf | Std::SizeOfVal => {
                let name = match known {
                    Std::SizeOf => "size_of",
                    Std::AlignOf => "align_of",
                    _ => "size_of_val",
                };
                let of = generic_args.types().next().expect("a size's type argument");
                if of.has_param() {
                    return Err(self.unsupported(span, &format!("`{name}` of a type parameter")));
                }
                if !of.is_sized(self.tcx, self.typing_env) {
                    return Err(self.unsupported(span, &format!("`{name}` of a value without one size")));
                }
                // Of a type without parameters, as codegen asks: 1.98 finds an
                // `async fn`'s future too generic to lay out otherwise (ADR 0109).
                let layout = self
                    .tcx
                    .layout_of(ty::TypingEnv::fully_monomorphized().as_query_input(of))
                    .map_err(|_| self.unsupported(span, &format!("`{name}` of this type")))?;
                // What's measured still runs, if it does anything.
                if matches!(known, Std::SizeOfVal) {
                    let measured = arg();
                    if measured.has_effects() {
                        out.push(StmtKind::Expr(measured).at(js_span));
                    }
                }
                let bytes = if matches!(known, Std::AlignOf) {
                    layout.align.abi.bytes()
                } else {
                    layout.size.bytes()
                };
                Expr::int(bytes as i128)
            }
            // A `&str` or a `String` is the panic's message, as Rust's hook
            // shows it; another payload, `panic!(5)`, has none.
            Std::BeginPanic => {
                let payload = generic_args.types().next().expect("`begin_panic` has a type argument");
                let text = matches!(payload.kind(), ty::Ref(_, inner, _) if inner.is_str())
                    || self.is_lang_adt(payload, LangItem::String);
                if !text {
                    return Err(self.unsupported(span, "a panic whose payload isn't text"));
                }
                out.push(StmtKind::Throw(Expr::new_(Expr::var("Error"), vec![arg()])).at(js_span));
                Expr::undefined()
            }
            // A whole line is `console.log`'s, which ends it; text that may
            // not end one is written as it is (ADR 0087).
            Std::Print { error } => match without_newline(arg()) {
                Ok(line) => Expr::call(
                    Expr::member(Expr::var("console"), if error { "error" } else { "log" }),
                    vec![line],
                ),
                Err(text) => {
                    self.runtime.insert(Helper::Print);
                    Expr::call(Expr::var(if error { "$eprint" } else { "$print" }), vec![text])
                }
            },
            Std::FmtStr => arg(),
            Std::FmtDisplay => {
                let ty = generic_args.types().next().expect("`new_display` has a type argument");
                self.display_string(arg(), ty, span)?
            }
            Std::FmtDebug => {
                let ty = generic_args.types().next().expect("`new_debug` has a type argument");
                self.debug_string(arg(), ty, span)?
            }
            // Only in a `format_args!` it recognizes whole (ADR 0058).
            Std::FmtRadix(_) | Std::FmtExp(_) | Std::FmtUsize => {
                return Err(self.unsupported(span, "`{:x}` and the like here"));
            }
            Std::Map(_)
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
            // `Some(&x)` is `x`, and its clone is `x`'s.
            Std::OptionCloned => {
                let item = generic_args.types().next().expect("`Option<T>` has a `T`");
                let value = arg();
                let ty = ty::Ty::new_adt(
                    self.tcx,
                    self.tcx.adt_def(self.tcx.require_lang_item(LangItem::Option, span)),
                    self.tcx.mk_args(&[item.into()]),
                );
                self.clone_value(value, ty, span, out)?
            }
            Std::Push => {
                let (v, x) = (arg(), arg());
                Expr::call(Expr::member(v, "push"), vec![x])
            }
            // `count()` of a JS iterator (ADR 0055) takes all of it.
            Std::Len if self.is_lazy_iter(self.thir[args[0]].ty) => {
                let items = self.iter_source(arg(), self.thir[args[0]].ty, span)?;
                Expr::member(Expr::call(Expr::member(items, "toArray"), vec![]), "length")
            }
            Std::Len => Expr::member(arg(), "length"),
            Std::Index => {
                self.runtime.insert(Helper::Index);
                Expr::call(Expr::var("$index"), vec![arg(), arg()])
            }
            Std::Clear => {
                out.push(StmtKind::Assign(Expr::member(arg(), "length"), Expr::num(0)).at(js_span));
                Expr::undefined()
            }
            Std::Retain => {
                self.runtime.insert(Helper::Retain);
                let (v, keep) = (arg(), arg());
                Expr::call(Expr::var("$retain"), vec![v, keep])
            }
            Std::ToString => self.display_string(arg(), generic_args.type_at(0), span)?,
        })
    }

    /// `mem::swap(&mut a, &mut b)`: `const t = a; a = b; b = t;`, and
    /// `mem::replace(&mut a, v)`: `const old = a; a = v;`, and `old`. While
    /// the call has a place's `&mut`, nothing else can use the place, so
    /// writing each in turn is exact. Neither drops what it moves out: it's
    /// the other place's now, or returned (ADR 0098), when it's used.
    fn swap_or_replace(
        &mut self,
        known: Std,
        args: &[ExprId],
        discarded: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let js_span = self.js_span(span);
        let a = self.mut_place(args[0], span)?;
        let (b, b_place) = match known {
            Std::Swap => {
                let b = self.mut_place(args[1], span)?;
                (b.clone(), Some(b))
            }
            _ => (self.expr(args[1], out)?, None),
        };
        if b_place.is_none() && discarded {
            out.push(StmtKind::Assign(a, b).at(js_span));
            return Ok(Expr::undefined());
        }
        let old = self.spill(if b_place.is_some() { "t" } else { "old" }, a.clone(), out);
        out.push(StmtKind::Assign(a, b).at(js_span));
        match b_place {
            Some(b) => {
                out.push(StmtKind::Assign(b, old).at(js_span));
                Ok(Expr::undefined())
            }
            None => Ok(old),
        }
    }

    /// `Instance::try_resolve` of `def_id` with `args`, normalized first, as
    /// it requires, or `None` where they can't be here: an associated type
    /// only a caller knows isn't one to resolve with (ADR 0106).
    pub(super) fn resolve_instance(
        &self,
        def_id: DefId,
        args: ty::GenericArgsRef<'tcx>,
    ) -> Result<Option<ty::Instance<'tcx>>, rustc_span::ErrorGuaranteed> {
        let Ok(args) = self
            .tcx
            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(args))
        else {
            return Ok(None);
        };
        ty::Instance::try_resolve(self.tcx, self.typing_env, def_id, args)
    }

    /// The crate's own impl method a trait method call resolves to, if it
    /// does: in a copied default, `Self` is the impl's type (ADR 0049).
    pub(super) fn impl_method(
        &self,
        id: DefId,
        generic_args: ty::GenericArgsRef<'tcx>,
    ) -> R<Option<(DefId, ty::GenericArgsRef<'tcx>)>> {
        let generic_args = match self.self_args {
            Some(args) => ty::EarlyBinder::bind(self.tcx, generic_args)
                .instantiate(self.tcx, args)
                .skip_normalization(),
            None => generic_args,
        };
        Ok(self
            .resolve_instance(id, generic_args)?
            .filter(|instance| {
                self.is_rust_fn(instance.def_id()) && self.tcx.trait_of_assoc(instance.def_id()).is_none()
            })
            .map(|instance| (instance.def_id(), instance.args)))
    }

    /// What a `&mut` argument points at, to read and write: `p` of `&mut p`,
    /// or a box's `value` (ADR 0074). A place with an item in it isn't one:
    /// its index would be evaluated at each use.
    fn mut_place(&self, arg: ExprId, span: Span) -> R<Expr> {
        if let Some(place) = self.mut_borrowed(arg) {
            if self.element(place).is_none() {
                return self.assignee(place);
            }
        } else if let ExprKind::VarRef { id } = self.thir[self.strip(arg)].kind
            && self.locals.boxes.contains(&id)
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
    fn pointee_value(&mut self, a: ExprId, span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
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
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } if !self.locals.boxes.contains(&id) => {
                self.place(e)
                    .map(|(place, _)| place)
                    .ok_or_else(|| self.unsupported(span, "comparing this `&mut`"))
            }
            _ if self.is_cell_value(e) => Ok(Expr::member(self.expr(e, out)?, "value")),
            _ => Err(self.unsupported(span, "comparing this `&mut`")),
        }
    }

    /// A `&mut` to a value JS can't change in place that a std call's result,
    /// or the items of the iterator it is, holds, that neither its arguments
    /// nor its type's parameters did: one the call made, `get_mut`'s or
    /// `iter_mut`'s, which is the item itself (ADR 0099).
    fn makes_items(
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
        self.locals.item_calls.insert(fun);
        true
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

    /// `ty`, of a function's own generics, in a call of it, `generic_args`.
    fn instantiated(&self, ty: Ty<'tcx>, generic_args: ty::GenericArgsRef<'tcx>) -> Ty<'tcx> {
        let ty = ty::EarlyBinder::bind(self.tcx, ty)
            .instantiate(self.tcx, generic_args)
            .skip_normalization();
        self.tcx
            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(ty))
            .unwrap_or(ty)
    }

    /// A `&mut` to one of `def_id`'s type parameters that's an object in a
    /// call of it, `generic_args`, anywhere but a parameter or its result: in
    /// a field of the crate's own type, an `Option` or a `Vec`, or a closure's
    /// parameters. Generic code has a cell there (ADR 0099), and the caller
    /// the object; a parameter is given a box, and the result's taken out.
    fn nested_mut_object(&self, def_id: DefId, generic_args: ty::GenericArgsRef<'tcx>) -> Option<Ty<'tcx>> {
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
                    todo.extend(
                        adt.all_fields()
                            .map(|field| field.ty(self.tcx, args).skip_normalization()),
                    );
                }
            }
        }
        None
    }

    /// Can what `fn_id` returns hold the borrow its parameter `i` is given: does
    /// its return type name a lifetime that parameter's type does (ADR 0099)?
    fn result_borrows(&self, fn_id: DefId, i: usize) -> bool {
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
    fn arg_form(&self, fn_id: DefId, i: usize, arg: ExprId) -> ArgForm {
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
            && self.locals.boxes.contains(&id)
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
    fn boxed_callee(
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
                    let generic_args = match self.self_args {
                        Some(args) => ty::EarlyBinder::bind(self.tcx, generic_args)
                            .instantiate(self.tcx, args)
                            .skip_normalization(),
                        None => generic_args,
                    };
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
    fn call_with_boxes(
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
                    if inputs.get(i).is_some_and(|input| matches!(input.kind(), ty::Param(_)))
                        && self.is_user_iterator(self.reveal(self.thir[arg].ty))
                    {
                        value = self.iter_source(value, self.thir[arg].ty, span)?;
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

    /// A std function taken as a value, `str::trim` in `.map(str::trim)`:
    /// an arrow of one parameter, doing what a call does. `None` for one
    /// that isn't one of these.
    pub(super) fn std_fn_value(&mut self, known: Std, ty: Ty<'tcx>, span: Span) -> R<Option<Expr>> {
        let ty::FnDef(def_id, args) = *ty.kind() else {
            return Ok(None);
        };
        let sig = self
            .tcx
            .fn_sig(def_id)
            .instantiate(self.tcx, args)
            .skip_normalization()
            .skip_binder();
        let [input] = sig.inputs() else {
            return Ok(None);
        };
        let input = input.peel_refs();
        let name = if input.is_char() {
            "c"
        } else if self.is_string_like(input) {
            "s"
        } else if Num::of(input).is_some() {
            "n"
        } else {
            "x"
        };
        let x = Expr::var(name);
        let body = match known {
            Std::Trim => Expr::call(Expr::member(x, "trim"), vec![]),
            Std::Method(method) => Expr::call(Expr::member(x, method), vec![]),
            Std::Same => x,
            Std::ToBig => Expr::call(Expr::var("BigInt"), vec![x]),
            Std::ToString => self.display_string(x, input, span)?,
            Std::Text(TextOp::Is(regex)) => Expr::call(Expr::member(Expr::regex(regex), "test"), vec![x]),
            // An `f32`'s rounded to one, as its call is, but for those that are
            // exact already (ADR 0122).
            Std::Number(NumOp::Math(function)) => {
                let value = Expr::call(Expr::member(Expr::var("Math"), function), vec![x]);
                match Num::of(input) {
                    Some(num @ Num::F32) if !matches!(function, "floor" | "ceil" | "trunc" | "abs") => num.wrap(value),
                    _ => value,
                }
            }
            // `i32::abs`, wrapped as its call is: `i32::MIN`'s is itself (ADR 0125).
            Std::Number(NumOp::Abs) if let Some(num) = Num::of(input).filter(|n| !n.big()) => {
                num.wrap(Expr::call(Expr::member(Expr::var("Math"), "abs"), vec![x]))
            }
            _ => return Ok(None),
        };
        let js_span = self.js_span(span);
        Ok(Some(Expr::arrow(
            vec![name.into()],
            vec![StmtKind::Return(Some(body)).at(js_span)],
        )))
    }

    /// `Into::<U>::into` of a `T` as `<U as From<T>>::from`, and
    /// `TryInto` as `TryFrom`, if that's a hand-written impl.
    /// The function a call runs: the crate's impl a trait method resolves to,
    /// or the `From` an `Into` does, else the one named.
    fn callee(&self, def_id: DefId, args: ty::GenericArgsRef<'tcx>) -> (DefId, ty::GenericArgsRef<'tcx>) {
        self.impl_method(def_id, args)
            .ok()
            .flatten()
            .or_else(|| self.resolve_into(def_id, args))
            .unwrap_or((def_id, args))
    }

    fn resolve_into(&self, def_id: DefId, args: ty::GenericArgsRef<'tcx>) -> Option<(DefId, ty::GenericArgsRef<'tcx>)> {
        let (method, args, implementation) = self.recognition().resolve_into(def_id, args)?;
        self.is_rust_fn(implementation).then_some((method, args))
    }

    /// `Some` of `items[index]`, or `None` if there's none (ADR 0051).
    pub(super) fn some_at(&mut self, items: Expr, index: Expr) -> Expr {
        self.runtime.insert(Helper::SomeAt);
        Expr::call(Expr::var("$someAt"), vec![items, index])
    }

    /// A function or its type's method object, imported by name when it lives
    /// in another module. The linker resolves collisions after lowering.
    /// Is `def_id` a Rust function rust-js compiled: the crate's own, or one a
    /// library exports (ADR 0100)?
    pub(super) fn is_rust_fn(&self, def_id: DefId) -> bool {
        self.krate.fns.contains_key(&def_id) || self.krate.foreign.item(def_id).is_some()
    }

    pub(super) fn fn_ref(&self, def_id: DefId) -> Expr {
        // A library's, imported by the name it chose (ADR 0100).
        if let Some(item) = self.krate.foreign.item(def_id) {
            let export = (item.from.clone(), item.export.clone());
            self.dependencies
                .borrow_mut()
                .package_uses
                .insert((self.module, export.clone()));
            let reference = Expr::var(&self.krate.imports[&export]);
            return match &item.member {
                Some(method) => Expr::member(reference, method.clone()),
                None => reference,
            };
        }
        self.dependencies.borrow_mut().uses.push((self.item, def_id));
        let target = &self.krate.fns[&def_id];
        if target.module != self.module {
            self.dependencies.borrow_mut().references.insert((self.module, def_id));
        }
        let export = target.owner.as_ref().unwrap_or(&target.name);
        let reference = if target.module != self.module {
            Expr {
                kind: js::ExprKind::Symbol(super::module_symbol(target.module, export)),
                span: js::Span::NONE,
            }
        } else {
            Expr::var(export)
        };
        if target.owner.is_some() {
            Expr::member(reference, target.name.clone())
        } else {
            reference
        }
    }

    /// A binding as a value, `.map(encode)` (ADR 0039): an arrow of its own
    /// parameters, calling it as a call would, so JS gives it no more than
    /// Rust does. `[..].map(parseInt)` would give `parseInt` each index too.
    pub(super) fn binding_value(&mut self, def_id: DefId, args: ty::GenericArgsRef<'tcx>, span: Span) -> R<Expr> {
        let inputs = self
            .tcx
            .fn_sig(def_id)
            .instantiate(self.tcx, args)
            .skip_normalization()
            .skip_binder()
            .inputs()
            .to_vec();
        let idents = self.tcx.fn_arg_idents(def_id);
        // Named as the binding names them, and `this` after its type:
        // `(signal) => signal.aborted`.
        let params: Vec<String> = inputs
            .iter()
            .enumerate()
            .map(|(i, input)| {
                let name = match idents.get(i).copied().flatten().map(|ident| ident.name.to_string()) {
                    Some(name) if name != "this" && !name.starts_with('_') => camel_case(&name),
                    _ => match input.peel_refs().kind() {
                        ty::Adt(adt, _) => super::lower_first(self.tcx.item_name(adt.did()).as_str()),
                        _ => format!("arg{i}"),
                    },
                };
                self.fresh(&name)
            })
            .collect();
        let mut values: Vec<Expr> = params.iter().map(|name| Expr::var(name)).collect();
        let this = is_method(self.tcx, def_id).then(|| values.remove(0));
        let value = match (js_form(self.tcx, def_id), this) {
            (JsForm::Call(name), Some(this)) if !name.contains('#') => Expr::call(Expr::member(this, name), values),
            (JsForm::Call(name), None) => Expr::call(self.js_ref(&name), values),
            (JsForm::New(name), None) => Expr::new_(self.js_ref(&name), values),
            (JsForm::Get(name), Some(this)) if values.is_empty() && !name.contains('#') => Expr::member(this, name),
            (JsForm::This, Some(this)) if values.is_empty() => this,
            (JsForm::CallThis, Some(this)) => Expr::call(this, values),
            _ => {
                let what = format!("`{}` as a value", self.tcx.def_path_str(def_id));
                return Err(self.unsupported(span, &what));
            }
        };
        let body = self.catching(def_id, value);
        Ok(Expr::arrow(
            params.into_iter().map(Into::into).collect(),
            vec![StmtKind::Return(Some(body)).at(self.js_span(span))],
        ))
    }

    /// A binding that's a JSX component, `<Toaster />` of `sonner#Toaster`:
    /// what it's imported as, or `None` for a Rust function's.
    pub(super) fn binding_component(&self, component: ExprId) -> Option<Expr> {
        let mut at = self.strip(component);
        while let ExprKind::Borrow { arg, .. } = self.thir[at].kind {
            at = self.strip(arg);
        }
        let (ExprKind::ZstLiteral { .. }, &ty::FnDef(def_id, _)) = (&self.thir[at].kind, self.thir[at].ty.kind())
        else {
            return None;
        };
        match js_form(self.tcx, def_id) {
            JsForm::Call(name) if is_binding(self.tcx, def_id) && !is_method(self.tcx, def_id) => {
                Some(self.js_ref(&name))
            }
            _ => None,
        }
    }

    /// A JS global (`console.log`) or an explicit package import
    /// (`node:path#posix.join` is `posix.join`, ADR 0028).
    pub(super) fn js_ref(&self, path: &str) -> Expr {
        match js_import(path) {
            Some((export, rest)) => {
                self.dependencies
                    .borrow_mut()
                    .package_uses
                    .insert((self.module, export.clone()));
                global(&format!("{}{rest}", self.krate.imports[&export]))
            }
            None => global(path),
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

/// `"a\n"` or `` `a ${x}\n` ``: the line, `"a"`, without the newline
/// `println!` ends it with. Anything else is given back.
fn without_newline(text: Expr) -> Result<Expr, Expr> {
    match &text.kind {
        js::ExprKind::Str(s) if s.ends_with('\n') => Ok(Expr::str(&s[..s.len() - 1])),
        js::ExprKind::Template(texts, values) if texts.last().is_some_and(|last| last.ends_with('\n')) => {
            let mut texts = texts.clone();
            texts.last_mut().expect("a template has a last text").pop();
            Ok(Expr::template(texts, values.clone()))
        }
        _ => Err(text),
    }
}
