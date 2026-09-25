# 0013. Fieldless enum variants are strings

Status: Accepted

## Context

An enum whose variants carry no data (a "C-like" enum) needs a JS
representation:

```rust
pub enum Order { Ascending, Descending }
```

## Decision

A fieldless variant is **its name as a JS string**: `Order::Ascending` is
`"Ascending"`. Matching compares with `===`:

```js
if (order === "Ascending") { ... }
```

The enum declaration itself emits nothing.

## Why

- **Readable at both ends.** In a debugger or log you see `"Ascending"`, not
  `0`. A JS caller writes `nth("Ascending", 10)`, which explains itself.
- **Cheap.** JS engines intern short strings, so `===` on them is fast.
- **Sound enough.** Rust's type checker has already guaranteed that only
  valid variants of the right enum reach this code, so two enums sharing a
  variant name can never be confused at runtime.
- ReScript made the same choice for payload-less variants.

## Alternatives

- **Integers (the discriminant)**: compact, and what `as i32` would need,
  but `0`/`1` in JS output tells a reader nothing.
- **Objects** (`{ TAG: "Ascending" }`): the natural shape for variants *with*
  fields, but wasteful for fieldless ones.

## Consequences

- `order as i32` (enum to integer cast) isn't supported yet. It will need the
  variant's discriminant, not its name.
- Enums **with** fields are a separate, future decision. They'll likely be
  tagged objects, as ReScript does (see [0020](0020-structs-and-tuples.md) for
  structs), so a mixed enum could use strings for its fieldless
  variants and objects for the rest.
- `==` on enums goes through the `PartialEq` trait (a method call), which
  isn't supported yet. `match` works.
