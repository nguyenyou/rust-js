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
    name: "debug-width-dropped",
    breaks: "`{:5?}` of `Some(1)` shows `Some(1)`, where Rust gives the width to the `1`",
    file: "src/lower/format_spec.rs",
    find: "                _ => true,\n            };\n            if handed_on && width.is_some() {",
    replace: "                _ => false,\n            };\n            if handed_on && width.is_some() {",
    tests: ["test/diagnostics.test.ts", "-t", "Option's"],
  },
  {
    name: "display-options-handed-on-allowed",
    breaks: "a width for a generic `T`'s `{}` is dropped, as if its `fmt` were one of the crate's that only writes",
    file: "src/lower/format_spec.rs",
    find: "                Std::FmtDisplay => !self.has_user_impl(display, shown) || self.hands_options_on(display, shown)?,",
    replace: "                Std::FmtDisplay => self.hands_options_on(display, shown)?,",
    tests: ["test/diagnostics.test.ts", "-t", "generic code|a width for a dyn"],
  },
];
