//! Collect crate facts before emission; no function lowering or linking.

use super::bindings;
use super::bindings::{Export, is_binding, js_import, js_path, module_binding};
use super::traits;
use super::{Body, FnInfo, TestFn, camel_case, fresh_in, module_path, strip};
use rustc_hir::def::DefKind;
use rustc_hir::{LangItem, find_attr};
use rustc_middle::middle::codegen_fn_attrs::CodegenFnAttrFlags;
use rustc_middle::mir::BorrowKind;
use rustc_middle::thir::{ExprId, ExprKind, Thir};
use rustc_middle::ty;
use rustc_middle::ty::{Ty, TyCtxt, TypeVisitableExt};
use rustc_span::def_id::{DefId, LocalDefId, LocalModDefId};
use rustc_span::{Symbol, sym};
use std::collections::{BTreeMap, HashMap, HashSet};

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
    pub generic_consts: HashSet<DefId>,
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
            DefKind::Const { .. } => !markers.iter().any(|&(m, _)| m == d) && !from_serde_derive(tcx, d),
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

    let mutated = mutated_types(all_bodies);
    let changed_vecs = changed_vecs(tcx, all_bodies);
    let drop_params = drop_params(tcx, all_bodies, &fns, &foreign, library);
    let generic_consts = generic_consts(tcx, all_bodies);

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
        generic_consts,
    })
}

/// `thread_local!` (ADR 0037) is a `const NAME: LocalKey<T>` whose block holds
/// `fn __rust_std_internal_init_fn() -> T { init }`, then std's storage for
/// it. In JS, it's a variable of its module, made from `init`.
pub(super) fn is_thread_local(tcx: TyCtxt<'_>, d: LocalDefId) -> bool {
    matches!(tcx.def_kind(d), DefKind::Const { .. })
        && matches!(tcx.type_of(d).instantiate_identity().skip_normalization().kind(), ty::Adt(adt, _) if tcx.is_diagnostic_item(Symbol::intern("LocalKey"), adt.did()))
}

/// The thread-local whose block `d` is in, if any.
fn in_thread_local(tcx: TyCtxt<'_>, d: LocalDefId) -> Option<LocalDefId> {
    let mut parent = tcx.opt_local_parent(d);
    while let Some(p) = parent {
        if is_thread_local(tcx, p) {
            return Some(p);
        }
        parent = tcx.opt_local_parent(p);
    }
    None
}

/// Report each item rust-js can't compile yet. False if there was one.
fn reject_unsupported(
    tcx: TyCtxt<'_>,
    foreign: &super::library::Foreign<'_, '_>,
    markers: &[(LocalDefId, Symbol)],
) -> bool {
    let mut valid = true;
    // What the crate exports by a symbol of its own, `#[no_mangle]` or
    // `#[export_name]`: an `extern` declaration of one is a JS binding
    // naming a global no JS has, and the function is the crate's, by its path.
    let exported: HashSet<Symbol> = tcx
        .hir_crate_items(())
        .definitions()
        .filter(|&id| matches!(tcx.def_kind(id), DefKind::Fn | DefKind::AssocFn) && !tcx.is_foreign_item(id))
        .filter_map(|id| {
            let attrs = tcx.codegen_fn_attrs(id);
            match attrs.flags.contains(CodegenFnAttrFlags::NO_MANGLE) {
                true => Some(attrs.symbol_name.unwrap_or_else(|| tcx.item_name(id.to_def_id()))),
                false => attrs.symbol_name,
            }
        })
        .collect();
    for def_id in tcx.hir_crate_items(()).definitions() {
        let what = match tcx.def_kind(def_id) {
            DefKind::Fn
                if tcx.is_foreign_item(def_id)
                    && exported.contains(
                        &tcx.codegen_fn_attrs(def_id)
                            .symbol_name
                            .unwrap_or_else(|| tcx.item_name(def_id.to_def_id())),
                    ) =>
            {
                "an `extern` declaration of this crate's own `#[no_mangle]` function"
            }
            // `#[eii] static HELLO: u64;`, which the linker makes another
            // item: rust-js has none to link it to, and JS would read a name
            // nothing defines (ADR 0109).
            _ if find_attr!(tcx, def_id, EiiImpls(..) | EiiDeclaration(..) | RustcEiiForeignItem) => {
                "externally implementable items"
            }
            _ if markers.iter().any(|&(marker, _)| marker == def_id) => continue,
            _ if from_serde_derive(tcx, def_id) => continue,
            // std's storage for a thread-local: JS needs none.
            _ if in_thread_local(tcx, def_id).is_some() => continue,
            // Methods, trait impls' included (ADRs 0047, 0049). Derives like
            // `#[derive(Clone)]` write impls that are never called.
            DefKind::AssocFn => continue,
            // A `type const`, of `min_generic_const_args`, has no body to type-check.
            DefKind::AssocConst { is_type_const: true } => "type constants",
            // A type's own `const`, as `Vec2::ZERO`: its value where it's used,
            // as rustc computed it (ADR 0031). A trait's too, and in generic code
            // its impl's dictionary's (ADR 0106). Not one with parameters of its own.
            DefKind::AssocConst { .. } if tcx.generics_of(def_id).own_params.is_empty() => continue,
            DefKind::AssocConst { .. } => "generic constants",
            // An `Iterator`'s `Item` (ADR 0055), an operator's `Output` (ADR
            // 0064) and a `TryFrom`'s `Error`: rustc works out what they are.
            DefKind::AssocTy
                if tcx.trait_impl_of_assoc(def_id.to_def_id()).is_some_and(|imp| {
                    let tr = tcx
                        .impl_trait_ref(imp)
                        .instantiate_identity()
                        .skip_normalization()
                        .def_id;
                    tcx.is_diagnostic_item(sym::Iterator, tr)
                        || tcx.is_diagnostic_item(sym::TryFrom, tr)
                        || traits::is_operator(tcx, tr)
                }) =>
            {
                continue;
            }
            // A trait's own, a type only a caller knows in generic code, as a
            // type parameter is (ADR 0106). Not one with parameters of its own.
            DefKind::AssocTy if tcx.generics_of(def_id).own_params.is_empty() => continue,
            DefKind::AssocTy => "generic associated types",
            DefKind::Impl { of_trait: true }
                if !tcx.is_automatically_derived(def_id.to_def_id())
                    && !traits::implementable(
                        tcx,
                        foreign,
                        tcx.impl_trait_ref(def_id)
                            .instantiate_identity()
                            .skip_normalization()
                            .def_id,
                    ) =>
            {
                "user implementations of this standard or external trait"
            }
            DefKind::Static { .. } if tcx.is_thread_local_static(def_id.to_def_id()) => "`#[thread_local]` statics",
            _ => continue,
        };
        tcx.dcx()
            .span_err(tcx.def_span(def_id), format!("rust-js does not support {what} yet"));
        valid = false;
    }
    valid
}

/// The type parameters of the crate's own generic functions that a caller
/// gives a value with a destructor (ADR 0098), directly or through a
/// generic function of its own: those functions drop a `T` through a drop
/// function they're given. Only they are, so generic code nothing gives such
/// a value to is what it was.
fn drop_params<'tcx>(
    tcx: TyCtxt<'tcx>,
    all_bodies: &[&Body<'tcx>],
    fns: &HashMap<DefId, FnInfo>,
    foreign: &super::library::Foreign<'_, 'tcx>,
    library: bool,
) -> HashMap<DefId, Vec<u32>> {
    let mut given: HashSet<(DefId, u32)> = HashSet::new();
    // A trait impl's methods are called through its dictionary, or resolved
    // where they're called, by callers the walk below can't see (ADRs 0098,
    // 0100), and what one drops needn't be in its own body: a helper it lends
    // a value to may, or a std method, or a default the trait wrote. So a
    // generic impl is given a drop for each type parameter that isn't `Copy`,
    // and so are its methods, unless it's a derive whose body drops none.
    let impls: HashSet<DefId> = fns
        .keys()
        .filter_map(|&id| {
            tcx.trait_impl_of_assoc(id)
                .or((tcx.def_kind(id) == DefKind::Impl { of_trait: true }).then_some(id))
        })
        .collect();
    for imp in impls {
        if drops_nothing_derived(tcx, imp) {
            continue;
        }
        let typing_env = ty::TypingEnv::non_body_analysis(tcx, imp);
        let owners: Vec<DefId> = std::iter::once(imp)
            .chain(tcx.associated_item_def_ids(imp).iter().copied())
            .collect();
        for param in &tcx.generics_of(imp).own_params {
            if let ty::GenericParamDefKind::Type { .. } = param.kind
                && !tcx.type_is_copy_modulo_regions(typing_env, Ty::new_param(tcx, param.index, param.name))
            {
                for &id in &owners {
                    given.insert((id, param.index));
                }
            }
        }
    }
    // A trait's default body is copied into each impl that keeps it (ADR 0049),
    // and dropped there as the impl's type drops: what it gives a generic
    // function of the crate's, as `discard(self)`, that function must be able
    // to drop. So its `Self`, and each type parameter of its trait that isn't
    // `Copy`, count as given a drop, here to be passed on.
    for body in all_bodies {
        let method = tcx.typeck_root_def_id(body.def_id.to_def_id());
        let Some(trait_id) = tcx.trait_of_assoc(method) else {
            continue;
        };
        let typing_env = ty::TypingEnv::non_body_analysis(tcx, method);
        for param in &tcx.generics_of(trait_id).own_params {
            if let ty::GenericParamDefKind::Type { .. } = param.kind
                && !tcx.type_is_copy_modulo_regions(typing_env, Ty::new_param(tcx, param.index, param.name))
            {
                given.insert((method, param.index));
            }
        }
    }
    // A library's consumers are callers it never sees (ADR 0100): a function
    // of it they can reach is given a drop for each type parameter they could
    // give a value with a destructor, one that isn't `Copy`.
    if library {
        for &id in fns.keys() {
            // A trait impl's, and its methods, are decided above.
            if tcx.trait_of_assoc(id).is_some()
                || tcx.trait_impl_of_assoc(id).is_some()
                || matches!(tcx.def_kind(id), DefKind::Impl { .. })
                || !super::library::reachable(tcx, id)
            {
                continue;
            }
            let typing_env = ty::TypingEnv::non_body_analysis(tcx, id);
            let generics = tcx.generics_of(id);
            for index in 0..generics.count() {
                let param = generics.param_at(index, tcx);
                if let ty::GenericParamDefKind::Type { .. } = param.kind
                    && !tcx.type_is_copy_modulo_regions(typing_env, Ty::new_param(tcx, param.index, param.name))
                {
                    given.insert((id, index as u32));
                }
            }
        }
    }
    // A caller's type parameter passed on as a callee's: `relay<U>` calling `consume::<U>`.
    let mut passed: Vec<((DefId, u32), (DefId, u32))> = Vec::new();
    for body in all_bodies {
        let caller = tcx.typeck_root_def_id(body.def_id.to_def_id());
        for expr in body.thir.exprs.iter() {
            let (ExprKind::ZstLiteral { .. }, &ty::FnDef(callee, args)) = (&expr.kind, expr.ty.kind()) else {
                continue;
            };
            if !fns.contains_key(&callee) || tcx.trait_of_assoc(callee).is_some() {
                continue;
            }
            for (index, arg) in args.iter().enumerate() {
                let Some(ty) = arg.as_type() else { continue };
                let index = index as u32;
                if holds_user_drop(tcx, foreign, ty, &mut Vec::new()) {
                    given.insert((callee, index));
                }
                for part in ty.walk() {
                    if let Some(part) = part.as_type()
                        && let ty::Param(param) = part.kind()
                    {
                        passed.push(((caller, param.index), (callee, index)));
                    }
                }
            }
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for &(from, to) in &passed {
            if given.contains(&from) && given.insert(to) {
                changed = true;
            }
        }
    }
    let mut params: HashMap<DefId, Vec<u32>> = HashMap::new();
    for (def, index) in given {
        params.entry(def).or_default().push(index);
    }
    for indices in params.values_mut() {
        indices.sort();
    }
    params
}

/// Is `imp` a derive whose body drops nothing of what it's given or makes:
/// `Clone`'s, `Debug`'s, `Default`'s and the comparisons', or serde's, whose
/// codecs rust-js writes (ADR 0077)?
fn drops_nothing_derived(tcx: TyCtxt<'_>, imp: DefId) -> bool {
    if !tcx.is_automatically_derived(imp) {
        return false;
    }
    let tr = tcx
        .impl_trait_ref(imp)
        .instantiate_identity()
        .skip_normalization()
        .def_id;
    [
        LangItem::Clone,
        LangItem::Copy,
        LangItem::PartialEq,
        LangItem::PartialOrd,
    ]
    .into_iter()
    .any(|item| tcx.is_lang_item(tr, item))
        || [
            sym::Eq,
            sym::Ord,
            sym::Hash,
            Symbol::intern("Debug"),
            Symbol::intern("Default"),
        ]
        .into_iter()
        .any(|name| tcx.is_diagnostic_item(name, tr))
        || serde_impl(tcx, imp).is_some()
}

/// Whether dropping a `ty` could run a `Drop` of the crate's own, through
/// its fields, variants or what it holds.
fn holds_user_drop<'tcx>(
    tcx: TyCtxt<'tcx>,
    foreign: &super::library::Foreign<'_, 'tcx>,
    ty: Ty<'tcx>,
    seen: &mut Vec<Ty<'tcx>>,
) -> bool {
    if seen.contains(&ty) {
        return false;
    }
    seen.push(ty);
    let found = match ty.kind() {
        ty::Adt(adt, args) => {
            tcx.adt_destructor(adt.did())
                .is_some_and(|d| d.did.is_local() || foreign.item(d.did).is_some())
                || adt
                    .all_fields()
                    .any(|f| holds_user_drop(tcx, foreign, f.ty(tcx, args).skip_normalization(), seen))
                || args.types().any(|t| holds_user_drop(tcx, foreign, t, seen))
        }
        ty::Tuple(items) => items.iter().any(|t| holds_user_drop(tcx, foreign, t, seen)),
        ty::Array(item, _) | ty::Slice(item) => holds_user_drop(tcx, foreign, *item, seen),
        ty::Closure(_, args) => args
            .as_closure()
            .upvar_tys()
            .iter()
            .any(|t| holds_user_drop(tcx, foreign, t, seen)),
        _ => false,
    };
    seen.pop();
    found
}

/// Report each reference rust-js can't make to a static yet (ADR 0096): a
/// `&mut` to a `static mut` or a part of one, and a raw address of any. A
/// shared one is the value it points to, as any is. False if there was one.
fn reject_static_references(tcx: TyCtxt<'_>, all_bodies: &[&Body<'_>]) -> bool {
    let mut valid = true;
    for body in all_bodies {
        let thir = &body.thir;
        for expr in thir.exprs.iter() {
            let (arg, raw) = match expr.kind {
                ExprKind::Borrow {
                    borrow_kind: BorrowKind::Mut { .. },
                    arg,
                } => (arg, false),
                ExprKind::RawBorrow { arg, .. } => (arg, true),
                _ => continue,
            };
            let what = match static_of(thir, arg) {
                Some(d) if tcx.is_foreign_item(d) => continue,
                Some(_) if raw => "raw addresses of statics",
                Some(d) if tcx.is_mutable_static(d) => "`&mut` references to a `static mut`",
                _ => continue,
            };
            tcx.dcx()
                .span_err(expr.span, format!("rust-js does not support {what} yet"));
            valid = false;
        }
    }
    valid
}

/// The static that place `e` is, or is a part of.
fn static_of(thir: &Thir<'_>, e: ExprId) -> Option<DefId> {
    match thir[strip(thir, e)].kind {
        ExprKind::Field { lhs, .. } | ExprKind::Index { lhs, .. } => static_of(thir, lhs),
        ExprKind::Deref { arg } => match thir[strip(thir, arg)].kind {
            ExprKind::StaticRef { def_id, .. } => Some(def_id),
            _ => None,
        },
        _ => None,
    }
}

/// What of JS the crate's bodies use (ADRs 0024, 0028).
struct JsUses {
    /// JS globals, whether declared here or in another crate (`web`): every
    /// module reserves them, so a local named `console` can't hide the real one.
    globals: HashSet<String>,
    /// Exports of JS modules, with the modules that use each.
    imported: BTreeMap<Export, HashSet<LocalModDefId>>,
    /// What each import is bound to in Rust, to name a default import after.
    bound_to: HashMap<Export, HashSet<DefId>>,
}

fn js_uses<'tcx>(tcx: TyCtxt<'tcx>, all_bodies: &[&Body<'tcx>]) -> JsUses {
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
fn name_imports(
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
fn item_js_name(tcx: TyCtxt<'_>, def_id: DefId, trait_impls: &[DefId]) -> String {
    if trait_impls.contains(&def_id) {
        return traits::impl_name(tcx, def_id);
    }
    if tcx.def_kind(def_id) == DefKind::AssocFn && tcx.inherent_impl_of_assoc(def_id).is_none() {
        let parent = tcx.parent(def_id);
        let prefix = if trait_impls.contains(&parent) {
            traits::impl_name(tcx, parent)
        } else {
            super::lower_first(tcx.item_name(parent).as_str())
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
fn name_items(
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
fn exported_across_modules<'tcx>(
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

/// The types whose JS objects get changed in place somewhere in the crate:
/// `a.b.c = ..` changes the object `a.b`, so it's `a.b`'s type. Only these
/// ever need copying (ADR 0020).
fn mutated_types<'tcx>(all_bodies: &[&Body<'tcx>]) -> HashSet<Ty<'tcx>> {
    let mut mutated = HashSet::new();
    for body in all_bodies {
        for expr in body.thir.exprs.iter() {
            // An enum something takes `&mut` of, or matches with a `ref mut`
            // binding, may have a variant's field changed through it.
            let enum_ty = |ty: Ty<'tcx>| matches!(ty.peel_refs().kind(), ty::Adt(adt, _) if adt.is_enum());
            match expr.kind {
                ExprKind::Borrow {
                    borrow_kind: BorrowKind::Mut { .. },
                    arg,
                } if enum_ty(body.thir[arg].ty) => {
                    mutated.insert(body.thir[arg].ty.peel_refs());
                }
                ExprKind::Match {
                    scrutinee, ref arms, ..
                } if enum_ty(body.thir[scrutinee].ty)
                    && arms.iter().any(|&arm| binds_ref_mut(&body.thir[arm].pattern)) =>
                {
                    mutated.insert(body.thir[scrutinee].ty.peel_refs());
                }
                ExprKind::Let {
                    expr: scrutinee,
                    ref pat,
                } if enum_ty(body.thir[scrutinee].ty) && binds_ref_mut(pat) => {
                    mutated.insert(body.thir[scrutinee].ty.peel_refs());
                }
                _ => {}
            }
            // `a[i] = ..` changes the array `a` the same way.
            if let ExprKind::Assign { lhs, .. } | ExprKind::AssignOp { lhs, .. } = expr.kind
                && let ExprKind::Field { lhs: object, .. } | ExprKind::Index { lhs: object, .. } =
                    body.thir[strip(&body.thir, lhs)].kind
            {
                mutated.insert(body.thir[object].ty);
            }
        }
    }
    mutated
}

/// Does `pat` bind a variable by `ref mut`, or through a `&mut` subject?
fn binds_ref_mut(pat: &rustc_middle::thir::Pat<'_>) -> bool {
    let mut found = false;
    pat.walk_always(|p| {
        if let rustc_middle::thir::PatKind::Binding { mode, .. } = p.kind
            && matches!(mode.0, rustc_hir::ByRef::Yes(_, rustc_ast::Mutability::Mut))
        {
            found = true;
        }
    });
    found
}

/// A `#[derive(Debug)]` impl: lowered, since it's how `{:?}` shows its type.
pub(super) fn derived_debug(tcx: TyCtxt<'_>, id: DefId) -> bool {
    tcx.is_automatically_derived(id)
        && matches!(tcx.def_kind(id), DefKind::Impl { of_trait: true })
        && tcx.is_diagnostic_item(
            Symbol::intern("Debug"),
            tcx.impl_trait_ref(id)
                .instantiate_identity()
                .skip_normalization()
                .def_id,
        )
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
                && matches!(ty.kind(), ty::Adt(adt, _) if [sym::Vec, Symbol::intern("VecDeque"), Symbol::intern("BinaryHeap")].into_iter().any(|name| tcx.is_diagnostic_item(name, adt.did())))
            {
                changed.insert(ty);
            }
        }
    }
    changed
}
