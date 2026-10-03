// Mutations of src/lower/format_spec.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "format-without-cells",
    breaks: "`println!(\"{y}\")` of a `&mut` to a number prints its cell, `[object Object]`, not what it points at",
    file: "src/lower/format_spec.rs",
    find: "        let (value, ty) = self.through_refs(value, ty);\n",
    replace: "        let ty = ty.peel_refs();\n",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_local"],
  },
  {
    name: "generic-width-dropped",
    breaks: "a width for a generic `T` is dropped, so `[{:>6}]` of a `3` shows `[3]`",
    file: "src/lower/format_spec.rs",
    find: "            if width.is_some() {\n                return Err(self.unsupported(span, &format!(\"a width for a",
    replace: "            if false && width.is_some() {\n                return Err(self.unsupported(span, &format!(\"a width for a",
    tests: ["test/diagnostics.test.ts", "-t", "generic code"],
  },
  {
    name: "dyn-width-dropped",
    breaks: "a width for a `dyn Display` is dropped, as a generic `T`'s was",
    file: "src/lower/format_spec.rs",
    find: "if self.is_unknown(shown) || matches!(shown.kind(), ty::Dynamic(..)) {",
    replace: "if self.is_unknown(shown) {",
    tests: ["test/diagnostics.test.ts", "-t", "a width for a dyn"],
  },
];
