//! References to items: what a function, a binding or a std function is
//! in JS, whether it's the crate's, and which impl a call runs (ADRs 0049,
//! 0100).

use super::bindings::{JsForm, is_binding, is_method, js_form, js_import};
use super::numbers::NumOp;
use super::recognition::Std;
use super::representation::Num;
use super::text::TextOp;
use super::{FnCx, R, camel_case, global};
use crate::js;
use crate::js::{Expr, Op, StmtKind};
use rustc_middle::thir::{ExprId, ExprKind};
use rustc_middle::ty::{self, Ty};
use rustc_span::Span;
use rustc_span::def_id::DefId;

impl<'a, 'tcx> FnCx<'a, 'tcx> {
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

    /// A function or its type's method object, imported by name when it lives
    /// in another module. The linker resolves collisions after lowering.
    /// Is `def_id` a Rust function rust-js compiled: the crate's own, or one a
    /// library exports (ADR 0100)?
    pub(super) fn is_rust_fn(&self, def_id: DefId) -> bool {
        self.krate.fns.contains_key(&def_id) || self.krate.foreign.item(def_id).is_some()
    }

    /// Is `def_id` a trait's function that a call of runs through the trait:
    /// one of a trait the crate's own impls are called through, or that
    /// resolves to a function rust-js compiled.
    pub(super) fn is_rust_trait_fn(&self, def_id: DefId, args: ty::GenericArgsRef<'tcx>) -> bool {
        self.tcx.trait_of_assoc(def_id).is_some_and(|id| {
            super::traits::operational(self.tcx, self.krate.foreign, id)
                || self
                    .resolve_instance(def_id, args)
                    .ok()
                    .flatten()
                    .is_some_and(|i| self.is_rust_fn(i.def_id()))
        })
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

    /// `resolve_instance` of a call whose arguments a copied default's
    /// `Self` was replaced in (ADR 0049): they're the impl's, which resolve in
    /// the impl's typing environment, as its own body's types don't.
    pub(super) fn resolve_self_instance(
        &self,
        def_id: DefId,
        args: ty::GenericArgsRef<'tcx>,
    ) -> Result<Option<ty::Instance<'tcx>>, rustc_span::ErrorGuaranteed> {
        let typing_env = self.given.self_env.unwrap_or(self.typing_env);
        let Ok(args) = self
            .tcx
            .try_normalize_erasing_regions(typing_env, ty::Unnormalized::new_wip(args))
        else {
            return Ok(None);
        };
        ty::Instance::try_resolve(self.tcx, typing_env, def_id, args)
    }

    /// `Into::<U>::into` of a `T` as `<U as From<T>>::from`, and
    /// `TryInto` as `TryFrom`, if that's a hand-written impl.
    pub(super) fn resolve_into(
        &self,
        def_id: DefId,
        args: ty::GenericArgsRef<'tcx>,
    ) -> Option<(DefId, ty::GenericArgsRef<'tcx>)> {
        let (method, args, implementation) = self.recognition().resolve_into(def_id, args)?;
        self.is_rust_fn(implementation).then_some((method, args))
    }

    /// The function a call runs: the crate's impl a trait method resolves to,
    /// or the `From` an `Into` does, else the one named.
    pub(super) fn callee(&self, def_id: DefId, args: ty::GenericArgsRef<'tcx>) -> (DefId, ty::GenericArgsRef<'tcx>) {
        self.impl_method(def_id, args)
            .ok()
            .flatten()
            .or_else(|| self.resolve_into(def_id, args))
            .unwrap_or((def_id, args))
    }

    /// Which std function `fun` is, if rust-js knows what it means in JS.
    pub(super) fn std_fn(&self, fun: ExprId) -> Option<Std> {
        let &ty::FnDef(def_id, args) = self.thir[self.strip(fun)].ty.kind() else {
            return None;
        };
        self.recognition().classify(def_id, args)
    }

    /// A std function taken as a value, `str::trim` in `.map(str::trim)`:
    /// an arrow of one parameter, doing what a call does. `None` for one
    /// that isn't one of these.
    pub(super) fn std_fn_value(&mut self, known: Std, ty: Ty<'tcx>, span: Span) -> R<Option<Expr>> {
        let ty::FnDef(def_id, args) = *ty.kind() else {
            return Ok(None);
        };
        let js_span = self.js_span(span);
        // `size_of::<u16>`: `() => 2`. `drop`: what dropping its value runs,
        // and `forget`: nothing (ADR 0098).
        match known {
            Std::SizeOf | Std::AlignOf => {
                let bytes = self.layout_bytes(known, args.type_at(0), span)?;
                return Ok(Some(Expr::arrow(
                    Vec::new(),
                    vec![StmtKind::Return(Some(Expr::int(bytes))).at(js_span)],
                )));
            }
            Std::Drop | Std::Forget => {
                let mut body = Vec::new();
                if known == Std::Drop {
                    self.drop_value(Expr::var("value"), args.type_at(0), span, &mut body)?;
                }
                return Ok(Some(Expr::arrow(vec!["value".into()], body)));
            }
            _ => {}
        }
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
            Std::IsSome => Expr::bin(Op::LooseNe, x, Expr::null()),
            Std::IsNone => Expr::bin(Op::LooseEq, x, Expr::null()),
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
        Ok(Some(Expr::arrow(
            vec![name.into()],
            vec![StmtKind::Return(Some(body)).at(js_span)],
        )))
    }

    /// The crate's own impl method a trait method call resolves to, if it
    /// does: in a copied default, `Self` is the impl's type (ADR 0049).
    pub(super) fn impl_method(
        &self,
        id: DefId,
        generic_args: ty::GenericArgsRef<'tcx>,
    ) -> R<Option<(DefId, ty::GenericArgsRef<'tcx>)>> {
        let generic_args = match self.given.self_args {
            Some(args) => ty::EarlyBinder::bind(self.tcx, generic_args)
                .instantiate(self.tcx, args)
                .skip_normalization(),
            None => generic_args,
        };
        Ok(self
            .resolve_self_instance(id, generic_args)?
            .filter(|instance| {
                self.is_rust_fn(instance.def_id()) && self.tcx.trait_of_assoc(instance.def_id()).is_none()
            })
            .map(|instance| (instance.def_id(), instance.args)))
    }

    /// `ty`, of a function's own generics, in a call of it, `generic_args`.
    pub(super) fn instantiated(&self, ty: Ty<'tcx>, generic_args: ty::GenericArgsRef<'tcx>) -> Ty<'tcx> {
        let ty = ty::EarlyBinder::bind(self.tcx, ty)
            .instantiate(self.tcx, generic_args)
            .skip_normalization();
        self.tcx
            .try_normalize_erasing_regions(self.typing_env, ty::Unnormalized::new_wip(ty))
            .unwrap_or(ty)
    }
}
