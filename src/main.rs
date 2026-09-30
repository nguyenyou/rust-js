//! rust-js: compile Rust to readable JavaScript, in the spirit of ReScript.
//!
//! ReScript reuses OCaml's type checker and swaps in a JS backend. We do
//! the same with rustc:
//!
//! ```text
//!   .rs ─► rustc: parse, resolve, type check ─► THIR ──┐ (copied)
//!                                                │      │
//!                                   borrowck (MIR)      ▼
//!                                                │   lower.rs ─► js.rs ─► prepare.rs ─► to_oxc.rs ─► .js + .js.map
//!                          errors? stop here ◄───┘
//! ```
//!
//! Usage: `rust-js [--test] <input.rs> [-o <output.js>] [--manifest <file.json>]
//! [--library] [--dependency <manifest.json>] [-- <rustc flags>]`. Also
//! writes `<output.js>.map`. Flags after `--` go to rustc unchanged. With
//! `--test`, the crate's `#[test]` functions are compiled too, and
//! `<output>.test.js` runs them with `bun test` (ADR 0026).

#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_expand;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_parse;
extern crate rustc_session;
extern crate rustc_span;

mod cargo;
mod format;
mod hooks;
mod js;
mod jsx_syntax;
mod library;
mod link;
mod lower;
mod manifest;
mod names;
mod output;
mod prepare;
mod program;
mod publish;
mod reachability;
mod runtime;
mod settings;
mod to_oxc;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::OnceLock;

use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::RegisteredTools;
use rustc_middle::ty::TyCtxt;
use rustc_session::config::CrateType;
use rustc_span::def_id::{LOCAL_CRATE, StableCrateId};
use rustc_span::{Ident, Symbol};

/// The crate's `StableCrateId`, as rustc computes it once it has the crate's
/// context, which is after JSX's expansions need it (ADR 0110). rustc's own
/// helpers, in its order; `after_expansion` checks it's rustc's.
fn stable_crate_id(compiler: &Compiler, krate: &rustc_ast::Crate) -> StableCrateId {
    let sess = &compiler.sess;
    let attrs = rustc_expand::config::pre_configure_attrs(sess, &krate.attrs);
    let name = rustc_interface::passes::get_crate_name(sess, &attrs);
    let backend = &compiler.codegen_backend;
    let types = rustc_interface::passes::collect_crate_types(
        sess,
        &backend.supported_crate_types(sess),
        backend.name(),
        &attrs,
        krate.spans.inner_span,
    );
    StableCrateId::new(
        name,
        types.contains(&CrateType::Executable),
        sess.opts.cg.metadata.clone(),
        sess.cfg_version,
    )
}

/// rustc's own tools, `rustfmt` and the rest, which `rust_js` joins.
static RUSTC_TOOLS: OnceLock<for<'tcx> fn(TyCtxt<'tcx>, ()) -> RegisteredTools> = OnceLock::new();

struct RustJs {
    output: output::OutputPlan,
    dependencies: library::Dependencies,
    export_library: bool,
    /// `--emit=metadata` among rustc's flags: rustc goes on to write the
    /// crate's metadata, for the crates that use it (ADR 0100).
    metadata: bool,
    /// What's planned and published once rustc has written the metadata too:
    /// a library's JS and its metadata are one build's, or neither is.
    pending: Option<(link::Linked, Vec<PathBuf>)>,
}

/// `rust_js` is a tool rustc knows, as it knows `rustfmt`: a program
/// writes `#[rust_js::link_name = ".."]` as stable Rust (ADR 0110).
fn register_tool(config: &mut rustc_interface::interface::Config) {
    config.override_queries = Some(|_, providers| {
        RUSTC_TOOLS.get_or_init(|| providers.queries.registered_tools);
        providers.queries.registered_tools = |tcx, ()| {
            let mut tools = RUSTC_TOOLS.get().expect("rustc's tools, kept first")(tcx, ());
            tools.insert(Ident::with_dummy_span(Symbol::intern("rust_js")));
            tools
        };
    });
}

/// rustc itself, with rust-js's tool and syntax and nothing else, `--rustc`:
/// what the crates a program uses, js, webapi and react, are compiled with,
/// for their metadata (ADR 0112).
struct Syntax;

impl Callbacks for Syntax {
    fn config(&mut self, config: &mut rustc_interface::interface::Config) {
        register_tool(config);
    }

    fn after_crate_root_parsing(&mut self, compiler: &Compiler, krate: &mut rustc_ast::Crate) -> Compilation {
        jsx_syntax::expand(&compiler.sess, krate, stable_crate_id(compiler, krate));
        Compilation::Continue
    }

    fn after_expansion<'tcx>(&mut self, _compiler: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        jsx_syntax::check_crate_id(tcx.stable_crate_id(LOCAL_CRATE));
        Compilation::Continue
    }
}

impl Callbacks for RustJs {
    fn config(&mut self, config: &mut rustc_interface::interface::Config) {
        register_tool(config);
    }

    fn after_crate_root_parsing(&mut self, compiler: &Compiler, krate: &mut rustc_ast::Crate) -> Compilation {
        Syntax.after_crate_root_parsing(compiler, krate)
    }

    fn after_expansion<'tcx>(&mut self, _compiler: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        jsx_syntax::check_crate_id(tcx.stable_crate_id(LOCAL_CRATE));
        // 0. `#[serde(..)]`, which only the expanded crate still has (ADR 0077).
        let serde_attrs = lower::serde_attributes(tcx);
        // 1. Copy each function's THIR. MIR building (for borrowck) steals it.
        let bodies = lower::collect_bodies(tcx);

        // 2. Run rustc's full analysis: type check, borrow check, lints.
        tcx.ensure_ok().analysis(());

        // 3. Only a program rustc accepts becomes JavaScript.
        if tcx.dcx().has_errors().is_none()
            && let Some(unlinked) =
                lower::lower_crate(tcx, &bodies, &serde_attrs, &self.dependencies, self.export_library)
            && tcx.dcx().has_errors().is_none()
        {
            let linked = link::link(unlinked);
            let sources = tcx
                .sess
                .source_map()
                .files()
                .iter()
                .filter(|file| file.src.is_some())
                .filter_map(|file| file.name.clone().into_local_path())
                .chain(self.dependencies.inputs.iter().cloned())
                .collect();
            if self.metadata {
                self.pending = Some((linked, sources));
            } else if let Err(err) = self.output.plan(linked, sources).and_then(|plan| plan.publish()) {
                tcx.dcx().err(format!("rust-js: {err}"));
            }
        }

        // We never want rustc's own codegen, only, for a library, the metadata
        // its consumers' rustc reads: that of the crate rust-js compiled, as it
        // configured it, `cfg(rust_js)` and all.
        if self.metadata && tcx.dcx().has_errors().is_none() {
            Compilation::Continue
        } else {
            Compilation::Stop
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Cargo's workspace wrapper (ADR 0101): its call, as rust-js's, or rustc's.
    let args = match cargo::translate(args) {
        cargo::Invocation::RustJs(args) => args,
        cargo::Invocation::Rustc(exit) => return exit,
    };
    // `@rust-js/runtime`'s module (ADR 0103), for `runtime/index.js`.
    if args.as_slice() == ["--runtime-module"] {
        print!("{}", runtime::package_module());
        return ExitCode::SUCCESS;
    }
    if args.as_slice() == ["--version-json"] {
        println!(
            "{}",
            serde_json::to_string(&manifest::Compiler::current()).expect("serialize compiler identity")
        );
        return ExitCode::SUCCESS;
    }
    if args.as_slice() == ["--version"] {
        let compiler = manifest::Compiler::current();
        println!(
            "rust-js {} ({}; ABI {})",
            compiler.version, compiler.toolchain, compiler.abi
        );
        return ExitCode::SUCCESS;
    }
    if args.first().is_some_and(|arg| arg == "--rustc") {
        // `cfg(rust_js)` says it's rust-js, as it says to a program: the crates'
        // attributes of its tool are `cfg_attr(rust_js, ..)`, which a plain
        // rustc, a user's own `cargo check`, leaves out (ADR 0113).
        let ours = ["rust-js", "--cfg=rust_js", "--check-cfg=cfg(rust_js)"].map(String::from);
        let rustc_args: Vec<String> = ours.into_iter().chain(args[1..].iter().cloned()).collect();
        return rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&rustc_args, &mut Syntax));
    }
    if args.first().is_some_and(|arg| arg == "--format-jsx") {
        let input = match args.as_slice() {
            [_] => "-",
            [_, input] => input.as_str(),
            _ => {
                eprintln!("usage: rust-js --format-jsx [input.rs] (defaults to stdin; writes stdout)");
                return ExitCode::FAILURE;
            }
        };
        let mut formatter = jsx_syntax::formatting::Formatter::default();
        let args = vec![
            "rust-js".into(),
            input.into(),
            "--crate-type=lib".into(),
            "--edition=2024".into(),
        ];
        let result = rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut formatter));
        if let Some(output) = formatter.output {
            print!("{output}");
        }
        return result;
    }
    // Anything after `--` goes to rustc as-is, e.g. `-- --sysroot /sysroot`.
    let (ours, to_rustc) = match args.iter().position(|a| a == "--") {
        Some(i) => (&args[..i], &args[i + 1..]),
        None => (&args[..], &[][..]),
    };
    let mut ours = ours.to_vec();
    // Cargo's build (ADR 0101): its record of the sources is one of rustc's
    // outputs rust-js lets rustc write, where Cargo asks.
    let cargo = if let Some(i) = ours.iter().position(|arg| arg == "--cargo") {
        ours.remove(i);
        true
    } else {
        false
    };
    let export_library = if let Some(i) = ours.iter().position(|arg| arg == "--library") {
        ours.remove(i);
        true
    } else {
        false
    };
    let mut dependency_paths = Vec::new();
    while let Some(i) = ours.iter().position(|arg| arg == "--dependency") {
        if i + 1 >= ours.len() {
            eprintln!("--dependency requires a manifest path");
            return ExitCode::FAILURE;
        }
        dependency_paths.push(PathBuf::from(ours.remove(i + 1)));
        ours.remove(i);
    }
    // Which of a crate's checks this build runs (ADR 0117): a save's, in the
    // Vite server, or, by default, a build's, all of them.
    let save = if let Some(i) = ours.iter().position(|arg| arg == "--hooks") {
        let which = ours.get(i + 1).cloned().unwrap_or_default();
        if which != "save" && which != "build" {
            eprintln!("--hooks requires `save` or `build`");
            return ExitCode::FAILURE;
        }
        ours.drain(i..=i + 1);
        which == "save"
    } else {
        false
    };
    let manifest = if let Some(i) = ours.iter().position(|arg| arg == "--manifest") {
        if i + 1 >= ours.len() {
            eprintln!("--manifest requires a path");
            return ExitCode::FAILURE;
        }
        let path = PathBuf::from(ours.remove(i + 1));
        ours.remove(i);
        Some(path)
    } else {
        None
    };
    let ours = ours.as_slice();
    let test = ours.first().is_some_and(|a| a == "--test");
    let ours = if test { &ours[1..] } else { ours };
    let (input, output) = match ours {
        [input] => (PathBuf::from(input), PathBuf::from(input).with_extension("js")),
        [input, flag, output] if flag == "-o" => (PathBuf::from(input), PathBuf::from(output)),
        _ => {
            eprintln!(
                "usage: rust-js [--test] <input.rs> [-o <output.js>] [--manifest <file.json>] [--library] [--dependency <manifest.json>] [--hooks save|build] [-- <rustc flags>]"
            );
            return ExitCode::FAILURE;
        }
    };

    if export_library && manifest.is_none() {
        eprintln!("--library requires --manifest");
        return ExitCode::FAILURE;
    }
    let dependencies = match library::Dependencies::load(&dependency_paths, &output) {
        Ok(dependencies) => dependencies,
        Err(error) => {
            eprintln!("rust-js: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut rustc_args = vec![
        "rust-js".to_string(), // argv[0], ignored by rustc
        input.display().to_string(),
        "--crate-type=lib".to_string(),
        "--edition=2024".to_string(),
        // What rustc works out for the program, `usize::MAX`, `size_of` and
        // `cfg(target_pointer_width)`, is for a 32-bit `usize`, as rust-js's is
        // (ADR 0090). `cfg(rust_js)` says it's rust-js.
        format!("--target={TARGET}"),
        "--cfg=rust_js".to_string(),
        // `#[cfg(browser)]` marks tests that need a real browser (ADR 0027):
        // `-- --cfg browser` turns it on. Declaring any cfg makes rustc check them
        // all, so `test` is declared too, as Cargo does.
        "--check-cfg=cfg(browser, test, rust_js)".to_string(),
    ];
    if test {
        rustc_args.push("--test".to_string());
    }
    // An edition or a target after `--` is the one: rustc takes only one.
    if to_rustc
        .iter()
        .any(|arg| arg == "--edition" || arg.starts_with("--edition="))
    {
        rustc_args.retain(|arg| arg != "--edition=2024");
    }
    if to_rustc
        .iter()
        .any(|arg| arg == "--target" || arg.starts_with("--target="))
    {
        rustc_args.retain(|arg| !arg.starts_with("--target="));
    }
    rustc_args.extend(to_rustc.iter().cloned());
    // `--emit=metadata=<path>` (ADR 0100): rustc writes it to a directory of
    // its own beside the JS, and it's published with the JS, as an artifact
    // of the same plan, checked for collisions with the rest, or not at all.
    let mut metadata: Option<(PathBuf, PathBuf)> = None;
    let mut dep_info: Option<(PathBuf, PathBuf)> = None;
    // Named only when something is staged: WASI, the playground's, has no
    // process id to name it by.
    let beside = output
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    let stage = move || beside.join(format!(".rust-js-metadata-{}", std::process::id()));
    // `--emit mir`, rustc's other spelling, as `--emit=mir`.
    while let Some(i) = rustc_args.iter().position(|arg| arg == "--emit") {
        let kinds = if i + 1 < rustc_args.len() {
            rustc_args.remove(i + 1)
        } else {
            String::new()
        };
        rustc_args[i] = format!("--emit={kinds}");
    }
    for arg in rustc_args.iter_mut() {
        let Some(kinds) = arg.strip_prefix("--emit=") else {
            continue;
        };
        let mut rewritten = Vec::new();
        for kind in kinds.split(',') {
            match kind.strip_prefix("metadata") {
                Some(rest) => {
                    let Some(path) = rest.strip_prefix('=').map(PathBuf::from) else {
                        eprintln!("rust-js: say where a library's metadata goes: --emit=metadata=<path>");
                        return ExitCode::FAILURE;
                    };
                    let staged = stage().join(path.file_name().unwrap_or_default());
                    rewritten.push(format!("metadata={}", staged.display()));
                    metadata = Some((staged, path));
                }
                // rustc's other outputs are written after rust-js has published,
                // past its checks, and are no part of what rust-js makes.
                // Cargo's record of the sources (ADR 0101), staged as the
                // metadata is, and published with it.
                None if cargo && let Some(path) = kind.strip_prefix("dep-info=").map(PathBuf::from) => {
                    let staged = stage().join(path.file_name().unwrap_or_default());
                    rewritten.push(format!("dep-info={}", staged.display()));
                    dep_info = Some((staged, path));
                }
                None => {
                    eprintln!(
                        "rust-js: rustc's `--emit={kind}` isn't something rust-js writes; only a library's --emit=metadata=<path>"
                    );
                    return ExitCode::FAILURE;
                }
            }
        }
        *arg = format!("--emit={}", rewritten.join(","));
    }
    if (metadata.is_some() || dep_info.is_some())
        && let Err(error) = std::fs::create_dir_all(stage())
    {
        eprintln!("rust-js: cannot stage the metadata: {error}");
        return ExitCode::FAILURE;
    }
    let recorded = cargo.then(|| (manifest.clone(), metadata.clone()));
    let settings = match settings::read(&input, cargo) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("rust-js: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut plan = output::OutputPlan::new(input, output, test, manifest);
    plan.metadata = metadata.clone();
    plan.settings = settings;
    let mut callbacks = RustJs {
        dependencies,
        export_library,
        metadata: metadata.is_some(),
        pending: None,
        output: plan,
    };
    let exit = rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&rustc_args, &mut callbacks));
    // Once rustc has written the metadata: if it couldn't, nothing is published.
    let published = match callbacks.pending.take() {
        Some((linked, sources)) if exit == ExitCode::SUCCESS => {
            // What Cargo is told of the build, published with it (ADR 0101).
            let extra = match &recorded {
                Some((Some(manifest), Some((_, metadata)))) => cargo::record(
                    manifest,
                    &dependency_paths,
                    metadata,
                    dep_info
                        .as_ref()
                        .map(|(staged, path)| (staged.as_path(), path.as_path())),
                ),
                _ => Ok(Vec::new()),
            };
            extra
                .and_then(|extra| {
                    callbacks.output.extra = extra;
                    callbacks.output.plan(linked, sources)
                })
                .and_then(|plan| plan.publish())
        }
        _ => Ok(()),
    };
    if metadata.is_some() || dep_info.is_some() {
        let _ = std::fs::remove_dir_all(stage());
    }
    // Once it's all written, the crate's checks of it. Cargo's build runs
    // rust-js for each crate, so its driver is to run them, once (ADR 0117).
    let checked = published.and_then(|()| {
        if exit == ExitCode::SUCCESS && !cargo {
            hooks::check(&callbacks.output.settings, &callbacks.output.written, save)
        } else {
            Ok(())
        }
    });
    match checked {
        Ok(()) => exit,
        Err(error) => {
            eprintln!("rust-js: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The target rustc checks programs for (ADR 0090): WebAssembly's, whose
/// `usize` is 32 bits, as a JS one is here (ADR 0025). Nothing is made for it.
const TARGET: &str = "wasm32-unknown-unknown";
