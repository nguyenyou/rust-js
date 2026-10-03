// Mutations of src/runtime/append.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "append-keeps-other",
    breaks: "`v.append(&mut other)` leaves `other`'s items in it",
    file: "src/runtime/append.js",
    find: "  other.length = 0;\n",
    replace: "",
    tests: ["test/corpus.test.ts","-t","std_methods"],
  },
];
