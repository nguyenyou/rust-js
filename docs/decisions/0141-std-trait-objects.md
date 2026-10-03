# 0141. A `dyn Display` or `dyn Error` is a value and its dictionary

Status: Accepted. Extends [0049](0049-traits-and-generics.md), [0054](0054-display.md) and [0060](0060-debug.md).

## Context

A trait object of the crate's own trait is a pair, `{ value, impl }`, of a
value and its dictionary (ADR 0049). A `dyn Debug` is the string it shows
(ADR 0060), since showing is all one is for. Two of std's trait objects
are everyday Rust, and were errors:

```rust
fn describe(d: &dyn Display) -> String { format!("<{d}>") }
fn lookup(name: &str) -> Result<u32, Box<dyn Error>> {
    let n = find(name)?;                       // a `Missing`, as a `Box<dyn Error>`
    if n > 10 { return Err("too big".into()); }
    Ok(n)
}
```

A `dyn Display` could have been the string it shows, as a `dyn Debug` is.
It can't be: a value is shown when it's shown, not when it's made into a
`dyn`. A `Cell` it holds may change in between, and a `fmt` that prints
would print too early.

## Decision

**A `dyn Display` and a `dyn Error` are pairs, as a `dyn` of the crate's
own trait is,** of the value and the dictionary generic code is given for
its type (ADR 0054):

| Rust | JS |
|---|---|
| `&33u8 as &dyn Display` | `{ value: 33, impl: { fmt: String } }` |
| `Box::new(point) as Box<dyn Display>` | `{ value: point, impl: pointDisplay() }` |
| `{}` of one | `d.impl.fmt(d.value)` |
| `missing as Box<dyn Error>`, and `?` that makes one | `{ value: missing, impl: missingError() }` |
| `{}` and `{:?}` of a `dyn Error` | `e.impl.Display().fmt(e.value)`, `e.impl.Debug().fmt(e.value)` |
| `Box<dyn Error>::from("too big")`, `"too big".into()` | `{ value: "too big", impl: $stringError() }` |

- **`Error` is a trait rust-js makes dictionaries for,** as `Display` and
  `Debug` are. Its supertraits are accessors, `Display()` and `Debug()`, as
  a crate trait's are. A `dyn` of the crate's trait whose supertrait is
  `Display` or `Debug` shows through them the same way, and an upcast to
  one, `&dyn Baz` to `&dyn Debug`, is its string.
- **An `Error` dictionary has `source`:** the impl's own, or std's
  default, `() => undefined`. Of the other methods std provides,
  `description` and `cause`, called through a `dyn Error`, each is an
  error: the dictionary has none of them, and a call would throw.
- **std's error of a message** shows the message, and `{:?}` of it the
  message quoted, `"too big"`, as Rust's does.
- **A width or a sign for a `dyn`'s `{}` is an error,** as for a generic
  `T`'s (ADR 0058): only its dictionary knows whether it pads.

## Why

- **Rust's order:** the value is shown by its own `fmt`, when it's shown.
  The corpus case changes a `Cell` after making the `dyn`, and shows what
  Rust shows.
- **One representation for every `dyn` that's dispatched:** the crate's,
  a library's, and these, so upcasts, `Vec`s of them and parameters work
  the same way.

## Alternatives

- **The string it shows, as a `dyn Debug` is.** Wrong when the value
  changes, or its `fmt` does anything else, before it's shown.
- **`Error` dictionaries with every method std provides,** each a copy of
  std's default body. `type_id` and `provide` are std internals; nothing
  but `source` is called in practice.

## Consequences

- A `dyn ToString`, `dyn Any` and `dyn DoubleEndedIterator` are still
  errors.
- Of rustc's tests, 8 more pass: `dyn Display` in its forms, `Box<dyn
  Error>` from a string, and three of trait upcasting's.
