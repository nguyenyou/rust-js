// Mutations of src/lower/serde/de.rs (ADR 0093).
import type { Mutation } from "../../../mutations";

export const mutations: Mutation[] = [
  {
    name: "json-value-set",
    breaks: "a `HashSet<Point>` read from JSON is a plain `Set`, which keeps equal points twice",
    file: "src/lower/serde/de.rs",
    find: "                if self.is_value_key(item) && !self.is_js_key(item) {",
    replace: "                if false && self.is_value_key(item) && !self.is_js_key(item) {",
    tests: ["test/serde.test.ts", "-t", "set of structs"],
  },
];
