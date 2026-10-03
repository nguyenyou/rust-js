// Mutations of src/runtime/checked_index.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "index-panic-message",
    breaks: "an index out of bounds panics with another message than Rust's",
    file: "src/runtime/checked_index.js",
    find: "function $index(items, index) {\n  if (index < 0 || index >= items.length) throw new Error(`index out of bounds: the len is",
    replace: "function $index(items, index) {\n  if (index < 0 || index >= items.length) throw new Error(`index out of bounds: the length is",
    tests: ["test/corpus.test.ts", "-t", "index_out_of_bounds"],
  },
];
