// Mutations of src/lower/bodies.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "drop-ref-parameter",
    breaks: "a parameter bound by `ref` isn't dropped as its function ends",
    file: "src/lower/bodies.rs",
    find: "                        if self.has_drops(param.ty) {\n                            self.own(*var, Expr::var(&name), param.ty, pat.span, out)?;\n",
    replace: "                        if mode.0 == ByRef::No && self.has_drops(param.ty) {\n                            self.own(*var, Expr::var(&name), param.ty, pat.span, out)?;\n",
    tests: ["test/corpus.test.ts", "-t", "drop_params"],
  },
  {
    name: "generic-mut-param-unboxed",
    breaks: "`x: &mut T` isn't a box: `*x = v` of a generic `T` is refused",
    file: "src/lower/bodies.rs",
    find: " || self.is_generic_boxed(inner, self.typing_env.param_env)",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "generic_mut_ref"],
  },
  {
    name: "parameter-rest-unowned",
    breaks: "what a parameter's pattern leaves of it, `ref b`'s, is never dropped",
    file: "src/lower/bodies.rs",
    find: "                        if self.has_drops(param.ty) {\n                            self.own_rest(",
    replace: "                        if false && self.has_drops(param.ty) {\n                            self.own_rest(",
    tests: ["test/corpus.test.ts","-t","temporaries_taken_apart"],
  },
  {
    name: "closure-body-drops-reversed",
    breaks: "a closure called once drops what it took last first, not in the order it took them",
    file: "src/lower/bodies.rs",
    find: "        for (var, place, ty) in held.into_iter().rev() {",
    replace: "        for (var, place, ty) in held.into_iter() {",
    tests: ["test/corpus.test.ts", "-t", "closure_drops"],
  },
  {
    name: "closure-flags-lost",
    breaks: "a variable a closure took loses its flag, so assigning it again drops what the closure holds",
    file: "src/lower/bodies.rs",
    find: "        self.give_flags(flags);\n",
    replace: "        let _ = flags;\n",
    tests: ["test/corpus.test.ts", "-t", "closure_drops"],
  },
];
