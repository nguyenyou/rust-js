# 0097. A user `Deref` or `IndexMut` is its method, and an auto trait's impl is nothing

Status: Accepted. Extends [0052](0052-std-trait-impls.md) and [0056](0056-indexing.md).

## Context

208 of rustc's tests stop at "user implementations of this standard or
external trait". Which traits they implement:

| Trait | Tests that implement it | Tests that implement only it |
|---|---|---|
| `Drop` | 128 | 110 |
| `Deref`, `DerefMut` | 21 | 12 |
| `Fn`, `FnMut`, `FnOnce` | 13 | 3 |
| `Send`, `Sync` | 14 | 12 |
| `IndexMut` | 4 | 4 |

`Drop` is a design of its own: when a value is dropped is scope, move and
temporary rules rustc works out as it builds MIR, and rust-js lowers THIR,
which has no drops in it. This ADR takes what needs no new shape.

## Decision

**An auto trait's impl, as `unsafe impl Sync for Counter {}`, is
nothing.** `Send`, `Sync`, `Unpin`, `UnwindSafe` and the others have no
items: an impl says what the type may be used for, and there's nothing to
run. A negative impl, `impl !Send`, is nothing too. It's how a `Cell` goes
in a static (ADR 0096).

**A user `Deref`, `DerefMut` or `IndexMut` is its method, called where
rustc calls it**, as a user `Index` already is (ADR 0056): for `*w`, for a
method or a field reached through `w`, and for `grid[i]` written to. What
it returns is a reference: the value it points to (ADR 0023), or the
object itself for a `&mut` (ADR 0025).

```rust
impl DerefMut for Tracked {
    fn deref_mut(&mut self) -> &mut Point { &mut self.point }
}
t.y = 0;
grid[(1, 0)].x = 7;
```

```js
trackedDerefMut_deref_mut(t).y = 0;
gridIndexMut_usize__usize__index_mut(grid, [1, 0]).x = 7;
```

**A field of what any call's `&mut` points to is written where it is**,
as an element's is (ADR 0056): the right side first, as Rust runs it,
then the call, then the write. So `v.iter_mut().last().unwrap().count = 9`
and `map.get_mut(k).unwrap().n *= 4` work too.

## Why

- **It's the code the user wrote,** called where rustc calls it: the
  number of `deref` calls, and what each does, is Rust's.
- **An auto trait is a promise to the compiler,** which JS has no
  compiler to keep.

## Consequences

- A whole value written through a user's `&mut`, `*w = Point { .. }` or
  `grid[i] = p`, is still an error: the reference is the object, and JS
  can't make another object be it.
- A `Deref` or `IndexMut` to a number, `&mut i32`, is an error, as any
  `&mut` of a number returned is.
- **Of rustc's tests, 14 more pass** (1,425 of 2,691). One more compiled
  and threw: `#[derive(CoercePointee)]` writes a pointer of the crate's
  own that unsizes to a `dyn`, and its derived impls pass unchecked, so
  `ptr as MyPointer<dyn MyTrait>` was left as it was, not a `dyn`'s value
  and impl. Unsizing a type of the crate's own is an error now.
- `Drop` is left for its own ADR, and `Fn*`, `Wake`, `AsyncDrop` and the
  rest are still errors.
