# 0036. An iterator is a JS array; `Ordering` is -1, 0 or 1

Status: Accepted. Extends [0025](0025-vec-loops-refcell-mut.md), where a
`Vec` is a JS array and `for` loops over one.

## Context

Rust code works on collections through iterator chains, such as
`v.iter().filter(..).map(..).collect()`, and sorts with `sort`, `sort_by` and
`sort_by_key`. rust-js had only `for` loops and a few `Vec` methods. The
playground's file trees are sorted lists, built and walked with these.

A Rust iterator is lazy: nothing happens until something consumes it, one
element at a time, through the whole chain. JS arrays have the same
vocabulary (`map`, `filter`, `some`, `every`, `find`, `reduce`), but each step
runs over the whole array at once. ReScript's `Array` module wraps JS's array
methods, and Scala.js's collections are Scala's own.

## Decision

**An iterator is a JS array, and its adapters are the array's methods:**

| Rust | JS |
|---|---|
| `v.iter()`, `v.into_iter()`, `.copied()`, `.cloned()` | `v` |
| `(a..b)` as an iterator | `$range(a, b)` |
| `s.chars()` | `Array.from(s)` (by code point) |
| `.map(f)`, `.filter(p)`, `.find(p)`, `.for_each(f)` | the same methods |
| `.any(p)`, `.all(p)` | `.some(p)`, `.every(p)` |
| `.enumerate()` | `.map((x, i) => [i, x])` |
| `.rev()`, `.skip(n)`, `.take(n)` | `.toReversed()`, `.slice(n)`, `.slice(0, n)` |
| `.fold(init, f)`, `.sum()` | `.reduce(f, init)`, `.reduce((a, b) => a + b, 0)`, wrapped (ADR 0011) |
| `.count()`, `.last()` | `.length`, `.at(-1)` |
| `.position(p)`, `.max()`, `.min()` | `$position`, `$max`, `$min`: options (ADR 0030) |
| `.collect::<Vec<_>>()`, `.collect::<String>()` | the array, `.join("")` |

**Sorting sorts in place, with a comparator**, and JS's sort is stable, as
Rust's `sort` is:

| Rust | JS |
|---|---|
| `v.sort()` on numbers | `v.sort((a, b) => a - b)`, since JS's own `sort()` compares as strings |
| `v.sort()` on strings or `bool`s | `v.sort()` |
| `v.sort_by(f)` | `v.sort(f)` |
| `v.sort_by_key(k)` | `v.sort((a, b) => $cmp(key(a), key(b)))` |
| `v.reverse()`, `v.to_vec()` | `v.reverse()`, `v.slice()` |

**`Ordering` is -1, 0 or 1**, its discriminant (`#[repr(i8)]`), not a string
as other fieldless enums are (ADR 0013). That's what a JS comparator returns,
so `sort_by(|a, b| b.cmp(a))` is `sort((a, b) => $cmp(b, a))`. `a.cmp(&b)` of
numbers, strings, `char`s and `bool`s is `$cmp(a, b)`. `then(o)` and
`then_with(f)` are `||`, since `Equal` is 0, the one that's falsy, and
`reverse()` is `-o`. `a.max(b)` and `a.min(b)` of numbers are `Math.max` and
`Math.min`.

Two things came with it:
- **An operator on references to numbers** (`x % 10` with `x: &i32`, which
  rustc writes as a call of `Rem::rem`) is the operator.
- **A closure's `|&x|` or `|&&x|` parameter is just `x`**, and a closure's own
  names are free again after it, so sibling closures can each have an `x`.

## Why

- **The JS reads as JS programmers write it**: `v.filter((x) => x % 2 === 0)`.
- **A chain that `collect`s, `count`s or `sum`s**, which is what programs do
  with iterators, gives the same answer either way.
- **Sorting numbers right** is where JS surprises people: `[10, 9].sort()` is
  `[10, 9]`. The comparator is always there for numbers.

## Alternatives

- **JS iterators** (`Iterator.prototype.map` and friends): lazy, like Rust's,
  but newer than the array methods, and a chain would end in `.toArray()`.
- **Generators and a helper per adapter**: faithful, but every chain would be
  helper calls.
- **`Ordering` as `"Less"`, `"Equal"`, `"Greater"`**, like other enums: then
  every comparator would need a helper to turn the name into a number.

## Consequences

- **Every step runs over the whole array before the next.** A chain whose
  closures have effects runs them in a different order than Rust would: all
  of `map`'s calls, then all of `filter`'s. So does a chain that `find`s:
  Rust stops at the first match, and JS has run earlier steps on every
  element. Infinite iterators (`0..`) don't work.
- `$cmp` on strings compares UTF-16 units, as `<` does (ADR 0034).
- Not yet: `filter_map`, `flat_map`, `zip`, `chain`, `peekable`, `next()` on a
  held iterator, ranges in variables, sorting tuples or structs by `Ord`, and
  `binary_search`.
