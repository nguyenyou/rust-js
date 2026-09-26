//! Collect crate facts and orchestrate lowering; no filesystem writes.

use super::bindings;
use super::bindings::{Export, is_binding, js_import, js_path, module_binding};
use super::{
    Body, CrateFacts, FnCx, FnInfo, Lowered, LoweredModule, TestFn, camel_case, const_js, eval_const, fresh_in,
    module_file, module_path, strip,
};
use crate::js;
use crate::js::{Expr, StmtKind};
use crate::runtime::Helper;
use rustc_hir::def::DefKind;
use rustc_hir::find_attr;
use rustc_middle::thir::ExprKind;
use rustc_middle::ty;
use rustc_middle::ty::{Ty, TyCtxt};
use rustc_span::def_id::{DefId, LOCAL_CRATE, LocalDefId, LocalModDefId};
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

/// Lower every function, grouped by module. Reports all unsupported
/// features as rustc errors.
pub fn lower_crate<'tcx>(tcx: TyCtxt<'tcx>, all_bodies: &[Body<'tcx>]) -> Option<Lowered> {
    if !bindings::validate(tcx) {
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

    // `thread_local!` (ADR 0037) is a `const NAME: LocalKey<T>` whose block holds
    // `fn __rust_std_internal_init_fn() -> T { init }`, then std's storage for
    // it. In JS, it's a variable of its module, made from `init`.
    let is_thread_local = |d: LocalDefId| {
        matches!(tcx.def_kind(d), DefKind::Const { .. })
            && matches!(tcx.type_of(d).instantiate_identity().kind(), ty::Adt(adt, _) if tcx.is_diagnostic_item(Symbol::intern("LocalKey"), adt.did()))
    };
    let in_thread_local = |d: LocalDefId| {
        let mut parent = tcx.opt_local_parent(d);
        while let Some(p) = parent {
            if is_thread_local(p) {
                return Some(p);
            }
            parent = tcx.opt_local_parent(p);
        }
        None
    };

    let mut failed = false;
    for def_id in tcx.hir_crate_items(()).definitions() {
        let what = match tcx.def_kind(def_id) {
            _ if markers.iter().any(|&(marker, _)| marker == def_id) => continue,
            // std's storage for a thread-local: JS needs none.
            _ if in_thread_local(def_id).is_some() => continue,
            // `#[derive(Clone, Copy)]` and friends write impls we never call.
            DefKind::AssocFn if tcx.is_automatically_derived(tcx.parent(def_id.to_def_id())) => continue,
            DefKind::AssocFn if is_binding(tcx, def_id.to_def_id()) => continue,
            DefKind::AssocFn => "methods",
            DefKind::AssocConst { .. } => "associated constants",
            DefKind::Static { .. } if tcx.is_foreign_item(def_id) => continue,
            DefKind::Static { .. } => "statics",
            _ => continue,
        };
        tcx.dcx()
            .span_err(tcx.def_span(def_id), format!("rust-js does not support {what} yet"));
        failed = true;
    }
    if failed {
        return None;
    }

    // Closures are lowered inside the function that creates them.
    let (bodies, closures): (Vec<&Body<'tcx>>, Vec<&Body<'tcx>>) = all_bodies
        .iter()
        .partition(|body| tcx.def_kind(body.def_id) == DefKind::Fn);
    let all_bodies = &all_bodies;
    let closures: HashMap<LocalDefId, &Body<'tcx>> = closures.into_iter().map(|b| (b.def_id, b)).collect();

    // JS globals the crate uses, whether declared here or in another crate
    // (`web`, ADR 0024): every module reserves them, so a local named
    // `console` can't hide the real one.
    // Imports from JS modules (ADR 0028) are found the same way, with the
    // modules that use each one.
    let mut globals: HashSet<String> = HashSet::new();
    let mut imported: BTreeMap<Export, HashSet<LocalModDefId>> = BTreeMap::new();
    // What each import is bound to in Rust, to name a default import after.
    let mut bound_to: HashMap<Export, HashSet<DefId>> = HashMap::new();
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
                    bound_to.entry(export.clone()).or_default().insert(def_id);
                    imported.entry(export).or_default().insert(module);
                }
                Some((path, None)) => {
                    globals.insert(path.split('.').next().unwrap_or_default().to_string());
                }
                None => {}
            }
        }
    }
    // Each import's name, the same in every file, and unique in the crate:
    // after the export, or the module for a default or namespace import. A
    // default import held by one `static` is named after it, as JS code names
    // an asset: `static hero_img` is `import heroImg from "./hero.png"`.
    // Like globals, every module reserves them. Namespaces are named last,
    // so a module's default export gets its plain name.
    let mut reserved = globals;
    let (namespaces, others): (Vec<&Export>, Vec<&Export>) = imported.keys().partition(|(_, export)| export == "*");
    let import_names: HashMap<Export, String> = others
        .into_iter()
        .chain(namespaces)
        .map(|(from, export)| {
            let export_key = (from.clone(), export.clone());
            let held_by = match bound_to[&export_key].iter().collect::<Vec<_>>().as_slice() {
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
    let globals = reserved;

    // Each thread-local's `init` function: lowered like any function, its
    // body is the variable's value.
    let thread_local_inits: HashMap<LocalDefId, LocalDefId> = bodies
        .iter()
        .filter(|body| tcx.def_kind(body.def_id) == DefKind::Fn)
        .filter_map(|body| Some((body.def_id, in_thread_local(body.def_id)?)))
        .collect();

    // `const` items (ADR 0031), with the values rustc has computed. One in a
    // function goes beside it, in its module.
    let consts: Vec<LocalDefId> = tcx
        .hir_crate_items(())
        .definitions()
        .filter(|&d| matches!(tcx.def_kind(d), DefKind::Const { .. }) && !markers.iter().any(|&(m, _)| m == d))
        .collect();

    // The modules that get a JS file: the root, then every module with a
    // function or a `const`, in the order the first one appears.
    let mut modules = vec![LocalModDefId::CRATE_DEF_ID];
    for def_id in bodies.iter().map(|body| body.def_id).chain(consts.iter().copied()) {
        let module = tcx.parent_module_from_def_id(def_id);
        if !modules.contains(&module) {
            modules.push(module);
        }
    }

    // Each function's JS name, unique within its module's file. `taken` also
    // collects the import aliases below, so local variables avoid both.
    let mut taken: HashMap<LocalModDefId, HashSet<String>> = modules.iter().map(|&m| (m, globals.clone())).collect();
    let fns: HashMap<DefId, FnInfo> = bodies
        .iter()
        .map(|body| body.def_id)
        .chain(consts.iter().copied())
        .map(|def_id| {
            let module = tcx.parent_module_from_def_id(def_id);
            let names = taken.entry(module).or_default();
            let name = fresh_in(names, &bindings::fn_name(tcx, def_id.to_def_id()));
            (def_id.to_def_id(), FnInfo { module, name })
        })
        .collect();

    // Which modules each module calls into, and which functions are called
    // from another module: those must be exported, even if private in Rust
    // (a child module may call its parent's private functions).
    let mut uses: HashMap<LocalModDefId, Vec<LocalModDefId>> = HashMap::new();
    let mut called_from_elsewhere: HashSet<DefId> = HashSet::new();
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
                called_from_elsewhere.insert(*def_id);
                let targets = uses.entry(from).or_default();
                if !targets.contains(&target.module) {
                    targets.push(target.module);
                }
            }
        }
    }

    // The tests: each marker names a function beside it, of the same name.
    // The test file imports them, so they're exported.
    let mut tests = Vec::new();
    for &(marker, label) in &markers {
        let module = tcx.parent_module_from_def_id(marker);
        let name = tcx.item_name(marker.to_def_id());
        let Some(test) = bodies
            .iter()
            .map(|body| body.def_id)
            .find(|&f| tcx.parent_module_from_def_id(f) == module && tcx.item_name(f.to_def_id()) == name)
        else {
            continue;
        };
        called_from_elsewhere.insert(test.to_def_id());
        let should_panic = find_attr!(tcx, test, ShouldPanic { reason, .. } => reason.map(|r| r.to_string()));
        tests.push(TestFn {
            module: module_path(tcx, module),
            name: fns[&test.to_def_id()].name.clone(),
            label: label.to_string(),
            should_panic,
            ignore: find_attr!(tcx, test, Ignore { .. }),
        });
    }

    // Import aliases: the module's last path segment (the crate name for the
    // root), unique within the importing file.
    let paths: HashMap<LocalModDefId, Vec<String>> = modules.iter().map(|&m| (m, module_path(tcx, m))).collect();
    let crate_name = tcx.crate_name(LOCAL_CRATE).to_string();
    let mut aliases: HashMap<LocalModDefId, HashMap<LocalModDefId, String>> = HashMap::new();
    for &module in &modules {
        let mut targets = uses.remove(&module).unwrap_or_default();
        targets.sort_by(|a, b| paths[a].cmp(&paths[b]));
        let names = taken.entry(module).or_default();
        let module_aliases = targets
            .into_iter()
            .map(|target| (target, fresh_in(names, paths[&target].last().unwrap_or(&crate_name))))
            .collect();
        aliases.insert(module, module_aliases);
    }

    // The types whose JS objects get changed in place somewhere in the
    // crate: `a.b.c = ..` changes the object `a.b`, so it's `a.b`'s type.
    // Only these ever need copying (ADR 0020).
    let mut mutated: HashSet<Ty<'tcx>> = HashSet::new();
    for body in all_bodies {
        for expr in body.thir.exprs.iter() {
            if let ExprKind::Assign { lhs, .. } | ExprKind::AssignOp { lhs, .. } = expr.kind
                && let ExprKind::Field { lhs: object, .. } = body.thir[strip(&body.thir, lhs)].kind
            {
                mutated.insert(body.thir[object].ty);
            }
        }
    }

    let mut const_items: HashMap<LocalModDefId, Vec<js::Const>> = HashMap::new();
    for &def_id in consts.iter().filter(|&&d| !is_thread_local(d)) {
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

    let crate_facts = CrateFacts {
        mutated: &mutated,
        closures: &closures,
        fns: &fns,
        imports: &import_names,
    };
    let mut functions: HashMap<LocalModDefId, Vec<js::Function>> = HashMap::new();
    let mut runtime: HashMap<LocalModDefId, HashSet<Helper>> = HashMap::new();
    let mut jsx: HashSet<LocalModDefId> = HashSet::new();
    for body in &bodies {
        let def_id = body.def_id.to_def_id();
        let module = fns[&def_id].module;
        let file = module_file(tcx, module);
        let mut cx = FnCx {
            tcx,
            typing_env: ty::TypingEnv::post_analysis(tcx, def_id),
            krate: &crate_facts,
            captures: HashMap::new(),
            file_start: file.start_pos,
            file_end: file.end_position(),
            thir: &body.thir,
            module,
            aliases: &aliases[&module],
            vars: HashMap::new(),
            // Locals must never shadow a function or an import of this file.
            names: taken[&module].clone(),
            module_names: &taken[&module],
            labels: HashSet::new(),
            loops: Vec::new(),
            runtime: HashSet::new(),
            jsx: false,
        };
        match cx.lower_fn(body) {
            Ok(lowered) if let Some(&key) = thread_local_inits.get(&body.def_id) => {
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
                const_items.entry(info.module).or_default().push(js::Const {
                    name: info.name.clone(),
                    value,
                    export: tcx.visibility(key).is_public() || called_from_elsewhere.contains(&key.to_def_id()),
                    span: js::Span {
                        lo: (span.lo() - file.start_pos).0,
                        hi: (span.hi() - file.start_pos).0,
                    },
                });
                runtime.entry(module).or_default().extend(lowered.runtime);
                if lowered.jsx {
                    jsx.insert(module);
                }
            }
            Ok(mut lowered) => {
                if lowered.jsx {
                    jsx.insert(module);
                }
                lowered.function.export |= called_from_elsewhere.contains(&def_id);
                functions.entry(module).or_default().push(lowered.function);
                runtime.entry(module).or_default().extend(lowered.runtime);
            }
            Err(_) => failed = true,
        }
    }
    if failed {
        return None;
    }

    let lowered = modules
        .into_iter()
        .map(|module| {
            let mut imports: Vec<(String, Vec<String>)> = aliases[&module]
                .iter()
                .map(|(target, alias)| (alias.clone(), paths[target].clone()))
                .collect();
            imports.sort_by(|a, b| a.1.cmp(&b.1));
            // One `import` per JS module, of what this file uses from it.
            let mut packages: BTreeMap<&str, js::Package> = BTreeMap::new();
            for export in imported
                .iter()
                .filter(|(_, users)| users.contains(&module))
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
            let mut helpers: Vec<Helper> = runtime.remove(&module).unwrap_or_default().into_iter().collect();
            helpers.sort();
            LoweredModule {
                path: paths[&module].clone(),
                file: module_file(tcx, module),
                packages,
                imports,
                consts: const_items.remove(&module).unwrap_or_default(),
                functions: functions.remove(&module).unwrap_or_default(),
                runtime: helpers,
                jsx: jsx.contains(&module),
            }
        })
        .collect();
    tcx.dcx().has_errors().is_none().then_some(Lowered {
        modules: lowered,
        tests,
    })
}
