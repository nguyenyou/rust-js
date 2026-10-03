// Mutations of src/runtime/char_range.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "char-range-surrogates",
    breaks: "a range of `char`s gives the surrogates, which no `char` is",
    file: "src/runtime/char_range.js",
    find: "    if (c < 0xd800 || c > 0xdfff) {",
    replace: "    if (true) {",
    tests: ["test/corpus.test.ts","-t","range_values"],
  },
];
