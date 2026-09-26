//! Element bindings are compiler plumbing. Check resolved uses, including
//! aliases, method references and unused functions, rather than source names.
use super::bindings::{JsForm, js_form};
use rustc_hir::{
    self as hir,
    intravisit::{self, Visitor},
};
use rustc_middle::ty::{self, TyCtxt, TypeckResults};
use rustc_span::def_id::DefId;
use rustc_span::{Span, Symbol};

pub(super) fn validate(tcx: TyCtxt<'_>) -> bool {
    let mut valid = true;
    for owner in tcx.hir_body_owners() {
        let body = tcx.hir_body_owned_by(owner);
        let mut visitor = Uses {
            tcx,
            types: tcx.typeck_body(body.id()),
            permitted: None,
            valid: true,
        };
        visitor.visit_body(body);
        valid &= visitor.valid;
    }
    valid
}

struct Uses<'tcx> {
    tcx: TyCtxt<'tcx>,
    types: &'tcx TypeckResults<'tcx>,
    permitted: Option<hir::HirId>,
    valid: bool,
}

impl Uses<'_> {
    fn check(&mut self, def: DefId, span: Span, permitted: bool) {
        if permitted
            || !matches!(
                self.tcx.def_kind(def),
                hir::def::DefKind::Fn | hir::def::DefKind::AssocFn
            )
        {
            return;
        }
        let internal = match js_form(self.tcx, def) {
            JsForm::Jsx(_) => true,
            JsForm::Prop(_) => {
                let ty = self.tcx.fn_sig(def).instantiate_identity().skip_binder().output();
                matches!(ty.kind(), ty::Adt(adt, _) if self.tcx.get_attrs_by_path(adt.did(), &[Symbol::intern("rust_js"), Symbol::intern("jsx_element")]).next().is_some())
            }
            _ => false,
        };
        if internal {
            self.tcx.dcx().span_err(span, "rust-js: element builders are compiler-only; construct elements and set their props inside `jsx! { <Tag ... /> }`");
            self.valid = false;
        }
    }
}

impl<'tcx> Visitor<'tcx> for Uses<'tcx> {
    fn visit_expr(&mut self, expr: &'tcx hir::Expr<'tcx>) {
        let generated = self
            .tcx
            .hir_attrs(expr.hir_id)
            .iter()
            .any(|a| a.path_matches(&[Symbol::intern("rust_js"), Symbol::intern("jsx")]));
        let old = self.permitted;
        match expr.kind {
            hir::ExprKind::Call(callee, _) if generated => self.permitted = Some(callee.hir_id),
            hir::ExprKind::MethodCall(_, _, _, _) => {
                if let Some(def) = self.types.type_dependent_def_id(expr.hir_id) {
                    self.check(def, expr.span, generated);
                }
            }
            hir::ExprKind::Path(ref path) => {
                if let hir::def::Res::Def(_, def) = self.types.qpath_res(path, expr.hir_id) {
                    self.check(def, expr.span, self.permitted == Some(expr.hir_id));
                }
            }
            _ => {}
        }
        intravisit::walk_expr(self, expr);
        self.permitted = old;
    }
}
