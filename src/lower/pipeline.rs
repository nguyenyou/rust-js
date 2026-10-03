//! Orchestrate analyzed crate facts, function emission, reachability and symbolic module assembly.

use super::analysis::{AnalyzedCrate, analyze_crate, is_thread_local};
use super::bindings::Export;
use super::{Body, CrateFacts, FnCx, Locals, const_js, eval_const, module_file, module_symbol, static_value};
use crate::js::{self, Expr, Prop, StmtKind};
use crate::program::{ImportRequest, LoweredModule, Unlinked, UnlinkedModule};
use crate::runtime::Helper;
use rustc_hir::def::DefKind;
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::def_id::{DefId, LocalModDefId};
use std::collections::{BTreeMap, HashMap, HashSet};

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
pub fn lower_crate<'tcx>(
    tcx: TyCtxt<'tcx>,
    all_bodies: &[Body<'tcx>],
    initializers: &[Body<'tcx>],
    serde_attrs: &super::SerdeAttributes,
    dependencies: &crate::library::Dependencies,
    export_library: bool,
) -> Option<Unlinked> {
    let sources = super::sources::CapturedSources::new(tcx);
    let AnalyzedCrate {
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
        mut failed,
        mut called_from_elsewhere,
        tests,
        paths,
        mutated,
        changed_vecs,
        drop_params,
        type_facts,
        generic_consts,
        pretty_debug,
        format_options,
    } = analyze_crate(tcx, all_bodies, dependencies, export_library)?;
    // A library exports what its consumers can reach (ADR 0100).
    if export_library {
        called_from_elsewhere.extend(fns.keys().copied().filter(|&id| super::library::reachable(tcx, id)));
    }

    let mut const_items: HashMap<LocalModDefId, Vec<js::Const>> = HashMap::new();
    // The statics and constants whose initializers are lowered as code.
    let mut initialized: Vec<&Body<'tcx>> = Vec::new();
    for &def_id in consts.iter().filter(|&&d| !is_thread_local(tcx, d)) {
        let span = tcx.def_span(def_id);
        let typing_env = ty::TypingEnv::fully_monomorphized();
        let args = ty::GenericArgs::identity_for_item(tcx, def_id);
        // A `static mut` is its value's `{ value }` (ADR 0096), which any
        // module can write.
        let (value, what) = match tcx.def_kind(def_id) {
            DefKind::Static { mutability, .. } => (
                static_value(tcx, def_id)
                    .and_then(|v| const_js(tcx, v))
                    .map(|v| match mutability.is_mut() {
                        true => Expr::object(vec![Prop::Field("value".into(), v)]),
                        false => v,
                    }),
                "statics",
            ),
            _ => (
                eval_const(tcx, typing_env, def_id.to_def_id(), args, span).and_then(|v| const_js(tcx, v)),
                "constants",
            ),
        };
        let Some(value) = value else {
            let ty = tcx.type_of(def_id).instantiate_identity().skip_normalization();
            // A value rustc's can't say, a function's, a `dyn`'s or one too
            // large for a value tree, is its initializer's, where one JS value
            // can be every use's, and it reads no static, whose value may not
            // be made yet.
            let mutable = matches!(tcx.def_kind(def_id), DefKind::Static { mutability, .. } if mutability.is_mut());
            if !mutable
                && super::copies::shareable(tcx, ty)
                && let Some(body) = initializers.iter().find(|body| body.def_id == def_id)
                && !super::body_queries::reads_statics(&body.thir)
            {
                initialized.push(body);
                continue;
            }
            tcx.dcx()
                .span_err(span, format!("rust-js does not support {what} of type `{ty}` yet"));
            failed = true;
            continue;
        };
        let info = &fns[&def_id.to_def_id()];
        let span = span.source_callsite();
        const_items.entry(info.module).or_default().push(js::Const {
            name: info.name.clone(),
            value,
            export: tcx.visibility(def_id).is_public() || called_from_elsewhere.contains(&def_id.to_def_id()),
            span: sources.span(span),
        });
    }

    let function_bodies = bodies.iter().map(|body| (body.def_id.to_def_id(), *body)).collect();
    // A trait impl's dictionary (ADR 0049) has no body of its own, so its
    // function context gets an empty one; a copied default brings its own.
    let no_body = rustc_middle::thir::Thir::new(rustc_middle::thir::BodyTy::Const(tcx.types.unit));
    // Lower each body once. Cross-module references have symbolic names until
    // the link step knows exactly which imports and local names survive.
    let mut lowered_items = Vec::new();
    let foreign = super::library::Foreign::new(tcx, dependencies);
    let crate_facts = CrateFacts {
        foreign: &foreign,
        library: export_library,
        sources: &sources,
        mutated: &mutated,
        generic_consts: &generic_consts,
        changed_vecs: &changed_vecs,
        drop_params: &drop_params,
        type_facts: &type_facts,
        closures: &closures,
        bodies: &function_bodies,
        fns: &fns,
        imports: &import_names,
        trait_impls: &trait_impls,
        serde_attrs,
        pretty_debug,
        format_options,
    };
    let mut work: Vec<(DefId, Option<&Body<'tcx>>)> = bodies
        .iter()
        .filter(|b| tcx.trait_of_assoc(b.def_id.to_def_id()).is_none())
        .map(|b| (b.def_id.to_def_id(), Some(*b)))
        .chain(dictionaries.iter().map(|id| (*id, None)))
        .chain(initialized.iter().map(|body| (body.def_id.to_def_id(), Some(*body))))
        .collect();
    let (mut used, mut queued) = (HashSet::new(), HashSet::new());
    let no_facts = super::body_queries::BodyFacts::default();
    let mut next = 0;
    loop {
        // Then the codecs something uses, a round at a time, each in the
        // order they're declared. A shared crate derives both for its types,
        // so one that's never used isn't lowered: its type needn't be one
        // rust-js reads or writes. A library's consumers may use one they can
        // reach (ADR 0100).
        if next == work.len() {
            let round: Vec<_> = codecs
                .iter()
                .filter(|&&id| {
                    (used.contains(&id) || (export_library && super::library::reachable(tcx, id))) && queued.insert(id)
                })
                .map(|&id| (id, None))
                .collect();
            if round.is_empty() {
                break;
            }
            work.extend(round);
        }
        let (def_id, body) = work[next];
        next += 1;
        let module = fns[&def_id].module;
        let mut cx = FnCx {
            tcx,
            typing_env: ty::TypingEnv::post_analysis(tcx, def_id),
            given: Default::default(),
            codecs: Vec::new(),
            krate: &crate_facts,
            dependencies: Default::default(),
            captures: HashMap::new(),
            thir: body.map_or(&no_body, |body| &body.thir),
            body_facts: body.map_or(&no_facts, |body| &body.facts),
            module,
            locals: Locals::default(),
            // Locals must never shadow a function or an import of this file.
            names: taken[&module].clone(),
            module_names: &taken[&module],
            labels: HashSet::new(),
            loops: Vec::new(),
            runtime: HashSet::new(),
            jsx: false,
            writing: Default::default(),
            chains: Default::default(),
            stepping: body.map_or_else(Default::default, |body| {
                super::iterators::Stepping::of(&body.facts.stepped)
            }),
            cloning: Vec::new(),
            item: def_id,
            walks: Default::default(),
            drop_state: Default::default(),
            body_owner: def_id,
        };
        let initializer = initialized.iter().any(|body| body.def_id.to_def_id() == def_id);
        let result = match body {
            Some(body) if initializer => cx.lower_initializer(body),
            Some(body) => cx.lower_fn(body),
            None if codecs.contains(&def_id) => cx.lower_codec(def_id).map(|function| super::LoweredFn {
                function,
                runtime: std::mem::take(&mut cx.runtime),
                jsx: cx.jsx,
                dependencies: cx.dependencies.take(),
            }),
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
            Ok(lowered) => {
                used.extend(lowered.dependencies.uses.iter().map(|&(_, to)| to));
                lowered_items.push((def_id, lowered));
            }
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
    let edges = lowered_items
        .iter()
        .flat_map(|(_, item)| item.dependencies.uses.iter().copied());
    let roots = edges
        .clone()
        .filter(|(from, _)| !derived.contains(from))
        .map(|(_, to)| to)
        // A library's consumers may reach one (ADR 0100).
        .chain(
            derived
                .iter()
                .copied()
                .filter(|&id| export_library && super::library::reachable(tcx, id)),
        );
    let reached = crate::reachability::reachable(roots, edges);
    let mut pass = Pass::default();
    for (def_id, mut lowered) in lowered_items
        .into_iter()
        .filter(|(id, _)| !derived.contains(id) || reached.contains(id))
    {
        let module = fns[&def_id].module;
        let initializer = initialized.iter().any(|body| body.def_id.to_def_id() == def_id);
        pass.references.extend(lowered.dependencies.references.drain());
        pass.package_uses.extend(lowered.dependencies.package_uses.drain());
        if dictionaries.contains(&def_id) {
            pass.caches
                .entry(module)
                .or_default()
                .push(format!("${}", fns[&def_id].name));
        }
        match lowered {
            lowered
                if let Some(key) = thread_local_inits
                    .get(&def_id.expect_local())
                    .copied()
                    .or_else(|| initializer.then(|| def_id.expect_local())) =>
            {
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
                let span = tcx.def_span(key).source_callsite();
                pass.local_consts.entry(info.module).or_default().push(js::Const {
                    name: info.name.clone(),
                    value,
                    export: tcx.visibility(key).is_public() || called_from_elsewhere.contains(&key.to_def_id()),
                    span: sources.span(span),
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
            // `js::import!("./App.css");`: `import "./App.css";`, for what a
            // module does when loaded, as a bundler's CSS does (ADR 0110).
            for attr in super::bindings::marks(tcx, module, "import") {
                let Some(from) = attr.value_str().map(|s| s.to_string()) else {
                    tcx.dcx()
                        .span_err(attr.span(), "rust-js: write it `js::import!(\"./file.css\");`");
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
            let lowered = LoweredModule {
                path: paths[&module].clone(),
                file: module_file(tcx, module).name.clone().into_local_path(),
                packages,
                imports: Vec::new(),
                namespaces: pass.namespaces.remove(&module).unwrap_or_default(),
                consts: const_items.remove(&module).unwrap_or_default(),
                functions: pass.functions.remove(&module).unwrap_or_default(),
                caches: pass.caches.remove(&module).unwrap_or_default(),
                runtime: Vec::new(),
                jsx: pass.jsx.contains(&module),
            };
            let mut imports: Vec<_> = targets.remove(&module).unwrap_or_default().into_iter().collect();
            imports.sort_by(|(a, an), (b, bn)| (&paths[a], an).cmp(&(&paths[b], bn)));
            let candidates: Vec<_> = imports
                .into_iter()
                .map(|(target, export)| ImportRequest {
                    symbol: module_symbol(target, &export),
                    export,
                    path: paths[&target].clone(),
                })
                .collect();
            UnlinkedModule {
                module: lowered,
                imports: candidates,
                reserved_names: taken[&module].clone(),
                runtime: pass.runtime.remove(&module).unwrap_or_default(),
            }
        })
        .collect();
    tcx.dcx().has_errors().is_none().then_some(Unlinked {
        library: export_library.then(|| super::library::exports(tcx, &fns, &drop_params, &trait_impls, dependencies)),
        sources: sources.output,
        modules: lowered,
        tests,
    })
}
