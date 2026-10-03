// Mutations of src/lower/drops/types.rs (ADR 0093).
import type { Mutation } from "../../../mutations";

export const mutations: Mutation[] = [
  {
    name: "drops-walk-uncached",
    breaks: "what a type drops is found again for each path to it, which takes exponential time",
    file: "src/lower/drops/types.rs",
    find: "            self.drop_state.cache.borrow_mut().insert(ty, found);\n",
    replace: "",
    tests: ["test/corpus.test.ts", "-t", "nested_generic_types"],
  },
  {
    name: "dyn-drops-nothing",
    breaks: "a `dyn` of the crate's trait is taken to drop nothing, and what it holds is never dropped",
    file: "src/lower/drops/types.rs",
    find: "is_some_and(|id| self.is_rust_trait(id)) => {\n                match self.may_have_destructors() {\n                    true => Drops::Runs,",
    replace: "is_some_and(|id| self.is_rust_trait(id)) => {\n                match self.may_have_destructors() {\n                    true => Drops::Nothing,",
    tests: ["test/corpus.test.ts", "-t", "dyn_drops"],
  },
  {
    name: "library-drop-skipped",
    breaks: "a library's type's destructor isn't run by a crate that drops a value of it",
    file: "src/lower/drops/types.rs",
    find: "        drop.is_local() || self.krate.foreign.item(drop).is_some()\n",
    replace: "        drop.is_local()\n",
    tests: ["test/crates.test.ts", "-t", "two crates: drop"],
  },
  {
    name: "projection-drops-nothing",
    breaks: "an associated type's value is taken to have nothing to drop where a type has a destructor, and its destructor never runs",
    file: "src/lower/drops/types.rs",
    find: "            ) if self.is_unknown(ty) => match self.may_have_destructors() {\n",
    replace: "            ) if self.is_unknown(ty) => match false {\n",
    tests: ["test/diagnostics.test.ts","-t","associated type"],
  },
  {
    name: "closure-part-taken",
    breaks: "a closure that takes part of a value with a destructor is taken as holding it whole",
    file: "src/lower/drops/types.rs",
    find: "                Drops::Runs if self.takes_whole(*def_id) => Drops::Runs,",
    replace: "                Drops::Runs if true || self.takes_whole(*def_id) => Drops::Runs,",
    tests: ["test/traits.test.ts", "-t", "closure holding part"],
  },
  {
    name: "channel-ends-drop-nothing",
    breaks: "a channel's ends have no destructor, so a sender dropped still counts",
    file: "src/lower/drops/types.rs",
    find: "                match self.drops_in(args.type_at(0), walk) {\n                    Drops::Nothing => Drops::Runs,\n                    _ => Drops::Unsupported(ty, \"a channel of a value with a destructor\"),",
    replace: "                match self.drops_in(args.type_at(0), walk) {\n                    Drops::Nothing => Drops::Nothing,\n                    _ => Drops::Unsupported(ty, \"a channel of a value with a destructor\"),",
    tests: ["test/corpus.test.ts", "-t", "channels"],
  },
];
