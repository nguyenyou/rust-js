//! Owned compiler output. No rustc or printer types cross this boundary.

use crate::js;
use crate::runtime::Helper;
use std::collections::HashSet;
use std::path::PathBuf;

/// Normalized Rust source retained independently of the frontend.
pub struct Source {
    pub path: Option<PathBuf>,
    pub text: String,
    pub line: u32,
}

/// One Rust module's functions: a future JS file (ADR 0019).
pub struct LoweredModule {
    /// The module's path below the crate root: `[]` for the root itself,
    /// `["math", "stats"]` for `crate::math::stats`.
    pub path: Vec<String>,
    /// The `.rs` file the module's code lives in.
    pub file: Option<PathBuf>,
    /// What it imports from JS modules (ADR 0028), with the modules' names
    /// as written in `#[link_name]`.
    pub packages: Vec<js::Package>,
    pub imports: Vec<LoweredImport>,
    pub namespaces: Vec<js::Namespace>,
    pub consts: Vec<js::Const>,
    pub functions: Vec<js::Function>,
    pub caches: Vec<String>,
    /// Runtime helpers its functions use.
    pub runtime: Vec<Helper>,
    /// Whether it has JSX, so it's a `.jsx` file (ADR 0040).
    pub jsx: bool,
}

/// Named exports used from one Rust module, before its JS path is resolved.
pub struct LoweredImport {
    pub path: Vec<String>,
    pub named: Vec<(String, String)>,
}

/// A `#[test]` function (ADR 0026).
pub struct TestFn {
    /// The module it's in, and its JS name there.
    pub module: Vec<String>,
    pub name: String,
    /// What the runner calls it: `tests::adds`.
    pub label: String,
    /// `#[should_panic]`, with its `expected` substring if any.
    pub should_panic: Option<Option<String>>,
    pub ignore: bool,
}

/// Source arena used by the owned JS spans, with original file boundaries.
pub struct Sources {
    pub text: String,
    pub files: Vec<Source>,
}

/// The linked crate as JS: one module per Rust module, and tests in test mode.
pub struct Lowered {
    pub sources: Sources,
    pub modules: Vec<LoweredModule>,
    pub tests: Vec<TestFn>,
}

/// A module dependency expressed without frontend identities.
pub struct ImportRequest {
    pub symbol: js::Symbol,
    pub export: String,
    pub path: Vec<String>,
}

/// Symbolic module output. Imports and runtime closure are filled by linking.
pub struct UnlinkedModule {
    pub module: LoweredModule,
    pub imports: Vec<ImportRequest>,
    pub reserved_names: HashSet<String>,
    pub runtime: HashSet<Helper>,
}

/// Complete lowering output, owned independently of rustc. Only linking turns
/// this into the Lowered value accepted by output planning.
pub struct Unlinked {
    pub sources: Sources,
    pub modules: Vec<UnlinkedModule>,
    pub tests: Vec<TestFn>,
}
