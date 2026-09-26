# 0074. A `&mut` to a string or a number is a box the caller copies back

Status: Accepted. Extends [0025](0025-vec-loops-refcell-mut.md) and [0052](0052-std-trait-impls.md).

## Context

A `&mut` to a JS object is the object (ADR 0025): a change through it is a
change to the one object. But a JS string or number can't be changed in
place, so `&mut` to a `String` or a `u32` was an error. Those are common:
a pretty-printer that writes to `out: &mut String`, a counter passed as
`count: &mut u32`, `settle(&mut stats.last)`.

And cloning a type inside itself, like a JSON value whose objects hold
values, never finished: the clone was written out in place, part by part,
and a `Value` holds `Value`s.

## Decision

**A parameter that's a `&mut` to a value that isn't a JS object (a
string, a number, a `bool`, a fieldless enum or an `Option` of one) is a
box, `{ value }`.** The caller puts the value in it, and copies it back
after the call:

```rust
fn bump(count: &mut u32, by: u32) -> u32 { *count += by; *count }
bump(&mut stats.hits, 4);
```

```js
function bump(count, by) {
  count.value = (count.value + by) >>> 0;
  return count.value;
}
const count = { value: stats.hits };
bump(count, 4);
stats.hits = count.value;
```

- **It's exact.** While the function has the `&mut`, the borrow checker
  lets nothing else read or write the place, so no one can tell the box
  from it. When the call returns, so does the `&mut`, and the value is
  copied back before anything else runs.
- **The box is named as the parameter is**, `count` for `count: &mut u32`,
  so the call reads as what it passes.
- **A box handed on is the box:** `x.pretty(indent + 2, out)` passes `out`.
- The place can be a variable, a field (`&mut stats.hits`), or an item
  (`&mut counts[1]`), written back through `$at` (ADR 0056).
- **Still errors:** a `&mut` to one of these returned (it would outlive the
  copy back), kept in a local or a struct, or given to a trait method.

**A recursive type's clone is a function that calls itself,** for a type
of the crate's own inside itself, through its fields or what a std type
holds:

```js
const cloneValue = (value) =>
  value.TAG === "Obj" ? { ...value, _0: new Map(Array.from(value._0).map(([key, value]) => [key, cloneValue(value)])) } : value;
```

### Also here

- `v.get(i)` of a slice or a `Vec` is `v[i]`, which is `undefined` past the
  end, as `None`.

## Why

- **It's Rust's answer.** The example's pretty-printer, its counter, a
  field, an item and an `Option` written through `&mut` all match native
  Rust, and so does its recursive value's clone and `==`.
- **It's what a person writes** to pass something JS can't change in
  place: an object holding it.

## Alternatives

- **Returning the new value** (`p = pretty(v, 0, p)`): plain JS, but a
  function can take several, and return a value of its own too.
- **Every string and number a box:** uniform, but every read would be
  `.value`, for a case that only calls with `&mut` need.
