// Mutations of src/lower/combinators.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "contains-by-identity",
    breaks: "`refs.contains(&&mut 2)` is `includes`, which compares the cells by identity",
    file: "src/lower/combinators.rs",
    find: "        // A `&mut` to one is a cell, an object (ADR 0099): not by identity.\n        if self.has_cell_layer(ty) {",
    replace: "        // A `&mut` to one is a cell, an object (ADR 0099): not by identity.\n        if false && self.has_cell_layer(ty) {",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "lazy-chain-concat",
    breaks: "`[1, 2].into_iter().chain(repeat(0))` is `concat`, which adds the JS iterator as one item",
    file: "src/lower/combinators.rs",
    find: "            IterComb::Chain if lazy => {",
    replace: "            IterComb::Chain if false && lazy => {",
    tests: ["test/corpus.test.ts","-t","iterator_sources"],
  },
  {
    name: "lazy-zip-eager",
    breaks: "`chars().zip(repeat(7))` is `$zip`, which takes a JS iterator's length to be none",
    file: "src/lower/combinators.rs",
    find: "            IterComb::Zip if lazy => {",
    replace: "            IterComb::Zip if false && lazy => {",
    tests: ["test/corpus.test.ts","-t","iterator_sources"],
  },
  {
    name: "lazy-take-while-eager",
    breaks: "a JS iterator's `take_while` is an array's, which it has no `findIndex` of",
    file: "src/lower/combinators.rs",
    find: "            IterComb::TakeWhile if lazy => {",
    replace: "            IterComb::TakeWhile if false && lazy => {",
    tests: ["test/corpus.test.ts","-t","iterator_sources"],
  },
  {
    name: "lazy-skip-while-eager",
    breaks: "a JS iterator's `skip_while` is an array's, which it has no `findIndex` of",
    file: "src/lower/combinators.rs",
    find: "            IterComb::SkipWhile if lazy => {",
    replace: "            IterComb::SkipWhile if false && lazy => {",
    tests: ["test/corpus.test.ts","-t","iterator_sources"],
  },
  {
    name: "inspect-skipped",
    breaks: "`inspect(f)` doesn't call `f`",
    file: "src/lower/combinators.rs",
    find: "                        StmtKind::Expr(Expr::call(f, vec![item.clone()])).at(js::Span::NONE),\n",
    replace: "",
    tests: ["test/corpus.test.ts","-t","std_methods"],
  },
  {
    name: "next-eager",
    breaks: "`next()` of a chain that does what can be seen runs every item through it first",
    file: "src/lower/combinators.rs",
    find: "        if matches!(op, StepOp::Next | StepOp::Peekable) {\n            self.mark_lazy_chain(args[0], true);\n        }",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "lazy_chains"],
  },
  {
    name: "peekable-of-lazy-accepted",
    breaks: "`peekable()` of a chain that does what can be seen takes all of it first",
    file: "src/lower/combinators.rs",
    find: "            StepOp::Peekable => {\n                if lazy {",
    replace: "            StepOp::Peekable => {\n                if false {",
    tests: ["test/traits.test.ts", "-t", "peekable after"],
  },
];
