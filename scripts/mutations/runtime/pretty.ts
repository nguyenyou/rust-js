// Mutations of src/runtime/pretty.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "pretty-not-indented",
    breaks: "a nested part of `{:#?}`'s lines aren't indented",
    file: "src/runtime/pretty.js",
    find: "  const lines = items.map((item) => \"    \" + item.replaceAll(\"\\n\", \"\\n    \") + \",\\n\").join(\"\");",
    replace: "  const lines = items.map((item) => \"    \" + item + \",\\n\").join(\"\");",
    tests: ["test/corpus.test.ts","-t","pretty_debug"],
  },
];
