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
//! Usage: `rust-js [--test] <input.rs> [-o <output.js>] [--manifest <file.json>] [-- <rustc flags>]`. Also
//! writes `<output.js>.map`. Flags after `--` go to rustc unchanged. With
//! `--test`, the crate's `#[test]` functions are compiled too, and
//! `<output>.test.js` runs them with `bun test` (ADR 0026).

#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

mod js;
mod lower;
mod output;
mod prepare;
mod runtime;
mod to_oxc;

use std::path::PathBuf;
use std::process::ExitCode;

use rustc_driver::{Callbacks, Compilation};
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;

struct RustJs {
    output: output::OutputPlan,
}

impl Callbacks for RustJs {
    fn after_expansion<'tcx>(&mut self, _compiler: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        // 1. Copy each function's THIR. MIR building (for borrowck) steals it.
        let bodies = lower::collect_bodies(tcx);

        // 2. Run rustc's full analysis: type check, borrow check, lints.
        tcx.ensure_ok().analysis(());

        // 3. Only a program rustc accepts becomes JavaScript.
        if tcx.dcx().has_errors().is_none()
            && let Some(lowered) = lower::lower_crate(tcx, &bodies)
            && tcx.dcx().has_errors().is_none()
            && let Err(err) = self.output.write(
                lowered,
                tcx.sess
                    .source_map()
                    .files()
                    .iter()
                    .filter_map(|file| file.name.clone().into_local_path())
                    .collect(),
            )
        {
            tcx.dcx().err(format!("rust-js: {err}"));
        }

        // We never want rustc's own codegen.
        Compilation::Stop
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Anything after `--` goes to rustc as-is, e.g. `-- --sysroot /sysroot`.
    let (ours, to_rustc) = match args.iter().position(|a| a == "--") {
        Some(i) => (&args[..i], &args[i + 1..]),
        None => (&args[..], &[][..]),
    };
    let mut ours = ours.to_vec();
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
                "usage: rust-js [--test] <input.rs> [-o <output.js>] [--manifest <file.json>] [-- <rustc flags>]"
            );
            return ExitCode::FAILURE;
        }
    };

    let mut rustc_args = vec![
        "rust-js".to_string(), // argv[0], ignored by rustc
        input.display().to_string(),
        "--crate-type=lib".to_string(),
        "--edition=2024".to_string(),
        // `#[cfg(browser)]` marks tests that need a real browser (ADR 0027):
        // `-- --cfg browser` turns it on. Declaring any cfg makes rustc check them
        // all, so `test` is declared too, as Cargo does.
        "--check-cfg=cfg(browser, test)".to_string(),
        // `#[rust_js::link_name]`, for bindings that are generic (ADR 0039),
        // and `#![rust_js::import = "./App.css"]` inside a module.
        "-Zcrate-attr=feature(register_tool, custom_inner_attributes)".to_string(),
        "-Zcrate-attr=register_tool(rust_js)".to_string(),
    ];
    if test {
        rustc_args.push("--test".to_string());
    }
    rustc_args.extend(to_rustc.iter().cloned());
    let mut callbacks = RustJs {
        output: output::OutputPlan::new(input, output, test, manifest),
    };
    rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&rustc_args, &mut callbacks))
}
