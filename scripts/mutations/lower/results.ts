// Mutations of src/lower/results.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "question-box-error-unconverted",
    breaks: "`?` into a `Box<dyn Error>` returns the error as it is, not as a pair",
    file: "src/lower/results.rs",
    find: "                (None, Some(dictionary)) => Some(Expr::object(vec![",
    replace: "                (None, Some(_)) => Some(error.clone()),\n                (None, Some(dictionary)) => Some(Expr::object(vec![",
    tests: ["test/corpus.test.ts", "-t", "dyn_display"],
  },
];
