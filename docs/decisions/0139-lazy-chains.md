# 0139. A chain whose stages do what can be seen runs in Rust's order

Status: Accepted. Amends [0036](0036-iterators-and-sorting.md); extends [0055](0055-iterator.md) and [0128](0128-iterator-sources.md).

## Context

An iterator chain is a JS array, and its adapters the array's methods (ADR
0036). Rust's chain is lazy: each item goes through every stage before the
next item starts. An array runs every item through one stage before the
next stage starts. The two agree on what they give, and on what they do
when only one stage does anything that can be seen. They disagree when
two do:

```rust
v.iter().map(|&x| noisy("map", x)).filter(|&x| noisy("filter", x) > 2)
```

```
Rust:  map 1, filter 1, map 2, filter 2, ..
JS:    map 1, map 2, .., filter 1, filter 2, ..
```

They also disagree when a stage that does something is followed by one
that stops early, such as `take(2)`, `find`, `any` or `position`: Rust
stops calling the stage, and the array has already called it for every
item. And a `for` loop's body runs between items, so the body and the
stage take turns. No corpus case or rustc test had found it.

## Decision

**A chain is lazy, a JS iterator, from its first stage that does what can
be seen, when something after that stage can tell:**

```js
const kept = v
  .values()
  .map((x) => noisy("map", Math.imul(x, 2)))
  .filter((x) => noisy("filter", x) > 2)
  .toArray();
```

- **What does what can be seen** is a closure or function given to a stage
  whose call might change anything, or panic. A closure is pure when its
  body is made only of values, places, operators on numbers, `bool`s,
  `char`s and strings (dividing an integer only by a literal other than
  zero), comparisons of those, an `if` and `let`s of such, constructors, and
  a few of std's own methods that run no code of the crate's: `len()`,
  `is_empty()`, `is_some()`, `is_none()`, `is_ok()` and `is_err()`, and
  `clone()`, `to_owned()`, `to_string()` and `as_str()` of numbers and
  strings. This is the test the destructors' analysis makes of what can't
  leave early (ADR 0098), now of any body. A constructor, `map(Noisy)`, is
  pure; any other function, or a type parameter's, isn't.
- **What can tell** is a later stage that does what can be seen too, one
  that stops early, or, as `rev` does, runs its iterator from the other
  end (`take`, `take_while`, `zip`, `find`, `any`, `all`, `position`,
  `find_map` and `nth`), or a `for` loop's body.
- **What ends the chain decides.** The consumer, or the loop, is lowered
  before its stages, and marks the ones that must be lazy, from the first
  stage that does what can be seen. That stage's receiver starts the JS
  iterator, `v.values()`. A chain whose value goes anywhere else, an
  argument or a return value, is as it was.
- **A chain kept in a variable** is lazy when a stage does what can be
  seen and every use of the variable iterates it, a loop over it or a
  stage or a consumer of it, since what ends it can't be told where it's
  made: those uses take a JS iterator as it is. One that's returned,
  passed on, or stepped with `next()` is as it was.
- **`chain` and `zip` do what can be seen if their other side does**, and
  when they're lazy, so is that side, from its own first such stage. A
  `zip` whose other side does always is: it takes from that side only
  when this one gives an item, and stops at the shorter. Their helpers
  take either side as it is, an array or a JS iterator.
- **A JS iterator's helpers are Rust's order**: `map`, `filter`, `take`,
  `drop`, `flatMap`, `find`, `some`, `every`, `reduce` and `forEach` take
  each item as far as it goes before the next. `find_map` given a closure
  that does what can be seen is a lazy `map` and `find`. `position` walks
  the iterator, and stops at the first match.
- **`rev` after a stage that does what can be seen** is an error: Rust
  calls that stage from the end, which a JS iterator can't do.

## Why

- **It's exact**: the same calls, in the same order, as many times as
  Rust's, compared with native Rust by the `lazy_chains` corpus case.
- **It's the JS a person writes**: a chain that only computes stays an
  array's `map` and `filter`, as before, and one that does something reads
  as a modern JS iterator chain. No other example's JS changed.

## Alternatives

- **Every chain a JS iterator**: one rule, but `.values()` and `.toArray()`
  around every `map`, where nothing could tell.
- **A chain lazy as soon as a stage does something**: decided by each
  stage alone, from its type, but most chains have one such stage and
  nothing that can tell, and paid the same.

## Consequences

- Chains whose closures print, change state or may panic, and loops over
  them, run in Rust's order.
- A chain kept in a variable that's returned or passed on, and stepped
  with `next()`, is still an array. (Amended: a chain kept in a variable,
  and the other side of a `zip` or a `chain`, were arrays.)
