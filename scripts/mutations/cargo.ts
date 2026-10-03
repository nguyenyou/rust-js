// Mutations of src/cargo.rs (ADR 0093).
import type { Mutation } from "../mutations";

export const mutations: Mutation[] = [
  {
    name: "cargo-build-compiled",
    breaks: "`cargo build` is given metadata and JS for what it links, and fails later, or links rustc's",
    file: "src/cargo.rs",
    find: '    if emitted.iter().any(|kind| kind == "link") {\n',
    replace: '    if emitted.iter().any(|kind| kind == "link") && false {\n',
    tests: ["test/cargo-workspace.test.ts", "-t", "done when their JS is"],
  },
  {
    name: "cargo-probe-compiled",
    breaks: "Cargo's probe of rustc, for what it prints of the target, is compiled by rust-js",
    file: "src/cargo.rs",
    find: '        .any(|flag| flag == "-" || flag == "-vV" || flag.starts_with("--print"));\n',
    replace: '        .any(|flag| flag == "-vV");\n',
    tests: ["test/cargo-workspace.test.ts", "test/cargo-react.test.ts", "-t", "Cargo workspace"],
  },
  {
    name: "cargo-app-not-library",
    breaks: "the package Cargo was asked for is built as the app, and fresh as that when another build uses it",
    file: "src/cargo.rs",
    find: '        "--library".into(),\n    ];\n',
    replace: '    ];\n    if std::env::var_os("CARGO_PRIMARY_PACKAGE").is_none() {\n        ours.push("--library".into());\n    }\n',
    tests: ["test/cargo-workspace.test.ts", "test/cargo-react.test.ts", "-t", "Cargo workspace"],
  },
  {
    name: "cargo-transitive-untold",
    breaks: "a crate is told of the libraries it names, not of those they were compiled against",
    file: "src/cargo.rs",
    find: "        .flat_map(|recorded| recorded.lines().map(PathBuf::from).collect::<Vec<_>>())\n",
    replace: "        .flat_map(|recorded| recorded.lines().take(1).map(PathBuf::from).collect::<Vec<_>>())\n",
    tests: ["test/cargo-workspace.test.ts", "test/cargo-react.test.ts", "-t", "Cargo workspace"],
  },
  {
    name: "cargo-rust-js-untracked",
    breaks: "Cargo has a crate as done when rust-js, which made its JS, has changed",
    file: "src/cargo.rs",
    find: "    first.push(' ');\n",
    replace: "    first.push(' ');\n    let exe = PathBuf::new();\n",
    tests: ["test/cargo-workspace.test.ts", "-t", "done when their JS is"],
  },
  {
    name: "cargo-missing-manifest-ignored",
    breaks: "a library of the workspace whose manifest is gone is used as if rustc had built it",
    file: "src/cargo.rs",
    find: "    if let Some(gone) = found.iter().find(|manifest| !manifest.is_file()) {\n",
    replace: "    found.retain(|manifest| manifest.is_file());\n    if let Some(gone) = found.iter().find(|manifest| !manifest.is_file()) {\n",
    tests: ["test/cargo-workspace.test.ts", "-t", "done when their JS is"],
  },
  {
    name: "cargo-shared-output",
    breaks: "each build of a crate, of a feature set say, writes one place, so a build Cargo has as done has another's JS",
    file: "src/cargo.rs",
    find: '    let dir = out_dir.join("rust-js").join(format!("{name}{extra}"));\n',
    replace: '    let dir = out_dir.join("rust-js").join(&name);\n',
    tests: ["test/cargo-workspace.test.ts", "-t", "another feature set"],
  },
  {
    name: "cargo-bindings-compiled",
    breaks: "the bindings installed inside an app's workspace are its members, and compiled by rust-js, which can't",
    file: "src/cargo.rs",
    find: "BINDINGS.contains(&name.as_str())",
    replace: "BINDINGS.contains(&name.as_str()) && false",
    tests: ["test/cargo-react.test.ts", "-t", "installed inside a Cargo workspace"],
  },
];
