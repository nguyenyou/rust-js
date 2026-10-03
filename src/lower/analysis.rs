//! Collect crate facts before emission; no function lowering or linking.

mod debug;
mod drops;
mod mutation;
mod naming;
mod type_facts;
mod validation;

use super::bindings;
use super::bindings::{Export, is_binding};
use super::recognition::TypeFact;
use super::traits;
use super::{Body, FnInfo, TestFn, module_path};
use crate::lower::recognition::{StdItem, is_std_def};
use debug::{derived_debug, uses_format_options, uses_pretty_debug};
use drops::drop_params;
use mutation::mutated_types;
use naming::{exported_across_modules, js_uses, name_imports, name_items};
use rustc_hir::def::DefKind;
use rustc_hir::find_attr;
use rustc_middle::mir::BorrowKind;
use rustc_middle::thir::ExprKind;
use rustc_middle::ty;
use rustc_middle::ty::{Ty, TyCtxt, TypeVisitableExt};
use rustc_span::Symbol;
use rustc_span::def_id::{DefId, LocalDefId, LocalModDefId};
use std::collections::{BTreeMap, HashMap, HashSet};
use type_facts::type_fact_params;
pub(super) use validation::is_thread_local;
use validation::{in_thread_local, reject_static_references, reject_unsupported};

/// Made by serde's `#[derive(Serialize)]` or `#[derive(Deserialize)]`, or
/// inside what they made (its `const _: () = { .. }`): left out, since
/// rust-js writes each type's JSON codec itself (ADR 0077).
pub(super) use super::recognition::from_serde_derive;

/// Is `id` serde's derived `impl Serialize` (`Some(true)`) or `impl
/// Deserialize` (`Some(false)`)?
/// Only the impls for the crate's own types: the derive's helpers inside
/// its `const _` block have impls too.
pub(super) use super::recognition::serde_impl;

/// Copy the THIR of every function and closure in the crate.
///
/// Must run *before* `analysis`: building MIR for borrowck consumes ("steals")
/// the THIR, so this is our only chance to read it.
pub fn collect_bodies(tcx: TyCtxt<'_>) -> Vec<Body<'_>> {
    let items = tcx.hir_crate_items(());
    items
        .definitions()
        .chain(items.nested_bodies())
        .filter(|&def_id| !from_serde_derive(tcx, def_id))
        .filter(|&def_id| match tcx.def_kind(def_id) {
            // A function declared in an `extern` block is JS's (ADR 0021),
            // and so is one with `#[rust_js::link_name]` (ADR 0039).
            DefKind::Fn => !is_binding(tcx, def_id.to_def_id()),
            // A method of an `impl Type` block (ADR 0047).
            // A derived impl's is never called, but a derived `Debug`'s is
            // how `{:?}` shows its type (ADR 0060).
            DefKind::AssocFn => {
                let parent = tcx.parent(def_id.to_def_id());
                tcx.hir_maybe_body_owned_by(def_id).is_some()
                    && (!tcx.is_automatically_derived(parent) || derived_debug(tcx, parent))
                    && !is_binding(tcx, def_id.to_def_id())
            }
            DefKind::Closure => true,
            _ => false,
        })
        .filter_map(|def_id| {
            let (thir, expr) = tcx.thir_body(def_id).ok()?;
            let thir = (*thir.borrow()).clone();
            let facts = super::body_queries::BodyFacts::collect(tcx, &thir);
            Some(Body {
                def_id,
                thir,
                expr,
                facts,
            })
        })
        .collect()
}

/// The initializers of the crate's statics and constants, copied as its
/// functions' are, before MIR building steals them: one whose value rustc's
/// can't say is lowered as code instead (ADR 0096).
pub fn collect_initializers(tcx: TyCtxt<'_>) -> Vec<Body<'_>> {
    tcx.hir_crate_items(())
        .definitions()
        .filter(|&def_id| matches!(tcx.def_kind(def_id), DefKind::Const { .. } | DefKind::Static { .. }))
        .filter(|&def_id| !tcx.is_foreign_item(def_id) && tcx.hir_maybe_body_owned_by(def_id).is_some())
        .filter_map(|def_id| {
            let (thir, expr) = tcx.thir_body(def_id).ok()?;
            let thir = (*thir.borrow()).clone();
            let facts = super::body_queries::BodyFacts::collect(tcx, &thir);
            Some(Body {
                def_id,
                thir,
                expr,
                facts,
            })
        })
        .collect()
}

/// Crate-wide facts collected before function emission. Body references point
/// into the captured THIR; all collections are owned by this analysis result.
pub(super) struct AnalyzedCrate<'a, 'tcx> {
    pub bodies: Vec<&'a Body<'tcx>>,
    pub closures: HashMap<LocalDefId, &'a Body<'tcx>>,
    pub trait_impls: Vec<DefId>,
    pub dictionaries: Vec<DefId>,
    pub import_names: HashMap<Export, String>,
    pub imported: BTreeMap<Export, HashSet<LocalModDefId>>,
    pub thread_local_inits: HashMap<LocalDefId, LocalDefId>,
    pub consts: Vec<LocalDefId>,
    pub codecs: Vec<DefId>,
    pub modules: Vec<LocalModDefId>,
    pub taken: HashMap<LocalModDefId, HashSet<String>>,
    pub fns: HashMap<DefId, FnInfo>,
    /// Keep collecting lowering diagnostics after item-name validation fails.
    pub failed: bool,
    pub called_from_elsewhere: HashSet<DefId>,
    pub tests: Vec<TestFn>,
    pub paths: HashMap<LocalModDefId, Vec<String>>,
    pub mutated: HashSet<Ty<'tcx>>,
    pub changed_vecs: HashSet<Ty<'tcx>>,
    /// Each generic function's type parameters it's given a drop function
    /// for (ADR 0098), by their indices.
    pub drop_params: HashMap<DefId, Vec<u32>>,
    /// Each generic function's type parameters it's given a size, an
    /// alignment or a name of (ADR 0145), by their indices.
    pub type_facts: HashMap<DefId, Vec<(u32, TypeFact)>>,
    pub generic_consts: HashSet<DefId>,
    /// Whether the crate shows anything with `{:#?}`, or asks a `Formatter`
    /// if it's alternate: then its `Debug` functions take whether (ADR 0137).
    pub pretty_debug: bool,
    /// Whether the crate gives a placeholder's options to a value it doesn't
    /// apply them to itself: then its writers take them (ADR 0058).
    pub format_options: bool,
}

pub(super) fn analyze_crate<'a, 'tcx>(
    tcx: TyCtxt<'tcx>,
    all_bodies: &'a [Body<'tcx>],
    dependencies: &crate::library::Dependencies,
    library: bool,
) -> Option<AnalyzedCrate<'a, 'tcx>> {
    let foreign = super::library::Foreign::new(tcx, dependencies);
    if !bindings::validate(tcx)
        || !traits::validate(tcx, &foreign)
        || !super::jsx_api::validate(tcx)
        || !foreign.check()
    {
        return None;
    }
    // With `--test`, rustc adds a harness: a `const` per test, marked
    // `#[rustc_test_marker]`, and a `main` that runs them with libtest. The
    // JS runner takes their place (ADR 0026), so they're left out.
    let markers: Vec<(LocalDefId, Symbol)> = tcx
        .hir_crate_items(())
        .definitions()
        .filter_map(|def_id| find_attr!(tcx, def_id, RustcTestMarker(label) => (def_id, *label)))
        .collect();
    let harness_main = tcx
        .sess
        .opts
        .test
        .then(|| tcx.entry_fn(()).map(|(main, _)| main))
        .flatten();
    let is_harness = |def_id: LocalDefId| {
        let root = tcx.typeck_root_def_id(def_id.to_def_id());
        Some(root) == harness_main || markers.iter().any(|&(marker, _)| marker.to_def_id() == root)
    };
    let all_bodies: Vec<&Body<'tcx>> = all_bodies.iter().filter(|body| !is_harness(body.def_id)).collect();

    if !reject_unsupported(tcx, &foreign, &markers) || !reject_static_references(tcx, &all_bodies) {
        return None;
    }

    let trait_impls: Vec<DefId> = tcx
        .hir_crate_items(())
        .definitions()
        .filter(|&id| {
            matches!(tcx.def_kind(id), DefKind::Impl { of_trait: true })
                && (!tcx.is_automatically_derived(id.to_def_id())
                    || derived_debug(tcx, id.to_def_id())
                    || serde_impl(tcx, id.to_def_id()).is_some())
                && (!from_serde_derive(tcx, id) || serde_impl(tcx, id.to_def_id()).is_some())
        })
        .map(|id| id.to_def_id())
        .collect();
    // The impls that get a dictionary: not `From`'s (ADR 0052), nor serde's,
    // whose evidence is a codec (ADR 0081).
    let dictionaries: Vec<DefId> = trait_impls
        .iter()
        .copied()
        .filter(|&id| {
            traits::operational(
                tcx,
                &foreign,
                tcx.impl_trait_ref(id)
                    .instantiate_identity()
                    .skip_normalization()
                    .def_id,
            )
        })
        .filter(|&id| serde_impl(tcx, id).is_none())
        .collect();

    // Closures are lowered inside the function that creates them.
    let (bodies, closures): (Vec<&Body<'tcx>>, Vec<&Body<'tcx>>) = all_bodies
        .iter()
        .partition(|body| matches!(tcx.def_kind(body.def_id), DefKind::Fn | DefKind::AssocFn));
    let all_bodies = &all_bodies;
    let closures: HashMap<LocalDefId, &Body<'tcx>> = closures.into_iter().map(|b| (b.def_id, b)).collect();

    let mut uses = js_uses(tcx, all_bodies);
    // What the crate's libraries export (ADR 0100) is named before lowering,
    // and imported by the modules that turn out to use it.
    for imported in foreign.all() {
        let export = (imported.from.clone(), imported.export.clone());
        uses.bound_to.entry(export.clone()).or_default();
        uses.imported.entry(export).or_default();
    }

    // Each thread-local's `init` function: lowered like any function, its
    // body is the variable's value.
    let thread_local_inits: HashMap<LocalDefId, LocalDefId> = bodies
        .iter()
        .filter(|body| tcx.def_kind(body.def_id) == DefKind::Fn)
        .filter_map(|body| Some((body.def_id, in_thread_local(tcx, body.def_id)?)))
        .collect();

    // `const` items (ADR 0031) and statics (ADR 0096), with the values rustc
    // has computed. One in a function goes beside it, in its module.
    let consts: Vec<LocalDefId> = tcx
        .hir_crate_items(())
        .definitions()
        .filter(|&d| match tcx.def_kind(d) {
            // Not a `js::import!`'s, which rust-js reads (ADR 0110).
            DefKind::Const { .. } => {
                !markers.iter().any(|&(m, _)| m == d)
                    && !from_serde_derive(tcx, d)
                    && !bindings::is_mark(tcx, d.to_def_id())
            }
            // Not std's storage for a thread-local, which JS needs none of.
            DefKind::Static { .. } => !tcx.is_foreign_item(d) && in_thread_local(tcx, d).is_none(),
            _ => false,
        })
        .collect();

    // A derived `Serialize`'s `serialize` and `Deserialize`'s `deserialize`,
    // which rust-js writes (ADRs 0077 and 0078).
    let codecs: Vec<DefId> = trait_impls
        .iter()
        .filter(|&&id| serde_impl(tcx, id).is_some())
        .map(|&id| tcx.associated_item_def_ids(id)[0])
        .collect();
    // What gets a JS name: functions and methods, `const`s, dictionaries,
    // and codecs.
    let items: Vec<LocalDefId> = bodies
        .iter()
        .map(|body| body.def_id)
        .chain(consts.iter().copied())
        .chain(dictionaries.iter().map(|id| id.expect_local()))
        .chain(codecs.iter().map(|id| id.expect_local()))
        .collect();
    // The modules that get a JS file: the root, then every module with one of
    // those, in the order the first one appears.
    let mut modules = vec![LocalModDefId::CRATE_DEF_ID];
    let mut seen_modules = HashSet::from([LocalModDefId::CRATE_DEF_ID]);
    for &def_id in &items {
        let module = tcx.parent_module_from_def_id(def_id);
        if seen_modules.insert(module) {
            modules.push(module);
        }
    }

    // The crate's own names first: an export is what its consumers and JS
    // call it by. An import is named around every one of them.
    let (mut taken, fns, failed) = name_items(tcx, &items, &modules, &uses.globals, &trait_impls);
    let import_names = name_imports(tcx, &uses, &taken);
    for names in taken.values_mut() {
        names.extend(import_names.values().cloned());
    }
    let imported = uses.imported;
    let mut called_from_elsewhere = exported_across_modules(tcx, all_bodies, &fns);
    let tests = collect_tests(tcx, &markers, &bodies, &fns, &mut called_from_elsewhere);

    let paths: HashMap<LocalModDefId, Vec<String>> = modules.iter().map(|&m| (m, module_path(tcx, m))).collect();

    let mutated = mutated_types(tcx, all_bodies);
    let changed_vecs = changed_vecs(tcx, all_bodies);
    let drop_params = drop_params(tcx, all_bodies, &fns, &foreign, library);
    let type_facts = type_fact_params(tcx, all_bodies, &fns, library);
    let generic_consts = generic_consts(tcx, all_bodies);
    let pretty_debug = uses_pretty_debug(tcx, all_bodies);
    let format_options = uses_format_options(tcx, all_bodies);

    Some(AnalyzedCrate {
        bodies,
        closures,
        trait_impls,
        dictionaries,
        import_names,
        imported,
        thread_local_inits,
        consts,
        codecs,
        modules,
        taken,
        fns,
        failed,
        called_from_elsewhere,
        tests,
        paths,
        mutated,
        changed_vecs,
        drop_params,
        type_facts,
        generic_consts,
        pretty_debug,
        format_options,
    })
}

/// The tests: each marker names a function beside it, of the same name.
/// The test file imports them, so they're `exported` too.
fn collect_tests<'tcx>(
    tcx: TyCtxt<'tcx>,
    markers: &[(LocalDefId, Symbol)],
    bodies: &[&Body<'tcx>],
    fns: &HashMap<DefId, FnInfo>,
    exported: &mut HashSet<DefId>,
) -> Vec<TestFn> {
    let mut tests = Vec::new();
    for &(marker, label) in markers {
        let module = tcx.parent_module_from_def_id(marker);
        let name = tcx.item_name(marker.to_def_id());
        let Some(test) = bodies
            .iter()
            .map(|body| body.def_id)
            .find(|&f| tcx.parent_module_from_def_id(f) == module && tcx.item_name(f.to_def_id()) == name)
        else {
            continue;
        };
        exported.insert(test.to_def_id());
        let should_panic = find_attr!(tcx, test, ShouldPanic { reason, .. } => reason.map(|r| r.to_string()));
        tests.push(TestFn {
            module: module_path(tcx, module),
            name: fns[&test.to_def_id()].name.clone(),
            label: label.to_string(),
            should_panic,
            ignore: find_attr!(tcx, test, Ignore { .. }),
        });
    }
    tests
}

/// The trait constants generic code reads, `S::SIDES` (ADR 0106): a
/// dictionary has only these, as rustc evaluates only the constants a
/// program uses, and a default no impl uses may not evaluate.
fn generic_consts(tcx: TyCtxt<'_>, all_bodies: &[&Body<'_>]) -> HashSet<DefId> {
    all_bodies
        .iter()
        .flat_map(|body| body.thir.exprs.iter())
        .filter_map(|expr| match expr.kind {
            ExprKind::NamedConst { def_id, args, .. }
                if tcx.trait_of_assoc(def_id).is_some() && args.has_non_region_param() =>
            {
                Some(def_id)
            }
            _ => None,
        })
        .collect()
}

/// The `Vec` types something takes `&mut` of: `push`, `sort`, `v[i] = x`
/// and every other change to one does. A clone of any other `Vec` can be
/// the same array (ADR 0052).
fn changed_vecs<'tcx>(tcx: TyCtxt<'tcx>, all_bodies: &[&Body<'tcx>]) -> HashSet<Ty<'tcx>> {
    let mut changed = HashSet::new();
    for body in all_bodies {
        for expr in body.thir.exprs.iter() {
            if let ExprKind::Borrow {
                borrow_kind: BorrowKind::Mut { .. },
                arg,
            } = expr.kind
                && let ty = body.thir[arg].ty
                && matches!(ty.kind(), ty::Adt(adt, _) if [StdItem::Vec, StdItem::VecDeque, StdItem::BinaryHeap].into_iter().any(|item| is_std_def(tcx, adt.did(), item)))
            {
                changed.insert(ty);
            }
        }
    }
    changed
}
