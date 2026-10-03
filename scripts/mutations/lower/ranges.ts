// Mutations of src/lower/ranges.rs (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "inclusive-items-end-dropped",
    breaks: "a `RangeInclusive` value's items stop before its end",
    file: "src/lower/ranges.rs",
    find: "                    _ if kind == RangeKind::Inclusive => Expr::bin(Op::Add, end.clone(), num.literal(1)),",
    replace: "                    _ if false && kind == RangeKind::Inclusive => Expr::bin(Op::Add, end.clone(), num.literal(1)),",
    tests: ["test/corpus.test.ts","-t","range_values"],
  },
  {
    name: "contains-inclusive-end-excluded",
    breaks: "`(1..=7).contains(&7)` is false",
    file: "src/lower/ranges.rs",
    find: "                    Expr::bin(Op::Le, item, end.clone()),\n                ],\n                (RangeKind::From, [start])",
    replace: "                    Expr::bin(Op::Lt, item, end.clone()),\n                ],\n                (RangeKind::From, [start])",
    tests: ["test/corpus.test.ts","-t","range_values"],
  },
  {
    name: "range-debug-inclusive-dots",
    breaks: "`{:?}` of `1..=6` is `1..6`",
    file: "src/lower/ranges.rs",
    find: "            RangeKind::Inclusive | RangeKind::ToInclusive => \"..=\",",
    replace: "            RangeKind::ToInclusive => \"..=\",",
    tests: ["test/corpus.test.ts","-t","range_values"],
  },
  {
    name: "range-len-unclamped",
    breaks: "`(5..2).len()` is -3",
    file: "src/lower/ranges.rs",
    find: "                Expr::call(Expr::member(Expr::var(\"Math\"), \"max\"), vec![Expr::int(0), count])",
    replace: "                count",
    tests: ["test/corpus.test.ts","-t","range_values"],
  },
];
