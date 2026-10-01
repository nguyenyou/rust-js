# 0128. std's iterator sources: `once` is an array, and `repeat` a JS iterator

Status: Accepted. Extends [0036](0036-iterators-and-sorting.md) and [0055](0055-iterator.md).

## Context

std makes iterators out of nothing: `iter::once(x)`, `iter::empty()`,
`iter::repeat(x)`, `iter::repeat_with(f)`, `iter::successors(first, f)` and
`iter::from_fn(f)`, and an `Option`'s `iter()` and `into_iter()`. All were
errors, and so was `unzip()`.

`once` and `empty` have all their items already, as a slice's iterator does
(ADR 0036). `repeat`, `repeat_with`, `successors` and `from_fn` may never
end, as one of the crate's own may not (ADR 0055): as an array, `repeat(0)`
would never be made.

## Decision

| Rust | JS |
|---|---|
| `iter::once(x)` | `[x]` |
| `iter::empty()` | `[]` |
| `opt.iter()`, `opt.into_iter()`, `for x in &opt` | `opt == null ? [] : [opt]` |
| `iter::repeat(x)` | `$repeating(x)`, a JS generator, or `$repeating(x, clone)` |
| `iter::repeat_with(f)` | `$repeatingWith(f)` |
| `iter::successors(Some(1), f)` | `$successors(1, f)` |
| `iter::from_fn(f)` | `$fromFn(f)` |
| `xs.into_iter().chain(repeat(0))` | `$lazyChain(xs, $repeating(0))` |
| `s.chars().zip(repeat(7))` | `$lazyZip(Array.from(s), $repeating(7))` |
| `successors(..).take_while(p)` | `$lazyTakeWhile($successors(..), p)` |
| `pairs.into_iter().unzip()` | `$unzip(pairs)` |

- **The endless four are JS iterators**, and the adapters on them are the
  JS iterator helpers, which take only as far as they're used (ADR 0055).
- **`repeat` clones its value for each item**, as Rust's does, when the
  value is one a clone copies (ADR 0052): `repeat(vec![0])` gives a new
  array each time.
- **`successors` finds the next item before it gives this one**, as
  Rust's does, so its closure runs as often as Rust's.
- **`from_fn` calls `f` for each `next()`**, after a `None` too, as Rust's
  does, so it isn't a generator, which would end at the first `None`.
- **Closures giving a generic `T`'s `Option`** box a `Some` that looks like
  `None` (ADR 0051): `successors` and `from_fn` unbox it.
- **`chain` and `zip` are lazy when either side is**: an array's `concat`
  would add a JS iterator as one item, and `$zip` wants lengths. What's
  chained or zipped on is a JS iterator when it's one of the crate's own,
  which it wasn't: `xs.iter().chain(counter)` was its struct, not its items.
- **`take_while` and `skip_while` of a JS iterator** are generators, no
  longer errors.
- **A JS iterator in a local that `next()` steps through** is itself, not a
  `$iter` of it (ADR 0071), which wanted an array: `let mut r = repeat(4);
  r.next()` was `None`.
- **`unzip` into two `Vec`s** is two arrays; into anything else it's an
  error.
- **A `&mut Option`'s items** are places, and iterating over one is still
  an error.

## Why

- **It's the JS a person writes**: `[x]` for one item, and a generator for
  an endless iterator.
- **It's exact**: the generators take as many items, and call their
  closures as often and in the same order, as Rust's.

## Consequences

- The sources, and `chain`, `zip`, `take_while` and `skip_while` on JS
  iterators, compile, compared with native Rust by the `iterator_sources`
  corpus case.
- `once_with`, `repeat_n` and `cycle` are still errors.
