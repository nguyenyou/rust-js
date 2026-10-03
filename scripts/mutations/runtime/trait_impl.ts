// Mutations of src/runtime/trait_impl.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "impl-dictionary-undefined-key",
    breaks: "a generic impl's dictionary given no drop is keyed by `undefined`, which a `WeakMap` can't hold",
    file: "src/runtime/trait_impl.js",
    find: "  const key = keys[keys.length - 1] ?? $traitImpl;\n",
    replace: "  const key = keys[keys.length - 1];\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl_dictionary"],
  },
];
