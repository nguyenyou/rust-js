// Mutations of src/runtime/unzip.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "unzip-swapped",
    breaks: "`unzip()` gives the second parts first",
    file: "src/runtime/unzip.js",
    find: "  return [pairs.map(([a]) => a), pairs.map(([, b]) => b)];",
    replace: "  return [pairs.map(([, b]) => b), pairs.map(([a]) => a)];",
    tests: ["test/corpus.test.ts","-t","iterator_sources"],
  },
];
