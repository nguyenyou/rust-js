# 0030. `Option`: `Some(x)` is `x`, `None` is `undefined`

Status: Accepted. Changes [0024](0024-web-crate.md): results that may be
`null` are `Option`s now.

## Context

The web platform answers "nothing here" with `null`:
`document.getElementById("app")` when there's no `#app`, `textContent` of a
document. The web crate typed those results as if they were never null (ADR
0024), which isn't honest. Rust says "maybe nothing" with `Option`, and rust-js
couldn't compile it. How the others do it (checked in local clones):

- **ReScript**: `Some(x)` is `x` itself, and `None` is `undefined`. Only
  when that would be ambiguous (`Some(None)`) is the value boxed, as
  `{BS_PRIVATE_NESTED_SOME_NONE: 0}` (`Primitive_option.res`). JS's
  `null` is a separate type, `Nullable.t`, converted with `fromNullable`.
- **Scala.js**: Scala's `Option` stays a class, `Some` and `None`. At the JS
  boundary there's `js.UndefOr[A]`, which is `A | Unit`: a value or
  `undefined`.

## Decision

**`Some(x)` is `x`, and `None` is `undefined`**, as in ReScript, but with
`null` counting as `None` too, so the web platform's `null` needs no
conversion:

| Rust | JS |
|---|---|
| `Some(x)`, `None` | `x`, `undefined` |
| `match o { Some(0) => .., Some(n) => .., None => .. }` | `if (o === 0) .. else if (o != null) .. else ..` |
| `if let Some(el) = find() { .. }` | `const el = find(); if (el != null) { .. }` |
| `while let Some(h) = half(n) { .. }` | `while (true) { const h = half(n); if (h != null) { .. } else { break; } }` |
| `o.is_some()`, `o.is_none()` | `o != null`, `o == null` |
| `o.unwrap()`, `o.expect("why")` | `$unwrap(o)`, `$unwrap(o, "why")`, which throw Rust's message on `None` |
| `o.unwrap_or(d)` | `o ?? d` |
| `a == b` on options of numbers, strings, `bool` and fieldless enums | `a == b` |
| `a == b` on options of structs | `$eq(a, b)`, where `null` and `undefined` are equal |

- **`!= null`, not `!== undefined`**: JS's loose comparison with `null` is
  true for both `null` and `undefined`, so `None` from Rust and "nothing"
  from the DOM are the same. It's also how JS programmers write "is there
  one".
- **`Option<T>` needs a `T` that's never `undefined` or `null` itself**, or
  `Some(x)` and `None` would be the same value. `Option<()>`, `Option` of a
  unit struct, and `Option<Option<T>>` are errors for now. ReScript boxes
  them. rust-js can do the same once a program needs it.
- **`unwrap_or`'s argument runs even when it isn't needed**, as in Rust. `??`
  skips it, so an argument with effects is computed first, in order:
  `const option = half(n); const fallback = bump(); option ?? fallback`.
- **`if let`** is new, and works with any pattern (`if let (0, y) = p`).
  When its value isn't already in a variable, it goes into a `const` named
  like the pattern's variable, which that variable then just is.

**The web crate's results that may be `null` are `Option`s**, 78 of them:
`document::get_element_by_id(..) -> Option<&Element>`,
`node::text_content(..) -> Option<String>`. Programs say what happens when
there's nothing:

```rust
let app = document::get_element_by_id(document, "app").expect("the page has an #app");
```

```js
const app = $unwrap(document.getElementById("app"), "the page has an #app");
```

## Why

- **No wrapper objects**: an `Option<&Element>` is the element or nothing, as
  the DOM itself gives it. The JS reads like the checks people write.
- **One representation for Rust's `None` and JS's `null`**, so bindings pass
  results straight through, with no `fromNullable` at every call.
- **Honest types**: a result that can be missing says so, and rustc makes
  the program deal with it, which the old non-null types skipped.

## Alternatives

- **`None` is `null`**: the DOM's own value. But JS's optional things
  (missing properties, `Array.prototype.find`, default parameters) are
  `undefined`, and so is ReScript's `None`. With `!= null` it doesn't
  matter which one arrives.
- **`{ tag: "Some", value }` objects, like any enum with fields**: faithful to
  Rust in every case, but an allocation per `Some`, and nothing a JS caller
  would expect.
- **A separate `Nullable<T>` type for the web crate**, as ReScript has:
  another type and a conversion at every call, to tell apart two things
  rust-js doesn't need to tell apart.

## Consequences

- The examples say `.expect("the page has an #app")` where they used to
  assume the element, and their tests `.unwrap()` what they query.
- `{:?}` of an option prints the value for `Some` and `()` for `None`, since
  `$debug` can't tell `None` from `()` at run time. `assert_eq!` messages on
  options read that way too.
- Not yet: `map`, `and_then`, `ok_or`, `?` on `Option`, `take`, `as_ref`,
  `as_mut`, and nested options. Other enums with fields are tagged objects
  (ADR 0033); `Option` is the special case that needs no tag.
