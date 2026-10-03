// Mutations of src/runtime/rfind.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "rfind-first",
    breaks: "`s.rfind(p)` finds the first, not the last",
    file: "src/runtime/rfind.js",
    find: "  const at = s.lastIndexOf(pattern);",
    replace: "  const at = s.indexOf(pattern);",
    tests: ["test/corpus.test.ts", "-t", "string_bytes"],
  },
];
