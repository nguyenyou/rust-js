# 0060. `{:?}` by the type, and a derived `Debug` is a function

Status: Accepted. Extends [0054](0054-display.md).

## Context

`{:?}` was `$debug(value)`, a runtime helper that looks at the JS value
alone. So it couldn't show what Rust shows:

| Rust | `$debug` |
|---|---|
| `Point { x: 1.0, y: 2.0 }` | `{ x: 1, y: 2 }`: no name, and `1.0` is `1` |
| `(1, "a")` | `[1, "a"]`: a tuple is an array |
| `None`, `Some(3)` | `()`, `3` |
| `'c'`, `Dot` | `"c"`, `"Dot"` |

`assert_eq!`'s failure message showed its values the same way. The type
that would tell them apart is known when compiling, and nowhere else.

## Decision

**`{:?}` is written from the value's type, and a `Debug` impl is a
function, derived or not.**

- **Std's types are shown in place:**
  - numbers are `String(n)`, and an `f64` is `$debugF64(x)`;
  - strings are `JSON.stringify(s)`, and a `char` is `$debugChar(c)`, `'c'`;
  - an `Option` is `Some(..)` or `None`, a tuple `(a, b)` (and `(a,)`), a
    `Vec` `[..]`, a map `{k: v}`, a `Result` `Ok(..)` or `Err(..)`, and an
    `Ordering` `Less`, `Equal` or `Greater`.
- **A `#[derive(Debug)]` is lowered from the body rustc derives,** like a
  hand-written `fmt` (ADR 0054):

  ```js
  function pointDebug_fmt(point) {
    return "Point { x: " + $debugF64(point.x) + ", y: " + $debugF64(point.y) + " }";
  }
  ```

  - That body hands each field to `Formatter::debug_struct_field2_finish`
    and the like as a `&dyn Debug`. **A `&dyn Debug` is the string it
    shows,** since showing is all one is for, so those calls join names
    and strings.
  - A struct with more than five fields gets arrays of names and strings,
    joined by `$debugFields`.
- **A derived `Debug` is left out unless something shows the type,** or
  shows one that shows it. `#[derive(Debug)]` on every type costs nothing
  in the JS.
- **Generic code:** `T: Debug` takes a dictionary, `{ fmt }`, and it's
  `TDebug.fmt(x)`.
- **`assert_eq!` and `assert_ne!` show their values the same way,** as
  strings made where the assertion fails.
- **Still errors:** `{:#?}` (with line breaks), and `{:?}` of a std type
  rust-js doesn't model. A `Result`'s `unwrap()` still shows its error
  with `$debug`.

## Why

- **It's Rust's output, character for character.** Differential tests
  check structs, tuple structs, unit structs, enums of all three kinds,
  six-field structs, generics, `Option`s, tuples, `Vec`s, `Result`s,
  `char`s with quotes, and `Ordering`.
- **Derived impls read as a person would write them:** one function
  returning a string, with a `return` per variant.
- **No cost for what isn't shown.** Only the `Debug` impls something
  reaches are in the output.

## Alternatives

- **Type descriptors for `$debug`** (`$debug(value, shape)`). A runtime
  walk of a description of the type. It would handle recursive types as
  well, but every `{:?}` would carry a description, and the derived impls
  would be data rather than code.
- **Type names in the values** (`{ $type: "Point", .. }`). It would let
  `$debug` read the name, but it changes every object for the sake of
  debugging.

## Consequences

- A `Debug` impl's function can be called from JS: `pointDebug_fmt(p)`.
- The two-pass lowering (ADR 0049) also decides which derived `Debug`
  impls are used, from the first pass's references.
