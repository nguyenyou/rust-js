// Mutations of src/runtime/range_next.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "range-next-unmoved",
    breaks: "a `Range`'s `next()` gives its start but doesn't move it",
    file: "src/runtime/range_next.js",
    find: "  return range.start < range.end ? range.start++ : undefined;",
    replace: "  return range.start < range.end ? range.start : undefined;",
    tests: ["test/corpus.test.ts","-t","range_values"],
  },
];
