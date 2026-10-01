# 0123. A slice's pattern tests its length and its items

Status: Accepted. Extends [0023](0023-strings-references-shared-state.md) and [0063](0063-text.md).

## Context

`match xs { [] => .., [x] => .., [first, rest @ ..] => .. }` and `let [a, b,
c] = arr;` were errors: a slice's or an array's pattern was the pattern
rustc's own tests most often stopped at (ADR 0089), and they're how Rust
code takes a slice apart. A slice and an array are JS arrays (ADR 0036).

## Decision

**A slice's pattern is a test of its length, then of each item, where it
is; an array's length is its type's, so it needs no test:**

| Rust | JS |
|---|---|
| `[]`, `[a, b]` of a slice | `xs.length === 0`, `xs.length === 2` |
| `[first, .., last]` | `xs.length >= 2`, then `xs[0]` and `xs[xs.length - 1]` |
| `[1, n, 3, ..]` | `xs.length >= 3 && xs[0] === 1 && xs[2] === 3`, `n` is `xs[1]` |
| `[.., last]` of a `[T; 3]` | `xs[2]` |
| `rest @ ..` | `xs.slice(1, xs.length - 1)` |
| `let [a, b, c] = arr`, `fn f([x, y]: [T; 2])` | `const [a, b, c] = arr`, `function f([x, y])` |

- **An item's pattern is any pattern**, a constant, a range, a variant, a
  tuple, another slice's, tested on `xs[i]`, as a field's is on its place.
- **An item counted from the end is `xs[xs.length - k]`**, not `xs.at(-k)`:
  a `ref mut` binding of it names a place, and `xs[i] = ..` writes one,
  where `xs.at(-1) = ..` isn't JS.
- **What `..` binds is a copy** of the items it stands for, `xs.slice(a,
  b)`, as `&v[a..b]` is (ADR 0063). So a `ref mut` one, or one of a `&mut
  [T]`, which would be written through, is an error, as `&mut v[a..b]` is.
  An item bound by `ref mut`, or through a `&mut`, names its place, and
  writes it: `*head += 10` is `grid[0] = grid[0] + 10 | 0`.
- **An array's first items, each a binding or `_`, are JS's own
  destructuring**: `const [a, b] = xs`, as a tuple's are. One with a rest
  or items from the end is tested and bound as any.
- **A constant array as a pattern**, `ORIGIN`, is its items: rustc gives it
  as their patterns.
- **`Vec::as_slice()` and `as_mut_slice()` are the `Vec`**, the same array,
  so a slice pattern of either is of the `Vec`, and writes reach it.

## Why

- **It's the JS a person writes**: a length test, then indexes, and
  `const [a, b] = xs` where JS has one.
- **It's exact**: a slice's length is its array's, and each item is where
  Rust's is.

## Consequences

- Rust's slice patterns compile, compared with native Rust by the
  `slice_patterns` corpus case.
- A `..` bound by `ref mut` is still an error, until a part of a slice can
  be written through (ADR 0063).
