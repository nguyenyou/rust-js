# 0033. Enums with fields are ReScript's tagged objects

Status: Accepted. Extends [0013](0013-fieldless-enums.md): variants without
fields are still their names.

## Context

Rust's enums carry data: `Shape::Circle(r)`, `Result<T, E>`, a tree's
`Leaf` or `Node`. rust-js only had fieldless enums, as strings (ADR 0013), and
the playground's port is waiting on them: its file tree is folders or files,
and a trapped compile is a `Result`. How the others do it (checked in local
clones):

- **ReScript**: a constructor without arguments is its name, `"NoParam"`. One
  with arguments is an object tagged with its name, `{ TAG: "Num", _0: 1 }`,
  or with named fields for an inline record. A `match` tests `x.TAG`. Its
  `result` is `{ TAG: "Ok", _0: v }`. Where the payload can't be confused
  with another case, `@unboxed` drops the object.
- **Scala.js**: a Scala enum case with fields is a class instance, tested with
  `instanceof`.

## Decision

**ReScript's shapes:**

| Rust | JS |
|---|---|
| `Shape::Empty` | `"Empty"`, as before |
| `Shape::Circle(r)` | `{ TAG: "Circle", _0: r }` |
| `Shape::Rect { w, h }` | `{ TAG: "Rect", w, h }` |
| `Ok(v)`, `Err(e)` | `{ TAG: "Ok", _0: v }`, `{ TAG: "Err", _0: e }` |
| `match s { Shape::Empty => .., Shape::Circle(r) => .. }` | `if (s === "Empty") .. else if (s.TAG === "Circle") { .. s._0 .. }` |
| `a == b`, with a derived `PartialEq` | `$eq(a, b)` |

- **The test is the name for a variant without fields**, and its `TAG` for
  one with fields, then the fields' own patterns, on the fields in place:
  `s.TAG === "Rect" && s.w === 0`. An enum with only one variant needs no test.
- **Bindings name the fields where they are**, as struct patterns do (ADR
  0020): `Shape::Circle(r) => 3 * r * r` is `3 * s._0 * s._0`.
- **Matching through a reference works**, and so do `ref` bindings: a
  reference is the value (ADR 0023), and rustc keeps it from changing while
  it's borrowed. `match t { Tree::Leaf(n) => *n, .. }` with `t: &Tree` is
  `t._0`. A `ref mut` binding works where `&mut` does, to an object (ADR 0025).
- **Recursive enums** (`Node(Box<Tree>, Box<Tree>)`) are nested objects:
  `Box` is the value itself.
- **`Option` stays special** (ADR 0030): it's the value or `undefined`, with
  no tag.
- Constants of these enums are written in the same shapes (ADR 0031).

## Why

- **The JS reads like the Rust**, with the variant's name there to see.
  `s.TAG === "Circle"` says what it tests.
- **ReScript users know it**, and it's what their code expects: a `result`
  from rust-js is a ReScript `result`.
- **Variants without fields stay strings**, so ADR 0013's JS, and anything
  written against it, is unchanged.
- **`TAG` can't clash with a field**: Rust field names are snake_case, and
  `_0` can't be a named field.

## Alternatives

- **An object for every variant, `{ TAG: "Empty" }`**: one test for every
  case, but an allocation for each value that has no data, and ADR 0013's
  strings would change.
- **Arrays, `["Circle", r]`**, as tuples are (ADR 0020): shorter, but
  `s[1]` says less than `s._0` or `s.w`.
- **Classes and `instanceof`**, as Scala.js has: a class per variant, and
  `new` for each value, where a plain object does.
- **Unboxed variants** (ReScript's `@unboxed`) for single-field enums: an
  optimization for later, once a program needs it.

## Consequences

- `{:?}` of an enum prints its JS shape: a variant without fields is a string
  at run time, which `$debug` can't tell from a Rust string. Getting it right
  needs the type at the `{:?}`.
- A `matches!` (a `match` of `pat => true, _ => false`) is its test alone:
  `s.TAG === "Circle"`.
- `?` and `Result`'s methods came with ADR 0035. Casts of a fieldless enum
  are its discriminant (ADR 0013).
- **`&mut` to an enum with fields is the variant's object,** as a `&mut` to
  a struct is (ADR 0025), and `Option` excepted, which is its value itself.
  - A field bound by `ref mut`, or through a `&mut` subject, names its
    place, so `*r *= 2.0` is `f.r *= 2`.
  - A fieldless variant is a string, so it can't be changed through the
    `&mut`; replacing the whole value (`*f = Figure::Dot`) is still an error.
  - An enum that's taken `&mut`, or matched with a `ref mut` binding,
    changes in place, so copying and cloning one copies its variants'
    objects (ADR 0020, 0052).
- An arm that does nothing, before others, is the negated test:
  `if (f !== "Dot") { .. }`, not an empty `if` with an `else`.
