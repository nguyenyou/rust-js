// Mutations of src/lower/analysis/drops.rs (ADR 0093).
import type { Mutation } from "../../../mutations";

export const mutations: Mutation[] = [
  {
    name: "library-generic-drops-nothing",
    breaks: "a library's generic function isn't given a `dropT`, so a consumer's value with a destructor is never dropped",
    file: "src/lower/analysis/drops.rs",
    find: "    if library {\n        for &id in fns.keys() {\n",
    replace: "    if false {\n        for &id in fns.keys() {\n",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "impl-drops-unseeded",
    breaks: "a generic impl isn't given drops, so what its methods, their helpers, or a default of its trait drop is never dropped",
    file: "src/lower/analysis/drops.rs",
    find: "        if drops_nothing_derived(tcx, imp) {\n",
    replace: "        if true {\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl"],
  },
  {
    name: "default-body-drops-unpassed",
    breaks: "a default body gives its value to a generic helper, which isn't given a drop, and drops nothing",
    file: "src/lower/analysis/drops.rs",
    find: "        let Some(trait_id) = tcx.trait_of_assoc(method) else {\n",
    replace: "        let Some(trait_id) = tcx.trait_of_assoc(method).filter(|_| false) else {\n",
    tests: ["test/corpus.test.ts", "-t", "drop_impl_indirect"],
  },
  {
    name: "drops-without-destructors",
    breaks: "a crate with no destructor gives its generic impls drops nothing calls: right, but not the JS a person writes",
    file: "src/lower/analysis/drops.rs",
    find: "    if !library && !crate::lower::traits::may_have_destructors(tcx, foreign) {",
    replace: "    if false {",
    tests: ["test/corpus.test.ts", "-t", "generic_traits"],
    snapshots: true,
  },
  {
    name: "library-drops-skipped",
    breaks: "a library with no destructor gives its generic impls no drops, though its consumers' values may have one",
    file: "src/lower/analysis/drops.rs",
    find: "    if !library && !crate::lower::traits::may_have_destructors(tcx, foreign) {",
    replace: "    if !crate::lower::traits::may_have_destructors(tcx, foreign) {",
    tests: ["test/crates.test.ts", "-t", "impl_drops"],
  },
];
