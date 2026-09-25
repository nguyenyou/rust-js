# 0011. Integers are JS numbers, wrapped like release Rust

Status: Accepted

## Context

JS has one number type: a 64-bit float (f64). Rust has `i8` through `u128`,
each with fixed-width wraparound. We need Rust's answers from JS numbers.

The key fact: **an f64 holds every integer up to 2^53 exactly.** For two
32-bit values, sums and differences are far smaller than that, so JS computes
them *exactly*. All we have to do is fold the exact answer back into the
type's range, which is what wrapping means.

## Decision

**Supported types:** `i8`, `i16`, `i32`, `u8`, `u16`, `u32`, `f64`, `bool`.
Anything else (`i64`, `u64`, `usize`, `f32`, `char`, ...) is a
compile error for now.

**Overflow wraps**, matching Rust's release profile (`overflow-checks = off`).

**The wrap table**, meaning how an exact result goes back into range:

| Type | Wrap | Why it works |
|---|---|---|
| `i32` | `x \| 0` | JS bitwise ops convert to signed 32-bit (ToInt32) |
| `u32` | `x >>> 0` | `>>>` converts to unsigned 32-bit (ToUint32) |
| `i8`, `i16` | `x << 24 >> 24`, `x << 16 >> 16` | shift up, then arithmetic-shift down to sign-extend |
| `u8`, `u16` | `x & 255`, `x & 65535` | keep the low bits |
| `f64` | none | JS numbers *are* f64 |

**Per operation:**

| Op | Integers | Notes |
|---|---|---|
| `+ -` | wrap(`a op b`) | exact, then wrap |
| `*` (32-bit) | `Math.imul(a, b)` (u32: `>>> 0` after) | a 32×32 product can reach 2^64, past 2^53, so a plain `*` would lose low bits. `imul` doesn't. |
| `*` (8/16-bit) | wrap(`a * b`) | product < 2^32, exact |
| `/` | wrap(`a / b`) | float division then truncation. For 32-bit operands the float quotient never rounds across an integer, so truncation gives Rust's answer. Zero/overflow checks: [0012](0012-panics-and-runtime-helpers.md) |
| `%` | `a % b` | JS `%` on integers is exact, and its sign follows the dividend, like Rust |
| `& \| ^` | as-is; u32: `>>> 0` after | JS returns *signed* 32-bit results, which only u32 must fix |
| `<<` | wrap(`a << b`); 8/16-bit: `b & 7` / `b & 15` | release Rust masks the shift amount to the type's width. JS masks to 5 bits, which is only right for 32-bit types |
| `>>` | signed `>>`, unsigned `>>>` (masked the same way) | arithmetic vs logical shift |
| `== != < ...` | `=== !== < ...` | u32 values are stored unsigned, so comparisons just work |
| `-x` | wrap(`-x`) | `-i32::MIN` wraps to itself, as in Rust |
| `!x` | `~x`; unsigned: wrap(`~x`) | |

**`bool`** is a JS boolean. `&` and `|` on bools become `!!(a & b)` and
`!!(a | b)` (both sides always evaluated, result is a boolean), and `^`
becomes `a !== b`.

**Casts (`as`):**

- int → int: nothing if the value always fits (`u8 as u32`), otherwise wrap
  to the target (`x as u8` → `x & 255`).
- int → f64: nothing, since every supported integer is exact in f64.
- bool → int: `b ? 1 : 0`.
- f64 → int: **a compile error for now.** Rust saturates (`300.0 as u8` is
  255, NaN is 0), while `| 0` is modular, and we won't ship the wrong one.

**Literals** are stored as their exact `f64` value. Every supported integer
is exact in an f64, so nothing is lost. Whole numbers print in plain decimal,
as written (`1000`, not oxc's shortest form `1e3`), and fractions print in
their shortest round-trip form ([0018](0018-print-with-oxc.md)). Negative
literals get parentheses where precedence needs them.

## Why release semantics?

Debug Rust panics on overflow. Release Rust wraps. We chose wrapping for now:

- it's what shipped Rust code does, and it's cheap and readable in JS;
- modeling debug panics means a check around almost every `+`, which hurts
  both size and readability.

A future `--overflow-checks` flag could emit checked helpers.

## Alternatives

- **Everything via `BigInt`**: exact for all widths, but slow, and it can't
  mix with plain JS numbers. We'll likely use it (or a Scala.js-style
  `RuntimeLong`) for 64-bit types later.
- **Typed arrays** for wrapping (`Int32Array` stores): correct but obscure.

## Consequences

- **This is a documented behavior difference from debug Rust:** `u32::MAX + 1`
  is `0` in rust-js, and a panic in a debug build.
- `usize` isn't supported yet. It's 64-bit on the host, and deciding how it
  maps is a separate decision.
