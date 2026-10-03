//! The names a crate's items have in JS, and what each module imports and
//! exports (ADRs 0021, 0100).

use crate::lower::bindings;
use crate::lower::bindings::{Export, is_binding, js_import, js_path, module_binding};
use crate::lower::traits;
use crate::lower::{Body, FnInfo, camel_case, fresh_in};
use rustc_hir::def::DefKind;
use rustc_middle::thir::ExprKind;
use rustc_middle::ty;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::{DefId, LocalDefId, LocalModDefId};
use std::collections::{BTreeMap, HashMap, HashSet};

/// What of JS the crate's bodies use (ADRs 0024, 0028).
pub(super) struct JsUses {
    /// JS globals, whether declared here or in another crate (`web`): every
    /// module reserves them, so a local named `console` can't hide the real one.
    pub(super) globals: HashSet<String>,
    /// Exports of JS modules, with the modules that use each.
    pub(super) imported: BTreeMap<Export, HashSet<LocalModDefId>>,
    /// What each import is bound to in Rust, to name a default import after.
    pub(super) bound_to: HashMap<Export, HashSet<DefId>>,
}

pub(super) fn js_uses<'tcx>(tcx: TyCtxt<'tcx>, all_bodies: &[&Body<'tcx>]) -> JsUses {
    let mut uses = JsUses {
        globals: HashSet::new(),
        imported: BTreeMap::new(),
        bound_to: HashMap::new(),
    };
    for body in all_bodies {
        let module = tcx.parent_module_from_def_id(body.def_id);
        for expr in body.thir.exprs.iter() {
            let def_id = match (&expr.kind, expr.ty.kind()) {
                (ExprKind::ZstLiteral { .. }, ty::FnDef(def_id, _)) | (ExprKind::StaticRef { def_id, .. }, _) => {
                    *def_id
                }
                _ => continue,
            };
            if !is_binding(tcx, def_id) {
                continue;
            }
            match js_path(tcx, def_id).as_deref().map(|path| (path, js_import(path))) {
                Some((_, Some((export, _)))) => {
                    uses.bound_to.entry(export.clone()).or_default().insert(def_id);
                    uses.imported.entry(export).or_default().insert(module);
                }
                Some((path, None)) => {
                    uses.globals
                        .insert(path.split('.').next().unwrap_or_default().to_string());
                }
                None => {}
            }
        }
    }
    uses
}

/// Each import's name, the same in every file, and unique in the crate:
/// after the export, or the module for a default or namespace import. A
/// default import held by one `static` is named after it, as JS code names
/// an asset: `static hero_img` is `import heroImg from "./hero.png"`.
/// It's named around the globals and every module's items, as `taken` has
/// them, which then reserve it like a global.
/// Namespaces are named last, so a module's default export gets its plain name.
pub(super) fn name_imports(
    tcx: TyCtxt<'_>,
    uses: &JsUses,
    taken: &HashMap<LocalModDefId, HashSet<String>>,
) -> HashMap<Export, String> {
    let mut reserved: HashSet<String> = uses.globals.iter().chain(taken.values().flatten()).cloned().collect();
    let (namespaces, others): (Vec<&Export>, Vec<&Export>) =
        uses.imported.keys().partition(|(_, export)| export == "*");
    others
        .into_iter()
        .chain(namespaces)
        .map(|(from, export)| {
            let export_key = (from.clone(), export.clone());
            let held_by = match uses.bound_to[&export_key].iter().collect::<Vec<_>>().as_slice() {
                [only] if matches!(tcx.def_kind(**only), DefKind::Static { .. }) => {
                    Some(camel_case(tcx.item_name(**only).as_str()))
                }
                _ => None,
            };
            let base = match (export.as_str(), held_by) {
                ("default", Some(name)) => name,
                ("default" | "*", _) => module_binding(from),
                _ => export.clone(),
            };
            ((from.clone(), export.clone()), fresh_in(&mut reserved, &base))
        })
        .collect()
}

/// An item's JS name before it's made unique: a trait impl's accessor
/// (`circleShape`, ADR 0049), a trait method's body (`circleShape_area`,
/// or `shape_name` for a default), or the item's own name.
pub(super) fn item_js_name(tcx: TyCtxt<'_>, def_id: DefId, trait_impls: &[DefId]) -> String {
    if trait_impls.contains(&def_id) {
        return traits::impl_name(tcx, def_id);
    }
    if tcx.def_kind(def_id) == DefKind::AssocFn && tcx.inherent_impl_of_assoc(def_id).is_none() {
        let parent = tcx.parent(def_id);
        let prefix = if trait_impls.contains(&parent) {
            traits::impl_name(tcx, parent)
        } else {
            crate::lower::lower_first(tcx.item_name(parent).as_str())
        };
        return format!("{prefix}_{}", bindings::fn_name(tcx, def_id));
    }
    bindings::fn_name(tcx, def_id)
}

/// Each item's JS name, unique within its module's file, and each module's
/// names so far (`taken`, which also gets the import aliases, so local
/// variables avoid both). A method is its type's (ADR 0047): a property of
/// the object named after the type, unique in the module, and its name is
/// unique among the type's. The last result says whether it all went well.
pub(super) fn name_items(
    tcx: TyCtxt<'_>,
    items: &[LocalDefId],
    modules: &[LocalModDefId],
    globals: &HashSet<String>,
    trait_impls: &[DefId],
) -> (HashMap<LocalModDefId, HashSet<String>>, HashMap<DefId, FnInfo>, bool) {
    let mut failed = false;
    let mut taken: HashMap<LocalModDefId, HashSet<String>> = modules.iter().map(|&m| (m, globals.clone())).collect();
    let mut owners: HashMap<(LocalModDefId, DefId), String> = HashMap::new();
    let mut methods: HashMap<(LocalModDefId, DefId), HashSet<String>> = HashMap::new();
    let mut fns: HashMap<DefId, FnInfo> = HashMap::new();
    for &def_id in items {
        let module = tcx.parent_module_from_def_id(def_id);
        let names = taken.entry(module).or_default();
        let js_name = item_js_name(tcx, def_id.to_def_id(), trait_impls);
        if trait_impls.contains(&def_id.to_def_id()) && names.contains(&js_name) {
            let message = format!(
                "rust-js: generated trait implementation name `{js_name}` collides; put the implementations in separate modules"
            );
            tcx.dcx().span_err(tcx.def_span(def_id), message);
            failed = true;
        }
        let owner_type = tcx.inherent_impl_of_assoc(def_id.to_def_id()).and_then(|imp| {
            match tcx.type_of(imp).instantiate_identity().skip_normalization().kind() {
                ty::Adt(adt, _) => Some(adt.did()),
                _ => None,
            }
        });
        let (name, owner) = match owner_type {
            Some(ty) => {
                let owner = owners
                    .entry((module, ty))
                    .or_insert_with(|| fresh_in(names, tcx.item_name(ty).as_str()));
                // A property, so a name JS reserves for variables, like `new`, is fine.
                let names = methods.entry((module, ty)).or_default();
                let name = match names.insert(js_name.clone()) {
                    true => js_name,
                    false => (1..)
                        .map(|k| format!("{js_name}${k}"))
                        .find(|n| names.insert(n.clone()))
                        .expect("a free name"),
                };
                (name, Some(owner.clone()))
            }
            None => (fresh_in(names, &js_name), None),
        };
        fns.insert(def_id.to_def_id(), FnInfo { module, name, owner });
    }
    (taken, fns, failed)
}

/// The functions and `const`s used by another module: those must be
/// exported, even if private in Rust (a child module may call its parent's
/// private functions).
pub(super) fn exported_across_modules<'tcx>(
    tcx: TyCtxt<'tcx>,
    all_bodies: &[&Body<'tcx>],
    fns: &HashMap<DefId, FnInfo>,
) -> HashSet<DefId> {
    let mut exported = HashSet::new();
    for body in all_bodies {
        let from = tcx.parent_module_from_def_id(body.def_id);
        for expr in body.thir.exprs.iter() {
            let def_id = match (&expr.kind, expr.ty.kind()) {
                (ExprKind::ZstLiteral { .. }, ty::FnDef(def_id, _))
                | (ExprKind::NamedConst { def_id, .. } | ExprKind::StaticRef { def_id, .. }, _) => def_id,
                _ => continue,
            };
            if let Some(target) = fns.get(def_id)
                && target.module != from
            {
                exported.insert(*def_id);
            }
        }
    }
    exported
}
