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
    name: "options-not-handed-on",
    breaks: "`{:8.2}` of a type whose `fmt` hands its `Formatter` on to an `f64`'s shows it unpadded, with all its digits",
    file: "src/lower/format_spec.rs",
    find: "        if options && !leaf && matches!(kind, Std::FmtDisplay | Std::FmtDebug) {",
    replace: "        if options && !leaf && matches!(kind, Std::FmtDisplay | Std::FmtDebug) && false {",
    tests: ["test/corpus.test.ts", "-t", "options_handed_on|debug_options"],
  },
  {
    name: "options-parts-not-given",
    breaks: "`{:5?}` of `Some(1)` gives the `1` no width, where Rust gives a `{:?}`'s options to each part",
    file: "src/lower/format_spec.rs",
    find: "            return self.with_options(Some(options), |cx| match kind {",
    replace: "            return self.with_options(None.filter(|_: &Options| options.spec.plus), |cx| match kind {",
    tests: ["test/corpus.test.ts", "-t", "debug_options"],
  },
  {
    name: "options-object-without-align",
    breaks: "`{:<9.1}` of a type whose `fmt` hands its `Formatter` on to a number pads it on the left",
    file: "src/lower/format_spec.rs",
    find: "        field(\"align\", Expr::str(align.to_string()));",
    replace: "        let _ = align;",
    tests: ["test/corpus.test.ts", "-t", "options_handed_on"],
  },
  {
    name: "options-object-without-precision",
    breaks: "`{:8.2}` of a type whose `fmt` hands its `Formatter` on to an `f64` shows all its digits",
    file: "src/lower/format_spec.rs",
    find: "        field(\"precision\", precision.clone());",
    replace: "        let _ = precision;",
    tests: ["test/corpus.test.ts", "-t", "options_handed_on"],
  },
  {
    name: "debug-bool-unpadded",
    breaks: "`{:5?}` of `true` isn't padded, where a `bool`'s `Debug` is its `Display`, which pads",
    file: "src/lower/format_spec.rs",
    find: "        let pads = num.is_some()\n            || ty.is_bool()\n",
    replace: "        let pads = num.is_some()\n",
    tests: ["test/corpus.test.ts", "-t", "debug_options"],
  },
];
