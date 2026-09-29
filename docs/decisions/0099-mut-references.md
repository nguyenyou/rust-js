# 0099. A `&mut` held in a variable names its place; one kept elsewhere is a handle

Status: Accepted in part: its first two rules, a `&mut` in a variable and
the index loop. Handles, generic `&mut T` and closures are to come. Extends [0025](0025-vec-loops-refcell-mut.md), [0033](0033-enums-with-fields.md) and [0074](0074-mut-boxes.md).

## Context

A `&mut` to a JS object is the object (ADR 0025), and a `&mut` to anything
else, a number say, is a box `{ value }` when it's a parameter, copied back
after the call (ADR 0074). A `ref mut` binding of a field names the field
(ADR 0033). Any other `&mut` to a value that isn't an object is an error:
one held in a variable, kept in a struct or a `Vec`, or returned.

It's the value rustc's tests stop at most: 70 of them. By the type they
stop at:

| `&mut` to | Tests | Like |
|---|---|---|
| a number, `()`, a `&x`, a `Box` of one, a `String` or an `Option` | 37 | `let y = &mut x; *y = 5;`, `for i in &mut ints { *i += 22 }` |
| a type parameter, or `Self` in a trait | 13 | `fn to_refs<T>(list: &mut List<T>) -> Vec<&mut T>` |
| a `dyn Trait` | 12 | `let w: &mut dyn Write = &mut out;` |
| a closure or a future | 6 | `fn call<F: FnMut()>(f: &mut F) { f() }` |
| a struct or a union of the test's own | 3 | |

In most, the `&mut` never leaves the function that made it: a variable
holds it for a few lines, or a loop gives one for each item. The rest keep
it, in a `Vec` or a struct, or return it.

## Decision

**A `&mut` a variable holds names its place,** as a `ref mut` binding does
(ADR 0033). `*r` is the place, read or written:

```rust
let mut x = 3;
let y = &mut x;
*y = 5;
println!("{}", *y);
```

```js
let x = 3;
x = 5;
console.log(`${x}`);
```

- **It's exact.** While `y` lives, Rust lets nothing else use `x`, so a
  write through `y` is a write to `x` that nothing can tell apart.
- The place is fixed where it's borrowed: an index is evaluated then,
  once, `let r = &mut v[i]` keeping `i`'s value if `i` could change, and a
  place through an object keeps the object, not the variable that held it.
- It holds for a variable bound once, by `let`, to `&mut` of a place or a
  reborrow of one, and used as `*r`, a method's receiver, or passed on, as
  `&mut *r` is. Passed on, it's the place given to a call, boxed as ADR
  0074 boxes one.
- `*r = v` replaces the whole value, an object's too, which ADR 0025
  refused: it's `x = v`.

**`for x in &mut v`, of values that aren't objects, is an index loop,** and
`*x` names `v[i]`:

```js
for (let i = 0; i < ints.length; i++) {
  ints[i] += 22;
}
```

For an array, a `Vec` or a slice, and `iter_mut()` of one. Of objects it
stays `for (const todo of s.todos)` (ADR 0025).

**A `&mut` kept anywhere else is a handle:** in a struct, a `Vec` or an
`Option`, in a variable that's assigned again, chosen by a branch, or
returned. It reads and writes its place:

```js
{ get value() { return x; }, set value(value) { x = value; } }
```

- It has a box's `value` (ADR 0074), so a function taking a `&mut` reads it
  the same way whichever it's given.
- Its place is fixed when it's made, as a variable's is: `&mut list.value`
  keeps the `list` object of that moment, `const o = list;`, so moving
  `list` on doesn't move the handle.
- A call whose result can hold its parameter's borrow, one whose return
  type names that parameter's lifetime, is given a handle, not a box: the
  box is copied back when the call returns, and the borrow outlives it.
- Of an object, a handle's `value` is the object, and changes through it
  are the object's; only a `&mut` to an object that's kept, and replaced
  whole through, needs one.

**A generic `&mut T` is a box or a handle whatever `T` is.** A generic
function is compiled once, and its `T` might be a number, so `*r` is
`r.value` in it. A caller whose `T` is an object gives it a box too, and
takes the value back after. A `&mut T` that generic code returns or keeps,
for a caller whose `T` is an object, is an error for now: the caller's own
`&mut` to one is the object, not a handle.

**A `&mut` to a closure is the closure,** a type parameter bound by `Fn`,
`FnMut` or `FnOnce` too: calling a JS function changes what it captured,
as calling it through the `&mut` does in Rust. `f()`, not `f.value()`.
Assigning a new closure through one is an error.

**Not here:** `&mut dyn Trait`, where a trait's `&mut self` methods on a
number have nowhere to write, raw pointers, and a `&mut` in a `static`.

## Why

- **It's what a person writes.** `x = 5` for `*y = 5`, and an index loop to
  change an array's numbers. A handle, where one's needed, is the object a
  person would make to pass a variable around: React's `useRef` is one.
- **It's exact.** Each form is the place, for as long as Rust lets the
  `&mut` be used, and the borrow checker keeps anything else from using
  the place meanwhile.
- **It's small where it's common.** Most of these tests only hold a `&mut`
  for a few lines, which needs no new shape at all.

## Alternatives

- **A handle for every `&mut`:** one rule, but `y.value = 5` for `*y = 5`,
  where the place itself reads as the Rust does.
- **Boxing every variable a `&mut` is taken to** (`let x = { value: 3 }`):
  ADR 0025's alternative, with every read of `x` a `.value`.
- **A place as a pair,** `[object, "key"]`: small, but `r[0][r[1]] = 5`
  reads worst of all, and a variable isn't a key of anything.
- **A copy of each generic function for each `T`:** `*r` would be the
  place for an object and a box only for a number, but rust-js compiles a
  generic function once, given what depends on the type (ADR 0049).

## Consequences

- **The first two rules are in,** each with its corpus cases, compared
  with native Rust, and its mutations:
  - `mut_ref_local`: a number, a `String`, an `Option` and a field
    through a `&mut` in a variable, one passed on and one reborrowed; an
    index fixed where it's borrowed, `const at = $at(v, i);` and then
    `v[at]`. Reached through a reference that can be assigned again, one
    anywhere but an immutable variable (a `let mut`, a field, an element,
    behind another `&mut`), the place is in the object of that moment,
    kept: `const o = cur;` for `&mut cur[0]`, `const o = h.list;` for
    `&mut h.list[0]`, `const o = refs[0];` for `&mut refs[0][0]`. Rust
    freezes the rest of the path while it's borrowed, but not where a
    reference is.
  - `mut_ref_loop`: `&mut v` and `iter_mut()` of a `Vec`, an array and a
    slice, `continue` and `break`. The loop keeps the collection it starts
    with, `const items = cur;`, if what holds it is assigned again. Of
    `&mut v[a..b]`, it runs from `a` to where `$sliceEnd` says it ends,
    which panics as `$slice` does (`mut_ref_loop_bounds`).
- **A `ref mut` binding of a `let` variable writes it too,** as a `&mut`
  in a variable does, for a value that isn't an object: `if let Some(n) =
  p { *n += 1 }` is `o += 1`. Of an object, its variable may be a `&mut`
  itself, and assigning it wouldn't replace what it points to: that stays
  an error.
- **A trait's `&mut self` method on a number or a `String` is called as a
  function taking one is:** its impl's method takes a box (ADR 0074), so
  `n.bump()`, which resolves to it, boxes `n` and takes it back after
  (`trait_mut_self_value`).
- **A `&mut` to an object in a variable is still the object** (ADR 0025),
  and `*r = v` of one still an error: replacing an object whole through a
  `&mut` comes with handles.
- **Anything else stays an error:** a `&mut` kept in a struct, a `Vec` or
  an `Option`, chosen by a branch, passed to a generic function or
  returned. The diagnostics test checks a struct's, a branch's and a
  generic function's.
- Most of the 37 tests of `&mut` to a value that isn't an object need the
  first two rules only, and those come first; handles, generic `&mut T`
  and closures follow, each with its corpus cases and mutations.
- `&mut dyn Trait` (12 tests) needs its own decision: where a `dyn` of a
  number keeps the number its `&mut self` methods change.
- Each test that gets further may stop at something else, as rustc's tests
  do; the known failures say where.
