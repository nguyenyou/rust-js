// Mutations of src/lower/calls.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "unfollowed-drops-taken",
    breaks: "a std call takes an iterator whose destructors rust-js can't follow, and drops what it skips silently",
    file: "src/lower/calls.rs",
    find: "        let holds_drops = |ty: Ty<'tcx>| self.drops(ty) != Drops::Nothing;\n",
    replace: "        let holds_drops = |ty: Ty<'tcx>| self.drops(ty) == Drops::Runs;\n",
    tests: ["test/corpus.test.ts", "-t", "drop_skipped_items"],
  },
  {
    name: "compare-mut-cells",
    breaks: "`p == q` of two `&mut`s to numbers compares their handles, not what they point at",
    file: "src/lower/calls.rs",
    find: "            && generic_args.types().next().is_some_and(|t| self.is_cell(t))",
    replace: "            && generic_args.types().next().is_some_and(|t| false && self.is_cell(t))",
    tests: ["test/corpus.test.ts", "-t", "mut_ref_compare"],
  },
  {
    name: "option-as-mut-same",
    breaks: "`list.next.as_mut()` of a handle on `list.next` is the handle, not the `Option`",
    file: "src/lower/calls.rs",
    find: "            Std::Pointee => self.through_refs(arg(), self.thir[args[0]].ty).0,\n",
    replace: "            Std::Pointee => arg(),\n",
    tests: ["test/corpus.test.ts","-t","generic_mut_ref_kept"],
  },
  {
    name: "push-str-plain-place",
    breaks: "`pick(false, &mut s.0, &mut s.1).push('!')` is refused: `push_str` found only plain places",
    file: "src/lower/calls.rs",
    find: "            let (target, value) = self.prepare_assignment_target(place, true, value, span, out)?;\n",
    replace: "            let (target, value) = (super::places::PreparedPlace::Direct(self.assignee(place)?), value);\n",
    tests: ["test/corpus.test.ts","-t","generic_mut_ref_kept"],
  },
  {
    name: "std-items-accepted",
    breaks: "`m.get_mut(&k)` kept or passed on is taken for a cell, and `*v` reads `v.value` of the number itself",
    file: "src/lower/calls.rs",
    find: "            && !self.locals.item_calls.contains(&fun)\n",
    replace: "            && false\n",
    tests: ["test/diagnostics.test.ts", "-t", "std &mut"],
  },
  {
    name: "type-name-unerased",
    breaks: "`type_name` names the type as written, `Vec<Option<&str>>`, not rustc's paths",
    file: "src/lower/calls.rs",
    find: "            return Ok(Expr::str(rustc_const_eval::util::type_name(self.tcx, of)));",
    replace: "            return Ok(Expr::str(of.to_string()));",
    tests: ["test/corpus.test.ts","-t","std_odds"],
  },
  {
    name: "cmp-max-unknown",
    breaks: "`cmp::max(a, b)` of what isn't a number is refused",
    file: "src/lower/calls.rs",
    find: "                \"std::cmp::max\" | \"core::cmp::max\" => Some(\"max\"),",
    replace: "                \"std::cmp::max\" | \"core::cmp::max\" => None,",
    tests: ["test/corpus.test.ts","-t","std_methods"],
  },
  {
    name: "drain-unmarked",
    breaks: "`collect()` of a chain of owned items doesn't let its stages through",
    file: "src/lower/calls.rs",
    find: "            self.mark_owned_drain(receiver);",
    replace: "            let _ = receiver;",
    tests: ["test/corpus.test.ts", "-t", "collect_drops"],
  },
];
