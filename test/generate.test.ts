// The generator and the reducer (ADR 0092), without running what they make.

import { expect, test } from "bun:test";

import { generate, print, random, reduce, size, type Program } from "./generate";

test("a seed is the same program, and another seed another", () => {
  expect(print(generate(7))).toBe(print(generate(7)));
  const programs = new Set(Array.from({ length: 50 }, (_, seed) => print(generate(seed))));
  expect(programs.size).toBe(50);
  // The same numbers from the same seed, and in [0, 1).
  const a = random(3), b = random(3);
  const draws = Array.from({ length: 100 }, () => a.next());
  expect(draws).toEqual(Array.from({ length: 100 }, () => b.next()));
  expect(draws.every((x) => x >= 0 && x < 1)).toBe(true);
});

test("a program is `main` and `id`, which hides each literal from rustc", () => {
  const text = print(generate(1), 1);
  expect(text).toStartWith("// Generated from seed 1.\nfn id<T>(x: T) -> T {");
  expect(text).toContain("#[allow(unused, arithmetic_overflow, unconditional_panic)]\nfn main() {");
  // Every typed literal is `id(..)`'s, so rustc can't work out an overflow.
  const literals = text.match(/-?\b\d+[iu](8|16|32|64)\b/g) ?? [];
  const hidden = text.match(/id\(-?\d+[iu](8|16|32|64)\)/g) ?? [];
  expect(literals.length).toBeGreaterThan(5);
  expect(hidden.length).toBe(literals.length);
});

const seven = { kind: "lit", ty: "i32", value: 7n } as const;
const one = { kind: "lit", ty: "i32", value: 1n } as const;

test("a failing program is reduced to as little as still fails", async () => {
  // Fails while it prints 7, wherever that is.
  const program: Program = [
    { kind: "let", name: "v0", ty: "i32", value: one },
    { kind: "print", format: "{}", value: one },
    {
      kind: "for",
      name: "k1",
      n: 2,
      body: [
        { kind: "if", c: { kind: "lit", ty: "bool", value: true }, then: [{ kind: "print", format: "{}", value: { kind: "bin", ty: "i32", op: "+", a: seven, b: one } }], else: [] },
      ],
    },
    { kind: "print", format: "{:x}", value: one },
  ];
  let tried = 0;
  const smallest = await reduce(program, async (candidate) => {
    tried++;
    return print(candidate).includes("id(7i32)");
  });
  expect(size(program)).toBe(6);
  // The loop and the `if` around it, and the `+` in it, are gone.
  expect(smallest).toEqual([{ kind: "print", format: "{}", value: seven }]);
  expect(tried).toBeGreaterThan(5);
});

test("a program that fails only as it is stays as it is", async () => {
  const program = generate(5);
  const text = print(program);
  expect(await reduce(program, async (candidate) => print(candidate) === text)).toEqual(program);
});
