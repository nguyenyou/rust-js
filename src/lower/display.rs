//! `Display` (ADR 0054): a function that writes to a `Formatter` returns the
//! string it writes. Its formatter is a local string, each write is `f += s`,
//! and `fmt::Result`, which is always `Ok`, is nothing at all.

use super::representation::Num;
use super::{Dest, FnCx, R};
use crate::js::{self, Expr, Op, Stmt, StmtKind};
use crate::runtime::Helper;
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
                self.runtime.insert(Helper::Debug);
                Expr::call(Expr::var("$debug"), vec![values.remove(0)])
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
        // `Box` and `Rc` show what they hold.
        let ty = match ty.kind() {
            ty::Adt(_, args) if ty.is_box() || self.is_std_adt(ty, Symbol::intern("Rc")) => args.type_at(0).peel_refs(),
            _ => ty,
        };
        if self.is_string_like(ty) {
            return Ok(value);
        }
        if ty.is_bool() || Num::of(ty).is_some_and(|n| n != Num::F64) {
            return Ok(Expr::call(Expr::var("String"), vec![value]));
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

fn is_var(e: &Expr, name: &str) -> bool {
    matches!(&e.kind, js::ExprKind::Var(n) if n == name)
}
