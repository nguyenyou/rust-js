# 0140. An iterator trait object is a JS iterator

Status: Accepted. Extends [0055](0055-iterator.md), [0061](0061-generic-iterators.md), [0071](0071-stepping-iterators.md) and [0139](0139-lazy-chains.md).

## Context

A function that returns one of several iterators, or takes any iterator
without being generic, uses a trait object:

```rust
fn numbers(desc: bool) -> Box<dyn Iterator<Item = u32>> {
    if desc { Box::new(Countdown(3)) } else { Box::new(vec![1, 2, 3].into_iter()) }
}
fn first_two(it: &mut dyn Iterator<Item = u32>) -> (Option<u32>, Option<u32>) {
    (it.next(), it.next())
}
```

A `dyn` of the crate's own trait is a value and its dictionary (ADR 0049).
An iterator can be in JS what it is in Rust, though: something with a
`next`, whatever made it. Its items can come from an array, a lazy chain,
a `$iter` stepped through (ADR 0071) or one of the crate's own iterators
(ADR 0055), so they were an error.

## Decision

**A `dyn Iterator`, boxed or lent, is a JS iterator with JS's iterator
helpers**, made where the value becomes one:

| Given as a `dyn Iterator` | JS |
|---|---|
| A lazy chain (ADR 0139), another `dyn Iterator` | itself |
| One of the crate's own | `$iterator(value, countdownIterator_next)`, as generic code is given it (ADR 0061) |
| A generic one, an array or an iterator | `Iterator.from(items)` (ADR 0061) |
| Anything else: an array, a stepped local's `$iter` | `Iterator.from(value)` |

- **What's done with one is what's done with a lazy iterator:** `next()`
  is `$next(it)`, wherever it's kept, a parameter included; a `for` is
  `for (const n of it)`; adapters and consumers are its helpers,
  `it.map(f).toArray()`.
- **Its chain runs item by item,** as what it's given to takes them: a
  chain boxed as one is lazy from its first stage that does what can be
  seen, as a chain stepped through is (ADR 0139).
- **A local lent as a `&mut dyn Iterator` is stepped through,** by
  whatever it's lent to, so it's a `$iter` (ADR 0071), and `Iterator.from`
  of it shares its place: `first_two(&mut it)` then `total(&mut it)` sums
  what's left.
- **A `&mut dyn Iterator` is the iterator,** as a `&mut` to a JS object
  is the object.
- **A `dyn DoubleEndedIterator` is an error:** a JS iterator only steps
  from the front, so `next_back()` has nothing to call.

## Why

- **It's the JS a person writes:** `function first_two(it) { return
  [$next(it), $next(it)]; }`, with no dictionary to carry.
- **JS's iterators are already dynamic.** Each kind of Rust iterator
  already becomes one where it needs to, for generic code and lazy chains;
  this makes that happen where a value becomes a `dyn`, too.

## Alternatives

- **A `{ value, impl }` pair, as for the crate's traits.** Every call
  would go through `impl.next`, and every adapter would have to be lowered
  by hand, where JS's iterator helpers already are.
- **An array or a JS iterator, as a generic iterator is (ADR 0061).**
  `next()` and a lent local's place need an iterator: an array doesn't
  know where it is.

## Consequences

- `Iterator.from` of an array is one more call than the array itself; a
  `dyn Iterator` is one only where the program asks for one.
- Of rustc's tests, 5 more pass, those that box an iterator, lend one or
  return one from a closure.
