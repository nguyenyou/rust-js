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
];
