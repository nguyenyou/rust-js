// Mutations of src/lower/ordering.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "cmp-without-cells",
    breaks: "`refs.sort()` of a `Vec<&mut i32>` orders the cells, not what they point at",
    file: "src/lower/ordering.rs",
    find: "        let (a, _) = self.through_refs(a, ty);\n        let (b, ty) = self.through_refs(b, ty);\n",
    replace: "        let ty = ty.peel_refs();\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
];
