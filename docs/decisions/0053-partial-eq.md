# 0053. `==`: JS's `===` or `$eq`, until a hand-written `eq` is in it

Status: Accepted. Extends [0052](0052-std-trait-impls.md).

## Context

`==` already compiled in two ways:
- **JS primitives use `===`:** strings, numbers, `bool`s and fieldless enums.
- **Everything with a derived `PartialEq` uses `$eq`:** structs, tuples,
  `Vec`s and enums with fields. `$eq` is a runtime helper that compares
  field by field and element by element.

What didn't compile:
- **A hand-written `impl PartialEq`.** It was rejected, like every std trait
  ADR 0052 didn't cover.
- **`==` in generic code.** A `T` could be anything, so `a == b` had no JS
  meaning.

A hand-written `eq` usually means "equal enough": versions with the same
major number, or records with the same id. Anywhere it appears, it must be
the one that decides. That includes inside a derived `PartialEq`, which
`$eq` would otherwise compare field by field.

## Decision

**`a == b` stays `===` or `$eq(a, b)` unless a custom `eq` is somewhere
inside the type.** A custom `eq` is a hand-written one, or a `T`'s, which
might be hand-written.

| The type | `a == b` |
|---|---|
| A JS primitive | `a === b` |
| An `Option` of one | `a == b`, so `null` counts as `None` |
| Compared against a constant, like `Change::Nothing` | `a === "Nothing"` |
| Derived, with no custom `eq` inside | `$eq(a, b)` |
| A hand-written impl | `versionPartialEq_eq(a, b)` |
| A generic `T` | `TPartialEq.eq(a, b)` |
| Derived, with a custom `eq` inside | the parts compared one by one |

- **Parts compared one by one:**
  - a struct is `versionPartialEq_eq(a.version, b.version) && $eq(a.notes, b.notes)`;
  - a `Vec` is `a.length === b.length && a.every((x, i) => ..)`;
  - an enum tests the variants with custom parts, and `$eq` does the rest:
    `a.TAG === "Bump" ? b.TAG === "Bump" && .. : $eq(a, b)`.
- **`a != b` is `!(a == b)`,** written the way JS writes it: `a !== b`, or
  `!f(a, b)`. Rust requires `ne` to agree with `eq`.
- **`T: PartialEq` takes a dictionary, `{ eq }`,** as ADR 0049 laid out:
  - a type that compares with `===` gets `{ eq: (a, b) => a === b }`;
  - one that compares with `$eq` gets `{ eq: $eq }`;
  - a hand-written impl gets its own accessor, `versionPartialEq()`.
- **`T: Eq` gets `T`'s `PartialEq` dictionary.** `Eq` has no methods, so
  `impl Eq for Version {}` is allowed and adds nothing to the JS.
- **A `PartialEq<Rhs>` whose `Rhs` isn't `Self` is named after it:**
  `metersPartialEqF64`. A trait argument that's just its default isn't in
  the name, so `impl PartialEq for Version` is `versionPartialEq`.
- **Still errors:** `PartialOrd` and `Ord` impls, and `==` on a std type
  rust-js doesn't model.

## Why

- **Nothing changed for existing code.** Every snapshot, including the
  playground's, compiles as before, except that `x == Some(true)` is
  `x === true`, which gives the same answer.
- **A hand-written `eq` decides wherever it is:** directly, inside a
  derived one, in a `Vec`, in an `Option`, or through a generic.
- **The cost falls only on types that have a custom part.** Everything else
  keeps its one `===` or one `$eq`.

## Alternatives

- **Make `$eq` look up hand-written impls at run time.** That needs every
  object to carry its type, which JS values don't.
- **Always compare part by part.** That would be correct everywhere, but it
  replaces a short `$eq(a, b)` with long expressions for the common case.

## Consequences

- Every generic function with a `T: PartialEq` or `T: Eq` bound takes a
  `TPartialEq` argument.
- `Std::Eq`, `Std::LooseEq` and `Std::StructEq` are gone: all `==` goes
  through one function, `eq_value`.
