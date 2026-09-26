# 0064. Numbers' methods are `Math`'s, where JS agrees; operators call their impl

Status: Accepted. Extends [0025](0025-vec-loops-refcell-mut.md), [0031](0031-consts.md) and [0052](0052-std-trait-impls.md).

## Context

A program that computes calls numbers' methods (`x.sqrt()`, `n.pow(2)`,
`a.checked_add(b)`), puts operators on its own types (`impl Add for Vec2`),
names constants on them (`Vec2::ZERO`), and makes grids (`vec![vec![0;
n]; n]`). All of these were errors.

A number is a JS number (ADR 0025): integers are kept in range by wrapping
each result. JS's `Math` has most of Rust's methods, but not all agree:
`Math.round(-2.5)` is `-2`, and Rust's `(-2.5f64).round()` is `-3`.
`7 ** 30` is rounded past 2^53, and `7u32.pow(30)` wraps exactly. And
JS has `-0`, which no integer is: `-4 % 2` is `-0`.

## Decision

**A method that JS has, and gives the same answer, is JS's; one where JS
differs is a helper with Rust's answer:**

| Rust | JS |
|---|---|
| `x.floor()`, `sqrt()`, `abs()`, `trunc()`, `hypot(y)`, `atan2(x)`, … | `Math.floor(x)`, `Math.sqrt(x)`, … |
| `x.ln()`, `x.is_nan()`, `x.is_finite()` | `Math.log(x)`, `Number.isNaN(x)`, `Number.isFinite(x)` |
| `x.powf(y)` | `x ** y` |
| `x.round()` | `$round(x)`: a half away from zero |
| `x.powi(n)` | `$powi(x, n)`: the multiplications Rust's `__powidf2` does, in its order |
| `n.pow(e)` | `$pow(n, e)`: multiplied with `Math.imul`, then wrapped |
| `n.abs()`, `n.signum()`, `a.abs_diff(b)` | `Math.abs(n) \| 0`, `Math.sign(n)`, `Math.abs(a - b)` |
| `a.checked_add(b)`, `checked_div` | `$checked(a + b, lo, hi)`, `$checkedDiv(a, b, min)`: `undefined` is `None` |
| `a.saturating_sub(b)` of a `u32` | `Math.max(a - b, 0)` |
| `a.wrapping_mul(b)` | `Math.imul(a, b)`, as `*` is |
| `a.rem_euclid(b)`, `a.div_euclid(b)` | `$remEuclid(a, b, min)`, `$divEuclid(a, b, min)`; `%` and `/` for unsigned types |
| `n.leading_zeros()`, `trailing_zeros()`, `count_ones()` | `Math.clz32(n)`, `$trailingZeros(n, 32)`, `$countOnes(n)` |
| `v.binary_search(&x)` | `$binarySearch(v, x)`: Rust's search, step for step |
| `vec![0; n]` | `new Array(n).fill(0)` |
| `vec![vec![0; m]; n]` | `Array.from({ length: n }, () => new Array(m).fill(0))` |

- **Exact results are exact.** Integers are below 2^32, so a sum or
  difference is exact in JS, and a product is exact below 2^53. Past that,
  a product is rounded, but it's far out of any range, so `$checked` still
  knows it's out.
- **An integer is never `-0`.** `%`, `/` and `*` can make it in JS, and
  `1.0 / x as f64` would tell. Helpers add `0`, which makes `-0` `0`.
- **`binary_search` finds the index Rust does** among equal items, since it
  halves the same way. It takes integers, `char`s, `bool`s and strings,
  which `<` orders as `Ord` does.
- **`vec![x; n]` fills with `x` when copies of it can't be told apart**
  (ADR 0052). Otherwise each item is its own, as Rust clones it: made again
  when `x` is built from pure parts, as `vec![0; m]` is, or cloned,
  `Array.from({ length: 3 }, () => ({ ...cell }))`.
- **An operator on the crate's own type calls its impl:** `a + b` is
  `vec2Add_add(a, b)`, `-a` is `vec2Neg_neg(a)`, and `c += b` is
  `vec2AddAssign_add_assign(c, b)`. Like `From`, operators have no
  dictionaries, so `T: Add` in generic code is still an error.
- **A type's own `const`,** `Vec2::ZERO`, is its value where it's used, as
  Rust's is: `{ x: 0, y: 0 }`, and `Vec2::ZERO.y` is `0`. A trait's
  associated constants are still errors.
- **Also here:**
  - a derived `Debug` of a fieldless enum is the value, its variant's name
    (ADR 0013), with no function;
  - `for (j, &v) in row.iter().enumerate()` takes `&v` apart like `v`:
    `for (const [j, v] of row.entries())`;
  - numbers are written as JS's `String(n)` writes them: `0.25`, not `.25`,
    and `2.220446049250313e-16`;
  - `{}` of an integer constant is its text: `" 2147483647 "`.
- **Still errors:** `clamp`, `f64`'s `signum`, `mul_add` and `to_degrees`,
  `overflowing_*`, `binary_search_by`, and 64-bit integers.

## Why

- **It's Rust's answer.** Differential tests check each method on the edge
  cases: `i32::MIN`, `u32::MAX`, halves, `-0.0`, NaN, infinities, `1e21`,
  and `i8`s and `u8`s that wrap.
- **It's the JS a person writes** where JS agrees, and a named helper where
  it doesn't, so the difference is visible.

## Alternatives

- **`Math.round`, `Math.pow` and `/` alone:** shorter, but wrong for
  halves below zero, for powers past 2^53, and wherever a `-0` can show.
- **Operators as methods of the type's object** (`Vec2.add(a, b)`). It would
  read well, but a trait impl's functions are named for the trait and the
  type already (ADR 0052), and `Vec2.add` could collide with an inherent
  `add`.
- **A type's constants in its object** (`Vec2.ZERO`). It reads well, but a
  use that changes its copy would need `{ ...Vec2.ZERO }`, which is the
  value written out anyway.

## Consequences

- JS's `Math.sin`, `Math.exp`, `Math.log` and `**` aren't required to be
  correctly rounded, and neither is the C library Rust calls. They agree on
  the values tested, but may differ in the last bit for others.
