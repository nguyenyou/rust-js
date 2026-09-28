// The differential oracle: what native Rust did with a call, and what the
// generated JS did, in one shape, compared exactly.
//
//   native Rust ── JSON line ──► decode ──► { value } | { panic: message } ─┐
//   generated JS ─── observe ──► { value } | { panic } | { error } ─────────┴─► same?
//
// A Rust panic is a plain `Error` with Rust's message (ADR 0012). Any other
// exception, a `TypeError` say, is the generated JS going wrong, so it's an
// `error`, which no native outcome is.

export type Outcome = { value: unknown } | { panic: string } | { error: string };

// JSON has no -0, NaN, infinities or BigInts, so the native side tags them:
// `{"$f64":"-0"}`, `{"$bigint":"18446744073709551615"}`.
const floats: Record<string, number> = { "-0": -0, NaN: NaN, inf: Infinity, "-inf": -Infinity };

/** A line of JSON native Rust printed, with its tagged values restored. */
export function decode(line: string): any {
  return JSON.parse(line, (_, v) => {
    if (v === null || typeof v !== "object" || Array.isArray(v)) return v;
    const keys = Object.keys(v);
    if (keys.length === 1 && keys[0] === "$f64" && v.$f64 in floats) return floats[v.$f64];
    if (keys.length === 1 && keys[0] === "$bigint") return BigInt(v.$bigint);
    return v;
  });
}

/** Values as JSON, tagged as native Rust tags them, for `decode` to restore
 * in another process. What no Rust value is, a `Map` or a function, is
 * tagged by its kind, so it can't come back as one that is. */
export function encode(value: unknown): string {
  const tagged = (v: unknown): unknown => {
    if (typeof v === "bigint") return { $bigint: String(v) };
    if (typeof v === "number") {
      if (Object.is(v, -0)) return { $f64: "-0" };
      if (Number.isNaN(v)) return { $f64: "NaN" };
      if (v === Infinity) return { $f64: "inf" };
      if (v === -Infinity) return { $f64: "-inf" };
      return v;
    }
    if (v === undefined || v === null || typeof v === "string" || typeof v === "boolean") return v ?? null;
    if (Array.isArray(v)) return v.map(tagged);
    if (typeof v === "object" && Object.getPrototypeOf(v) === Object.prototype) {
      return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, tagged(x)]));
    }
    return { $js: typeof v === "object" ? (v.constructor?.name ?? "object") : typeof v };
  };
  return JSON.stringify(tagged(value));
}

/** Run generated JS as native Rust ran the Rust, and say what happened. */
export function observe(run: () => unknown): Outcome {
  try {
    return { value: normalize(run()) };
  } catch (e) {
    if (e instanceof Error && e.constructor === Error) return { panic: e.message };
    return { error: e instanceof Error ? `${e.name}: ${e.message}` : `a thrown ${typeof e}: ${String(e)}` };
  }
}

/** `None` is `undefined` in JS and `null` in JSON; everything else is kept. */
export function normalize(v: unknown): unknown {
  if (v === undefined) return null;
  if (Array.isArray(v)) return v.map(normalize);
  if (v !== null && typeof v === "object" && Object.getPrototypeOf(v) === Object.prototype) {
    return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, normalize(x)]));
  }
  return v;
}

/** What native Rust recorded: `{ value }`, or `{ panic }` with its message. */
export function expected(c: { value?: unknown; panic?: string }): Outcome {
  return c.panic !== undefined ? { panic: c.panic } : { value: c.value };
}

/** Strictly the same: -0 isn't 0, 1n isn't 1, a `TypeError` isn't a panic. */
export function same(a: Outcome, b: Outcome): boolean {
  return Bun.deepEquals(a, b, true);
}
