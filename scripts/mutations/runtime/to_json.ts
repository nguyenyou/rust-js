// Mutations of src/runtime/to_json.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "json-f32-as-f64",
    breaks: "an `f32` in JSON is written as its `f64`, `0.10000000149011612`, where serde_json writes `0.1`",
    file: "src/runtime/to_json.js",
    find: "this.text += single ? $jsonF32(x) : $jsonNumber(x);",
    replace: "this.text += $jsonNumber(x);",
    tests: ["test/serde.test.ts", "-t", "an f32 is written"],
  },
];
