// Mutations of src/lower/analysis/debug.rs (ADR 0093).
import type { Mutation } from "../../../mutations";

export const mutations: Mutation[] = [
  {
    name: "pretty-flag-any-alternate",
    breaks: "`{:#}` of a `Value` gives the crate's `Debug`s an `alternate` they're never asked for: right, but not the JS a person writes",
    file: "src/lower/analysis/debug.rs",
    find: "                        &ty::FnDef(made_by, _) => tcx.item_name(made_by).as_str().starts_with(\"new_debug\"),",
    replace: "                        &ty::FnDef(_, _) => true,",
    tests: ["test/snapshots.test.ts","-t","dynamic"],
    snapshots: true,
  },
];
