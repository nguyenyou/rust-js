// Mutations of src/runtime/string_error.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "string-error-debug-unquoted",
    breaks: "`{:?}` of a `Box<dyn Error>` made from a message shows it without quotes",
    file: "src/runtime/string_error.js",
    find: "    Debug: () => ({ fmt: (message) => $debugStr(message) }),",
    replace: "    Debug: () => ({ fmt: (message) => message }),",
    tests: ["test/corpus.test.ts", "-t", "dyn_display"],
  },
];
