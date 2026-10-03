// Mutations of src/runtime/parse_f32.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "parse-f32-through-f64",
    breaks: "`s.parse::<f32>()` is the `f64` nearest the digits, rounded to an `f32`, which a hair off a tie rounds as the tie",
    file: "src/runtime/parse_f32.js",
    find: "  if (read.TAG === \"Err\" || !Number.isFinite(d) || Math.fround(d) === d) {",
    replace: "  if (true) {",
    tests: ["test/corpus.test.ts", "-t", "f32_parse"],
  },
];
