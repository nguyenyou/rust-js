//! Calls to local functions, JavaScript bindings, closures and standard operations.

use super::bindings::{JsForm, is_binding, is_method, js_form, js_import};
use super::representation::Num;
use super::stdlib::Std;
use super::{FnCx, R, global};
use crate::js;
use crate::js::{Expr, Op, Prop, Stmt, StmtKind, UnaryOp};
use crate::runtime::Helper;
use rustc_ast::LitKind;
use rustc_hir::LangItem;
use rustc_middle::thir::{ExprId, ExprKind};
use rustc_middle::ty;
use rustc_span::def_id::DefId;
use rustc_span::{Span, sym};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// A call to one of our functions (`f`, or `alias.f` in another module),
    /// to JS (ADR 0021), or to one of the std functions rust-js knows (ADR 0023).
    pub(super) fn call(&mut self, fun: ExprId, args: &[ExprId], span: Span, out: &mut Vec<Stmt>) -> R<Expr> {
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
        if self.krate.fns.contains_key(&def_id) && self.tcx.trait_of_assoc(def_id).is_none() {
            let callee = self.fn_ref(def_id);
            let mut args = self.operands(args, out)?;
            args.extend(self.evidence_args(def_id, generic_args, span)?);
            return Ok(Expr::call(callee.or_at(fun_span), args));
        }
        if is_binding(self.tcx, def_id) {
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
            .is_some_and(|id| super::traits::operational(self.tcx, id))
            || (self.tcx.trait_of_assoc(def_id).is_some()
                && ty::Instance::try_resolve(self.tcx, self.typing_env, def_id, generic_args)?
                    .is_some_and(|i| self.krate.fns.contains_key(&i.def_id())))
        {
            let mut pending = Vec::new();
            let values = self.operands(args, &mut pending)?;
            if let Some(call) = self.trait_call(def_id, generic_args, values, span, &mut pending)? {
                out.extend(pending);
                return Ok(call);
            }
        }
        let Some(known) = self.std_fn(fun) else {
            // Rust counts a string's UTF-8 bytes, and JS its UTF-16 units (ADR 0034).
            let on_string = args.first().is_some_and(|&a| self.is_string_like(self.thir[a].ty));
            let indexing = self
                .tcx
                .trait_of_assoc(def_id)
                .is_some_and(|t| self.tcx.is_lang_item(t, LangItem::Index));
            if on_string && (indexing || self.tcx.item_name(def_id).as_str() == "len") {
                let what = if indexing {
                    "indexing or slicing a string"
                } else {
                    "`len()` of a string"
                };
                let why = "Rust counts its UTF-8 bytes, and JS its UTF-16 units; `is_empty()` works";
                return Err(self
                    .tcx
                    .dcx()
                    .span_err(span, format!("rust-js does not support {what}: {why}")));
            }
            let path = self.tcx.def_path_str(def_id);
            return Err(self.unsupported(self.thir[fun].span, &format!("calling `{path}`")));
        };
        // A std function that makes an `Option` of a generic `T` must box it
        // (ADR 0051); these do, and others aren't supported.
        // Normalized, so an iterator's `Self::Item` is the item's type.
        let output = self
            .tcx
            .fn_sig(def_id)
            .instantiate(self.tcx, generic_args)
            .skip_binder()
            .output();
        let output = self
            .tcx
            .try_normalize_erasing_regions(self.typing_env, output)
            .unwrap_or(output);
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
            )
        {
            return Err(self.unsupported(span, "this call, for an `Option` of a generic type"));
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
            let target = self.assignee(place)?;
            let value = self.expr(args[1], out)?;
            let js_span = self.js_span(span);
            out.push(StmtKind::Assign(target.clone(), Expr::bin(Op::Add, target, value)).at(js_span));
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
            let values = self.operands(&list, out)?;
            self.runtime.extend([Helper::AssertFailed, Helper::Debug]);
            return Ok(Expr::call(Expr::var("$assertFailed"), values));
        }
        let mut values = self.operands(args, out)?.into_iter();
        let mut arg = || values.next().expect("rustc checked the arguments");
        let js_span = self.js_span(span);
        Ok(match known {
            // An `Rc` is the JS reference itself: the garbage collector does
            // its counting, so a clone is the same object.
            Std::Same => arg(),
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
            Std::Concat => Expr::bin(Op::Add, arg(), arg()),
            Std::Eq(eq) => Expr::bin(if eq { Op::Eq } else { Op::Ne }, arg(), arg()),
            Std::LooseEq(eq) => Expr::bin(if eq { Op::LooseEq } else { Op::LooseNe }, arg(), arg()),
            Std::Method("pop") if boxed => {
                self.runtime.extend([Helper::Pop, Helper::Some]);
                Expr::call(Expr::var("$pop"), vec![arg()])
            }
            Std::Method(name) => {
                let this = arg();
                let rest = (1..args.len()).map(|_| arg()).collect();
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
            Std::SliceLast => Expr::call(Expr::member(arg(), "at"), vec![Expr::int(-1)]),
            Std::ToVec => Expr::call(Expr::member(arg(), "slice"), vec![]),
            Std::SortBy => {
                let (v, compare) = (arg(), arg());
                Expr::call(Expr::member(v, "sort"), vec![compare])
            }
            Std::Cmp => {
                self.runtime.insert(Helper::Cmp);
                Expr::call(Expr::var("$cmp"), vec![arg(), arg()])
            }
            Std::MaxOf(max) => {
                let callee = if Num::of(self.thir[args[0]].ty.peel_refs()) == Some(Num::F64) {
                    self.runtime.insert(if max { Helper::F64Max } else { Helper::F64Min });
                    Expr::var(if max { "$f64Max" } else { "$f64Min" })
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
                self.runtime.extend([Helper::UnwrapOk, Helper::Debug]);
                let list = (0..args.len()).map(|_| arg()).collect();
                Expr::call(Expr::var("$unwrapOk"), list)
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
            Std::PushStr => unreachable!("handled above"),
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
            Std::VecMacro | Std::FmtNew | Std::AssertFailed => unreachable!("handled above"),
            Std::Panic | Std::PanicFmt => {
                out.push(StmtKind::Throw(Expr::new_(Expr::var("Error"), vec![arg()])).at(js_span));
                Expr::undefined()
            }
            Std::FmtStr => arg(),
            Std::FmtDisplay => {
                let ty = generic_args.types().next().expect("`new_display` has a type argument");
                if self.is_string_like(ty) {
                    arg()
                } else if ty.is_bool() || Num::of(ty).is_some_and(|n| n != Num::F64) {
                    Expr::call(Expr::var("String"), vec![arg()])
                } else if Num::of(ty) == Some(Num::F64) {
                    self.runtime.insert(Helper::DisplayF64);
                    Expr::call(Expr::var("$displayF64"), vec![arg()])
                } else {
                    return Err(self.unsupported(span, &format!("`{{}}` of a `{ty}`")));
                }
            }
            Std::FmtDebug => {
                self.runtime.insert(Helper::Debug);
                Expr::call(Expr::var("$debug"), vec![arg()])
            }
            Std::StructEq(eq) => {
                self.runtime.insert(Helper::Eq);
                let same = Expr::call(Expr::var("$eq"), vec![arg(), arg()]);
                if eq { same } else { Expr::unary(UnaryOp::Not, same) }
            }
            Std::Push => {
                let (v, x) = (arg(), arg());
                Expr::call(Expr::member(v, "push"), vec![x])
            }
            Std::Len => Expr::member(arg(), "length"),
            Std::Clear => {
                out.push(StmtKind::Assign(Expr::member(arg(), "length"), Expr::num(0)).at(js_span));
                Expr::undefined()
            }
            Std::Retain => {
                self.runtime.insert(Helper::Retain);
                let (v, keep) = (arg(), arg());
                Expr::call(Expr::var("$retain"), vec![v, keep])
            }
            Std::ToString => {
                let ty = generic_args.type_at(0);
                if self.is_string_like(ty) {
                    arg()
                } else if ty.is_bool() || Num::of(ty).is_some_and(|n| n != Num::F64) {
                    Expr::call(Expr::var("String"), vec![arg()])
                } else {
                    return Err(self.unsupported(span, &format!("`to_string` on `{ty}`")));
                }
            }
        })
    }

    /// A JS global or a path from one (`console.log`), or from an import
    /// (`node:path#posix.join` is `posix.join`, ADR 0028).
    /// One of our functions: `f`, or `alias.f` in another module.
    /// `Some` of `items[index]`, or `None` if there's none (ADR 0051).
    fn some_at(&mut self, items: Expr, index: Expr) -> Expr {
        self.runtime.extend([Helper::SomeAt, Helper::Some]);
        Expr::call(Expr::var("$someAt"), vec![items, index])
    }

    /// `f`, or `util.f` in another module; a method, `Counter.tick` or
    /// `util.Counter.tick` (ADR 0047).
    pub(super) fn fn_ref(&self, def_id: DefId) -> Expr {
        let target = &self.krate.fns[&def_id];
        if target.module != self.module {
            self.krate.references.borrow_mut().insert((self.module, def_id));
        }
        let module = (target.module != self.module).then(|| Expr::var(&self.aliases[&target.module]));
        let holder = match (module, &target.owner) {
            (Some(module), Some(owner)) => Some(Expr::member(module, owner.clone())),
            (None, Some(owner)) => Some(Expr::var(owner)),
            (module, None) => module,
        };
        match holder {
            Some(holder) => Expr::member(holder, target.name.clone()),
            None => Expr::var(&target.name),
        }
    }

    pub(super) fn js_ref(&self, path: &str) -> Expr {
        match js_import(path) {
            Some((export, rest)) => {
                self.krate
                    .package_uses
                    .borrow_mut()
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
        let output = self.tcx.fn_sig(def_id).skip_binder().skip_binder().output();
        if self.is_std_adt(output, sym::Result) {
            self.runtime.insert(Helper::Try);
            let span = value.span;
            let thunk = Expr::arrow(Vec::new(), vec![StmtKind::Return(Some(value)).at(span)]);
            return Expr::call(Expr::var("$try"), vec![thunk]);
        }
        let settles = matches!(output.kind(), ty::Adt(adt, args) if self.is_js_object(output)
            && self.tcx.item_name(adt.did()).as_str() == "Promise"
            && args.types().next().is_some_and(|t| self.is_std_adt(t, sym::Result)));
        if settles {
            self.runtime.insert(Helper::Settle);
            return Expr::call(Expr::var("$settle"), vec![value]);
        }
        value
    }
}

/// `f(args)`, with a closure that only returns put in place, its parameters
/// the arguments: `((s) => s.value)(START)` is `START.value`. Only for
/// arguments that read the same however often they're read.
fn apply(f: Expr, args: Vec<Expr>) -> Expr {
    fn reads_same(e: &Expr) -> bool {
        match &e.kind {
            js::ExprKind::Var(_) => true,
            js::ExprKind::Member(object, _) => reads_same(object),
            _ => e.is_constant(),
        }
    }
    if let js::ExprKind::Arrow(params, body) = &f.kind
        && let [
            js::Stmt {
                kind: StmtKind::Return(Some(value)),
                ..
            },
        ] = body.as_slice()
        && params.len() <= args.len()
        && args.iter().all(reads_same)
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
