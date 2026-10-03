// Mutations of src/runtime/successors.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "successors-behind",
    breaks: "`successors` finds the next item after it gives this one, calling its closure once less",
    file: "src/runtime/successors.js",
    find: "    next = successor(item);\n    yield item;",
    replace: "    yield item;\n    next = successor(item);",
    tests: ["test/corpus.test.ts","-t","iterator_sources"],
  },
];
