// Mutations of src/jsx_syntax.rs (ADR 0093).
import type { Mutation } from "../mutations";

export const mutations: Mutation[] = [
  {
    name: "jsx-spans-unmarked",
    breaks: "JSX's `#[rust_js::jsx] f(..)` is an expression's attribute a stable release refuses",
    file: "src/jsx_syntax.rs",
    find: "    let span = expanded(sess, span);\n",
    replace: "",
    tests: ["test/jsx.test.ts","-t","JSX supports components across modules"],
  },
  {
    name: "jsx-call-keeps-its-jsx",
    breaks: "an expression's `jsx!` keeps its JSX, which react's macro makes its placeholder, not the element",
    file: "src/jsx_syntax.rs",
    find: "            mac.args.tokens = arm(rust, span);\n",
    replace: "",
    tests: ["test/jsx.test.ts","-t","JSX supports components across modules"],
  },
];
