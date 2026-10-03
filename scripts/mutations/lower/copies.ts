// Mutations of src/lower/copies.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "copy-on-read",
    breaks: "a `Copy` value read from a place is that place, not a copy",
    file: "src/lower/copies.rs",
    find: "        if self.contains_mutated(ty) && self.is_copy(ty) {",
    replace: "        if false && self.contains_mutated(ty) && self.is_copy(ty) {",
    tests: ["test/corpus.test.ts", "-t", "copy_mutation"],
  },
  {
    name: "library-type-shared",
    breaks: "a library's type is read without a copy by a crate that never changes it itself, so a clone, or `ORIGIN`, is shared",
    file: "src/lower/copies.rs",
    find: "                self.krate.foreign.in_library(adt.did())\n                    || (self.krate.library",
    replace: "                false\n                    || (self.krate.library",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "library-vec-shared",
    breaks: "a crate using a library clones a `Vec` without a copy, which the library changes through its own method",
    file: "src/lower/copies.rs",
    find: "        (self.krate.library || self.krate.foreign.any())\n",
    replace: "        (self.krate.library && self.krate.foreign.any())\n",
    tests: ["test/crates.test.ts", "test/cargo-workspace.test.ts", "-t", "prints what native"],
  },
  {
    name: "library-tuple-shared",
    breaks: "a tuple a library hands out is read without a copy, so changing it changes the library's constant",
    file: "src/lower/copies.rs",
    find: "            ty::Tuple(_) | ty::Array(..) => self.krate.library || self.krate.foreign.any(),\n",
    replace: "            ty::Tuple(_) | ty::Array(..) => false,\n",
    tests: ["test/crates.test.ts", "-t", "two crates: tuple"],
  },
  {
    name: "library-copy-enum-shared",
    breaks: "a copy of a library's `Copy` enum with fields is the same object",
    file: "src/lower/copies.rs",
    find: "            && (adt.did().is_local() || self.krate.foreign.in_library(adt.did()) || self.is_std_adt(ty, sym::Result)))\n",
    replace: "            && (adt.did().is_local() || self.is_std_adt(ty, sym::Result)))\n",
    tests: ["test/crates.test.ts", "-t", "two crates: copy_enum"],
  },
  {
    name: "functions-can-change",
    breaks: "a static of a function, a closure or a `dyn` is taken to change in place, and refused",
    file: "src/lower/copies.rs",
    find: "            ty::FnPtr(..) | ty::FnDef(..) | ty::Closure(..) | ty::Dynamic(..) => true,",
    replace: "            ty::FnPtr(..) | ty::FnDef(..) | ty::Closure(..) | ty::Dynamic(..) => false,",
    tests: ["test/corpus.test.ts", "-t", "static_fns"],
  },
];
