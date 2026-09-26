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
            return Err(self.unsupported(f.span, "calling this"));
        };
        if self.krate.fns.contains_key(&def_id) {
            let callee = self.fn_ref(def_id);
            let args = self.operands(args, out)?;
            return Ok(Expr::call(callee.or_at(fun_span), args));
        }
        if is_binding(self.tcx, def_id) {
            match js_form(self.tcx, def_id) {
                JsForm::Jsx(tag) => return self.jsx(&tag, args, span, out),
                JsForm::Prop(name) => return self.jsx_prop(name.as_deref(), args, span, out),
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
        // `vec![a, b]` is `box_assume_init_into_vec_unsafe(write_box_via_move(<box>, [a, b]))`.
        if known == Std::VecMacro {
            let ExprKind::Call { args: ref inner, .. } = self.thir[self.strip(args[0])].kind else {
                return Err(self.unsupported(span, "this `vec!`"));
            };
            return self.expr(inner[1], out);
        }
        if known.takes_iterator() || matches!(known, Std::Sort | Std::SortByKey) {
            return self.iterator_call(known, args, generic_args, span, out);
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
            Std::MaxOf(max) => Expr::call(
                Expr::member(Expr::var("Math"), if max { "max" } else { "min" }),
                vec![arg(), arg()],
            ),
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
            Std::LocalWith => {
                let (key, f) = (arg(), arg());
                Expr::call(f, vec![key])
            }
            Std::LocalBorrow => {
                let (key, f) = (arg(), arg());
                Expr::call(f, vec![Expr::member(key, "value")])
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
                Expr::cond(ok, Expr::member(result, "_0"), otherwise)
            }
            Std::PushStr => unreachable!("handled above"),
            Std::IsSome => Expr::bin(Op::LooseNe, arg(), Expr::null()),
            Std::IsNone => Expr::bin(Op::LooseEq, arg(), Expr::null()),
            Std::Unwrap => {
                self.runtime.insert(Helper::Unwrap);
                // `expect` has a message too.
                let list = (0..args.len()).map(|_| arg()).collect();
                Expr::call(Expr::var("$unwrap"), list)
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
                Expr::bin(Op::Coalesce, option, default)
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
    pub(super) fn fn_ref(&self, def_id: DefId) -> Expr {
        let target = &self.krate.fns[&def_id];
        if target.module == self.module {
            Expr::var(&target.name)
        } else {
            Expr::member(Expr::var(&self.aliases[&target.module]), target.name.clone())
        }
    }

    pub(super) fn js_ref(&self, path: &str) -> Expr {
        match js_import(path) {
            Some((export, rest)) => global(&format!("{}{rest}", self.krate.imports[&export])),
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
