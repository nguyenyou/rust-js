// Mutations of src/to_oxc.rs (ADR 0093).
import type { Mutation } from "../mutations";

export const mutations: Mutation[] = [
  {
    name: "runtime-import-missing",
    breaks: "a module compiled against @rust-js/runtime calls its helpers, and neither defines nor imports them",
    file: "src/to_oxc.rs",
    find: "    let helpers = crate::runtime::imported_helpers(&module.runtime, &generated.code);\n",
    replace: "    let helpers = Vec::<&str>::new();\n",
    tests: ["test/runtime-package.test.ts"],
  },
  {
    name: "pair-without-impl",
    breaks: "a number's `&mut dyn` pair is printed without its `impl`, and `d.impl.bump` throws",
    file: "src/to_oxc.rs",
    find: "dictionary.into_iter().chain([get, set])",
    replace: "dictionary.into_iter().take(0).chain([get, set])",
    tests: ["test/corpus.test.ts", "-t", "dyn_mut"],
  },
];
