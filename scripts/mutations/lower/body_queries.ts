// Mutations of src/lower/body_queries.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "never-loop-value",
    breaks: "a `loop` that never ends, used as a value, is rejected",
    file: "src/lower/body_queries.rs",
    find: "            ExprKind::NeverToAny { source } => match self.thir[source].kind {\n                ExprKind::Loop { body } => Some(body),",
    replace: "            ExprKind::NeverToAny { source } => match self.thir[source].kind {\n                ExprKind::Loop { body } if false => Some(body),",
    tests: ["test/corpus.test.ts", "-t", "loop_values"],
  },
  {
    name: "dyn-iter-lent-not-stepped",
    breaks: "a local lent as a `&mut dyn Iterator` stays an array, so what it's lent to starts it over",
    file: "src/lower/body_queries.rs",
    find: "            && let ExprKind::VarRef { id } = thir[lent(thir, source)].kind\n        {\n            stepped.insert(id);",
    replace: "            && let ExprKind::VarRef { id } = thir[lent(thir, source)].kind\n        {\n            let _ = id;",
    tests: ["test/corpus.test.ts", "-t", "dyn_iterators"],
  },
];
