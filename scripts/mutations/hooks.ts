// Mutations of src/hooks.rs (ADR 0093).
import type { Mutation } from "../mutations";

export const mutations: Mutation[] = [
  {
    name: "check-said-of-js",
    breaks: "what a check says of the generated JS is printed of the JS, which nobody edits, not the Rust",
    file: "src/hooks.rs",
    find: "        out.push_str(rust.as_deref().unwrap_or(line));",
    replace: "        let _ = rust;\n        out.push_str(line);",
    tests: ["test/settings.test.ts", "-t", "said of the Rust"],
  },
  {
    name: "check-build-only-on-save",
    breaks: "a check only for a build, too slow for a save, runs on each save",
    file: "src/hooks.rs",
    find: "        if save && check.when == When::Build {",
    replace: "        if false && save && check.when == When::Build {",
    tests: ["test/settings.test.ts", "-t", "only in a build"],
  },
];
