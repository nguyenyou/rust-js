// Mutations of src/runtime/find.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "find-units",
    breaks: "`s.find(p)` is where in UTF-16 units, not UTF-8 bytes",
    file: "src/runtime/find.js",
    find: "  const at = s.indexOf(pattern);\n  return at === -1 ? undefined : $byteLen(s.slice(0, at));",
    replace: "  const at = s.indexOf(pattern);\n  return at === -1 ? undefined : at;",
    tests: ["test/corpus.test.ts", "-t", "string_bytes"],
  },
];
