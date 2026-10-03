// Mutations of src/runtime/byte_len.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "byte-len-units",
    breaks: "a string's `len()` is its UTF-16 units, as JS counts, not its UTF-8 bytes",
    file: "src/runtime/byte_len.js",
    find: "  return bytes;\n}",
    replace: "  return s.length;\n}",
    tests: ["test/corpus.test.ts", "-t", "string_bytes"],
  },
  {
    name: "byte-len-pair-three",
    breaks: "a character outside the BMP, two UTF-16 units, is counted as two 3-byte ones, not 4 bytes",
    file: "src/runtime/byte_len.js",
    find: "      bytes += 4;\n      i++;",
    replace: "      bytes += 3;",
    tests: ["test/corpus.test.ts", "-t", "string_bytes"],
  },
];
