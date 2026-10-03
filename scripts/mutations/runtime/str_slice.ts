// Mutations of src/runtime/str_slice.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "str-slice-units",
    breaks: "`&s[a..b]` slices UTF-16 units, not UTF-8 bytes",
    file: "src/runtime/str_slice.js",
    find: "  return s.slice($unitAt(s, start, \"start\"), $unitAt(s, end, \"end\"));",
    replace: "  return s.slice(start, end);",
    tests: ["test/corpus.test.ts", "-t", "string_bytes"],
  },
  {
    name: "str-slice-inside-char",
    breaks: "slicing inside a character gives part of it, where Rust panics",
    file: "src/runtime/str_slice.js",
    find: "    if (next > at) {\n      throw",
    replace: "    if (false) {\n      throw",
    tests: ["test/corpus.test.ts", "-t", "string_slice_boundary"],
  },
  {
    name: "str-slice-past-end",
    breaks: "slicing past a string's end gives what's there, where Rust panics",
    file: "src/runtime/str_slice.js",
    find: "  if (end > length) throw new Error(`end byte index",
    replace: "  if (false) throw new Error(`end byte index",
    tests: ["test/corpus.test.ts", "-t", "string_slice_bounds"],
  },
];
