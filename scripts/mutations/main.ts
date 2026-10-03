// Mutations of src/main.rs (ADR 0093).
import type { Mutation } from "../mutations";

export const mutations: Mutation[] = [
  {
    name: "library-metadata-unstaged",
    breaks: "rustc writes a library's metadata where it's asked for, before its JS is published, and whether it is or not",
    file: "src/main.rs",
    find: "                    rewritten.push(format!(\"metadata={}\", staged.display()));\n",
    replace: "                    rewritten.push(format!(\"metadata={}\", path.display()));\n                    std::fs::create_dir_all(stage()).ok();\n                    std::fs::write(&staged, b\"\").ok();\n",
    tests: ["test/crates.test.ts", "-t", "neither is"],
  },
  {
    name: "rustc-outputs-passed-through",
    breaks: "rustc's other outputs, `--emit=mir` say, are written past rust-js's checks",
    file: "src/main.rs",
    find: "                None => {\n                    eprintln!(\n                        \"rust-js: rustc's `--emit={kind}` isn't something rust-js writes; only a library's --emit=metadata=<path>\"\n                    );\n                    return ExitCode::FAILURE;\n                }\n",
    replace: "                None => rewritten.push(kind.to_string()),\n",
    tests: ["test/crates.test.ts", "-t", "outputs are refused"],
  },
  {
    name: "cargo-record-outside-plan",
    breaks: "what Cargo is told of a crate is written on its own, and a build that fails writing it has changed the JS",
    file: "src/main.rs",
    find: "                    callbacks.output.extra = extra;\n",
    replace: "                    for (path, bytes) in &extra {\n                        let _ = std::fs::write(path, bytes);\n                    }\n",
    tests: ["test/cargo-workspace.test.ts", "-t", "can't record what it made"],
  },
  {
    name: "rust-js-tool-unregistered",
    breaks: "`#[rust_js::link_name]` names a tool rustc doesn't know, and a binding is an error",
    file: "src/main.rs",
    find: "            tools.insert(Ident::with_dummy_span(Symbol::intern(\"rust_js\")));\n",
    replace: "",
    tests: ["test/diagnostics.test.ts","-t","unstable feature rust-js"],
  },
  {
    name: "crate-id-miscomputed",
    breaks: "JSX's expansions are hashed with another crate's id than rustc's",
    file: "src/main.rs",
    find: "    StableCrateId::new(\n        name,\n",
    replace: "    StableCrateId::new(\n        Symbol::intern(\"elsewhere\"),\n",
    tests: ["test/cargo-react.test.ts", "-t", "a component of a Cargo workspace using the react crate"],
  },
  {
    name: "rustc-mode-without-tool",
    breaks: "`rust-js --rustc` is rustc without rust-js's tool, and the binding crates don't compile",
    file: "src/main.rs",
    find: "impl Callbacks for Syntax {\n    fn config(&mut self, config: &mut rustc_interface::interface::Config) {\n        register_tool(config);\n",
    replace: "impl Callbacks for Syntax {\n    fn config(&mut self, _config: &mut rustc_interface::interface::Config) {\n",
    tests: ["test/cargo-react.test.ts"],
  },
  {
    name: "rustc-mode-without-cfg",
    breaks: "`rust-js --rustc` doesn't set `cfg(rust_js)`, and the binding crates' metadata leaves out every `#[rust_js::link_name]`",
    file: "src/main.rs",
    find: "        let ours = [\"rust-js\", \"--cfg=rust_js\", \"--check-cfg=cfg(rust_js)\"].map(String::from);\n",
    replace: "        let ours = [\"rust-js\", \"--check-cfg=cfg(rust_js)\"].map(String::from);\n",
    tests: ["test/snapshots.test.ts","-t","counter"],
  },
];
