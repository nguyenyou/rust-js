// Mutations of src/runtime/f32_digits.js (ADR 0093).
import type { Mutation } from "../../mutations";

export const mutations: Mutation[] = [
  {
    name: "f32-digits-symmetric",
    breaks: "an `f32` at a power of two is shown as if the gap below it were the gap above, as it isn't",
    file: "src/runtime/f32_digits.js",
    find: "  const below = fraction === 0 && exponentBits > 1 ? 1n : 2n;",
    replace: "  const below = 2n;",
    tests: ["test/corpus.test.ts", "-t", "f32_digits"],
  },
];
