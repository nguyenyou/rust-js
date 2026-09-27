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
extern crate rustc_builtin_macros;
extern crate rustc_driver;
extern crate rustc_expand;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_parse;
extern crate rustc_session;
extern crate rustc_span;

mod format;
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
mod to_oxc;

use std::path::PathBuf;
use std::process::ExitCode;

use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;
use rustc_session::Session;
use rustc_span::{Symbol, sym};

struct RustJs {
    output: output::OutputPlan,
    dependencies: library::Dependencies,
    export_library: bool,
}

impl Callbacks for RustJs {
    fn after_crate_root_parsing(&mut self, compiler: &Compiler, krate: &mut rustc_ast::Crate) -> Compilation {
        enable_features(&compiler.sess, krate);
        jsx_syntax::expand(&compiler.sess, krate);
        Compilation::Continue
    }

    fn after_expansion<'tcx>(&mut self, _compiler: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
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
            && let Err(err) = self
                .output
                .plan(
                    link::link(unlinked),
                    tcx.sess
                        .source_map()
                        .files()
                        .iter()
                        .filter(|file| file.src.is_some())
                        .filter_map(|file| file.name.clone().into_local_path())
                        .chain(self.dependencies.inputs.iter().cloned())
                        .collect(),
                )
                .and_then(|plan| plan.publish())
        {
            tcx.dcx().err(format!("rust-js: {err}"));
        }

        // We never want rustc's own codegen.
        Compilation::Stop
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
                "usage: rust-js [--test] <input.rs> [-o <output.js>] [--manifest <file.json>] [--library] [--dependency <manifest.json>] [-- <rustc flags>]"
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
    let mut callbacks = RustJs {
        dependencies,
        export_library,
        output: output::OutputPlan::new(input, output, test, manifest),
    };
    rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&rustc_args, &mut callbacks))
}

/// The target rustc checks programs for (ADR 0090): WebAssembly's, whose
/// `usize` is 32 bits, as a JS one is here (ADR 0025). Nothing is made for it.
const TARGET: &str = "wasm32-unknown-unknown";

/// `#[rust_js::link_name]`, for bindings that are generic (ADR 0039), and
/// `#![rust_js::import = "./App.css"]` inside a module need these features
/// and the `rust_js` tool, which a program never asks for. What the crate
/// root has already, itself or by a `cfg_attr` whose `cfg` holds, isn't
/// added again, which rustc rejects: its attributes are read as rustc
/// configures them, before rustc reads which features are on.
fn enable_features(sess: &Session, krate: &mut rustc_ast::Crate) {
    let attrs = jsx_syntax::configured_attrs(sess, &krate.attrs).unwrap_or_default();
    let listed = |name: Symbol| -> Vec<Symbol> {
        attrs
            .iter()
            .filter(|attr| attr.has_name(name))
            .flat_map(|attr| attr.meta_item_list().unwrap_or_default())
            .filter_map(|item| item.ident().map(|ident| ident.name))
            .collect()
    };
    let (features, tools) = (listed(sym::feature), listed(sym::register_tool));
    let mut missing: Vec<String> = [
        sym::register_tool,
        sym::custom_inner_attributes,
        sym::decl_macro,
        sym::stmt_expr_attributes,
    ]
    .into_iter()
    .filter(|feature| !features.contains(feature))
    .map(|feature| format!("feature({feature})"))
    .collect();
    if !tools.contains(&Symbol::intern("rust_js")) {
        missing.push("register_tool(rust_js)".to_string());
    }
    rustc_builtin_macros::cmdline_attrs::inject(krate, &sess.psess, &missing);
}
