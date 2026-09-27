// Negative controls: the oracle must tell apart what a lax comparison
// wouldn't, or a test it runs proves nothing.

import { expect, test } from "bun:test";

import { decode, expected, normalize, observe, same } from "./oracle";

const panics = (message: string) => () => {
  throw new Error(message);
};

test("a panic is a plain Error, compared by its whole message", () => {
  const native = expected({ panic: "attempt to divide by zero" });
  expect(same(observe(panics("attempt to divide by zero")), native)).toBe(true);
  // Another message, or the same one with more after it.
  expect(same(observe(panics("attempt to divide with overflow")), native)).toBe(false);
  expect(same(observe(panics("attempt to divide by zero!")), native)).toBe(false);
  expect(same(observe(panics("")), native)).toBe(false);
});

test("an exception that isn't a panic never passes for one", () => {
  const native = expected({ panic: "attempt to divide by zero" });
  const typeError = () => {
    throw new TypeError("attempt to divide by zero");
  };
  expect(observe(typeError)).toEqual({ error: "TypeError: attempt to divide by zero" });
  expect(same(observe(typeError), native)).toBe(false);
  // A missing export, BigInt mixed with a number, and a thrown string.
  const missing: Record<string, () => unknown> = {};
  expect(same(observe(() => missing.gone()), native)).toBe(false);
  expect(observe(() => 1n + (1 as any))).toMatchObject({ error: expect.stringContaining("TypeError") });
  expect(observe(() => {
    throw "attempt to divide by zero";
  })).toEqual({ error: "a thrown string: attempt to divide by zero" });
});

test("a value and a panic are different outcomes", () => {
  expect(same(observe(() => 0), expected({ panic: "x" }))).toBe(false);
  expect(same(observe(panics("x")), expected({ value: 0 }))).toBe(false);
});

test("values are compared strictly: -0, NaN, BigInts and None", () => {
  const is = (actual: unknown, native: unknown) => same(observe(() => actual), expected({ value: native }));
  expect(is(-0, -0)).toBe(true);
  expect(is(0, -0)).toBe(false);
  expect(is(NaN, NaN)).toBe(true);
  expect(is(1n, 1)).toBe(false);
  expect(is(18446744073709551615n, 18446744073709551615n)).toBe(true);
  expect(is(2 ** 53, 2 ** 53 + 1)).toBe(true); // One number in JS: why u64s are BigInts.
  expect(is([undefined, { a: undefined }], [null, { a: null }])).toBe(true);
  expect(is({ a: 1 }, { a: 1, b: null })).toBe(false);
  expect(is([1, 2], [1, 2, 3])).toBe(false);
  expect(normalize(new Map([["k", undefined]]))).toEqual(new Map([["k", undefined]]));
});

test("native Rust's tagged values decode to what JS holds", () => {
  expect(decode('[{"$f64":"-0"},{"$f64":"NaN"},{"$f64":"inf"},{"$f64":"-inf"},1.5]')).toEqual([-0, NaN, Infinity, -Infinity, 1.5]);
  expect(Object.is(decode('{"$f64":"-0"}'), -0)).toBe(true);
  expect(decode('{"$bigint":"18446744073709551615"}')).toBe(18446744073709551615n);
  expect(decode('{"$bigint":"-9223372036854775808"}')).toBe(-9223372036854775808n);
  // An object that only looks tagged is left as it is.
  expect(decode('{"$f64":"other"}')).toEqual({ $f64: "other" });
  expect(decode('{"$bigint":"1","x":2}')).toEqual({ $bigint: "1", x: 2 });
});
