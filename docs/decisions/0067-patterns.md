# 0067. Range patterns, `@`, `let ... else`, and a `&mut` into a map

Status: Accepted. Extends [0033](0033-enums-with-fields.md), [0059](0059-hashmap.md) and [0062](0062-combinators.md).

## Context

A program that applies events to a store hit, in turn:

- `match n { i32::MIN..=-1 => .., 1..=9 => .. }`: range patterns were errors;
- `x @ 1..=9 if ..`: a binding with a pattern after it;
- `let Some(have) = m.get_mut(item) else { return Err(..) };`;
- `*have -= qty`, through that `&mut` to a number in a map;
- `(revenue < min).then(|| ..)`;
- `{}` of `counter.borrow()`, a `Ref<i32>`.

## Decision

| Rust | JS |
|---|---|
| `1..=9 =>`, `0..LIMIT =>` | `n >= 1 && n <= 9`, `n < 5` |
| `x @ 1..10 =>` | `x >= 1 && x < 10`, and `x` is the value matched |
| `let Some(x) = e else { return .. };` | the test, `if (x == null) { return .. }`, then the bindings |
| `b.then(\|\| x)`, `b.then_some(x)` | `b ? x : undefined` |
| `if let Some(n) = m.get_mut(&k) { *n *= 2 }` | `let n = m.get(k); if (n != null) { n = ..; m.set(k, n); }` |

- **A range compares with `<`:** numbers by value, `char`s by their UTF-16
  units (ADR 0034). A bound at the type's own end always holds, so it
  isn't tested: `i32::MIN..=-1` is `n <= -1`.
- **A `&mut` to a map's value that's a primitive is a copy of it,** and
  each write through it puts the copy back in the map. That's exact: while
  the `&mut` lives, the borrow checker lets nothing else touch that entry.
  A value that's an object is already shared, as ADR 0059 has it.
- **`then_some(x)` works out `x` either way,** as Rust does, in a `const`
  first if it has effects. `then` to a type whose `Some` would look like
  `None` is an error, as `map` to one is.
- **Also here:**
  - `{}` and `{:?}` of a `Ref` or `RefMut` show what they hold, as a `Box`
    does;
  - `*m.entry(k).or_insert(0) += n` with a `&u32` `n` is the count idiom,
    as with a `u32`;
  - `let f = { let c = ..; move |n| .. };` puts the block's statements
    first, then `const f = ..`, since every local has a JS name of its own.
- **Still errors:** `Rc::strong_count` (an `Rc` is the value itself, with no
  count), a `&mut` to a map's value passed on or kept past the pattern
  it's bound by, and `match` on `get_mut`.

## Why

- **It's Rust's answer.** The example's report, with every event, rule,
  range and tally, matches native Rust's.
- **It's what a person writes:** a range is two comparisons, and `let ...
  else` is an early return.

## Alternatives

- **A `&mut` to a map's value as an object, `{ get, set }`.** It would work
  wherever the reference goes, but every read would be a call, for a case
  where the reference rarely leaves the block.
