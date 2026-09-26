# 0076. A struct's `..base` after its fields, and std's own steps for a few more methods

Status: Accepted. Extends [0020](0020-structs-and-tuples.md), [0051](0051-generic-options.md), [0058](0058-format-options.md), [0062](0062-combinators.md) and [0071](0071-stepping-iterators.md).

## Context

A program with traits, generics and version numbers wrote
`Version { major: 0, minor: 9, ..Default::default() }`, a generic
`largest` that starts from `it.next()?`, and used `scan`, `drain`,
`split_off`, `f64::total_cmp`, `char::from_digit` and `{:e}`. Each was an
error. `words.concat()` of strings was wrong: it made `a,b`, not `ab`.

## Decision

**A struct's `..base` that isn't a place is worked out once, after the
fields,** as Rust does: a field with effects goes in a `const` first, then
the base. A base that's an object of constants, as a derived `Default` is,
is read in place, so its fields are those constants:
`{ major: 0, minor: 9, patch: 0 }`.

**What std does, step for step:**

| Rust | JS |
|---|---|
| `it.next()` of a generic `T`'s | `$nextSome(it)`: a `Some` that looks like `None` is boxed (ADR 0051) |
| `v.iter().scan(init, \|acc, x\| ..)` | `$scan(v, init, f)`: the state in a box (ADR 0074), until `None` |
| `v.drain(a..b)` | `$drain(v, a, b)`: `splice`, with `$slice`'s panics |
| `v.split_off(at)` | `$splitOff(v, at)`: `splice`, with Rust's panic past the end |
| `a.total_cmp(&b)` | `$totalCmp(a, b)`: Rust's, on the bits as `i64`s |
| `char::from_digit(n, radix)`, `char::from_u32(n)` | `$fromDigit(n, radix)`, `$fromU32(n)` |
| `{:e}`, `{:E}` | `$lowerExp(x)`: `1.2345e3`, where JS writes `1.2345e+3` |
| `words.concat()` of strings | `words.join("")` |

- **`{:.2e}` is still an error:** Rust rounds a tie to even, and JS's
  `toExponential(2)` away from zero.
- `$fromDigit` is Rust's arithmetic, not `toString(radix)`, which throws
  for a radix of 1 that Rust takes.

## Why

- **It's Rust's answer.** The example's struct update runs its fields and
  base in Rust's order, and its sorts, scans, drains, `total_cmp` of NaN,
  `-0.0` and the infinities, digits and exponents match native Rust, and
  so do the panics.
