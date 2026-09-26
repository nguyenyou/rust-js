# 0057. `PartialOrd` and `Ord`: an `Ordering`, and the parts in turn

Status: Accepted. `Reverse` compares the other way round, and another crate's struct is an error rather than field by field ([0068](0068-queues.md)). Extends [0036](0036-iterators-and-sorting.md) and [0053](0053-partial-eq.md).

## Context

An `Ordering` is -1, 0 or 1 (ADR 0036), and JS's `<` compares numbers the
way Rust does. Nothing else could be compared:

- `<` on strings, `bool`s or structs;
- `cmp` of a struct;
- sorting a `Vec` of them;
- a hand-written `impl Ord`, or a generic `T: Ord`.

`sort_by_key` compared keys with `$cmp`, which is JS's `<`, so for a
tuple key it silently didn't sort.

## Decision

**`a.cmp(&b)` is an `Ordering`, and `a.partial_cmp(&b)` is one too, or
`undefined` for `None`** (`Some(x)` is `x`, ADR 0030).

| The type | `cmp` |
|---|---|
| numbers, strings, `char`s, `bool`s, `Ordering` | `$cmp(a, b)` |
| `f64`, `partial_cmp` | `$partialCmp(a, b)`: `undefined` if either is `NaN` |
| a hand-written impl | `wordOrd_cmp(a, b)` |
| a generic `T` | `TOrd.cmp(a, b)`, or `TPartialOrd.partial_cmp(a, b)` |
| derived, a struct or tuple | `$cmp(a.major, b.major) \|\| $cmp(a.minor, b.minor)` |
| derived, a fieldless enum | `$cmpIn(["Low", "Mid", "High"], a, b)`, by declaration order |
| `Option` | `None` first, then the values |
| `Vec`, slices, arrays | `$cmpItems(a, b, cmp)`: item by item, then shorter first |

- **Derived: the parts in turn, joined with `||`.** `Equal` is 0, the one
  falsy `Ordering`, so `||` gives the first part that isn't equal.
- **Where a part can be unordered** (an `f64`, or anything that isn't
  `Ord`), `undefined` is falsy too, so the parts go through `$thenCmp`
  instead, which stops at it.
- **`a < b` is `cmp < 0`,** and the same for `<=`, `>` and `>=`. Strings
  and `bool`s use JS's own `<`. For an unordered pair, `undefined < 0` is
  false, as every comparison is in Rust.
- **`a.max(b)` is `cmp > 0 ? a : b`, and `a.min(b)` is `cmp > 0 ? b : a`.**
  On a tie, Rust's `max` returns `b` and its `min` returns `a`.
- **Sorting:**
  - `v.sort()` is `v.sort(cmp)`, which is stable, as Rust's is;
  - `sort_by_key` compares the keys with their `cmp`;
  - an iterator's `max` and `min` are `$maxBy(items, cmp)` and
    `$minBy(items, cmp)`: the last of the greatest and the first of the
    least, as in Rust.
- **Dictionaries, as ADR 0049 has them:** `T: Ord` gets `{ cmp }` and
  `T: PartialOrd` gets `{ partial_cmp }`. A derived impl's dictionary is
  its comparison, or `$cmp` itself.
  - **Only an `Ord` or `PartialOrd` bound:** `a == b` is `cmp === 0`
    rather than a third dictionary.
  - **A generic `max()` boxes its `Some`** if it would look like `None`
    (ADR 0051).
- **Numbers stay as they were:** JS's `<`, `Math.max`, `$cmp`.
- **Still errors:** comparing enums with fields, and `clamp`.

## Why

- **The plain case stays short.** A derived `Ord` is the comparison a
  person would write, `||` included, with no helper call.
- **`NaN` behaves.** An `f64` field makes the struct's `<` false both ways,
  as Rust's does, instead of quietly treating `NaN` as equal.
- **One function, `cmp_value`, serves operators, `max`, sorting and
  dictionaries alike,** so they can't disagree.

## Alternatives

- **A named function per derived impl** (`semverOrd_cmp`). Each comparison
  would be a call rather than an expression. It would read better where a
  struct with many fields is compared often, and it could come later.
- **Always `$thenCmp`.** It's correct everywhere, but it evaluates every
  part when `||` stops at the first difference, and it reads worse.
