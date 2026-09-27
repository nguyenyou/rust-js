# 0086. An `i64` or a `u64` is a BigInt, wrapped as release Rust wraps it

Status: Accepted. Extends [0011](0011-numbers.md), [0064](0064-numbers.md), [0077](0077-serde-json.md) and [0083](0083-serde-json-value.md).

## Context

IDs, timestamps, hashes and money are 64-bit in most APIs: a database's
`BIGINT` key, milliseconds since 1970, cents past $90 trillion, an FNV hash.
They were an error. A JS number is exact only to 2^53, so an ID past it
would read as a neighbour's, and a hash would lose the bits it's made of.

JS has one type that holds every 64-bit integer: the BigInt, `5n`. It's
exact at any size, and `BigInt.asUintN(64, x)` and `BigInt.asIntN(64, x)`
fold an answer back into 64 bits, as ADR 0011's `x | 0` does into 32.

## Decision

**An `i64` or a `u64` is a BigInt,** and a literal is `5n`. Every operation
is exact, then wrapped, as release Rust wraps it (ADR 0011):

| Rust | JS |
|---|---|
| `a + b`, `a - b`, `a * b` | `BigInt.asUintN(64, a + b)`, once for a whole `a + b * c` |
| `a / b`, `a % b` | `a / b`; `$bigDiv(a, b, min)` if `b` may be `0` or `-1` |
| `a & b`, `a \| b`, `a ^ b`, `a >> n` | the same |
| `a << n` | `BigInt.asUintN(64, a << (BigInt(n) & 63n))`, masked as release Rust masks it |
| `x as u64` of a narrower integer | `BigInt(x)`, wrapped if it may not fit |
| `x as u32` of a 64-bit one | `Number(BigInt.asUintN(32, x))`; `Number(x & 1023n)` if a mask keeps it in range |
| `x as f64` | `Number(x)`, its nearest `f64`, as `as` rounds it |
| `u64::from(x)`, `x.into()` | `BigInt(x)` |

Division panics as Rust's does, `attempt to divide by zero` and `attempt to
divide with overflow` for `i64::MIN / -1`, which panics even in release.

**Their methods are Rust's, in small helpers where `Math` takes only
numbers:** `pow` and `checked_pow` square as Rust's do, and `checked_*`,
`saturating_*`, `wrapping_*`, `rem_euclid`, `div_euclid`, `abs`,
`unsigned_abs`, `signum`, `abs_diff`, `leading_zeros`, `trailing_zeros`,
`count_ones`, `is_power_of_two`, `min` and `max` give the same answers. What
they count is a `u32`, a number. `parse` is `$parseBig`, with Rust's errors.

**Everything else is as it is for any integer:** a `match` on ranges and
constants, `{}`, `{:x}` and `{:05}`, a `Vec` sorted, summed or collected
from `a..b`, a map's values, `Default`, and a `const`.

**JSON reads and writes one to the digit.** serde_json's `u64` and `i64`
readers give a BigInt, with serde's errors (`invalid value: integer `-1`,
expected u64`), and a `Value`'s `as_u64()` and `as_i64()` work.

### Also here

- **`f64` to an integer, `x as u8`, saturates,** as Rust's does: `300.0` is
  `255`, `-1.5` is `0`, and `NaN` is `0` (`$f64ToInt`, `$f64ToBig`).
  ADR 0011 had made it an error.
- **`u8::try_from(x)` and `x.try_into()` between integers** are `Ok` of it
  in range, or a `TryFromIntError`, which is its message, as a parse error
  is (ADR 0063): `out of range integral type conversion attempted`, and
  `TryFromIntError(())` with `{:?}`.
- **`unwrap()` of a parse error** shows its `Debug`, `ParseIntError { kind:
  InvalidDigit }`, as Rust's panic does, where it had shown the message.

## Why

- **It's Rust's answer.** The `wide` example packs and unpacks IDs, hashes
  with a wrapping multiply, totals cents past 2^53, casts, parses and
  converts at every edge, reads and writes JSON of `u64::MAX` and
  `i64::MIN`, and panics where Rust does; each value and message matches
  native Rust.
- **A caller from JS sees what JS has for it:** a BigInt in and out, which
  `JSON`, `Intl` and `===` all understand, and never a number that's lost
  digits.
- **The JS reads as the Rust does:** `hash * 1099511628211n`, wrapped once.

## Alternatives

- **A pair of 32-bit numbers, as Scala.js's `RuntimeLong` is:** faster in
  arithmetic, but every `+` is a call, a JS caller gets an object, and
  printing one needs its own code. BigInts are native to every engine
  rust-js targets.
- **A number, exact to 2^53, and an error past it:** plain, but IDs and
  hashes are past it, and a check on every operation is slower than a
  BigInt.

## Consequences

- JS throws if a BigInt and a number are added: a JS caller must pass
  `5n`, not `5`. Comparing them, `5n < 7`, works.
- A BigInt is slower than a number: a hot loop over `u64`s is slower than
  over `u32`s.
- `usize` and `isize` stay 32-bit, as on `wasm32` (ADR 0025), so
  `usize::try_from(1u64 << 40)` is an `Err` where a 64-bit host's is `Ok`.
- `i128`, `u128` and `f32` are still errors.
