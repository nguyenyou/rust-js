# 0129. A range is a value: `{ start, end }`, iterated as its items

Status: Accepted. Extends [0036](0036-iterators-and-sorting.md) and [0128](0128-iterator-sources.md).

## Context

A range was an iterator only where it was written: `for i in 0..n` and
`(0..n).map(f)`. Kept in a variable, passed, or held in a field, `0..n`
was already `{ start: 0, end: n }`, as any struct is, but nothing could
use it: iterating it, `r.contains(&x)`, `r.len()`, `{:?}`, `==` and
`clone()` were errors, and so was `1..=n` as a value. `(1..).take(3)`
compiled and threw, its object taken for items, and `('a'..'e')` was no
items at all.

## Decision

**A range is the object of its bounds, and wherever an iterator is taken
from it, its items:**

| Rust | JS |
|---|---|
| `let r = 2..7;`, `1..=6`, `4..`, `..=4` | `{ start: 2, end: 7 }`, `{ start: 1, end: 6 }`, `{ start: 4 }`, `{ end: 4 }` |
| `r.collect()`, `r.map(f)` | `$range(r.start, r.end)`, `.map(f)` |
| `(1..=n).filter(p)` of a value | `$range(r.start, r.end + 1).filter(p)` |
| `(1..).take(3)` | `$rangeFrom(1).take(3)`, a JS iterator |
| `for i in r` | `for (let i = r.start; i < r.end; i++)` |
| `for n in 1..` | `for (let n = 1; true; n++)` |
| `('a'..='c').collect()` | `$charRange("a", "c", true)` |
| `r.contains(&x)` | `r.start <= x && x < r.end` |
| `r.len()`, `r.is_empty()` | `Math.max(0, r.end - r.start)`, `!(r.start < r.end)` |
| `r.start()`, `r.into_inner()` of `a..=b` | `r.start`, `[r.start, r.end]` |
| `&v[r]`, `&v[..=1]`, `v.drain(r)` | `$slice(v, r.start, r.end)`, `$slice(v, 0, 2)` |
| `{:?}` | `` `${r.start}..${r.end}` `` |
| `r == s`, `r.clone()` | `r.start === s.start && r.end === s.end`, `{ start: r.start, end: r.end }` |
| `r.next()`, `r.next_back()` of a `Range` | `$rangeNext(r)`, `$rangeNextBack(r)`, which move `r.start` or `r.end` |

- **The bounds are read once**: a range read more than once is a `const`
  first, unless it reads the same each time; a `{ start, end }` written in
  place gives its bounds themselves.
- **A `for`'s end is read each time round only if nothing can change it**,
  as with any `for`: else it's a `const` first.
- **A range is changed in place** by assigning to a bound, as any
  struct's field (ADR 0020), and by `next()` and `next_back()`, which take
  `&mut` of it: a range of a type something changes is copied by
  `clone()`, and only then. A `for`'s own stepping isn't a change.
- **A range given where a generic `I: Iterator` goes is its items**; where
  any other `T` goes, it's the range.
- **`char`s step past the surrogates**, as Rust's do: `'\u{d7ff}'` is
  followed by `'\u{e000}'`.
- **`a..` counts on past its type's end**, both iterated and stepped by
  `next()`.
- **Still errors:** `next()` of a `RangeInclusive` kept as a value, which
  Rust marks as having reached its end with a field rust-js's object has no
  place for; iterating through a `&mut` to a range, which steps it; and a
  range of anything but numbers or `char`s as an iterator.

## Why

- **It's the JS a person writes**: an object of two numbers, `start <= x
  && x < end`, and a `for` from `r.start` to `r.end`.
- **It's exact**: the items, the tests, the order the bounds are read in,
  and what `next()` changes are Rust's.

## Consequences

- Ranges as values compile, compared with native Rust by the
  `range_values` corpus case. The literal ranges' JS is unchanged.
- `('a'..'e')` and `('a'..='c')` written in place were no items; they're
  their `char`s now.
- `size_hint()`, `by_ref()` and `RangeInclusive`'s `next()` are still errors.
