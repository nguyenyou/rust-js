//! Collect crate facts and orchestrate lowering; no filesystem writes.

use super::bindings;
use super::bindings::{Export, is_binding, js_import, js_path, module_binding};
use super::{
    Body, CrateFacts, FnCx, FnInfo, Lowered, LoweredModule, TestFn, camel_case, const_js, eval_const, fresh_in,
    module_file, module_path, strip,
};
use super::{link, traits};
use crate::js;
use crate::js::{Expr, StmtKind};
use crate::runtime::Helper;
use rustc_hir::def::DefKind;
use rustc_hir::find_attr;
use rustc_middle::mir::BorrowKind;
use rustc_middle::thir::ExprKind;
use rustc_middle::ty;
use rustc_middle::ty::{Ty, TyCtxt};
use rustc_span::def_id::{DefId, LocalDefId, LocalModDefId};
use rustc_span::{Symbol, sym};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Copy the THIR of every function and closure in the crate.
///
/// Must run *before* `analysis`: building MIR for borrowck consumes ("steals")
/// the THIR, so this is our only chance to read it.
pub fn collect_bodies(tcx: TyCtxt<'_>) -> Vec<Body<'_>> {
    let items = tcx.hir_crate_items(());
    items
        .definitions()
        .chain(items.nested_bodies())
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
            Some(Body { def_id, thir, expr })
        })
        .collect()
}

/// Retained functions and their dependencies, grouped for output.
#[derive(Default)]
struct Pass {
    functions: HashMap<LocalModDefId, Vec<js::Function>>,
    namespaces: HashMap<LocalModDefId, Vec<js::Namespace>>,
    runtime: HashMap<LocalModDefId, HashSet<Helper>>,
    jsx: HashSet<LocalModDefId>,
    caches: HashMap<LocalModDefId, Vec<String>>,
    /// `thread_local!`s' values, made from their lowered `init`s.
    local_consts: HashMap<LocalModDefId, Vec<js::Const>>,
    /// Which items each module uses from another, and which JS imports.
    references: HashSet<(LocalModDefId, DefId)>,
    package_uses: HashSet<(LocalModDefId, Export)>,
}

/// Lower every function, grouped by module. Reports all unsupported
/// features as rustc errors.
pub fn lower_crate<'tcx>(tcx: TyCtxt<'tcx>, all_bodies: &[Body<'tcx>]) -> Option<Lowered> {
    if !bindings::validate(tcx) || !traits::validate(tcx) {
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

    if !reject_unsupported(tcx, &markers) {
        return None;
    }

    let trait_impls: Vec<DefId> = tcx
        .hir_crate_items(())
        .definitions()
        .filter(|&id| {
            matches!(tcx.def_kind(id), DefKind::Impl { of_trait: true })
                && (!tcx.is_automatically_derived(id.to_def_id()) || derived_debug(tcx, id.to_def_id()))
        })
        .map(|id| id.to_def_id())
        .collect();
    // The impls that get a dictionary: not `From`'s (ADR 0052).
    let dictionaries: Vec<DefId> = trait_impls
        .iter()
        .copied()
        .filter(|&id| traits::operational(tcx, tcx.impl_trait_ref(id).instantiate_identity().def_id))
        .collect();

    // Closures are lowered inside the function that creates them.
    let (bodies, closures): (Vec<&Body<'tcx>>, Vec<&Body<'tcx>>) = all_bodies
        .iter()
        .partition(|body| matches!(tcx.def_kind(body.def_id), DefKind::Fn | DefKind::AssocFn));
    let all_bodies = &all_bodies;
    let closures: HashMap<LocalDefId, &Body<'tcx>> = closures.into_iter().map(|b| (b.def_id, b)).collect();

    let uses = js_uses(tcx, all_bodies);
    let (import_names, globals) = name_imports(tcx, &uses);
    let imported = uses.imported;

    // Each thread-local's `init` function: lowered like any function, its
    // body is the variable's value.
    let thread_local_inits: HashMap<LocalDefId, LocalDefId> = bodies
        .iter()
        .filter(|body| tcx.def_kind(body.def_id) == DefKind::Fn)
        .filter_map(|body| Some((body.def_id, in_thread_local(tcx, body.def_id)?)))
        .collect();

    // `const` items (ADR 0031), with the values rustc has computed. One in a
    // function goes beside it, in its module.
    let consts: Vec<LocalDefId> = tcx
        .hir_crate_items(())
        .definitions()
        .filter(|&d| matches!(tcx.def_kind(d), DefKind::Const { .. }) && !markers.iter().any(|&(m, _)| m == d))
        .collect();

    // What gets a JS name: functions and methods, `const`s, and dictionaries.
    let items: Vec<LocalDefId> = bodies
        .iter()
        .map(|body| body.def_id)
        .chain(consts.iter().copied())
        .chain(dictionaries.iter().map(|id| id.expect_local()))
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

    let (taken, fns, mut failed) = name_items(tcx, &items, &modules, &globals, &trait_impls);
    let mut called_from_elsewhere = exported_across_modules(tcx, all_bodies, &fns);
    let tests = collect_tests(tcx, &markers, &bodies, &fns, &mut called_from_elsewhere);

    let paths: HashMap<LocalModDefId, Vec<String>> = modules.iter().map(|&m| (m, module_path(tcx, m))).collect();

    let mutated = mutated_types(all_bodies);
    let changed_vecs = changed_vecs(tcx, all_bodies);

    let mut const_items: HashMap<LocalModDefId, Vec<js::Const>> = HashMap::new();
    for &def_id in consts.iter().filter(|&&d| !is_thread_local(tcx, d)) {
        let span = tcx.def_span(def_id);
        let typing_env = ty::TypingEnv::fully_monomorphized();
        let args = ty::GenericArgs::identity_for_item(tcx, def_id);
        let Some(value) = eval_const(tcx, typing_env, def_id.to_def_id(), args, span).and_then(|v| const_js(tcx, v))
        else {
            let ty = tcx.type_of(def_id).instantiate_identity();
            tcx.dcx()
                .span_err(span, format!("rust-js does not support constants of type `{ty}` yet"));
            failed = true;
            continue;
        };
        let info = &fns[&def_id.to_def_id()];
        let file = module_file(tcx, info.module);
        let span = span.source_callsite();
        const_items.entry(info.module).or_default().push(js::Const {
            name: info.name.clone(),
            value,
            export: tcx.visibility(def_id).is_public() || called_from_elsewhere.contains(&def_id.to_def_id()),
            span: js::Span {
                lo: (span.lo() - file.start_pos).0,
                hi: (span.hi() - file.start_pos).0,
            },
        });
    }

    let function_bodies = bodies.iter().map(|body| (body.def_id.to_def_id(), *body)).collect();
    // A trait impl's dictionary (ADR 0049) has no body of its own, so its
    // function context gets an empty one; a copied default brings its own.
    let no_body = rustc_middle::thir::Thir::new(rustc_middle::thir::BodyTy::Const(tcx.types.unit));
    // Lower each body once. Cross-module references have symbolic names until
    // the link step knows exactly which imports and local names survive.
    let mut lowered_items = Vec::new();
    let crate_facts = CrateFacts {
        mutated: &mutated,
        changed_vecs: &changed_vecs,
        closures: &closures,
        bodies: &function_bodies,
        fns: &fns,
        imports: &import_names,
        trait_impls: &trait_impls,
    };
    for (def_id, body) in bodies
        .iter()
        .filter(|b| tcx.trait_of_assoc(b.def_id.to_def_id()).is_none())
        .map(|b| (b.def_id.to_def_id(), Some(*b)))
        .chain(dictionaries.iter().map(|id| (*id, None)))
    {
        let module = fns[&def_id].module;
        let file = module_file(tcx, module);
        let mut cx = FnCx {
            tcx,
            typing_env: ty::TypingEnv::post_analysis(tcx, def_id),
            evidence: Vec::new(),
            self_args: None,
            krate: &crate_facts,
            dependencies: Default::default(),
            captures: HashMap::new(),
            file_start: file.start_pos,
            file_end: file.end_position(),
            thir: body.map_or(&no_body, |body| &body.thir),
            module,
            vars: HashMap::new(),
            // Locals must never shadow a function or an import of this file.
            names: taken[&module].clone(),
            module_names: &taken[&module],
            labels: HashSet::new(),
            loops: Vec::new(),
            runtime: HashSet::new(),
            jsx: false,
            writer: None,
            slots: HashMap::new(),
            stepped: body.map_or_else(HashSet::new, |body| super::stepped_locals(tcx, &body.thir)),
            iterators: HashSet::new(),
            boxes: HashSet::new(),
            cloning: Vec::new(),
            discarded: false,
            item: def_id,
        };
        let result = match body {
            Some(body) => cx.lower_fn(body),
            None => {
                let cache = format!("${}", fns[&def_id].name);
                cx.lower_dictionary(def_id, &cache).map(|function| super::LoweredFn {
                    function,
                    runtime: std::mem::take(&mut cx.runtime),
                    jsx: cx.jsx,
                    dependencies: cx.dependencies.take(),
                })
            }
        };
        match result {
            Ok(lowered) => lowered_items.push((def_id, lowered)),
            Err(_) => failed = true,
        }
    }
    if failed {
        return None;
    }

    // Reachability for derived Debug implementations, with adjacency lists
    // rather than scanning every edge again for every reached function.
    let derived: HashSet<DefId> = trait_impls
        .iter()
        .filter(|&&id| tcx.is_automatically_derived(id))
        .flat_map(|&id| std::iter::once(id).chain(tcx.associated_item_def_ids(id).iter().copied()))
        .collect();
    let mut edges: HashMap<DefId, Vec<DefId>> = HashMap::new();
    let mut todo = Vec::new();
    for (_, lowered) in &lowered_items {
        for &(from, to) in &lowered.dependencies.uses {
            edges.entry(from).or_default().push(to);
            if !derived.contains(&from) {
                todo.push(to);
            }
        }
    }
    let mut reached = HashSet::new();
    while let Some(id) = todo.pop() {
        if reached.insert(id)
            && let Some(targets) = edges.get(&id)
        {
            todo.extend(targets);
        }
    }
    let mut pass = Pass::default();
    for (def_id, mut lowered) in lowered_items
        .into_iter()
        .filter(|(id, _)| !derived.contains(id) || reached.contains(id))
    {
        let module = fns[&def_id].module;
        pass.references.extend(lowered.dependencies.references.drain());
        pass.package_uses.extend(lowered.dependencies.package_uses.drain());
        if dictionaries.contains(&def_id) {
            pass.caches
                .entry(module)
                .or_default()
                .push(format!("${}", fns[&def_id].name));
        }
        match lowered {
            lowered if let Some(&key) = thread_local_inits.get(&def_id.expect_local()) => {
                // `const COUNT = { value: 0 };`: made when the module loads.
                let function = lowered.function;
                let value = match function.body.as_slice() {
                    [
                        js::Stmt {
                            kind: StmtKind::Return(Some(value)),
                            ..
                        },
                    ] => value.clone(),
                    _ => Expr::call(Expr::arrow(Vec::new(), function.body), Vec::new()),
                };
                let info = &fns[&key.to_def_id()];
                let file = module_file(tcx, info.module);
                let span = tcx.def_span(key).source_callsite();
                pass.local_consts.entry(info.module).or_default().push(js::Const {
                    name: info.name.clone(),
                    value,
                    export: tcx.visibility(key).is_public() || called_from_elsewhere.contains(&key.to_def_id()),
                    span: js::Span {
                        lo: (span.lo() - file.start_pos).0,
                        hi: (span.hi() - file.start_pos).0,
                    },
                });
                pass.runtime.entry(module).or_default().extend(lowered.runtime);
                if lowered.jsx {
                    pass.jsx.insert(module);
                }
            }
            mut lowered => {
                if lowered.jsx {
                    pass.jsx.insert(module);
                }
                lowered.function.export |= called_from_elsewhere.contains(&def_id);
                match &fns[&def_id].owner {
                    // Its type's object is exported if any of its methods is.
                    Some(owner) => {
                        let module_namespaces = pass.namespaces.entry(module).or_default();
                        let export = lowered.function.export;
                        match module_namespaces.iter_mut().find(|n| n.name == *owner) {
                            Some(namespace) => {
                                namespace.export |= export;
                                namespace.methods.push(lowered.function);
                            }
                            None => module_namespaces.push(js::Namespace {
                                name: owner.clone(),
                                methods: vec![lowered.function],
                                export,
                            }),
                        }
                    }
                    None => pass.functions.entry(module).or_default().push(lowered.function),
                }
                pass.runtime.entry(module).or_default().extend(lowered.runtime);
            }
        }
    }
    for (module, consts) in pass.local_consts.drain() {
        const_items.entry(module).or_default().extend(consts);
    }

    for &(_, id) in pass.references.iter() {
        let info = &fns[&id];
        if let Some(owner) = &info.owner {
            if let Some(ns) = pass
                .namespaces
                .get_mut(&info.module)
                .and_then(|ns| ns.iter_mut().find(|ns| ns.name == *owner))
            {
                ns.export = true;
            }
        } else if let Some(f) = pass
            .functions
            .get_mut(&info.module)
            .and_then(|fs| fs.iter_mut().find(|f| f.name == info.name))
        {
            f.export = true;
        }
    }
    let mut targets: HashMap<LocalModDefId, HashSet<(LocalModDefId, String)>> = HashMap::new();
    for &(from, id) in &pass.references {
        let info = &fns[&id];
        targets
            .entry(from)
            .or_default()
            .insert((info.module, info.owner.as_ref().unwrap_or(&info.name).clone()));
    }
    let lowered = modules
        .into_iter()
        .map(|module| {
            // One `import` per JS module, of what this file uses from it.
            let mut packages: BTreeMap<&str, js::Package> = BTreeMap::new();
            for export in imported
                .iter()
                .filter(|(export, users)| {
                    users.contains(&module) || pass.package_uses.contains(&(module, (*export).clone()))
                })
                .map(|(export, _)| export)
            {
                let (from, name) = export;
                let package = packages.entry(from).or_insert_with(|| js::Package {
                    from: from.clone(),
                    default: None,
                    named: Vec::new(),
                    namespace: None,
                });
                let local = import_names[export].clone();
                match name.as_str() {
                    "default" => package.default = Some(local),
                    "*" => package.namespace = Some(local),
                    _ => package.named.push((name.clone(), local)),
                }
            }
            let mut packages: Vec<js::Package> = packages.into_values().collect();
            // `#![rust_js::import = "./App.css"]`: `import "./App.css";`, for
            // what a module does when loaded, as a bundler's CSS does.
            for attr in tcx.get_attrs_by_path(module.to_def_id(), &[Symbol::intern("rust_js"), sym::import]) {
                let Some(from) = attr.value_str().map(|s| s.to_string()) else {
                    tcx.dcx()
                        .span_err(attr.span(), "rust-js: write it `#![rust_js::import = \"./file.css\"]`");
                    continue;
                };
                if !packages.iter().any(|p| p.from == from) {
                    packages.push(js::Package {
                        from,
                        default: None,
                        named: Vec::new(),
                        namespace: None,
                    });
                }
            }
            let mut helpers: Vec<Helper> = pass.runtime.remove(&module).unwrap_or_default().into_iter().collect();
            helpers.sort();
            let mut lowered = LoweredModule {
                path: paths[&module].clone(),
                file: module_file(tcx, module),
                packages,
                imports: Vec::new(),
                namespaces: pass.namespaces.remove(&module).unwrap_or_default(),
                consts: const_items.remove(&module).unwrap_or_default(),
                functions: pass.functions.remove(&module).unwrap_or_default(),
                caches: pass.caches.remove(&module).unwrap_or_default(),
                runtime: helpers,
                jsx: pass.jsx.contains(&module),
            };
            let mut imports: Vec<_> = targets.remove(&module).unwrap_or_default().into_iter().collect();
            imports.sort_by(|(a, an), (b, bn)| (&paths[a], an).cmp(&(&paths[b], bn)));
            let candidates: Vec<_> = imports
                .into_iter()
                .map(|(target, export)| (link::symbol(target, &export), export, paths[&target].clone()))
                .collect();
            link::resolve(&mut lowered, &candidates, taken[&module].clone());
            lowered
        })
        .collect();
    tcx.dcx().has_errors().is_none().then_some(Lowered {
        modules: lowered,
        tests,
    })
}

/// `thread_local!` (ADR 0037) is a `const NAME: LocalKey<T>` whose block holds
/// `fn __rust_std_internal_init_fn() -> T { init }`, then std's storage for
/// it. In JS, it's a variable of its module, made from `init`.
fn is_thread_local(tcx: TyCtxt<'_>, d: LocalDefId) -> bool {
    matches!(tcx.def_kind(d), DefKind::Const { .. })
        && matches!(tcx.type_of(d).instantiate_identity().kind(), ty::Adt(adt, _) if tcx.is_diagnostic_item(Symbol::intern("LocalKey"), adt.did()))
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
fn reject_unsupported(tcx: TyCtxt<'_>, markers: &[(LocalDefId, Symbol)]) -> bool {
    let mut valid = true;
    for def_id in tcx.hir_crate_items(()).definitions() {
        let what = match tcx.def_kind(def_id) {
            _ if markers.iter().any(|&(marker, _)| marker == def_id) => continue,
            // std's storage for a thread-local: JS needs none.
            _ if in_thread_local(tcx, def_id).is_some() => continue,
            // Methods, trait impls' included (ADRs 0047, 0049). Derives like
            // `#[derive(Clone)]` write impls that are never called.
            DefKind::AssocFn => continue,
            // A type's own `const`, as `Vec2::ZERO`: its value where it's used,
            // as rustc computed it (ADR 0031). A trait's are still errors.
            DefKind::AssocConst { .. }
                if tcx
                    .opt_local_parent(def_id)
                    .is_some_and(|p| matches!(tcx.def_kind(p), DefKind::Impl { of_trait: false })) =>
            {
                continue;
            }
            DefKind::AssocConst { .. } => "associated constants",
            // An `Iterator`'s `Item` (ADR 0055) and an operator's `Output`
            // (ADR 0064): rustc works out what they are.
            DefKind::AssocTy
                if tcx.trait_impl_of_assoc(def_id.to_def_id()).is_some_and(|imp| {
                    let tr = tcx.impl_trait_ref(imp).instantiate_identity().def_id;
                    tcx.is_diagnostic_item(sym::Iterator, tr) || traits::is_operator(tcx, tr)
                }) =>
            {
                continue;
            }
            DefKind::AssocTy => "associated types",
            DefKind::Impl { of_trait: true }
                if !tcx.is_automatically_derived(def_id.to_def_id())
                    && !traits::implementable(tcx, tcx.impl_trait_ref(def_id).instantiate_identity().def_id) =>
            {
                "user implementations of this standard or external trait"
            }
            DefKind::Static { .. } if tcx.is_foreign_item(def_id) => continue,
            DefKind::Static { .. } => "statics",
            _ => continue,
        };
        tcx.dcx()
            .span_err(tcx.def_span(def_id), format!("rust-js does not support {what} yet"));
        valid = false;
    }
    valid
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
/// Like globals, every module reserves them: the second result is both.
/// Namespaces are named last, so a module's default export gets its plain name.
fn name_imports(tcx: TyCtxt<'_>, uses: &JsUses) -> (HashMap<Export, String>, HashSet<String>) {
    let mut reserved = uses.globals.clone();
    let (namespaces, others): (Vec<&Export>, Vec<&Export>) =
        uses.imported.keys().partition(|(_, export)| export == "*");
    let names = others
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
        .collect();
    (names, reserved)
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
            match tcx.type_of(imp).instantiate_identity().kind() {
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
                (ExprKind::ZstLiteral { .. }, ty::FnDef(def_id, _)) | (ExprKind::NamedConst { def_id, .. }, _) => {
                    def_id
                }
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
            tcx.impl_trait_ref(id).instantiate_identity().def_id,
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
