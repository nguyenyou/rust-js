//! `?` of an `Option` or a `Result`, and the `From` that converts its error
//! (ADRs 0052, 0141).

use super::recognition::{StdItem, std_item};
use super::{Dest, FnCx, R};
use crate::js::{self, Expr, Op, Prop, Stmt, StmtKind};
use rustc_middle::thir::{ExprId, ExprKind};
use rustc_middle::ty::{self, Ty};

impl<'a, 'tcx> FnCx<'a, 'tcx> {
    /// `e?`: the value inside, after returning early with an `Err` or `None`.
    /// Only when the `Err` is returned as it is: a `From` conversion isn't
    /// supported yet.
    pub(super) fn question(
        &mut self,
        question: ExprId,
        tried: ExprId,
        base: Option<&str>,
        out: &mut Vec<Stmt>,
    ) -> R<Expr> {
        let span = self.thir[question].span;
        let ty = self.thir[tried].ty;
        // A write never fails (ADR 0054, ADR 0132).
        if self.is_fmt_result(ty) || self.recognition().is_io_unit_result(ty) {
            self.stmt(tried, &Dest::Discard, out)?;
            return Ok(Expr::undefined());
        }
        let is_option = self.option_of(ty).is_some();
        if !is_option && !self.is_std_type(ty, StdItem::Result) {
            return Err(self.unsupported(span, &format!("`?` on a `{ty}`")));
        }
        // The function's error type: the same as this one's, and the `Err` is
        // returned as it is, or one with a `From` of the crate's own, and it's
        // `{ TAG: "Err", _0: from(error) }`.
        let mut from = None;
        // Or a `Box<dyn Error>`, of the error and its dictionary (ADR 0141).
        let mut boxed_error = None;
        if !is_option {
            let ExprKind::Match { ref arms, .. } = self.thir[self.strip(question)].kind else {
                unreachable!("checked")
            };
            let returned = arms
                .iter()
                .find_map(|&arm| match self.thir[self.strip(self.thir[arm].body)].kind {
                    ExprKind::Return { value: Some(v) } => Some(self.thir[v].ty),
                    _ => None,
                });
            let error = |t: Ty<'tcx>| match t.kind() {
                ty::Adt(_, args) => args.types().nth(1),
                _ => None,
            };
            let (to, from_ty) = (returned.and_then(error), error(ty));
            // A `&str` error to a `String` one: the same JS string.
            let same_string = to
                .zip(from_ty)
                .is_some_and(|(to, from_ty)| self.is_string_like(to) && self.is_string_like(from_ty));
            if to != from_ty && !same_string {
                let (Some(to), Some(from_ty)) = (to, from_ty) else {
                    return Err(self.unsupported(span, "this `?`"));
                };
                match self.dyn_error_from(to, from_ty, span)? {
                    Some(dictionary) => boxed_error = Some(dictionary),
                    None => {
                        from =
                            Some(self.error_from(to, from_ty)?.ok_or_else(|| {
                                self.unsupported(span, "`?` that converts the error with this `From`")
                            })?);
                    }
                }
            }
        }
        let (subject, _) = self.subject(tried, base.unwrap_or(if is_option { "value" } else { "result" }), out)?;
        let js_span = self.js_span(span);
        let (failed, ret, value) = if is_option {
            let boxed = self.option_of(ty).is_some_and(|inner| self.boxed_payload(inner));
            let value = if boxed {
                self.some_value(subject.clone())
            } else {
                subject.clone()
            };
            (Expr::bin(Op::LooseEq, subject, Expr::null()), Expr::undefined(), value)
        } else {
            let failed = Expr::bin(Op::Eq, Expr::member(subject.clone(), "TAG"), Expr::str("Err"));
            let error = Expr::member(subject.clone(), "_0");
            let converted = match (from, boxed_error) {
                (Some(from), _) => Some(Expr::call(from, vec![error])),
                (None, Some(dictionary)) => Some(Expr::object(vec![
                    Prop::Field("value".into(), error),
                    Prop::Field("impl".into(), dictionary),
                ])),
                (None, None) => None,
            };
            let ret = match converted {
                Some(converted) => Expr::object(vec![
                    Prop::Field("TAG".into(), Expr::str("Err")),
                    Prop::Field("_0".into(), converted),
                ]),
                None => subject.clone(),
            };
            (failed, ret, Expr::member(subject, "_0"))
        };
        out.push(StmtKind::If(failed, vec![StmtKind::Return(Some(ret)).at(js_span)], None).at(js_span));
        Ok(value)
    }

    /// The function `?` converts an error with, `<to as From<from>>::from`,
    /// if it's one of the crate's own (ADR 0052).
    pub(super) fn error_from(&mut self, to: Ty<'tcx>, from: Ty<'tcx>) -> R<Option<Expr>> {
        let from_trait = std_item(self.tcx, StdItem::From);
        let method = self.tcx.associated_item_def_ids(from_trait)[0];
        let args = self.tcx.mk_args(&[to.into(), from.into()]);
        let Some(instance) = self.resolve_instance(method, args)? else {
            return Ok(None);
        };
        if !self.krate.fns.contains_key(&instance.def_id()) {
            return Ok(None);
        }
        let evidence = self.evidence_args(instance.def_id(), instance.args, self.tcx.def_span(instance.def_id()))?;
        let callee = self.fn_ref(instance.def_id());
        Ok(Some(if evidence.is_empty() {
            callee
        } else {
            let mut values = vec![Expr::var("error")];
            values.extend(evidence);
            Expr::arrow(
                vec!["error".into()],
                vec![StmtKind::Return(Some(Expr::call(callee, values))).at(js::Span::NONE)],
            )
        }))
    }
}
