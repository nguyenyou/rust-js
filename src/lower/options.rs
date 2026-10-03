//! An `Option`'s or a `Result`'s method, and `bool::then` (ADRs 0030, 0062).

use super::calls::Call;
use super::recognition::Std;
use super::{FnCx, R};
use crate::js;
use crate::js::{Expr, Op, Prop, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_middle::ty::{self, Ty};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// An `Option`'s or a `Result`'s method, and `bool::then` (ADRs 0030, 0062): `None` if `known` is another.
    pub(super) fn option_call(
        &mut self,
        known: Std,
        call: Call<'_, 'tcx>,
        values: &mut std::vec::IntoIter<Expr>,
        boxed: bool,
        out: &mut Vec<Stmt>,
    ) -> R<Option<Expr>> {
        let Call {
            generic_args,
            args,
            span,
            ..
        } = call;
        let mut arg = || values.next().expect("rustc checked the arguments");
        Ok(Some(match known {
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
                    return Ok(Some(value.clone()));
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
            Std::OptionIter => {
                let item = self
                    .option_of(self.thir[args[0]].ty.peel_refs())
                    .expect("an `Option` has a `T`");
                let option = arg();
                self.option_items(option, item, out)
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
            _ => return Ok(None),
        }))
    }

    /// `Some` of `items[index]`, or `None` if there's none (ADR 0051).
    pub(super) fn some_at(&mut self, items: Expr, index: Expr) -> Expr {
        self.runtime.insert(Helper::SomeAt);
        Expr::call(Expr::var("$someAt"), vec![items, index])
    }

    /// `Some(value)` of a generic `T` (ADR 0051): `$some(value)`.
    pub(super) fn some(&mut self, value: Expr) -> Expr {
        self.runtime.insert(Helper::Some);
        Expr::call(Expr::var("$some"), vec![value])
    }

    /// An `Option<T>`'s items, as its `iter()` gives them (ADR 0128):
    /// `option == null ? [] : [option]`.
    pub(super) fn option_items(&mut self, option: Expr, item: Ty<'tcx>, out: &mut Vec<Stmt>) -> Expr {
        let option = if option.reads_same() {
            option
        } else {
            self.spill("option", option, out)
        };
        let value = if self.boxed_payload(item) {
            self.some_value(option.clone())
        } else {
            option.clone()
        };
        Expr::cond(
            Expr::bin(Op::LooseEq, option, Expr::null()),
            Expr::array(vec![]),
            Expr::array(vec![value]),
        )
    }

    /// What's in an `Option` of a generic `T` (ADR 0051): `$someValue(option)`.
    pub(super) fn some_value(&mut self, option: Expr) -> Expr {
        self.runtime.insert(Helper::SomeValue);
        Expr::call(Expr::var("$someValue"), vec![option])
    }
}
