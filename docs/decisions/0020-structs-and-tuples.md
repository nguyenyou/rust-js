# 0020. Structs are objects, tuples are arrays

Status: Accepted. Extends [0013](0013-fieldless-enums.md) to types with fields.

## Context

Structs and tuples need a JS form. ReScript's answer, checked in its compiler
tests (rescript-lang/rescript at `5b00bcf`), is one rule: **use the plainest
JS value that the type checker already makes unambiguous**. Types are erased,
so a value only needs enough shape to tell cases apart at runtime:

| ReScript | JS |
|---|---|
| record `{x: 1, y: 2}` | `{x: 1, y: 2}` |
| tuple `(1, "a")` | `[1, "a"]` |
| variant without payload, `Red` | `"Red"` |
| variant with payload, `Circle(1.)` | `{TAG: "Circle", _0: 1}` |
| `None`, `Some(5)` | `undefined`, `5` |

Rust adds a problem ReScript doesn't have. **Rust structs are values; JS
objects are references.** In ReScript, records are immutable unless a field
is marked `mutable`, and sharing one is ML semantics. In Rust:

```rust
let a = Point { x: 1, y: 2 };   // Point: Copy
let mut b = a;                  // a copy
b.x = 5;                        // `a.x` is still 1
```

The naive JS, `let b = a; b.x = 5;`, changes `a` too.

## Decision

**Representation.**

| Rust | JS |
|---|---|
| `struct Point { x: i32, y: i32 }`, `Point { x, y }` | `{ x, y }`, fields in declaration order |
| `struct Size(u32, u32)`, `(a, b)` | `[a, b]` |
| `struct Marker;` | `undefined`, like `()` |
| `p.x`, `t.0` | `p.x`, `t[0]` |
| `p.x = v`, `p.x += v` | `p.x = v`, `p.x = p.x + v \| 0` |
| `Point { x, ..base }` | `{ x, y: base.y }`: every field written out |

The declarations themselves emit nothing, as for enums (0013).

**Patterns.** A struct or tuple pattern tests each field: `Point { x: 0, .. }`
is `p.x === 0`. An immutable variable bound into a subject that can't
change while it lives doesn't get a JS variable. It just names the place
it matched, as in ReScript: `Point { x, y } => x + y` becomes `p.x + p.y`.
Other variables get their own `const` or `let`.

A subject can't change if it's a `const` holding the scrutinee, or an
immutable variable that is `Copy` or holds nothing changed in place.
Immutable alone isn't enough, because a move can hand the object to a
`mut` variable:

```rust
let Rect { origin: Point { x: before, .. }, .. } = r;
let mut s = r;       // same JS object as `r`
s.origin.x = 0;      // `before` must still be the old value
```

So there `before` is `const before = r.origin.x`, read before the move. `match (a, b)` tests `a` and `b`
directly, without building the array. `let` and parameters take the same
patterns: `fn f((x, y): (i32, i32))` is `function f(param)`, reading
`param[0]`.

**Value semantics.** Fields are changed in place. Rust's copies are made
explicit, but only where they would be visible:

1. **A move never copies.** After `let s = r;` with a non-`Copy` `r`, Rust
   forbids using `r` again, so sharing the object can't be observed.
2. **A read of a `Copy` value copies it** (`{ ...a }`, `[t[0], t[1]]`, with
   nested structs copied in turn) **only if its type contains a type whose
   objects are changed in place somewhere in the crate.** `a.b.c = ..`
   changes the object `a.b`, so it's `a.b`'s type that counts. Every other
   type's objects never change after they're built, so sharing them is
   the same as copying them.
3. **Returning a place doesn't copy it**, because every local dies at
   `return`. (`return (p, p)` still copies: that builds a new tuple from
   two reads.)

Why this is enough: an object can only be changed through a `mut`
variable, and every path by which a value of a changed type reaches one (a
`let`, an argument, a field of a new struct) is a read, which copies it.

**Evaluation order.** Rust evaluates struct fields in the order they're
written; the object lists them in declaration order. If that reorders two
expressions with effects (calls, or divisions that can panic), both go into
`const`s first, named after their fields:

```js
const y = $div(100, a, -2147483648) | 0;   // Point { y: 100 / a, x: 100 % b }
const x = $rem(100, b, -2147483648) | 0;
return { x, y };
```

## Why

- **Readable at both ends.** The JS reads like hand-written JS, and a JS
  caller passes `{ origin: { x: 0, y: 0 }, size: [3, 4] }` with no wrapper
  or constructor to learn.
- **One shape per type.** Declaration order means every `Point` is built
  with the same keys in the same order, so JS engines give them one hidden
  class, and property access stays fast.
- **Writes stay writes.** `p.x += 1` is one field store, not a new object.
  That is also what `&mut` will want: a reference to a struct can be the
  object itself.
- **Copies only where they matter.** A crate that never assigns a field
  never copies anything, and in one that does, only the affected types
  are copied.

## Alternatives

- **Never change objects** (`p = { ...p, x: p.x + 1 }`): trivially safe,
  and ReScript's default. But it allocates on every write, reads worse,
  and leaves `&mut` nothing to point at.
- **Copy on every `Copy` read**, like MIR's `Operand::Copy`: simplest, but
  puts `f({ ...a })` everywhere, for types that are never changed.
- **Classes** (`new Point(1, 2)`): a place for methods later, but a JS
  caller couldn't just write an object literal. Methods can be plain
  functions, as ReScript does.
- **Tuple structs as objects** (`{ _0: a, _1: b }`): arrays are what tuples
  already are, and a tuple struct is a named tuple.
- **Erase single-field structs** (`struct Meters(f64)` as the bare number,
  like ReScript's `@unboxed`): attractive, and possible later. It changes
  which object a field write touches, so it needs rule 2 to look through
  erased types.

## Consequences

- A JS caller that passes an object to a function taking it by value (moved,
  or into a `mut` parameter) may see the function change it. In Rust, the
  caller gave it up.
- Copying is decided per type, for the whole crate: once any `Point` field
  is assigned anywhere, every `Point` read copies, even where the source is
  never used again. Knowing a read is a variable's last use would remove
  most of those copies. We don't do that yet.
- Derived impls (`#[derive(Clone, Copy, PartialEq, ..)]`) are skipped, since
  `Copy` needs one. `==` on structs is `$eq` (ADR 0026), and `.clone()` of
  a type that's never changed in place is the value itself (ADR 0052).
- Not supported yet: unions, `..base` where `base` isn't
  a variable or field, default field values. Enums with fields follow
  ReScript (`{ TAG: "Circle", _0: 1 }`, and `Option` erased): ADRs 0030
  and 0033.
- When references arrive, rule 2 must count `&mut place` as changing
  `place`'s type in place.
- oxc prints an object with more than one field across several lines.
  ReScript's output does the same.
