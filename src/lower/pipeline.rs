//! Orchestrate analyzed crate facts, function emission, reachability and symbolic module assembly.

use super::analysis::{AnalyzedCrate, analyze_crate, is_thread_local};
use super::bindings::Export;
use super::{Body, CrateFacts, FnCx, const_js, eval_const, module_file, module_symbol};
use crate::js::{self, Expr, StmtKind};
use crate::program::{ImportRequest, LoweredModule, Unlinked, UnlinkedModule};
use crate::runtime::Helper;
use rustc_middle::ty::{self, TyCtxt};
use rustc_span::def_id::{DefId, LocalModDefId};
use rustc_span::{Symbol, sym};
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
    serde_attrs: &super::SerdeAttributes,
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
        called_from_elsewhere,
        tests,
        paths,
        mutated,
        changed_vecs,
    } = analyze_crate(tcx, all_bodies)?;

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
    let crate_facts = CrateFacts {
        sources: &sources,
        mutated: &mutated,
        changed_vecs: &changed_vecs,
        closures: &closures,
        bodies: &function_bodies,
        fns: &fns,
        imports: &import_names,
        trait_impls: &trait_impls,
        serde_attrs,
    };
    let mut work: Vec<(DefId, Option<&Body<'tcx>>)> = bodies
        .iter()
        .filter(|b| tcx.trait_of_assoc(b.def_id.to_def_id()).is_none())
        .map(|b| (b.def_id.to_def_id(), Some(*b)))
        .chain(dictionaries.iter().map(|id| (*id, None)))
        .collect();
    let (mut used, mut queued) = (HashSet::new(), HashSet::new());
    let mut next = 0;
    loop {
        // Then the codecs something uses, a round at a time, each in the
        // order they're declared. A shared crate derives both for its types,
        // so one that's never used isn't lowered: its type needn't be one
        // rust-js reads or writes.
        if next == work.len() {
            let round: Vec<_> = codecs
                .iter()
                .filter(|&&id| used.contains(&id) && queued.insert(id))
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
            evidence: Vec::new(),
            self_args: None,
            krate: &crate_facts,
            dependencies: Default::default(),
            captures: HashMap::new(),
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
            codec_params: Vec::new(),
            slots: HashMap::new(),
            stepped: body.map_or_else(HashSet::new, |body| super::stepped_locals(tcx, &body.thir)),
            iterators: HashSet::new(),
            boxes: HashSet::new(),
            cloning: Vec::new(),
            item: def_id,
        };
        let result = match body {
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
        .map(|(_, to)| to);
    let reached = crate::reachability::reachable(roots, edges);
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
        sources: sources.output,
        modules: lowered,
        tests,
    })
}
