// Mutations of src/lower/analysis/mutation.rs (ADR 0093).
import type { Mutation } from "../../../mutations";

export const mutations: Mutation[] = [
  {
    name: "range-step-untracked",
    breaks: "a range `next()` changes is taken to never change, so a clone of it is the same object",
    file: "src/lower/analysis/mutation.rs",
    find: "                } if enum_ty(body.thir[arg].ty) || range_ty(body.thir[arg].ty) => {",
    replace: "                } if enum_ty(body.thir[arg].ty) => {",
    tests: ["test/corpus.test.ts","-t","range_values"],
  },
  {
    name: "for-loop-steps-counted",
    breaks: "a `for` over a range counts as changing it, so its end is a `const` first: right, but not the JS a person writes",
    file: "src/lower/analysis/mutation.rs",
    find: "                !expr.span.is_desugaring(DesugaringKind::ForLoop)\n",
    replace: "                true\n",
    tests: ["test/corpus.test.ts","-t","range_values"],
    snapshots: true,
  },
];
