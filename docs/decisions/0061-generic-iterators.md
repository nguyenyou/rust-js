# 0061. `impl Iterator` is the type it hides; a generic iterator is any JS iterable

Status: Accepted. Extends [0036](0036-iterators-and-sorting.md) and [0055](0055-iterator.md).

## Context

An iterator has two JS forms:
- a std one over a slice, a `Vec` or a range is an array (ADR 0036);
- one of the crate's own is a JS iterator, made by `$iterator` (ADR 0055).

Neither covered the two ways Rust code hands iterators around:

```rust
fn evens(n: u32) -> impl Iterator<Item = u32> { (0..n).filter(|x| x % 2 == 0) }
fn total<I: Iterator<Item = u32>>(items: I) -> u32 { items.sum() }
```

An `impl Iterator` was an error: an opaque type is none of the types
rust-js knows. A generic `I` could be either form, depending on the caller.

## Decision

- **An `impl Trait` type is the type it hides.** Once type checking is
  done, rustc knows what `evens` returns, a `Filter` of a range, and
  rust-js reveals it wherever it decides by type: an array here, `$range(0,
  n).filter(..)`. An `impl Iterator` hiding one of the crate's own
  iterators is that struct, as it is in Rust.
- **Generic code takes an iterator with `Iterator.from(items)`.** It
  accepts an array and a JS iterator alike, and gives JS's lazy helpers
  (ADR 0055): `Iterator.from(items).drop(1).take(k).toArray()`. A `for`
  over an `I: IntoIterator` is `for (const x of items)`, which also takes
  either.
- **One of the crate's own iterators, given where a generic one goes, is
  made a JS iterator first:** `total($iterator({ n }, countdownIterator_next))`.
  An array needs nothing.

## Why

- **It's the same JS either way.** A caller's array or iterator reaches
  generic code as what it is, with no dictionary: `Iterator.from` does the
  one thing that differs.
- **Revealing the type is exact.** It's the type Rust itself uses when it
  generates code.

## Consequences

- `next()` on a generic iterator is still an error. Generic code uses the
  adapters, `for`, and what consumes one.
- A range handed over as a value (`total(0..3)`) is still an error, as a
  range in a variable is (ADR 0036).
