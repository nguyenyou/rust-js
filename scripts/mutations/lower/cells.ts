// Mutations of src/lower/cells.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "atomic-fetch-new-value",
    breaks: "an atomic's `fetch_add` and the like give the new value, not the old",
    file: "src/lower/cells.rs",
    find: "                out.push(StmtKind::Assign(slot, next).at(js_span));\n                previous\n",
    replace: "                out.push(StmtKind::Assign(slot.clone(), next).at(js_span));\n                slot\n",
    tests: ["test/corpus.test.ts", "-t", "atomics"],
  },
];
