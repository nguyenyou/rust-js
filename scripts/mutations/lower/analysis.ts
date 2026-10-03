// Mutations of src/lower/analysis.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "thread-local-storage-static",
    breaks: "std's storage for a `thread_local!` is taken as a static of the crate's, and rejected",
    file: "src/lower/analysis.rs",
    find: "            DefKind::Static { .. } => !tcx.is_foreign_item(d) && in_thread_local(tcx, d).is_none(),",
    replace: "            DefKind::Static { .. } => !tcx.is_foreign_item(d),",
    tests: ["test/corpus.test.ts", "-t", "thread_local_syntax"],
  },
  {
    name: "js-import-const-written",
    breaks: "`js::import!`'s `const _` is written to the JS as a constant",
    file: "src/lower/analysis.rs",
    find: " && !bindings::is_mark(tcx, d.to_def_id())\n",
    replace: "\n",
    tests: ["test/compiler.test.ts","-t","js::import"],
  },
  {
    name: "pretty-flag-off",
    breaks: "a crate that shows a value with `{:#?}` gives its `Debug`s no `alternate`, and they're plain",
    file: "src/lower/analysis.rs",
    find: "    let pretty_debug = uses_pretty_debug(tcx, all_bodies);",
    replace: "    let pretty_debug = false && uses_pretty_debug(tcx, all_bodies);",
    tests: ["test/corpus.test.ts","-t","pretty_debug"],
  },
];
