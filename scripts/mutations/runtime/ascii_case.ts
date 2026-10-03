// Mutations of src/runtime/ascii_case.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "ascii-case-unicode",
    breaks: "`to_ascii_lowercase` lowers every letter, `Ü` too, as JS's `toLowerCase` does",
    file: "src/runtime/ascii_case.js",
    find: "    : text.replace(/[A-Z]+/g, (letters) => letters.toLowerCase());",
    replace: "    : text.toLowerCase();",
    tests: ["test/corpus.test.ts","-t","std_methods"],
  },
];
