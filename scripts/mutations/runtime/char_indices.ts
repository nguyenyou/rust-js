// Mutations of src/runtime/char_indices.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "char-indices-units",
    breaks: "`char_indices()` counts UTF-16 units, not UTF-8 bytes",
    file: "src/runtime/char_indices.js",
    find: "    at += $byteLen(c);",
    replace: "    at += c.length;",
    tests: ["test/corpus.test.ts", "-t", "string_bytes"],
  },
];
