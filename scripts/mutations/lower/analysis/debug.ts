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
  {
    name: "format-options-flag-primitive",
    breaks: "a crate that gives a width to its own types is taken to give none, and the numbers its writers show are unpadded",
    file: "src/lower/analysis/debug.rs",
    find: "                args.first().is_none_or(|&arg| !primitive(thir[arg].ty.peel_refs()))",
    replace: "                args.first().is_some_and(|_| false)",
    tests: ["test/corpus.test.ts", "-t", "options_handed_on"],
  },
  {
    name: "formatter-asked-unseen",
    breaks: "a crate that asks `f.width()`, and gives no placeholder's options, has writers that take none, and the question is an error",
    file: "src/lower/analysis/debug.rs",
    find: "        formatter_query(tcx, id).is_some_and(|query| query != FormatterQuery::Alternate)",
    replace: "        false && formatter_query(tcx, id).is_some_and(|query| query != FormatterQuery::Alternate)",
    tests: ["test/corpus.test.ts", "-t", "formatter_asked"],
  },
];
