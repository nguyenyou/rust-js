// Mutations of src/lower/analysis/type_facts.rs (ADR 0093).
import type { Mutation } from "../../../mutations";

export const mutations: Mutation[] = [
  {
    name: "type-facts-not-passed-on",
    breaks: "a generic function passing its `U` on to one that asks `size_of::<T>()` isn't given `U`'s size",
    file: "src/lower/analysis/type_facts.rs",
    find: "                if asked.contains(&(callee, to, fact)) && asked.insert((caller, from, fact)) {",
    replace: "                if false && asked.contains(&(callee, to, fact)) && asked.insert((caller, from, fact)) {",
    tests: ["test/corpus.test.ts", "-t", "type_facts"],
  },
  {
    name: "type-facts-unsized-asked",
    breaks: "`size_of_val` of an unsized `T` asks its caller for a size its type hasn't",
    file: "src/lower/analysis/type_facts.rs",
    find: "                    && of.is_sized(tcx, typing_env)\n",
    replace: "\n",
    tests: ["test/diagnostics.test.ts", "-t", "size_of_val of an unsized"],
  },
];
