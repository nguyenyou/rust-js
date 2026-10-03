// Mutations of src/jsx_syntax/parser.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "props-macro-not-allowed",
    breaks: "a component's props `macro` writes `#[rust_js::jsx]` on an expression, which its expansion may not",
    file: "src/jsx_syntax/parser.rs",
    find: "        format!(\"#[allow_internal_unstable(stmt_expr_attributes)] macro {ident} {body}\"),\n",
    replace: "        format!(\"macro {ident} {body}\"),\n",
    tests: ["test/crates.test.ts","-t","component of another crate"],
  },
];
