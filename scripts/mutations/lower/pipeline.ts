// Mutations of src/lower/pipeline.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "library-codec-unused",
    breaks: "a library lowers only the codecs it uses itself, so its consumers can't read its types",
    file: "src/lower/pipeline.rs",
    find: "                    (used.contains(&id) || (export_library && super::library::reachable(tcx, id))) && queued.insert(id)\n",
    replace: "                    used.contains(&id) && queued.insert(id)\n",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "library-derive-pruned",
    breaks: "a library leaves out a derived impl only its consumers reach, such as `Debug` of a type it never prints",
    file: "src/lower/pipeline.rs",
    find: "                .filter(|&id| export_library && super::library::reachable(tcx, id)),\n",
    replace: "                .filter(|_| false),\n",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "js-import-unread",
    breaks: "`js::import!(\"./app.css\");` imports nothing",
    file: "src/lower/pipeline.rs",
    find: "            for attr in super::bindings::marks(tcx, module, \"import\") {\n",
    replace: "            for attr in super::bindings::marks(tcx, module, \"none\") {\n",
    tests: ["test/compiler.test.ts","-t","js::import"],
  },
  {
    name: "initializer-reads-statics",
    breaks: "a static's initializer that reads another is lowered, which JS may read before it's made",
    file: "src/lower/pipeline.rs",
    find: "                && !super::body_queries::reads_statics(&body.thir)",
    replace: "",
    tests: ["test/diagnostics.test.ts", "-t", "another static"],
  },
  {
    name: "initializer-a-function",
    breaks: "a static's initializer is made a function, not the static's value",
    file: "src/lower/pipeline.rs",
    find: "                    .or_else(|| initializer.then(|| def_id.expect_local())) =>",
    replace: "                    .or_else(|| (initializer && false).then(|| def_id.expect_local())) =>",
    tests: ["test/corpus.test.ts", "-t", "static_fns"],
  },
];
