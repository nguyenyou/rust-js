//! Translate checked Rust identities and signatures to the scalar library ABI.

use super::bindings::Export;
use super::{Body, FnInfo, module_path};
use crate::library::{Dependencies, Function, Library, Scalar, Signature, VERSION};
use rustc_hir::def::DefKind;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::def_id::{DefId, LOCAL_CRATE, LocalModDefId};
use std::collections::HashMap;

fn scalar(ty: Ty<'_>) -> Option<Scalar> {
    match ty.kind() {
        ty::Bool => Some(Scalar::Bool),
        ty::Int(ty::IntTy::I32) => Some(Scalar::I32),
        ty::Uint(ty::UintTy::U32) => Some(Scalar::U32),
        ty::Tuple(items) if items.is_empty() => Some(Scalar::Unit),
        _ => None,
    }
}

fn signature(tcx: TyCtxt<'_>, id: DefId) -> Option<Signature> {
    if tcx.def_kind(id) != DefKind::Fn || tcx.generics_of(id).count() != 0 || tcx.asyncness(id).is_async() {
        return None;
    }
    let sig = tcx.fn_sig(id).skip_binder().skip_binder();
    let inputs: Vec<_> = sig.inputs().iter().map(|t| scalar(*t)).collect::<Option<_>>()?;
    if inputs.contains(&Scalar::Unit) {
        return None;
    }
    Some(Signature {
        inputs,
        output: scalar(sig.output())?,
    })
}

pub(super) fn exports(tcx: TyCtxt<'_>, functions: &HashMap<DefId, FnInfo>) -> Library {
    let mut exports: Vec<_> = functions
        .iter()
        .filter_map(|(&id, info)| {
            if !tcx.visibility(id).is_public() || info.owner.is_some() {
                return None;
            }
            Some(Function {
                rust_path: format!("{}::{}", tcx.crate_name(LOCAL_CRATE), tcx.def_path_str(id)),
                module: module_path(tcx, info.module),
                export: info.name.clone(),
                signature: signature(tcx, id)?,
            })
        })
        .collect();
    exports.sort_by(|a, b| a.rust_path.cmp(&b.rust_path));
    Library {
        version: VERSION,
        name: tcx.crate_name(LOCAL_CRATE).to_string(),
        functions: exports,
        inputs: Vec::new(),
    }
}

pub(super) fn imports(
    tcx: TyCtxt<'_>,
    bodies: &[&Body<'_>],
    dependencies: &Dependencies,
) -> Option<HashMap<(LocalModDefId, DefId), Export>> {
    let mut imports = HashMap::new();
    let mut failed = false;
    for body in bodies {
        let module = tcx.parent_module_from_def_id(body.def_id);
        for expr in &body.thir.exprs {
            let &ty::FnDef(id, _) = expr.ty.kind() else {
                continue;
            };
            if id.is_local() {
                continue;
            }
            let Some(library) = dependencies.libraries.get(tcx.crate_name(id.krate).as_str()) else {
                continue;
            };
            let path = tcx.def_path_str(id);
            let Some(function) = library.get(&path) else {
                tcx.dcx().span_err(
                    expr.span,
                    format!("rust-js: dependency does not export `{path}` under the scalar library ABI"),
                );
                failed = true;
                continue;
            };
            if signature(tcx, id).as_ref() != Some(&function.signature) {
                tcx.dcx().span_err(
                    expr.span,
                    format!("rust-js: dependency signature mismatch for `{path}`"),
                );
                failed = true;
                continue;
            }
            imports.insert((module, id), (function.from.clone(), function.export.clone()));
        }
    }
    (!failed).then_some(imports)
}
