//! `Display` (ADR 0054): a function that writes to a `Formatter` returns the
//! string it writes. Its formatter is a local string, each write is `f += s`,
//! and `fmt::Result`, which is always `Ok`, is nothing at all.

use super::representation::{self, Num};
use super::{Dest, FnCx, R};
use crate::js::{self, Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
use rustc_hir::LangItem;
use rustc_middle::thir::{self, ExprId, ExprKind, PatKind};
use rustc_middle::ty::{self, Ty};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// `&mut Formatter<'_>`.
    fn is_formatter(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Ref(_, inner, rustc_ast::Mutability::Mut)
            if self.is_std_adt(*inner, Symbol::intern("Formatter")))
    }

    pub(super) fn display_trait(&self) -> DefId {
        self.tcx
            .get_diagnostic_item(Symbol::intern("Display"))
            .expect("std has `Display`")
    }

    /// `fmt::Result`, as `Display::fmt` returns it.
    pub(super) fn is_fmt_result(&self, ty: Ty<'tcx>) -> bool {
        let fmt = self.tcx.associated_item_def_ids(self.display_trait())[0];
        let result = self.tcx.fn_sig(fmt).skip_binder().skip_binder().output();
        self.tcx.erase_and_anonymize_regions(ty) == self.tcx.erase_and_anonymize_regions(result)
    }

    /// Which parameter of `def_id` is the `Formatter` it writes to, if it
    /// takes one and returns a `fmt::Result`. In JS it returns the string
    /// instead, and takes no formatter.
    pub(super) fn formatter_param(&self, def_id: DefId) -> Option<usize> {
        let sig = self.tcx.fn_sig(def_id).skip_binder().skip_binder();
        if !self.is_fmt_result(sig.output()) {
            return None;
        }
        sig.inputs().iter().position(|&t| self.is_formatter(t))
    }

    /// The JS parameters of a function that writes to a formatter, and its
    /// body in `out`: `let f = ""`, the writes, and `return f`. Just
    /// `return s` if it writes once.
    pub(super) fn lower_writer(
        &mut self,
        params: &[thir::Param<'tcx>],
        formatter: usize,
        body: ExprId,
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Vec<js::Pattern>> {
        let mut rest = params.to_vec();
        let param = rest.remove(formatter);
        let js_params = self.lower_params(&rest, span, out)?;
        let (var, name) = match param.pat.as_deref().map(|p| &p.kind) {
            Some(PatKind::Binding { name, var, .. }) => (Some(*var), self.bind(*var, name.as_str(), true)),
            // `_: &mut Formatter`: nothing is written.
            Some(PatKind::Wild) => (None, self.fresh("f")),
            _ => return Err(self.unsupported(span, "this `Formatter` parameter")),
        };
        let mut body_out = Vec::new();
        let previous = self.writer.replace((var, name.clone()));
        let lowered = self.stmt(body, &Dest::Discard, &mut body_out);
        self.writer = previous;
        lowered?;
        // Each way through writes once: each is a `return` of what it writes.
        if let Some(returns) = as_returns(&body_out, &name) {
            out.extend(returns);
            return Ok(js_params);
        }
        // Declarations, then one write: they, then `return` of what it writes.
        if let Some((last, before)) = body_out.split_last()
            && before
                .iter()
                .all(|s| matches!(s.kind, StmtKind::Const(..) | StmtKind::Let(..)))
            && let Some(returns) = as_returns(std::slice::from_ref(last), &name)
        {
            out.extend(before.iter().cloned());
            out.extend(returns);
            return Ok(js_params);
        }
        body_out.push(StmtKind::Return(Some(Expr::var(&name))).at(js::Span::NONE));
        // A first write starts the string: `let f = s;`.
        let first = match body_out.first().map(|s| &s.kind) {
            Some(StmtKind::Assign(target, value)) if is_var(target, &name) => match &value.kind {
                js::ExprKind::Binary(Op::Add, lhs, rhs) if is_var(lhs, &name) => Some((**rhs).clone()),
                _ => None,
            },
            _ => None,
        };
        match first {
            Some(s) => {
                out.push(StmtKind::Let(name, Some(s)).at(js::Span::NONE));
                out.extend(body_out.into_iter().skip(1));
            }
            None => {
                out.push(StmtKind::Let(name, Some(Expr::str(""))).at(js::Span::NONE));
                out.extend(body_out);
            }
        }
        Ok(js_params)
    }

    /// A call that writes to this function's formatter: `write!(f, ..)`,
    /// `f.write_str(s)`, `x.fmt(f)`, or a function of ours that writes. It's
    /// `f += s`, with `s` the string it writes. `None` if it isn't one.
    pub(super) fn write_call(
        &mut self,
        def_id: DefId,
        generic_args: ty::GenericArgsRef<'tcx>,
        args: &[ExprId],
        span: Span,
        out: &mut Vec<Stmt>,
    ) -> R<Option<Expr>> {
        let Some(i) = self.formatter_param(def_id) else {
            return Ok(None);
        };
        let Some((var, name)) = self.writer.clone() else {
            return Err(self.unsupported(span, "a `Formatter` outside a `fmt`"));
        };
        if var.is_none() || self.formatter_var(args[i]) != var {
            return Err(self.unsupported(self.thir[args[i]].span, "this `Formatter`"));
        }
        let others: Vec<ExprId> = args
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .map(|(_, &a)| a)
            .collect();
        let mut values = self.operands(&others, out)?;
        let tcx = self.tcx;
        let trait_id = tcx.trait_of_assoc(def_id);
        let is_trait = |name: &str| trait_id.is_some_and(|t| tcx.is_diagnostic_item(Symbol::intern(name), t));
        let owner = tcx
            .inherent_impl_of_assoc(def_id)
            .map(|imp| tcx.type_of(imp).instantiate_identity());
        let on_formatter = owner.is_some_and(|t| self.is_std_adt(t, Symbol::intern("Formatter")));
        let written = match tcx.item_name(def_id).as_str() {
            // `write!(f, ..)` is `f.write_fmt(format_args!(..))`, which is a string (ADR 0034).
            "write_fmt" | "write_str" | "write_char" if on_formatter || is_trait("FmtWrite") => values.remove(0),
            "fmt" if is_trait("Display") => {
                let ty = generic_args.type_at(0);
                self.display_string(values.remove(0), ty, span)?
            }
            "fmt" if is_trait("Debug") => {
                let ty = generic_args.type_at(0);
                self.debug_string(values.remove(0), ty, span)?
            }
            // More than five fields: arrays of their names and strings.
            "debug_struct_fields_finish" if on_formatter => {
                self.runtime.insert(Helper::DebugFields);
                Expr::call(Expr::var("$debugFields"), values)
            }
            "debug_tuple_fields_finish" if on_formatter => {
                let (type_name, items) = (values.remove(0), values.remove(0));
                let joined = Expr::call(Expr::member(items, "join"), vec![Expr::str(", ")]);
                join(vec![type_name, Expr::str("("), joined, Expr::str(")")])
            }
            // A derived `Debug`'s body (ADR 0060): its fields are strings
            // already, each a `&dyn Debug` (`debug_dyn`).
            name if on_formatter && name.starts_with("debug_struct_field") && name.ends_with("_finish") => {
                let type_name = values.remove(0);
                let mut parts = vec![type_name, Expr::str(" { ")];
                let mut first = true;
                while values.len() >= 2 {
                    let (field, value) = (values.remove(0), values.remove(0));
                    if !first {
                        parts.push(Expr::str(", "));
                    }
                    first = false;
                    parts.extend([field, Expr::str(": "), value]);
                }
                parts.push(Expr::str(" }"));
                join(parts)
            }
            name if on_formatter && name.starts_with("debug_tuple_field") && name.ends_with("_finish") => {
                let type_name = values.remove(0);
                let mut parts = vec![type_name, Expr::str("(")];
                for (i, value) in values.drain(..).enumerate() {
                    if i > 0 {
                        parts.push(Expr::str(", "));
                    }
                    parts.push(value);
                }
                parts.push(Expr::str(")"));
                join(parts)
            }
            _ if self.krate.fns.contains_key(&def_id) && trait_id.is_none() => {
                values.extend(self.evidence_args(def_id, generic_args, span)?);
                Expr::call(self.fn_ref(def_id), values)
            }
            _ if trait_id.is_some_and(|t| t.is_local()) => {
                match self.trait_call(def_id, generic_args, values, span, out)? {
                    Some(call) => call,
                    None => return Err(self.unsupported(span, "this call")),
                }
            }
            _ => {
                let what = format!("calling `{}`", tcx.def_path_str(def_id));
                return Err(self.unsupported(span, &what));
            }
        };
        let target = Expr::var(&name);
        let js_span = self.js_span(span);
        out.push(StmtKind::Assign(target.clone(), Expr::bin(Op::Add, target, written)).at(js_span));
        Ok(Some(Expr::undefined()))
    }

    /// The variable a `Formatter` argument is: `f`, or `&mut *f`.
    fn formatter_var(&self, e: ExprId) -> Option<thir::LocalVarId> {
        match self.thir[self.strip(e)].kind {
            ExprKind::Borrow { arg, .. } | ExprKind::Deref { arg } => self.formatter_var(arg),
            ExprKind::VarRef { id } | ExprKind::UpvarRef { var_hir_id: id, .. } => Some(id),
            _ => None,
        }
    }

    /// `{}` of a `ty` value: the string itself, `String(x)`, `$displayF64(x)`,
    /// a hand-written `fmt`'s string, or `TDisplay.fmt(x)` in generic code.
    pub(super) fn display_string(&mut self, value: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let ty = ty.peel_refs();
        // `Box`, `Rc` and a `RefCell`'s `borrow()` show what they hold, as
        // they are it in JS.
        let ty = match ty.kind() {
            ty::Adt(_, args) if self.shows_inside(ty) => args.types().next().expect("what it holds").peel_refs(),
            _ => ty,
        };
        if self.is_string_like(ty) || self.is_parse_error(ty) {
            return Ok(value);
        }
        if ty.is_bool() || Num::of(ty).is_some_and(|n| n != Num::F64) {
            return Ok(shown_number(value));
        }
        if Num::of(ty) == Some(Num::F64) {
            self.runtime.insert(Helper::DisplayF64);
            return Ok(Expr::call(Expr::var("$displayF64"), vec![value]));
        }
        let display = self.display_trait();
        if let ty::Param(_) = ty.kind() {
            let tr = ty::TraitRef::new(self.tcx, display, [ty]);
            let dictionary = self
                .evidence_for(tr)
                .ok_or_else(|| self.unsupported(span, &format!("implementation evidence for `{tr}`")))?;
            return Ok(Expr::call(Expr::member(dictionary, "fmt"), vec![value]));
        }
        if self.has_user_impl(display, ty) {
            let fmt = self.tcx.associated_item_def_ids(display)[0];
            let args = self.tcx.mk_args(&[self.tcx.erase_and_anonymize_regions(ty).into()]);
            return self.impl_call(fmt, args, vec![value], span);
        }
        Err(self.unsupported(span, &format!("`{{}}` of a `{ty}`")))
    }

    /// `ParseIntError` and the like, which rust-js holds as their message
    /// (ADR 0063): `e.to_string()` is the message itself.
    pub(super) fn is_parse_error(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.crate_name(adt.did().krate) == rustc_span::sym::core
            && ["ParseIntError", "ParseFloatError", "ParseBoolError", "ParseCharError"]
                .contains(&self.tcx.item_name(adt.did()).as_str()))
    }

    pub(super) fn debug_trait(&self) -> DefId {
        self.tcx
            .get_diagnostic_item(Symbol::intern("Debug"))
            .expect("std has `Debug`")
    }

    /// Is `ty` `dyn Debug`, which rust-js holds as the string it shows
    /// (ADR 0060)? A derived `Debug` hands its fields to the formatter so.
    pub(super) fn is_dyn_debug(&self, ty: Ty<'tcx>) -> bool {
        matches!(ty.peel_refs().kind(), ty::Dynamic(traits, ..)
            if traits.principal_def_id() == Some(self.debug_trait()))
    }

    /// `{:?}` of a `ty` value (ADR 0060), as Rust shows it: `Some(1)`,
    /// `(1, "a")`, `[1.0, 2.5]`, a call of a `Debug` impl of the crate's own,
    /// derived or not, or `TDebug.fmt(x)` in generic code.
    pub(super) fn debug_string(&mut self, value: Expr, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let ty = ty.peel_refs();
        let std = |name: &str| self.is_std_adt(ty, Symbol::intern(name));
        let num = Num::of(ty);
        if self.is_dyn_debug(ty) {
            return Ok(value);
        }
        if num == Some(Num::F64) {
            self.runtime.extend([Helper::DebugF64, Helper::DisplayF64]);
            return Ok(Expr::call(Expr::var("$debugF64"), vec![value]));
        }
        if num.is_some() || ty.is_bool() {
            return Ok(shown_number(value));
        }
        if ty.is_unit() {
            return Ok(Expr::str("()"));
        }
        if ty.is_char() {
            self.runtime.insert(Helper::DebugStr);
            return Ok(Expr::call(Expr::var("$debugStr"), vec![value, Expr::str("'")]));
        }
        if self.is_string_like(ty) {
            self.runtime.insert(Helper::DebugStr);
            return Ok(Expr::call(Expr::var("$debugStr"), vec![value]));
        }
        if self.is_lang_adt(ty, LangItem::OrderingEnum) {
            let names = ["Less", "Equal", "Greater"];
            if let Some(n) = value.as_int().filter(|n| (-1..=1).contains(n)) {
                return Ok(Expr::str(names[(n + 1) as usize]));
            }
            let names = Expr::array(names.into_iter().map(Expr::str).collect());
            return Ok(Expr::index(names, Expr::bin(Op::Add, value, Expr::int(1))));
        }
        let debug = self.debug_trait();
        if let ty::Param(_) = ty.kind() {
            let tr = ty::TraitRef::new(self.tcx, debug, [ty]);
            let dictionary = self
                .evidence_for(tr)
                .ok_or_else(|| self.unsupported(span, &format!("implementation evidence for `{tr}`")))?;
            return Ok(Expr::call(Expr::member(dictionary, "fmt"), vec![value]));
        }
        // A fieldless enum is its variant's name (ADR 0013), which is what a
        // derived `Debug` shows.
        if let ty::Adt(adt, _) = ty.kind()
            && representation::is_fieldless_enum(*adt)
            && self.is_derived_impl(debug, ty)
        {
            return Ok(value);
        }
        // The crate's own, hand-written or derived.
        if self.has_user_impl(debug, ty) {
            let fmt = self.tcx.associated_item_def_ids(debug)[0];
            let args = self.args_of(debug, ty);
            return self.impl_call(fmt, args, vec![value], span);
        }
        match ty.kind() {
            // A constant is known: `Some(1.0)` is `"Some(1.0)"`, with no test.
            _ if let Some(inner) = self.option_of(ty)
                && value.is_constant()
                && !self.boxed_payload(inner) =>
            {
                if matches!(value.kind, js::ExprKind::Undefined | js::ExprKind::Null) {
                    return Ok(Expr::str("None"));
                }
                let shown = self.debug_string(value, inner, span)?;
                Ok(join(vec![Expr::str("Some("), shown, Expr::str(")")]))
            }
            _ if let Some(inner) = self.option_of(ty) => {
                let inside = if self.boxed_payload(inner) {
                    self.some_value(Expr::var("value"))
                } else {
                    Expr::var("value")
                };
                let shown = self.debug_string(inside, inner, span)?;
                let some = join(vec![Expr::str("Some("), shown, Expr::str(")")]);
                let none = Expr::bin(Op::LooseEq, Expr::var("value"), Expr::null());
                let f = Expr::arrow(
                    vec!["value".into()],
                    vec![StmtKind::Return(Some(Expr::cond(none, Expr::str("None"), some))).at(js::Span::NONE)],
                );
                Ok(self.applied(f, value))
            }
            ty::Tuple(tys) => {
                let tys: Vec<Ty<'tcx>> = tys.to_vec();
                let mut parts = vec![Expr::str("(")];
                for (i, &t) in tys.iter().enumerate() {
                    if i > 0 {
                        parts.push(Expr::str(", "));
                    }
                    parts.push(self.debug_string(Expr::index(Expr::var("tuple"), Expr::int(i as i128)), t, span)?);
                }
                if tys.len() == 1 {
                    parts.push(Expr::str(","));
                }
                parts.push(Expr::str(")"));
                let f = Expr::arrow(
                    vec!["tuple".into()],
                    vec![StmtKind::Return(Some(join(parts))).at(js::Span::NONE)],
                );
                Ok(self.applied(f, value))
            }
            ty::Array(item, _) | ty::Slice(item) => self.debug_items(value, *item, "[", "]", span),
            ty::Adt(_, args) if self.is_vec_like(ty) => self.debug_items(value, args.type_at(0), "[", "]", span),
            ty::Adt(_, args) if self.is_reverse(ty) => {
                let shown = self.debug_string(Expr::index(value, Expr::int(0)), args.type_at(0), span)?;
                Ok(join(vec![Expr::str("Reverse("), shown, Expr::str(")")]))
            }
            ty::Adt(_, args) if self.shows_inside(ty) => {
                self.debug_string(value, args.types().next().expect("what it holds"), span)
            }
            ty::Adt(_, args) if std("Cell") || std("RefCell") => {
                let name = if std("Cell") {
                    "Cell { value: "
                } else {
                    "RefCell { value: "
                };
                let shown = self.debug_string(Expr::member(value, "value"), args.type_at(0), span)?;
                Ok(join(vec![Expr::str(name), shown, Expr::str(" }")]))
            }
            ty::Adt(_, args) if self.is_set(ty) => {
                let items = self.in_order_of(value, ty, span)?;
                self.debug_items(items, args.type_at(0), "{", "}", span)
            }
            ty::Adt(_, args) if self.is_map(ty) => {
                let value = self.in_order_of(value, ty, span)?;
                let (key, item) = (args.type_at(0), args.type_at(1));
                let key = self.debug_string(Expr::var("key"), key, span)?;
                let item = self.debug_string(Expr::var("value"), item, span)?;
                let pair = join(vec![key, Expr::str(": "), item]);
                let f = Expr::arrow(
                    vec![js::Pattern::Array(vec![Some("key".into()), Some("value".into())])],
                    vec![StmtKind::Return(Some(pair)).at(js::Span::NONE)],
                );
                let entries = Expr::call(Expr::member(Expr::var("Array"), "from"), vec![value]);
                let shown = Expr::call(
                    Expr::member(Expr::call(Expr::member(entries, "map"), vec![f]), "join"),
                    vec![Expr::str(", ")],
                );
                Ok(join(vec![Expr::str("{"), shown, Expr::str("}")]))
            }
            ty::Adt(_, args) if std("Result") => {
                let inside = || Expr::member(Expr::var("result"), "_0");
                let ok = self.debug_string(inside(), args.type_at(0), span)?;
                let err = self.debug_string(inside(), args.type_at(1), span)?;
                let f = Expr::arrow(
                    vec!["result".into()],
                    vec![
                        StmtKind::Return(Some(Expr::cond(
                            Expr::bin(Op::Eq, Expr::member(Expr::var("result"), "TAG"), Expr::str("Ok")),
                            join(vec![Expr::str("Ok("), ok, Expr::str(")")]),
                            join(vec![Expr::str("Err("), err, Expr::str(")")]),
                        )))
                        .at(js::Span::NONE),
                    ],
                );
                Ok(self.applied(f, value))
            }
            _ => Err(self.unsupported(span, &format!("`{{:?}}` of a `{ty}`"))),
        }
    }

    /// A sequence's `{:?}`: `"[" + items.map((item) => ..).join(", ") + "]"`.
    fn debug_items(&mut self, items: Expr, item: Ty<'tcx>, open: &str, close: &str, span: Span) -> R<Expr> {
        let shown = self.debug_string(Expr::var("item"), item, span)?;
        let f = Expr::arrow(
            vec!["item".into()],
            vec![StmtKind::Return(Some(shown)).at(js::Span::NONE)],
        );
        let items = if self.is_map(item) || open == "{" {
            Expr::call(Expr::member(Expr::var("Array"), "from"), vec![items])
        } else {
            items
        };
        let joined = Expr::call(
            Expr::member(Expr::call(Expr::member(items, "map"), vec![f]), "join"),
            vec![Expr::str(", ")],
        );
        Ok(join(vec![Expr::str(open), joined, Expr::str(close)]))
    }

    /// `Box<T>`, `Rc<T>`, `Ref<T>` and `RefMut<T>`: shown as their `T`,
    /// which is the value they are in JS (ADR 0023).
    fn shows_inside(&self, ty: Ty<'tcx>) -> bool {
        ty.is_box()
            || ["Rc", "RefCellRef", "RefCellRefMut"]
                .iter()
                .any(|n| self.is_std_adt(ty, Symbol::intern(n)))
    }

    /// Does `{:?}` of a `ty` read the value more than once? An `Option`, a
    /// `Result` and a tuple are shown by their parts.
    pub(super) fn debug_reads_parts(&self, ty: Ty<'tcx>) -> bool {
        let ty = ty.peel_refs();
        if self.is_dyn_debug(ty) || matches!(ty.kind(), ty::Param(_)) || self.has_user_impl(self.debug_trait(), ty) {
            return false;
        }
        match ty.kind() {
            ty::Tuple(tys) => !tys.is_empty(),
            ty::Adt(_, args) if ty.is_box() || self.is_std_adt(ty, Symbol::intern("Rc")) => {
                self.debug_reads_parts(args.type_at(0))
            }
            _ => self.option_of(ty).is_some() || self.is_std_adt(ty, Symbol::intern("Result")),
        }
    }

    /// `f(value)`, with a function that only returns written in place when
    /// `value` is a variable: `value == null ? "None" : ..`.
    fn applied(&mut self, f: Expr, value: Expr) -> Expr {
        if let js::ExprKind::Arrow(params, body) = &f.kind
            && let [js::Pattern::Name(name)] = params.as_slice()
            && let [
                Stmt {
                    kind: StmtKind::Return(Some(result)),
                    ..
                },
            ] = body.as_slice()
            && value.reads_same()
            && let Some(inlined) = result.substitute_in_callbacks(&|n: &str| (n == name).then(|| value.clone()))
        {
            return inlined;
        }
        Expr::call(f, vec![value])
    }

    /// `(value) => <its string>` for a dictionary's `fmt`, or the function
    /// itself: `String`, `$displayF64`.
    pub(super) fn display_fn(&mut self, ty: Ty<'tcx>, span: Span) -> R<Expr> {
        let shown = self.display_string(Expr::var("value"), ty, span)?;
        if let js::ExprKind::Call(callee, args) = &shown.kind
            && matches!(callee.kind, js::ExprKind::Var(_))
            && matches!(args.as_slice(), [only] if is_var(only, "value"))
        {
            return Ok((**callee).clone());
        }
        Ok(Expr::arrow(
            vec!["value".into()],
            vec![StmtKind::Return(Some(shown)).at(js::Span::NONE)],
        ))
    }
}

/// `f += s`, as a `return s`, for a body that writes once whichever way
/// it goes: `if (c) { f += a } else { f += b }` is `if (c) { return a } else
/// { return b }`. `None` if some way writes more, or does anything else.
fn as_returns(body: &[Stmt], name: &str) -> Option<Vec<Stmt>> {
    let [stmt] = body else { return None };
    let kind = match &stmt.kind {
        StmtKind::Assign(target, value) if is_var(target, name) => match &value.kind {
            js::ExprKind::Binary(Op::Add, lhs, rhs) if is_var(lhs, name) => StmtKind::Return(Some((**rhs).clone())),
            _ => return None,
        },
        StmtKind::If(test, then, Some(els)) => {
            StmtKind::If(test.clone(), as_returns(then, name)?, Some(as_returns(els, name)?))
        }
        _ => return None,
    };
    Some(vec![kind.at(stmt.span)])
}

/// Strings joined: text alone is a string, text and values a template
/// literal, \`Some(${x})\`, and values alone `a + b`. A part that's itself
/// strings joined is taken apart, so templates don't nest needlessly.
pub(super) fn join(parts: Vec<Expr>) -> Expr {
    let mut pieces: Vec<Expr> = Vec::new();
    for piece in parts.into_iter().flat_map(joined_pieces) {
        match (pieces.last_mut(), &piece.kind) {
            (
                Some(Expr {
                    kind: js::ExprKind::Str(before),
                    ..
                }),
                js::ExprKind::Str(after),
            ) => before.push_str(after),
            _ => pieces.push(piece),
        }
    }
    let is_text = |p: &Expr| matches!(p.kind, js::ExprKind::Str(_));
    if pieces.iter().all(is_text) || !pieces.iter().any(is_text) {
        return pieces
            .into_iter()
            .reduce(|a, b| Expr::bin(Op::Add, a, b))
            .unwrap_or_else(|| Expr::str(""));
    }
    let (mut texts, mut values) = (vec![String::new()], Vec::new());
    for piece in pieces {
        match piece.kind {
            js::ExprKind::Str(text) => texts.last_mut().expect("a text").push_str(&text),
            _ => {
                values.push(unstringed(piece));
                texts.push(String::new());
            }
        }
    }
    Expr::template(texts, values)
}

/// `String(x)` is `x` in a template, which makes it a string the same way.
fn unstringed(value: Expr) -> Expr {
    match value.kind {
        js::ExprKind::Call(ref callee, ref args)
            if matches!(&callee.kind, js::ExprKind::Var(name) if name == "String") && args.len() == 1 =>
        {
            args[0].clone()
        }
        _ => value,
    }
}

/// The pieces of strings joined: of a template, its texts and values; of
/// `"(" + a + ")"`, which starts with a string, so that each `+` in it
/// concatenates, its operands. Anything else is one piece.
fn joined_pieces(e: Expr) -> Vec<Expr> {
    if let js::ExprKind::Template(texts, values) = e.kind {
        let mut pieces = Vec::new();
        let mut values = values.into_iter();
        for text in texts {
            if !text.is_empty() {
                pieces.push(Expr::str(text));
            }
            pieces.extend(values.next());
        }
        return pieces;
    }
    let mut pieces = Vec::new();
    let mut rest = e;
    while let js::ExprKind::Binary(Op::Add, left, right) = rest.kind {
        pieces.push(*right);
        rest = *left;
    }
    let starts_with_string = matches!(rest.kind, js::ExprKind::Str(_));
    pieces.push(rest);
    pieces.reverse();
    if starts_with_string || pieces.len() == 1 {
        pieces
    } else {
        vec![
            pieces
                .into_iter()
                .reduce(|a, b| Expr::bin(Op::Add, a, b))
                .expect("a piece"),
        ]
    }
}

fn is_var(e: &Expr, name: &str) -> bool {
    matches!(&e.kind, js::ExprKind::Var(n) if n == name)
}

/// `String(n)`, or the text itself for a constant: `"4096"`.
fn shown_number(value: Expr) -> Expr {
    match value.as_int() {
        Some(n) => Expr::str(n.to_string()),
        None => Expr::call(Expr::var("String"), vec![value]),
    }
}
